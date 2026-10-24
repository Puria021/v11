//! Phase 4 integration tests: providers (Anthropic / Gemini / image), templates, the job queue,
//! content + image generation, QC retries, backoff, pause / resume / cancel and crash recovery.

use myarticlemanager_lib::events::CollectEvents;
use myarticlemanager_lib::queue::{self, Scheduler};
use myarticlemanager_lib::secrets::MemorySecretStore;
use myarticlemanager_lib::services::ai::{self, SaveAiProvider};
use myarticlemanager_lib::services::campaigns::{self, CampaignView, CreateCampaign};
use myarticlemanager_lib::services::image_providers::{self, SaveImageProvider};
use myarticlemanager_lib::services::projects::{self, ProjectInput};
use myarticlemanager_lib::services::templates::{self, Template};
use myarticlemanager_lib::services::{generation, keywords, secondary, titles};
use myarticlemanager_lib::state::AppState;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

const PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

fn chat(content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"choices":[{"message":{"role":"assistant","content":content}}]}))
}

fn word(i: usize) -> String {
    let letters = ['ا', 'ب', 'پ', 'ت', 'ث', 'ج', 'چ', 'ح', 'خ', 'د', 'ذ', 'ر', 'ز', 'ژ', 'س', 'ش', 'ص', 'ض', 'ط', 'ظ'];
    let mut n = i + 7;
    let mut s = String::new();
    for _ in 0..5 {
        s.push(letters[(n * 7 + s.len() * 3) % letters.len()]);
        s.push(letters[(n / 3 + s.len()) % letters.len()]);
        n = n * 31 + 17;
    }
    s
}

fn hash(s: &str) -> usize {
    s.bytes().fold(7usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize)) % 100_000
}

fn good_article(keyword: &str, same_body: bool) -> Value {
    let seed = if same_body { 1 } else { hash(keyword) };
    let filler = |from: usize, n: usize| (from..from + n).map(|i| word(seed * 13 + i)).collect::<Vec<_>>().join(" ");
    // QC rules: the keyword phrase 27 times, once within the first 150 words, short paragraphs,
    // and two external links (the output is rejected otherwise).
    let mut html = format!("<h1>تیتر</h1><p>در این مقاله درباره {keyword} صحبت می‌کنیم. {}</p><h2>بخش اول</h2>", filler(0, 12));
    for i in 1..27 {
        if i == 13 {
            html.push_str("<h2>بخش دوم</h2>");
        }
        html.push_str(&format!("<p>{keyword} {}</p>", filler(100 + i * 5, 6)));
    }
    html.push_str(r#"<p><a href="https://www.who.int/">منبع</a> و <a href="https://en.wikipedia.org/wiki/Laptop">منبع دوم</a></p><script>alert(1)</script>"#);
    json!({
        "title": "ignored by the app", "slug": "یک اسلاگ", "excerpt": "خلاصه", "content": html,
        "meta_title": "متا", "meta_description": "توضیح متا",
        "faq": [{"q": "پرسش یک؟", "a": "پاسخ یک."}, {"q": "پرسش دو؟", "a": "پاسخ دو."}],
        "image_prompt": "a calm illustration of a laptop on a desk"
    })
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Normal,
    AlwaysShort,
    ShortFirst,
    RateLimitedThenOk,
    SameBody,
}

struct Shared {
    mode: Mutex<Mode>,
    article_calls: AtomicUsize,
    per_keyword: Mutex<std::collections::HashMap<String, usize>>,
    kw_calls: AtomicUsize,
    rate_limit_left: AtomicUsize,
}

struct Fake(Arc<Shared>);

impl Respond for Fake {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let sh = &self.0;
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        let system = body["messages"][0]["content"].as_str().unwrap_or("").to_string();
        let user = body["messages"][1]["content"].as_str().unwrap_or("").to_string();
        if user.contains("single word OK") {
            return chat("OK");
        }
        if system.contains("OUTPUT CONTRACT") {
            sh.article_calls.fetch_add(1, Ordering::SeqCst);
            let kw = user.split("Focus (secondary) keyword: ").nth(1).and_then(|r| r.lines().next()).unwrap_or("").to_string();
            let mode = *sh.mode.lock().unwrap();
            if mode == Mode::RateLimitedThenOk {
                let left = sh.rate_limit_left.load(Ordering::SeqCst);
                if left > 0 {
                    sh.rate_limit_left.fetch_sub(1, Ordering::SeqCst);
                    return ResponseTemplate::new(429).set_body_json(json!({"error": {"message": "slow down"}}));
                }
            }
            let n = {
                let mut m = sh.per_keyword.lock().unwrap();
                let e = m.entry(kw.clone()).or_insert(0);
                *e += 1;
                *e
            };
            let short = json!({"content": format!("<h2>الف</h2><h2>ب</h2><p>{kw}</p>"), "faq": [{"q":"q","a":"a"}]});
            let out = match mode {
                Mode::AlwaysShort => short,
                Mode::ShortFirst if n == 1 => short,
                Mode::SameBody => good_article(&kw, true),
                _ => good_article(&kw, false),
            };
            return chat(&out.to_string());
        }
        if user.contains("Items:") {
            let start = user.find("Items: ").unwrap() + 7;
            let end = user[start..].find("\n\n").unwrap() + start;
            let items: Vec<Value> = serde_json::from_str(&user[start..end]).unwrap();
            let out: Vec<Value> = items
                .iter()
                .map(|it| json!({"no": it["no"], "title": format!("{} چیست؟", it["keyword"].as_str().unwrap())}))
                .collect();
            return chat(&serde_json::to_string(&out).unwrap());
        }
        if user.contains("Titles already tried") {
            let n = sh.kw_calls.fetch_add(1, Ordering::SeqCst);
            return chat(&format!("[\"عنوان جایگزین {n}\"]"));
        }
        let n = sh.kw_calls.fetch_add(1, Ordering::SeqCst);
        let items: Vec<Value> = (n * 10..n * 10 + 10)
            .map(|i| json!({"keyword": format!("موضوع {} درباره لپ تاپ {}", i, word(i)), "intent": "informational"}))
            .collect();
        chat(&serde_json::to_string(&items).unwrap())
    }
}

struct Cx {
    state: AppState,
    events: Arc<CollectEvents>,
    campaign: CampaignView,
    shared: Arc<Shared>,
    sched: Scheduler,
    _ai: MockServer,
    _img: MockServer,
}

fn tpl(image_count: i64) -> Template {
    // Templates are fixed: tests edit the article template instead of creating one.
    let mut t = templates::list_blocking_default();
    t.id = Some(templates::ARTICLE_TEMPLATE_ID.into());
    t.name = "تست".into();
    t.min_words = 50;
    t.max_words = 400;
    t.faq_count = 2;
    t.image_count = image_count;
    t.max_retries = 2;
    t
}

/// Builds a project + campaign (3 articles) with titles approved, but generation not started.
async fn setup(image_count: i64) -> Cx {
    let events = Arc::new(CollectEvents::default());
    let state = AppState::for_tests_with_events(Arc::new(MemorySecretStore::new()), events.clone()).await.unwrap();
    let shared = Arc::new(Shared {
        mode: Mutex::new(Mode::Normal),
        article_calls: AtomicUsize::new(0),
        per_keyword: Mutex::new(Default::default()),
        kw_calls: AtomicUsize::new(0),
        rate_limit_left: AtomicUsize::new(0),
    });
    let ai_srv = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/chat/completions")).and(header("authorization", "Bearer sk-test"))
        .respond_with(Fake(shared.clone())).mount(&ai_srv).await;
    let img_srv = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/images/generations")).and(header("authorization", "Bearer img-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"b64_json": PNG_B64}]})))
        .mount(&img_srv).await;

    let provider = ai::save(&state, SaveAiProvider {
        id: None, name: "Fake".into(), kind: "openai_compatible".into(), base_url: Some(format!("{}/v1", ai_srv.uri())),
        model: "m".into(), temperature: None, max_tokens: None, timeout_sec: Some(20), proxy_url: None,
        rpm_limit: None, max_concurrent: None, api_key: Some("sk-test".into()),
    }).await.unwrap();
    let img = image_providers::save(&state, SaveImageProvider {
        id: None, name: "Img".into(), kind: "openai_image".into(), base_url: Some(format!("{}/v1", img_srv.uri())),
        model: "gpt-image-1".into(), timeout_sec: Some(30), proxy_url: None, rpm_limit: None, max_concurrent: None,
        output_format: None, image_quality: None, preferred_size: None, api_key: Some("img-key".into()),
    }).await.unwrap();
    let p = projects::create(&state, ProjectInput { name: "پروژه".into(), site_url: "example.com".into(), ..Default::default() }).await.unwrap();
    let t = templates::save(&state, tpl(image_count)).await.unwrap();
    let c = campaigns::create(&state, CreateCampaign {
        project_id: p.id, name: "کمپین".into(), target_count: Some(3), country: None, language: None, ai_provider_id: Some(provider.id),
    }).await.unwrap();
    campaigns::set_template(&state, &c.id, t.id.clone()).await.unwrap();
    if image_count > 0 {
        campaigns::set_image_provider(&state, &c.id, Some(img.id)).await.unwrap();
    }
    keywords::save_primary(&state, &c.id, vec!["لپ تاپ".into()]).await.unwrap();
    secondary::generate(&state, &c.id, true).await.unwrap();
    secondary::confirm(&state, &c.id).await.unwrap();
    titles::generate(&state, &c.id).await.unwrap();
    titles::approve_all(&state, &c.id).await.unwrap();
    let campaign = campaigns::get(&state, &c.id).await.unwrap();
    Cx { state, events, campaign, shared, sched: Scheduler::new(), _ai: ai_srv, _img: img_srv }
}

async fn run_all(cx: &Cx) {
    generation::start(&cx.state, &cx.campaign.id, true).await.unwrap();
    cx.sched.drain(&cx.state, Duration::from_secs(30)).await.unwrap();
}

async fn statuses(cx: &Cx) -> Vec<String> {
    generation::list_articles(&cx.state, &cx.campaign.id).await.unwrap().into_iter().map(|a| a.status).collect()
}

#[tokio::test]
async fn full_pipeline_content_and_images() {
    let cx = setup(1).await;
    run_all(&cx).await;
    assert_eq!(statuses(&cx).await, vec!["READY_TO_UPLOAD"; 3]);

    let p = generation::progress(&cx.state, &cx.campaign.id).await.unwrap();
    assert_eq!((p.content.done, p.images.done, p.failed, p.percent), (3, 3, 0, 100));
    assert!(p.titles.done == 3 && p.images_enabled);

    let arts = generation::list_articles(&cx.state, &cx.campaign.id).await.unwrap();
    let a = generation::get_article(&cx.state, &arts[0].plan_id).await.unwrap();
    assert_eq!(a.title, arts[0].title.clone().unwrap(), "the approved title is authoritative");
    assert!(!a.content_html.contains("<script") && !a.content_html.contains("<h1"), "{}", a.content_html);
    assert!(a.content_html.contains("<h2>") && a.faq.len() == 2 && a.word_count > 50);
    assert!(!a.slug.is_empty() && !a.meta_title.is_empty());

    let data_url = generation::get_article_image(&cx.state, &arts[0].plan_id).await.unwrap().unwrap();
    // Generated images are normalized to WebP before persistence and preview.
    assert!(data_url.starts_with("data:image/webp;base64,"));
    let (rel, mime): (String, String) = sqlx::query_as(
        "SELECT local_path, mime_type FROM article_images WHERE plan_id=? AND is_featured=1 LIMIT 1"
    ).bind(&arts[0].plan_id).fetch_one(&cx.state.db).await.unwrap();
    assert_eq!(mime, "image/webp");
    assert!(rel.ends_with(".webp"));
    let saved = std::fs::read(cx.state.data_dir.join(&rel)).unwrap();
    assert!(saved.starts_with(b"RIFF") && saved.get(8..12) == Some(b"WEBP"), "expected an actual WebP file");

    let names = cx.events.names();
    assert!(names.iter().any(|n| n == "campaign:progress") && names.iter().any(|n| n == "plan:status") && names.iter().any(|n| n == "job:updated"));
}

#[tokio::test]
async fn text_only_when_template_has_no_image() {
    let cx = setup(0).await;
    run_all(&cx).await;
    assert_eq!(statuses(&cx).await, vec!["READY_TO_UPLOAD"; 3]);
    let p = generation::progress(&cx.state, &cx.campaign.id).await.unwrap();
    assert!(!p.images_enabled && p.images.total == 0 && p.percent == 100);
}

#[tokio::test]
async fn start_is_gated() {
    let cx = setup(1).await;
    assert_eq!(generation::start(&cx.state, &cx.campaign.id, false).await.unwrap_err().code, "VALIDATION");
    // un-approve one title
    let l = titles::list(&cx.state, &cx.campaign.id).await.unwrap();
    titles::reject(&cx.state, &l.plans[0].plan_id, l.plans[0].title_id.as_ref().unwrap()).await.unwrap();
    let e = generation::start(&cx.state, &cx.campaign.id, true).await.unwrap_err();
    assert_eq!(e.code, "VALIDATION");
    assert!(e.message.contains('۱') || e.message.contains('1'));
    titles::generate(&cx.state, &cx.campaign.id).await.unwrap();
    titles::approve_all(&cx.state, &cx.campaign.id).await.unwrap();

    // Template and image provider are fixed: a missing one is filled in automatically on start.
    // The fixed image provider exists once the license is activated (created by the app).
    sqlx::query("INSERT INTO image_providers (id, name, kind, base_url, model, output_format, image_quality, preferred_size, timeout_sec, secret_ref, max_concurrent)
                 VALUES ('builtin-image', 'تصویر', 'openai_image', 'https://tooken.club/v1', 'gpt-image-2.5', 'png', 'low', 'template', 180, 'image-key:builtin-image', 2)")
        .execute(&cx.state.db).await.unwrap();
    campaigns::set_template(&cx.state, &cx.campaign.id, None).await.unwrap();
    campaigns::set_image_provider(&cx.state, &cx.campaign.id, None).await.unwrap();
    assert_eq!(generation::start(&cx.state, &cx.campaign.id, true).await.unwrap().queued, 3);
    let c = campaigns::get(&cx.state, &cx.campaign.id).await.unwrap();
    assert_eq!(c.template_id.as_deref(), Some(templates::ARTICLE_TEMPLATE_ID));
    assert_eq!(c.image_provider_id.as_deref(), Some("builtin-image"));
}

#[tokio::test]
async fn qc_failure_is_retried_with_feedback_then_succeeds() {
    let cx = setup(0).await;
    *cx.shared.mode.lock().unwrap() = Mode::ShortFirst;
    run_all(&cx).await;
    assert_eq!(statuses(&cx).await, vec!["READY_TO_UPLOAD"; 3]);
    assert_eq!(cx.shared.article_calls.load(Ordering::SeqCst), 6, "each article needed exactly one QC retry");
}

#[tokio::test]
async fn persistent_qc_failure_marks_failed_and_can_be_retried() {
    let cx = setup(0).await;
    *cx.shared.mode.lock().unwrap() = Mode::AlwaysShort;
    run_all(&cx).await;
    assert_eq!(statuses(&cx).await, vec!["FAILED"; 3]);
    assert_eq!(cx.shared.article_calls.load(Ordering::SeqCst), 9, "1 try + 2 retries per article");
    let a = generation::list_articles(&cx.state, &cx.campaign.id).await.unwrap();
    assert_eq!(a[0].failed_stage.as_deref(), Some("content"));
    assert!(a[0].last_error.as_deref().unwrap().contains("کنترل کیفیت"));
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM article_contents").fetch_one(&cx.state.db).await.unwrap();
    assert_eq!(n, 0, "rejected output is never stored");
    assert_eq!(generation::progress(&cx.state, &cx.campaign.id).await.unwrap().failed, 3);

    *cx.shared.mode.lock().unwrap() = Mode::Normal;
    assert_eq!(generation::retry_failed(&cx.state, &cx.campaign.id).await.unwrap(), 3);
    cx.sched.drain(&cx.state, Duration::from_secs(30)).await.unwrap();
    assert_eq!(statuses(&cx).await, vec!["READY_TO_UPLOAD"; 3]);
}

#[tokio::test]
async fn duplicate_articles_are_rejected() {
    let cx = setup(0).await;
    queue_one_at_a_time(&cx).await;
    *cx.shared.mode.lock().unwrap() = Mode::SameBody;
    run_all(&cx).await;
    let s = statuses(&cx).await;
    assert_eq!(s.iter().filter(|x| *x == "READY_TO_UPLOAD").count(), 1, "{s:?}");
    assert_eq!(s.iter().filter(|x| *x == "FAILED").count(), 2);
    let a = generation::list_articles(&cx.state, &cx.campaign.id).await.unwrap();
    assert!(a.iter().any(|x| x.last_error.as_deref().map(|e| e.contains("تکراری")).unwrap_or(false)));
}

async fn queue_one_at_a_time(cx: &Cx) {
    generation::update_queue_settings(&cx.state, 1, 1, 1).await.unwrap();
}

#[tokio::test]
async fn rate_limit_triggers_backoff_and_recovers() {
    let cx = setup(0).await;
    queue_one_at_a_time(&cx).await;
    *cx.shared.mode.lock().unwrap() = Mode::RateLimitedThenOk;
    // 3 consecutive 429s exhaust the in-request retries (1 + 2), forcing a job-level RETRY_PENDING.
    cx.shared.rate_limit_left.store(3, Ordering::SeqCst);
    run_all(&cx).await;
    assert_eq!(statuses(&cx).await, vec!["READY_TO_UPLOAD"; 3]);
    let (retried,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE retry_count >= 1 AND status='DONE'").fetch_one(&cx.state.db).await.unwrap();
    assert_eq!(retried, 1, "exactly one job needed a backoff retry");
}

#[tokio::test]
async fn pause_resume_and_cancel_restart() {
    let cx = setup(0).await;
    let id = cx.campaign.id.clone();
    generation::start(&cx.state, &id, true).await.unwrap();
    generation::pause(&cx.state, &id).await.unwrap();
    cx.sched.drain(&cx.state, Duration::from_secs(5)).await.unwrap();
    assert_eq!(cx.shared.article_calls.load(Ordering::SeqCst), 0, "paused campaigns start no new jobs");
    assert_eq!(generation::pause(&cx.state, &id).await.unwrap_err().code, "CONFLICT");

    generation::resume(&cx.state, &id).await.unwrap();
    cx.sched.drain(&cx.state, Duration::from_secs(30)).await.unwrap();
    assert_eq!(statuses(&cx).await, vec!["READY_TO_UPLOAD"; 3]);

    // Cancel before anything runs, then restart.
    let cx2 = setup(0).await;
    let id2 = cx2.campaign.id.clone();
    generation::start(&cx2.state, &id2, true).await.unwrap();
    generation::cancel(&cx2.state, &id2).await.unwrap();
    cx2.sched.drain(&cx2.state, Duration::from_secs(5)).await.unwrap();
    assert_eq!(statuses(&cx2).await, vec!["CANCELLED"; 3]);
    assert_eq!(cx2.shared.article_calls.load(Ordering::SeqCst), 0);
    let (cancelled,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE status='CANCELLED'").fetch_one(&cx2.state.db).await.unwrap();
    assert_eq!(cancelled, 3);

    assert_eq!(generation::start(&cx2.state, &id2, true).await.unwrap().queued, 3);
    cx2.sched.drain(&cx2.state, Duration::from_secs(30)).await.unwrap();
    assert_eq!(statuses(&cx2).await, vec!["READY_TO_UPLOAD"; 3]);
}

#[tokio::test]
async fn crash_recovery_requeues_interrupted_jobs() {
    let cx = setup(0).await;
    generation::start(&cx.state, &cx.campaign.id, true).await.unwrap();
    // Simulate the app dying while one article was being generated.
    sqlx::query("UPDATE jobs SET status='PROCESSING' WHERE id=(SELECT MIN(id) FROM jobs)").execute(&cx.state.db).await.unwrap();
    sqlx::query("UPDATE article_plans SET status='GENERATING' WHERE id=(SELECT plan_id FROM jobs WHERE status='PROCESSING')").execute(&cx.state.db).await.unwrap();

    assert_eq!(queue::recover(&cx.state).await.unwrap(), 1);
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs WHERE status='RETRY_PENDING'").fetch_one(&cx.state.db).await.unwrap();
    assert_eq!(n, 1);
    assert!(!statuses(&cx).await.contains(&"GENERATING".to_string()));
    cx.sched.drain(&cx.state, Duration::from_secs(30)).await.unwrap();
    assert_eq!(statuses(&cx).await, vec!["READY_TO_UPLOAD"; 3]);
}

#[tokio::test]
async fn enqueue_is_idempotent_per_plan_and_stage() {
    let cx = setup(0).await;
    generation::start(&cx.state, &cx.campaign.id, true).await.unwrap();
    assert_eq!(generation::start(&cx.state, &cx.campaign.id, true).await.unwrap().queued, 0, "no duplicate jobs");
    let l = generation::list_articles(&cx.state, &cx.campaign.id).await.unwrap();
    assert_eq!(generation::retry_article(&cx.state, &l[0].plan_id, "content").await.unwrap_err().code, "CONFLICT");
    assert_eq!(generation::retry_article(&cx.state, &l[0].plan_id, "image").await.unwrap_err().code, "VALIDATION");
    let (n,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobs").fetch_one(&cx.state.db).await.unwrap();
    assert_eq!(n, 3);
}

#[tokio::test]
async fn template_validation_and_duplicate() {
    let cx = setup(0).await;
    let mut bad = tpl(0);
    bad.min_words = 10;
    assert_eq!(templates::save(&cx.state, bad).await.unwrap_err().code, "VALIDATION");
    let mut bad2 = tpl(0);
    bad2.max_words = 10;
    assert_eq!(templates::save(&cx.state, bad2).await.unwrap_err().code, "VALIDATION");
    // Fixed templates: no creating, copying or deleting.
    let mut new_one = tpl(0);
    new_one.id = None;
    assert_eq!(templates::save(&cx.state, new_one).await.unwrap_err().code, "VALIDATION");
    assert_eq!(templates::duplicate(&cx.state, templates::ARTICLE_TEMPLATE_ID).await.unwrap_err().code, "VALIDATION");
    assert_eq!(templates::delete(&cx.state, templates::PRODUCT_TEMPLATE_ID).await.unwrap_err().code, "VALIDATION");
    assert_eq!(templates::list(&cx.state).await.unwrap().len(), 2);
    // The preview shows the rendered prompt for a plan.
    let plan = generation::list_articles(&cx.state, &cx.campaign.id).await.unwrap();
    let pv = generation::preview_prompt(&cx.state, &plan[0].plan_id).await.unwrap();
    assert!(pv["user"].as_str().unwrap().contains(&plan[0].keyword));
    assert!(pv["system"].as_str().unwrap().contains("OUTPUT CONTRACT"));
}

#[tokio::test]
async fn anthropic_and_gemini_adapters() {
    let state = AppState::for_tests(Arc::new(MemorySecretStore::new())).await.unwrap();
    let a = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/messages")).and(header("x-api-key", "ant-key")).and(header("anthropic-version", "2023-06-01"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"content": [{"type": "text", "text": "OK"}]})))
        .expect(1).mount(&a).await;
    let g = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1beta/models/gem-model:generateContent")).and(header("x-goog-api-key", "gem-key"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"candidates": [{"content": {"parts": [{"text": "OK"}]}}]})))
        .expect(1).mount(&g).await;

    let mk = |name: &str, kind: &str, base: String, model: &str, key: &str| SaveAiProvider {
        id: None, name: name.into(), kind: kind.into(), base_url: Some(base), model: model.into(), temperature: Some(1.8),
        max_tokens: None, timeout_sec: Some(20), proxy_url: None, rpm_limit: Some(600), max_concurrent: Some(2), api_key: Some(key.into()),
    };
    let pa = ai::save(&state, mk("A", "anthropic", a.uri(), "claude-x", "ant-key")).await.unwrap();
    let pg = ai::save(&state, mk("G", "gemini", g.uri(), "models/gem-model", "gem-key")).await.unwrap();
    assert!(ai::test(&state, &pa.id).await.unwrap().ok);
    assert!(ai::test(&state, &pg.id).await.unwrap().ok);
    let body: Value = serde_json::from_slice(&a.received_requests().await.unwrap()[0].body).unwrap();
    assert!(body["temperature"].as_f64().unwrap() <= 1.0, "Anthropic temperature is clamped to 1");
}

#[tokio::test]
async fn image_provider_handles_url_responses_and_rejects_non_images() {
    let state = AppState::for_tests(Arc::new(MemorySecretStore::new())).await.unwrap();
    let s = MockServer::start().await;
    let png = base64_decode(PNG_B64);
    Mock::given(method("GET")).and(path("/img.png")).respond_with(ResponseTemplate::new(200).set_body_bytes(png.clone())).mount(&s).await;
    Mock::given(method("POST")).and(path("/v1/images/generations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"url": format!("{}/img.png", s.uri())}]})))
        .mount(&s).await;
    let p = image_providers::save(&state, SaveImageProvider {
        id: None, name: "I".into(), kind: "openai_image".into(), base_url: Some(format!("{}/v1", s.uri())), model: "m".into(),
        timeout_sec: Some(20), proxy_url: None, rpm_limit: None, max_concurrent: None, output_format: None, image_quality: None, preferred_size: None, api_key: Some("k".into()),
    }).await.unwrap();
    let img = image_providers::generate(&state, &p.id, "x", "1024x1024").await.unwrap();
    assert_eq!((img.mime.as_str(), img.extension()), ("image/png", "png"));

    let bad = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/images/generations"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [{"b64_json": "aGVsbG8gd29ybGQ="}]}))).mount(&bad).await;
    let p2 = image_providers::save(&state, SaveImageProvider {
        id: None, name: "J".into(), kind: "openai_image".into(), base_url: Some(format!("{}/v1", bad.uri())), model: "m".into(),
        timeout_sec: Some(20), proxy_url: None, rpm_limit: None, max_concurrent: None, output_format: None, image_quality: None, preferred_size: None, api_key: Some("k".into()),
    }).await.unwrap();
    assert_eq!(image_providers::generate(&state, &p2.id, "x", "1024x1024").await.err().unwrap().code, "INVALID_RESPONSE");
}

fn base64_decode(s: &str) -> Vec<u8> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s).unwrap()
}
