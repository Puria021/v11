//! WordPress REST API client (Application Password / HTTP Basic).

use super::{MediaRef, MediaUpload, NewPost, PostRef, SiteInfo, Taxonomies, Term, WordPressClient};
use crate::providers::{ProviderError, ProviderErrorKind};
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use serde_json::Value;
use std::time::Duration;

const MAX_PAGES: u32 = 30;

/// Authentication for WordPress requests.
/// - With a username: HTTP Basic (Application Password), the original behaviour.
/// - Without a username: the MyArticleManager Bridge token, sent in the X-MAM-Token header.
trait WithAuth {
    fn with_auth(self, c: &RestWordPressClient) -> Self;
}

impl WithAuth for reqwest::RequestBuilder {
    fn with_auth(self, c: &RestWordPressClient) -> Self {
        if c.user.trim().is_empty() {
            self.header("X-MAM-Token", c.pass.as_str())
        } else {
            self.basic_auth(&c.user, Some(&c.pass))
        }
    }
}

pub struct RestWordPressClient {
    base: String, // e.g. https://site.com/wp-json (no trailing slash)
    user: String,
    pass: String,
    http: reqwest::Client,
}

impl RestWordPressClient {
    pub fn new(
        base: &str,
        user: &str,
        pass: &str,
        proxy: Option<&str>,
        timeout: Duration,
    ) -> Result<Self, ProviderError> {
        let mut b = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(15))
            .user_agent("MyArticleManager/0.1");
        b = crate::services::network::apply_http(b, proxy)?;
        let http = b
            .build()
            .map_err(|_| ProviderError::new(ProviderErrorKind::Network, "ساخت کلاینت HTTP ناموفق بود."))?;
        Ok(Self {
            base: base.trim_end_matches('/').to_string(),
            user: user.to_string(),
            pass: pass.to_string(),
            http,
        })
    }

    /// Optional companion bridge: requires the MyArticleManager Bridge WordPress plugin.
    pub async fn list_products(&self, search: &str, page: i64) -> Result<Value, ProviderError> {
        let path = format!("myarticle/v1/products?page={}&per_page=20&search={}", page, url::form_urlencoded::byte_serialize(search.as_bytes()).collect::<String>());
        Ok(self.get(&path).await?.0)
    }

    pub async fn update_product_description(&self, id: i64, description: &str) -> Result<(), ProviderError> {
        self.post_json(&format!("myarticle/v1/products/{id}/description"), &serde_json::json!({"description": description})).await.map(|_| ())
    }

    pub async fn update_rank_math(&self, id: i64, title: &str, description: &str) -> Result<(), ProviderError> {
        self.post_json(&format!("myarticle/v1/rankmath/{id}"), &serde_json::json!({"title": title, "description": description})).await.map(|_| ())
    }

    /// Writes the Rank Math SEO fields as post meta through the core REST API, then reads them back.
    /// Returns Ok(true) only when WordPress really stored the values (Rank Math may not expose them).
    pub async fn set_rank_math_meta(&self, id: i64, keyword: &str, title: &str, description: &str) -> Result<bool, ProviderError> {
        let body = serde_json::json!({"meta": {
            "rank_math_focus_keyword": keyword,
            "rank_math_title": title,
            "rank_math_description": description,
        }});
        self.post_json(&format!("wp/v2/posts/{id}"), &body).await?;
        let (post, _) = self.get(&format!("wp/v2/posts/{id}?context=edit&_fields=meta")).await?;
        let meta = post.get("meta");
        let read = |k: &str| meta.and_then(|m| m.get(k)).and_then(|v| v.as_str()).unwrap_or("").to_string();
        Ok(read("rank_math_title") == title && read("rank_math_description") == description && read("rank_math_focus_keyword") == keyword)
    }

    pub async fn published_posts(&self) -> Result<Vec<(String, String)>, ProviderError> {
        let items = self.get_all("wp/v2/posts?status=publish&_fields=id,title,link,slug").await?;
        Ok(items.into_iter().filter_map(|v| {
            let name = v.pointer("/title/rendered")?.as_str()?.to_string();
            let link = v.get("link")?.as_str()?.to_string();
            let parsed = url::Url::parse(&link).ok()?;
            let site = url::Url::parse(&self.base).ok()?;
            if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str() != site.host_str() { return None; }
            Some((name, link))
        }).collect())
    }

    fn url(&self, path: &str) -> String {
        if path.is_empty() { self.base.clone() } else { format!("{}/{}", self.base, path) }
    }

    async fn get(&self, path: &str) -> Result<(Value, HeaderMap), ProviderError> {
        let rb = self.http.get(self.url(path)).with_auth(self).header("Accept", "application/json");
        self.send(rb).await
    }

    async fn post_json(&self, path: &str, body: &Value) -> Result<(Value, HeaderMap), ProviderError> {
        let rb = self.http.post(self.url(path)).with_auth(self).header("Accept", "application/json").json(body);
        self.send(rb).await
    }

    async fn send(&self, rb: reqwest::RequestBuilder) -> Result<(Value, HeaderMap), ProviderError> {
        let resp = rb.send().await.map_err(map_reqwest_error)?;
        let status = resp.status();
        let headers = resp.headers().clone();
        if !status.is_success() {
            let body: Option<Value> = resp.bytes().await.ok().and_then(|b| serde_json::from_slice(&b).ok());
            let wp_msg = body
                .as_ref()
                .and_then(|b| b.get("message"))
                .and_then(|m| m.as_str())
                .map(|m| format!(" ({})", m.chars().take(160).collect::<String>()))
                .unwrap_or_default();
            let mut err = match status.as_u16() {
                401 | 403 => ProviderError::new(
                    ProviderErrorKind::AuthInvalid,
                    "نام کاربری یا Application Password نامعتبر است (یا دسترسی کافی ندارد).",
                ),
                404 => ProviderError::new(
                    ProviderErrorKind::NotFound,
                    "REST API وردپرس در این آدرس پیدا نشد. آدرس را بررسی کنید و مطمئن شوید پیوند یکتا (Permalink) فعال است.",
                ),
                413 => ProviderError::new(ProviderErrorKind::InvalidResponse, "حجم فایل برای سرور وردپرس زیاد است."),
                429 => ProviderError::new(ProviderErrorKind::RateLimited, "محدودیت تعداد درخواست؛ کمی بعد دوباره تلاش کنید."),
                400 | 422 => ProviderError::new(ProviderErrorKind::InvalidResponse, format!("درخواست توسط وردپرس رد شد{wp_msg}")),
                s if s >= 500 => ProviderError::new(ProviderErrorKind::Remote, format!("خطای سمت سرور وردپرس (کد {s}).")),
                s => ProviderError::new(ProviderErrorKind::Remote, format!("پاسخ نامنتظره از وردپرس (کد {s}).")),
            };
            err.status = Some(status.as_u16());
            err.retry_after_secs = headers.get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok());
            err.body = body;
            return Err(err);
        }

        let bytes = resp.bytes().await.map_err(map_reqwest_error)?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
            ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "پاسخ وردپرس JSON معتبر نبود (ممکن است افزونه‌ی امنیتی یا فایروال REST API را مسدود کرده باشد).",
            )
        })?;
        Ok((value, headers))
    }

    /// Fetch every page of a collection (uses the X-WP-TotalPages header).
    async fn get_all(&self, path_with_query: &str) -> Result<Vec<Value>, ProviderError> {
        let sep = if path_with_query.contains('?') { '&' } else { '?' };
        let mut out = Vec::new();
        let mut page = 1u32;
        loop {
            let (v, headers) = self.get(&format!("{path_with_query}{sep}per_page=100&page={page}")).await?;
            let total: u32 = headers
                .get("x-wp-totalpages")
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            match v {
                Value::Array(items) => {
                    if items.is_empty() {
                        break;
                    }
                    out.extend(items);
                }
                _ => {
                    return Err(ProviderError::new(
                        ProviderErrorKind::InvalidResponse,
                        "ساختار پاسخ وردپرس نامعتبر است.",
                    ))
                }
            }
            if page >= total || page >= MAX_PAGES {
                break;
            }
            page += 1;
        }
        Ok(out)
    }
}

fn map_reqwest_error(e: reqwest::Error) -> ProviderError {
    if e.is_timeout() {
        ProviderError::new(ProviderErrorKind::Timeout, "زمان انتظار برای پاسخ سایت تمام شد.")
    } else if e.is_connect() {
        ProviderError::new(ProviderErrorKind::Network, "اتصال به سایت برقرار نشد. آدرس، اینترنت یا Proxy را بررسی کنید.")
    } else {
        ProviderError::new(ProviderErrorKind::Network, "خطای شبکه هنگام ارتباط با سایت.")
    }
}

fn str_field(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|s| s.to_string())
}

fn to_term(v: &Value) -> Option<Term> {
    Some(Term {
        wp_id: v.get("id")?.as_i64()?,
        name: str_field(v, "name")?,
        slug: str_field(v, "slug"),
        parent_wp_id: v.get("parent").and_then(|p| p.as_i64()).filter(|p| *p != 0),
    })
}

#[async_trait]
impl WordPressClient for RestWordPressClient {
    async fn test_connection(&self) -> Result<SiteInfo, ProviderError> {
        // 1) site index (public): site name
        let (root, _) = self.get("").await?;
        let site_name = str_field(&root, "name").unwrap_or_default();
        if root.get("namespaces").is_none() && root.get("name").is_none() {
            return Err(ProviderError::new(
                ProviderErrorKind::InvalidResponse,
                "این آدرس شبیه REST API وردپرس نیست.",
            ));
        }

        // 2) authenticated identity + capabilities
        let (me, _) = self.get("wp/v2/users/me?context=edit").await?;
        let caps = me.get("capabilities");
        let has = |c: &str| caps.and_then(|m| m.get(c)).and_then(|v| v.as_bool()).unwrap_or(false);

        Ok(SiteInfo {
            site_name,
            user_id: me.get("id").and_then(|v| v.as_i64()).unwrap_or(0),
            user_name: str_field(&me, "name").unwrap_or_default(),
            // The REST index does not expose the WordPress version; left empty on purpose.
            wp_version: None,
            media_capable: has("upload_files"),
            can_edit_posts: has("edit_posts"),
        })
    }

    async fn fetch_taxonomies(&self) -> Result<Taxonomies, ProviderError> {
        let categories = self
            .get_all("wp/v2/categories?hide_empty=false")
            .await?
            .iter()
            .filter_map(to_term)
            .collect();
        let tags = self.get_all("wp/v2/tags?hide_empty=false").await?.iter().filter_map(to_term).collect();

        // Authors: some sites restrict the users list — treat that as "no authors", not a failure.
        let authors = match self.get_all("wp/v2/users").await {
            Ok(list) => list.iter().filter_map(to_term).collect(),
            Err(e) if e.kind == ProviderErrorKind::AuthInvalid => Vec::new(),
            Err(e) => return Err(e),
        };

        // Post types come as an object keyed by slug and have no numeric id: number them.
        let mut post_types = Vec::new();
        if let (Value::Object(map), _) = self.get("wp/v2/types").await? {
            for (i, (slug, v)) in map.iter().enumerate() {
                if v.get("rest_base").is_none() {
                    continue;
                }
                post_types.push(Term {
                    wp_id: (i as i64) + 1,
                    name: str_field(v, "name").unwrap_or_else(|| slug.clone()),
                    slug: Some(slug.clone()),
                    parent_wp_id: None,
                });
            }
        }

        Ok(Taxonomies { categories, tags, authors, post_types })
    }

    async fn upload_media(&self, m: MediaUpload) -> Result<MediaRef, ProviderError> {
        let safe: String = m.filename.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_')).collect();
        let rb = self
            .http
            .post(self.url("wp/v2/media"))
            .with_auth(self)
            .header("Content-Type", m.mime.as_str())
            .header("Content-Disposition", format!("attachment; filename=\"{safe}\""))
            .header("Accept", "application/json")
            .body(m.bytes);
        let (v, _) = self.send(rb).await?;
        let id = v.get("id").and_then(|x| x.as_i64()).ok_or_else(|| {
            ProviderError::new(ProviderErrorKind::InvalidResponse, "پاسخ آپلود رسانه شناسه‌ی فایل را نداشت.")
        })?;
        Ok(MediaRef { id, url: str_field(&v, "source_url").unwrap_or_default() })
    }

    async fn update_media_meta(&self, id: i64, title: &str, alt_text: &str, caption: &str) -> Result<(), ProviderError> {
        self.post_json(&format!("wp/v2/media/{id}"), &serde_json::json!({"title": title, "alt_text": alt_text, "caption": caption}))
            .await
            .map(|_| ())
    }

    async fn create_post(&self, p: NewPost) -> Result<PostRef, ProviderError> {
        let mut body = serde_json::json!({
            "title": p.title, "content": p.content, "excerpt": p.excerpt, "slug": p.slug, "status": "draft",
            "categories": p.categories, "tags": p.tags,
        });
        if let Some(m) = p.featured_media {
            body["featured_media"] = serde_json::json!(m);
        }
        let (v, _) = self.post_json("wp/v2/posts", &body).await?;
        post_ref(&v).ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidResponse, "پاسخ ساخت نوشته شناسه نداشت."))
    }

    async fn find_post_by_slug(&self, slug: &str) -> Result<Option<PostRef>, ProviderError> {
        let q: String = url::form_urlencoded::byte_serialize(slug.as_bytes()).collect();
        let (v, _) = self
            .get(&format!("wp/v2/posts?slug={q}&status=draft,pending,publish,private,future&per_page=5&context=edit"))
            .await?;
        Ok(v.as_array().and_then(|a| a.first()).and_then(post_ref))
    }

    async fn ensure_tag(&self, name: &str) -> Result<i64, ProviderError> {
        let q: String = url::form_urlencoded::byte_serialize(name.as_bytes()).collect();
        let (found, _) = self.get(&format!("wp/v2/tags?search={q}&per_page=100")).await?;
        let wanted = name.trim().to_lowercase();
        if let Some(id) = found.as_array().and_then(|a| {
            a.iter().find(|t| str_field(t, "name").map(|n| n.trim().to_lowercase() == wanted).unwrap_or(false)).and_then(|t| t.get("id")?.as_i64())
        }) {
            return Ok(id);
        }
        match self.post_json("wp/v2/tags", &serde_json::json!({"name": name})).await {
            Ok((v, _)) => v.get("id").and_then(|x| x.as_i64()).ok_or_else(|| ProviderError::new(ProviderErrorKind::InvalidResponse, "پاسخ ساخت برچسب شناسه نداشت.")),
            // A concurrent request may have created it: WordPress answers "term_exists" with the existing id.
            Err(e) => e
                .body
                .as_ref()
                .and_then(|b| b.pointer("/data/term_id"))
                .and_then(|t| t.as_i64())
                .ok_or(e),
        }
    }
}

fn post_ref(v: &Value) -> Option<PostRef> {
    Some(PostRef { id: v.get("id")?.as_i64()?, link: str_field(v, "link").unwrap_or_default(), status: str_field(v, "status").unwrap_or_default() })
}
