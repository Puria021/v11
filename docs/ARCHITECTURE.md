# MyArticleManager — معماری (مرحله ۱)

> مبنا: سند دوم (Tauri 2 + SQLite). از سند اول فقط مدل داده، وضعیت‌ها، Providerها و منطق کمپین حفظ شده است.
> PostgreSQL، Redis، NestJS، Docker و WebSocket حذف شده‌اند.

## ۱. تصمیم‌های معماری

| موضوع | انتخاب | دلیل |
|---|---|---|
| Runtime | Tauri 2 (Rust) | بدون نیاز به Node/Python در سیستم کاربر؛ خروجی سبک |
| UI | React + TypeScript + Tailwind، RTL | طبق سند |
| فونت | Vazirmatn (bundle محلی) | کار در حالت آفلاین و بدون CDN |
| Database | SQLite (WAL) با `sqlx` + migration | بدون سرور؛ تراکنش و قفل مطمئن |
| Queue | جدول `jobs` در SQLite + Worker با `tokio` | ماندگار در برابر بستن برنامه و Crash |
| Secrets | `keyring` (Windows Credential Manager) | کلیدها هرگز در SQLite یا React نمی‌روند |
| HTTP | `reqwest` (rustls) | تنظیم Proxy و Timeout؛ فقط در Rust |
| Realtime | Tauri Events (`emit`) | جایگزین WebSocket/SSE |
| Sanitization | crate `ammonia` در Rust | قبل از ذخیره و قبل از ارسال به WordPress |
| State در UI | TanStack Query + Zustand | کش درخواست‌ها + وضعیت محلی Wizard |
| ویرایشگر مقاله | TipTap (HTML) | Heading/List/Table/Link/Quote |
| Notification | `tauri-plugin-notification` | اعلان ویندوز |
| Updater | `tauri-plugin-updater` (غیرفعال در v1) | آماده برای آینده |

### نکته درباره پراکسی و دسترسی
چون ممکن است بعضی سرویس‌ها از ایران مستقیم در دسترس نباشند، تنظیم **Proxy (HTTP/SOCKS5)** سراسری و به‌ازای هر Provider در Settings پیش‌بینی شده است.

### نکته درباره Keyword Research
Search Volume فقط از `KeywordProvider` می‌آید. Providerهای نسخه اول:
1. `manual_csv` — ورود CSV از ابزارهای دیگر (بدون API)
2. `dataforseo` — اختیاری
3. `ai_suggest` — فقط پیشنهاد کلمه؛ **همیشه** `search_volume = NULL` و `source = 'ai'`

## ۲. نمودار لایه‌ها

```
┌──────────────────────── React (WebView2) ────────────────────────┐
│ pages/  components/  hooks/  api/ (invoke wrappers)  store/      │
└───────────────┬───────────────────────────────▲──────────────────┘
        invoke(command)                   listen(event)
┌───────────────▼───────────────────────────────┴──────────────────┐
│ Tauri / Rust                                                     │
│  commands/   ← لایه نازک: validation + فراخوانی service          │
│  services/   ← منطق کسب‌وکار (campaign, keywords, titles, ...)    │
│  providers/  ← traits + adapterها (ai, keyword, image, wordpress)│
│  queue/      ← scheduler, workers, rate_limiter, recovery        │
│  db/         ← pool, migrations, repositories                    │
│  secrets/    ← keyring wrapper                                   │
│  events/     ← emit progress/log                                 │
│  util/       ← crypto, slug, similarity, sanitize, net_status    │
└───────┬──────────────────────┬───────────────────────────────────┘
        │ SQLite (app.db)      │ HTTPS خروجی
        │ Credential Manager   │ WordPress / Keyword / AI / Image
```

## ۳. ساختار پوشه‌ها

```
MyArticleManager/
├─ docs/                      ARCHITECTURE.md  COMMANDS.md
├─ src/                       React
│  ├─ api/                    wrapperهای تایپ‌شده invoke
│  ├─ pages/                  Dashboard, Projects, Campaigns, Wizard, Templates, Providers, Logs, Settings
│  ├─ components/  hooks/  store/  i18n/fa.ts
├─ src-tauri/
│  ├─ migrations/             0001_init.sql ...
│  ├─ src/
│  │  ├─ main.rs  lib.rs  state.rs  error.rs
│  │  ├─ commands/            projects.rs wordpress.rs keywords.rs campaigns.rs articles.rs providers.rs jobs.rs logs.rs backup.rs settings.rs
│  │  ├─ services/
│  │  ├─ providers/
│  │  │  ├─ ai/               mod.rs(trait) openai.rs anthropic.rs gemini.rs openai_compat.rs
│  │  │  ├─ keyword/          mod.rs(trait) manual_csv.rs dataforseo.rs ai_suggest.rs
│  │  │  ├─ image/            mod.rs(trait) openai_image.rs
│  │  │  └─ wordpress/        mod.rs(trait) rest.rs mock.rs
│  │  ├─ queue/               scheduler.rs worker.rs rate_limit.rs recovery.rs
│  │  ├─ db/  secrets/  events/  util/
│  └─ tests/                  integration + mock WordPress (wiremock)
└─ .github/workflows/build-windows.yml   (مرحله ۵)
```

## ۴. مسیرهای ذخیره‌سازی (`%APPDATA%\MyArticleManager\`)

```
data\app.db
projects\<project_id>\campaigns\<campaign_id>\images\
                                          \exports\
                                          \logs\
logs\app-YYYY-MM-DD.log
```

## ۵. قراردادهای Provider (Rust traits)

```rust
#[async_trait]
pub trait AiProvider: Send + Sync {
    async fn generate_text(&self, req: TextRequest) -> Result<TextResponse, ProviderError>;
    async fn generate_structured(&self, req: StructuredRequest) -> Result<serde_json::Value, ProviderError>;
    async fn generate_image_prompt(&self, req: ImagePromptRequest) -> Result<String, ProviderError>;
    async fn test(&self) -> Result<(), ProviderError>;
}

#[async_trait]
pub trait KeywordProvider: Send + Sync {
    async fn research(&self, req: KeywordQuery) -> Result<Vec<KeywordRecord>, ProviderError>;
    fn capabilities(&self) -> KeywordCaps; // volume, cpc, trend, related, countries
}

#[async_trait]
pub trait ImageProvider: Send + Sync {
    async fn generate(&self, req: ImageRequest) -> Result<GeneratedImage, ProviderError>; // bytes + mime
}

#[async_trait]
pub trait WordPressClient: Send + Sync {
    async fn test_connection(&self) -> Result<SiteInfo, ProviderError>;
    async fn fetch_taxonomies(&self) -> Result<Taxonomies, ProviderError>; // categories, tags, authors
    async fn upload_media(&self, m: MediaUpload) -> Result<MediaRef, ProviderError>;
    async fn create_post(&self, p: NewPost) -> Result<PostRef, ProviderError>;
    async fn find_post_by_meta(&self, key: &str) -> Result<Option<PostRef>, ProviderError>; // ضد Duplicate
}
```

Registry: `ProviderRegistry` مقدار `kind` ذخیره‌شده در DB را به یک adapter نگاشت می‌کند؛ افزودن Provider جدید = یک فایل جدید + یک خط ثبت.

**مدل خطا:** `ProviderError { kind: AuthInvalid | RateLimited{retry_after} | Timeout | Network | InvalidResponse | Remote{status} | Offline, message, retryable }`. پیام‌ها قبل از لاگ از کلیدها پاک می‌شوند (redaction).

## ۶. صف و Worker

- سه Queue: `content`، `image`، `wordpress` (به‌علاوه `keyword` سبک). هر کدام `Semaphore` با Concurrency قابل تنظیم (پیش‌فرض ۳/۲/۳).
- هر Provider یک Token-bucket (req/min، req/sec، max concurrent) دارد. روی 429 → Exponential Backoff + jitter و احترام به `Retry-After`.
- چرخه حیات Job:
  `PENDING → PROCESSING → DONE`  یا  `PROCESSING → RETRY_PENDING (با run_after) → ...` یا `FAILED / CANCELLED / PAUSED`
- **Pause:** Scheduler دیگر Job جدید برنمی‌دارد؛ Jobهای در حال اجرا تمام می‌شوند.
- **Cancel:** Jobهای PENDING لغو می‌شوند؛ Job جاری در نقطه‌ی بررسی بعدی متوقف می‌شود.
- **Crash Recovery:** در راه‌اندازی، هر Job با وضعیت `PROCESSING` به `RETRY_PENDING` برمی‌گردد.
- هر مرحله‌ی یک مقاله یک Job جدا: `generate_content` → `generate_image` → `upload_wordpress` (Job بعدی پس از موفقیت قبلی ساخته می‌شود).
- `dedupe_key` یکتا برای Job فعال هر (مقاله، نوع) جلوی دوباره‌کاری را می‌گیرد.

## ۷. جلوگیری از Duplicate Upload

۱. جدول `wordpress_posts` برای هر `article_plan_id` فقط یک ردیف (UNIQUE).
۲. قبل از `create_post` اگر `wordpress_post_id` وجود دارد → `DUPLICATE_BLOCKED`.
۳. هنگام ساخت پست، یک meta/slug یکتا با شناسه‌ی مقاله ارسال می‌شود تا در Retry پس از Timeout، `find_post_by_meta` پست نیمه‌ساخته را پیدا کند.

## ۸. امنیت

- کلیدها و Application Password فقط در Credential Manager؛ در DB فقط `secret_ref`.
- React هیچ‌وقت کلید را نمی‌بیند؛ UI فقط `has_secret: bool` و `••••` دریافت می‌کند.
- لاگ‌ها redaction می‌شوند؛ خروجی پشتیبان بدون کلید است.
- `tauri.conf.json`: CSP سخت‌گیرانه، بدون `fetch` مستقیم به اینترنت از WebView، allowlist فقط برای Commandهای خودمان.
- HTML خروجی AI با `ammonia` پاک‌سازی می‌شود (قبل از ذخیره و قبل از آپلود).
- Backup رمزگذاری‌شده اختیاری (Argon2 + AES-GCM) اگر کاربر بخواهد کلیدها هم باشند؛ در حالت عادی بدون کلید.

## ۹. نقشه‌ی مراحل بعد

| مرحله | محتوا |
|---|---|
| ۲ | اسکلت Tauri/React، migration، Project CRUD، WordPress connection + Mock |
| ۳ | Keyword Provider، Campaign، Secondary Keywords (۶۰)، Titles، Article Plan UI |
| ۴ | Template، AI/Image Providerها، Queue، تولید مقاله، کنترل کیفیت |
| ۵ | آپلود Draft، Editor/Preview، Logs، Backup/Import، تست‌ها، CI ساخت `.exe/.msi` |
