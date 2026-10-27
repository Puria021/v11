-- Remote WordPress status of an uploaded article: draft, future or publish. Lets the UI show a published article.
ALTER TABLE wordpress_posts ADD COLUMN wp_status TEXT;
