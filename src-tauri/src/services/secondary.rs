use crate::error::{AppError, AppResult};
use crate::services::keywords::{insert_keyword, NewKeyword};
use crate::services::{ai, campaigns, logs};
use crate::state::AppState;
use crate::util::json::string_items;
use crate::util::text::{language_name, normalize, similarity};
use serde::Serialize;

const MAX_ROUNDS: usize = 18;
/// A new keyword this similar to an accepted one is dropped outright (softer overlaps become UI warnings).
const HARD_DUP: f64 = 0.85;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecondaryResult {
    pub target: i64,
    pub total: i64,
    pub added: usize,
    pub missing: i64,
    pub rounds: usize,
}

async fn plan_count(state: &AppState, campaign_id: &str) -> AppResult<i64> {
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM article_plans WHERE campaign_id=?")
        .bind(campaign_id)
        .fetch_one(&state.db)
        .await?;
    Ok(n)
}

/// Generates secondary keywords until the campaign has `target_count` of them (or sources run dry).
/// `replace = true` first deletes the current secondary set. Search metrics are never invented:
/// AI keywords are stored with NULL volume/competition/CPC.
pub async fn generate(state: &AppState, campaign_id: &str, replace: bool) -> AppResult<SecondaryResult> {
    let c = campaigns::get(state, campaign_id).await?;
    if plan_count(state, campaign_id).await? > 0 {
        return Err(AppError::conflict("برنامه‌ی مقالات قبلاً ساخته شده است؛ ابتدا آن را بازنشانی کنید."));
    }
    let provider = campaigns::require_ai_provider(&c)?;

    // Basis = primary keywords + the candidates the user selected.
    let basis: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT keyword, source, related_json FROM keywords
         WHERE campaign_id=? AND (role='primary' OR (role='candidate' AND is_selected=1)) ORDER BY rowid",
    )
    .bind(campaign_id)
    .fetch_all(&state.db)
    .await?;
    if basis.is_empty() {
        return Err(AppError::validation("ابتدا کلمات کلیدی اصلی را وارد و کلمات مورد نظر را انتخاب کنید."));
    }

    if replace {
        sqlx::query("DELETE FROM keywords WHERE campaign_id=? AND role='secondary'")
            .bind(campaign_id)
            .execute(&state.db)
            .await?;
    }

    // Everything already in the campaign counts as "taken".
    let taken_rows: Vec<(String,)> = sqlx::query_as("SELECT keyword FROM keywords WHERE campaign_id=?")
        .bind(campaign_id)
        .fetch_all(&state.db)
        .await?;
    let mut taken: Vec<String> = taken_rows.into_iter().map(|r| r.0).collect();
    let (have0,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM keywords WHERE campaign_id=? AND role='secondary'")
        .bind(campaign_id)
        .fetch_one(&state.db)
        .await?;
    let mut have = have0;
    let target = c.target_count;
    let mut added = 0usize;

    let is_free = |kw: &str, taken: &[String]| -> bool {
        let n = normalize(kw);
        !n.is_empty() && !taken.iter().any(|t| normalize(t) == n || similarity(t, kw) >= HARD_DUP)
    };

    // 1) Related keywords that came with the user's own data (CSV "related" column).
    let mut tx = state.db.begin().await?;
    'related: for (_, source, related) in &basis {
        let Some(json) = related else { continue };
        let list: Vec<String> = serde_json::from_str(json).unwrap_or_default();
        for kw in list {
            if have >= target {
                break 'related;
            }
            if kw.chars().count() > 200 || !is_free(&kw, &taken) {
                continue;
            }
            let src: &'static str = if source == "keyword_api" { "keyword_api" } else { "user" };
            if insert_keyword(&mut tx, &c, NewKeyword::plain(&kw, "secondary", src, None)).await? {
                taken.push(kw);
                have += 1;
                added += 1;
            }
        }
    }

    tx.commit().await?;

    // 2) AI rounds for the rest. No transaction is held while waiting for the AI.
    let seeds: Vec<&str> = basis.iter().map(|b| b.0.as_str()).collect();
    let mut rounds = 0;
    let mut stalls = 0;
    while have < target && rounds < MAX_ROUNDS && stalls < 2 {
        rounds += 1;
        let need = (target - have) as usize;
        let ask = (need + need / 2 + 5).clamp(10, 80);
        let system = "You are an SEO content strategist. Reply with ONLY valid JSON, no commentary.";
        let user = format!(
            "Language: {lang}. Target country: {country}.\nMain keywords: {seeds}\n\
             Already used (do not repeat or merely rephrase any of these): {taken}\n\n\
             Produce {ask} NEW, distinct secondary keywords. Each must be able to support its own article, \
             cover a different sub-topic, and the set should mix informational, commercial and transactional intents. \
             Do NOT include search volumes — you do not have that data.\n\
             Return a JSON array: [{{\"keyword\":\"...\",\"intent\":\"informational|commercial|transactional|navigational\"}}]",
            lang = language_name(&c.language),
            country = c.country,
            seeds = serde_json::to_string(&seeds).unwrap_or_default(),
            taken = serde_json::to_string(&taken.iter().take(150).collect::<Vec<_>>()).unwrap_or_default(),
        );
        let v = match ai::complete_json(state, &provider, system, &user, Some(3500)).await {
            Ok(v) => v,
            Err(e) => {
                // Keep what we already have; report the failure.
                logs::record(state, "ERROR", Some(&c.project_id), Some(&c.id), "secondary_keywords", "FAILED", &e.message).await;
                return Err(e);
            }
        };
        let mut round_added = 0;
        let mut tx = state.db.begin().await?;
        for (kw, intent) in string_items(&v, &["keyword", "text"], "intent") {
            if have >= target {
                break;
            }
            if kw.chars().count() > 200 || !is_free(&kw, &taken) {
                continue;
            }
            if insert_keyword(&mut tx, &c, NewKeyword::plain(&kw, "secondary", "ai", intent)).await? {
                taken.push(kw);
                have += 1;
                added += 1;
                round_added += 1;
            }
        }
        tx.commit().await?;
        if round_added == 0 {
            stalls += 1;
        } else {
            stalls = 0;
        }
    }
    let result = SecondaryResult { target, total: have, added, missing: (target - have).max(0), rounds };
    logs::record(
        state,
        if result.missing == 0 { "INFO" } else { "WARN" },
        Some(&c.project_id),
        Some(&c.id),
        "secondary_keywords",
        if result.missing == 0 { "SUCCESS" } else { "WARN" },
        &format!("total={} target={} added={} rounds={}", result.total, target, added, rounds),
    )
    .await;
    campaigns::set_step(state, campaign_id, 6).await?;
    Ok(result)
}

/// Locks in the secondary set: creates one article plan per keyword (exactly `target_count`).
pub async fn confirm(state: &AppState, campaign_id: &str) -> AppResult<i64> {
    let c = campaigns::get(state, campaign_id).await?;
    if plan_count(state, campaign_id).await? > 0 {
        return Err(AppError::conflict("برنامه‌ی مقالات قبلاً ساخته شده است."));
    }
    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, intent FROM keywords WHERE campaign_id=? AND role='secondary' ORDER BY rowid")
            .bind(campaign_id)
            .fetch_all(&state.db)
            .await?;
    if rows.len() as i64 != c.target_count {
        return Err(AppError::validation(format!(
            "برای ادامه دقیقاً {} کلمه‌ی کلیدی ثانویه لازم است؛ اکنون {} مورد دارید.",
            c.target_count,
            rows.len()
        )));
    }
    let mut tx = state.db.begin().await?;
    for (i, (kid, intent)) in rows.iter().enumerate() {
        sqlx::query(
            "INSERT INTO article_plans (id, project_id, campaign_id, article_no, keyword_id, intent) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(&c.project_id)
        .bind(campaign_id)
        .bind((i + 1) as i64)
        .bind(kid)
        .bind(intent)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("UPDATE campaigns SET status='IN_PROGRESS' WHERE id=?").bind(campaign_id).execute(&mut *tx).await?;
    tx.commit().await?;
    campaigns::set_step(state, campaign_id, 7).await?;
    logs::record(state, "INFO", Some(&c.project_id), Some(&c.id), "article_plans_created", "SUCCESS", &format!("count={}", rows.len())).await;
    Ok(rows.len() as i64)
}

/// Deletes all article plans (and their titles) so keywords can be changed again.
pub async fn reset_plans(state: &AppState, campaign_id: &str, confirm: bool) -> AppResult<()> {
    if !confirm {
        return Err(AppError::validation("بازنشانی برنامه‌ی مقالات نیاز به تأیید صریح دارد."));
    }
    campaigns::get(state, campaign_id).await?;
    sqlx::query("DELETE FROM article_plans WHERE campaign_id=?").bind(campaign_id).execute(&state.db).await?;
    sqlx::query("UPDATE campaigns SET status='DRAFT', wizard_step=6 WHERE id=?").bind(campaign_id).execute(&state.db).await?;
    Ok(())
}
