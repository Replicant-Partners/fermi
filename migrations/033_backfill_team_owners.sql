-- Backfill: ensure every team owner is also a team member
INSERT INTO team_members (team_id, member_type, member_id, role, invited_by)
SELECT t.id, 'user', t.owner_id, 'owner', t.owner_id
FROM teams t
WHERE NOT EXISTS (
    SELECT 1 FROM team_members tm
    WHERE tm.team_id = t.id AND tm.member_id = t.owner_id
)
ON CONFLICT (team_id, member_id) DO NOTHING;
