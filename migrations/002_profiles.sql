-- Runner profile: one per user, collected during intake flow
CREATE TABLE profiles (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,

    -- Personal
    name            TEXT NOT NULL,
    age             SMALLINT NOT NULL CHECK (age > 0 AND age < 120),
    gender          TEXT NOT NULL,

    -- Experience
    running_years   TEXT NOT NULL,
    weekly_km       REAL NOT NULL CHECK (weekly_km >= 0),
    previous_ultra  TEXT NOT NULL DEFAULT 'none',

    -- Race times (optional, stored as text e.g. "45:30" or "3:45:00")
    time_10k        TEXT,
    time_half_marathon TEXT,
    time_marathon   TEXT,

    -- Race goal
    race_goal       JSONB NOT NULL,   -- serialized RaceGoal enum
    race_date       DATE,
    terrain         TEXT NOT NULL DEFAULT 'mixed',

    -- Training preferences
    training_days   SMALLINT[] NOT NULL,           -- [0,2,4,5] = Mon,Wed,Fri,Sat
    max_duration_per_day JSONB NOT NULL DEFAULT '[]', -- [{day, max_minutes}]
    long_run_day    SMALLINT NOT NULL DEFAULT 5,

    -- Heart rate
    max_hr          SMALLINT,
    rest_hr         SMALLINT NOT NULL DEFAULT 50,
    hr_zones        JSONB,

    -- Health
    sleep_hours     TEXT NOT NULL DEFAULT 'seven_to_eight',
    complaints      TEXT,
    previous_injuries TEXT[] NOT NULL DEFAULT '{}',

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(user_id)
);

CREATE INDEX idx_profiles_user_id ON profiles(user_id);
