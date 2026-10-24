> **توجه:** این سند قرارداد اولیه‌ی طراحی (مرحله ۱) است. فهرست دقیق Commandهای پیاده‌سازی‌شده در `src-tauri/src/lib.rs` و نوع‌های TypeScript آن‌ها در `src/api/tauri.ts` است.

# MyArticleManager — قرارداد Commandهای Tauri (جایگزین API REST)

React فقط از `invoke("<command>", args)` و `listen("<event>")` استفاده می‌کند. تمام تماس‌های بیرونی در Rust انجام می‌شود.

## قواعد عمومی

- نام Command: `snake_case`؛ ورودی/خروجی: JSON با `camelCase`.
- همه‌ی Commandها `Result<T, AppError>` برمی‌گردانند:

```ts
type AppError = {
  code: "VALIDATION" | "NOT_FOUND" | "OFFLINE" | "AUTH_INVALID" | "RATE_LIMITED" | "TIMEOUT"
      | "NETWORK" | "INVALID_RESPONSE" | "REMOTE" | "DUPLICATE_BLOCKED" | "CONFLICT" | "DB" | "INTERNAL";
  message: string;      // فارسی، قابل نمایش به کاربر
  retryable: boolean;
  details?: unknown;    // بدون هیچ Secret
};
```

- Secret فقط در Commandهای `set_*_secret` وارد می‌شود و هرگز برنمی‌گردد؛ خروجی‌ها فقط `hasSecret: boolean` دارند.
- عملیات نیازمند اینترنت در حالت آفلاین با `OFFLINE` رد می‌شوند.
- عملیات طولانی Job می‌سازند و `jobId` برمی‌گردانند؛ پیشرفت از Event می‌آید.

## Commandها

### سیستم / تنظیمات
| Command | ورودی | خروجی |
|---|---|---|
| `get_app_info` | – | `{version, dataDir, online}` |
| `check_internet` | – | `{online}` |
| `get_settings` / `update_settings` | `{patch}` | settings |
| `check_for_updates` | – | `{available, version?}` |
| `get_dashboard_stats` | – | شمارنده‌ها (پروژه، کمپین، پلن، Draft، خطا) |

### Project
| Command | ورودی | خروجی |
|---|---|---|
| `list_projects` | – | `Project[]` + آمار کارت |
| `get_project` | `{id}` | `Project` |
| `create_project` | `{name, description, siteUrl, brandName, targetAudience, defaultCountry, defaultLanguage}` | `Project` |
| `update_project` | `{id, patch}` | `Project` |
| `delete_project` | `{id, confirm: true}` | `void` (بدون `confirm` رد می‌شود) |
| `get_project_overview` | `{id}` | آمار §۴۵ سند اول |

### WordPress
| Command | ورودی | خروجی |
|---|---|---|
| `save_wordpress_connection` | `{projectId, apiUrl, username, applicationPassword}` | `WpConnection` (بدون رمز) |
| `test_wordpress_connection` | `{projectId}` | `{success, site, user, wpVersion?, mediaCapable}` |
| `sync_wordpress_taxonomies` | `{projectId}` | `{categories, tags, authors, postTypes}` |
| `list_wp_terms` | `{projectId, kind}` | `WpTerm[]` |

### Providerها (AI / Keyword / Image)
| Command | ورودی | خروجی |
|---|---|---|
| `list_ai_providers` / `list_keyword_providers` / `list_image_providers` | – | آرایه (با `hasSecret`) |
| `save_ai_provider` (و معادل keyword/image) | `{id?, name, kind, baseUrl, model, temperature, maxTokens, timeoutSec, proxyUrl, rpmLimit, maxConcurrent}` | provider |
| `set_provider_secret` | `{providerType, id, secret}` | `void` |
| `test_provider` | `{providerType, id}` | `{ok, latencyMs}` |
| `delete_provider` | `{providerType, id}` | `void` |
| `assign_project_providers` | `{projectId, aiProviderId, keywordProviderId, imageProviderId}` | `Project` |

### Template
`list_templates`, `get_template`, `save_template`, `duplicate_template`, `delete_template`, `preview_template_prompt {templateId, planId}` → prompt رندرشده با Placeholderها (`{{title}}`, `{{primary_keyword}}`, `{{secondary_keyword}}`, `{{brand_name}}`, `{{target_audience}}`, `{{internal_links}}`, `{{faq}}`).

### Campaign و Wizard
| Command | ورودی | خروجی |
|---|---|---|
| `list_campaigns` | `{projectId?}` | `Campaign[]` |
| `create_campaign` | `{projectId, name, targetCount=60, country, language}` | `Campaign` |
| `get_campaign` | `{id}` | `Campaign` + شمارنده‌ی مراحل |
| `save_wizard_state` | `{campaignId, step, state}` | `void` (Auto-save) |
| `delete_campaign` | `{id, confirm}` | `void` |

### Keyword
| Command | ورودی | خروجی |
|---|---|---|
| `run_keyword_research` | `{campaignId, seeds[], providerId, country, language}` | `{jobId}` |
| `import_keywords_csv` | `{campaignId, filePath, mapping}` | `{imported, skipped}` |
| `list_keywords` | `{campaignId, role?, filter, sort, page}` | صفحه‌بندی‌شده |
| `select_keywords` | `{campaignId, keywordIds[], selected}` | `void` |
| `add_manual_keyword` | `{campaignId, keyword}` | `Keyword` (`source=user`) |
| `generate_secondary_keywords` | `{campaignId, count=60}` | `{jobId}` |
| `get_similarity_warnings` | `{campaignId}` | `[{aKeywordId, bKeywordId, score, intent}]` |
| `resolve_similarity` | `{campaignId, keepKeywordId, dropKeywordId}` | `void` |

قانون: `generate_secondary_keywords` دقیقاً `count` مورد یکتا می‌سازد؛ اگر کمتر شد، `INSUFFICIENT_KEYWORDS` با تعداد کمبود برمی‌گردد و هیچ Volume ساختگی تولید نمی‌شود.

### Article Plan و عنوان
| Command | ورودی | خروجی |
|---|---|---|
| `generate_titles` | `{campaignId}` | `{jobId}` |
| `list_article_plans` | `{campaignId}` | پلن‌ها + عنوان فعلی + `approved/pending` |
| `approve_title` | `{planId, titleId}` | `void` |
| `edit_title` | `{planId, title}` | عنوان APPROVED جدید (`origin=manual`) |
| `generate_alternative_title` | `{planId}` | `ArticleTitle` (متفاوت از قبلی‌ها) |
| `reject_title` | `{planId, titleId}` | `void` |
| `approve_all_titles` | `{campaignId}` | `{approved}` |
| `set_plan_taxonomy` | `{planId, categoryIds[], tags[]}` | `void` |

`start_generation` تا زمانی که همه‌ی `targetCount` عنوان APPROVED نباشند خطای `VALIDATION` می‌دهد.

### تولید و مقاله
| Command | ورودی | خروجی |
|---|---|---|
| `start_generation` | `{campaignId, confirm: true}` | `{queuedJobs}` |
| `generate_article` | `{planId}` | `{jobId}` |
| `regenerate_article` / `retry_stage` | `{planId, stage: "content"|"image"|"upload"}` | `{jobId}` |
| `get_article` | `{planId}` | محتوا + تصویر + وضعیت WP |
| `update_article` | `{planId, patch, expectedVersion}` | `ArticleContent` (کنترل همزمانی) |
| `preview_article` | `{planId}` | HTML پاک‌سازی‌شده با استایل پیش‌نمایش |
| `upload_to_wordpress` | `{planId}` | `{jobId}` یا `DUPLICATE_BLOCKED` |
| `open_wordpress_draft` | `{planId}` | باز کردن URL در مرورگر |

### Job / Queue
`pause_campaign`, `resume_campaign`, `cancel_campaign` (`{campaignId}`) ، `list_jobs {filter}` ، `retry_job {jobId}` ، `get_progress {campaignId}` →
`{titles:{done,total}, content:{done,total}, images:{done,total}, wordpress:{done,total}, failed}`.

### لاگ، Backup
`list_logs {filter,page}`, `export_logs {path}`, `export_backup {projectId?, path, includeSecretsEncrypted?, password?}`, `preview_backup {path}` → `{projects, campaigns, articles, templates}`, `import_backup {path, mode}`.

## Eventها (Rust → React)

| Event | Payload |
|---|---|
| `connectivity:changed` | `{online}` |
| `job:updated` | `{jobId, queue, type, status, planId?, campaignId?, error?}` |
| `plan:status` | `{planId, articleNo, status}` |
| `campaign:progress` | `{campaignId, titles, content, images, wordpress, failed, percent}` |
| `campaign:status` | `{campaignId, status}` |
| `log:new` | ردیف `system_logs` |
| `notify` | `{kind: "campaign_done"|"errors", campaignId, text}` |

## ماشین حالت مقاله (`article_plans.status`)

```
PLANNED → TITLE_APPROVED → QUEUED → GENERATING → CONTENT_GENERATED
        → IMAGE_GENERATING → READY_TO_UPLOAD → UPLOADING → DRAFT_CREATED
هر مرحله ⇒ FAILED (با failed_stage) ⇄ retry ؛ هر نقطه ⇒ CANCELLED
```

## خروجی ساختاریافته‌ی AI (§26)

```json
{ "title": "", "slug": "", "excerpt": "", "content": "<html>", "meta_title": "",
  "meta_description": "", "faq": [{"q":"","a":""}], "image_prompt": "" }
```
اعتبارسنجی: JSON معتبر، عنوان/محتوا خالی نباشد، کلیدواژه‌ی هدف حاضر باشد، سلسله‌مراتب Hν درست، حداقل تعداد کلمات، FAQ در صورت فعال بودن، شباهت بیش از آستانه با مقالات دیگر کمپین ⇒ هشدار. شکست اعتبارسنجی ⇒ Retry تا `max_retries`.
