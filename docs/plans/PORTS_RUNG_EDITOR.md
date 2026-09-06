# The ports rung — `accepts` editor

**Written 2026-09-03 as a fresh-session entry point.** Everything needed to
start is here.

**Status:** The `produces` half of the ports rung is closed by ContractBuilder
(derived from `produces_schema`). The `accepts` half has no editor anywhere.
This is the one declaration on the ladder that §6.8 of
`AGENT_COMPILE_AND_TOOL_REGISTRY.md` explicitly flagged as nobody's.

---

## 1. The goal

An author on the specimen shelf can edit `agents.accepts` — the labels this
agent declares it can consume. A single save closes the ports rung for any
agent that already has `produces` (via the contract compiler). No wizard, no
separate page, no form that can drift from the endpoint.

---

## 2. Measured facts

Re-derived at the time of writing. Do not re-measure to start; do re-measure
before claiming any of them fixed.

| fact | value | why it matters |
|---|---|---|
| agents with `accepts` declared | 93 of 96 real producing agents | 3 already gone. Ports is the cheapest rung; the coverage is high. |
| agents with `produces` but no `accepts` | check with `SELECT count(*) FROM agents WHERE cardinality(produces) > 0 AND cardinality(accepts) = 0` | the target population |
| distinct `accepts` labels in the fleet | 236 | see `LADDER.unlocks` |
| labels appearing on both `accepts` and `produces` | 13 | a seam exists between them |
| `PUT /api/agents/:agent_id` accepts `accepts` field | **yes** — `AgentUpdate.accepts: Option<Vec<String>>` in `agent-bestiary/memory/src/types.rs` L742 | no backend work required |
| `AgentFields` groups that are editable | intelligence, manage | not ports |
| check that guards the ports row from ContractBuilder | `check_specimen_shelf.js` L280–284 | read it before touching the shelf |

---

## 3. What already exists — read this before building anything

| thing | where | state |
|---|---|---|
| **AgentFields widget** | `static/js/widgets/agent-fields.js` | works. `AgentFields.mount({ container, agentId, group })`. Drives both Intelligence and Manage panels. The `FIELDS` table drives rendering and the save path. |
| **AgentFields `kind: "tags"`** | same file, `control()` function | already exists. Renders a comma-separated text input; `read()` splits on `,` and strips blanks. This is the exact input type `accepts` needs. |
| **`PUT /api/agents/:agent_id`** | `src/handlers/agents.rs::update_agent_handler` | works. Accepts `accepts: Option<Vec<String>>`. Only changed fields sent (diff save). |
| **ports rung row** | `templates/specimen.html::paneDeclaration()` or equivalent | rendered as read-only chips via `studChips(p.accepts, p.typed)`. No edit button. |
| **ports rung check** | `scripts/check_specimen_shelf.js` L280–284 | ACTIVE. It asserts the ports row does NOT claim ContractBuilder closes it. Adding an editor must not break this; adding the right editor should make a new assertion pass. |
| **`produces` is derived, not directly edited** | ContractBuilder closes it via `produces_schema` compile | Do not add a `produces` text editor. Editing `accepts` alone is correct. |
| **specimen shelf structure** | `templates/specimen.html` — three groups: Declaration, Intelligence, Manage | Declaration group has the ladder rungs. Intelligence and Manage mount `AgentFields`. |

---

## 4. The design

### 4.1 What `accepts` is

`agents.accepts` is a `text[]` column. An entry is a **label** — either:
- A bare noun the author invented (`"query"`, `"forecast-question"`)
- A schema ID the contract compiler produced (`"fermi/football_evidence"`)

They are not types. A stud connects where labels match, and `port_trust` runs
at execution time. The label is the declaration; whether it connects is a
measurement.

### 4.2 Where the editor goes

The ports rung row in the Declaration group of the shelf. The row grammar is
`value · condition · act`. For ports:

- **value** — the current `accepts` labels, shown as chips (already rendered)
- **condition** — `declared` / `missing` (already in the ladder check)
- **act** — **an inline tag editor for `accepts`**

The editor opens in-place (not a modal). It is the same `AgentFields` tag
input rendered directly, not mounted as a full panel. One text box, one save
button, labelled "Accepts".

The `produces` half remains read-only on this row, with a note: *"Produces is
set by the contract compiler — edit via the field contract editor."* This
matches what `check_specimen_shelf.js` L280–284 expects.

### 4.3 The save path

```
POST/PUT /api/agents/:agent_id   { "accepts": ["label-a", "label-b"] }
```

Same endpoint as Intelligence and Manage. Diff save — only `accepts` in the
body. The response is the full agent profile; refresh the rung display from it.

### 4.4 Label vocabulary

The editor is a free-text tag input (comma-separated, same as the `tags`
field). It does NOT enforce schema IDs. An author types what they mean; whether
labels connect is port_trust's job, not the editor's.

Optional enhancement (do not block on it): after saving, show which of the new
labels appear anywhere in the fleet's `produces` column. This is the stud
indicator `studChips` already computes. The enhancement requires no backend
change; the fleet data is already in the profile response.

---

## 5. Implementation

### 5.1 Add the field to `AgentFields.FIELDS`

In `static/js/widgets/agent-fields.js`, add one entry to `FIELDS`:

```js
// ── ports ─────────────────────────────────────────────────────────────
{ group: "ports", key: "accepts", path: "accepts",
  label: "accepts", kind: "tags",
  help: "Labels this agent declares it can consume. A stud connects where " +
        "another agent's `produces` carries a matching label. Bare nouns " +
        "and schema IDs (e.g. fermi/forecast_request) are both valid." },
```

No other changes to `agent-fields.js`. The `read()` function already handles
`kind: "tags"` (splits on comma, trims, filters empty).

### 5.2 Add the mount point in `specimen.html`

In the ports rung row inside `paneDeclaration()` (or wherever the ports row is
rendered), add:

```html
<!-- accepts editor — the one port declaration with no editor (§6.8) -->
<div id="af-ports" class="rung-act"></div>
```

Then in the boot sequence (after the agent profile is loaded):

```js
AgentFields.mount({
  container: document.getElementById("af-ports"),
  agentId: AGENT_ID,
  group: "ports",
  profile: profileData,   // the loaded agent profile
});
```

`AgentFields.mount` with `group: "ports"` will render only the `accepts`
field. Saves go to `PUT /api/agents/:agent_id` with `{ "accepts": [...] }`.

### 5.3 Keep `produces` read-only on the ports row

The existing `studChips(p.produces, p.typed)` display stays. Add a note
beneath it:

```html
<span class="rung-note">Produces is set by the contract compiler.</span>
```

This is required by `check_specimen_shelf.js` L280–284 (the check that the
ports row does NOT claim ContractBuilder closes it). The note must not say the
contract editor closes `accepts`.

### 5.4 Update `check_specimen_shelf.js`

Add one assertion:

```js
// The ports rung now has an editor for `accepts`.
ok(shelf.includes('id="af-ports"'),
  "the ports rung has no mount point for the accepts editor");
```

This guards against future regressions. It should be added alongside the
existing ports-row checks at lines 280–284.

---

## 6. Verification

1. **`node scripts/check_specimen_shelf.js`** — must pass with zero failures,
   including the existing ports check (L280–284) and the new assertion from §5.4.

2. **Manual: save a new label.** Open a specimen page for an agent that
   already has `produces`. Edit `accepts`, add a label, save. Reload. The
   new label appears as a chip, and the ports rung shows `declared`.

3. **Manual: first ports declaration.** Open a specimen for an agent with no
   `accepts` and no `produces`. Edit `accepts`, add one label. The rung
   changes from `missing` to `declared` (one of the two columns satisfied).

4. **No regression on Intelligence/Manage.** `AgentFields.mount` with
   `group: "intelligence"` and `group: "manage"` must still work identically —
   the `FIELDS` table addition is additive.

5. **Diff save check.** Opening the editor and immediately clicking Save
   (without changing anything) must send an empty body or `{ "accepts": [...] }`
   with the same value. A spurious write on no change is the defect
   `collect_changed_fields` was written to prevent.

---

## 7. Traps

- **Do not add a `produces` editor here.** `produces` is derived from
  `produces_schema` by the contract compiler. A free-text `produces` editor
  would overwrite the compiler's output and break the type match surface.
  `check_specimen_shelf.js` already guards this.

- **The check at line 280–284 must keep passing.** It asserts the ports row
  does not claim ContractBuilder closes it. Your new editor is for `accepts`
  only and has its own save path; ContractBuilder is not involved.

- **`produces` coming from the compile is additive (`merge_produces`).** See
  `AGENT_COMPILE_AND_TOOL_REGISTRY.md §6.8.0`. The compile adds the declared
  type at the front and removes nothing. An `accepts` editor that also touches
  `produces` breaks this invariant.

- **Labels are not enforced to be schema IDs.** `port_trust` handles
  disambiguation at runtime. Do not add client-side validation that refuses
  bare nouns — many existing agents use them and they are valid.

- **Empty `accepts` after save.** If the author clears the field and saves,
  `AgentUpdate.accepts = Some(vec![])`. The endpoint accepts this. The rung
  returns to `missing`. That is correct.

- **The `produces` column in the profile response is the compiler's output.**
  After saving `accepts`, refresh the full profile from the response and
  re-render the ports row from it. Do not try to preserve a locally-held
  `produces` value across the save.

---

## 8. What success looks like

The specimen shelf has a text input under the ports rung, labelled "Accepts",
that saves a comma-separated list of labels to `agents.accepts`. Saving it
closes the ports rung for any agent that already has `produces`. The
`check_specimen_shelf.js` check passes. No modal, no wizard, no second
endpoint.
