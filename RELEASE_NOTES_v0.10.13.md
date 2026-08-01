# v0.10.13 — exhaustive `text = uuid` sweep post-mig-165

## Why this release

v0.10.9 realigned `fermi_forecasts / fermi_portfolios / fermi_notebooks` `owner_id`
from `UUID → TEXT` (targeting `users(user_id)`, the declared FK in mig 094).
Subsequent releases (v0.10.11, v0.10.12) fixed individual read sites as we
found them: JOINs on `u.id`, display-name COALESCE, dashboard reads.

That was whack-a-mole. This release does the audit properly — one grep pass
over every handler in `src/handlers/` for the two patterns that break under
the new schema:

1. `WHERE owner_id = $N::uuid` (or `VALUES (..., $N::uuid, ...)`) on fermi tables
2. `let owner_id: Uuid = row.get(...)` from fermi tables

...and fixes every remaining site.

## The blocker being fixed

Ilabra's dashboard showed 0 forecasts / 0 portfolios. Root cause:

```
GET /api/forecasts?scope=mine → HTTP/2 500
error returned from database: operator does not exist: text = uuid
```

Handlers matching pattern (1) or (2) 500'd → UI rendered empty state.

## Changes

### `src/handlers/polymarket.rs`

Three fermi_forecasts.owner_id sites, all dropped `::uuid` cast:

- `create_forecast_from_market_handler` (INSERT `VALUES ($1, $2::uuid, …)`)
- `check_resolutions_handler` (SELECT `WHERE f.owner_id = $1::uuid`)
- inside the same handler, UPDATE `WHERE owner_id = $7::uuid`

### `src/handlers/notebooks.rs`

Four fixes on `create_notebook_handler`, `get_notebook_handler`,
`execute_notebook_handler`:

- INSERT: `.bind(Uuid::parse_str(&user_id)?)` → `.bind(&user_id)` (TEXT
  matches TEXT column)
- Two `let owner_id: Uuid = row.get("owner_id")` → `let owner_id: String`
- Two `owner_id.to_string() != user_id` → `owner_id != user_id`
- One `owner_id: owner_id.to_string()` (into `NotebookPermissions { owner_id: String }`)
  → `owner_id: owner_id.clone()`

## Audit method

The following grep triple, run against `src/handlers/`, must return zero
hits related to `fermi_forecasts / fermi_portfolios / fermi_notebooks`:

```bash
grep -rn "::uuid" src/handlers/ --include="*.rs"
grep -rn "owner_id: Uuid" src/handlers/ --include="*.rs"
grep -rn "u\.id\s*=\s*[a-z_]+\.owner_id" src/handlers/ --include="*.rs"
```

Remaining `::uuid` hits (verified safe):

- `admin.rs:502,526` — `teams.id` is still UUID
- `admin_rbac.rs:418,422,565` — the substrate's own runtime type-detection
  helper (correct by design — it picks the right cast per column)
- `apps.rs:960` — `unnest($1::uuid[], $2::uuid[])` array unnest, unrelated
- `agents.rs:2746` — `o.session_id`, unrelated
- `polymarket.rs:849` — inline explanatory comment (not code)

Remaining `owner_id: Uuid` hits: none.

## Verification post-deploy

```bash
curl -si -H "Authorization: Bearer $ILABRA_TOKEN" \
     "https://agent-bestiary.world/api/forecasts?scope=mine&limit=50"
# Expect: HTTP/2 200 with Ilabra's forecasts array
```

## Known follow-ups (deferred, not this release)

- `eval_brier.rs:91,108` references a non-existent column `agents.owner_id`
  (should be `agents.user_id`). Latent bug on the calibration-lookup path;
  only fires when agent Brier evaluation runs. Not the current dashboard
  blocker. v0.10.14 candidate.

- Duplicate FK on `fermi_market_observations` (both `_fkey` validated and
  substrate `_fk NOT VALID` — both target `users(user_id)`, cosmetic). v0.10.14
  candidate.

- v0.11.0 "trust contract": boot-time schema-consistency check that compares
  `pg_get_constraintdef()` vs migration files and errors on drift. This is
  the substrate that would have caught the mig-094-vs-deployed-schema drift
  at boot instead of during Mo's first save attempt.
