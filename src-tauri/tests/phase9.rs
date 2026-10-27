//! Phase 9 integration tests: license quota, campaign caps, the single-article mode and the product quota.

use myarticlemanager_lib::services::{campaigns, quota, single};
use myarticlemanager_lib::services::campaigns::CreateCampaign;
use myarticlemanager_lib::services::projects::{self, ProjectInput};
use myarticlemanager_lib::services::single::SingleInput;
use myarticlemanager_lib::secrets::MemorySecretStore;
use myarticlemanager_lib::state::AppState;
use std::sync::Arc;

async fn state_with_project() -> (AppState, String) {
    let state = AppState::for_tests(Arc::new(MemorySecretStore::new())).await.unwrap();
    let p = projects::create(&state, ProjectInput { name: "پروژه".into(), site_url: "example.com".into(), ..Default::default() })
        .await
        .unwrap();
    (state, p.id)
}

fn campaign(project_id: &str, target: i64) -> CreateCampaign {
    CreateCampaign { project_id: project_id.into(), name: "کمپین".into(), target_count: Some(target), country: None, language: None, ai_provider_id: None }
}

#[tokio::test]
async fn new_license_has_ten_articles_and_five_product_descriptions() {
    let (state, _) = state_with_project().await;
    let q = quota::view(&state).await.unwrap();
    assert_eq!((q.articles.total, q.articles.remaining, q.articles.available), (10, 10, 10));
    assert_eq!((q.products.total, q.products.remaining), (5, 5));
}

#[tokio::test]
async fn campaigns_are_capped_by_the_remaining_quota() {
    let (state, pid) = state_with_project().await;
    let first = campaigns::create(&state, campaign(&pid, 60)).await.unwrap();
    assert_eq!(first.target_count, 10, "a 60-article request is capped to the license quota");
    // Everything is reserved by the first campaign, so the second one has nothing left.
    assert!(campaigns::create(&state, campaign(&pid, 5)).await.is_err());
}

#[tokio::test]
async fn single_article_counts_against_the_same_quota_and_is_hidden_from_campaigns() {
    let (state, pid) = state_with_project().await;
    let created = single::create(
        &state,
        SingleInput {
            project_id: pid.clone(),
            keyword: "خرید لپ تاپ".into(),
            image_count: 2,
            external_url: Some("https://www.who.int/".into()),
            external_title: Some("سازمان جهانی بهداشت".into()),
            external_follow: false,
            cta: None,
        },
    )
    .await
    .unwrap();
    let c = campaigns::get(&state, &created.campaign_id).await.unwrap();
    assert_eq!((c.kind.as_str(), c.target_count, c.image_count), ("single", 1, Some(2)));
    assert_eq!(c.external_follow, 0);
    assert!(campaigns::list(&state, Some(&pid)).await.unwrap().iter().all(|x| x.id != c.id));

    let q = quota::view(&state).await.unwrap();
    assert_eq!(q.articles.available, 9, "the single article reserves one unit");
    // A campaign can now take at most the nine remaining articles.
    let big = campaigns::create(&state, campaign(&pid, 10)).await.unwrap();
    assert_eq!(big.target_count, 9);
}

#[tokio::test]
async fn single_article_rejects_bad_input() {
    let (state, pid) = state_with_project().await;
    let bad_images = SingleInput { project_id: pid.clone(), keyword: "k".into(), image_count: 5, external_url: None, external_title: None, external_follow: true, cta: None };
    assert!(single::create(&state, bad_images).await.is_err());
    let bad_url = SingleInput { project_id: pid, keyword: "k".into(), image_count: 1, external_url: Some("ftp://x".into()), external_title: None, external_follow: true, cta: None };
    assert!(single::create(&state, bad_url).await.is_err());
}

#[tokio::test]
async fn product_quota_stops_at_five_and_reset_restores_it() {
    let (state, _) = state_with_project().await;
    for _ in 0..5 {
        quota::take_product(&state).await.unwrap();
    }
    assert!(quota::take_product(&state).await.is_err());
    quota::reset(&state).await.unwrap();
    let q = quota::view(&state).await.unwrap();
    assert_eq!((q.products.remaining, q.articles.remaining), (5, 10));
}
