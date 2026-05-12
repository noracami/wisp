CREATE TABLE IF NOT EXISTS reminders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    platform        TEXT NOT NULL DEFAULT 'discord',
    guild_id        TEXT NOT NULL,
    channel_id      TEXT NOT NULL,
    source_message_id TEXT,
    user_id         UUID NOT NULL REFERENCES users(id),

    body            TEXT NOT NULL,
    fire_at         TIMESTAMPTZ NOT NULL,

    fired_at        TIMESTAMPTZ,
    failed_attempts INT NOT NULL DEFAULT 0,
    last_error      TEXT,

    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS reminders_due_idx
    ON reminders (fire_at)
    WHERE fired_at IS NULL;
