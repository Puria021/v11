//! Phase 2 integration tests: projects + WordPress connection, against a mock WordPress API.

use base64::Engine;
use myarticlemanager_lib::error::AppResult;
use myarticlemanager_lib::secrets::{MemorySecretStore, SecretStore};
use myarticlemanager_lib::services::projects::{self, ProjectInput};
use myarticlemanager_lib::services::wordpress::{self as wp, normalize_api_url, SaveConnection};
use myarticlemanager_lib::state::AppState;
use serde_json::json;
use std::sync::Arc;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn setup() -> (AppState, Arc<MemorySecretStore>) {
    let secrets = Arc::new(MemorySecretStore::new());
    let state = AppState::for_tests(secrets.clone()).await.unwrap();
    (state, secrets)
}

fn input(name: &str) -> ProjectInput {
    ProjectInput { name: name.into(), site_url: "example.com".into(), ..Default::default() }
}

fn basic(user: &str, pass: &str) -> String {
    format!("Basic {}", base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}")))
}

async fn project_with_conn(state: &AppState, server: &MockServer, password: &str) -> AppResult<String> {
    let p = projects::create(state, input("سایت تست")).await?;
    wp::save_connection(
        state,
        SaveConnection {
            project_id: p.id.clone(),
            api_url: server.uri(),
            username: "admin".into(),
            application_password: Some(password.into()),
            proxy_url: None,
        },
    )
    .await?;
    Ok(p.id)
}

#[tokio::test]
async fn project_crud_and_validation() {
    let (state, _) = setup().await;

    assert_eq!(projects::create(&state, input("   ")).await.unwrap_err().code, "VALIDATION");
    let mut bad = input("x");
    bad.site_url = "ftp://foo".into();
    assert_eq!(projects::create(&state, bad).await.unwrap_err().code, "VALIDATION");

    let p = projects::create(&state, input("  سایت پزشکی ")).await.unwrap();
    assert_eq!(p.name, "سایت پزشکی");
    assert_eq!(p.site_url, "https://example.com");
    assert_eq!(p.default_country, "IR");
    assert_eq!(p.campaign_count, 0);

    let mut upd = input("سایت گردشگری");
    upd.site_url = "https://travel.example.org/".into();
    let u = projects::update(&state, &p.id, upd).await.unwrap();
    assert_eq!(u.name, "سایت گردشگری");
    assert_eq!(u.site_url, "https://travel.example.org");

    assert_eq!(projects::list(&state).await.unwrap().len(), 1);
    assert_eq!(projects::get(&state, "nope").await.unwrap_err().code, "NOT_FOUND");
}

#[tokio::test]
async fn delete_requires_confirmation_and_removes_secret() {
    let (state, secrets) = setup().await;
    let server = MockServer::start().await;
    let id = project_with_conn(&state, &server, "abcd efgh").await.unwrap();
    let key = format!("wp-app-password:{id}");
    assert!(secrets.get(&key).unwrap().is_some());

    assert_eq!(projects::delete(&state, &id, false).await.unwrap_err().code, "VALIDATION");
    assert!(projects::get(&state, &id).await.is_ok());

    projects::delete(&state, &id, true).await.unwrap();
    assert_eq!(projects::get(&state, &id).await.unwrap_err().code, "NOT_FOUND");
    assert!(secrets.get(&key).unwrap().is_none());
    // cascade removed the connection row too
    assert!(wp::get_connection(&state, &id).await.unwrap().is_none());
}

#[test]
fn api_url_normalization() {
    assert_eq!(normalize_api_url("example.com").unwrap(), "https://example.com/wp-json");
    assert_eq!(normalize_api_url("https://example.com/").unwrap(), "https://example.com/wp-json");
    assert_eq!(normalize_api_url("https://example.com/wp-json/").unwrap(), "https://example.com/wp-json");
    assert_eq!(normalize_api_url("https://example.com/blog/wp-json/wp/v2").unwrap(), "https://example.com/blog/wp-json");
    assert!(normalize_api_url("ftp://example.com").is_err());
}

#[tokio::test]
async fn connection_dto_never_exposes_password() {
    let (state, _) = setup().await;
    let server = MockServer::start().await;
    let id = project_with_conn(&state, &server, "super-secret-pw").await.unwrap();
    let conn = wp::get_connection(&state, &id).await.unwrap().unwrap();
    assert!(conn.has_secret);
    let json = serde_json::to_string(&conn).unwrap();
    assert!(!json.contains("super-secret-pw"));
    assert_eq!(conn.status, "UNTESTED");

    // Updating without a password keeps the stored one.
    wp::save_connection(
        &state,
        SaveConnection {
            project_id: id.clone(),
            api_url: server.uri(),
            username: "editor".into(),
            application_password: None,
            proxy_url: None,
        },
    )
    .await
    .unwrap();
    let again = wp::get_connection(&state, &id).await.unwrap().unwrap();
    assert!(again.has_secret);
    assert_eq!(again.username, "editor");
}

#[tokio::test]
async fn test_connection_success_uses_basic_auth() {
    let (state, _) = setup().await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/wp-json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"name":"وب‌سایت من","namespaces":["wp/v2"]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/wp-json/wp/v2/users/me"))
        .and(header("authorization", basic("admin", "abcd efgh").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"id":1,"name":"Admin","capabilities":{"upload_files":true,"edit_posts":true}}),
        ))
        .mount(&server)
        .await;

    let id = project_with_conn(&state, &server, "abcd efgh").await.unwrap();
    let r = wp::test_connection(&state, &id).await.unwrap();
    assert!(r.success && r.media_capable);
    assert_eq!(r.site, "وب‌سایت من");
    assert_eq!(r.user, "Admin");

    let conn = wp::get_connection(&state, &id).await.unwrap().unwrap();
    assert_eq!(conn.status, "CONNECTED");
    assert_eq!(conn.media_capable, Some(true));
    assert_eq!(projects::get(&state, &id).await.unwrap().wp_status.as_deref(), Some("CONNECTED"));
}

#[tokio::test]
async fn test_connection_invalid_credentials() {
    let (state, _) = setup().await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/wp-json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"name":"S","namespaces":[]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/wp-json/wp/v2/users/me"))
        .respond_with(ResponseTemplate::new(401).set_body_json(json!({"code":"incorrect_password"})))
        .mount(&server)
        .await;

    let id = project_with_conn(&state, &server, "wrong").await.unwrap();
    let err = wp::test_connection(&state, &id).await.unwrap_err();
    assert_eq!(err.code, "AUTH_INVALID");
    assert!(!err.message.contains("wrong"));
    let conn = wp::get_connection(&state, &id).await.unwrap().unwrap();
    assert_eq!(conn.status, "FAILED");
    assert!(conn.last_error.is_some());
}

#[tokio::test]
async fn test_connection_non_json_and_not_found() {
    let (state, _) = setup().await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/wp-json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>Blocked by firewall</html>"))
        .mount(&server)
        .await;
    let id = project_with_conn(&state, &server, "pw").await.unwrap();
    assert_eq!(wp::test_connection(&state, &id).await.unwrap_err().code, "INVALID_RESPONSE");

    let server2 = MockServer::start().await; // everything 404
    let id2 = project_with_conn(&state, &server2, "pw").await.unwrap();
    assert_eq!(wp::test_connection(&state, &id2).await.unwrap_err().code, "NOT_FOUND");
}

#[tokio::test]
async fn sync_taxonomies_with_pagination() {
    let (state, _) = setup().await;
    let server = MockServer::start().await;

    for (pg, cat) in [("1", json!({"id":5,"name":"پزشکی","slug":"med","parent":0})), ("2", json!({"id":9,"name":"قلب","slug":"heart","parent":5}))] {
        Mock::given(method("GET"))
            .and(path("/wp-json/wp/v2/categories"))
            .and(query_param("page", pg))
            .respond_with(ResponseTemplate::new(200).insert_header("X-WP-TotalPages", "2").set_body_json(json!([cat])))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path("/wp-json/wp/v2/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id":3,"name":"سلامت","slug":"health"}])))
        .mount(&server)
        .await;
    // Users list forbidden → treated as "no authors", not an error.
    Mock::given(method("GET"))
        .and(path("/wp-json/wp/v2/users"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/wp-json/wp/v2/types"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "post": {"name":"نوشته‌ها","slug":"post","rest_base":"posts"},
            "page": {"name":"برگه‌ها","slug":"page","rest_base":"pages"},
            "wp_block": {"name":"hidden","slug":"wp_block"}
        })))
        .mount(&server)
        .await;

    let id = project_with_conn(&state, &server, "pw").await.unwrap();
    let r = wp::sync_taxonomies(&state, &id).await.unwrap();
    assert_eq!((r.categories, r.tags, r.authors, r.post_types), (2, 1, 0, 2));

    let cats = wp::list_terms(&state, &id, "category").await.unwrap();
    assert_eq!(cats.len(), 2);
    assert!(cats.iter().any(|c| c.wp_id == 9 && c.parent_wp_id == Some(5)));

    // Re-sync replaces rather than duplicates.
    wp::sync_taxonomies(&state, &id).await.unwrap();
    assert_eq!(wp::list_terms(&state, &id, "category").await.unwrap().len(), 2);
    assert!(wp::get_connection(&state, &id).await.unwrap().unwrap().last_synced_at.is_some());
    assert_eq!(wp::list_terms(&state, &id, "bogus").await.unwrap_err().code, "VALIDATION");
}

#[tokio::test]
async fn projects_are_isolated() {
    let (state, _) = setup().await;
    let server = MockServer::start().await;
    let a = project_with_conn(&state, &server, "a").await.unwrap();
    let b = projects::create(&state, input("B")).await.unwrap();
    assert!(wp::get_connection(&state, &a).await.unwrap().is_some());
    assert!(wp::get_connection(&state, &b.id).await.unwrap().is_none());
    assert_eq!(wp::test_connection(&state, &b.id).await.unwrap_err().code, "VALIDATION");
}
