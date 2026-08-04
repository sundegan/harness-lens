-- Clear saved checkpoints so analytics are rebuilt with legacy Codex turn
-- inference and resilient Skill projection.
DELETE FROM analytics_sync_state;
