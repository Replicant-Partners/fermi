-- Waitlist for early access / notify me
CREATE TABLE IF NOT EXISTS waitlist (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT NOT NULL,
    source TEXT DEFAULT 'landing',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(email)
);
