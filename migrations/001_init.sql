-- Endurunce v2 — Clean-slate schema
-- All tables, indexes, and constraints in one migration.

-- ── Users ──────────────────────────────────────────────────────────────────────
CREATE TABLE users (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    display_name  TEXT,
    avatar_url    TEXT,
    strava_id     BIGINT UNIQUE,
    google_id     TEXT UNIQUE,
    is_admin      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Profiles ───────────────────────────────────────────────────────────────────
CREATE TABLE profiles (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id             UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    name                TEXT NOT NULL,
    date_of_birth       DATE NOT NULL,
    gender              TEXT NOT NULL CHECK (gender IN ('male', 'female', 'other')),
    running_experience  TEXT NOT NULL DEFAULT 'two_to_five_years',
    weekly_km           REAL NOT NULL DEFAULT 0 CHECK (weekly_km >= 0),
    time_5k             TEXT,
    time_10k            TEXT,
    time_half           TEXT,
    time_marathon       TEXT,
    rest_hr             SMALLINT DEFAULT 60,
    max_hr              SMALLINT,
    sleep_quality       TEXT DEFAULT 'seven_to_eight',
    complaints          TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Training Preferences ───────────────────────────────────────────────────────
CREATE TABLE training_preferences (
    id                   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id              UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    training_days        SMALLINT[] NOT NULL DEFAULT '{1,3,5}',
    long_run_day         SMALLINT NOT NULL DEFAULT 6,
    strength_days        SMALLINT[] NOT NULL DEFAULT '{}',
    max_duration_per_day JSONB NOT NULL DEFAULT '[]',
    terrain              TEXT NOT NULL DEFAULT 'road' CHECK (terrain IN ('road', 'trail', 'mixed')),
    created_at           TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at           TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── Plans (metadata only) ──────────────────────────────────────────────────────
CREATE TABLE plans (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    race_goal      TEXT NOT NULL,
    race_goal_km   REAL,
    race_time_goal TEXT,
    race_date      DATE,
    terrain        TEXT NOT NULL DEFAULT 'road',
    num_weeks      SMALLINT NOT NULL,
    start_km       REAL NOT NULL DEFAULT 0,
    active         BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_plans_user_active ON plans(user_id) WHERE active = TRUE;

-- ── Plan Weeks ─────────────────────────────────────────────────────────────────
CREATE TABLE plan_weeks (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plan_id      UUID NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
    week_number  SMALLINT NOT NULL CHECK (week_number > 0),
    phase        TEXT NOT NULL CHECK (phase IN ('build_1', 'build_2', 'peak', 'taper', 'recovery')),
    target_km    REAL NOT NULL DEFAULT 0,
    is_recovery  BOOLEAN NOT NULL DEFAULT FALSE,
    notes        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(plan_id, week_number)
);
CREATE INDEX idx_plan_weeks_plan ON plan_weeks(plan_id);

-- ── Sessions ───────────────────────────────────────────────────────────────────
CREATE TABLE sessions (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plan_week_id        UUID NOT NULL REFERENCES plan_weeks(id) ON DELETE CASCADE,
    user_id             UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    weekday             SMALLINT NOT NULL CHECK (weekday BETWEEN 0 AND 6),
    session_type        TEXT NOT NULL DEFAULT 'rest',
    target_km           REAL NOT NULL DEFAULT 0,
    target_duration_min SMALLINT,
    target_hr_zones     SMALLINT[],
    notes               TEXT,
    sort_order          SMALLINT NOT NULL DEFAULT 0,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(plan_week_id, weekday)
);
CREATE INDEX idx_sessions_plan_week ON sessions(plan_week_id);
CREATE INDEX idx_sessions_user ON sessions(user_id);

-- ── Activities ─────────────────────────────────────────────────────────────────
CREATE TABLE activities (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id          UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_id       UUID REFERENCES sessions(id) ON DELETE SET NULL,
    source           TEXT NOT NULL DEFAULT 'manual',
    source_id        TEXT,
    activity_type    TEXT NOT NULL DEFAULT 'run',
    distance_km      REAL,
    duration_seconds INT,
    avg_pace_sec_km  INT,
    avg_hr           SMALLINT,
    max_hr           SMALLINT,
    elevation_m      REAL,
    calories         INT,
    feeling          SMALLINT CHECK (feeling BETWEEN 1 AND 5),
    pain             BOOLEAN DEFAULT FALSE,
    notes            TEXT,
    started_at       TIMESTAMPTZ,
    completed_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(source, source_id)
);
CREATE INDEX idx_activities_user ON activities(user_id, completed_at DESC);
CREATE INDEX idx_activities_session ON activities(session_id);

-- ── Injuries ───────────────────────────────────────────────────────────────────
CREATE TABLE injuries (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    locations   TEXT[] NOT NULL,
    severity    SMALLINT NOT NULL CHECK (severity BETWEEN 1 AND 10),
    can_walk    BOOLEAN NOT NULL DEFAULT TRUE,
    can_run     BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'recovering', 'resolved')),
    reported_at DATE NOT NULL DEFAULT CURRENT_DATE,
    resolved_at DATE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_injuries_user_active ON injuries(user_id) WHERE status != 'resolved';

-- ── Conversations ──────────────────────────────────────────────────────────────
CREATE TABLE conversations (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role       TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content    TEXT NOT NULL,
    metadata   JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_conversations_user ON conversations(user_id, created_at DESC);

-- ── Agent Events ───────────────────────────────────────────────────────────────
CREATE TABLE agent_events (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    trigger_type TEXT NOT NULL,
    tools_used   JSONB,
    plan_changes JSONB,
    latency_ms   INT,
    error        TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_agent_events_user ON agent_events(user_id, created_at DESC);

-- ── Plan Changes ───────────────────────────────────────────────────────────────
CREATE TABLE plan_changes (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plan_id        UUID NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
    user_id        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    change_type    TEXT NOT NULL,
    week_number    SMALLINT,
    before_state   JSONB NOT NULL,
    after_state    JSONB NOT NULL,
    reason         TEXT NOT NULL,
    agent_event_id UUID REFERENCES agent_events(id),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_plan_changes_plan ON plan_changes(plan_id, created_at DESC);

-- ── Strava Tokens ──────────────────────────────────────────────────────────────
CREATE TABLE strava_tokens (
    id                    UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id               UUID NOT NULL UNIQUE REFERENCES users(id) ON DELETE CASCADE,
    athlete_id            BIGINT NOT NULL,
    access_token          TEXT NOT NULL,
    refresh_token         TEXT NOT NULL,
    expires_at            TIMESTAMPTZ NOT NULL,
    scope                 TEXT NOT NULL DEFAULT '',
    strava_client_id      TEXT,
    strava_client_secret  TEXT,
    display_name          TEXT,
    avatar_url            TEXT,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ── OAuth Sessions ─────────────────────────────────────────────────────────────
CREATE TABLE oauth_sessions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    jwt          TEXT NOT NULL,
    email        TEXT NOT NULL,
    display_name TEXT,
    is_admin     BOOLEAN NOT NULL DEFAULT FALSE,
    is_new       BOOLEAN NOT NULL DEFAULT FALSE,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
