//! Phase 3 integration tests: campaigns, keywords (CSV / AI), 60 secondary keywords, plans, titles.
//! The AI service is a wiremock server speaking the OpenAI chat-completions format.

use myarticlemanager_lib::secrets::MemorySecretStore;
use myarticlemanager_lib::services::ai::{self, SaveAiProvider};
use myarticlemanager_lib::services::campaigns::{self, CampaignView, CreateCampaign};
use myarticlemanager_lib::services::keywords::{self, KeywordQuery};
use myarticlemanager_lib::services::projects::{self, ProjectInput};
use myarticlemanager_lib::services::{secondary, titles};
use myarticlemanager_lib::state::AppState;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

fn chat(content: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(json!({"choices":[{"message":{"role":"assistant","content":content}}]}))
}

/// Replies by looking at the prompt, so one server can play every AI role in the pipeline.
struct Scripted {
    keyword_calls: Arc<AtomicUsize>,
    /// How many unique keywords each "secondary" call returns (to test top-up rounds).
    per_call: usize,
    /// Cap on total unique keywords the fake can ever produce (to test the shortfall path).
    cap: usize,
}

impl Respond for Scripted {
    fn respond(&self, req: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&req.body).unwrap();
        let prompt = body["messages"][1]["content"].as_str().unwrap_or("").to_string();
        if prompt.contains("connectivity") || prompt.contains("single word OK") {
            return chat("OK");
        }
        if prompt.contains("Items:") {
            // titles batch: echo one title per item
            let start = prompt.find("Items: ").unwrap() + 7;
            let end = prompt[start..].find("\n\n").unwrap() + start;
            let items: Vec<Value> = serde_json::from_str(&prompt[start..end]).unwrap();
            let out: Vec<Value> = items
                .iter()
                .map(|it| json!({"no": it["no"], "title": format!("{} چیست و چه مزایایی دارد؟", it["keyword"].as_str().unwrap())}))
                .collect();
            return chat(&format!("```json\n{}\n```", serde_json::to_string(&out).unwrap()));
        }
        if prompt.contains("Titles already tried") {
            let n = self.keyword_calls.fetch_add(1, Ordering::SeqCst);
            // Titles must contain the keyword phrase (server-side rule).
            let kw = prompt.lines().find_map(|l| l.strip_prefix("Keyword: ")).unwrap_or("").trim().to_string();
            return chat(&format!("[\"عنوان جایگزین شماره {} درباره {}\"]", n + 1, kw));
        }
        // secondary / suggest keywords
        let n = self.keyword_calls.fetch_add(1, Ordering::SeqCst);
        let start = n * self.per_call;
        let intents = ["informational", "commercial", "transactional"];
        let items: Vec<Value> = (start..start + self.per_call)
            .filter(|i| *i < self.cap)
            .map(|i| {
                let intent = intents[i % 3];
                json!({"keyword": format!("موضوع شماره {} درباره لپ تاپ {}", i, word(i)), "intent": intent})
            })
            .collect();
        chat(&serde_json::to_string(&items).unwrap())
    }
}

/// Distinct pseudo-words so generated keywords are not "similar" to each other.
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

struct Ctx {
    state: AppState,
    campaign: CampaignView,
    server: MockServer,
}

async fn setup(per_call: usize, cap: usize) -> Ctx {
    let state = AppState::for_tests(Arc::new(MemorySecretStore::new())).await.unwrap();
    // These tests exercise 60-article campaigns; lift the license quota for them.
    sqlx::query("UPDATE quota_usage SET article_total=1000").execute(&state.db).await.unwrap();
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Bearer sk-test"))
        .respond_with(Scripted { keyword_calls: Arc::new(AtomicUsize::new(0)), per_call, cap })
        .mount(&server)
        .await;
    let provider = ai::save(
        &state,
        SaveAiProvider {
            id: None,
            name: "Fake".into(),
            kind: "openai_compatible".into(),
            base_url: Some(format!("{}/v1", server.uri())),
            model: "m".into(),
            temperature: None,
            max_tokens: None,
            timeout_sec: Some(20),
            proxy_url: None,
            rpm_limit: None,
            max_concurrent: None,
            api_key: Some("sk-test".into()),
        },
    )
    .await
    .unwrap();
    let p = projects::create(&state, ProjectInput { name: "پروژه".into(), site_url: "example.com".into(), ..Default::default() })
        .await
        .unwrap();
    let campaign = campaigns::create(
        &state,
        CreateCampaign {
            project_id: p.id,
            name: "لپ تاپ دانشجویی".into(),
            target_count: None,
            country: None,
            language: None,
            ai_provider_id: Some(provider.id),
        },
    )
    .await
    .unwrap();
    Ctx { state, campaign, server }
}

fn kq(campaign_id: &str, role: &str) -> KeywordQuery {
    KeywordQuery {
        campaign_id: campaign_id.into(),
        role: Some(role.into()),
        search: None,
        source: None,
        selected_only: None,
        sort_by: None,
        sort_dir: None,
        page: None,
        page_size: Some(500),
    }
}

#[tokio::test]
async fn ai_provider_roundtrip_hides_key_and_validates() {
    let cx = setup(10, 100).await;
    let list = ai::list(&cx.state).await.unwrap();
    assert_eq!(list.len(), 1);
    assert!(list[0].has_secret);
    assert!(!serde_json::to_string(&list).unwrap().contains("sk-test"));
    assert!(ai::test(&cx.state, &list[0].id).await.unwrap().ok);

    let bad = ai::save(&cx.state, SaveAiProvider { id: None, name: "x".into(), kind: "bogus".into(), base_url: None, model: "m".into(), temperature: None, max_tokens: None, timeout_sec: None, proxy_url: None, rpm_limit: None, max_concurrent: None, api_key: Some("k".into()) }).await;
    assert_eq!(bad.unwrap_err().code, "VALIDATION");
}

#[tokio::test]
async fn ai_auth_failure_is_reported_without_leaking_key() {
    let state = AppState::for_tests(Arc::new(MemorySecretStore::new())).await.unwrap();
    let server = MockServer::start().await;
    Mock::given(method("POST")).respond_with(ResponseTemplate::new(401).set_body_json(json!({"error":{"message":"bad key sk-secret-123"}}))).mount(&server).await;
    let p = ai::save(&state, SaveAiProvider { id: None, name: "x".into(), kind: "openai_compatible".into(), base_url: Some(server.uri()), model: "m".into(), temperature: None, max_tokens: None, timeout_sec: None, proxy_url: None, rpm_limit: None, max_concurrent: None, api_key: Some("sk-secret-123".into()) }).await.unwrap();
    let err = ai::test(&state, &p.id).await.unwrap_err();
    assert_eq!(err.code, "AUTH_INVALID");
    assert!(!err.message.contains("sk-secret-123"));
}

#[tokio::test]
async fn csv_import_keeps_real_metrics_and_ai_has_none() {
    let cx = setup(10, 100).await;
    let id = &cx.campaign.id;
    keywords::save_primary(&cx.state, id, vec!["لپ تاپ دانشجویی".into(), " لپ‌تاپ دانشجویی ".into()]).await.unwrap();
    assert_eq!(keywords::primaries(&cx.state, id).await.unwrap().len(), 1, "duplicates collapse");

    let r = keywords::import_keywords_csv_for_test(&cx.state, id, "keyword,search_volume,competition,cpc\nخرید لپ تاپ,\"1,900\",0.5,0.2\nبهترین لپ تاپ,2400,0.7,0.3\n").await;
    assert_eq!((r.imported, r.updated), (2, 0));
    let r2 = keywords::import_keywords_csv_for_test(&cx.state, id, "keyword,search_volume\nخرید لپ‌تاپ,2000\n").await;
    assert_eq!((r2.imported, r2.updated), (0, 1), "same normalized text updates instead of duplicating");

    let s = keywords::ai_suggest(&cx.state, id, 10).await.unwrap();
    assert_eq!(s.added, 10);
    let page = keywords::list(&cx.state, kq(id, "candidate")).await.unwrap();
    assert_eq!(page.total, 12);
    for k in page.items.iter().filter(|k| k.source == "ai") {
        assert!(k.search_volume.is_none() && k.competition.is_none() && k.cpc.is_none());
    }
    let real = page.items.iter().find(|k| k.keyword == "خرید لپ تاپ").unwrap();
    assert_eq!(real.search_volume, Some(2000));
    // Real data sorts first when ordering by volume.
    let mut q = kq(id, "candidate");
    q.sort_by = Some("searchVolume".into());
    q.sort_dir = Some("desc".into());
    let sorted = keywords::list(&cx.state, q).await.unwrap();
    assert_eq!(sorted.items[0].search_volume, Some(2400));
}

#[tokio::test]
async fn generates_exactly_60_unique_secondary_keywords_then_plans() {
    let cx = setup(25, 500).await; // 25 per call → needs several top-up rounds
    let id = &cx.campaign.id;
    keywords::save_primary(&cx.state, id, vec!["لپ تاپ دانشجویی".into()]).await.unwrap();

    let r = secondary::generate(&cx.state, id, true).await.unwrap();
    assert_eq!((r.total, r.missing), (60, 0));
    assert!(r.rounds >= 2);
    let sec = keywords::list(&cx.state, kq(id, "secondary")).await.unwrap();
    assert_eq!(sec.total, 60);
    let uniq: std::collections::HashSet<_> = sec.items.iter().map(|k| k.keyword.clone()).collect();
    assert_eq!(uniq.len(), 60);
    assert!(sec.items.iter().all(|k| k.source == "ai" && k.search_volume.is_none()));

    assert_eq!(secondary::confirm(&cx.state, id).await.unwrap(), 60);
    assert_eq!(secondary::confirm(&cx.state, id).await.unwrap_err().code, "CONFLICT");
    assert_eq!(secondary::generate(&cx.state, id, true).await.unwrap_err().code, "CONFLICT");
    assert_eq!(campaigns::get(&cx.state, id).await.unwrap().plan_count, 60);

    // Plans block deleting their keywords; reset frees them.
    let ids = vec![sec.items[0].id.clone()];
    assert_eq!(keywords::delete(&cx.state, id, &ids).await.unwrap_err().code, "CONFLICT");
    assert_eq!(secondary::reset_plans(&cx.state, id, false).await.unwrap_err().code, "VALIDATION");
    secondary::reset_plans(&cx.state, id, true).await.unwrap();
    assert_eq!(keywords::delete(&cx.state, id, &ids).await.unwrap(), 1);
    let _ = &cx.server;
}

#[tokio::test]
async fn shortfall_is_reported_never_padded() {
    let cx = setup(20, 35).await; // the fake can only ever produce 35
    let id = &cx.campaign.id;
    keywords::save_primary(&cx.state, id, vec!["لپ تاپ".into()]).await.unwrap();
    let r = secondary::generate(&cx.state, id, true).await.unwrap();
    assert_eq!(r.total, 35);
    assert_eq!(r.missing, 25);
    assert_eq!(secondary::confirm(&cx.state, id).await.unwrap_err().code, "VALIDATION");
    assert_eq!(campaigns::get(&cx.state, id).await.unwrap().plan_count, 0);
}

#[tokio::test]
async fn related_keywords_from_csv_are_used_before_ai() {
    let cx = setup(40, 500).await;
    let id = &cx.campaign.id;
    keywords::save_primary(&cx.state, id, vec!["لپ تاپ".into()]).await.unwrap();
    keywords::import_keywords_csv_for_test(&cx.state, id, "keyword,search_volume,related\nلپ تاپ گیمینگ,900,لپ تاپ ایسوس|لپ تاپ لنوو\n").await;
    let all = keywords::list(&cx.state, kq(id, "candidate")).await.unwrap();
    keywords::select(&cx.state, id, &[all.items[0].id.clone()], true).await.unwrap();
    secondary::generate(&cx.state, id, true).await.unwrap();
    let sec = keywords::list(&cx.state, kq(id, "secondary")).await.unwrap();
    let from_user: Vec<_> = sec.items.iter().filter(|k| k.source == "user").map(|k| k.keyword.as_str()).collect();
    assert!(from_user.contains(&"لپ تاپ ایسوس") && from_user.contains(&"لپ تاپ لنوو"));
}

#[tokio::test]
async fn similarity_warning_for_near_duplicates() {
    let cx = setup(10, 100).await;
    let id = &cx.campaign.id;
    keywords::save_primary(&cx.state, id, vec!["بهترین لپ تاپ دانشجویی".into(), "بهترین لپ تاپ دانشجویی جدید".into(), "آموزش پخت کیک".into()]).await.unwrap();
    let w = keywords::similarity_warnings(&cx.state, id, 0.6).await.unwrap();
    assert_eq!(w.len(), 1);
    assert!(w[0].a_keyword.contains("بهترین") && w[0].score >= 0.6);
}

#[tokio::test]
async fn titles_flow_generate_approve_edit_alternative_reject() {
    let cx = setup(25, 500).await;
    let id = &cx.campaign.id;
    keywords::save_primary(&cx.state, id, vec!["لپ تاپ".into()]).await.unwrap();
    assert_eq!(titles::generate(&cx.state, id).await.unwrap_err().code, "VALIDATION", "needs plans first");
    secondary::generate(&cx.state, id, true).await.unwrap();
    secondary::confirm(&cx.state, id).await.unwrap();

    let t = titles::generate(&cx.state, id).await.unwrap();
    assert_eq!((t.generated, t.missing.len()), (60, 0));
    let l = titles::list(&cx.state, id).await.unwrap();
    assert_eq!((l.total, l.approved, l.pending, l.all_approved), (60, 0, 60, false));
    // Re-running only fills gaps.
    assert_eq!(titles::generate(&cx.state, id).await.unwrap().generated, 0);

    let p0 = &l.plans[0];
    titles::approve(&cx.state, &p0.plan_id, p0.title_id.as_ref().unwrap()).await.unwrap();
    assert_eq!(titles::list(&cx.state, id).await.unwrap().approved, 1);

    // Manual edit becomes the single approved title.
    let p1 = &l.plans[1];
    let manual = format!("  عنوان دستی من {}  ", p1.keyword);
    let e = titles::edit(&cx.state, &p1.plan_id, &manual).await.unwrap();
    assert_eq!(e.title, format!("عنوان دستی من {}", p1.keyword));
    assert_eq!(titles::edit(&cx.state, &p1.plan_id, "   ").await.unwrap_err().code, "VALIDATION");
    // A manual title must contain the plan's keyword.
    assert_eq!(titles::edit(&cx.state, &p1.plan_id, "عنوانی بدون کلمه").await.unwrap_err().code, "VALIDATION");

    // Alternative: new, different title; an approved title goes back to pending.
    let alt = titles::alternative(&cx.state, &p0.plan_id).await.unwrap();
    assert_ne!(Some(alt.title.clone()), p0.title);
    let l2 = titles::list(&cx.state, id).await.unwrap();
    assert_eq!(l2.plans[0].status, "PLANNED");
    assert_eq!(l2.plans[0].title.as_deref(), Some(alt.title.as_str()));
    let alt2 = titles::alternative(&cx.state, &p0.plan_id).await.unwrap();
    assert_ne!(alt.title, alt2.title);

    // Reject an approved title → plan returns to pending.
    titles::reject(&cx.state, &p1.plan_id, &e.id).await.unwrap();
    assert_eq!(titles::list(&cx.state, id).await.unwrap().plans[1].status, "PLANNED");

    // Approve everything that is pending; only plans that still have a title can be approved.
    let n = titles::approve_all(&cx.state, id).await.unwrap();
    assert_eq!(n, 59, "all but the plan whose only title was rejected");
    let regen = titles::generate(&cx.state, id).await.unwrap();
    assert_eq!(regen.generated, 1);
    titles::approve_all(&cx.state, id).await.unwrap();
    let done = titles::list(&cx.state, id).await.unwrap();
    assert!(done.all_approved && done.approved == 60);
}
