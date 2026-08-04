CREATE TABLE pending_skill_reads (
    source_path TEXT NOT NULL,
    call_id TEXT NOT NULL,
    turn_id TEXT NOT NULL REFERENCES session_turns(id) ON DELETE CASCADE,
    PRIMARY KEY (source_path, call_id)
);
