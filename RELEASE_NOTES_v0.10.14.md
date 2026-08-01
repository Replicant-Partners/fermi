# v0.10.14 — one publish path, no fast lane

## Why

Prior to v0.10.13 publish 500'd for non-admin users (`text = uuid`), so
nobody ever completed a "click the Publish chip" flow. v0.10.13 fixed the
RBAC substrate and publish started working — which surfaced a UX
inconsistency that had been latent since **v0.8.8**:

- Clicking the **Publish chip** did a **direct publish as `private`**,
  with **no** visibility picker, **no** team-share step.
- Pressing **Ctrl+P** opened the full commit sheet (visibility +
  team-share targets).

Two paths to publish, one silent, one loud, no visible cue about which
you were on. Ivan noticed the mismatch on the first successful chip
publish under v0.10.13.

## Change

Chip click now dispatches the `PublishForecast` action — the same one
`Ctrl+P` triggers — instead of calling `Cockpit::publish_forecast(...)`
directly.

Result: **one publish path**, always via the commit sheet. Visibility
picker + team-share pills appear every time.

```rust
// crates/fermi-console/src/cockpit.rs — publish chip on_click
.on_click(cx.listener(|_this, _e, window, cx| {
    window.dispatch_action(Box::new(crate::PublishForecast), cx);
}))
```

No backend change — this is UI-layer only.

## Rationale

> "consistency is super consistent and highly informative
> — UX polish comes after core functions are consistent"
>                                       — Ivan, 03:26 UTC

Under-the-hood consequences:

- The commit sheet already coerces stale `team`/`shared` picker state
  down to `private` if `commit_sheet_visibility != "public"`, so the
  default lands where the fast-path used to (`private`) but the user
  can actually see and confirm/change it.
- The commit sheet warms the teams picker on-open (`fetch_teams` if
  `teams.is_empty() && !teams_loading`), so team-share is available on
  the first click even when the teams list hasn't been fetched yet.

## Not in scope (still deferred)

- `eval_brier.rs:91,108` — latent references to non-existent
  `agents.owner_id` column. Filed for a later hotfix.
- Duplicate FK on `fermi_market_observations` (harmless, cosmetic).
- Orphaned forecasts across dual-identity accounts (`ivan@axolotl` vs
  `ilabra@gmail`) — reassign UI TBD.
- v0.11.0 "trust contract" — boot-time schema-consistency check that
  compares `pg_get_constraintdef()` against migration files. This is
  what would have caught the mig-094-vs-deployed-schema drift the first
  time a non-admin OAuth user hit publish.
