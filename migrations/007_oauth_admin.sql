-- OAuth providers + admin flag
ALTER TABLE users
    ADD COLUMN IF NOT EXISTS strava_id    BIGINT UNIQUE,
    ADD COLUMN IF NOT EXISTS google_id    TEXT UNIQUE,
    ADD COLUMN IF NOT EXISTS display_name TEXT,
    ADD COLUMN IF NOT EXISTS avatar_url   TEXT,
    ADD COLUMN IF NOT EXISTS is_admin     BOOLEAN NOT NULL DEFAULT FALSE;

CREATE INDEX IF NOT EXISTS idx_users_strava_id ON users(strava_id);
CREATE INDEX IF NOT EXISTS idx_users_google_id ON users(google_id);
