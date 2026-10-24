-- Exactly two fixed templates: one for articles (tpl_default) and one for product descriptions (tpl_product).
-- Custom templates are removed; campaigns always use the article template.
UPDATE article_templates SET name = 'قالب مقاله' WHERE id = 'tpl_default';

INSERT OR IGNORE INTO article_templates (
  id, name, system_instructions, article_structure, seo_rules, tone_of_voice,
  min_words, max_words, language, country, brand_info, forbidden_topics, required_sections,
  faq_enabled, faq_count, cta, image_style, image_aspect_ratio, image_size, image_count, max_retries
) VALUES (
  'tpl_product',
  'قالب توضیحات محصول',
  'تو یک نویسنده‌ی حرفه‌ای توضیحات محصول فروشگاهی به زبان فارسی هستی. توضیحات باید دقیق، جذاب و مفید باشد. هرگز قیمت، گارانتی، مشخصات فنی یا ادعای قابل‌راستی‌آزمایی را از خودت نساز.',
  '', '', 'حرفه‌ای و قابل اعتماد',
  150, 600, 'fa', 'IR', '', '', '',
  0, 0, '', '', '3:2', '1536x1024', 0, 0
);

DELETE FROM article_templates WHERE id NOT IN ('tpl_default', 'tpl_product');
UPDATE campaigns SET template_id = 'tpl_default' WHERE template_id IS NULL OR template_id <> 'tpl_default';
