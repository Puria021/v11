use crate::error::{AppError, AppResult};
use crate::services::{ai, wordpress as wp};
use crate::state::AppState;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductItem {
    pub id: i64,
    pub name: String,
    pub sku: String,
    pub description: String,
    pub short_description: String,
    pub permalink: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductPage {
    pub items: Vec<ProductItem>,
    pub total: i64,
    pub pages: i64,
}

pub async fn list(state: &AppState, project_id: &str, search: &str, page: i64) -> AppResult<ProductPage> {
    let (wp_client, _) = wp::build_client(state, project_id).await?;
    let v = wp_client.list_products(search, page.clamp(1, 100_000)).await.map_err(|e| wp::refine(state, e))?;
    serde_json::from_value(v).map_err(|_| AppError::internal("پاسخ محصولات معتبر نیست."))
}

pub async fn generate(state: &AppState, provider_id: &str, name: &str, keyword: &str, existing: &str, language: &str) -> AppResult<String> {
    if name.trim().is_empty() || keyword.trim().is_empty() {
        return Err(AppError::validation("نام محصول و کلمه کلیدی الزامی است."));
    }
    let tpl = crate::services::templates::get(state, crate::services::templates::PRODUCT_TEMPLATE_ID).await?;
    let sys = format!("{}\n\nReturn only JSON with field description, containing safe HTML using p, h2, h3, ul, li, strong. No scripts or external claims.", tpl.system_instructions);
    let prompt = format!("Product: {name}\nSEO focus keyword: {keyword}\nLanguage: {language}\nCurrent long description for reference: {}\nWrite a complete informative new LONG product description. Preserve confirmed facts, avoid price, avoid product title changes. JSON: {{\"description\":\"<p>...</p>\"}}", existing.chars().take(4000).collect::<String>());
    let v = ai::complete_json(state, provider_id, &sys, &prompt, Some(4000)).await?;
    let html = v.get("description").and_then(|v| v.as_str()).ok_or_else(|| AppError::validation("توضیحات محصول در پاسخ AI وجود ندارد."))?;
    Ok(crate::services::qc::sanitize_html(html))
}

pub async fn save(state: &AppState, project_id: &str, product_id: i64, description: &str) -> AppResult<()> {
    if product_id <= 0 || description.trim().is_empty() || description.len() > 250_000 {
        return Err(AppError::validation("توضیحات یا شناسه محصول نامعتبر است."));
    }
    let html = crate::services::qc::sanitize_html(description);
    let (wp_client, _) = wp::build_client(state, project_id).await?;
    wp_client.update_product_description(product_id, &html).await.map_err(|e| wp::refine(state, e))?;
    Ok(())
}
