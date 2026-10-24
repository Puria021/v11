use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, sqlx::FromRow)]
struct Row {
    id: String,
    name: String,
    system_instructions: String,
    article_structure: String,
    seo_rules: String,
    tone_of_voice: String,
    min_words: i64,
    max_words: i64,
    language: String,
    country: String,
    brand_info: String,
    forbidden_topics: String,
    required_sections: String,
    faq_enabled: i64,
    faq_count: i64,
    cta: String,
    image_style: String,
    image_aspect_ratio: String,
    image_size: String,
    image_count: i64,
    image_paragraph_interval: i64,
    keyword_density_target: Option<f64>,
    internal_linking: i64,
    internal_link_rules: String,
    faq_schema: i64,
    max_retries: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Template {
    pub id: Option<String>,
    pub name: String,
    pub system_instructions: String,
    pub article_structure: String,
    pub seo_rules: String,
    pub tone_of_voice: String,
    pub min_words: i64,
    pub max_words: i64,
    pub language: String,
    pub country: String,
    pub brand_info: String,
    pub forbidden_topics: String,
    pub required_sections: String,
    pub faq_enabled: bool,
    pub faq_count: i64,
    pub cta: String,
    pub image_style: String,
    pub image_aspect_ratio: String,
    pub image_size: String,
    pub image_count: i64,
    #[serde(default = "default_image_interval")]
    pub image_paragraph_interval: i64,
    pub keyword_density_target: Option<f64>,
    pub internal_linking: bool,
    #[serde(default)]
    pub internal_link_rules: String,
    pub faq_schema: bool,
    pub max_retries: i64,
}

fn default_image_interval() -> i64 { 3 }

impl From<Row> for Template {
    fn from(r: Row) -> Self {
        Template {
            id: Some(r.id),
            name: r.name,
            system_instructions: r.system_instructions,
            article_structure: r.article_structure,
            seo_rules: r.seo_rules,
            tone_of_voice: r.tone_of_voice,
            min_words: r.min_words,
            max_words: r.max_words,
            language: r.language,
            country: r.country,
            brand_info: r.brand_info,
            forbidden_topics: r.forbidden_topics,
            required_sections: r.required_sections,
            faq_enabled: r.faq_enabled != 0,
            faq_count: r.faq_count,
            cta: r.cta,
            image_style: r.image_style,
            image_aspect_ratio: r.image_aspect_ratio,
            image_size: r.image_size,
            image_count: r.image_count,
            image_paragraph_interval: r.image_paragraph_interval,
            keyword_density_target: r.keyword_density_target,
            internal_linking: r.internal_linking != 0,
            internal_link_rules: r.internal_link_rules,
            faq_schema: r.faq_schema != 0,
            max_retries: r.max_retries,
        }
    }
}

const COLS: &str = "id, name, system_instructions, article_structure, seo_rules, tone_of_voice, min_words, max_words, language, country,
  brand_info, forbidden_topics, required_sections, faq_enabled, faq_count, cta, image_style, image_aspect_ratio, image_size,
  image_count, image_paragraph_interval, keyword_density_target, internal_linking, internal_link_rules, faq_schema, max_retries";

/// The two fixed templates. They can be edited but never created, copied or deleted.
pub const ARTICLE_TEMPLATE_ID: &str = "tpl_default";
pub const PRODUCT_TEMPLATE_ID: &str = "tpl_product";

fn fixed_error() -> AppError {
    AppError::validation("فقط دو قالب ثابت (مقاله و توضیحات محصول) وجود دارد؛ ساخت، کپی و حذف قالب ممکن نیست.")
}

pub async fn list(state: &AppState) -> AppResult<Vec<Template>> {
    let rows = sqlx::query_as::<_, Row>(&format!("SELECT {COLS} FROM article_templates ORDER BY created_at, rowid"))
        .fetch_all(&state.db)
        .await?;
    Ok(rows.into_iter().map(Template::from).collect())
}

/// Test helper: the seeded default template, read synchronously via a throw-away runtime-free default.
pub fn list_blocking_default() -> Template {
    Template {
        id: None,
        name: "قالب".into(),
        system_instructions: "تو یک نویسنده‌ی سئو هستی.".into(),
        article_structure: "مقدمه\nبخش‌ها\nجمع‌بندی".into(),
        seo_rules: "{{secondary_keyword}} را طبیعی بیاور.".into(),
        tone_of_voice: "صمیمی".into(),
        min_words: 800,
        max_words: 1600,
        language: "fa".into(),
        country: "IR".into(),
        brand_info: String::new(),
        forbidden_topics: String::new(),
        required_sections: "مقدمه".into(),
        faq_enabled: true,
        faq_count: 5,
        cta: String::new(),
        image_style: "مینیمال".into(),
        image_aspect_ratio: "3:2".into(),
        image_size: "1536x1024".into(),
        image_count: 1,
        image_paragraph_interval: 3,
        keyword_density_target: None,
        internal_linking: false,
        internal_link_rules: DEFAULT_INTERNAL_LINK_RULES.into(),
        faq_schema: true,
        max_retries: 2,
    }
}

pub async fn get(state: &AppState, id: &str) -> AppResult<Template> {
    sqlx::query_as::<_, Row>(&format!("SELECT {COLS} FROM article_templates WHERE id = ?"))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .map(Template::from)
        .ok_or_else(|| AppError::not_found("قالب پیدا نشد."))
}

fn validate(t: &Template) -> AppResult<()> {
    if t.name.trim().is_empty() || t.name.chars().count() > 120 {
        return Err(AppError::validation("نام قالب الزامی است."));
    }
    if t.min_words < 50 || t.max_words < t.min_words || t.max_words > 6000 {
        return Err(AppError::validation("محدوده‌ی تعداد کلمات نامعتبر است (حداقل ۵۰، حداکثر ۶۰۰۰ و بزرگ‌تر از حداقل)."));
    }
    if !(0..=10).contains(&t.image_count) {
        return Err(AppError::validation("تعداد تصاویر باید بین ۰ تا ۱۰ باشد."));
    }
    if !(1..=30).contains(&t.image_paragraph_interval) { return Err(AppError::validation("فاصله تصاویر باید بین ۱ تا ۳۰ پاراگراف باشد.")); }
    if t.image_count > 0 {
        let ok = t.image_size.split_once('x').map(|(w, h)| w.parse::<u32>().is_ok() && h.parse::<u32>().is_ok()).unwrap_or(false);
        if !ok {
            return Err(AppError::validation("اندازه‌ی تصویر باید مثل 1536x1024 باشد."));
        }
    }
    if t.faq_enabled && !(1..=15).contains(&t.faq_count) {
        return Err(AppError::validation("تعداد پرسش‌های FAQ باید بین ۱ تا ۱۵ باشد."));
    }
    if !(0..=5).contains(&t.max_retries) {
        return Err(AppError::validation("تعداد تلاش مجدد باید بین ۰ تا ۵ باشد."));
    }
    if let Some(d) = t.keyword_density_target {
        if !(0.0..=10.0).contains(&d) {
            return Err(AppError::validation("تراکم کلمه‌ی کلیدی باید بین ۰ تا ۱۰ درصد باشد."));
        }
    }
    if t.language.trim().len() < 2 || t.country.trim().len() != 2 {
        return Err(AppError::validation("زبان یا کشور قالب نامعتبر است."));
    }
    Ok(())
}

pub async fn save(state: &AppState, t: Template) -> AppResult<Template> {
    match t.id.as_deref() {
        Some(id) if id == ARTICLE_TEMPLATE_ID || id == PRODUCT_TEMPLATE_ID => {}
        _ => return Err(fixed_error()),
    }
    validate(&t)?;
    let id = match &t.id {
        Some(id) => {
            let e: Option<(String,)> = sqlx::query_as("SELECT id FROM article_templates WHERE id=?").bind(id).fetch_optional(&state.db).await?;
            e.ok_or_else(|| AppError::not_found("قالب پیدا نشد."))?;
            id.clone()
        }
        None => ulid::Ulid::new().to_string(),
    };
    sqlx::query(
        "INSERT INTO article_templates (id, name, system_instructions, article_structure, seo_rules, tone_of_voice, min_words, max_words,
           language, country, brand_info, forbidden_topics, required_sections, faq_enabled, faq_count, cta, image_style,
           image_aspect_ratio, image_size, image_count, image_paragraph_interval, keyword_density_target, internal_linking, internal_link_rules, faq_schema, max_retries)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, system_instructions=excluded.system_instructions,
           article_structure=excluded.article_structure, seo_rules=excluded.seo_rules, tone_of_voice=excluded.tone_of_voice,
           min_words=excluded.min_words, max_words=excluded.max_words, language=excluded.language, country=excluded.country,
           brand_info=excluded.brand_info, forbidden_topics=excluded.forbidden_topics, required_sections=excluded.required_sections,
           faq_enabled=excluded.faq_enabled, faq_count=excluded.faq_count, cta=excluded.cta, image_style=excluded.image_style,
           image_aspect_ratio=excluded.image_aspect_ratio, image_size=excluded.image_size, image_count=excluded.image_count, image_paragraph_interval=excluded.image_paragraph_interval,
           keyword_density_target=excluded.keyword_density_target, internal_linking=excluded.internal_linking, internal_link_rules=excluded.internal_link_rules,
           faq_schema=excluded.faq_schema, max_retries=excluded.max_retries,
           updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')",
    )
    .bind(&id)
    .bind(t.name.trim())
    .bind(&t.system_instructions)
    .bind(&t.article_structure)
    .bind(&t.seo_rules)
    .bind(&t.tone_of_voice)
    .bind(t.min_words)
    .bind(t.max_words)
    .bind(t.language.trim().to_lowercase())
    .bind(t.country.trim().to_uppercase())
    .bind(&t.brand_info)
    .bind(&t.forbidden_topics)
    .bind(&t.required_sections)
    .bind(t.faq_enabled as i64)
    .bind(t.faq_count)
    .bind(&t.cta)
    .bind(&t.image_style)
    .bind(&t.image_aspect_ratio)
    .bind(&t.image_size)
    .bind(t.image_count)
    .bind(t.image_paragraph_interval)
    .bind(t.keyword_density_target)
    .bind(t.internal_linking as i64)
    .bind(&t.internal_link_rules)
    .bind(t.faq_schema as i64)
    .bind(t.max_retries)
    .execute(&state.db)
    .await?;
    get(state, &id).await
}

pub async fn duplicate(_state: &AppState, _id: &str) -> AppResult<Template> {
    Err(fixed_error())
}

pub async fn delete(_state: &AppState, _id: &str) -> AppResult<()> {
    Err(fixed_error())
}

/// Replaces `{{name}}` placeholders; unknown placeholders are left untouched.
pub fn render(text: &str, vars: &HashMap<&str, String>) -> String {
    let mut out = text.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{{{k}}}}}"), v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_known_placeholders_only() {
        let mut v = HashMap::new();
        v.insert("title", "عنوان".to_string());
        v.insert("brand_name", "برند".to_string());
        assert_eq!(render("{{title}} - {{brand_name}} - {{unknown}}", &v), "عنوان - برند - {{unknown}}");
    }
}

/// Default safe internal linking policy. Only validated URLs from the CMS may be used.
pub const DEFAULT_INTERNAL_LINK_RULES: &str = "فقط به URLهای واقعی و منتشرشده که برنامه صریحاً ارائه کرده لینک بده. URL یا نامک را هرگز حدس نزن. اگر فهرست لینک تأییدشده خالی است، هیچ لینک داخلی ایجاد نکن. از لینک فرضی یا صفحه منتشرنشده استفاده نکن. متن لینک طبیعی و مرتبط باشد.";
