-- Default content for the article template (tpl_default). Applied only to the untouched factory template.
UPDATE article_templates SET
  system_instructions = 'تو یک نویسنده‌ی حرفه‌ای محتوای سئو به زبان فارسی هستی. مقاله باید دقیق، مفید، طبیعی و بدون حشو باشد. هرگز آمار، قیمت یا ادعای قابل‌راستی‌آزمایی را از خودت نساز؛ اگر مطمئن نیستی کلی‌تر بنویس.',
  article_structure = 'مقدمه‌ای کوتاه که مستقیماً به پرسش عنوان پاسخ می‌دهد
چند بخش اصلی با H2 (و در صورت نیاز زیرمجموعه‌ها با H3)
وجود جدول مرتبط با موضوع مقاله الزامی است.
جمع‌بندی
فراخوان به اقدام (CTA)',
  seo_rules = 'کلمه‌ی کلیدی ثانویه ({{secondary_keyword}}) باید در مقدمه و حداقل یک H2 به‌طور طبیعی بیاید. عنوان مقاله: {{title}}.
در 150 کلمه اول حتماً از {{primary_keyword}} و {{secondary_keyword}} استفاده کن.
از پاراگراف‌های طولانی پرهیز کن.
از {{brand_name}} برای دادن لینک داخلی به صفحه اصلی سایت استفاده کن.',
  tone_of_voice = 'صمیمی، حرفه‌ای و روان',
  brand_info = '{{brand_name}}',
  required_sections = 'مقدمه
جمع‌بندی',
  min_words = 2000,
  max_words = 3000,
  max_retries = 2,
  faq_enabled = 1,
  faq_count = 5,
  faq_schema = 1,
  image_count = 3,
  image_paragraph_interval = 7,
  image_size = '1536x1024',
  image_style = 'تصویر شاخص مفهومی، سبک مینیمال و مدرن، رنگ‌های ملایم، بدون متن',
  internal_linking = 1,
  internal_link_rules = 'فقط به URLهای واقعی‌ای که برنامه به صراحت ارائه کرده لینک بده. URL یا نامک را هرگز حدس نزن. اگر لینک تأییدشده نداریم، لینک داخلی نگذار.',
  updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
-- Only while the template still has the factory values; any change made by the user keeps it as is.
WHERE id = 'tpl_default' AND min_words = 800 AND max_words = 1600 AND image_count = 1 AND internal_linking = 0;
