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
