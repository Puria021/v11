-- Reuses the existing background queue's run_after timestamp.
ALTER TABLE article_plans ADD COLUMN upload_at TEXT;
