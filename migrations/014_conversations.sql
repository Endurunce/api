-- Agent conversation memory (replaces coach_messages over time)
CREATE TABLE conversations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role        TEXT NOT NULL,  -- 'user', 'assistant', 'tool_use', 'tool_result'
    content     JSONB NOT NULL, -- Text content or tool call/result
    token_count INT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_conv_user_time ON conversations(user_id, created_at DESC);

-- Conversation summaries for context window management
CREATE TABLE conversation_summaries (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    summary      TEXT NOT NULL,
    covers_until TIMESTAMPTZ NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_conv_summary_user ON conversation_summaries(user_id, created_at DESC);
