-- Normalized keyword text (Persian-aware) used for de-duplication within a campaign.
ALTER TABLE keywords ADD COLUMN norm TEXT;
CREATE UNIQUE INDEX uq_keywords_norm ON keywords(campaign_id, role, norm)
  WHERE norm IS NOT NULL AND campaign_id IS NOT NULL;
