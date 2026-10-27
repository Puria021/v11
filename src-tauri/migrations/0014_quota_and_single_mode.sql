-- Local license quota counters (10 articles, 5 product descriptions per license). Reset when a license is activated.
CREATE TABLE IF NOT EXISTS quota_usage (
  id              INTEGER PRIMARY KEY CHECK (id = 1),
  article_total   INTEGER NOT NULL DEFAULT 10,
  product_total   INTEGER NOT NULL DEFAULT 5,
  articles_used   INTEGER NOT NULL DEFAULT 0,
  products_used   INTEGER NOT NULL DEFAULT 0,
  epoch           INTEGER NOT NULL DEFAULT 0
);
INSERT OR IGNORE INTO quota_usage (id) VALUES (1);

-- Campaigns created by the single-article tab are campaigns of one article with their own options.
ALTER TABLE campaigns ADD COLUMN kind TEXT NOT NULL DEFAULT 'campaign';
ALTER TABLE campaigns ADD COLUMN image_count INTEGER;
ALTER TABLE campaigns ADD COLUMN external_url TEXT;
ALTER TABLE campaigns ADD COLUMN external_title TEXT;
ALTER TABLE campaigns ADD COLUMN external_follow INTEGER NOT NULL DEFAULT 0;
ALTER TABLE campaigns ADD COLUMN cta_text TEXT;
ALTER TABLE campaigns ADD COLUMN quota_epoch INTEGER NOT NULL DEFAULT 0;
