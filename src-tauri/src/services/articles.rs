//! Manual editing of generated articles.

use crate::error::{AppError, AppResult};
use crate::services::generation::{self, ArticleDetail};
use crate::services::qc::{self, FaqItem, QcInput};
use crate::state::AppState;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArticlePatch {
    pub title: String,
    pub slug: String,
    pub excerpt: String,
    pub content_html: String,
    pub meta_title: String,
    pub meta_description: String,
    pub faq: Vec<FaqItem>,
}

/// Same sanitizer the pipeline uses; lets the editor preview exactly what will be stored.
pub fn preview_html(raw: &str) -> String {
    qc::sanitize_html(raw)
}

pub async fn update(state: &AppState, plan_id: &str, patch: ArticlePatch, expected_version: i64) -> AppResult<ArticleDetail> {
    let row: Option<(String, String, i64, String, String)> = sqlx::query_as(
        "SELECT p.status, k.keyword, c.version, p.campaign_id, COALESCE((SELECT pr.site_url FROM projects pr WHERE pr.id=p.project_id), '')
         FROM article_plans p JOIN keywords k ON k.id=p.keyword_id
         JOIN article_contents c ON c.plan_id=p.id WHERE p.id=?",
    )
    .bind(plan_id)
    .fetch_optional(&state.db)
    .await?;
    let (status, keyword, version, campaign_id, site_url) = row.ok_or_else(|| AppError::not_found("محتوایی برای این مقاله وجود ندارد."))?;
    if !matches!(status.as_str(), "CONTENT_GENERATED" | "READY_TO_UPLOAD" | "FAILED") {
        return Err(AppError::conflict(if status == "DRAFT_CREATED" || status == "UPLOADING" {
            "پس از آپلود، ویرایش محلی روی وردپرس اعمال نمی‌شود؛ مقاله را در خود وردپرس ویرایش کنید."
        } else {
            "در این وضعیت امکان ویرایش مقاله نیست."
        }));
    }
    if version != expected_version {
        return Err(AppError::conflict("مقاله در این فاصله تغییر کرده است؛ صفحه را دوباره باز کنید."));
    }

    let title = patch.title.trim().to_string();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(AppError::validation("عنوان باید بین ۱ تا ۲۰۰ نویسه باشد."));
    }
    let content = qc::sanitize_html(&patch.content_html);
    if qc::word_count(&qc::plain_text(&content)) == 0 {
        return Err(AppError::validation("متن مقاله نمی‌تواند خالی باشد."));
    }
    // The primary keyword is the only permitted slug source (including manual edits).
    let slug = qc::slugify(&keyword);
    if slug.is_empty() {
        return Err(AppError::validation("Slug نامعتبر است."));
    }
    let flat = |s: &str| qc::plain_text(s).split_whitespace().collect::<Vec<_>>().join(" ");
    let faq: Vec<FaqItem> = patch
        .faq
        .iter()
        .map(|f| FaqItem { q: flat(&f.q), a: flat(&f.a) })
        .filter(|f| !f.q.is_empty() && !f.a.is_empty())
        .collect();
    let excerpt = qc::truncate_chars(&flat(&patch.excerpt), 300);
    let meta_title = qc::truncate_chars(&flat(&patch.meta_title), 70);
    let meta_description = qc::truncate_chars(&flat(&patch.meta_description), 170);

    // Re-run quality control for information only: manual edits are never blocked by warnings.
    let (min_w, max_w, faq_enabled): (i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(t.min_words,0), COALESCE(t.max_words,100000), COALESCE(t.faq_enabled,0)
         FROM campaigns c LEFT JOIN article_templates t ON t.id=c.template_id WHERE c.id=?",
    )
    .bind(&campaign_id)
    .fetch_one(&state.db)
    .await?;
    let report = qc::validate(&QcInput {
        content_html: &content,
        keyword: &keyword,
        own_host: &qc::host_of(&site_url),
        min_words: min_w as usize,
        max_words: max_w as usize,
        faq_enabled: faq_enabled != 0,
        faq: &faq,
        others: &[],
    });
    let mut warnings = report.warnings.clone();
    warnings.extend(report.errors.iter().map(|e| format!("هشدار: {e}")));
    let validation = serde_json::json!({"wordCount": report.word_count, "maxSimilarity": 0, "errors": [], "warnings": warnings});

    sqlx::query(
        "UPDATE article_contents SET title=?, slug=?, excerpt=?, content_html=?, meta_title=?, meta_description=?, faq_json=?,
         word_count=?, validation_json=?, edited_by_user=1, version=version+1, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now')
         WHERE plan_id=? AND version=?",
    )
    .bind(&title)
    .bind(&slug)
    .bind(&excerpt)
    .bind(&content)
    .bind(&meta_title)
    .bind(&meta_description)
    .bind(serde_json::to_string(&faq).unwrap_or_else(|_| "[]".into()))
    .bind(report.word_count as i64)
    .bind(validation.to_string())
    .bind(plan_id)
    .bind(version)
    .execute(&state.db)
    .await?;
    generation::get_article(state, plan_id).await
}
