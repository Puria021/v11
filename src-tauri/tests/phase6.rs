//! License and fixed (hidden) providers.

use myarticlemanager_lib::secrets::MemorySecretStore;
use myarticlemanager_lib::services::{ai, image_providers, license};
use myarticlemanager_lib::state::AppState;
use std::sync::Arc;

async fn state() -> AppState {
    AppState::for_tests(Arc::new(MemorySecretStore::new())).await.unwrap()
}

#[tokio::test]
async fn license_starts_inactive_and_rejects_an_empty_key() {
    let s = state().await;
    assert!(!license::status(&s).await.unwrap().activated, "the license tab must show on first install");
    assert_eq!(license::activate(&s, "   ").await.unwrap_err().code, "VALIDATION");
    assert!(!license::status(&s).await.unwrap().activated);
}

#[tokio::test]
async fn built_in_providers_are_hidden_and_protected() {
    let s = state().await;
    // Not created until activation, so the lists are empty and deleting/testing them fails cleanly.
    assert!(ai::list(&s).await.unwrap().is_empty());
    assert!(image_providers::list(&s).await.unwrap().is_empty());
    assert_eq!(ai::delete(&s, license::TEXT_PROVIDER_ID).await.unwrap_err().code, "VALIDATION", "app-managed providers cannot be deleted");
    assert!(license::is_builtin(license::TEXT_PROVIDER_ID) && license::is_builtin(license::IMAGE_PROVIDER_ID));
    assert!(!license::is_builtin("some-user-provider"));
}
