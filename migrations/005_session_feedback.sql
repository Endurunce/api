-- Feedback per completed session (linked to plan + week + day)
CREATE TABLE session_feedback (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    plan_id         UUID NOT NULL REFERENCES plans(id) ON DELETE CASCADE,

    week_number     SMALLINT NOT NULL,
    weekday         SMALLINT NOT NULL CHECK (weekday BETWEEN 0 AND 6),

    -- Session outcome
    feeling         SMALLINT NOT NULL CHECK (feeling BETWEEN 1 AND 5),
    pain            BOOLEAN NOT NULL DEFAULT FALSE,
    notes           TEXT,

    -- Actual distance (may differ from target)
    actual_km       REAL,

    -- Strava
    strava_activity_id TEXT,

    -- Injury linked to this session (if any)
    injury_report_id UUID REFERENCES injury_reports(id) ON DELETE SET NULL,

    -- AI advice generated for this session
    ai_advice       JSONB,

    completed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(plan_id, week_number, weekday)
);

CREATE INDEX idx_feedback_plan_id ON session_feedback(plan_id);
CREATE INDEX idx_feedback_user_id ON session_feedback(user_id);
