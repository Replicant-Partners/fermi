# Deploy trigger 2026-06-16T14:18:50Z

sync-auto-hire performance fix: replace 720-roundtrip nested loop with
a single bulk INSERT using Postgres `unnest` cross-product. Previous
version was timing out at the Railway proxy layer for the 60-workspace
WC fleet. New shape is one network roundtrip per call, sub-second.

After deploy:
  curl -X POST $API/api/apps/fermi_forecast/sync-auto-hire \
    -H "Authorization: Bearer $ABW_API_KEY"
should return in <2s with `hires_added` populated.
