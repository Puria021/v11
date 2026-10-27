import { invoke } from "@tauri-apps/api/core";

// ---- types (mirror the Rust DTOs, camelCase) ----
export type AppError = { code: string; message: string; retryable: boolean; details?: unknown };

export type ProjectCard = {
  id: string;
  name: string;
  description: string;
  siteUrl: string;
  brandName: string;
  logoDataUrl: string;
  targetAudience: string;
  phone: string;
  defaultArticleCount: number;
  defaultCountry: string;
  defaultLanguage: string;
  lastActivityAt: string | null;
  defaultCategoryWpId: number | null;
  createdAt: string;
  updatedAt: string;
  wpStatus: "UNTESTED" | "CONNECTED" | "FAILED" | null;
  campaignCount: number;
  articleCount: number;
};

export type ProjectInput = {
  name: string;
  description: string;
  siteUrl: string;
  brandName: string;
  logoDataUrl: string;
  targetAudience: string;
  phone: string;
  defaultArticleCount: number;
  defaultCountry: string;
  defaultLanguage: string;
};

export type WpConnection = {
  projectId: string;
  apiUrl: string;
  username: string;
  proxyUrl: string | null;
  hasSecret: boolean;
  status: "UNTESTED" | "CONNECTED" | "FAILED";
  lastError: string | null;
  wpVersion: string | null;
  siteName: string | null;
  wpUserName: string | null;
  mediaCapable: boolean | null;
  lastTestedAt: string | null;
  lastSyncedAt: string | null;
};

export type SaveConnection = {
  projectId: string;
  apiUrl: string;
  username: string;
  applicationPassword?: string;
  proxyUrl?: string;
};

export type TestResult = { success: boolean; site: string; user: string; wpVersion: string | null; mediaCapable: boolean };
export type SyncResult = { categories: number; tags: number; authors: number; postTypes: number };
export type Term = { wpId: number; name: string; slug: string | null; parentWpId: number | null };

export type QuotaBucket = {
  /** Total allowed by the license (articles: 10, product descriptions: 5). */
  total: number;
  /** Already written. */
  used: number;
  /** total - used: what the user sees as "remaining". */
  remaining: number;
  /** Still available for new campaigns or single articles (remaining minus planned but unwritten articles). */
  available: number;
};
export type Quota = { articles: QuotaBucket; products: QuotaBucket };

export type DashboardStats = {
  projects: number;
  campaigns: number;
  keywordsResearched: number;
  keywordsApproved: number;
  articlePlans: number;
  articlesReady: number;
  articlesGenerating: number;
  draftsCreated: number;
  errors: number;
};

export type NetworkSettings = { proxyUrl: string; dnsOverrides: string };
export type AppInfo = { version: string; dataDir: string };
export type LicenseStatus = { activated: boolean };

export type AiProviderView = {
  id: string; name: string; kind: string; baseUrl: string | null; model: string;
  temperature: number; maxTokens: number; timeoutSec: number; proxyUrl: string | null;
  rpmLimit: number | null; maxConcurrent: number; hasSecret: boolean;
};
export type SaveAiProvider = {
  id?: string; name: string; kind: string; baseUrl?: string; model: string;
  temperature?: number; maxTokens?: number; timeoutSec?: number; proxyUrl?: string;
  rpmLimit?: number; maxConcurrent?: number; apiKey?: string;
};

export type CampaignView = {
  id: string; projectId: string; projectName: string; name: string; targetCount: number; status: string;
  wizardStep: number; country: string; language: string; aiProviderId: string | null;
  imageProviderId: string | null; templateId: string | null;
  createdAt: string; updatedAt: string;
  primaryCount: number; candidateCount: number; selectedCount: number; secondaryCount: number;
  planCount: number; approvedCount: number; completeCount: number;
};
export type CreateCampaign = {
  projectId: string; name: string; targetCount?: number; country?: string; language?: string; aiProviderId?: string;
};

export type Intent = "informational" | "commercial" | "transactional" | "navigational";
export type Keyword = {
  id: string; keyword: string; role: "primary" | "candidate" | "secondary"; source: "keyword_api" | "ai" | "user";
  searchVolume: number | null; competition: number | null; cpc: number | null;
  trend: number[] | null; related: string[]; intent: Intent | null; isSelected: boolean;
};
export type KeywordQuery = {
  campaignId: string; role?: string; search?: string; source?: string; selectedOnly?: boolean;
  sortBy?: string; sortDir?: "asc" | "desc"; page?: number; pageSize?: number;
};
export type KeywordPage = { items: Keyword[]; total: number; page: number; pageSize: number };
export type SimilarityWarning = {
  aId: string; aKeyword: string; bId: string; bKeyword: string; score: number; sameIntent: boolean; message: string;
};
export type SecondaryResult = { target: number; total: number; added: number; missing: number; rounds: number };

export type PlanView = {
  planId: string; articleNo: number; keywordId: string; keyword: string; intent: Intent | null; status: string;
  titleId: string | null; title: string | null; titleState: "SUGGESTED" | "APPROVED" | null; titleOrigin: "ai" | "manual" | null;
};
export type PlanList = { plans: PlanView[]; total: number; approved: number; pending: number; allApproved: boolean };
export type TitlesResult = { generated: number; missing: number[] };

export type Template = {
  id?: string | null; name: string; systemInstructions: string; articleStructure: string; seoRules: string;
  toneOfVoice: string; minWords: number; maxWords: number; language: string; country: string; brandInfo: string;
  forbiddenTopics: string; requiredSections: string; faqEnabled: boolean; faqCount: number; cta: string;
  imageStyle: string; imageAspectRatio: string; imageSize: string; imageCount: number; imageParagraphInterval: number;
  keywordDensityTarget: number | null; internalLinking: boolean; internalLinkRules: string; faqSchema: boolean; maxRetries: number;
};
export type ImageProviderView = {
  id: string; name: string; kind: string; baseUrl: string | null; model: string; timeoutSec: number;
  proxyUrl: string | null; rpmLimit: number | null; maxConcurrent: number; hasSecret: boolean;
  outputFormat: string; imageQuality: string; preferredSize: string;
};
export type SaveImageProvider = {
  id?: string; name: string; kind: string; baseUrl?: string; model: string; timeoutSec?: number;
  proxyUrl?: string; rpmLimit?: number; maxConcurrent?: number; apiKey?: string;
  outputFormat?: string; imageQuality?: string; preferredSize?: string;
};
export type Stage = { done: number; total: number };
export type Progress = {
  campaignId: string; campaignStatus: string; titles: Stage; content: Stage; images: Stage; wordpress: Stage;
  imagesEnabled: boolean; failed: number; running: number; queued: number; percent: number;
  /** Articles whose text AND images are ready (counted as built). */
  complete: number;
};
export type ArticleRow = {
  planId: string; articleNo: number; keyword: string; title: string | null; status: string;
  failedStage: string | null; lastError: string | null; wordCount: number | null; hasImage: number;
  categoryIds: number[]; tags: string[]; uploadStatus: string | null; wordpressPostId: number | null; wordpressUrl: string | null; uploadAt: string | null;
  /** Remote WordPress status: "draft", "future", "publish" (null before upload). */
  wpStatus: string | null;
};
export type SingleInput = {
  projectId: string; keyword: string; imageCount: number; externalUrl: string | null; externalTitle: string | null;
  externalFollow: boolean; cta: string | null;
};
export type SingleCreated = { campaignId: string; planId: string };
export type TitleOption = { id: string; title: string };
export type ArticlePatch = {
  title: string; slug: string; excerpt: string; contentHtml: string; metaTitle: string; metaDescription: string;
  faq: { q: string; a: string }[];
};
export type LogQuery = {
  projectId?: string; campaignId?: string; articleNo?: number; level?: string; status?: string; action?: string;
  search?: string; from?: string; to?: string; page?: number; pageSize?: number;
};
export type LogRow = {
  id: number; at: string; level: string; projectName: string | null; campaignName: string | null; articleNo: number | null;
  action: string; status: string; message: string | null;
};
export type LogPage = { items: LogRow[]; total: number; page: number; pageSize: number };
export type BackupPreview = {
  createdAt: string; appVersion: string; scope: string;
  projects: { id: string; name: string; campaigns: number; articles: number; exists: boolean }[];
  templates: number; aiProviders: number; imageProviders: number; images: number;
};
export type ExportResult = { path: string; bytes: number; projects: number; campaigns: number; articles: number; images: number };
export type ImportResult = { projectsImported: number; projectsSkipped: string[]; campaigns: number; articles: number; files: number };
export type MetricsConfig = { configured: boolean; locationCode: number | null; languageCode: string };
export type MetricsResult = { requested: number; updated: number; withoutData: number; costUsd: number };
export type ProductItem = { id: number; name: string; sku: string; description: string; shortDescription: string; permalink: string };
export type ProductPage = { items: ProductItem[]; total: number; pages: number };

export type ArticleDetail = {
  planId: string; articleNo: number; keyword: string; status: string; title: string; slug: string; excerpt: string;
  contentHtml: string; metaTitle: string; metaDescription: string; faq: { q: string; a: string }[];
  imagePrompt: string; wordCount: number; validation: { warnings?: string[]; maxSimilarity?: number } | null; version: number;
};

// ---- commands ----
export const api = {
  getMetricsConfig: () => invoke<MetricsConfig>("get_metrics_config"),
  saveMetricsConfig: (input: { login?: string; password?: string; locationCode: number | null; languageCode: string }) => invoke<MetricsConfig>("save_metrics_config", { input }),
  fetchKeywordMetrics: (campaignId: string) => invoke<MetricsResult>("fetch_keyword_metrics", { campaignId }),
  listProducts: (projectId: string, search: string, page: number) => invoke<ProductPage>("list_products", { projectId, search, page }),
  generateProductDescription: (providerId: string, name: string, keyword: string, existing: string, language: string) => invoke<string>("generate_product_description", { providerId, name, keyword, existing, language }),
  saveProductDescription: (projectId: string, productId: number, description: string) => invoke<void>("save_product_description", { projectId, productId, description }),
  getNetworkSettings: () => invoke<NetworkSettings>("get_network_settings"),
  saveNetworkSettings: (settings: NetworkSettings) => invoke<NetworkSettings>("save_network_settings", { settings }),
  getAppInfo: () => invoke<AppInfo>("get_app_info"),
  getLicenseStatus: () => invoke<LicenseStatus>("get_license_status"),
  activateLicense: (apiKey: string) => invoke<LicenseStatus>("activate_license", { apiKey }),
  checkInternet: () => invoke<boolean>("check_internet"),
  getDashboardStats: () => invoke<DashboardStats>("get_dashboard_stats"),
  getQuota: () => invoke<Quota>("get_quota"),
  createSingleArticle: (input: SingleInput) => invoke<SingleCreated>("create_single_article", { input }),
  suggestSingleTitles: (campaignId: string) => invoke<TitleOption[]>("suggest_single_titles", { campaignId }),

  resetApplication: (confirmation: string) => invoke<void>("reset_application", { confirmation }),
  listProjects: () => invoke<ProjectCard[]>("list_projects"),
  getProject: (id: string) => invoke<ProjectCard>("get_project", { id }),
  createProject: (input: ProjectInput) => invoke<ProjectCard>("create_project", { input }),
  updateProject: (id: string, input: ProjectInput) => invoke<ProjectCard>("update_project", { id, input }),
  deleteProject: (id: string) => invoke<void>("delete_project", { id, confirm: true }),

  getWpConnection: (projectId: string) => invoke<WpConnection | null>("get_wordpress_connection", { projectId }),
  saveWpConnection: (input: SaveConnection) => invoke<WpConnection>("save_wordpress_connection", { input }),
  testWpConnection: (projectId: string) => invoke<TestResult>("test_wordpress_connection", { projectId }),
  syncWpTaxonomies: (projectId: string) => invoke<SyncResult>("sync_wordpress_taxonomies", { projectId }),
  listWpTerms: (projectId: string, kind: "category" | "tag" | "author" | "post_type") =>
    invoke<Term[]>("list_wp_terms", { projectId, kind }),

  listAiProviders: () => invoke<AiProviderView[]>("list_ai_providers"),
  saveAiProvider: (input: SaveAiProvider) => invoke<AiProviderView>("save_ai_provider", { input }),
  deleteAiProvider: (id: string) => invoke<void>("delete_ai_provider", { id }),
  testAiProvider: (id: string) => invoke<{ ok: boolean; latencyMs: number }>("test_ai_provider", { id }),

  listCampaigns: (projectId?: string) => invoke<CampaignView[]>("list_campaigns", { projectId }),
  getCampaign: (id: string) => invoke<CampaignView>("get_campaign", { id }),
  createCampaign: (input: CreateCampaign) => invoke<CampaignView>("create_campaign", { input }),
  deleteCampaign: (id: string) => invoke<void>("delete_campaign", { id, confirm: true }),
  saveWizardState: (campaignId: string, step: number, data: unknown) =>
    invoke<void>("save_wizard_state", { campaignId, step, data }),
  setCampaignAiProvider: (campaignId: string, providerId: string | null) =>
    invoke<CampaignView>("set_campaign_ai_provider", { campaignId, providerId }),

  savePrimaryKeywords: (campaignId: string, seeds: string[]) => invoke<Keyword[]>("save_primary_keywords", { campaignId, seeds }),
  importKeywordsCsv: (campaignId: string, csv: string) =>
    invoke<{ imported: number; updated: number; skipped: number }>("import_keywords_csv", { campaignId, csv }),
  addSecondaryKeyword: (campaignId: string, keyword: string) => invoke<Keyword>("add_secondary_keyword", { campaignId, keyword }),
  editCampaignKeyword: (campaignId: string, id: string, keyword: string) => invoke<void>("edit_campaign_keyword", { campaignId, id, keyword }),
  addManualKeyword: (campaignId: string, keyword: string) => invoke<Keyword>("add_manual_keyword", { campaignId, keyword }),
  aiSuggestKeywords: (campaignId: string, count?: number) =>
    invoke<{ requested: number; added: number }>("ai_suggest_keywords", { campaignId, count }),
  listKeywords: (query: KeywordQuery) => invoke<KeywordPage>("list_keywords", { query }),
  selectKeywords: (campaignId: string, ids: string[], selected: boolean) =>
    invoke<number>("select_keywords", { campaignId, ids, selected }),
  deleteKeywords: (campaignId: string, ids: string[]) => invoke<number>("delete_keywords", { campaignId, ids }),
  getSimilarityWarnings: (campaignId: string) => invoke<SimilarityWarning[]>("get_similarity_warnings", { campaignId }),

  generateSecondaryKeywords: (campaignId: string, replace: boolean) =>
    invoke<SecondaryResult>("generate_secondary_keywords", { campaignId, replace }),
  confirmSecondaryKeywords: (campaignId: string) => invoke<number>("confirm_secondary_keywords", { campaignId }),
  resetArticlePlans: (campaignId: string) => invoke<void>("reset_article_plans", { campaignId, confirm: true }),

  generateTitles: (campaignId: string) => invoke<TitlesResult>("generate_titles", { campaignId }),
  listArticlePlans: (campaignId: string) => invoke<PlanList>("list_article_plans", { campaignId }),
  approveTitle: (planId: string, titleId: string) => invoke<void>("approve_title", { planId, titleId }),
  editTitle: (planId: string, title: string) => invoke<unknown>("edit_title", { planId, title }),
  rejectTitle: (planId: string, titleId: string) => invoke<void>("reject_title", { planId, titleId }),
  generateAlternativeTitle: (planId: string) => invoke<unknown>("generate_alternative_title", { planId }),
  approveAllTitles: (campaignId: string) => invoke<number>("approve_all_titles", { campaignId }),

  listTemplates: () => invoke<Template[]>("list_templates"),
  saveTemplate: (template: Template) => invoke<Template>("save_template", { template }),
  duplicateTemplate: (id: string) => invoke<Template>("duplicate_template", { id }),
  deleteTemplate: (id: string) => invoke<void>("delete_template", { id }),

  listImageProviders: () => invoke<ImageProviderView[]>("list_image_providers"),
  saveImageProvider: (input: SaveImageProvider) => invoke<ImageProviderView>("save_image_provider", { input }),
  deleteImageProvider: (id: string) => invoke<void>("delete_image_provider", { id }),
  testImageProvider: (id: string) => invoke<string>("test_image_provider", { id }),

  setCampaignTemplate: (campaignId: string, templateId: string | null) =>
    invoke<CampaignView>("set_campaign_template", { campaignId, templateId }),
  setCampaignImageProvider: (campaignId: string, providerId: string | null) =>
    invoke<CampaignView>("set_campaign_image_provider", { campaignId, providerId }),
  startGeneration: (campaignId: string) => invoke<{ queued: number }>("start_generation", { campaignId, confirm: true }),
  pauseCampaign: (campaignId: string) => invoke<void>("pause_campaign", { campaignId }),
  resumeCampaign: (campaignId: string) => invoke<void>("resume_campaign", { campaignId }),
  cancelCampaign: (campaignId: string) => invoke<void>("cancel_campaign", { campaignId }),
  getProgress: (campaignId: string) => invoke<Progress>("get_progress", { campaignId }),
  listArticles: (campaignId: string) => invoke<ArticleRow[]>("list_articles", { campaignId }),
  getArticle: (planId: string) => invoke<ArticleDetail>("get_article", { planId }),
  getArticleImage: (planId: string) => invoke<string | null>("get_article_image", { planId }),
  getArticlePreviewImages: (planId: string) => invoke<{ images: { src: string; alt: string; isFeatured: boolean }[]; paragraphInterval: number }>("get_article_preview_images", { planId }),
  retryArticle: (planId: string, stage: "content" | "image") => invoke<void>("retry_article", { planId, stage }),
  retryFailed: (campaignId: string) => invoke<number>("retry_failed", { campaignId }),
  getQueueSettings: () => invoke<{ content: number; image: number; wordpress: number }>("get_queue_settings"),
  updateQueueSettings: (content: number, image: number, wordpress: number) =>
    invoke<{ content: number; image: number; wordpress: number }>("update_queue_settings", { content, image, wordpress }),

  setUploadSchedule: (campaignId: string, planId: string | null, uploadAt: string | null) => invoke<number>("set_upload_schedule", { campaignId, planId, uploadAt }),
  uploadToWordpress: (planId: string, publishNow = false) => invoke<void>("upload_to_wordpress", { planId, publishNow }),
  clearCache: () => invoke<{ filesRemoved: number; bytesFreed: number; keptPending: number }>("clear_cache"),
  uploadCampaign: (campaignId: string, publishNow = false) => invoke<number>("upload_campaign", { campaignId, confirm: true, publishNow }),
  publishUploadedNow: (planId: string) => invoke<void>("publish_uploaded_now", { planId }),
  setPlanTaxonomy: (planId: string, categoryIds: number[], tags: string[]) =>
    invoke<void>("set_plan_taxonomy", { planId, categoryIds, tags }),
  setDefaultCategory: (projectId: string, wpId: number | null) => invoke<void>("set_default_category", { projectId, wpId }),
  updateArticle: (planId: string, patch: ArticlePatch, expectedVersion: number) =>
    invoke<ArticleDetail>("update_article", { planId, patch, expectedVersion }),
  previewHtml: (html: string) => invoke<string>("preview_html", { html }),

  listLogs: (query: LogQuery) => invoke<LogPage>("list_logs", { query }),
  listLogActions: () => invoke<string[]>("list_log_actions"),
  exportLogs: (query: LogQuery) => invoke<string>("export_logs", { query }),
  exportBackup: (path: string | null, projectId: string | null) => invoke<ExportResult>("export_backup", { path, projectId }),
  previewBackup: (path: string) => invoke<BackupPreview>("preview_backup", { path }),
  importBackup: (path: string, mode: "skip" | "replace") => invoke<ImportResult>("import_backup", { path, mode }),
};
