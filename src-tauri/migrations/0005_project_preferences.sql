ALTER TABLE projects ADD COLUMN phone TEXT NOT NULL DEFAULT '';
ALTER TABLE projects ADD COLUMN default_article_count INTEGER NOT NULL DEFAULT 60;
ALTER TABLE article_templates ADD COLUMN image_paragraph_interval INTEGER NOT NULL DEFAULT 3;
UPDATE article_templates SET faq_schema = 1 WHERE faq_enabled = 1;
