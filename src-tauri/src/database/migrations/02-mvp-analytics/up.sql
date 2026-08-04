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

CREATE TABLE agent_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    source_session_id TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    project_name TEXT NOT NULL DEFAULT '',
    cwd TEXT,
    rollout_path TEXT UNIQUE,
    created_at_ms INTEGER,
    updated_at_ms INTEGER,
    tokens_used INTEGER,
    archived INTEGER NOT NULL DEFAULT 0 CHECK (archived IN (0, 1)),
    metadata_present INTEGER NOT NULL DEFAULT 1 CHECK (metadata_present IN (0, 1)),
    UNIQUE (provider, source_session_id)
);

CREATE INDEX agent_sessions_updated_at_idx
    ON agent_sessions(updated_at_ms DESC);

CREATE TABLE rollout_sources (
    path TEXT PRIMARY KEY NOT NULL,
    provider TEXT NOT NULL,
    session_id TEXT REFERENCES agent_sessions(id) ON DELETE SET NULL,
    current_turn_id TEXT,
    updated_at_ms INTEGER NOT NULL
);

CREATE INDEX rollout_sources_session_idx
    ON rollout_sources(session_id);

CREATE TABLE session_turns (
    id TEXT PRIMARY KEY NOT NULL,
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

CREATE INDEX session_turns_session_idx
    ON session_turns(session_id);

CREATE INDEX session_turns_source_idx
    ON session_turns(source_path);

CREATE INDEX session_turns_status_idx
    ON session_turns(status);

CREATE TABLE skill_invocations (
    id TEXT PRIMARY KEY NOT NULL,
    turn_id TEXT NOT NULL REFERENCES session_turns(id) ON DELETE CASCADE,
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
