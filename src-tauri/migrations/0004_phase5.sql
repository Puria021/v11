-- Phase 5: one-shot completion notification flag per run.
ALTER TABLE campaigns ADD COLUMN notified INTEGER NOT NULL DEFAULT 0;
