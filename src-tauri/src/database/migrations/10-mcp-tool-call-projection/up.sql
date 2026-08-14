-- Tool-call analytics is an MCP-only product surface. The old tables are
-- derived projections, so discard them and replay provider sources instead of
-- carrying broad built-in, hosted, custom, or unknown tool rows forward.
DROP TABLE tool_call_retry;
DROP TABLE tool_call;

-- Clearing checkpoints makes each configured provider rebuild this projection
-- from its read-only durable sources on the next monitor start.
DELETE FROM provider_sync_state;

CREATE TABLE mcp_tool_call (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    source_id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    invocation_id TEXT REFERENCES agent_invocations(id) ON DELETE SET NULL,
    source_path TEXT NOT NULL,
    call_id TEXT NOT NULL,
    tool_name TEXT NOT NULL DEFAULT '',
    namespace TEXT,
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
    repeat_of_id TEXT REFERENCES mcp_tool_call(id) ON DELETE SET NULL,
    repeat_index INTEGER NOT NULL DEFAULT 0,
    retry_class TEXT NOT NULL DEFAULT 'none'
        CHECK (retry_class IN ('none', 'inferred')),
    retry_of_id TEXT REFERENCES mcp_tool_call(id) ON DELETE SET NULL,
    explicit_retry_count INTEGER NOT NULL DEFAULT 0,
    evidence_quality TEXT NOT NULL DEFAULT 'observed'
        CHECK (evidence_quality IN ('observed', 'derived', 'unknown')),
    CHECK (duration_ms IS NULL OR duration_ms >= 0),
    UNIQUE (source_id, source_path, call_id)
);

CREATE INDEX mcp_tool_call_started_idx ON mcp_tool_call(started_at_ms);
CREATE INDEX mcp_tool_call_tool_status_idx ON mcp_tool_call(tool_name, status);
CREATE INDEX mcp_tool_call_session_idx ON mcp_tool_call(session_id, started_at_ms);
CREATE INDEX mcp_tool_call_invocation_idx ON mcp_tool_call(invocation_id);
CREATE INDEX mcp_tool_call_provider_idx ON mcp_tool_call(provider, started_at_ms);
CREATE INDEX mcp_tool_call_server_idx ON mcp_tool_call(mcp_server, started_at_ms);
CREATE INDEX mcp_tool_call_repeat_idx ON mcp_tool_call(repeat_group_id, repeat_index);
CREATE INDEX mcp_tool_call_source_idx ON mcp_tool_call(source_id, source_path);
CREATE UNIQUE INDEX mcp_tool_call_call_event_idx ON mcp_tool_call(call_event_id)
    WHERE call_event_id IS NOT NULL;
CREATE UNIQUE INDEX mcp_tool_call_result_event_idx ON mcp_tool_call(result_event_id)
    WHERE result_event_id IS NOT NULL;

-- Retry analytics only includes evidence structurally linked to an MCP call.
-- Provider retry events without that link remain available in session_events.
CREATE TABLE mcp_tool_call_retry (
    id TEXT PRIMARY KEY NOT NULL REFERENCES session_events(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    source_id TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES agent_sessions(id) ON DELETE CASCADE,
    invocation_id TEXT REFERENCES agent_invocations(id) ON DELETE SET NULL,
    source_path TEXT NOT NULL,
    timestamp_ms INTEGER,
    mcp_tool_call_id TEXT NOT NULL REFERENCES mcp_tool_call(id) ON DELETE CASCADE,
    attempt INTEGER,
    delay_ms INTEGER,
    evidence_quality TEXT NOT NULL DEFAULT 'observed'
        CHECK (evidence_quality IN ('observed', 'derived', 'unknown')),
    CHECK (attempt IS NULL OR attempt > 0),
    CHECK (delay_ms IS NULL OR delay_ms >= 0)
);

CREATE INDEX mcp_tool_call_retry_source_idx
    ON mcp_tool_call_retry(source_id, source_path);
CREATE INDEX mcp_tool_call_retry_session_idx
    ON mcp_tool_call_retry(session_id, timestamp_ms);
CREATE INDEX mcp_tool_call_retry_call_idx
    ON mcp_tool_call_retry(mcp_tool_call_id);

CREATE TRIGGER mcp_tool_call_retry_insert_count
AFTER INSERT ON mcp_tool_call_retry
BEGIN
    UPDATE mcp_tool_call
    SET explicit_retry_count = explicit_retry_count + 1
    WHERE id = NEW.mcp_tool_call_id;
END;

CREATE TRIGGER mcp_tool_call_retry_delete_count
AFTER DELETE ON mcp_tool_call_retry
BEGIN
    UPDATE mcp_tool_call
    SET explicit_retry_count = MAX(explicit_retry_count - 1, 0)
    WHERE id = OLD.mcp_tool_call_id;
END;

CREATE TRIGGER mcp_tool_call_retry_update_count
AFTER UPDATE OF mcp_tool_call_id ON mcp_tool_call_retry
WHEN OLD.mcp_tool_call_id IS NOT NEW.mcp_tool_call_id
BEGIN
    UPDATE mcp_tool_call
    SET explicit_retry_count = MAX(explicit_retry_count - 1, 0)
    WHERE id = OLD.mcp_tool_call_id;
    UPDATE mcp_tool_call
    SET explicit_retry_count = explicit_retry_count + 1
    WHERE id = NEW.mcp_tool_call_id;
END;
