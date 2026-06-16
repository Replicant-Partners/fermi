# Deploy trigger 2026-06-16T13:43:11Z

Loosen sync-auto-hire auth: workspace members of an App can now sync
its auto_hire (not just App owners / platform admins). Curated platform
apps (sys-owned) need this so users who have spawned workspaces can
reconcile their fleet without an admin-scoped API key. The operation
remains idempotent and only adds agents declared in the App manifest.

After deploy:
  curl -X POST $API/api/apps/fermi_forecast/sync-auto-hire \
    -H "Authorization: Bearer $ABW_API_KEY"
should now succeed for the user who spawned the WC workspaces.
