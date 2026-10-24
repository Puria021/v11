-- MyArticleManager — migration 0001 (SQLite)
-- IDs: TEXT (ULID/UUID). Timestamps: ISO-8601 UTC text. Booleans: INTEGER 0/1.
-- Secrets are NEVER stored here; only secret_ref (key name in Windows Credential Manager).

PRAGMA foreign_keys = ON;

------------------------------------------------------------------ users (local)
CREATE TABLE users (
  id            TEXT PRIMARY KEY,
  display_name  TEXT NOT NULL,
  password_hash TEXT,                       -- Argon2id, optional app lock
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

------------------------------------------------------------------ providers
CREATE TABLE ai_providers (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('openai','anthropic','gemini','openai_compatible')),
  base_url      TEXT,
  model         TEXT NOT NULL,
  temperature   REAL NOT NULL DEFAULT 0.7,
  max_tokens    INTEGER NOT NULL DEFAULT 4000,
  timeout_sec   INTEGER NOT NULL DEFAULT 120,
  proxy_url     TEXT,
  secret_ref    TEXT,                       -- credential key
  rpm_limit     INTEGER,                    -- requests / minute
  rps_limit     INTEGER,
  max_concurrent INTEGER NOT NULL DEFAULT 3,
  is_active     INTEGER NOT NULL DEFAULT 1,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE keyword_providers (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('manual_csv','dataforseo','ai_suggest','custom')),
  base_url      TEXT,
  config_json   TEXT NOT NULL DEFAULT '{}', -- non-secret options
  secret_ref    TEXT,
  proxy_url     TEXT,
  rpm_limit     INTEGER,
  max_concurrent INTEGER NOT NULL DEFAULT 2,
  is_active     INTEGER NOT NULL DEFAULT 1,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE image_providers (
  id            TEXT PRIMARY KEY,
  name          TEXT NOT NULL,
  kind          TEXT NOT NULL CHECK (kind IN ('openai_image','gemini_image','custom')),
  base_url      TEXT,
  model         TEXT NOT NULL,
  timeout_sec   INTEGER NOT NULL DEFAULT 180,
  proxy_url     TEXT,
  secret_ref    TEXT,
  rpm_limit     INTEGER,
  max_concurrent INTEGER NOT NULL DEFAULT 2,
  is_active     INTEGER NOT NULL DEFAULT 1,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

------------------------------------------------------------------ templates
CREATE TABLE article_templates (
  id              TEXT PRIMARY KEY,
  name            TEXT NOT NULL,
  system_instructions TEXT NOT NULL DEFAULT '',
  article_structure   TEXT NOT NULL DEFAULT '',   -- supports {{placeholders}}
  seo_rules       TEXT NOT NULL DEFAULT '',
  tone_of_voice   TEXT NOT NULL DEFAULT '',
  min_words       INTEGER NOT NULL DEFAULT 800,
  max_words       INTEGER NOT NULL DEFAULT 2000,
  language        TEXT NOT NULL DEFAULT 'fa',
  country         TEXT NOT NULL DEFAULT 'IR',
  brand_info      TEXT NOT NULL DEFAULT '',
  forbidden_topics TEXT NOT NULL DEFAULT '',
  required_sections TEXT NOT NULL DEFAULT '',
  faq_enabled     INTEGER NOT NULL DEFAULT 1,
  faq_count       INTEGER NOT NULL DEFAULT 5,
  cta             TEXT NOT NULL DEFAULT '',
  image_style     TEXT NOT NULL DEFAULT '',
  image_aspect_ratio TEXT NOT NULL DEFAULT '16:9',
  image_size      TEXT NOT NULL DEFAULT '1536x864',
  image_count     INTEGER NOT NULL DEFAULT 1,
  keyword_density_target REAL,
  internal_linking INTEGER NOT NULL DEFAULT 0,
  faq_schema      INTEGER NOT NULL DEFAULT 0,
  max_retries     INTEGER NOT NULL DEFAULT 2,
  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

------------------------------------------------------------------ projects
CREATE TABLE projects (
  id              TEXT PRIMARY KEY,
  user_id         TEXT REFERENCES users(id) ON DELETE SET NULL,
  name            TEXT NOT NULL,
  description     TEXT NOT NULL DEFAULT '',
  site_url        TEXT NOT NULL,
  brand_name      TEXT NOT NULL DEFAULT '',
  target_audience TEXT NOT NULL DEFAULT '',
  default_template_id TEXT REFERENCES article_templates(id) ON DELETE SET NULL,
  ai_provider_id  TEXT REFERENCES ai_providers(id) ON DELETE SET NULL,
  keyword_provider_id TEXT REFERENCES keyword_providers(id) ON DELETE SET NULL,
  image_provider_id TEXT REFERENCES image_providers(id) ON DELETE SET NULL,
  default_country  TEXT NOT NULL DEFAULT 'IR',
  default_language TEXT NOT NULL DEFAULT 'fa',
  default_category_wp_id INTEGER,
  last_activity_at TEXT,
  created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

-- v1: one WordPress site per project (UNIQUE project_id). Drop the UNIQUE later for multi-site.
CREATE TABLE wordpress_connections (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL UNIQUE REFERENCES projects(id) ON DELETE CASCADE,
  api_url       TEXT NOT NULL,              -- e.g. https://site.com/wp-json
  username      TEXT NOT NULL,
  secret_ref    TEXT NOT NULL,              -- Application Password in Credential Manager
  proxy_url     TEXT,
  status        TEXT NOT NULL DEFAULT 'UNTESTED' CHECK (status IN ('UNTESTED','CONNECTED','FAILED')),
  last_error    TEXT,
  wp_version    TEXT,
  site_name     TEXT,
  wp_user_name  TEXT,
  media_capable INTEGER,
  last_tested_at TEXT,
  last_synced_at TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE wp_terms (                      -- cache of categories / tags / authors
  id          TEXT PRIMARY KEY,
  project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  kind        TEXT NOT NULL CHECK (kind IN ('category','tag','author','post_type')),
  wp_id       INTEGER NOT NULL,
  name        TEXT NOT NULL,
  slug        TEXT,
  parent_wp_id INTEGER,
  UNIQUE (project_id, kind, wp_id)
);

------------------------------------------------------------------ campaigns
CREATE TABLE campaigns (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  name          TEXT NOT NULL,
  target_count  INTEGER NOT NULL DEFAULT 60 CHECK (target_count BETWEEN 1 AND 200),
  template_id   TEXT REFERENCES article_templates(id) ON DELETE SET NULL,
  ai_provider_id TEXT REFERENCES ai_providers(id) ON DELETE SET NULL,
  image_provider_id TEXT REFERENCES image_providers(id) ON DELETE SET NULL,
  country       TEXT NOT NULL DEFAULT 'IR',
  language      TEXT NOT NULL DEFAULT 'fa',
  status        TEXT NOT NULL DEFAULT 'DRAFT'
                CHECK (status IN ('DRAFT','IN_PROGRESS','PAUSED','COMPLETED','CANCELLED')),
  wizard_step   INTEGER NOT NULL DEFAULT 1 CHECK (wizard_step BETWEEN 1 AND 14),
  wizard_state_json TEXT NOT NULL DEFAULT '{}',   -- resumable wizard
  generation_started_at TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_campaigns_project ON campaigns(project_id);

------------------------------------------------------------------ keywords
CREATE TABLE keyword_researches (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  campaign_id   TEXT REFERENCES campaigns(id) ON DELETE CASCADE,
  provider_id   TEXT REFERENCES keyword_providers(id) ON DELETE SET NULL,
  seed_keywords_json TEXT NOT NULL,
  country       TEXT NOT NULL,
  language      TEXT NOT NULL,
  status        TEXT NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING','RUNNING','DONE','FAILED')),
  error         TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE keywords (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  campaign_id   TEXT REFERENCES campaigns(id) ON DELETE CASCADE,
  research_id   TEXT REFERENCES keyword_researches(id) ON DELETE SET NULL,
  keyword       TEXT NOT NULL,
  role          TEXT NOT NULL DEFAULT 'candidate' CHECK (role IN ('primary','candidate','secondary')),
  source        TEXT NOT NULL CHECK (source IN ('keyword_api','ai','user')),
  -- Real metrics: ONLY from a KeywordProvider. AI-sourced rows must keep these NULL.
  search_volume INTEGER,
  competition   REAL,
  cpc           REAL,
  trend_json    TEXT,
  related_json  TEXT,
  country       TEXT,
  language      TEXT,
  intent        TEXT CHECK (intent IN ('informational','commercial','transactional','navigational')),
  is_selected   INTEGER NOT NULL DEFAULT 0,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  CHECK (source <> 'ai' OR (search_volume IS NULL AND competition IS NULL AND cpc IS NULL)),
  UNIQUE (campaign_id, keyword, role)
);
CREATE INDEX idx_keywords_project ON keywords(project_id);
CREATE INDEX idx_keywords_campaign ON keywords(campaign_id, role);

------------------------------------------------------------------ article plans
CREATE TABLE article_plans (
  id            TEXT PRIMARY KEY,
  project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  campaign_id   TEXT NOT NULL REFERENCES campaigns(id) ON DELETE CASCADE,
  article_no    INTEGER NOT NULL CHECK (article_no >= 1),
  keyword_id    TEXT NOT NULL REFERENCES keywords(id) ON DELETE RESTRICT,
  primary_keyword_id TEXT REFERENCES keywords(id) ON DELETE SET NULL,
  intent        TEXT CHECK (intent IN ('informational','commercial','transactional','navigational')),
  status        TEXT NOT NULL DEFAULT 'PLANNED' CHECK (status IN (
                  'PLANNED','TITLE_APPROVED','QUEUED','GENERATING','CONTENT_GENERATED',
                  'IMAGE_GENERATING','READY_TO_UPLOAD','UPLOADING','DRAFT_CREATED','FAILED','CANCELLED')),
  failed_stage  TEXT,                      -- content | image | upload
  similarity_warning TEXT,                 -- JSON: [{other_article_no, score}]
  wp_category_ids_json TEXT NOT NULL DEFAULT '[]',
  tags_json     TEXT NOT NULL DEFAULT '[]',
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  UNIQUE (campaign_id, article_no),
  UNIQUE (campaign_id, keyword_id)
);
CREATE INDEX idx_plans_campaign_status ON article_plans(campaign_id, status);

CREATE TABLE article_titles (
  id            TEXT PRIMARY KEY,
  plan_id       TEXT NOT NULL REFERENCES article_plans(id) ON DELETE CASCADE,
  title         TEXT NOT NULL,
  origin        TEXT NOT NULL CHECK (origin IN ('ai','manual')),
  state         TEXT NOT NULL DEFAULT 'SUGGESTED' CHECK (state IN ('SUGGESTED','APPROVED','REJECTED','SUPERSEDED')),
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
-- at most one APPROVED title per plan
CREATE UNIQUE INDEX uq_titles_one_approved ON article_titles(plan_id) WHERE state = 'APPROVED';
CREATE INDEX idx_titles_plan ON article_titles(plan_id);

------------------------------------------------------------------ content / images
CREATE TABLE article_contents (
  id            TEXT PRIMARY KEY,
  plan_id       TEXT NOT NULL UNIQUE REFERENCES article_plans(id) ON DELETE CASCADE,
  title         TEXT NOT NULL,
  slug          TEXT NOT NULL,
  excerpt       TEXT NOT NULL DEFAULT '',
  content_html  TEXT NOT NULL,             -- sanitized
  meta_title    TEXT NOT NULL DEFAULT '',
  meta_description TEXT NOT NULL DEFAULT '',
  faq_json      TEXT NOT NULL DEFAULT '[]',
  image_prompt  TEXT NOT NULL DEFAULT '',
  word_count    INTEGER NOT NULL DEFAULT 0,
  validation_json TEXT,                    -- QC result
  raw_ai_json   TEXT,                      -- original structured output
  provider_id   TEXT REFERENCES ai_providers(id) ON DELETE SET NULL,
  model         TEXT,
  edited_by_user INTEGER NOT NULL DEFAULT 0,
  version       INTEGER NOT NULL DEFAULT 1,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);

CREATE TABLE article_images (
  id            TEXT PRIMARY KEY,
  plan_id       TEXT NOT NULL REFERENCES article_plans(id) ON DELETE CASCADE,
  is_featured   INTEGER NOT NULL DEFAULT 1,
  prompt        TEXT NOT NULL,
  local_path    TEXT,                      -- relative to app data dir
  mime_type     TEXT,
  width         INTEGER,
  height        INTEGER,
  alt_text      TEXT NOT NULL DEFAULT '',
  caption       TEXT NOT NULL DEFAULT '',
  wp_media_id   INTEGER,
  wp_media_url  TEXT,
  provider_id   TEXT REFERENCES image_providers(id) ON DELETE SET NULL,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_images_plan ON article_images(plan_id);

------------------------------------------------------------------ WordPress posts (duplicate guard)
CREATE TABLE wordpress_posts (
  id            TEXT PRIMARY KEY,
  plan_id       TEXT NOT NULL UNIQUE REFERENCES article_plans(id) ON DELETE CASCADE,
  project_id    TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
  upload_status TEXT NOT NULL DEFAULT 'NOT_UPLOADED'
                CHECK (upload_status IN ('NOT_UPLOADED','UPLOADING','UPLOADED','FAILED')),
  wordpress_post_id INTEGER,
  wordpress_url TEXT,
  idempotency_key TEXT NOT NULL,           -- sent to WP; used by find_post_by_meta on retry
  uploaded_at   TEXT,
  last_error    TEXT
);
CREATE UNIQUE INDEX uq_wp_posts_remote ON wordpress_posts(project_id, wordpress_post_id)
  WHERE wordpress_post_id IS NOT NULL;

------------------------------------------------------------------ queue
CREATE TABLE jobs (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  queue         TEXT NOT NULL CHECK (queue IN ('keyword','content','image','wordpress')),
  type          TEXT NOT NULL,             -- keyword_research | gen_secondary | gen_titles | gen_content | gen_image | upload_wp ...
  project_id    TEXT REFERENCES projects(id) ON DELETE CASCADE,
  campaign_id   TEXT REFERENCES campaigns(id) ON DELETE CASCADE,
  plan_id       TEXT REFERENCES article_plans(id) ON DELETE CASCADE,
  status        TEXT NOT NULL DEFAULT 'PENDING' CHECK (status IN (
                  'PENDING','PROCESSING','RETRY_PENDING','PAUSED','DONE','FAILED','CANCELLED')),
  payload       TEXT NOT NULL DEFAULT '{}',
  priority      INTEGER NOT NULL DEFAULT 0,
  retry_count   INTEGER NOT NULL DEFAULT 0,
  max_retries   INTEGER NOT NULL DEFAULT 3,
  run_after     TEXT,                      -- backoff schedule
  dedupe_key    TEXT,
  error_kind    TEXT,
  error         TEXT,
  created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  started_at    TEXT,
  completed_at  TEXT
);
CREATE INDEX idx_jobs_pick ON jobs(queue, status, run_after, priority DESC, id);
CREATE INDEX idx_jobs_campaign ON jobs(campaign_id, status);
-- only one active job per dedupe_key
CREATE UNIQUE INDEX uq_jobs_active_dedupe ON jobs(dedupe_key)
  WHERE dedupe_key IS NOT NULL AND status IN ('PENDING','PROCESSING','RETRY_PENDING','PAUSED');

CREATE TABLE job_logs (
  id        INTEGER PRIMARY KEY AUTOINCREMENT,
  job_id    INTEGER NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
  level     TEXT NOT NULL CHECK (level IN ('DEBUG','INFO','WARN','ERROR')),
  message   TEXT NOT NULL,
  at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_job_logs_job ON job_logs(job_id);

------------------------------------------------------------------ logs & settings
CREATE TABLE system_logs (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
  level       TEXT NOT NULL CHECK (level IN ('DEBUG','INFO','WARN','ERROR')),
  project_id  TEXT REFERENCES projects(id) ON DELETE SET NULL,
  campaign_id TEXT REFERENCES campaigns(id) ON DELETE SET NULL,
  plan_id     TEXT REFERENCES article_plans(id) ON DELETE SET NULL,
  provider    TEXT,
  action      TEXT NOT NULL,
  status      TEXT NOT NULL CHECK (status IN ('SUCCESS','FAILED','INFO','WARN')),
  message     TEXT,                        -- already redacted
  meta_json   TEXT
);
CREATE INDEX idx_logs_at ON system_logs(at DESC);
CREATE INDEX idx_logs_filter ON system_logs(project_id, level, at DESC);

CREATE TABLE settings (
  key         TEXT PRIMARY KEY,
  value_json  TEXT NOT NULL,
  updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
-- default keys: queue.concurrency.{content,image,wordpress}, net.proxy, ui.theme,
-- notifications.enabled, storage.root, updates.channel

INSERT INTO settings(key, value_json) VALUES
  ('queue.concurrency.content',  '3'),
  ('queue.concurrency.image',    '2'),
  ('queue.concurrency.wordpress','3'),
  ('ui.theme',                   '"light"'),
  ('notifications.enabled',      'true');
