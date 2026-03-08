-- Coach messages
CREATE TABLE IF NOT EXISTS coach_messages (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role       TEXT        NOT NULL CHECK (role IN ('user', 'assistant')),
    content    TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_coach_messages_user ON coach_messages(user_id, created_at);

-- Strava user-provided credentials
ALTER TABLE strava_tokens
    ADD COLUMN IF NOT EXISTS strava_client_id     TEXT,
    ADD COLUMN IF NOT EXISTS strava_client_secret TEXT,
    ADD COLUMN IF NOT EXISTS display_name         TEXT,
    ADD COLUMN IF NOT EXISTS avatar_url           TEXT;
