-- Injury reports: separate table so we can query history independently
CREATE TABLE injury_reports (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    plan_id         UUID REFERENCES plans(id) ON DELETE SET NULL,

    reported_at     DATE NOT NULL DEFAULT CURRENT_DATE,

    locations       TEXT[] NOT NULL,    -- ['knee', 'achilles']
    severity        SMALLINT NOT NULL CHECK (severity BETWEEN 1 AND 10),
    can_walk        BOOLEAN NOT NULL DEFAULT TRUE,
    can_run         BOOLEAN NOT NULL DEFAULT TRUE,
    description     TEXT,

    recovery_status TEXT NOT NULL DEFAULT 'active',  -- active | recovering | resolved
    resolved_at     DATE,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_injuries_user_id ON injury_reports(user_id);
CREATE INDEX idx_injuries_active  ON injury_reports(user_id, recovery_status)
    WHERE recovery_status != 'resolved';
