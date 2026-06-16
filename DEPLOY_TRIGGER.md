# Deploy trigger 2026-06-16T13:15:04Z

World Cup factor-model research orchestra — 3 new curated agents
(macro_data_agent, football_institution_agent, fixture_context_agent)
+ football_analyst v1.1 (factor X3-X5 awareness). All registered with
xaman_ek navigator. New endpoint:
POST /api/apps/:slug/sync-auto-hire — batch-reconciles existing
workspace agent rosters with the App's current auto_hire list.

After deploy, run once:
  curl -X POST $API/api/apps/fermi_forecast/sync-auto-hire \
    -H "Authorization: Bearer $ABW_API_KEY"
to hire the 4 new agents into the 60 existing WC team-prior + group-path
workspaces.
