# v0.10.20 — legacy agent-slug audit + rename

## Why

v0.10.16 closed the creation-time bypasses that let un-routable agent
names into the database. Its release notes named the remaining gap
explicitly:

> **Not in scope (v0.10.17 candidate):** Rename migration for the
> un-routable data already in the DB. `abw-cli agents legacy-slugs
> --dry-run / --apply`. Same invariant, different verb (heal existing
> data vs guard new writes).

This is that follow-up. v0.10.16 stopped the bleeding; this heals the
43 rows already in the table — Mario's `efra-ai/04-forensic` and its
siblings.

The names are unreachable because axum's tree router splits on `/`, so
`/agent/efra-ai/04-forensic` never matches `/agent/:name`. Names
containing `-` fail `slug::validate` on any subsequent write. Both
classes are invisible to the platform's own UI.

## Changes

### 1. `GET|POST /api/admin/agents/legacy-slugs`

`src/handlers/admin.rs` — `admin_legacy_agent_slugs_handler`

One handler, two modes, selected by the `apply` query param
(default `false`):

| call | effect |
|---|---|
| `GET  /api/admin/agents/legacy-slugs` | audit only |
| `POST /api/admin/agents/legacy-slugs` | audit only |
| `POST /api/admin/agents/legacy-slugs?apply=true` | execute rename |

Response shape is identical in both modes; only `action_taken` differs
per row, so a dry run is a faithful preview of the mutation.

Renames execute in a transaction and backfill the JSONB references in
`fermi_forecasts.agents_used`, so a renamed agent doesn't orphan the
forecasts that cite it. Every rename is written to
`admin_bypass_events` with the old → new mapping — the same audit
table v0.10.5 introduced for admin force-publish.

Platform-admin gated at the route, so `apply` needs no separate
permission check.

### 2. `sanitise_legacy_agent_name`

A conservative sanitiser scoped to this handler. `crate::slug::validate`
is reject-only and `apps::workspace_fork::slugify` is private to its
module, so neither could be reused directly.

- lowercase ASCII letters; keep digits and underscores
- every other character (`-`, `/`, `.`, space) becomes a single `_`,
  runs collapsed so we never emit `__`
- strip leading digits/underscores — slugs must start with a letter
- strip trailing underscores, truncate to 64 chars
- return `None` below 3 chars, or when the result collides with a
  reserved slug — those need manual attention rather than a guessed
  rename

Output is guaranteed to satisfy `slug::validate` when `Some`; the
caller re-validates defensively anyway.

### 3. `abw-cli admin legacy-slugs`

`crates/abw-cli/src/commands/admin.rs` (new), plus registration in
`commands/mod.rs` and `main.rs`.

CLI front-end for the endpoint, so the audit is runnable from a
terminal without hand-rolling curl against an admin token.

## Upgrade notes

No migration. The endpoint is inert until an admin calls it with
`?apply=true`.

**Run the dry run first.** Rows the sanitiser returns `None` for are
reported for manual review rather than skipped silently.

## Related

- v0.10.5 — `admin_bypass_events`, the audit table used here
- v0.10.16 — closed the creation-time bypasses that produced this data
- `d0f94e8` — `slug::validate` introduced (2026-05-23); everything
  created before this date was unchecked
