-- Migration 073: Separate walk-in budget from invite pool
-- invite_pool = for contacts/invitees
-- walk_in_budget = for free walk-ins (walk_in_price = 0), caps host spending

ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS walk_in_budget INT NOT NULL DEFAULT 0;
ALTER TABLE swarm_events ADD COLUMN IF NOT EXISTS walk_in_budget_remaining INT NOT NULL DEFAULT 0;
