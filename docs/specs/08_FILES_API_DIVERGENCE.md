# 08 — Files API vs Action Protocol Divergence

**For:** the ABW maintainer
**From:** kask team (companion piece to `06_ABW_HANDOFF.md`)
**Status:** confirmed by CLI probe; potentially affects all ABW apps.
Kask-side mitigation shipped (commit `dffb58e` in `ilabra-axo/kask`);
fermi/ABW-side fix pending.
**Kask-side source of truth:** `adaptogen/simops-v2/specs/v3/12_FILES_API_DIVERGENCE.md`
in the kask repo (same content, different number to fit each repo's
existing spec sequence).

---

## Symptom

A workspace appears healthy in the kask UI (process loaded, KPIs
rendered, sidestreams visible), but `abw workspace files get
<ws-id> simops/process.yaml` returns 404. The "Last save" pipeline
row claims a recent successful commit; the action protocol log
confirms the mutation was applied with a sha; but the files API
cannot serve the document.

## Reproduction

Workspace `036a0022-2bc7-4b60-bfb6-bce123c65821`
(`kask-simops-32526662`), 2026-05-19:

```bash
# Action log says simops/process.yaml was applied with sha
# 2bd46421de46dc423d33aa42d168537b3c9fa332 at 23:47:33 UTC.
$ abw workspace actions list 036a0022-2bc7-4b60-bfb6-bce123c65821 --json \
    | jq '.actions[0] | {applied, apply_result, action_type}'
{
  "applied": true,
  "apply_result": {
    "path": "simops/process.yaml",
    "sha": { "sha": "2bd46421de46dc423d33aa42d168537b3c9fa332", ... }
  },
  "action_type": "mutate_document"
}

# Files API: 404.
$ abw workspace files get 036a0022-2bc7-4b60-bfb6-bce123c65821 simops/process.yaml
error: 404 Not Found: Git repository not found at: File not found:
       simops/process.yaml in workspace kask-simops-32526662

# Direct PUT round-trip — write a probe, read it back. The PUT
# returns an empty body (CLI's JSON parse error is decoding
# `""` from a 200/204), and the subsequent GET returns 404.
$ abw workspace files put 036a0022-2bc7-4b60-bfb6-bce123c65821 \
    probe.txt -c "probe" -m "test"
error: parsing response
  caused by: EOF while parsing a value at line 1 column 0

$ abw workspace files get 036a0022-2bc7-4b60-bfb6-bce123c65821 probe.txt
error: 404 Not Found: File not found: probe.txt
```

## Mechanism (hypothesis)

Looking at the kask flow for a single companion edit:

1. Action dispatcher (`simops-actions.js::edit_process`) calls
   `KaskSim.commitProcessState`
2. `commitProcessState` calls `ABW.writeWorkspaceFile`
   → `PUT /api/workspaces/<ws>/files/simops/process.yaml`
3. After (2) returns, the dispatcher calls `_postActionToLog`
   → `POST /api/workspaces/<ws>/actions/mutate_document`
4. `_postActionToLog` includes the entire serialised document as
   `payload.content`.

Step 2 appears to succeed at the HTTP layer (no exception) but
returns an empty body, and the file is not subsequently readable.
Step 4 succeeds and the payload is persisted in the action ledger.

The two endpoints likely write to different stores or the files PUT
short-circuits before the git commit step. Either way, what
**reaches kask's reader** (the files GET) doesn't match what the
ledger says was applied.

## Why this matters beyond kask

Any ABW app that:

- writes workspace files via `PUT /files/<path>`
- reads them back via `GET /files/<path>` to reconstruct state
- treats the workspace as the source of truth across sessions

...will exhibit the same silent data-loss-on-reload behaviour. This
includes (per `01_SENSOR_BRIDGE.md` and `02_WRAP_A_SOURCE.md`)
any operational digital twin, any sensor source binding flow, any
forecasting app that wants to persist its dataset, anything that
expects workspace YAML to round-trip.

The action protocol does work; the files API doesn't. Apps that
exclusively use the action protocol (POST `mutate_document` and
read back from `actions list`) would not see this bug, but most
non-trivial apps need direct file access for non-actioned content
(templates, agents/, ontology/, etc.).

## Evidence the bug is in the files API write path, not the action protocol

- Direct PUT (no action involved) on a workspace that has readable
  files (`README.md` exists, root listing works) silently fails:
  - Probe file at `probe-root.txt` is not created
  - Overwrite of existing `README.md` doesn't change its content on
    next read
- Action protocol `mutate_document` writes succeed and are queryable
  via `actions list`, with full `payload.content` intact
- The CLI consistently sees an empty response body from PUT
  endpoints across multiple attempts

## Kask-side mitigation shipped (`v=20260519v4j+1`)

The kask client now treats the files API as best-effort and falls
back to the action ledger for reads:

1. **`KaskSim.recoverProcessFromActions(workspaceId, path)`** —
   scans the action log for the most recent applied
   `mutate_document` targeting the given path, parses
   `payload.content` (JSON), returns the document.
2. **`loadProcess` fallback chain** — file listing → file read →
   action-log recovery → null. The recovered object is tagged
   `_recovered_from_action_log: true` (non-enumerable property)
   so downstream code can detect this without breaking YAML
   serialisation.
3. **`loadVariation` fallback chain** — same pattern, keyed on
   `simops/variations/<slug>.yaml`.
4. **Process-tab divergence banner** — when the current `_proc`
   was recovered from the action log, render a red warning band
   above the sankey explaining what happened.
5. **`commitProcessState` write detection** — when
   `writeWorkspaceFile` returns no `commit.sha` (the silent-no-op
   signature), `console.warn` so the failure is visible in
   DevTools rather than completely silent.

The user's data IS preserved by this path: every `_postActionToLog`
call carries the complete document content, so as long as the
action ledger keeps working, kask state survives reloads.

## Proposed fermi/ABW-side resolutions

### A — Fix the files PUT endpoint

The most direct: make `PUT /api/workspaces/<ws>/files/<path>`
actually write to the git store that GET reads from. Return the
new commit sha in the response body so callers can verify the
write landed. This restores the documented contract.

### B — Make the files API read from the action ledger

If the action protocol IS the truth (which it functionally is
right now), make the files GET endpoint synthesise responses
from the most recent applied mutation when the underlying git
blob is missing. Less ideal — it conflates two abstractions —
but pragmatic.

### C — Document files API as write-once / template-seeded only

If the design intent is that file writes only happen at workspace
spawn (from app templates) and post-spawn mutations should go
through the action protocol exclusively, codify this in the API.
Kask + other apps would then never call PUT and would always read
through actions. This is a significant API/conceptual shift but
arguably cleaner.

## Recommendation

**A.** The files API has the right shape — read-write workspace
YAML with git-backed history. It just doesn't currently work for
writes on this workspace (and possibly all SimOps workspaces).
Restoring the PUT contract is the smallest-surface change.

The kask-side action-log replay should remain as a defensive
fallback even after the fix — file APIs in production can fail in
many ways, and graceful degradation is better than data loss.

## Verification after fix

To confirm the fix landed, this should pass on any new SimOps
workspace:

```bash
WS=<workspace-id>

# 1. Direct PUT then GET round-trip should preserve content
abw workspace files put $WS probe.txt -c "hello" -m "test"
abw workspace files get $WS probe.txt    # → "hello"

# 2. After a kask edit, the file should be readable via files GET
#    with content matching the action ledger's payload.content
abw workspace actions list $WS --json | jq '.actions[0].payload.content' > /tmp/from-action.txt
abw workspace files get $WS simops/process.yaml > /tmp/from-file.txt
diff /tmp/from-action.txt /tmp/from-file.txt    # → (no output)

# 3. Recovery banner should not appear in the kask UI after page
#    reload on a freshly-edited workspace
```
