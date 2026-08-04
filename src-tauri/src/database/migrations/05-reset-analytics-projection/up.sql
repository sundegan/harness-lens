-- The analytics projection is derived from coding-agent-data records. Recreate
-- its sync state without carrying the previous checkpoint forward so the
-- application performs a full, read-only rebuild after this migration.
DROP TABLE analytics_sync_state;

CREATE TABLE analytics_sync_state (
    provider TEXT PRIMARY KEY NOT NULL,
    checkpoint_json TEXT,
    status TEXT NOT NULL DEFAULT 'not_started',
    phase TEXT NOT NULL DEFAULT 'idle',
    processed_records INTEGER NOT NULL DEFAULT 0,
    diagnostic_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    updated_at_ms INTEGER NOT NULL
);
