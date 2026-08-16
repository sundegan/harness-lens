ALTER TABLE agent_sessions ADD COLUMN first_user_message_text TEXT;
ALTER TABLE agent_sessions ADD COLUMN first_user_message_event_id TEXT;
ALTER TABLE agent_sessions ADD COLUMN first_user_message_timestamp_ms INTEGER;

WITH ranked_user_messages AS (
    SELECT
        session_id,
        id,
        timestamp_ms,
        event_json,
        ROW_NUMBER() OVER (
            PARTITION BY session_id
            ORDER BY
                timestamp_ms IS NULL,
                timestamp_ms,
                sequence_position,
                sequence_part,
                id
        ) AS row_number
    FROM session_events
    WHERE event_type = 'message'
      AND (
          json_extract(event_json, '$.data.value.role') = 'user'
          OR json_extract(event_json, '$.actor') = 'user'
      )
), first_user_messages AS (
    SELECT session_id, id, timestamp_ms, event_json
    FROM ranked_user_messages
    WHERE row_number = 1
)
UPDATE agent_sessions
SET first_user_message_event_id = (
        SELECT first_user_messages.id
        FROM first_user_messages
        WHERE first_user_messages.session_id = agent_sessions.id
    ),
    first_user_message_timestamp_ms = (
        SELECT first_user_messages.timestamp_ms
        FROM first_user_messages
        WHERE first_user_messages.session_id = agent_sessions.id
    ),
    first_user_message_text = (
        SELECT trim(
            COALESCE(
                (
                    SELECT group_concat(json_extract(content.value, '$.text'), ' ')
                    FROM json_each(first_user_messages.event_json, '$.data.value.content') AS content
                    WHERE json_extract(content.value, '$.type') = 'text'
                       OR (
                           json_extract(content.value, '$.type') = 'resource'
                           AND json_extract(content.value, '$.text') IS NOT NULL
                       )
                ),
                ''
            )
        )
        FROM first_user_messages
        WHERE first_user_messages.session_id = agent_sessions.id
    )
WHERE EXISTS (
    SELECT 1
    FROM first_user_messages
    WHERE first_user_messages.session_id = agent_sessions.id
);
