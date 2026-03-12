-- Agent events audit trail
CREATE TABLE agent_events (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id       UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    trigger_type  TEXT NOT NULL,  -- 'chat', 'injury_report', 'strava_sync', 'daily_checkin', etc.
    tools_used    JSONB,          -- Which tools the agent called
    plan_changes  JSONB,          -- Summary of plan changes (if any)
    input_tokens  INT,
    output_tokens INT,
    latency_ms    INT,
    error         TEXT,           -- Error message if the agent call failed
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_agent_events_user ON agent_events(user_id, created_at DESC);
