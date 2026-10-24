-- Phase 4: per-article error text + a ready-to-use default article template.
ALTER TABLE article_plans ADD COLUMN last_error TEXT;

INSERT INTO article_templates (
  id, name, system_instructions, article_structure, seo_rules, tone_of_voice,
  min_words, max_words, language, country, brand_info, forbidden_topics, required_sections,
  faq_enabled, faq_count, cta, image_style, image_aspect_ratio, image_size, image_count, max_retries
) VALUES (
  'tpl_default',
  'قالب پیش‌فرض مقاله‌ی سئو',
  'تو یک نویسنده‌ی حرفه‌ای محتوای سئو به زبان فارسی هستی. مقاله باید دقیق، مفید، طبیعی و بدون حشو باشد. هرگز آمار، قیمت یا ادعای قابل‌راستی‌آزمایی را از خودت نساز؛ اگر مطمئن نیستی کلی‌تر بنویس.',
  'مقدمه‌ی کوتاه که مستقیماً به پرسش عنوان پاسخ می‌دهد
چند بخش اصلی با H2 (و در صورت نیاز زیرمجموعه‌ها با H3)
در صورت لزوم لیست یا جدول مقایسه
جمع‌بندی
فراخوان به اقدام (CTA)',
  'کلمه‌ی کلیدی ثانویه ({{secondary_keyword}}) باید در مقدمه و حداقل یک H2 به‌طور طبیعی بیاید. از تکرار بیش‌ازحد پرهیز کن. عنوان مقاله: {{title}}.',
  'صمیمی، حرفه‌ای و روان',
  800, 1600, 'fa', 'IR',
  '', '', 'مقدمه
جمع‌بندی',
  1, 5, '', 'تصویر شاخص مفهومی، سبک مینیمال و مدرن، رنگ‌های ملایم، بدون متن', '3:2', '1536x1024', 1, 2
);
