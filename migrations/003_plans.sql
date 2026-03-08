-- Training plan: generated per profile, weeks/days stored as JSONB
-- (A plan is always loaded whole; no need to normalize days into rows)
CREATE TABLE plans (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    profile_id  UUID NOT NULL REFERENCES profiles(id) ON DELETE CASCADE,

    -- Denormalized for quick access
    num_weeks   SMALLINT NOT NULL,
    race_date   DATE,
    race_goal   TEXT NOT NULL,

    -- Full plan stored as JSONB
    -- Structure: [{ week_number, phase, is_recovery, target_km, days: [...] }]
    weeks       JSONB NOT NULL DEFAULT '[]',

    active      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_plans_user_id ON plans(user_id);
CREATE INDEX idx_plans_active  ON plans(user_id, active) WHERE active = TRUE;
