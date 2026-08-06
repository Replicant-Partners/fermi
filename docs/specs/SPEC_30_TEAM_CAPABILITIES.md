# Spec 30 — Capabilities: separating "help me" from "close it"

**Status:** implemented (v0.11.10)
**Closes a hazard opened by:** Spec 26 (portfolio inheritance)

## 1. The hazard

Three facts that were individually reasonable and jointly a defect:

1. `resolve_forecast_handler` gated on `can_edit`.
2. Spec 26 made a portfolio team-share grant `edit` on every forecast inside
   it — and the console's own "share with team" button hardcodes `"edit"`.
3. Resolution is **irreversible**: migration 174's trigger freezes
   `scored_probability`, and `resolve_forecast()` requires `status='active'`,
   so a mis-resolution cannot be redone.

Therefore: **sharing a portfolio so colleagues could help silently delegated
the irreversible scoring decision to everyone on the team.** There was no way
to say "work on these with me" without also saying "you may close them".

It was also internally inconsistent. `resolve` was the *only* terminal action
gated at `edit`:

| Action | Effect | Gate before |
|---|---|---|
| Delete | removes the row | `has_admin()` |
| Void | discards *without* scoring | `has_admin()` |
| **Resolve** | **permanently writes the Brier score** | **`can_edit()`** |

Voiding needed admin; permanently scoring needed only edit.

## 2. Two axes, not one ladder

`team_members.role` is a ladder — viewer < member < admin < owner. It answers
*"who administers this team"*, and was being forced to also answer *"who may
do consequential work"*. A ladder cannot express the second question, because
the two concerns are orthogonal.

So migration 179 adds `team_members.capabilities TEXT[]`:

| | governs | values |
|---|---|---|
| `role` | administration of the team | viewer / member / admin / owner |
| `capabilities` | power over the team's **work** | `resolve`, `spend` |

This is EVE Online's split between a **Director** (administration) and a role
grant like **Accountant** (one specific power). A corp doesn't promote someone
to Director just so they can pay a bill.

Only `resolve` is enforced in v0.11.10. `spend` is declared because the
treasury slice needs the same column and a second migration on one column for
one feature family is churn — but nothing reads it, and that is stated rather
than implied. Tests assert it is never granted implicitly, so it cannot be
acquired by accident before it means something.

## 3. The new gate

`can_resolve_forecast` — two ways in:

1. **Object-admin** — you own it, hold an explicit `admin` share, or are a
   platform admin. This is what `delete` and `void` already required.
2. **Team `resolve` capability** — for teams that want to delegate closing
   without handing out object-admin.

`edit` alone is deliberately insufficient.

**Solo users are unaffected.** They own their forecasts, and ownership is
`Permission::Admin` in `can_access`. The tightening lands exactly on the
inherited/shared path, which is where the hazard was.

`void` now shares the same gate. It was already stricter than `resolve`, so
the inconsistency ran in both directions; routing both through one helper
means a team granted `resolve` can also retire a bad question — the same
authority expressed the other way.

## 4. Scoping: a capability is not global

`has_forecast_team_capability` asks *"do you hold this power on a team through
which **this** forecast is reachable?"* — walking the same three paths
`can_access` uses: team-owned, team-shared, or inherited from a team
portfolio.

Holding `resolve` on WC-analysts must not let you close an unrelated forecast
in a different team you happen to share with someone.

It deliberately does **not** re-apply Spec 26's leak guard. That guard stops a
portfolio share from *granting access* to a third party's private forecast.
Here access is already established; this answers the separate question of
whether your team standing permits a terminal action. Conflating them would
let an unrelated `edit` grant confer `resolve` — the exact bug being closed.

## 5. Backfill is a deliberate tightening

Owners and admins get `resolve`. Members and viewers do not. **A member who
could previously resolve a team-shared forecast now cannot until granted** —
that is the point, not a regression.

`TeamRole::default_capabilities()` mirrors the backfill exactly, and a test
asserts it, so "who can resolve" cannot come to depend on whether a member was
backfilled or added later.

Owners are exempt from capability edits (`set_member_capabilities` refuses
them) so a team cannot lock every terminal action out of itself. The roster
endpoint therefore reports owners as holding the full set regardless of the
stored column — the UI must show effective truth, not the row.

## 6. Grant surface

`PUT /api/teams/:id/members/:member_id/capabilities` — whole-set replacement,
team admins only. Whole-set rather than add/remove because the caller is a UI
rendering checkboxes, and a read-modify-write of individual grants silently
loses one of two concurrent admins' edits.

The handler is the validation boundary for the column: migration 179 ships no
CHECK constraint (an array-element CHECK must be dropped and recreated to add
a capability — the failure mode migration 157 exists to document), so
`TeamCapability::from_str` is strict and unknown values are a 400. A typo must
not read as a successful revocation.

Reads are forward-compatible: unrecognised strings are dropped, so a newer
node writing a capability this binary doesn't know cannot break its access
checks.

## 7. Not built

* **Enforcement of `spend`** — arrives with the treasury slice.
* **Per-portfolio or per-forecast capability grants.** Capabilities are
  team-scoped. Finer granularity is what `object_shares.permission='admin'`
  already provides.
* **A `curate` capability** (add/remove from team portfolios). Speculative
  until someone wants it.
