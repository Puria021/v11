pub mod rest;

use crate::providers::ProviderError;
use async_trait::async_trait;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SiteInfo {
    pub site_name: String,
    pub user_id: i64,
    pub user_name: String,
    pub wp_version: Option<String>,
    pub media_capable: bool,
    pub can_edit_posts: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Term {
    pub wp_id: i64,
    pub name: String,
    pub slug: Option<String>,
    pub parent_wp_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Taxonomies {
    pub categories: Vec<Term>,
    pub tags: Vec<Term>,
    pub authors: Vec<Term>,
    pub post_types: Vec<Term>,
}

pub struct MediaUpload {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub mime: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaRef {
    pub id: i64,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct NewPost {
    /// "draft" or "publish".
    pub status: String,
    /// WordPress publish date in GMT, "YYYY-MM-DDTHH:MM:SS". None = WordPress default.
    pub date_gmt: Option<String>,
    pub title: String,
    pub content: String,
    pub excerpt: String,
    pub slug: String,
    pub featured_media: Option<i64>,
    pub categories: Vec<i64>,
    pub tags: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PostRef {
    pub id: i64,
    pub link: String,
    pub status: String,
}

#[async_trait]
pub trait WordPressClient: Send + Sync {
    async fn test_connection(&self) -> Result<SiteInfo, ProviderError>;
    async fn fetch_taxonomies(&self) -> Result<Taxonomies, ProviderError>;
    async fn upload_media(&self, m: MediaUpload) -> Result<MediaRef, ProviderError>;
    async fn update_media_meta(&self, id: i64, title: &str, alt_text: &str, caption: &str) -> Result<(), ProviderError>;
    /// Creates the post with `p.status` ("draft", "future" or "publish").
    async fn create_post(&self, p: NewPost) -> Result<PostRef, ProviderError>;
    /// Changes status and, when given, the GMT publish date ("YYYY-MM-DDTHH:MM:SS").
    async fn set_post_status(&self, id: i64, status: &str, date_gmt: Option<&str>) -> Result<(), ProviderError>;
    /// Looks for an existing post with this slug in any status (used to adopt a post created by an
    /// earlier attempt whose response was lost).
    async fn find_post_by_slug(&self, slug: &str) -> Result<Option<PostRef>, ProviderError>;
    /// Finds a tag by exact name or creates it.
    async fn ensure_tag(&self, name: &str) -> Result<i64, ProviderError>;
}
