-- Delete test agents, keeping the database clean
DELETE FROM agents WHERE agent_name LIKE 'test_agent_%';

-- Show remaining count
SELECT COUNT(*) as remaining_agents FROM agents;
