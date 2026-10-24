-- Image output options per provider. Existing providers keep their behavior by default.
ALTER TABLE image_providers ADD COLUMN output_format TEXT NOT NULL DEFAULT 'png';
ALTER TABLE image_providers ADD COLUMN image_quality TEXT NOT NULL DEFAULT 'auto';
ALTER TABLE image_providers ADD COLUMN preferred_size TEXT NOT NULL DEFAULT 'template';
