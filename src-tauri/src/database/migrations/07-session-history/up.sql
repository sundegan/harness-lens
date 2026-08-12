ALTER TABLE agent_sessions ADD COLUMN model TEXT;
ALTER TABLE agent_sessions ADD COLUMN model_provider TEXT;
ALTER TABLE agent_sessions ADD COLUMN agent_version TEXT;
ALTER TABLE agent_sessions ADD COLUMN agent_name TEXT;
ALTER TABLE agent_sessions ADD COLUMN agent_role TEXT;
ALTER TABLE agent_sessions ADD COLUMN git_branch TEXT;
ALTER TABLE agent_sessions ADD COLUMN git_commit TEXT;
ALTER TABLE agent_sessions ADD COLUMN git_remote_url TEXT;
ALTER TABLE agent_sessions ADD COLUMN data_quality TEXT NOT NULL DEFAULT 'partial'
    CHECK (data_quality IN ('complete', 'partial'));

CREATE TABLE session_events (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    invocation_id TEXT,
    source_path TEXT NOT NULL,
    timestamp_ms INTEGER,
    sequence_position INTEGER NOT NULL,
    sequence_part INTEGER NOT NULL,
    logical_ordinal INTEGER,
    event_type TEXT NOT NULL,
    event_json TEXT NOT NULL
);

CREATE INDEX session_events_session_sequence_idx
    ON session_events(
        session_id,
        logical_ordinal,
        sequence_position,
        sequence_part
    );

CREATE INDEX session_events_source_idx
    ON session_events(source_path);

CREATE INDEX session_events_invocation_idx
    ON session_events(invocation_id);

-- The normalized event projection is new. Rebuild from coding-agent-data so
-- existing sessions receive their complete conversation and tool history.
DELETE FROM provider_sync_state;
