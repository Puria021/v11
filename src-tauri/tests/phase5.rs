//! Phase 5 integration tests: WordPress drafts (media / tags / categories / duplicates / adoption),
//! article editing, logs + CSV export, notifications and backup / restore.

use myarticlemanager_lib::events::CollectEvents;
use myarticlemanager_lib::queue::Scheduler;
use myarticlemanager_lib::secrets::MemorySecretStore;
use myarticlemanager_lib::services::articles::{self, ArticlePatch};
use myarticlemanager_lib::services::campaigns::{self, CreateCampaign};
use myarticlemanager_lib::services::logs::{self, LogQuery};
use myarticlemanager_lib::services::projects::{self, ProjectInput};
use myarticlemanager_lib::services::qc::FaqItem;
use myarticlemanager_lib::services::wordpress::{self as wp, SaveConnection};
use myarticlemanager_lib::services::{backup, generation, upload};
use myarticlemanager_lib::state::AppState;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

struct Cx {
    state: AppState,
    events: Arc<CollectEvents>,
    server: MockServer,
    project_id: String,
    campaign_id: String,
    plans: Vec<String>,
    sched: Scheduler,
}

fn png() -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(PNG_B64).unwrap()
}

async fn new_state() -> (AppState, Arc<CollectEvents>) {
    let events = Arc::new(CollectEvents::default());
    let state = AppState::for_tests_with_events(Arc::new(MemorySecretStore::new()), events.clone()).await.unwrap();
    (state, events)
}

/// A project with a WordPress connection and `n` articles that are ready to upload (image included).
async fn setup(n: usize, with_image: bool) -> Cx {
    let (state, events) = new_state().await;
    let server = MockServer::start().await;
    let p = projects::create(&state, ProjectInput { name: "سایت من".into(), site_url: "example.com".into(), ..Default::default() }).await.unwrap();
    wp::save_connection(&state, SaveConnection {
        project_id: p.id.clone(), api_url: server.uri(), username: "admin".into(),
        application_password: Some("app pass-word".into()), proxy_url: None,
    }).await.unwrap();
    let c = campaigns::create(&state, CreateCampaign {
        project_id: p.id.clone(), name: "کمپین".into(), target_count: Some(n as i64), country: None, language: None, ai_provider_id: None,
    }).await.unwrap();
    sqlx::query("UPDATE campaigns SET status='IN_PROGRESS' WHERE id=?").bind(&c.id).execute(&state.db).await.unwrap();
    // Keep the upload flow of these tests independent of the shipped default template content.
    sqlx::query("UPDATE article_templates SET image_count=1, internal_linking=0 WHERE id='tpl_default'").execute(&state.db).await.unwrap();

    let mut plans = Vec::new();
    for i in 1..=n {
        let (kid, pid) = (format!("k{i}"), format!("plan{i}"));
        sqlx::query("INSERT INTO keywords (id, project_id, campaign_id, keyword, norm, role, source, intent) VALUES (?,?,?,?,?,'secondary','ai','informational')")
            .bind(&kid).bind(&p.id).bind(&c.id).bind(format!("کلمه {i}")).bind(format!("کلمه {i}")).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO article_plans (id, project_id, campaign_id, article_no, keyword_id, status) VALUES (?,?,?,?,?,'READY_TO_UPLOAD')")
            .bind(&pid).bind(&p.id).bind(&c.id).bind(i as i64).bind(&kid).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO article_titles (id, plan_id, title, origin, state) VALUES (?,?,?,'ai','APPROVED')")
            .bind(format!("t{i}")).bind(&pid).bind(format!("عنوان مقاله {i}؟")).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO article_contents (id, plan_id, title, slug, excerpt, content_html, meta_title, meta_description, faq_json, image_prompt, word_count) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
            .bind(format!("c{i}")).bind(&pid).bind(format!("عنوان مقاله {i}؟")).bind(format!("slug-{i}")).bind("خلاصه")
            .bind(format!("<h2>الف</h2><p>متن مقاله {i} با کلمه {i}</p><h2>ب</h2><p>ادامه</p>"))
            .bind("متا").bind("توضیح").bind(r#"[{"q":"چرا <b>؟","a":"چون & دلیل."}]"#).bind("prompt").bind(120i64)
            .execute(&state.db).await.unwrap();
        if with_image {
            let rel = format!("projects/{}/campaigns/{}/images/article-{:03}.png", p.id, c.id, i);
            let abs = state.data_dir.join(&rel);
            std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
            std::fs::write(&abs, png()).unwrap();
            sqlx::query("INSERT INTO article_images (id, plan_id, is_featured, prompt, local_path, mime_type, alt_text) VALUES (?,?,1,'p',?, 'image/png', ?)")
                .bind(format!("i{i}")).bind(&pid).bind(&rel).bind(format!("عنوان مقاله {i}؟")).execute(&state.db).await.unwrap();
        }
        plans.push(pid);
    }
    Cx { state, events, server, project_id: p.id, campaign_id: c.id, plans, sched: Scheduler::new() }
}

/// Hands out a new post id per created post, like WordPress does.
struct PostIds(std::sync::atomic::AtomicI64, u16);
impl wiremock::Respond for PostIds {
    fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
        let id = 900 + self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        ResponseTemplate::new(self.1).set_body_json(json!({"id": id, "link": format!("https://x/?p={id}"), "status": "draft"}))
    }
}

async fn mount_wp(server: &MockServer, post_status: u16) {
    Mock::given(method("POST")).and(path("/wp-json/wp/v2/media")).respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": 77, "source_url": "https://x/m.png"}))).mount(server).await;
    Mock::given(method("POST")).and(path("/wp-json/wp/v2/media/77")).respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": 77}))).mount(server).await;
    Mock::given(method("GET")).and(path("/wp-json/wp/v2/tags")).respond_with(ResponseTemplate::new(200).set_body_json(json!([]))).mount(server).await;
    Mock::given(method("POST")).and(path("/wp-json/wp/v2/tags")).respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": 5, "name": "برچسب"}))).mount(server).await;
    Mock::given(method("GET")).and(path("/wp-json/wp/v2/posts")).respond_with(ResponseTemplate::new(200).set_body_json(json!([]))).mount(server).await;
    Mock::given(method("POST")).and(path("/wp-json/wp/v2/posts")).respond_with(PostIds(std::sync::atomic::AtomicI64::new(0), post_status)).mount(server).await;
}

async fn reqs(server: &MockServer, m: &str, p: &str) -> Vec<wiremock::Request> {
    server.received_requests().await.unwrap().into_iter().filter(|r| r.method.as_str() == m && r.url.path() == p).collect()
}

async fn drain(cx: &Cx) {
    cx.sched.drain(&cx.state, Duration::from_secs(30)).await.unwrap();
}

#[tokio::test]
async fn creates_a_draft_with_image_taxonomy_and_blocks_duplicates() {
    let cx = setup(1, true).await;
    mount_wp(&cx.server, 201).await;
    // categories come from the synced cache
    sqlx::query("INSERT INTO wp_terms (id, project_id, kind, wp_id, name) VALUES ('c3', ?, 'category', 3, 'اخبار')").bind(&cx.project_id).execute(&cx.state.db).await.unwrap();
    upload::set_taxonomy(&cx.state, &cx.plans[0], vec![3], vec!["برچسب".into(), "  برچسب ".into()]).await.unwrap();

    upload::enqueue_upload(&cx.state, &cx.plans[0]).await.unwrap();
    assert_eq!(upload::enqueue_upload(&cx.state, &cx.plans[0]).await.unwrap_err().code, "CONFLICT", "already queued");
    drain(&cx).await;

    let posts = reqs(&cx.server, "POST", "/wp-json/wp/v2/posts").await;
    assert_eq!(posts.len(), 1);
    let body: Value = serde_json::from_slice(&posts[0].body).unwrap();
    assert_eq!(body["status"], "draft", "always a draft");
    assert_eq!((body["featured_media"].as_i64(), body["categories"].clone(), body["tags"].clone()), (Some(77), json!([3]), json!([5])));
    let html = body["content"].as_str().unwrap();
    assert!(html.contains("<h2>الف</h2>") && html.contains("پرسش‌های متداول") && html.contains("چرا &lt;b&gt;؟") && html.contains("چون &amp; دلیل."));
    assert_eq!(body["slug"], "slug-1");

    let media = reqs(&cx.server, "POST", "/wp-json/wp/v2/media").await;
    assert_eq!(media.len(), 1);
    assert_eq!(media[0].headers.get("content-type").unwrap().to_str().unwrap(), "image/png");
    assert!(media[0].headers.get("content-disposition").unwrap().to_str().unwrap().contains("filename=\"article-001-"));
    let meta: Value = serde_json::from_slice(&reqs(&cx.server, "POST", "/wp-json/wp/v2/media/77").await[0].body).unwrap();
    assert_eq!(meta["alt_text"], "عنوان مقاله 1؟");
    assert_eq!(reqs(&cx.server, "POST", "/wp-json/wp/v2/tags").await.len(), 1, "duplicate tag names collapse");

    let (status, wp_id, url, st): (String, i64, String, String) = sqlx::query_as(
        "SELECT p.status, w.wordpress_post_id, w.wordpress_url, w.upload_status FROM article_plans p JOIN wordpress_posts w ON w.plan_id=p.id WHERE p.id=?",
    ).bind(&cx.plans[0]).fetch_one(&cx.state.db).await.unwrap();
    assert_eq!((status.as_str(), wp_id, url.as_str(), st.as_str()), ("DRAFT_CREATED", 900, "https://x/?p=900", "UPLOADED"));
    let (mid,): (Option<i64>,) = sqlx::query_as("SELECT wp_media_id FROM article_images WHERE plan_id=?").bind(&cx.plans[0]).fetch_one(&cx.state.db).await.unwrap();
    assert_eq!(mid, Some(77));
    assert_eq!(generation::progress(&cx.state, &cx.campaign_id).await.unwrap().wordpress.done, 1);

    // Second attempts are refused and never reach WordPress.
    assert_eq!(upload::enqueue_upload(&cx.state, &cx.plans[0]).await.unwrap_err().code, "DUPLICATE_BLOCKED");
    assert_eq!(generation::retry_article(&cx.state, &cx.plans[0], "upload").await.unwrap_err().code, "DUPLICATE_BLOCKED");
    assert_eq!(reqs(&cx.server, "POST", "/wp-json/wp/v2/posts").await.len(), 1);
    // …and the article is now locked for local taxonomy edits / editing.
    assert_eq!(upload::set_taxonomy(&cx.state, &cx.plans[0], vec![], vec![]).await.unwrap_err().code, "CONFLICT");
    let a = generation::get_article(&cx.state, &cx.plans[0]).await.unwrap();
    let patch = || ArticlePatch { title: a.title.clone(), slug: a.slug.clone(), excerpt: a.excerpt.clone(), content_html: a.content_html.clone(), meta_title: a.meta_title.clone(), meta_description: a.meta_description.clone(), faq: a.faq.clone() };
    assert_eq!(articles::update(&cx.state, &cx.plans[0], patch(), a.version).await.unwrap_err().code, "CONFLICT");

    // Campaign completed exactly once and notified.
    assert_eq!(campaigns::get(&cx.state, &cx.campaign_id).await.unwrap().status, "COMPLETED");
    let notes: Vec<_> = cx.events.items.lock().unwrap().iter().filter(|(n, _)| n == "notify").cloned().collect();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].1["kind"], "campaign_done");
}

#[tokio::test]
async fn failed_create_is_retried_and_adopts_the_existing_post() {
    let cx = setup(1, true).await;
    let s = &cx.server;
    Mock::given(method("POST")).and(path("/wp-json/wp/v2/media")).respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": 77, "source_url": "u"}))).mount(s).await;
    Mock::given(method("POST")).and(path("/wp-json/wp/v2/media/77")).respond_with(ResponseTemplate::new(200).set_body_json(json!({}))).mount(s).await;
    // The first create "fails" (e.g. the response was lost) ...
    Mock::given(method("POST")).and(path("/wp-json/wp/v2/posts")).respond_with(ResponseTemplate::new(500)).up_to_n_times(1).mount(s).await;
    // ... but a post with that slug exists on the site by the time we retry.
    Mock::given(method("GET")).and(path("/wp-json/wp/v2/posts")).and(query_param("slug", "slug-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([{"id": 555, "link": "https://x/?p=555", "status": "draft"}]))).mount(s).await;
    Mock::given(method("POST")).and(path("/wp-json/wp/v2/posts")).respond_with(ResponseTemplate::new(201).set_body_json(json!({"id": 999, "link": "l", "status": "draft"}))).mount(s).await;

    upload::enqueue_upload(&cx.state, &cx.plans[0]).await.unwrap();
    drain(&cx).await;

    let (wp_id,): (i64,) = sqlx::query_as("SELECT wordpress_post_id FROM wordpress_posts WHERE plan_id=?").bind(&cx.plans[0]).fetch_one(&cx.state.db).await.unwrap();
    assert_eq!(wp_id, 555, "adopted instead of creating a second post");
    assert_eq!(reqs(s, "POST", "/wp-json/wp/v2/posts").await.len(), 1, "only the failed attempt hit create");
    assert_eq!(reqs(s, "POST", "/wp-json/wp/v2/media").await.len(), 1, "media is not uploaded twice");
    let (retries,): (i64,) = sqlx::query_as("SELECT retry_count FROM jobs WHERE type='upload_wp'").fetch_one(&cx.state.db).await.unwrap();
    assert_eq!(retries, 1);
}

#[tokio::test]
async fn auth_failure_marks_failed_and_can_be_retried() {
    let cx = setup(1, false).await;
    Mock::given(method("POST")).and(path("/wp-json/wp/v2/posts")).respond_with(ResponseTemplate::new(401).set_body_json(json!({"message": "no"}))).mount(&cx.server).await;
    upload::enqueue_upload(&cx.state, &cx.plans[0]).await.unwrap();
    drain(&cx).await;
    let rows = generation::list_articles(&cx.state, &cx.campaign_id).await.unwrap();
    assert_eq!((rows[0].status.as_str(), rows[0].failed_stage.as_deref()), ("FAILED", Some("upload")));
    assert!(rows[0].last_error.as_deref().unwrap().contains("Application Password"));
    assert_eq!(rows[0].upload_status.as_deref(), Some("FAILED"));
    assert!(!rows[0].last_error.as_deref().unwrap().contains("app pass-word"));
    // retry goes back through the queue
    generation::retry_article(&cx.state, &cx.plans[0], "upload").await.unwrap();
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE type='upload_wp' AND status IN ('PENDING','RETRY_PENDING')").fetch_one(&cx.state.db).await.unwrap();
    assert_eq!(n, 1);
    // failures raise the "errors" notification
    drain(&cx).await;
    assert!(cx.events.items.lock().unwrap().iter().any(|(n, v)| n == "notify" && v["kind"] == "errors"));
}

#[tokio::test]
async fn campaign_upload_gate_default_category_and_logs() {
    let cx = setup(2, false).await;
    mount_wp(&cx.server, 201).await;
    assert_eq!(upload::enqueue_campaign(&cx.state, &cx.campaign_id, false, false).await.unwrap_err().code, "VALIDATION");
    assert_eq!(upload::set_default_category(&cx.state, &cx.project_id, Some(3)).await.unwrap_err().code, "VALIDATION", "must be a synced category");
    sqlx::query("INSERT INTO wp_terms (id, project_id, kind, wp_id, name) VALUES ('c3', ?, 'category', 3, 'اخبار')").bind(&cx.project_id).execute(&cx.state.db).await.unwrap();
    upload::set_default_category(&cx.state, &cx.project_id, Some(3)).await.unwrap();
    assert_eq!(projects::get(&cx.state, &cx.project_id).await.unwrap().default_category_wp_id, Some(3));

    assert_eq!(upload::enqueue_campaign(&cx.state, &cx.campaign_id, true, false).await.unwrap(), 2);
    drain(&cx).await;
    let bodies: Vec<Value> = reqs(&cx.server, "POST", "/wp-json/wp/v2/posts").await.iter().map(|r| serde_json::from_slice(&r.body).unwrap()).collect();
    assert_eq!(bodies.len(), 2);
    assert!(bodies.iter().all(|b| b["categories"] == json!([3]) && b.get("featured_media").is_none()), "default category used, no image → no media");
    assert_eq!(reqs(&cx.server, "POST", "/wp-json/wp/v2/media").await.len(), 0);

    // Logs: filter, article filter, export.
    let all = logs::list(&cx.state, &LogQuery::default()).await.unwrap();
    assert!(all.total >= 2);
    let q = LogQuery { action: Some("wordpress_draft".into()), article_no: Some(2), ..Default::default() };
    let one = logs::list(&cx.state, &q).await.unwrap();
    assert_eq!(one.total, 1);
    assert_eq!(one.items[0].article_no, Some(2));
    assert_eq!(one.items[0].project_name.as_deref(), Some("سایت من"));
    assert!(logs::actions(&cx.state).await.unwrap().contains(&"wordpress_draft".to_string()));
    let none = logs::list(&cx.state, &LogQuery { search: Some("no-such-text".into()), ..Default::default() }).await.unwrap();
    assert_eq!(none.total, 0);
    let file = logs::export(&cx.state, &LogQuery { campaign_id: Some(cx.campaign_id.clone()), ..Default::default() }).await.unwrap();
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(text.starts_with('\u{feff}') && text.contains("wordpress_draft") && text.lines().count() >= 3, "{text}");
}

#[tokio::test]
async fn taxonomy_validation_and_editor() {
    let cx = setup(1, false).await;
    let too_many: Vec<String> = (0..21).map(|i| format!("t{i}")).collect();
    assert_eq!(upload::set_taxonomy(&cx.state, &cx.plans[0], vec![], too_many).await.unwrap_err().code, "VALIDATION");
    upload::set_taxonomy(&cx.state, &cx.plans[0], vec![2, 2, 1], vec!["الف".into()]).await.unwrap();
    let rows = generation::list_articles(&cx.state, &cx.campaign_id).await.unwrap();
    assert_eq!((rows[0].category_ids.clone(), rows[0].tags.clone()), (vec![1, 2], vec!["الف".to_string()]));

    let a = generation::get_article(&cx.state, &cx.plans[0]).await.unwrap();
    let mk = |content: &str| ArticlePatch {
        title: "عنوان ویرایش‌شده".into(), slug: "  اسلاگ نو! ".into(), excerpt: "<b>خلاصه</b>".into(), content_html: content.into(),
        meta_title: "m".into(), meta_description: "d".into(),
        faq: vec![FaqItem { q: "<i>پرسش؟</i>".into(), a: "پاسخ".into() }, FaqItem { q: "".into(), a: "x".into() }],
    };
    let updated = articles::update(&cx.state, &cx.plans[0], mk("<h2>الف</h2><p>سلام <script>x()</script><b onclick=\"y()\">دنیا</b></p>"), a.version).await.unwrap();
    assert!(!updated.content_html.contains("script") && !updated.content_html.contains("onclick") && updated.content_html.contains("<b>دنیا</b>"));
    assert_eq!((updated.slug.as_str(), updated.excerpt.as_str(), updated.version), ("کلمه-1", "خلاصه", a.version + 1));
    assert_eq!(updated.faq.len(), 1);
    assert_eq!(updated.faq[0].q, "پرسش؟");
    assert!(updated.validation["warnings"].as_array().unwrap().len() >= 1, "QC runs as information");
    // stale version and empty content are refused
    assert_eq!(articles::update(&cx.state, &cx.plans[0], mk("<p>x</p>"), a.version).await.unwrap_err().code, "CONFLICT");
    assert_eq!(articles::update(&cx.state, &cx.plans[0], mk("<script>x</script>"), updated.version).await.unwrap_err().code, "VALIDATION");
    assert!(!articles::preview_html("<img src=x onerror=1><p>ok</p>").contains("onerror"));
}

#[tokio::test]
async fn backup_roundtrip_to_a_fresh_install() {
    let cx = setup(2, true).await;
    mount_wp(&cx.server, 201).await;
    upload::enqueue_upload(&cx.state, &cx.plans[0]).await.unwrap();
    drain(&cx).await;
    let file = cx.state.data_dir.join("b.aiproj").display().to_string();
    let r = backup::export(&cx.state, Some(file.clone()), Some(cx.project_id.clone())).await.unwrap();
    assert_eq!((r.projects, r.campaigns, r.articles, r.images), (1, 1, 2, 2));
    // Secrets never leave the credential store.
    let zip_bytes = std::fs::read(&file).unwrap();
    assert!(!zip_bytes.windows(12).any(|w| w == b"app pass-wor"));

    // A different machine: empty database + empty data dir.
    let (fresh, _) = new_state().await;
    let pv = backup::preview(&fresh, &file).await.unwrap();
    assert_eq!((pv.projects.len(), pv.projects[0].campaigns, pv.projects[0].articles, pv.projects[0].exists, pv.templates), (1, 1, 2, false, 2));
    let imp = backup::import(&fresh, &file, "skip").await.unwrap();
    assert_eq!((imp.projects_imported, imp.campaigns, imp.articles, imp.files), (1, 1, 2, 2));
    let rows = generation::list_articles(&fresh, &cx.campaign_id).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].status, "DRAFT_CREATED");
    assert_eq!(rows[0].wordpress_post_id, Some(900));
    assert!(generation::get_article_image(&fresh, &cx.plans[1]).await.unwrap().is_some(), "images restored on disk");
    let conn = wp::get_connection(&fresh, &cx.project_id).await.unwrap().unwrap();
    assert!(!conn.has_secret, "the app password must be entered again on the new machine");

    // Importing again: skip leaves it alone, replace re-imports it.
    let again = backup::import(&fresh, &file, "skip").await.unwrap();
    assert_eq!((again.projects_imported, again.projects_skipped.len(), again.articles), (0, 1, 0));
    assert!(backup::preview(&fresh, &file).await.unwrap().projects[0].exists);
    let rep = backup::import(&fresh, &file, "replace").await.unwrap();
    assert_eq!((rep.projects_imported, rep.articles), (1, 2));
    assert_eq!(generation::list_articles(&fresh, &cx.campaign_id).await.unwrap().len(), 2);

    // Garbage and unknown modes are rejected.
    let bad = cx.state.data_dir.join("bad.aiproj");
    std::fs::write(&bad, b"not a zip").unwrap();
    assert_eq!(backup::preview(&fresh, &bad.display().to_string()).await.err().unwrap().code, "VALIDATION");
    assert_eq!(backup::import(&fresh, &file, "merge").await.err().unwrap().code, "VALIDATION");
    assert_eq!(backup::export(&cx.state, None, Some("nope".into())).await.err().unwrap().code, "NOT_FOUND");
    // Full export lands in the app's exports folder by default.
    let all = backup::export(&cx.state, None, None).await.unwrap();
    assert!(all.path.contains("exports") && std::path::Path::new(&all.path).exists());
}
