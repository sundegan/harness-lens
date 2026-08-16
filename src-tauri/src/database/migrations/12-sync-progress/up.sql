ALTER TABLE provider_sync_state ADD COLUMN total_files INTEGER NOT NULL DEFAULT 0;
ALTER TABLE provider_sync_state ADD COLUMN processed_files INTEGER NOT NULL DEFAULT 0;
ALTER TABLE provider_sync_state ADD COLUMN processed_lines INTEGER NOT NULL DEFAULT 0;
ALTER TABLE provider_sync_state ADD COLUMN estimated_total_lines INTEGER;
ALTER TABLE provider_sync_state ADD COLUMN current_file TEXT;
ALTER TABLE provider_sync_state ADD COLUMN current_line INTEGER NOT NULL DEFAULT 0;
ALTER TABLE provider_sync_state ADD COLUMN estimated_remaining_ms INTEGER;
