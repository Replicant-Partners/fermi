-- Add status tracking to waitlist for invitation management
ALTER TABLE waitlist ADD COLUMN IF NOT EXISTS status TEXT DEFAULT 'pending';
ALTER TABLE waitlist ADD COLUMN IF NOT EXISTS invited_at TIMESTAMPTZ;
ALTER TABLE waitlist ADD COLUMN IF NOT EXISTS notes TEXT;

-- Index for filtering by status
CREATE INDEX IF NOT EXISTS idx_waitlist_status ON waitlist(status);
