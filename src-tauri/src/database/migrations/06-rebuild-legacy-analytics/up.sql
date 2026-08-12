-- Clear saved checkpoints so analytics are rebuilt with legacy Codex invocation
-- inference and resilient Skill projection.
DELETE FROM provider_sync_state;
