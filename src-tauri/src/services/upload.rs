//! Creating WordPress drafts: media upload, taxonomy resolution, post creation, duplicate protection.

use crate::error::{AppError, AppResult};
use crate::providers::wordpress::{MediaUpload, NewPost, WordPressClient};
use crate::queue::{self, Job, NewJob};
use crate::services::qc::FaqItem;
use crate::services::{campaigns, generation, logs, templates, wordpress as wp};
use crate::state::AppState;
use serde_json::json;
use crate::util::text::similarity;

const JOB_MAX_RETRIES: i64 = 3;

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// Final post body: sanitized article HTML + FAQ section (+ optional FAQPage JSON-LD).
pub fn assemble_post_html(content: &str, faq: &[FaqItem], schema: bool, language: &str) -> String {
    let mut out = content.to_string();
    let usable: Vec<&FaqItem> = faq.iter().filter(|f| !f.q.trim().is_empty() && !f.a.trim().is_empty()).collect();
    if usable.is_empty() {
        return out;
    }
    let heading = if language.starts_with("fa") { "پرسش‌های متداول" } else { "Frequently asked questions" };
    out.push_str(&format!("\n<h2>{}</h2>\n", esc(heading)));
    for f in &usable {
        out.push_str(&format!("<h3>{}</h3>\n<p>{}</p>\n", esc(&f.q), esc(&f.a)));
    }
    if schema {
        let ld = json!({
            "@context": "https://schema.org", "@type": "FAQPage",
            "mainEntity": usable.iter().map(|f| json!({"@type":"Question","name":f.q,"acceptedAnswer":{"@type":"Answer","text":f.a}})).collect::<Vec<_>>()
        });
        out.push_str(&format!("<script type=\"application/ld+json\">{}</script>\n", ld.to_string().replace('<', "\\u003c")));
    }
    out
}

/// Insert images between paragraphs without modifying the generated HTML paragraphs themselves.
/// The URLs come from WordPress media responses; escaping prevents attribute injection.
fn insert_inline_images(content: &str, images: &[(String, String)], interval: usize) -> String {
    if images.is_empty() { return content.to_string(); }
    let mut out = String::with_capacity(content.len() + images.len() * 200);
    let mut rest = content;
    let mut paragraph = 0usize;
    let mut used = 0usize;
    while let Some(pos) = rest.find("</p>") {
        let end = pos + 4;
        out.push_str(&rest[..end]);
        paragraph += 1;
        if used < images.len() && paragraph % interval.max(1) == 0 {
            let (url, alt) = &images[used];
            out.push_str(&format!("\n<figure><img src=\"{}\" alt=\"{}\" loading=\"lazy\" /></figure>\n", esc(url), esc(alt)));
            used += 1;
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    while used < images.len() {
        let (url, alt) = &images[used];
        out.push_str(&format!("\n<figure><img src=\"{}\" alt=\"{}\" loading=\"lazy\" /></figure>\n", esc(url), esc(alt)));
        used += 1;
    }
    out
}

/// Choose relevant previously published articles on the same WordPress site.
/// Never link unrelated old posts just to reach a quota.
fn related_links(keyword: &str, title: &str, published: &[(String, String)]) -> String {
    // Always link up to 3 published articles: the most similar ones first, whatever their score.
    let mut scored: Vec<(f64, &str, &str)> = published.iter().filter(|(name, _)| name != title).map(|(name, url)| {
        let score = similarity(keyword, name).max(similarity(title, name));
        (score, name.as_str(), url.as_str())
    }).collect();
    scored.sort_by(|a,b| b.0.total_cmp(&a.0));
    scored.truncate(3);
    if scored.is_empty() { return String::new(); }
    let mut result = String::from("\n<h2>مطالب مرتبط</h2>\n<ul>");
    for (_, label, url) in scored {
        result.push_str(&format!("<li><a href=\"{}\">{}</a></li>", esc(url), esc(label)));
    }
    result.push_str("</ul>\n");
    result
}

fn dup_blocked() -> AppError {
    AppError::from_parts("DUPLICATE_BLOCKED", "آپلود تکراری مسدود شد: این مقاله قبلاً در وردپرس ساخته شده است.".into(), false)
}

async fn is_uploaded(state: &AppState, plan_id: &str) -> AppResult<bool> {
    let r: Option<(String,)> = sqlx::query_as("SELECT upload_status FROM wordpress_posts WHERE plan_id=?").bind(plan_id).fetch_optional(&state.db).await?;
    Ok(matches!(r, Some((ref s,)) if s == "UPLOADED"))
}

/// Sets the WordPress publish date (UTC ISO 8601). The article is still created as a draft
/// unless it is published directly. Null clears the date. Uploads are never delayed by it.
/// Maps the chosen publish time to a WordPress status and GMT date.
/// No time: draft. Future time: scheduled by WordPress itself. Past time: published with that date.
async fn wp_target(state: &AppState, at: Option<&str>) -> AppResult<(String, Option<String>)> {
    let Some(value) = at.and_then(|s| s.get(..19)) else { return Ok(("draft".into(), None)) };
    let (future,): (i64,) = sqlx::query_as("SELECT ? > strftime('%Y-%m-%dT%H:%M:%S','now')").bind(value).fetch_one(&state.db).await?;
    Ok((if future == 1 { "future" } else { "publish" }.into(), Some(value.to_string())))
}

pub async fn set_schedule(state: &AppState, campaign_id: &str, plan_id: Option<&str>, upload_at: Option<&str>) -> AppResult<usize> {
    campaigns::get(state, campaign_id).await?;
    let at = upload_at.filter(|s| !s.trim().is_empty());
    if let Some(value) = at {
        if value.len() != 24 || !value.ends_with('Z') || !value.is_ascii() {
            return Err(AppError::validation("تاریخ باید در قالب UTC ISO8601 باشد."));
        }
        let parsed: Option<(Option<String>,)> = sqlx::query_as("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', ?)")
            .bind(value).fetch_optional(&state.db).await?;
        if parsed.and_then(|r| r.0).is_none() { return Err(AppError::validation("تاریخ نامعتبر است.")); }
    }
    let ids: Vec<(String,)> = if let Some(id) = plan_id {
        sqlx::query_as("SELECT id FROM article_plans WHERE id=? AND campaign_id=? AND status NOT IN ('UPLOADING')")
            .bind(id).bind(campaign_id).fetch_all(&state.db).await?
    } else {
        sqlx::query_as("SELECT id FROM article_plans WHERE campaign_id=? AND status NOT IN ('UPLOADING')")
            .bind(campaign_id).fetch_all(&state.db).await?
    };
    if ids.is_empty() { return Err(AppError::validation("مقاله قابل زمان‌بندی پیدا نشد.")); }
    let mut changed = 0;
    for (id,) in &ids {
        sqlx::query("UPDATE article_plans SET upload_at=? WHERE id=?").bind(at).bind(id).execute(&state.db).await?;
        let row: Option<(String, String, Option<i64>, Option<String>)> = sqlx::query_as(
            "SELECT p.status, p.project_id, w.wordpress_post_id, w.upload_status FROM article_plans p LEFT JOIN wordpress_posts w ON w.plan_id = p.id WHERE p.id=?",
        ).bind(id).fetch_optional(&state.db).await?;
        let Some((status, project_id, wp_id, upload_status)) = row else { continue };
        if upload_status.as_deref() == Some("UPLOADED") {
            // Already in WordPress: change its status and date there.
            if let Some(wp_id) = wp_id {
                let (wp_status, date) = wp_target(state, at).await?;
                let (client, _) = wp::build_client(state, &project_id).await?;
                client.set_post_status(wp_id, &wp_status, date.as_deref()).await.map_err(|e| wp::refine(state, e))?;
                sqlx::query("UPDATE wordpress_posts SET wp_status=? WHERE plan_id=?").bind(&wp_status).bind(id).execute(&state.db).await?;
                changed += 1;
            }
        } else if matches!(status.as_str(), "READY_TO_UPLOAD" | "FAILED") {
            // Not in WordPress yet: create it now with this publish time (it is scheduled there, not here).
            if request_upload(state, id, false).await.is_ok() { changed += 1; }
        } else {
            changed += 1;
        }
    }
    state.wake.notify_one();
    Ok(changed)
}

/// Queues an upload. `publish_now` selects direct publishing instead of a draft; it is stored on the plan
/// so retries keep the same mode.
pub async fn request_upload(state: &AppState, plan_id: &str, publish_now: bool) -> AppResult<()> {
    // Immediate publishing ignores any time that was set before.
    sqlx::query("UPDATE article_plans SET publish_now=?, upload_at=CASE WHEN ? = 1 THEN NULL ELSE upload_at END WHERE id=?")
        .bind(publish_now as i64).bind(publish_now as i64).bind(plan_id).execute(&state.db).await?;
    enqueue_upload(state, plan_id).await
}

pub async fn enqueue_upload(state: &AppState, plan_id: &str) -> AppResult<()> {
    let row: Option<(String, String, String)> = sqlx::query_as("SELECT campaign_id, project_id, status FROM article_plans WHERE id=?")
        .bind(plan_id)
        .fetch_optional(&state.db)
        .await?;
    let (campaign_id, project_id, status) = row.ok_or_else(|| AppError::not_found("مقاله پیدا نشد."))?;
    if is_uploaded(state, plan_id).await? || status == "DRAFT_CREATED" {
        return Err(dup_blocked());
    }
    if !matches!(status.as_str(), "READY_TO_UPLOAD" | "FAILED") {
        return Err(AppError::conflict("مقاله هنوز آماده‌ی آپلود نیست."));
    }
    let (has_content,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM article_contents WHERE plan_id=?").bind(plan_id).fetch_one(&state.db).await?;
    if has_content == 0 {
        return Err(AppError::validation("ابتدا باید متن مقاله تولید شود."));
    }
    let (has_conn,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM wordpress_connections WHERE project_id=?").bind(&project_id).fetch_one(&state.db).await?;
    if has_conn == 0 {
        return Err(AppError::validation("ابتدا اتصال وردپرس پروژه را تنظیم کنید."));
    }
    let queued = queue::enqueue(state, NewJob {
        queue: "wordpress", jtype: "upload_wp", project_id, campaign_id: campaign_id.clone(),
        plan_id: plan_id.to_string(), max_retries: JOB_MAX_RETRIES,
    }).await?;
    if queued.is_none() {
        return Err(AppError::conflict("آپلود این مقاله از قبل در صف یا در حال اجراست."));
    }
    sqlx::query("UPDATE campaigns SET status='IN_PROGRESS', notified=0 WHERE id=? AND status IN ('CANCELLED','DRAFT','COMPLETED','IN_PROGRESS')")
        .bind(&campaign_id)
        .execute(&state.db)
        .await?;
    generation::emit_progress(state, &campaign_id).await;
    Ok(())
}

/// Queues every article that is ready. Returns how many were queued.
/// Publishes one uploaded article now (WordPress status "publish" with today's date).
pub async fn publish_uploaded_now(state: &AppState, plan_id: &str) -> AppResult<()> {
    let row: Option<(String, i64)> = sqlx::query_as(
        "SELECT p.project_id, w.wordpress_post_id FROM article_plans p JOIN wordpress_posts w ON w.plan_id = p.id
         WHERE p.id=? AND w.upload_status='UPLOADED' AND w.wordpress_post_id IS NOT NULL",
    ).bind(plan_id).fetch_optional(&state.db).await?;
    let (project_id, wp_id) = row.ok_or_else(|| AppError::conflict("این مقاله هنوز در وردپرس ساخته نشده است."))?;
    let (now,): (String,) = sqlx::query_as("SELECT strftime('%Y-%m-%dT%H:%M:%S','now')").fetch_one(&state.db).await?;
    let (client, _) = wp::build_client(state, &project_id).await?;
    client.set_post_status(wp_id, "publish", Some(&now)).await.map_err(|e| wp::refine(state, e))?;
    sqlx::query("UPDATE article_plans SET upload_at=NULL WHERE id=?").bind(plan_id).execute(&state.db).await?;
    sqlx::query("UPDATE wordpress_posts SET wp_status='publish' WHERE plan_id=?").bind(plan_id).execute(&state.db).await?;
    generation::emit_progress(state, &project_id).await;
    Ok(())
}

pub async fn enqueue_campaign(state: &AppState, campaign_id: &str, confirm: bool, publish_now: bool) -> AppResult<i64> {
    if !confirm {
        return Err(AppError::validation("ساخت Draft نیاز به تأیید صریح دارد."));
    }
    campaigns::get(state, campaign_id).await?;
    let ids: Vec<(String,)> = sqlx::query_as("SELECT id FROM article_plans WHERE campaign_id=? AND status='READY_TO_UPLOAD' ORDER BY article_no")
        .bind(campaign_id)
        .fetch_all(&state.db)
        .await?;
    if ids.is_empty() {
        return Err(AppError::validation("مقاله‌ی آماده‌ی آپلود وجود ندارد."));
    }
    let mut n = 0;
    for (id,) in ids {
        match request_upload(state, &id, publish_now).await {
            Ok(()) => n += 1,
            Err(e) if e.code == "CONFLICT" || e.code == "DUPLICATE_BLOCKED" => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
}

pub async fn run_upload(state: &AppState, job: &Job) -> Result<(), AppError> {
    let plan_id = job.plan_id.clone().ok_or_else(|| AppError::internal("job without plan"))?;
    let r = upload_inner(state, job, &plan_id).await;
    if let Err(e) = &r {
        if e.code != "DUPLICATE_BLOCKED" && e.code != "CANCELLED" {
            let _ = sqlx::query("UPDATE wordpress_posts SET upload_status='FAILED', last_error=? WHERE plan_id=? AND upload_status<>'UPLOADED'")
                .bind(&e.message)
                .bind(&plan_id)
                .execute(&state.db)
                .await;
        }
    }
    r
}

async fn upload_inner(state: &AppState, job: &Job, plan_id: &str) -> Result<(), AppError> {
    let (campaign_id, project_id, article_no, tags_json, cats_json): (String, String, i64, String, String) = sqlx::query_as(
        "SELECT campaign_id, project_id, article_no, tags_json, wp_category_ids_json FROM article_plans WHERE id=?",
    )
    .bind(plan_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::not_found("مقاله پیدا نشد."))?;

    let existing: Option<(String, String, String)> =
        sqlx::query_as("SELECT id, upload_status, idempotency_key FROM wordpress_posts WHERE plan_id=?").bind(plan_id).fetch_optional(&state.db).await?;
    if matches!(&existing, Some((_, s, _)) if s == "UPLOADED") {
        return Err(dup_blocked());
    }
    generation::guard_cancel(state, job).await?;

    let (title, slug, excerpt, content_html, faq_json, meta_title, meta_description): (String, String, String, String, String, String, String) = sqlx::query_as(
        "SELECT title, slug, excerpt, content_html, faq_json, meta_title, meta_description FROM article_contents WHERE plan_id=?",
    )
    .bind(plan_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::validation("متن مقاله پیدا نشد."))?;
    let faq: Vec<FaqItem> = serde_json::from_str(&faq_json).unwrap_or_default();
    let (publish_now, publish_at): (i64, Option<String>) = sqlx::query_as("SELECT publish_now, upload_at FROM article_plans WHERE id=?")
        .bind(plan_id).fetch_one(&state.db).await?;
    let (wp_status, date_gmt) = if publish_now != 0 { ("publish".to_string(), None) } else { wp_target(state, publish_at.as_deref()).await? };
    let campaign = campaigns::get(state, &campaign_id).await?;
    let (schema, language, link_enabled) = match &campaign.template_id {
        Some(t) => templates::get(state, t).await.map(|t| (t.faq_schema, t.language, t.internal_linking)).unwrap_or((false, "fa".into(), false)),
        None => (false, "fa".into(), false),
    };
    let (keyword,): (String,) = sqlx::query_as("SELECT k.keyword FROM article_plans p JOIN keywords k ON k.id=p.keyword_id WHERE p.id=?")
        .bind(plan_id).fetch_one(&state.db).await?;

    let (client, _) = wp::build_client(state, &project_id).await?;

    sqlx::query(
        "INSERT INTO wordpress_posts (id, plan_id, project_id, upload_status, idempotency_key) VALUES (?, ?, ?, 'UPLOADING', ?)
         ON CONFLICT(plan_id) DO UPDATE SET upload_status='UPLOADING', last_error=NULL",
    )
    .bind(ulid::Ulid::new().to_string())
    .bind(plan_id)
    .bind(&project_id)
    .bind(ulid::Ulid::new().to_string())
    .execute(&state.db)
    .await?;
    sqlx::query("UPDATE article_plans SET last_error=NULL WHERE id=?").bind(plan_id).execute(&state.db).await?;
    generation::set_plan_status(state, plan_id, "UPLOADING").await;
    generation::emit_progress(state, &campaign_id).await;

    // 1) Upload all images to Media Library, set the first as featured.
    // Preserve uploaded media ids across retries and inject the remaining images into the article HTML.
    let mut featured: Option<i64> = None;
    let mut inline_images: Vec<(String, String)> = Vec::new();
    let rows: Vec<(String, String, Option<String>, Option<i64>, Option<String>, String, String, i64)> = sqlx::query_as(
        "SELECT id, local_path, mime_type, wp_media_id, wp_media_url, alt_text, caption, is_featured FROM article_images WHERE plan_id=? AND local_path IS NOT NULL ORDER BY is_featured DESC, rowid ASC",
    ).bind(plan_id).fetch_all(&state.db).await?;
    for (image_id, rel, mime, media_id, media_url, alt, caption, is_featured) in rows {
        let (id, url) = match (media_id, media_url) {
            (Some(id), Some(url)) => (id, url),
            _ => {
                let bytes = std::fs::read(state.data_dir.join(&rel))
                    .map_err(|_| AppError::validation("فایل تصویر روی دیسک پیدا نشد؛ تصویر را دوباره تولید کنید."))?;
                let ext = rel.rsplit('.').next().unwrap_or("png");
                let m = client.upload_media(MediaUpload {
                    bytes, filename: format!("article-{:03}-{}-{}.{}", article_no, &campaign_id[campaign_id.len().saturating_sub(6)..].to_lowercase(), &image_id[..image_id.len().min(6)], ext),
                    mime: mime.unwrap_or_else(|| "image/png".into()),
                }).await.map_err(|e| wp::refine(state, e))?;
                sqlx::query("UPDATE article_images SET wp_media_id=?, wp_media_url=? WHERE id=?")
                    .bind(m.id).bind(&m.url).bind(&image_id).execute(&state.db).await?;
                (m.id, m.url)
            }
        };
        client.update_media_meta(id, &format!("{} - {}", keyword, title), &alt, &caption).await.map_err(|e| wp::refine(state, e))?;
        // A featured image may also appear inside the post body. Keeping it
        // in inline_images ensures a one-image article still has an inline image.
        if is_featured != 0 { featured = Some(id); }
        inline_images.push((url, alt));
    }
    let image_interval = match &campaign.template_id {
        Some(t) => templates::get(state, t).await.map(|t| t.image_paragraph_interval).unwrap_or(3),
        None => 3,
    };
    let mut final_content_html = insert_inline_images(&content_html, &inline_images, image_interval as usize);
    if link_enabled {
        let old_posts = client.published_posts().await.map_err(|e| wp::refine(state, e))?;
        final_content_html.push_str(&related_links(&keyword, &title, &old_posts));
    }

    // 2) Categories and tags.
    let mut categories: Vec<i64> = serde_json::from_str(&cats_json).unwrap_or_default();
    if categories.is_empty() {
        let d: Option<(Option<i64>,)> = sqlx::query_as("SELECT default_category_wp_id FROM projects WHERE id=?").bind(&project_id).fetch_optional(&state.db).await?;
        if let Some((Some(c),)) = d {
            categories.push(c);
        }
    }
    let tag_names: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
    let mut tags: Vec<i64> = Vec::new();
    for name in &tag_names {
        let cached: Option<(i64,)> = sqlx::query_as("SELECT wp_id FROM wp_terms WHERE project_id=? AND kind='tag' AND lower(name)=lower(?)")
            .bind(&project_id)
            .bind(name)
            .fetch_optional(&state.db)
            .await?;
        let id = match cached {
            Some((id,)) => id,
            None => {
                let id = client.ensure_tag(name).await.map_err(|e| wp::refine(state, e))?;
                sqlx::query("INSERT OR REPLACE INTO wp_terms (id, project_id, kind, wp_id, name) VALUES (?, ?, 'tag', ?, ?)")
                    .bind(ulid::Ulid::new().to_string())
                    .bind(&project_id)
                    .bind(id)
                    .bind(name)
                    .execute(&state.db)
                    .await?;
                id
            }
        };
        tags.push(id);
    }

    // 3) Adopt a post a previous attempt may already have created, otherwise create it.
    let mut adopted = None;
    if existing.is_some() {
        adopted = client.find_post_by_slug(&slug).await.map_err(|e| wp::refine(state, e))?;
    }
    let post = match adopted {
        Some(p) => {
            logs::record_plan(state, "INFO", &project_id, &campaign_id, plan_id, "wordpress_adopt", "INFO", &format!("article {article_no}: adopted existing post {}", p.id)).await;
            p
        }
        None => {
            generation::guard_cancel(state, job).await?;
            client
                .create_post(NewPost {
                    status: wp_status.clone(),
                    date_gmt,
                    title: title.clone(),
                    content: assemble_post_html(&final_content_html, &faq, schema, &language),
                    excerpt,
                    slug,
                    featured_media: featured,
                    categories,
                    tags,
                })
                .await
                .map_err(|e| wp::refine(state, e))?
        }
    };

    // This must succeed before the local article is marked as uploaded. If the bridge
    // is absent a retry adopts the draft by slug and completes the SEO fields.
    // 1) Core REST post meta (works when Rank Math exposes its fields). 2) Optional companion endpoint.
    // Neither failure fails a created draft; the result is only logged.
    let seo_stored = match client.set_rank_math_meta(post.id, &keyword, &meta_title, &meta_description).await {
        Ok(stored) => stored,
        Err(err) => {
            eprintln!("Rank Math meta write failed for post {}: {:?}", post.id, err);
            false
        }
    };
    if !seo_stored {
        if let Err(err) = client.update_rank_math(post.id, &keyword, &meta_title, &meta_description).await {
            eprintln!("Optional Rank Math companion sync failed for post {}: {:?}", post.id, err);
        }
    }

    sqlx::query(
        "UPDATE wordpress_posts SET upload_status='UPLOADED', wordpress_post_id=?, wordpress_url=?, last_error=NULL, wp_status=?,
         uploaded_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE plan_id=?",
    )
    .bind(post.id)
    .bind(&post.link)
    .bind(&wp_status)
    .bind(plan_id)
    .execute(&state.db)
    .await?;
    sqlx::query("UPDATE article_plans SET last_error=NULL, failed_stage=NULL WHERE id=?").bind(plan_id).execute(&state.db).await?;
    generation::set_plan_status(state, plan_id, "DRAFT_CREATED").await;
    logs::record_plan(state, "INFO", &project_id, &campaign_id, plan_id, "wordpress_draft", "SUCCESS", &format!("article {article_no}: post {}", post.id)).await;

    let (remaining,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM article_plans WHERE campaign_id=? AND status<>'DRAFT_CREATED'").bind(&campaign_id).fetch_one(&state.db).await?;
    if remaining == 0 {
        sqlx::query("UPDATE campaigns SET status='COMPLETED' WHERE id=?").bind(&campaign_id).execute(&state.db).await?;
        state.events.emit("campaign:status", json!({"campaignId": campaign_id, "status": "COMPLETED"}));
    }
    Ok(())
}

pub async fn set_taxonomy(state: &AppState, plan_id: &str, category_ids: Vec<i64>, tags: Vec<String>) -> AppResult<()> {
    let st: Option<(String,)> = sqlx::query_as("SELECT status FROM article_plans WHERE id=?").bind(plan_id).fetch_optional(&state.db).await?;
    let (status,) = st.ok_or_else(|| AppError::not_found("مقاله پیدا نشد."))?;
    if status == "DRAFT_CREATED" || status == "UPLOADING" {
        return Err(AppError::conflict("مقاله قبلاً در وردپرس ساخته شده یا در حال آپلود است."));
    }
    let mut seen = std::collections::HashSet::new();
    let clean: Vec<String> = tags
        .into_iter()
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty() && seen.insert(t.to_lowercase()))
        .collect();
    if clean.len() > 20 || clean.iter().any(|t| t.chars().count() > 60) {
        return Err(AppError::validation("حداکثر ۲۰ برچسب و هر برچسب حداکثر ۶۰ نویسه."));
    }
    let mut cats = category_ids;
    cats.sort_unstable();
    cats.dedup();
    sqlx::query("UPDATE article_plans SET wp_category_ids_json=?, tags_json=? WHERE id=?")
        .bind(serde_json::to_string(&cats).unwrap_or_else(|_| "[]".into()))
        .bind(serde_json::to_string(&clean).unwrap_or_else(|_| "[]".into()))
        .bind(plan_id)
        .execute(&state.db)
        .await?;
    Ok(())
}

pub async fn set_default_category(state: &AppState, project_id: &str, wp_id: Option<i64>) -> AppResult<()> {
    if let Some(id) = wp_id {
        let ok: Option<(i64,)> = sqlx::query_as("SELECT wp_id FROM wp_terms WHERE project_id=? AND kind='category' AND wp_id=?").bind(project_id).bind(id).fetch_optional(&state.db).await?;
        if ok.is_none() {
            return Err(AppError::validation("این دسته‌بندی در فهرست همگام‌شده‌ی پروژه نیست؛ ابتدا همگام‌سازی کنید."));
        }
    }
    let r = sqlx::query("UPDATE projects SET default_category_wp_id=? WHERE id=?").bind(wp_id).bind(project_id).execute(&state.db).await?;
    if r.rows_affected() == 0 {
        return Err(AppError::not_found("پروژه پیدا نشد."));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faq_is_escaped_and_schema_cannot_break_out() {
        let faq = vec![FaqItem { q: "a <b> & c?".into(), a: "x </script><script>evil()</script>".into() }];
        let html = assemble_post_html("<p>body</p>", &faq, true, "fa");
        assert!(html.contains("پرسش‌های متداول") && html.contains("&lt;b&gt; &amp; c?"));
        assert_eq!(html.matches("<script").count(), 1, "only the JSON-LD script exists: {html}");
        assert!(html.contains("\\u003c/script>"), "{html}");
        assert_eq!(assemble_post_html("<p>x</p>", &[], true, "fa"), "<p>x</p>");
    }
}
