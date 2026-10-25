-- Direct publish flag: when set, the WordPress post is created with status "publish" instead of "draft".
-- article_plans.upload_at now means the WordPress publish date (sent as date_gmt), not the upload time.
ALTER TABLE article_plans ADD COLUMN publish_now INTEGER NOT NULL DEFAULT 0;
