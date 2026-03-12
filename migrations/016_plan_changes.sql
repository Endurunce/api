-- Plan change history — every agent modification tracked with before/after state
CREATE TABLE plan_changes (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    plan_id        UUID NOT NULL REFERENCES plans(id) ON DELETE CASCADE,
    user_id        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    change_type    TEXT NOT NULL,  -- 'update_week', 'set_rest_day', 'adjust_intensity', 'injury_adapt'
    week_number    INT,
    before_state   JSONB NOT NULL,
    after_state    JSONB NOT NULL,
    reason         TEXT NOT NULL,  -- Agent explains why the change was made
    agent_event_id UUID REFERENCES agent_events(id),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_plan_changes_plan ON plan_changes(plan_id, created_at DESC);
CREATE INDEX idx_plan_changes_user ON plan_changes(user_id, created_at DESC);
