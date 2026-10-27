//! Local usage counters for the license. Each license includes 10 articles (text + images, one unit each)
//! and 5 product descriptions. Counters and totals live in `quota_usage`; counters reset on activation.
//! Planned-but-unwritten articles (campaign targets) are reserved so campaigns cannot exceed the quota.

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use serde::Serialize;


#[derive(Debug, Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub struct QuotaBucket {
    pub total: i64,
    pub used: i64,
    pub remaining: i64,
    pub available: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quota {
    pub articles: QuotaBucket,
    pub products: QuotaBucket,
}

/// (articles_used, products_used, epoch) plus the totals granted by the license.
async fn counters(state: &AppState) -> AppResult<(i64, i64, i64)> {
    let row: (i64, i64, i64) = sqlx::query_as("SELECT articles_used, products_used, epoch FROM quota_usage WHERE id = 1")
        .fetch_one(&state.db)
        .await?;
    Ok(row)
}

async fn totals(state: &AppState) -> AppResult<(i64, i64)> {
    let row: (i64, i64) = sqlx::query_as("SELECT article_total, product_total FROM quota_usage WHERE id = 1").fetch_one(&state.db).await?;
    Ok(row)
}

/// Articles still promised to campaigns of the current license epoch but not written yet.
async fn reserved_articles(state: &AppState, epoch: i64) -> AppResult<i64> {
    let (n,): (i64,) = sqlx::query_as(
        "SELECT COALESCE(SUM(MAX(0, c.target_count - (
            SELECT COUNT(*) FROM article_contents ac JOIN article_plans p ON p.id = ac.plan_id WHERE p.campaign_id = c.id
         ))), 0) FROM campaigns c WHERE c.quota_epoch = ?",
    )
    .bind(epoch)
    .fetch_one(&state.db)
    .await?;
    Ok(n)
}

pub async fn view(state: &AppState) -> AppResult<Quota> {
    let (articles_used, products_used, epoch) = counters(state).await?;
    let (article_total, product_total) = totals(state).await?;
    let reserved = reserved_articles(state, epoch).await?;
    let articles = QuotaBucket {
        total: article_total,
        used: articles_used,
        remaining: (article_total - articles_used).max(0),
        available: (article_total - articles_used - reserved).max(0),
    };
    let products = QuotaBucket {
        total: product_total,
        used: products_used,
        remaining: (product_total - products_used).max(0),
        available: (product_total - products_used).max(0),
    };
    Ok(Quota { articles, products })
}

/// Articles a new campaign (or single article) may still be planned for.
pub async fn article_available(state: &AppState) -> AppResult<i64> {
    Ok(view(state).await?.articles.available)
}

/// The current license epoch; campaigns created now are stamped with it.
pub async fn current_epoch(state: &AppState) -> AppResult<i64> {
    Ok(counters(state).await?.2)
}

/// Counts one written article. Called once per article, when its text is first saved.
pub async fn consume_article(state: &AppState) -> AppResult<()> {
    sqlx::query("UPDATE quota_usage SET articles_used = articles_used + 1 WHERE id = 1").execute(&state.db).await?;
    Ok(())
}

/// Checks and counts one product description. Fails before any AI call when the quota is spent.
pub async fn take_product(state: &AppState) -> AppResult<()> {
    let (_, used, _) = counters(state).await?;
    let (_, product_total) = totals(state).await?;
    if used >= product_total {
        return Err(AppError::validation("سهمیه‌ی توضیحات محصول این لایسنس تمام شده است."));
    }
    sqlx::query("UPDATE quota_usage SET products_used = products_used + 1 WHERE id = 1").execute(&state.db).await?;
    Ok(())
}

/// Called after a license is activated: counters restart and campaigns from older licenses stop reserving.
pub async fn reset(state: &AppState) -> AppResult<()> {
    sqlx::query("UPDATE quota_usage SET articles_used = 0, products_used = 0, epoch = epoch + 1 WHERE id = 1")
        .execute(&state.db)
        .await?;
    Ok(())
}
