# v0.10.29 — Publish-checks failure UX: case-sensitivity bug + actionable checklist

## Why

Ivan's follow-up to v0.10.27: he could force-publish Mario's
`efra_ai_01_scout` (the `updated_at` column landed, the whole
lifecycle path now works) — but when Mario himself clicked
Publish, all he saw was a red pill in the corner reading
literally:

```
Cannot publish:
```

No reasons. No guidance. Nothing.

**Root cause: case-sensitivity bug in the frontend filter.**
`CheckSeverity` is declared in `src/workflows/types.rs` with:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckSeverity { Error, Warning, Info }
```

So JSON severity values are `"error"` / `"warning"` / `"info"`
(lowercase). But three frontend sites filtered for `"Error"`
(capitalized):

```javascript
const failing = checks.checks.filter(
    (c) => !c.passed && c.severity === "Error",   // ← matches NOTHING
);
Toast.show("Cannot publish: " + failing.map(c => c.message).join("; "), "error");
```

`failing` is always empty → the message ends at "Cannot publish: "
with no reasons. Mario's screenshot was the smoking gun; every
"Cannot publish" toast on the platform has been rendering this
way since the code shipped.

**The `xaman-ek.js` App-creation flow got it right** —
`severity === "error"` (lowercase) — so App-blocking issues show
their reasons correctly. Only the agent-publish flow had the
typo. Silent for the same reason the other schema-drift bugs
were silent: the codepath wasn't reachable end-to-end until v0.10.15
opened admin bypass and v0.10.27 landed `agents.updated_at`.

## Change

### 1. Fix the case-sensitivity typo (three sites)

- `templates/agent_detail.html::publishAgent` — the owner flow
  Mario hit.
- `templates/agent_detail.html::adminPublishAgentDetail` — the
  admin flow from v0.10.15's "Publish (as admin)" button.
- `templates/admin.html::adminPublishAgent` — the admin table
  view's inline Publish button.

All three: `severity === "Error"` → `severity === "error"`.

Now the failing checks actually appear in the failure message.

### 2. Owner flow: `alert()` instead of `Toast.show()`, formatted checklist

`templates/agent_detail.html::publishAgent`

The Toast component is a fixed-position pill with no `max-width`
guard, no wrap rules, and a 4-second auto-dismiss. On the desktop
it renders the full text if you notice it in time; on narrow
viewports the multi-line check list can overflow the visible
area. The admin flows (v0.10.15) already switched to `alert()` for
readability — bringing the owner flow in line:

```javascript
const bulletList = failing.map((c) => `• ${c.message}`).join("\n");
alert(
  `Cannot publish yet — fix the following before retrying:\n\n${bulletList}\n\n` +
    `Edit these fields in the Manage tab, then click Publish again.`,
);
```

Mario now sees:

```
Cannot publish yet — fix the following before retrying:

• Description is required for publication
• System prompt is required for publication
• At least one tag is required

Edit these fields in the Manage tab, then click Publish again.
```

Modal, blocking, actionable, and points at where to fix the
issues.

## What Mario now sees on his agent

The full picture on `efra_ai_01_scout` (visible from the
screenshot):

- `DESCRIPTION` field empty → **Description is required**
- `SYSTEM PROMPT` field empty → **System prompt is required**
- `TAGS` field empty → **At least one tag is required**

Any one of these blocks publish. Post-v0.10.29, the alert lists
all three at once so Mario can fix them in one editing pass and
publish successfully — no admin force-publish needed.

## Note on numbering

Both my fix and Ivan's parallel `v0.10.28` (fermi-console charts
release) were in flight at the same time. Rather than collide,
this hotfix ships as **v0.10.29**. v0.10.28 is the charts
release; unrelated to this.

## What this release does NOT do

**Persistent publish-readiness checklist on the Manage tab.**
Would be nicer UX than a modal-on-click — an inline checklist
that updates as-you-type showing which checks pass and which
don't. Out of scope for this hotfix; the `alert()` fix is
enough to unblock Mario end-to-end today. Filed as v0.10.30
candidate:

```
┌──────────────────────────────────────────────────────────┐
│ PUBLISH READINESS                          [Publish btn] │
│                                                          │
│ ✓ Name set (efra_ai_01_scout)                            │
│ ✗ Description required                                   │
│ ✗ System prompt required                                 │
│ ✗ At least one tag required                              │
│ ⚠ No sample queries — users won't know how to use this   │
│ ⚠ Zero executions — test before publishing               │
└──────────────────────────────────────────────────────────┘
```

Live-updates via the same `/publish-checks` endpoint on debounced
input changes. Turns publishing into a visible progress bar
instead of a click-and-hope loop.

**Audit-log the empty-fields state for legacy agents.** ~34 of
Mario's agents share the same shape (empty description / prompt
/ tags). Adding a one-shot admin dashboard to list "agents that
would fail publish checks" is worth doing but not urgent — Mario
now has clear feedback per-agent and can walk his list himself.
v0.10.30 candidate.

## Post-deploy verification

Owner flow:

```bash
# Mario logs in, navigates to /agent/efra_ai_01_scout,
# clicks Publish. Sees:
#   Cannot publish yet — fix the following before retrying:
#     • Description is required for publication
#     • System prompt is required for publication
#     • At least one tag is required
#   Edit these fields in the Manage tab, then click Publish again.
```

Admin flow:

```bash
# Ivan clicks Publish (as admin) on the same agent. Same list
# appears in the reason-prompt as it did in v0.10.15, but now
# it actually enumerates the failing checks (was: empty in the
# prompt because `failing` was silently empty).
```

Direct JSON smoke test — confirm the API returns lowercase severity:

```bash
curl -s -H "Authorization: Bearer $IVAN_TOKEN" \
     "https://agent-bestiary.world/api/agents/efra_ai_01_scout/publish-checks" \
     | jq '.checks[] | {name, severity, passed, message}'
# → severity fields are all lowercase: "error", "warning", "info"
```

## Follow-up (still elevated)

Sixth "the platform assumed X but shipped Y" bug in the run
starting v0.10.15. Every one of them would have been caught by a
substrate check pre-deploy:

| Release | Assumption | Reality |
|---|---|---|
| v0.10.15 | `agents.owner_id` column | it's `user_id` |
| v0.10.16 | fork.rs `owner_id` column | same |
| v0.10.18 | `agents.updated_at` from mig-166 | PgBouncer ate the DO $$ block |
| v0.10.19 | `resolve_forecast()` returns FLOAT8 | it's REAL |
| v0.10.27 | mig-166 landed in prod | it hadn't; fixed via `ensure_critical_schema` |
| **v0.10.29** | **frontend filters against `"Error"` (capitalized)** | **Rust serde emits `"error"` (lowercase)** |

The v0.11.0 trust-contract now needs to cover both directions:
DB schema drift AND wire-format drift between Rust and the
frontend. A Rust ↔ JSON case-consistency check (all serde
`rename_all` values documented and enforced in TS/JS shape
tests) would have caught this one at CI, not first user click.

## Related

- v0.10.15 — admin force-publish path (introduced the same bug
  in the admin flows).
- v0.10.27 — `agents.updated_at` via `ensure_critical_schema`
  (unblocked the codepath that surfaced this).
- v0.10.28 — parallel fermi-console charts release (unrelated).
- v0.10.30 (candidate) — persistent publish-readiness checklist
  on the Manage tab; admin dashboard for "agents that would fail
  publish checks" bulk view.
- v0.11.0 — trust-contract, now with wire-format drift checks
  alongside schema-drift checks.
