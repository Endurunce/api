CREATE TABLE oauth_sessions (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    jwt          TEXT        NOT NULL,
    email        TEXT        NOT NULL,
    display_name TEXT,
    is_admin     BOOLEAN     NOT NULL DEFAULT FALSE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
