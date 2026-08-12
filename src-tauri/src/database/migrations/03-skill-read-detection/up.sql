CREATE TABLE pending_skill_reads (
    source_path TEXT NOT NULL,
    call_id TEXT NOT NULL,
    invocation_id TEXT NOT NULL REFERENCES agent_invocations(id) ON DELETE CASCADE,
    PRIMARY KEY (source_path, call_id)
);
