-- HarnessLens SQLite schema baseline v1.
--
-- This database stores provider-neutral import provenance, canonical facts, and
-- rebuildable analytics. Provider-private SQLite tables and JSON fields must not
-- leak into this schema.

PRAGMA foreign_keys = ON;

CREATE TABLE schema_metadata (
    singleton       INTEGER PRIMARY KEY CHECK (singleton = 1),
    schema_version  INTEGER NOT NULL CHECK (schema_version > 0),
    created_at_ms   INTEGER NOT NULL,
    updated_at_ms   INTEGER NOT NULL
);

INSERT INTO schema_metadata (
    singleton,
    schema_version,
    created_at_ms,
    updated_at_ms
)
VALUES (
    1,
    1,
    CAST(strftime('%s', 'now') AS INTEGER) * 1000,
    CAST(strftime('%s', 'now') AS INTEGER) * 1000
);

-- ---------------------------------------------------------------------------
-- Layer 1: source discovery, import provenance, and parser diagnostics
-- ---------------------------------------------------------------------------

CREATE TABLE agent_providers (
    id               TEXT PRIMARY KEY,
    display_name     TEXT NOT NULL,
    provider_version TEXT,
    metadata_json    TEXT,
    created_at_ms    INTEGER NOT NULL,
    updated_at_ms    INTEGER NOT NULL,
    CHECK (metadata_json IS NULL OR json_valid(metadata_json))
);

CREATE TABLE data_sources (
    id                      TEXT PRIMARY KEY,
    agent_provider_id       TEXT NOT NULL
                                    REFERENCES agent_providers(id)
                                    ON DELETE RESTRICT,
    display_name            TEXT NOT NULL,
    locator                 TEXT NOT NULL,
    locator_hash            TEXT NOT NULL,
    source_format           TEXT NOT NULL,
    last_format_fingerprint TEXT,
    last_parser_version     TEXT,
    enabled                 INTEGER NOT NULL DEFAULT 1
                                    CHECK (enabled IN (0, 1)),
    privacy_mode            TEXT NOT NULL DEFAULT 'reference_only'
                                    CHECK (
                                        privacy_mode IN (
                                            'reference_only',
                                            'metadata_only',
                                            'snapshot'
                                        )
                                    ),
    config_json             TEXT,
    first_seen_at_ms        INTEGER NOT NULL,
    last_seen_at_ms         INTEGER NOT NULL,
    created_at_ms           INTEGER NOT NULL,
    updated_at_ms           INTEGER NOT NULL,
    UNIQUE (agent_provider_id, locator_hash),
    CHECK (config_json IS NULL OR json_valid(config_json))
);

CREATE TABLE import_runs (
    id                  TEXT PRIMARY KEY,
    data_source_id      TEXT NOT NULL
                               REFERENCES data_sources(id)
                               ON DELETE RESTRICT,
    parser_version      TEXT NOT NULL,
    format_fingerprint  TEXT,
    status              TEXT NOT NULL
                               CHECK (
                                   status IN (
                                       'running',
                                       'succeeded',
                                       'partial',
                                       'failed',
                                       'cancelled'
                                   )
                               ),
    started_at_ms       INTEGER NOT NULL,
    completed_at_ms     INTEGER,
    checkpoint_json     TEXT,
    artifacts_seen      INTEGER NOT NULL DEFAULT 0
                               CHECK (artifacts_seen >= 0),
    raw_records_read    INTEGER NOT NULL DEFAULT 0
                               CHECK (raw_records_read >= 0),
    facts_written       INTEGER NOT NULL DEFAULT 0
                               CHECK (facts_written >= 0),
    diagnostics_count   INTEGER NOT NULL DEFAULT 0
                               CHECK (diagnostics_count >= 0),
    error_summary       TEXT,
    created_at_ms       INTEGER NOT NULL,
    CHECK (checkpoint_json IS NULL OR json_valid(checkpoint_json)),
    CHECK (
        completed_at_ms IS NULL
        OR completed_at_ms >= started_at_ms
    )
);

CREATE TABLE source_artifacts (
    id                  TEXT PRIMARY KEY,
    data_source_id      TEXT NOT NULL
                               REFERENCES data_sources(id)
                               ON DELETE RESTRICT,
    artifact_key        TEXT NOT NULL,
    artifact_kind       TEXT NOT NULL,
    locator             TEXT NOT NULL,
    locator_hash        TEXT NOT NULL,
    content_fingerprint TEXT,
    size_bytes          INTEGER CHECK (size_bytes IS NULL OR size_bytes >= 0),
    modified_at_ms      INTEGER,
    cursor_json         TEXT,
    first_seen_run_id   TEXT NOT NULL
                               REFERENCES import_runs(id)
                               ON DELETE RESTRICT,
    last_seen_run_id    TEXT NOT NULL
                               REFERENCES import_runs(id)
                               ON DELETE RESTRICT,
    created_at_ms       INTEGER NOT NULL,
    updated_at_ms       INTEGER NOT NULL,
    UNIQUE (data_source_id, artifact_key),
    CHECK (cursor_json IS NULL OR json_valid(cursor_json))
);

CREATE TABLE raw_records (
    id                  TEXT PRIMARY KEY,
    source_artifact_id  TEXT NOT NULL
                               REFERENCES source_artifacts(id)
                               ON DELETE CASCADE,
    import_run_id       TEXT NOT NULL
                               REFERENCES import_runs(id)
                               ON DELETE RESTRICT,
    source_position     TEXT NOT NULL,
    record_kind         TEXT NOT NULL,
    recorded_at_ms      INTEGER,
    content_fingerprint TEXT NOT NULL,
    byte_start          INTEGER CHECK (byte_start IS NULL OR byte_start >= 0),
    byte_end            INTEGER CHECK (byte_end IS NULL OR byte_end >= 0),
    payload_text        TEXT,
    payload_json        TEXT,
    imported_at_ms      INTEGER NOT NULL,
    UNIQUE (source_artifact_id, source_position),
    CHECK (payload_json IS NULL OR json_valid(payload_json)),
    CHECK (
        byte_end IS NULL
        OR byte_start IS NULL
        OR byte_end >= byte_start
    )
);

CREATE TABLE parser_diagnostics (
    id                  TEXT PRIMARY KEY,
    import_run_id       TEXT NOT NULL
                               REFERENCES import_runs(id)
                               ON DELETE CASCADE,
    source_artifact_id  TEXT
                               REFERENCES source_artifacts(id)
                               ON DELETE SET NULL,
    raw_record_id       TEXT
                               REFERENCES raw_records(id)
                               ON DELETE SET NULL,
    severity            TEXT NOT NULL
                               CHECK (
                                   severity IN (
                                       'info',
                                       'warning',
                                       'error'
                                   )
                               ),
    code                TEXT NOT NULL,
    message             TEXT NOT NULL,
    source_position     TEXT,
    recoverable         INTEGER NOT NULL DEFAULT 1
                               CHECK (recoverable IN (0, 1)),
    details_json        TEXT,
    created_at_ms       INTEGER NOT NULL,
    CHECK (details_json IS NULL OR json_valid(details_json))
);

-- ---------------------------------------------------------------------------
-- Layer 2: provider-neutral canonical facts
-- ---------------------------------------------------------------------------

CREATE TABLE projects (
    id                  TEXT PRIMARY KEY,
    project_key         TEXT NOT NULL UNIQUE,
    display_name        TEXT NOT NULL,
    repository_url      TEXT,
    repository_hash     TEXT,
    root_path           TEXT,
    root_path_hash      TEXT,
    metadata_json       TEXT,
    first_seen_at_ms    INTEGER NOT NULL,
    last_seen_at_ms     INTEGER NOT NULL,
    created_at_ms       INTEGER NOT NULL,
    updated_at_ms       INTEGER NOT NULL,
    CHECK (metadata_json IS NULL OR json_valid(metadata_json))
);

CREATE TABLE models (
    id                  TEXT PRIMARY KEY,
    model_key           TEXT NOT NULL UNIQUE,
    vendor              TEXT NOT NULL,
    name                TEXT NOT NULL,
    version             TEXT NOT NULL DEFAULT '',
    context_window      INTEGER
                               CHECK (
                                   context_window IS NULL
                                   OR context_window > 0
                               ),
    metadata_json       TEXT,
    created_at_ms       INTEGER NOT NULL,
    updated_at_ms       INTEGER NOT NULL,
    CHECK (metadata_json IS NULL OR json_valid(metadata_json))
);

CREATE TABLE sessions (
    id                  TEXT PRIMARY KEY,
    data_source_id      TEXT NOT NULL
                               REFERENCES data_sources(id)
                               ON DELETE RESTRICT,
    external_session_id TEXT NOT NULL,
    project_id          TEXT
                               REFERENCES projects(id)
                               ON DELETE SET NULL,
    default_model_id    TEXT
                               REFERENCES models(id)
                               ON DELETE SET NULL,
    title               TEXT,
    working_directory   TEXT,
    working_dir_hash    TEXT,
    started_at_ms       INTEGER,
    ended_at_ms         INTEGER,
    updated_at_ms       INTEGER NOT NULL,
    status              TEXT NOT NULL DEFAULT 'unknown'
                               CHECK (
                                   status IN (
                                       'running',
                                       'completed',
                                       'failed',
                                       'cancelled',
                                       'unknown'
                                   )
                               ),
    archived            INTEGER NOT NULL DEFAULT 0
                               CHECK (archived IN (0, 1)),
    quality             TEXT NOT NULL DEFAULT 'observed'
                               CHECK (
                                   quality IN (
                                       'observed',
                                       'derived',
                                       'estimated',
                                       'inferred',
                                       'projected',
                                       'unknown'
                                   )
                               ),
    first_import_run_id TEXT NOT NULL
                               REFERENCES import_runs(id)
                               ON DELETE RESTRICT,
    last_import_run_id  TEXT NOT NULL
                               REFERENCES import_runs(id)
                               ON DELETE RESTRICT,
    metadata_json       TEXT,
    created_at_ms       INTEGER NOT NULL,
    UNIQUE (data_source_id, external_session_id),
    UNIQUE (id, data_source_id),
    CHECK (metadata_json IS NULL OR json_valid(metadata_json)),
    CHECK (
        ended_at_ms IS NULL
        OR started_at_ms IS NULL
        OR ended_at_ms >= started_at_ms
    )
);

CREATE TABLE session_relations (
    parent_session_id   TEXT NOT NULL
                               REFERENCES sessions(id)
                               ON DELETE CASCADE,
    child_session_id    TEXT NOT NULL
                               REFERENCES sessions(id)
                               ON DELETE CASCADE,
    relation_kind       TEXT NOT NULL
                               CHECK (
                                   relation_kind IN (
                                       'fork',
                                       'spawn',
                                       'resume',
                                       'continuation',
                                       'related'
                                   )
                               ),
    quality             TEXT NOT NULL
                               CHECK (
                                   quality IN (
                                       'observed',
                                       'derived',
                                       'estimated',
                                       'inferred',
                                       'projected',
                                       'unknown'
                                   )
                               ),
    metadata_json       TEXT,
    created_at_ms       INTEGER NOT NULL,
    PRIMARY KEY (
        parent_session_id,
        child_session_id,
        relation_kind
    ),
    CHECK (parent_session_id <> child_session_id),
    CHECK (metadata_json IS NULL OR json_valid(metadata_json))
);

CREATE TABLE turns (
    id                  TEXT PRIMARY KEY,
    session_id          TEXT NOT NULL
                               REFERENCES sessions(id)
                               ON DELETE CASCADE,
    external_turn_id    TEXT,
    ordinal             INTEGER NOT NULL CHECK (ordinal >= 0),
    started_at_ms       INTEGER,
    ended_at_ms         INTEGER,
    status              TEXT NOT NULL DEFAULT 'unknown'
                               CHECK (
                                   status IN (
                                       'running',
                                       'completed',
                                       'failed',
                                       'cancelled',
                                       'unknown'
                                   )
                               ),
    error_code          TEXT,
    error_message       TEXT,
    quality             TEXT NOT NULL DEFAULT 'observed'
                               CHECK (
                                   quality IN (
                                       'observed',
                                       'derived',
                                       'estimated',
                                       'inferred',
                                       'projected',
                                       'unknown'
                                   )
                               ),
    import_run_id       TEXT NOT NULL
                               REFERENCES import_runs(id)
                               ON DELETE RESTRICT,
    metadata_json       TEXT,
    created_at_ms       INTEGER NOT NULL,
    UNIQUE (session_id, ordinal),
    UNIQUE (id, session_id),
    CHECK (metadata_json IS NULL OR json_valid(metadata_json)),
    CHECK (
        ended_at_ms IS NULL
        OR started_at_ms IS NULL
        OR ended_at_ms >= started_at_ms
    )
);

CREATE TABLE events (
    id                  TEXT PRIMARY KEY,
    session_id          TEXT NOT NULL
                               REFERENCES sessions(id)
                               ON DELETE CASCADE,
    turn_id             TEXT,
    external_event_id   TEXT,
    sequence            INTEGER NOT NULL CHECK (sequence >= 0),
    occurred_at_ms      INTEGER,
    kind                TEXT NOT NULL,
    status              TEXT,
    raw_record_id       TEXT
                               REFERENCES raw_records(id)
                               ON DELETE SET NULL,
    quality             TEXT NOT NULL DEFAULT 'observed'
                               CHECK (
                                   quality IN (
                                       'observed',
                                       'derived',
                                       'estimated',
                                       'inferred',
                                       'projected',
                                       'unknown'
                                   )
                               ),
    derivation_version  TEXT,
    payload_json        TEXT,
    import_run_id       TEXT NOT NULL
                               REFERENCES import_runs(id)
                               ON DELETE RESTRICT,
    created_at_ms       INTEGER NOT NULL,
    UNIQUE (session_id, sequence),
    UNIQUE (id, session_id),
    FOREIGN KEY (turn_id, session_id)
        REFERENCES turns(id, session_id)
        ON DELETE CASCADE,
    CHECK (payload_json IS NULL OR json_valid(payload_json))
);

CREATE TABLE messages (
    event_id            TEXT PRIMARY KEY
                               REFERENCES events(id)
                               ON DELETE CASCADE,
    session_id          TEXT NOT NULL,
    role                TEXT NOT NULL
                               CHECK (
                                   role IN (
                                       'user',
                                       'assistant',
                                       'system',
                                       'developer',
                                       'tool',
                                       'unknown'
                                   )
                               ),
    phase               TEXT,
    content_type        TEXT NOT NULL DEFAULT 'text',
    content_text        TEXT,
    content_json        TEXT,
    content_hash        TEXT,
    redacted            INTEGER NOT NULL DEFAULT 0
                               CHECK (redacted IN (0, 1)),
    created_at_ms       INTEGER NOT NULL,
    FOREIGN KEY (event_id, session_id)
        REFERENCES events(id, session_id)
        ON DELETE CASCADE,
    CHECK (content_json IS NULL OR json_valid(content_json)),
    CHECK (
        content_text IS NOT NULL
        OR content_json IS NOT NULL
        OR content_hash IS NOT NULL
    )
);

CREATE TABLE model_calls (
    id                  TEXT PRIMARY KEY,
    session_id          TEXT NOT NULL
                               REFERENCES sessions(id)
                               ON DELETE CASCADE,
    turn_id             TEXT,
    model_id            TEXT
                               REFERENCES models(id)
                               ON DELETE SET NULL,
    external_call_id    TEXT,
    ordinal             INTEGER NOT NULL CHECK (ordinal >= 0),
    request_event_id    TEXT
                               REFERENCES events(id)
                               ON DELETE SET NULL,
    response_event_id   TEXT
                               REFERENCES events(id)
                               ON DELETE SET NULL,
    started_at_ms       INTEGER,
    ended_at_ms         INTEGER,
    status              TEXT NOT NULL DEFAULT 'unknown'
                               CHECK (
                                   status IN (
                                       'running',
                                       'success',
                                       'failed',
                                       'cancelled',
                                       'unknown'
                                   )
                               ),
    error_code          TEXT,
    quality             TEXT NOT NULL DEFAULT 'observed'
                               CHECK (
                                   quality IN (
                                       'observed',
                                       'derived',
                                       'estimated',
                                       'inferred',
                                       'projected',
                                       'unknown'
                                   )
                               ),
    import_run_id       TEXT NOT NULL
                               REFERENCES import_runs(id)
                               ON DELETE RESTRICT,
    metadata_json       TEXT,
    created_at_ms       INTEGER NOT NULL,
    UNIQUE (session_id, ordinal),
    UNIQUE (id, session_id),
    FOREIGN KEY (turn_id, session_id)
        REFERENCES turns(id, session_id)
        ON DELETE CASCADE,
    CHECK (metadata_json IS NULL OR json_valid(metadata_json)),
    CHECK (
        ended_at_ms IS NULL
        OR started_at_ms IS NULL
        OR ended_at_ms >= started_at_ms
    )
);

CREATE TABLE capabilities (
    id                  TEXT PRIMARY KEY,
    capability_key      TEXT NOT NULL UNIQUE,
    capability_type     TEXT NOT NULL
                               CHECK (
                                   capability_type IN (
                                       'skill',
                                       'mcp_tool',
                                       'tool',
                                       'workflow',
                                       'workflow_step',
                                       'agent',
                                       'other'
                                   )
                               ),
    namespace           TEXT NOT NULL DEFAULT '',
    name                TEXT NOT NULL,
    version             TEXT NOT NULL DEFAULT '',
    definition_source   TEXT NOT NULL
                               CHECK (
                                   definition_source IN (
                                       'explicit',
                                       'adapter',
                                       'inferred',
                                       'manual'
                                   )
                               ),
    description         TEXT,
    metadata_json       TEXT,
    created_at_ms       INTEGER NOT NULL,
    updated_at_ms       INTEGER NOT NULL,
    UNIQUE (
        capability_type,
        namespace,
        name,
        version
    ),
    CHECK (metadata_json IS NULL OR json_valid(metadata_json))
);

CREATE TABLE capability_invocations (
    id                      TEXT PRIMARY KEY,
    capability_id           TEXT NOT NULL
                                   REFERENCES capabilities(id)
                                   ON DELETE RESTRICT,
    session_id              TEXT NOT NULL
                                   REFERENCES sessions(id)
                                   ON DELETE CASCADE,
    turn_id                 TEXT,
    external_invocation_id  TEXT,
    ordinal                 INTEGER NOT NULL CHECK (ordinal >= 0),
    started_at_ms           INTEGER,
    ended_at_ms             INTEGER,
    duration_ms             INTEGER
                                   CHECK (
                                       duration_ms IS NULL
                                       OR duration_ms >= 0
                                   ),
    status                  TEXT NOT NULL DEFAULT 'unknown'
                                   CHECK (
                                       status IN (
                                           'running',
                                           'success',
                                           'failed',
                                           'cancelled',
                                           'timeout',
                                           'unknown'
                                       )
                                   ),
    error_type              TEXT,
    error_message           TEXT,
    retry_of_invocation_id  TEXT
                                   REFERENCES capability_invocations(id)
                                   ON DELETE SET NULL,
    output_bytes            INTEGER
                                   CHECK (
                                       output_bytes IS NULL
                                       OR output_bytes >= 0
                                   ),
    quality                 TEXT NOT NULL DEFAULT 'observed'
                                   CHECK (
                                       quality IN (
                                           'observed',
                                           'derived',
                                           'estimated',
                                           'inferred',
                                           'projected',
                                           'unknown'
                                       )
                                   ),
    confidence              REAL
                                   CHECK (
                                       confidence IS NULL
                                       OR (
                                           confidence >= 0.0
                                           AND confidence <= 1.0
                                       )
                                   ),
    derivation_version      TEXT,
    import_run_id           TEXT NOT NULL
                                   REFERENCES import_runs(id)
                                   ON DELETE RESTRICT,
    metadata_json           TEXT,
    created_at_ms           INTEGER NOT NULL,
    UNIQUE (session_id, ordinal),
    UNIQUE (id, session_id),
    FOREIGN KEY (turn_id, session_id)
        REFERENCES turns(id, session_id)
        ON DELETE CASCADE,
    CHECK (metadata_json IS NULL OR json_valid(metadata_json)),
    CHECK (
        ended_at_ms IS NULL
        OR started_at_ms IS NULL
        OR ended_at_ms >= started_at_ms
    )
);

CREATE TABLE invocation_evidence (
    id                  TEXT PRIMARY KEY,
    invocation_id       TEXT NOT NULL
                               REFERENCES capability_invocations(id)
                               ON DELETE CASCADE,
    event_id            TEXT
                               REFERENCES events(id)
                               ON DELETE CASCADE,
    raw_record_id       TEXT
                               REFERENCES raw_records(id)
                               ON DELETE CASCADE,
    relation_kind       TEXT NOT NULL
                               CHECK (
                                   relation_kind IN (
                                       'start',
                                       'end',
                                       'call',
                                       'result',
                                       'error',
                                       'context',
                                       'inference'
                                   )
                               ),
    created_at_ms       INTEGER NOT NULL,
    CHECK (event_id IS NOT NULL OR raw_record_id IS NOT NULL)
);

CREATE UNIQUE INDEX ux_invocation_evidence_identity
    ON invocation_evidence (
        invocation_id,
        relation_kind,
        COALESCE(event_id, ''),
        COALESCE(raw_record_id, '')
    );

CREATE TABLE usage_observations (
    id                  TEXT PRIMARY KEY,
    session_id          TEXT NOT NULL
                               REFERENCES sessions(id)
                               ON DELETE CASCADE,
    turn_id             TEXT,
    model_call_id       TEXT,
    invocation_id       TEXT,
    event_id            TEXT
                               REFERENCES events(id)
                               ON DELETE SET NULL,
    raw_record_id       TEXT
                               REFERENCES raw_records(id)
                               ON DELETE SET NULL,
    scope               TEXT NOT NULL
                               CHECK (
                                   scope IN (
                                       'session',
                                       'turn',
                                       'model_call',
                                       'capability_invocation'
                                   )
                               ),
    input_tokens        INTEGER
                               CHECK (
                                   input_tokens IS NULL
                                   OR input_tokens >= 0
                               ),
    output_tokens       INTEGER
                               CHECK (
                                   output_tokens IS NULL
                                   OR output_tokens >= 0
                               ),
    cached_input_tokens INTEGER
                               CHECK (
                                   cached_input_tokens IS NULL
                                   OR cached_input_tokens >= 0
                               ),
    reasoning_tokens    INTEGER
                               CHECK (
                                   reasoning_tokens IS NULL
                                   OR reasoning_tokens >= 0
                               ),
    total_tokens        INTEGER
                               CHECK (
                                   total_tokens IS NULL
                                   OR total_tokens >= 0
                               ),
    cost_micros         INTEGER
                               CHECK (
                                   cost_micros IS NULL
                                   OR cost_micros >= 0
                               ),
    currency            TEXT,
    quality             TEXT NOT NULL
                               CHECK (
                                   quality IN (
                                       'observed',
                                       'derived',
                                       'estimated',
                                       'inferred',
                                       'projected',
                                       'unknown'
                                   )
                               ),
    pricing_version     TEXT,
    observed_at_ms      INTEGER,
    import_run_id       TEXT NOT NULL
                               REFERENCES import_runs(id)
                               ON DELETE RESTRICT,
    metadata_json       TEXT,
    created_at_ms       INTEGER NOT NULL,
    FOREIGN KEY (turn_id, session_id)
        REFERENCES turns(id, session_id)
        ON DELETE CASCADE,
    FOREIGN KEY (model_call_id, session_id)
        REFERENCES model_calls(id, session_id)
        ON DELETE CASCADE,
    FOREIGN KEY (invocation_id, session_id)
        REFERENCES capability_invocations(id, session_id)
        ON DELETE CASCADE,
    CHECK (metadata_json IS NULL OR json_valid(metadata_json)),
    CHECK (
        input_tokens IS NOT NULL
        OR output_tokens IS NOT NULL
        OR cached_input_tokens IS NOT NULL
        OR reasoning_tokens IS NOT NULL
        OR total_tokens IS NOT NULL
        OR cost_micros IS NOT NULL
    ),
    CHECK (
        (
            scope = 'session'
            AND turn_id IS NULL
            AND model_call_id IS NULL
            AND invocation_id IS NULL
        )
        OR (
            scope = 'turn'
            AND turn_id IS NOT NULL
            AND model_call_id IS NULL
            AND invocation_id IS NULL
        )
        OR (
            scope = 'model_call'
            AND turn_id IS NULL
            AND model_call_id IS NOT NULL
            AND invocation_id IS NULL
        )
        OR (
            scope = 'capability_invocation'
            AND turn_id IS NULL
            AND model_call_id IS NULL
            AND invocation_id IS NOT NULL
        )
    ),
    CHECK (cost_micros IS NULL OR currency IS NOT NULL)
);

CREATE TABLE model_prices (
    id                          TEXT PRIMARY KEY,
    model_id                    TEXT NOT NULL
                                       REFERENCES models(id)
                                       ON DELETE CASCADE,
    pricing_version             TEXT NOT NULL,
    currency                    TEXT NOT NULL,
    effective_from_ms           INTEGER NOT NULL,
    effective_to_ms             INTEGER,
    input_micros_per_million    INTEGER
                                       CHECK (
                                           input_micros_per_million IS NULL
                                           OR input_micros_per_million >= 0
                                       ),
    output_micros_per_million   INTEGER
                                       CHECK (
                                           output_micros_per_million IS NULL
                                           OR output_micros_per_million >= 0
                                       ),
    cached_micros_per_million   INTEGER
                                       CHECK (
                                           cached_micros_per_million IS NULL
                                           OR cached_micros_per_million >= 0
                                       ),
    metadata_json               TEXT,
    created_at_ms               INTEGER NOT NULL,
    UNIQUE (
        model_id,
        pricing_version,
        effective_from_ms
    ),
    CHECK (metadata_json IS NULL OR json_valid(metadata_json)),
    CHECK (
        effective_to_ms IS NULL
        OR effective_to_ms > effective_from_ms
    ),
    CHECK (
        input_micros_per_million IS NOT NULL
        OR output_micros_per_million IS NOT NULL
        OR cached_micros_per_million IS NOT NULL
    )
);

-- ---------------------------------------------------------------------------
-- Layer 3: rebuildable analysis runs, metric cache, and anomaly candidates
-- ---------------------------------------------------------------------------

CREATE TABLE analysis_runs (
    id                      TEXT PRIMARY KEY,
    trigger_import_run_id   TEXT
                                   REFERENCES import_runs(id)
                                   ON DELETE SET NULL,
    derivation_version      TEXT NOT NULL,
    pricing_version         TEXT,
    status                  TEXT NOT NULL
                                   CHECK (
                                       status IN (
                                           'running',
                                           'succeeded',
                                           'partial',
                                           'failed',
                                           'cancelled',
                                           'invalidated'
                                       )
                                   ),
    input_watermark_ms      INTEGER,
    started_at_ms           INTEGER NOT NULL,
    completed_at_ms         INTEGER,
    invalidated_at_ms       INTEGER,
    error_summary           TEXT,
    created_at_ms           INTEGER NOT NULL,
    CHECK (
        completed_at_ms IS NULL
        OR completed_at_ms >= started_at_ms
    )
);

CREATE TABLE metric_definitions (
    metric_key          TEXT PRIMARY KEY,
    display_name        TEXT NOT NULL,
    description         TEXT NOT NULL,
    unit                TEXT NOT NULL,
    value_type          TEXT NOT NULL
                               CHECK (
                                   value_type IN (
                                       'integer',
                                       'real'
                                   )
                               ),
    aggregation_kind    TEXT NOT NULL,
    created_at_ms       INTEGER NOT NULL,
    updated_at_ms       INTEGER NOT NULL
);

CREATE TABLE metric_points (
    id                      TEXT PRIMARY KEY,
    analysis_run_id         TEXT NOT NULL
                                   REFERENCES analysis_runs(id)
                                   ON DELETE CASCADE,
    metric_key              TEXT NOT NULL
                                   REFERENCES metric_definitions(metric_key)
                                   ON DELETE RESTRICT,
    subject_type            TEXT NOT NULL
                                   CHECK (
                                       subject_type IN (
                                           'global',
                                           'project',
                                           'session',
                                           'capability',
                                           'model',
                                           'agent_provider'
                                       )
                                   ),
    subject_id              TEXT NOT NULL DEFAULT '',
    period_start_ms         INTEGER NOT NULL,
    period_end_ms           INTEGER NOT NULL,
    dimensions_json         TEXT NOT NULL DEFAULT '{}',
    dimensions_hash         TEXT NOT NULL,
    value_integer           INTEGER,
    value_real              REAL,
    sample_count            INTEGER NOT NULL DEFAULT 0
                                   CHECK (sample_count >= 0),
    covered_sample_count    INTEGER NOT NULL DEFAULT 0
                                   CHECK (covered_sample_count >= 0),
    total_sample_count      INTEGER NOT NULL DEFAULT 0
                                   CHECK (total_sample_count >= 0),
    quality                 TEXT NOT NULL
                                   CHECK (
                                       quality IN (
                                           'observed',
                                           'derived',
                                           'estimated',
                                           'inferred',
                                           'projected',
                                           'unknown'
                                       )
                                   ),
    created_at_ms           INTEGER NOT NULL,
    UNIQUE (
        analysis_run_id,
        metric_key,
        subject_type,
        subject_id,
        period_start_ms,
        period_end_ms,
        dimensions_hash
    ),
    CHECK (json_valid(dimensions_json)),
    CHECK (period_end_ms > period_start_ms),
    CHECK (
        (value_integer IS NOT NULL AND value_real IS NULL)
        OR (value_integer IS NULL AND value_real IS NOT NULL)
    ),
    CHECK (covered_sample_count <= total_sample_count)
);

CREATE TABLE anomaly_rules (
    id                  TEXT PRIMARY KEY,
    rule_key            TEXT NOT NULL,
    version             TEXT NOT NULL,
    display_name        TEXT NOT NULL,
    description         TEXT NOT NULL,
    severity            TEXT NOT NULL
                               CHECK (
                                   severity IN (
                                       'info',
                                       'warning',
                                       'critical'
                                   )
                               ),
    enabled             INTEGER NOT NULL DEFAULT 1
                               CHECK (enabled IN (0, 1)),
    config_json         TEXT NOT NULL,
    created_at_ms       INTEGER NOT NULL,
    updated_at_ms       INTEGER NOT NULL,
    UNIQUE (rule_key, version),
    CHECK (json_valid(config_json))
);

CREATE TABLE anomaly_candidates (
    id                          TEXT PRIMARY KEY,
    analysis_run_id             TEXT NOT NULL
                                       REFERENCES analysis_runs(id)
                                       ON DELETE CASCADE,
    anomaly_rule_id             TEXT NOT NULL
                                       REFERENCES anomaly_rules(id)
                                       ON DELETE RESTRICT,
    subject_type                TEXT NOT NULL,
    subject_id                  TEXT NOT NULL,
    period_start_ms             INTEGER NOT NULL,
    period_end_ms               INTEGER NOT NULL,
    status                      TEXT NOT NULL DEFAULT 'open'
                                       CHECK (
                                           status IN (
                                               'open',
                                               'acknowledged',
                                               'dismissed',
                                               'resolved'
                                           )
                                       ),
    score                       REAL,
    summary                     TEXT NOT NULL,
    supporting_metric_point_id  TEXT
                                       REFERENCES metric_points(id)
                                       ON DELETE SET NULL,
    evidence_filter_json        TEXT NOT NULL,
    first_detected_at_ms        INTEGER NOT NULL,
    last_detected_at_ms         INTEGER NOT NULL,
    resolved_at_ms              INTEGER,
    created_at_ms               INTEGER NOT NULL,
    updated_at_ms               INTEGER NOT NULL,
    CHECK (json_valid(evidence_filter_json)),
    CHECK (period_end_ms > period_start_ms)
);

-- ---------------------------------------------------------------------------
-- Query-path indexes
-- ---------------------------------------------------------------------------

CREATE INDEX ix_data_sources_provider
    ON data_sources (agent_provider_id, enabled);

CREATE INDEX ix_import_runs_source_started
    ON import_runs (data_source_id, started_at_ms DESC);

CREATE INDEX ix_import_runs_status
    ON import_runs (status, started_at_ms DESC);

CREATE INDEX ix_source_artifacts_source_kind
    ON source_artifacts (data_source_id, artifact_kind);

CREATE INDEX ix_raw_records_artifact_time
    ON raw_records (source_artifact_id, recorded_at_ms);

CREATE INDEX ix_raw_records_fingerprint
    ON raw_records (content_fingerprint);

CREATE INDEX ix_parser_diagnostics_run_severity
    ON parser_diagnostics (import_run_id, severity, code);

CREATE INDEX ix_sessions_source_updated
    ON sessions (data_source_id, updated_at_ms DESC);

CREATE INDEX ix_sessions_project_started
    ON sessions (project_id, started_at_ms DESC);

CREATE INDEX ix_turns_session_time
    ON turns (session_id, started_at_ms);

CREATE UNIQUE INDEX ux_turns_external_id
    ON turns (session_id, external_turn_id)
    WHERE external_turn_id IS NOT NULL;

CREATE INDEX ix_events_session_time
    ON events (session_id, occurred_at_ms, sequence);

CREATE UNIQUE INDEX ux_events_external_id
    ON events (session_id, external_event_id)
    WHERE external_event_id IS NOT NULL;

CREATE INDEX ix_events_kind_time
    ON events (kind, occurred_at_ms);

CREATE INDEX ix_events_raw_record
    ON events (raw_record_id);

CREATE INDEX ix_messages_session_role
    ON messages (session_id, role);

CREATE INDEX ix_model_calls_session_time
    ON model_calls (session_id, started_at_ms);

CREATE UNIQUE INDEX ux_model_calls_external_id
    ON model_calls (session_id, external_call_id)
    WHERE external_call_id IS NOT NULL;

CREATE INDEX ix_capabilities_type_name
    ON capabilities (capability_type, namespace, name);

CREATE INDEX ix_invocations_capability_time
    ON capability_invocations (
        capability_id,
        started_at_ms DESC
    );

CREATE INDEX ix_invocations_session_time
    ON capability_invocations (session_id, started_at_ms);

CREATE UNIQUE INDEX ux_invocations_external_id
    ON capability_invocations (session_id, external_invocation_id)
    WHERE external_invocation_id IS NOT NULL;

CREATE INDEX ix_invocations_status_time
    ON capability_invocations (status, started_at_ms DESC);

CREATE INDEX ix_invocations_retry
    ON capability_invocations (retry_of_invocation_id);

CREATE INDEX ix_invocation_evidence_event
    ON invocation_evidence (event_id);

CREATE INDEX ix_invocation_evidence_raw
    ON invocation_evidence (raw_record_id);

CREATE INDEX ix_usage_session_scope
    ON usage_observations (session_id, scope, observed_at_ms);

CREATE INDEX ix_usage_model_call
    ON usage_observations (model_call_id);

CREATE INDEX ix_usage_invocation
    ON usage_observations (invocation_id);

CREATE INDEX ix_model_prices_effective
    ON model_prices (model_id, effective_from_ms, effective_to_ms);

CREATE INDEX ix_analysis_runs_status_started
    ON analysis_runs (status, started_at_ms DESC);

CREATE INDEX ix_metric_points_lookup
    ON metric_points (
        metric_key,
        subject_type,
        subject_id,
        period_start_ms,
        period_end_ms
    );

CREATE INDEX ix_anomaly_candidates_subject
    ON anomaly_candidates (
        subject_type,
        subject_id,
        status,
        period_start_ms
    );

CREATE INDEX ix_anomaly_candidates_rule_status
    ON anomaly_candidates (
        anomaly_rule_id,
        status,
        last_detected_at_ms DESC
    );
