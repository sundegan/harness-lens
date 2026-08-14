-- Migration 08 had already been applied by development builds before its
-- source-scoped projection schema was finalized. Rebuild every derived table
-- once more under a new forward-only version so those version-8 databases are
-- upgraded and all configured providers can replay their read-only sources.
DROP TABLE IF EXISTS tool_call_retry;
DROP TABLE IF EXISTS tool_call;

-- Every table rebuilt here is a derived analytics projection. Recreating the
-- projection removes the old provider-global path constraints and deliberately
-- clears checkpoints so each configured source is replayed read-only.
DROP TABLE pending_skill_reads;
DROP TABLE skill_invocations;
DROP TABLE token_usage_records;
DROP TABLE session_events;
DROP TABLE agent_invocations;
DROP TABLE rollout_sources;
DROP TABLE agent_sessions;
DROP TABLE provider_sync_state;

CREATE TABLE provider_sync_state (
    source_id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    checkpoint_json TEXT,
    status TEXT NOT NULL DEFAULT 'not_started',
    phase TEXT NOT NULL DEFAULT 'idle',
    processed_records INTEGER NOT NULL DEFAULT 0,
    diagnostic_count INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    updated_at_ms INTEGER NOT NULL
);

CREATE TABLE agent_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    source_id TEXT NOT NULL,
    source_session_id TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    project_name TEXT NOT NULL DEFAULT '',
    project_key TEXT,
    cwd TEXT,
    rollout_path TEXT,
    created_at_ms INTEGER,
    updated_at_ms INTEGER,
    tokens_used INTEGER,
    archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    metadata_present INTEGER NOT NULL DEFAULT 1 CHECK (metadata_present IN (0, 1)),
    model TEXT,
    model_provider TEXT,
    agent_version TEXT,
    agent_name TEXT,
    agent_role TEXT,
    git_branch TEXT,
    git_commit TEXT,
    git_remote_url TEXT,
    data_quality TEXT NOT NULL DEFAULT 'partial'
        CHECK (data_quality IN ('complete', 'partial')),
    UNIQUE (source_id, source_session_id),
    UNIQUE (source_id, rollout_path)
);

CREATE INDEX agent_sessions_updated_at_idx
    ON agent_sessions(updated_at_ms DESC);
CREATE INDEX agent_sessions_provider_idx
    ON agent_sessions(provider, updated_at_ms DESC);
CREATE INDEX agent_sessions_project_idx
    ON agent_sessions(project_key);

CREATE TABLE rollout_sources (
    source_id TEXT NOT NULL,
    path TEXT NOT NULL,
    provider TEXT NOT NULL,
    session_id TEXT REFERENCES agent_sessions(id) ON DELETE SET NULL,
    current_invocation_id TEXT,
    updated_at_ms INTEGER NOT NULL,
    PRIMARY KEY (source_id, path)
);

CREATE INDEX rollout_sources_session_idx
    ON rollout_sources(session_id);

CREATE TABLE agent_invocations (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    source_path TEXT NOT NULL,
    started_at_ms INTEGER,
    completed_at_ms INTEGER,
    duration_ms INTEGER,
    total_tokens INTEGER,
    status TEXT NOT NULL DEFAULT 'in_progress'
        CHECK (status IN ('in_progress', 'succeeded', 'failed', 'cancelled', 'unknown')),
    error_message TEXT
);

CREATE INDEX agent_invocations_session_idx
    ON agent_invocations(session_id);
CREATE INDEX agent_invocations_source_idx
    ON agent_invocations(source_id, source_path);
CREATE INDEX agent_invocations_status_idx
    ON agent_invocations(status);

CREATE TABLE skill_invocations (
    id TEXT PRIMARY KEY NOT NULL,
    invocation_id TEXT NOT NULL REFERENCES agent_invocations(id) ON DELETE CASCADE,
    session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    skill_name TEXT NOT NULL,
    started_at_ms INTEGER,
    duration_ms INTEGER,
    total_tokens INTEGER,
    status TEXT NOT NULL DEFAULT 'in_progress'
        CHECK (status IN ('in_progress', 'succeeded', 'failed', 'cancelled', 'unknown'))
);

CREATE INDEX skill_invocations_skill_idx
    ON skill_invocations(skill_name);
CREATE INDEX skill_invocations_session_idx
    ON skill_invocations(session_id);
CREATE INDEX skill_invocations_status_idx
    ON skill_invocations(status);

CREATE TABLE pending_skill_reads (
    source_id TEXT NOT NULL,
    source_path TEXT NOT NULL,
    call_id TEXT NOT NULL,
    invocation_id TEXT NOT NULL REFERENCES agent_invocations(id) ON DELETE CASCADE,
    PRIMARY KEY (source_id, source_path, call_id)
);

CREATE TABLE token_usage_records (
    id TEXT PRIMARY KEY NOT NULL,
    source_id TEXT NOT NULL,
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
    ON token_usage_records(source_id, source_path);

CREATE TABLE session_events (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    source_id TEXT NOT NULL,
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
    ON session_events(source_id, source_path);
CREATE INDEX session_events_invocation_idx
    ON session_events(invocation_id);

CREATE TABLE tool_call (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    source_id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    invocation_id TEXT REFERENCES agent_invocations(id) ON DELETE SET NULL,
    source_path TEXT NOT NULL,
    call_id TEXT NOT NULL,
    tool_name TEXT NOT NULL DEFAULT '',
    namespace TEXT,
    source_kind TEXT NOT NULL DEFAULT 'unknown'
        CHECK (source_kind IN ('built_in', 'mcp', 'provider_hosted', 'plugin', 'custom', 'unknown')),
    mcp_server TEXT,
    tool_kind TEXT NOT NULL DEFAULT 'other'
        CHECK (tool_kind IN ('read', 'edit', 'delete', 'move', 'search', 'execute', 'think', 'fetch', 'switch_mode', 'other')),
    title TEXT,
    started_at_ms INTEGER,
    completed_at_ms INTEGER,
    duration_ms INTEGER,
    duration_source TEXT NOT NULL DEFAULT 'unknown'
        CHECK (duration_source IN ('normalized', 'event_delta', 'unknown')),
    status TEXT NOT NULL DEFAULT 'unknown'
        CHECK (status IN ('pending', 'awaiting_approval', 'in_progress', 'completed', 'failed', 'cancelled', 'declined', 'unknown')),
    has_result INTEGER NOT NULL DEFAULT 0 CHECK (has_result IN (0, 1)),
    has_error INTEGER NOT NULL DEFAULT 0 CHECK (has_error IN (0, 1)),
    call_event_id TEXT REFERENCES session_events(id) ON DELETE SET NULL,
    result_event_id TEXT REFERENCES session_events(id) ON DELETE SET NULL,
    input_fingerprint TEXT,
    repeat_group_id TEXT,
    repeat_of_id TEXT REFERENCES tool_call(id) ON DELETE SET NULL,
    repeat_index INTEGER NOT NULL DEFAULT 0,
    retry_class TEXT NOT NULL DEFAULT 'none'
        CHECK (retry_class IN ('none', 'inferred')),
    retry_of_id TEXT REFERENCES tool_call(id) ON DELETE SET NULL,
    explicit_retry_count INTEGER NOT NULL DEFAULT 0,
    evidence_quality TEXT NOT NULL DEFAULT 'observed'
        CHECK (evidence_quality IN ('observed', 'derived', 'unknown')),
    CHECK (duration_ms IS NULL OR duration_ms >= 0),
    CHECK (source_kind = 'mcp' OR mcp_server IS NULL),
    UNIQUE (source_id, source_path, call_id)
);

CREATE INDEX tool_call_started_idx ON tool_call(started_at_ms);
CREATE INDEX tool_call_tool_status_idx ON tool_call(tool_name, status);
CREATE INDEX tool_call_session_idx ON tool_call(session_id, started_at_ms);
CREATE INDEX tool_call_invocation_idx ON tool_call(invocation_id);
CREATE INDEX tool_call_provider_idx ON tool_call(provider, started_at_ms);
CREATE INDEX tool_call_mcp_idx ON tool_call(mcp_server, started_at_ms)
    WHERE source_kind = 'mcp';
CREATE INDEX tool_call_repeat_idx ON tool_call(repeat_group_id, repeat_index);
CREATE INDEX tool_call_source_idx ON tool_call(source_id, source_path);
CREATE UNIQUE INDEX tool_call_call_event_idx ON tool_call(call_event_id)
    WHERE call_event_id IS NOT NULL;
CREATE UNIQUE INDEX tool_call_result_event_idx ON tool_call(result_event_id)
    WHERE result_event_id IS NOT NULL;

-- Retry is independent evidence. It may be associated with a tool call or
-- remain unattributed when the provider does not persist a structural link.
CREATE TABLE tool_call_retry (
    id TEXT PRIMARY KEY NOT NULL REFERENCES session_events(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    source_id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    invocation_id TEXT REFERENCES agent_invocations(id) ON DELETE SET NULL,
    source_path TEXT NOT NULL,
    timestamp_ms INTEGER,
    tool_call_id TEXT REFERENCES tool_call(id) ON DELETE SET NULL,
    attempt INTEGER,
    delay_ms INTEGER,
    evidence_quality TEXT NOT NULL DEFAULT 'observed'
        CHECK (evidence_quality IN ('observed', 'derived', 'unknown')),
    CHECK (attempt IS NULL OR attempt > 0),
    CHECK (delay_ms IS NULL OR delay_ms >= 0)
);

CREATE INDEX tool_call_retry_source_idx
    ON tool_call_retry(source_id, source_path);
CREATE INDEX tool_call_retry_session_idx
    ON tool_call_retry(session_id, timestamp_ms);
CREATE INDEX tool_call_retry_call_idx
    ON tool_call_retry(tool_call_id);

CREATE TRIGGER tool_call_retry_insert_count
AFTER INSERT ON tool_call_retry
WHEN NEW.tool_call_id IS NOT NULL
BEGIN
    UPDATE tool_call
    SET explicit_retry_count = explicit_retry_count + 1
    WHERE id = NEW.tool_call_id;
END;
CREATE TRIGGER tool_call_retry_delete_count
AFTER DELETE ON tool_call_retry
WHEN OLD.tool_call_id IS NOT NULL
BEGIN
    UPDATE tool_call
    SET explicit_retry_count = MAX(explicit_retry_count - 1, 0)
    WHERE id = OLD.tool_call_id;
END;

CREATE TRIGGER tool_call_retry_update_count
AFTER UPDATE OF tool_call_id ON tool_call_retry
WHEN OLD.tool_call_id IS NOT NEW.tool_call_id
BEGIN
    UPDATE tool_call
    SET explicit_retry_count = MAX(explicit_retry_count - 1, 0)
    WHERE id = OLD.tool_call_id;
    UPDATE tool_call
    SET explicit_retry_count = explicit_retry_count + 1
    WHERE id = NEW.tool_call_id;
END;
