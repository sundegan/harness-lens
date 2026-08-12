CREATE TABLE token_usage_records (
    id TEXT PRIMARY KEY NOT NULL,
    session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    invocation_id TEXT REFERENCES agent_invocations(id) ON DELETE SET NULL,
    source_path TEXT NOT NULL,
    timestamp_ms INTEGER,
    cumulative_tokens INTEGER,
    delta_tokens INTEGER
);

CREATE INDEX token_usage_records_session_idx
    ON token_usage_records(session_id);

CREATE INDEX token_usage_records_invocation_idx
    ON token_usage_records(invocation_id);

CREATE INDEX token_usage_records_source_idx
    ON token_usage_records(source_path);
