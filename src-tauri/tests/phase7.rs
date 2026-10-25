//! Migration 0011 must succeed on older databases where the default article template row is missing.

use myarticlemanager_lib::db;

const MIGRATION_0011: &str = include_str!("../migrations/0011_fixed_templates.sql");

#[tokio::test]
async fn migration_0011_runs_when_default_template_row_is_missing() {
    let pool = db::connect_memory().await.unwrap();
    // Older database: no tpl_default row, and a campaign still points at a custom template.
    sqlx::query("INSERT INTO article_templates (id, name) VALUES ('tpl_custom', 'قالب دلخواه')").execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM article_templates WHERE id = 'tpl_default'").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO projects (id, name, site_url) VALUES ('p1', 'پروژه', 'https://example.com')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO campaigns (id, project_id, name, template_id) VALUES ('c1', 'p1', 'کمپین', 'tpl_custom')").execute(&pool).await.unwrap();

    sqlx::raw_sql(MIGRATION_0011).execute(&pool).await.expect("migration 0011 must not fail");

    let (tpl,): (Option<String>,) = sqlx::query_as("SELECT template_id FROM campaigns WHERE id = 'c1'").fetch_one(&pool).await.unwrap();
    assert_eq!(tpl.as_deref(), Some("tpl_default"));
}

#[tokio::test]
async fn fresh_database_gets_default_article_template_content() {
    let pool = db::connect_memory().await.unwrap();
    let (instructions, min_words, image_count, linking): (String, i64, i64, i64) = sqlx::query_as(
        "SELECT system_instructions, min_words, image_count, internal_linking FROM article_templates WHERE id = 'tpl_default'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(instructions.contains("نویسنده‌ی حرفه‌ای"), "system instructions must be filled");
    assert_eq!((min_words, image_count, linking), (2000, 3, 1));
}

#[tokio::test]
async fn default_template_migration_does_not_overwrite_user_content() {
    let pool = db::connect_memory().await.unwrap();
    sqlx::query("UPDATE article_templates SET system_instructions = 'متن کاربر', min_words = 900 WHERE id = 'tpl_default'").execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0013_default_article_template.sql")).execute(&pool).await.unwrap();
    let (instructions, min_words): (String, i64) = sqlx::query_as("SELECT system_instructions, min_words FROM article_templates WHERE id = 'tpl_default'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(instructions, "متن کاربر");
    assert_eq!(min_words, 900);
}
