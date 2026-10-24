//! Content + image generation pipeline, campaign run control (start/pause/resume/cancel) and progress.

use crate::error::{AppError, AppResult};
use crate::queue::{self, Job, NewJob};
use crate::services::campaigns::{self, CampaignView};
use crate::services::qc::{self, FaqItem, QcInput, QcReport};
use crate::services::templates::{self, Template};
use crate::services::{ai, image_providers, logs};
use crate::state::AppState;
use crate::util::text::language_name;
use base64::Engine;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;

const JOB_MAX_RETRIES: i64 = 3;
const WEBP_QUALITY: f32 = 82.0;

// ------------------------------------------------------------------ context & prompts

struct Ctx {
    campaign: CampaignView,
    template: Template,
    plan_id: String,
    project_id: String,
    article_no: i64,
    keyword: String,
    intent: Option<String>,
    title: String,
    project_name: String,
    brand_name: String,
    site_url: String,
    logo_data_url: String,
    target_audience: String,
    phone: String,
    primaries: Vec<String>,
}

async fn load_ctx(state: &AppState, plan_id: &str) -> AppResult<Ctx> {
    let row: Option<(String, String, i64, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT p.campaign_id, p.project_id, p.article_no, k.keyword, COALESCE(p.intent, k.intent),
                (SELECT t.title FROM article_titles t WHERE t.plan_id=p.id AND t.state='APPROVED')
         FROM article_plans p JOIN keywords k ON k.id=p.keyword_id WHERE p.id=?",
    )
    .bind(plan_id)
    .fetch_optional(&state.db)
    .await?;
    let (campaign_id, project_id, article_no, keyword, intent, title) = row.ok_or_else(|| AppError::not_found("مقاله پیدا نشد."))?;
    let title = title.ok_or_else(|| AppError::validation("برای این مقاله عنوان تأییدشده وجود ندارد."))?;
    let campaign = campaigns::get(state, &campaign_id).await?;
    let tid = campaign.template_id.clone().ok_or_else(|| AppError::validation("برای کمپین قالب مقاله انتخاب نشده است."))?;
    let template = templates::get(state, &tid).await?;
    let (project_name, brand_name, target_audience, phone, site_url, logo_data_url): (String, String, String, String, String, String) =
        sqlx::query_as("SELECT name, brand_name, target_audience, phone, site_url, logo_data_url FROM projects WHERE id=?")
            .bind(&project_id)
            .fetch_one(&state.db)
            .await?;
    let prim: Vec<(String,)> = sqlx::query_as("SELECT keyword FROM keywords WHERE campaign_id=? AND role='primary' ORDER BY rowid")
        .bind(&campaign_id)
        .fetch_all(&state.db)
        .await?;
    Ok(Ctx {
        campaign,
        template,
        plan_id: plan_id.to_string(),
        project_id,
        article_no,
        keyword,
        intent,
        title,
        project_name,
        brand_name,
        site_url,
        logo_data_url,
        target_audience,
        phone,
        primaries: prim.into_iter().map(|p| p.0).collect(),
    })
}

fn build_prompts(c: &Ctx) -> (String, String) {
    let t = &c.template;
    let brand = if c.brand_name.is_empty() { c.project_name.clone() } else { c.brand_name.clone() };
    let faq_note = if t.faq_enabled { format!("FAQ: provide {} question/answer pairs in the \"faq\" field.", t.faq_count) } else { "No FAQ.".to_string() };

    let mut vars: HashMap<&str, String> = HashMap::new();
    vars.insert("title", c.title.clone());
    vars.insert("primary_keyword", c.primaries.join("، "));
    vars.insert("secondary_keyword", c.keyword.clone());
    vars.insert("brand_name", brand.clone());
    vars.insert("brand_home_url", c.site_url.clone());
    vars.insert("target_audience", c.target_audience.clone());
    let tel: String = c.phone.chars().filter(|ch| ch.is_ascii_digit() || *ch == '+').collect();
    let phone_link = if tel.is_empty() { String::new() } else { format!("<a href=\"tel:{tel}\">{}</a>", c.phone) };
    vars.insert("phone", c.phone.clone());
    vars.insert("phone_link", phone_link.clone());
    vars.insert("internal_links", String::new());
    vars.insert("faq", faq_note.clone());

    let system = format!(
        "{sys}\n\nWrite in {lang}. You are writing for the website \"{brand}\".\n\
         OUTPUT CONTRACT: reply with ONLY one JSON object (no code fences, no commentary) with these keys:\n\
         - \"title\": the article title\n\
         - \"slug\": slug derived from the exact keyword (server enforces this)\n\
         - \"excerpt\": 1-2 sentence summary\n\
         - \"content\": the article body as clean semantic HTML using h2/h3, p, ul/ol/li, table, strong, em, a. \
           NO h1, NO html/head/body tags, NO scripts, NO inline styles, NO images.\n\
         - \"meta_title\": up to 60 characters\n\
         - \"meta_description\": up to 155 characters\n\
         - \"faq\": array of {{\"q\":\"...\",\"a\":\"...\"}} ({faq})\n\
         - \"image_prompt\": an English prompt for a featured image (no text inside the image)\n\
         Use the brand name as a hyperlink to the verified homepage {homepage}; do not invent URLs. Never invent statistics, prices or quotes.",
        homepage = c.site_url,
        sys = templates::render(&t.system_instructions, &vars),
        lang = language_name(&t.language),
        faq = if t.faq_enabled { format!("exactly {} items", t.faq_count) } else { "empty array".into() },
    );

    let mut u = String::new();
    u.push_str(&format!("Article title (use exactly this): {}\n", c.title));
    u.push_str(&format!("Focus (secondary) keyword: {}\n", c.keyword));
    if !c.primaries.is_empty() {
        u.push_str(&format!("Main topic keywords: {}\n", c.primaries.join(", ")));
    }
    if let Some(i) = &c.intent {
        u.push_str(&format!("Search intent: {i}\n"));
    }
    u.push_str(&format!("Length: between {} and {} words.\n", t.min_words, t.max_words));
    if !t.tone_of_voice.trim().is_empty() {
        u.push_str(&format!("Tone of voice: {}\n", templates::render(&t.tone_of_voice, &vars)));
    }
    if !c.target_audience.is_empty() {
        u.push_str(&format!("Target audience: {}\n", c.target_audience));
    }
    if !t.brand_info.trim().is_empty() {
        u.push_str(&format!("Brand information: {}\n", templates::render(&t.brand_info, &vars)));
    }
    if !t.article_structure.trim().is_empty() {
        u.push_str(&format!("\nArticle structure to follow:\n{}\n", templates::render(&t.article_structure, &vars)));
    }
    if !t.required_sections.trim().is_empty() {
        u.push_str(&format!("\nRequired sections:\n{}\n", templates::render(&t.required_sections, &vars)));
    }
    if !t.seo_rules.trim().is_empty() {
        u.push_str(&format!("\nSEO rules:\n{}\n", templates::render(&t.seo_rules, &vars)));
    }
    if t.internal_linking {
        u.push_str("\nINTERNAL LINK SAFETY: Never invent URLs, slugs, or article paths. Only use exact verified URLs explicitly provided in the input. No verified links have been supplied by this generation context, so DO NOT add internal links.\n");
        if !t.internal_link_rules.trim().is_empty() {
            u.push_str(&format!("Internal linking policy: {}\n", t.internal_link_rules));
        }
    } else {
        u.push_str("\nDo not invent or add internal links.\n");
    }
    if let Some(d) = t.keyword_density_target {
        u.push_str(&format!("Aim for a keyword density of about {d}% for the focus keyword (natural usage only).\n"));
    }
    let kw = c.keyword.as_str();
    u.push_str(&format!(
        "\nMANDATORY SEO RULES (every rule must be satisfied, the output is rejected otherwise):\n\
         - Use the exact focus phrase \"{kw}\" (same word order, complete phrase, never split) between {min} and {max} times in the body; aim for about {mid}.\n\
         - The exact focus phrase must appear at least once within the first 150 words of the body.\n\
         - Keep every paragraph at most 80 words. Use many short paragraphs instead of long blocks.\n\
         - Include at least 2 external links (<a href=\"https://...\">) to relevant, authoritative sources: official organizations, standards bodies, encyclopedias, major research or industry publications. Use only real, stable homepage or major-section URLs you are certain exist; never invent deep links. Never link to competitor websites, including Iranian competitors, and never use rel=\"nofollow\".\n\
         - Where it fits naturally, use numbers in some headings (for example \"۵ روش ...\" or \"۳ اشتباه رایج ...\").\n",
        min = qc::KEYWORD_MIN_USES, max = qc::KEYWORD_MAX_USES, mid = (qc::KEYWORD_MIN_USES + qc::KEYWORD_MAX_USES) / 2,
    ));
    if !t.forbidden_topics.trim().is_empty() {
        u.push_str(&format!("\nDo NOT mention or discuss:\n{}\n", t.forbidden_topics));
    }
    if !t.cta.trim().is_empty() {
        u.push_str(&format!("\nEnd with this call to action: {}\n", templates::render(&t.cta, &vars)));
        if !phone_link.is_empty() { u.push_str(&format!("All phone call CTA links MUST use href=\"tel:<digits>\", specifically: {phone_link}. Do not use website URLs or wa.me as the call link.\n")); }
    }
    if t.faq_enabled {
        u.push_str(&format!("\n{faq_note}\n"));
    }
    (system, u)
}

pub async fn preview_prompt(state: &AppState, plan_id: &str) -> AppResult<Value> {
    let c = load_ctx(state, plan_id).await?;
    let (system, user) = build_prompts(&c);
    Ok(json!({"system": system, "user": user}))
}

// ------------------------------------------------------------------ plan helpers & events

pub(crate) async fn set_plan_status(state: &AppState, plan_id: &str, status: &str) {
    let row: Option<(i64, String)> = sqlx::query_as(
        "UPDATE article_plans SET status=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=? RETURNING article_no, campaign_id",
    )
    .bind(status)
    .bind(plan_id)
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    if let Some((no, _)) = row {
        state.events.emit("plan:status", json!({"planId": plan_id, "articleNo": no, "status": status}));
    }
}

pub async fn mark_failed(state: &AppState, plan_id: &str, stage: &str, message: &str) {
    let _ = sqlx::query("UPDATE article_plans SET failed_stage=?, last_error=? WHERE id=?")
        .bind(stage)
        .bind(message)
        .bind(plan_id)
        .execute(&state.db)
        .await;
    set_plan_status(state, plan_id, "FAILED").await;
}

pub(crate) async fn guard_cancel(state: &AppState, job: &Job) -> Result<(), AppError> {
    let Some(cid) = &job.campaign_id else { return Ok(()) };
    let st: Option<(String,)> = sqlx::query_as("SELECT status FROM campaigns WHERE id=?").bind(cid).fetch_optional(&state.db).await?;
    if matches!(st, Some((ref s,)) if s == "CANCELLED") {
        if let Some(pid) = &job.plan_id {
            set_plan_status(state, pid, "CANCELLED").await;
        }
        return Err(AppError::from_parts("CANCELLED", "عملیات لغو شد.".into(), false));
    }
    Ok(())
}

// ------------------------------------------------------------------ content job

struct Assembled {
    title: String,
    slug: String,
    excerpt: String,
    content_html: String,
    meta_title: String,
    meta_description: String,
    faq: Vec<FaqItem>,
    image_prompt: String,
    report: QcReport,
}

fn parse_faq(v: &Value) -> Vec<FaqItem> {
    v.get("faq")
        .and_then(|f| f.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|it| {
                    let flat = |s: &str| qc::plain_text(s).split_whitespace().collect::<Vec<_>>().join(" ");
                    let q = flat(it.get("q").or_else(|| it.get("question"))?.as_str()?);
                    let ans = flat(it.get("a").or_else(|| it.get("answer"))?.as_str()?);
                    Some(FaqItem { q, a: ans })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn assemble(c: &Ctx, v: &Value, others: &[String]) -> Result<Assembled, Vec<String>> {
    let obj = match v {
        Value::Object(_) => v.clone(),
        Value::Array(a) if a.len() == 1 && a[0].is_object() => a[0].clone(),
        _ => return Err(vec!["خروجی باید یک شیء JSON باشد.".into()]),
    };
    let content = obj.get("content").and_then(|x| x.as_str()).unwrap_or("");
    if content.trim().is_empty() {
        return Err(vec!["فیلد content خالی یا موجود نیست.".into()]);
    }
    let content_html = qc::sanitize_html(content);
    let faq = parse_faq(&obj);
    let report = qc::validate(&QcInput {
        content_html: &content_html,
        keyword: &c.keyword,
        own_host: &qc::host_of(&c.site_url),
        min_words: c.template.min_words as usize,
        max_words: c.template.max_words as usize,
        faq_enabled: c.template.faq_enabled,
        faq: &faq,
        others,
    });
    if !report.errors.is_empty() {
        return Err(report.errors.clone());
    }
    let s = |k: &str| obj.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    let excerpt = {
        let e = s("excerpt");
        if e.is_empty() { qc::truncate_chars(qc::plain_text(&content_html).trim(), 200) } else { qc::truncate_chars(&e, 300) }
    };
    let slug = qc::slugify(&c.keyword);
    if slug.is_empty() { return Err(vec!["کلمه کلیدی برای ساخت اسلاگ معتبر نیست.".into()]); }
    let meta_title = { let m = s("meta_title"); qc::truncate_chars(if m.is_empty() { &c.title } else { &m }, 70) };
    let meta_description = { let m = s("meta_description"); qc::truncate_chars(if m.is_empty() { &excerpt } else { &m }, 170) };
    let image_prompt = {
        let p = s("image_prompt");
        if p.is_empty() { format!("A clean, modern featured image illustrating: {}", c.keyword) } else { p }
    };
    // Always provide a tel: anchor for the contact CTA. Do not invent a web link for phone calls.
    let mut content_html = content_html;
    if !c.phone.trim().is_empty() {
        let tel: String = c.phone.chars().filter(|ch| ch.is_ascii_digit() || *ch == '+').collect();
        if !tel.is_empty() && !content_html.contains("mam-cta-card") {
            let readable = c.phone.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;");
            content_html.push_str(&format!("\n<div class=\"mam-cta-card\" style=\"margin:32px 0;padding:28px;border-radius:20px;background:linear-gradient(135deg,#0f172a,#1e40af);color:#fff;text-align:center;box-shadow:0 12px 30px rgba(15,23,42,.15)\"><p style=\"color:#fff;font-size:20px;font-weight:700;margin:0 0 12px\">برای مشاوره و اطلاعات بیشتر تماس بگیرید</p><a href=\"tel:{tel}\" style=\"display:inline-block;background:#fff;color:#1e3a8a;padding:13px 28px;border-radius:999px;font-weight:700;text-decoration:none;direction:ltr\">☎ {readable}</a></div>"));
        }
    }
    let faq = if c.template.faq_enabled { faq } else { Vec::new() };
    Ok(Assembled { title: c.title.clone(), slug, excerpt, content_html, meta_title, meta_description, faq, image_prompt, report })
}

async fn other_texts(state: &AppState, campaign_id: &str, plan_id: &str) -> AppResult<Vec<String>> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT c.content_html, k.keyword FROM article_contents c JOIN article_plans p ON p.id=c.plan_id
         JOIN keywords k ON k.id=p.keyword_id WHERE p.campaign_id=? AND p.id<>?",
    )
    .bind(campaign_id)
    .bind(plan_id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows.into_iter().map(|(html, kw)| qc::strip_phrase(&qc::plain_text(&html), &kw)).collect())
}

pub async fn run_content(state: &AppState, job: &Job) -> Result<(), AppError> {
    let plan_id = job.plan_id.as_deref().ok_or_else(|| AppError::internal("job without plan"))?;
    guard_cancel(state, job).await?;
    let ctx = load_ctx(state, plan_id).await?;
    let provider = campaigns::require_ai_provider(&ctx.campaign)?;
    set_plan_status(state, plan_id, "GENERATING").await;
    emit_progress(state, &ctx.campaign.id).await;

    let (system, user) = build_prompts(&ctx);
    let others = other_texts(state, &ctx.campaign.id, plan_id).await?;
    let max_tokens = ((ctx.template.max_words as f64 * 3.2) as u32 + 1200).clamp(2000, 32000);
    let attempts = ctx.template.max_retries + 1;
    let mut feedback = String::new();
    let mut last: Vec<String> = Vec::new();

    for attempt in 0..attempts {
        let msg = if feedback.is_empty() {
            user.clone()
        } else {
            format!("{user}\n\nYour previous attempt was rejected. Fix ALL of these problems:\n- {feedback}")
        };
        let v = ai::complete_json(state, &provider, &system, &msg, Some(max_tokens)).await?;
        guard_cancel(state, job).await?;
        match assemble(&ctx, &v, &others) {
            Ok(a) => {
                save_content(state, &ctx, &a, &provider, &v).await?;
                return Ok(());
            }
            Err(errors) => {
                logs::record(state, "WARN", Some(&ctx.project_id), Some(&ctx.campaign.id), "content_qc", "WARN",
                    &format!("article {} attempt {}: {}", ctx.article_no, attempt + 1, errors.join(" | "))).await;
                feedback = errors.join("\n- ");
                last = errors;
            }
        }
    }
    Err(AppError::from_parts("QC_FAILED", format!("کنترل کیفیت ناموفق بود: {}", last.join("؛ ")), false))
}

async fn save_content(state: &AppState, ctx: &Ctx, a: &Assembled, provider_id: &str, raw: &Value) -> AppResult<()> {
    let model: (String,) = sqlx::query_as("SELECT model FROM ai_providers WHERE id=?").bind(provider_id).fetch_one(&state.db).await?;
    sqlx::query(
        "INSERT INTO article_contents (id, plan_id, title, slug, excerpt, content_html, meta_title, meta_description, faq_json,
           image_prompt, word_count, validation_json, raw_ai_json, provider_id, model)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(plan_id) DO UPDATE SET title=excluded.title, slug=excluded.slug, excerpt=excluded.excerpt,
           content_html=excluded.content_html, meta_title=excluded.meta_title, meta_description=excluded.meta_description,
           faq_json=excluded.faq_json, image_prompt=excluded.image_prompt, word_count=excluded.word_count,
           validation_json=excluded.validation_json, raw_ai_json=excluded.raw_ai_json, provider_id=excluded.provider_id,
           model=excluded.model, edited_by_user=0, version=article_contents.version+1,
           updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(&ctx.plan_id)
    .bind(&a.title)
    .bind(&a.slug)
    .bind(&a.excerpt)
    .bind(&a.content_html)
    .bind(&a.meta_title)
    .bind(&a.meta_description)
    .bind(serde_json::to_string(&a.faq).unwrap_or_else(|_| "[]".into()))
    .bind(&a.image_prompt)
    .bind(a.report.word_count as i64)
    .bind(serde_json::to_string(&a.report).unwrap_or_default())
    .bind(raw.to_string())
    .bind(provider_id)
    .bind(&model.0)
    .execute(&state.db)
    .await?;
    sqlx::query("UPDATE article_plans SET last_error=NULL, failed_stage=NULL WHERE id=?").bind(&ctx.plan_id).execute(&state.db).await?;

    let wants_image = ctx.template.image_count > 0 && ctx.campaign.image_provider_id.is_some();
    if wants_image {
        set_plan_status(state, &ctx.plan_id, "CONTENT_GENERATED").await;
        queue::enqueue(state, NewJob {
            queue: "image", jtype: "gen_image", project_id: ctx.project_id.clone(),
            campaign_id: ctx.campaign.id.clone(), plan_id: ctx.plan_id.clone(), max_retries: JOB_MAX_RETRIES,
        }).await?;
    } else {
        set_plan_status(state, &ctx.plan_id, "READY_TO_UPLOAD").await;
    }
    logs::record(state, "INFO", Some(&ctx.project_id), Some(&ctx.campaign.id), "content_generated", "SUCCESS",
        &format!("article {} words={}", ctx.article_no, a.report.word_count)).await;
    Ok(())
}


/// Decode provider image, composite project logo and encode the final WebP locally.
fn encode_watermarked_webp(bytes: &[u8], logo_data_url: &str) -> AppResult<Vec<u8>> {
    let mut picture = image::load_from_memory(bytes).map_err(|e| AppError::validation(format!("تصویر تولیدشده قابل خواندن نیست: {e}")))?.to_rgba8();
    if !logo_data_url.is_empty() {
        let (_, encoded) = logo_data_url.split_once(',').ok_or_else(|| AppError::validation("لوگوی پروژه نامعتبر است."))?;
        let logo_bytes = base64::engine::general_purpose::STANDARD.decode(encoded).map_err(|_| AppError::validation("لوگوی پروژه نامعتبر است."))?;
        let logo = image::load_from_memory(&logo_bytes).map_err(|_| AppError::validation("لوگوی پروژه قابل خواندن نیست."))?;
        let (w, h) = picture.dimensions();
        let max_w = (w / 5).max(1);
        let max_h = (h / 5).max(1);
        let scaled = logo.resize(max_w, max_h, image::imageops::FilterType::Lanczos3).to_rgba8();
        let (lw, lh) = scaled.dimensions();
        let margin = (w.min(h) / 40).max(4);
        let x = w.saturating_sub(lw + margin);
        let y = h.saturating_sub(lh + margin);
        let mut faded = scaled;
        for pixel in faded.pixels_mut() { pixel.0[3] = ((pixel.0[3] as u16 * 65) / 100) as u8; }
        image::imageops::overlay(&mut picture, &faded, x as i64, y as i64);
    }
    // Lossy WebP: much smaller than the lossless encoder of the `image` crate, visually near-identical.
    let (w, h) = picture.dimensions();
    let encoded = webp::Encoder::from_rgba(picture.as_raw(), w, h).encode(WEBP_QUALITY);
    Ok(encoded.to_vec())
}

// ------------------------------------------------------------------ image job

pub async fn run_image(state: &AppState, job: &Job) -> Result<(), AppError> {
    let plan_id = job.plan_id.as_deref().ok_or_else(|| AppError::internal("job without plan"))?;
    guard_cancel(state, job).await?;
    let ctx = load_ctx(state, plan_id).await?;
    let provider = ctx
        .campaign
        .image_provider_id
        .clone()
        .ok_or_else(|| AppError::validation("برای کمپین ارائه‌دهنده‌ی تصویر انتخاب نشده است."))?;
    let content: Option<(String,)> = sqlx::query_as("SELECT image_prompt FROM article_contents WHERE plan_id=?")
        .bind(plan_id)
        .fetch_optional(&state.db)
        .await?;
    let prompt = content.ok_or_else(|| AppError::validation("ابتدا باید متن مقاله تولید شود."))?.0;
    set_plan_status(state, plan_id, "IMAGE_GENERATING").await;
    emit_progress(state, &ctx.campaign.id).await;

    let full = {
        let style = ctx.template.image_style.trim();
        let base = prompt.trim().trim_end_matches('.');
        if style.is_empty() { format!("{base}. No text, no watermark, no logo.") } else { format!("{base}. {style}. No text, no watermark, no logo.") }
    };
    let (w, h) = ctx.template.image_size.split_once('x')
        .map(|(w, h)| (w.parse::<i64>().ok(), h.parse::<i64>().ok())).unwrap_or((None, None));
    // Regenerate as a complete set; each image gets a distinct prompt and filename.
    // Do not discard previous results until the replacement set is generated.
    let count = ctx.template.image_count.clamp(1, 10);
    let mut generated = Vec::new();
    for ix in 0..count {
        guard_cancel(state, job).await?;
        let prompt_i = format!("{full} Distinct composition variation {} of {}.", ix + 1, count);
        let img = image_providers::generate(state, &provider, &prompt_i, &ctx.template.image_size).await?;
        let rel = format!("projects/{}/campaigns/{}/images/article-{:03}-{}.webp", ctx.project_id, ctx.campaign.id, ctx.article_no, ix + 1);
        let abs = state.data_dir.join(&rel);
        if let Some(dir) = abs.parent() { std::fs::create_dir_all(dir)?; }
        let webp = encode_watermarked_webp(&img.bytes, &ctx.logo_data_url)?;
        std::fs::write(&abs, webp)?;
        generated.push((rel, "image/webp".to_string(), prompt_i, ix == 0, ix));
    }
    guard_cancel(state, job).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM article_images WHERE plan_id=?").bind(plan_id).execute(&mut *tx).await?;
    for (rel, mime, prompt_i, is_featured, ix) in generated {
        let image_alt = if ix == 0 { format!("{} - {}", ctx.keyword, ctx.title) } else { format!("{} - {} - تصویر {}", ctx.keyword, ctx.title, ix + 1) };
        sqlx::query("INSERT INTO article_images (id, plan_id, is_featured, prompt, local_path, mime_type, width, height, alt_text, provider_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(ulid::Ulid::new().to_string()).bind(plan_id).bind(is_featured as i64)
            .bind(&prompt_i).bind(&rel).bind(&mime).bind(w).bind(h).bind(&image_alt).bind(&provider)
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    sqlx::query("UPDATE article_plans SET last_error=NULL, failed_stage=NULL WHERE id=?").bind(plan_id).execute(&state.db).await?;
    set_plan_status(state, plan_id, "READY_TO_UPLOAD").await;
    logs::record(state, "INFO", Some(&ctx.project_id), Some(&ctx.campaign.id), "image_generated", "SUCCESS", &format!("article {}", ctx.article_no)).await;
    Ok(())
}

// ------------------------------------------------------------------ run control

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartResult {
    pub queued: i64,
}

async fn enqueue_for(state: &AppState, plan_id: &str, project_id: &str, campaign_id: &str, jtype: &'static str) -> AppResult<bool> {
    let (queue_name, ty) = if jtype == "image" { ("image", "gen_image") } else { ("content", "gen_content") };
    Ok(queue::enqueue(state, NewJob {
        queue: queue_name, jtype: ty, project_id: project_id.to_string(), campaign_id: campaign_id.to_string(),
        plan_id: plan_id.to_string(), max_retries: JOB_MAX_RETRIES,
    }).await?.is_some())
}

pub async fn start(state: &AppState, campaign_id: &str, confirm: bool) -> AppResult<StartResult> {
    if !confirm {
        return Err(AppError::validation("شروع تولید نیاز به تأیید صریح دارد."));
    }
    let c = campaigns::get(state, campaign_id).await?;
    if c.plan_count == 0 {
        return Err(AppError::validation("برنامه‌ی مقالات هنوز ساخته نشده است."));
    }
    let (missing,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM article_plans p WHERE p.campaign_id=?
         AND NOT EXISTS (SELECT 1 FROM article_titles t WHERE t.plan_id=p.id AND t.state='APPROVED')",
    )
    .bind(campaign_id)
    .fetch_one(&state.db)
    .await?;
    if missing > 0 {
        return Err(AppError::validation(format!("هنوز {missing} عنوان تأیید نشده است؛ همه‌ی عنوان‌ها باید تأیید شوند.")));
    }
    // The article template and the default providers are fixed; campaigns always use them.
    if c.template_id.as_deref() != Some(templates::ARTICLE_TEMPLATE_ID) {
        sqlx::query("UPDATE campaigns SET template_id=? WHERE id=?").bind(templates::ARTICLE_TEMPLATE_ID).bind(campaign_id).execute(&state.db).await?;
    }
    let template = templates::get(state, templates::ARTICLE_TEMPLATE_ID).await?;
    campaigns::require_ai_provider(&c)?;
    if template.image_count > 0 && c.image_provider_id.is_none() {
        let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM image_providers WHERE id=?")
            .bind(crate::services::license::IMAGE_PROVIDER_ID)
            .fetch_optional(&state.db)
            .await?;
        if exists.is_none() {
            return Err(AppError::validation("لایسنس را وارد کنید."));
        }
        sqlx::query("UPDATE campaigns SET image_provider_id=? WHERE id=?").bind(crate::services::license::IMAGE_PROVIDER_ID).bind(campaign_id).execute(&state.db).await?;
    }

    let plans: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, status FROM article_plans WHERE campaign_id=? AND status IN ('TITLE_APPROVED','CANCELLED','CONTENT_GENERATED') ORDER BY article_no",
    )
    .bind(campaign_id)
    .fetch_all(&state.db)
    .await?;
    let mut queued = 0;
    for (pid, _) in plans {
        let (has_content,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM article_contents WHERE plan_id=?").bind(&pid).fetch_one(&state.db).await?;
        let (has_image,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM article_images WHERE plan_id=? AND local_path IS NOT NULL").bind(&pid).fetch_one(&state.db).await?;
        if has_content == 0 {
            set_plan_status(state, &pid, "QUEUED").await;
            if enqueue_for(state, &pid, &c.project_id, campaign_id, "content").await? { queued += 1; }
        } else if template.image_count > 0 && has_image == 0 {
            set_plan_status(state, &pid, "CONTENT_GENERATED").await;
            if enqueue_for(state, &pid, &c.project_id, campaign_id, "image").await? { queued += 1; }
        } else {
            set_plan_status(state, &pid, "READY_TO_UPLOAD").await;
        }
    }
    sqlx::query("UPDATE campaigns SET status='IN_PROGRESS', notified=0, generation_started_at=COALESCE(generation_started_at, strftime('%Y-%m-%dT%H:%M:%fZ','now')) WHERE id=?")
        .bind(campaign_id)
        .execute(&state.db)
        .await?;
    campaigns::set_step(state, campaign_id, 9).await?;
    logs::record(state, "INFO", Some(&c.project_id), Some(campaign_id), "generation_started", "SUCCESS", &format!("queued={queued}")).await;
    state.wake.notify_one();
    emit_progress(state, campaign_id).await;
    Ok(StartResult { queued })
}

pub async fn pause(state: &AppState, campaign_id: &str) -> AppResult<()> {
    let r = sqlx::query("UPDATE campaigns SET status='PAUSED' WHERE id=? AND status='IN_PROGRESS'").bind(campaign_id).execute(&state.db).await?;
    if r.rows_affected() == 0 {
        return Err(AppError::conflict("کمپین در حال اجرا نیست."));
    }
    emit_status(state, campaign_id, "PAUSED");
    emit_progress(state, campaign_id).await;
    Ok(())
}

pub async fn resume(state: &AppState, campaign_id: &str) -> AppResult<()> {
    let r = sqlx::query("UPDATE campaigns SET status='IN_PROGRESS', notified=0 WHERE id=? AND status='PAUSED'").bind(campaign_id).execute(&state.db).await?;
    if r.rows_affected() == 0 {
        return Err(AppError::conflict("کمپین متوقف نشده است."));
    }
    state.wake.notify_one();
    emit_status(state, campaign_id, "IN_PROGRESS");
    emit_progress(state, campaign_id).await;
    Ok(())
}

pub async fn cancel(state: &AppState, campaign_id: &str) -> AppResult<()> {
    campaigns::get(state, campaign_id).await?;
    sqlx::query("UPDATE campaigns SET status='CANCELLED' WHERE id=? AND status IN ('IN_PROGRESS','PAUSED')").bind(campaign_id).execute(&state.db).await?;
    sqlx::query("UPDATE jobs SET status='CANCELLED', completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE campaign_id=? AND status IN ('PENDING','RETRY_PENDING')")
        .bind(campaign_id)
        .execute(&state.db)
        .await?;
    let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM article_plans WHERE campaign_id=? AND status='QUEUED'").bind(campaign_id).fetch_all(&state.db).await?;
    for (id,) in ids {
        set_plan_status(state, &id, "CANCELLED").await;
    }
    emit_status(state, campaign_id, "CANCELLED");
    emit_progress(state, campaign_id).await;
    Ok(())
}

fn emit_status(state: &AppState, campaign_id: &str, status: &str) {
    state.events.emit("campaign:status", json!({"campaignId": campaign_id, "status": status}));
}

pub async fn retry_article(state: &AppState, plan_id: &str, stage: &str) -> AppResult<()> {
    let row: Option<(String, String)> = sqlx::query_as("SELECT campaign_id, project_id FROM article_plans WHERE id=?").bind(plan_id).fetch_optional(&state.db).await?;
    let (campaign_id, project_id) = row.ok_or_else(|| AppError::not_found("مقاله پیدا نشد."))?;
    let kind = match stage {
        "content" => "content",
        "image" => "image",
        "upload" => return crate::services::upload::enqueue_upload(state, plan_id).await,
        _ => return Err(AppError::validation("مرحله‌ی نامعتبر است.")),
    };
    if kind == "image" {
        let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM article_contents WHERE plan_id=?").bind(plan_id).fetch_one(&state.db).await?;
        if n == 0 {
            return Err(AppError::validation("ابتدا باید متن مقاله تولید شود."));
        }
        let c = campaigns::get(state, &campaign_id).await?;
        if c.image_provider_id.is_none() {
            return Err(AppError::validation("برای کمپین ارائه‌دهنده‌ی تصویر انتخاب نشده است."));
        }
    }
    if !enqueue_for(state, plan_id, &project_id, &campaign_id, kind).await? {
        return Err(AppError::conflict("این مرحله از قبل در صف یا در حال اجراست."));
    }
    set_plan_status(state, plan_id, if kind == "image" { "CONTENT_GENERATED" } else { "QUEUED" }).await;
    sqlx::query("UPDATE article_plans SET last_error=NULL, failed_stage=NULL WHERE id=?").bind(plan_id).execute(&state.db).await?;
    sqlx::query("UPDATE campaigns SET status='IN_PROGRESS', notified=0 WHERE id=? AND status IN ('CANCELLED','DRAFT','COMPLETED')").bind(&campaign_id).execute(&state.db).await?;
    sqlx::query("UPDATE campaigns SET notified=0 WHERE id=?").bind(&campaign_id).execute(&state.db).await?;
    emit_progress(state, &campaign_id).await;
    Ok(())
}

pub async fn retry_failed(state: &AppState, campaign_id: &str) -> AppResult<i64> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as("SELECT id, failed_stage FROM article_plans WHERE campaign_id=? AND status='FAILED'")
        .bind(campaign_id)
        .fetch_all(&state.db)
        .await?;
    let mut n = 0;
    for (id, stage) in rows {
        let st = match stage.as_deref() { Some("image") => "image", Some("upload") => "upload", _ => "content" };
        if retry_article(state, &id, st).await.is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

// ------------------------------------------------------------------ progress & listings

#[derive(Debug, Serialize, Clone)]
pub struct Stage {
    pub done: i64,
    pub total: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub campaign_id: String,
    pub campaign_status: String,
    pub titles: Stage,
    pub content: Stage,
    /// Articles whose text and images are all ready (READY_TO_UPLOAD or later).
    pub complete: i64,
    pub images: Stage,
    pub wordpress: Stage,
    pub images_enabled: bool,
    pub failed: i64,
    pub running: i64,
    pub queued: i64,
    pub percent: i64,
}

async fn scalar(state: &AppState, sql: &str, id: &str) -> i64 {
    sqlx::query_as::<_, (i64,)>(sql).bind(id).fetch_one(&state.db).await.map(|r| r.0).unwrap_or(0)
}

pub async fn progress(state: &AppState, campaign_id: &str) -> AppResult<Progress> {
    let c = campaigns::get(state, campaign_id).await?;
    let total = c.plan_count;
    let images_enabled = match &c.template_id {
        Some(t) => templates::get(state, t).await.map(|t| t.image_count > 0).unwrap_or(false),
        None => false,
    };
    let content = scalar(state, "SELECT COUNT(*) FROM article_contents x JOIN article_plans p ON p.id=x.plan_id WHERE p.campaign_id=?", campaign_id).await;
    // An article counts as built only when its text AND all of its images are ready.
    let complete = scalar(state, "SELECT COUNT(*) FROM article_plans WHERE campaign_id=? AND status IN ('READY_TO_UPLOAD','UPLOADING','DRAFT_CREATED')", campaign_id).await;
    let images = scalar(state, "SELECT COUNT(DISTINCT i.plan_id) FROM article_images i JOIN article_plans p ON p.id=i.plan_id WHERE p.campaign_id=? AND i.local_path IS NOT NULL", campaign_id).await;
    let uploaded = scalar(state, "SELECT COUNT(*) FROM wordpress_posts w JOIN article_plans p ON p.id=w.plan_id WHERE p.campaign_id=? AND w.upload_status='UPLOADED'", campaign_id).await;
    let failed = scalar(state, "SELECT COUNT(*) FROM article_plans WHERE campaign_id=? AND status='FAILED'", campaign_id).await;
    let running = scalar(state, "SELECT COUNT(*) FROM jobs WHERE campaign_id=? AND status='PROCESSING'", campaign_id).await;
    let queued = scalar(state, "SELECT COUNT(*) FROM jobs WHERE campaign_id=? AND status IN ('PENDING','RETRY_PENDING')", campaign_id).await;
    let percent = if total == 0 { 0 } else { (complete * 100) / total };
    Ok(Progress {
        campaign_id: campaign_id.to_string(),
        campaign_status: c.status,
        titles: Stage { done: c.approved_count, total },
        content: Stage { done: content, total },
        complete,
        images: Stage { done: images, total: if images_enabled { total } else { 0 } },
        wordpress: Stage { done: uploaded, total },
        images_enabled,
        failed,
        running,
        queued,
        percent,
    })
}

pub async fn emit_progress(state: &AppState, campaign_id: &str) {
    if let Ok(p) = progress(state, campaign_id).await {
        state.events.emit("campaign:progress", serde_json::to_value(&p).unwrap_or(Value::Null));
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleRow {
    pub plan_id: String,
    pub article_no: i64,
    pub keyword: String,
    pub title: Option<String>,
    pub status: String,
    pub failed_stage: Option<String>,
    pub last_error: Option<String>,
    pub word_count: Option<i64>,
    pub has_image: i64,
    pub category_ids: Vec<i64>,
    pub tags: Vec<String>,
    pub upload_status: Option<String>,
    pub wordpress_post_id: Option<i64>,
    pub wordpress_url: Option<String>,
    pub upload_at: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ArticleRowRaw {
    plan_id: String,
    article_no: i64,
    keyword: String,
    title: Option<String>,
    status: String,
    failed_stage: Option<String>,
    last_error: Option<String>,
    word_count: Option<i64>,
    has_image: i64,
    cats: String,
    tags: String,
    upload_status: Option<String>,
    wordpress_post_id: Option<i64>,
    wordpress_url: Option<String>,
    upload_at: Option<String>,
}

pub async fn list_articles(state: &AppState, campaign_id: &str) -> AppResult<Vec<ArticleRow>> {
    let rows = sqlx::query_as::<_, ArticleRowRaw>(
        "SELECT p.id AS plan_id, p.article_no, k.keyword,
                (SELECT t.title FROM article_titles t WHERE t.plan_id=p.id AND t.state='APPROVED') AS title,
                p.status, p.failed_stage, p.last_error, c.word_count AS word_count,
                EXISTS(SELECT 1 FROM article_images i WHERE i.plan_id=p.id AND i.local_path IS NOT NULL) AS has_image,
                p.wp_category_ids_json AS cats, p.tags_json AS tags,
                w.upload_status AS upload_status, w.wordpress_post_id AS wordpress_post_id, w.wordpress_url AS wordpress_url, p.upload_at
         FROM article_plans p JOIN keywords k ON k.id=p.keyword_id
         LEFT JOIN article_contents c ON c.plan_id=p.id LEFT JOIN wordpress_posts w ON w.plan_id=p.id
         WHERE p.campaign_id=? ORDER BY p.article_no",
    )
    .bind(campaign_id)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| ArticleRow {
            plan_id: r.plan_id, article_no: r.article_no, keyword: r.keyword, title: r.title, status: r.status,
            failed_stage: r.failed_stage, last_error: r.last_error, word_count: r.word_count, has_image: r.has_image,
            category_ids: serde_json::from_str(&r.cats).unwrap_or_default(), tags: serde_json::from_str(&r.tags).unwrap_or_default(),
            upload_status: r.upload_status, wordpress_post_id: r.wordpress_post_id, wordpress_url: r.wordpress_url, upload_at: r.upload_at,
        })
        .collect())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticleDetail {
    pub plan_id: String,
    pub article_no: i64,
    pub keyword: String,
    pub status: String,
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    pub content_html: String,
    pub meta_title: String,
    pub meta_description: String,
    pub faq: Vec<FaqItem>,
    pub image_prompt: String,
    pub word_count: i64,
    pub validation: Value,
    pub version: i64,
}

pub async fn get_article(state: &AppState, plan_id: &str) -> AppResult<ArticleDetail> {
    #[derive(sqlx::FromRow)]
    struct ArtRow {
        article_no: i64, keyword: String, status: String, title: String, slug: String, excerpt: String,
        content_html: String, meta_title: String, meta_description: String, faq_json: String, image_prompt: String,
        word_count: i64, validation_json: Option<String>, version: i64,
    }
    let r = sqlx::query_as::<_, ArtRow>(
        "SELECT p.article_no, k.keyword, p.status, c.title, c.slug, c.excerpt, c.content_html, c.meta_title, c.meta_description,
                c.faq_json, c.image_prompt, c.word_count, c.validation_json, c.version
         FROM article_plans p JOIN keywords k ON k.id=p.keyword_id JOIN article_contents c ON c.plan_id=p.id WHERE p.id=?",
    )
    .bind(plan_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("محتوایی برای این مقاله هنوز تولید نشده است."))?;
    Ok(ArticleDetail {
        plan_id: plan_id.to_string(), article_no: r.article_no, keyword: r.keyword, status: r.status, title: r.title, slug: r.slug,
        excerpt: r.excerpt, content_html: r.content_html, meta_title: r.meta_title, meta_description: r.meta_description,
        faq: serde_json::from_str(&r.faq_json).unwrap_or_default(), image_prompt: r.image_prompt, word_count: r.word_count,
        validation: r.validation_json.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or(Value::Null), version: r.version,
    })
}


/// Local images for article preview before any WordPress upload.
/// Data URLs are returned only to the desktop UI; they are never inserted into persisted article HTML.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticlePreviewImages {
    pub images: Vec<PreviewImage>,
    pub paragraph_interval: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewImage {
    pub src: String,
    pub alt: String,
    pub is_featured: bool,
}

pub async fn get_article_preview_images(state: &AppState, plan_id: &str) -> AppResult<ArticlePreviewImages> {
    let rows: Vec<(String, Option<String>, Option<String>, i64)> = sqlx::query_as(
        "SELECT local_path, mime_type, alt_text, is_featured FROM article_images WHERE plan_id=? AND local_path IS NOT NULL ORDER BY is_featured DESC, rowid ASC"
    )
    .bind(plan_id)
    .fetch_all(&state.db)
    .await?;

    let mut images = Vec::with_capacity(rows.len());
    for (rel, mime, alt, is_featured) in rows {
        // Missing files should not hide the other successfully generated images.
        if let Ok(bytes) = std::fs::read(state.data_dir.join(&rel)) {
            let content_type = mime.unwrap_or_else(|| "image/png".into());
            // The mime value is only from the application image pipeline.
            if !matches!(content_type.as_str(), "image/png" | "image/jpeg" | "image/webp" | "image/gif") { continue; }
            images.push(PreviewImage {
                src: format!("data:{};base64,{}", content_type, base64::engine::general_purpose::STANDARD.encode(bytes)),
                alt: alt.unwrap_or_default(),
                is_featured: is_featured != 0,
            });
        }
    }

    let campaign_id: Option<(String,)> = sqlx::query_as("SELECT campaign_id FROM article_plans WHERE id=?")
        .bind(plan_id).fetch_optional(&state.db).await?;
    let interval = if let Some((campaign_id,)) = campaign_id {
        match campaigns::get(state, &campaign_id).await {
            Ok(c) => match c.template_id {
                Some(t) => templates::get(state, &t).await.map(|t| t.image_paragraph_interval).unwrap_or(3),
                None => 3,
            },
            Err(_) => 3,
        }
    } else { 3 };
    Ok(ArticlePreviewImages { images, paragraph_interval: interval.max(1) })
}

/// Featured image as a data URL (so the UI needs no file-system access).
pub async fn get_article_image(state: &AppState, plan_id: &str) -> AppResult<Option<String>> {
    let row: Option<(String, Option<String>)> = sqlx::query_as("SELECT local_path, mime_type FROM article_images WHERE plan_id=? AND local_path IS NOT NULL ORDER BY is_featured DESC, rowid ASC LIMIT 1")
        .bind(plan_id)
        .fetch_optional(&state.db)
        .await?;
    let Some((rel, mime)) = row else { return Ok(None) };
    let abs = state.data_dir.join(&rel);
    match std::fs::read(&abs) {
        Ok(bytes) => Ok(Some(format!("data:{};base64,{}", mime.unwrap_or_else(|| "image/png".into()), base64::engine::general_purpose::STANDARD.encode(bytes)))),
        Err(_) => Ok(None),
    }
}

pub async fn queue_settings(state: &AppState) -> AppResult<Value> {
    let get = |k: &'static str, d: i64| async move {
        sqlx::query_as::<_, (String,)>("SELECT value_json FROM settings WHERE key=?")
            .bind(k)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten()
            .and_then(|r| r.0.trim().parse::<i64>().ok())
            .unwrap_or(d)
    };
    Ok(json!({
        "content": get("queue.concurrency.content", 3).await,
        "image": get("queue.concurrency.image", 2).await,
        "wordpress": get("queue.concurrency.wordpress", 3).await,
    }))
}

pub async fn update_queue_settings(state: &AppState, content: i64, image: i64, wordpress: i64) -> AppResult<Value> {
    if ![content, image, wordpress].iter().all(|v| (1..=10).contains(v)) {
        return Err(AppError::validation("همزمانی باید بین ۱ تا ۱۰ باشد."));
    }
    for (k, v) in [("queue.concurrency.content", content), ("queue.concurrency.image", image), ("queue.concurrency.wordpress", wordpress)] {
        sqlx::query("INSERT INTO settings (key, value_json) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')")
            .bind(k)
            .bind(v.to_string())
            .execute(&state.db)
            .await?;
    }
    state.wake.notify_one();
    queue_settings(state).await
}
