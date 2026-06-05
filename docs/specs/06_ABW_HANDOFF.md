# SimOps v3 — ABW Handoff Brief

**For:** the ABW maintainer
**Status:** kask v3 surface is shipped and in production. The only
remaining ABW-side update before the v3 alpha can be smoke-tested
end-to-end is the `simops_companion` agent prompt.
**Time to ship:** ~10 minutes of file edits + a republish step.

---

## What to do

Update `agents/curated/simops_companion/agent_card.json` to the
**v3 strategist** card specified in
[`03_COMPANION_AGENT_CARD.md`](./03_COMPANION_AGENT_CARD.md).

The current deployed agent is the v2 "workbench navigator" — its
system prompt references seven modes (Intake, Compose, Scenarios,
Experiments, Twin, Chat, Operate) that no longer exist in the kask
client. Until the prompt is swapped, the page mounts and accepts
input but the companion answers in v2 vocabulary; the action grammar
that drives v3's structured edits never fires.

### The three things that change

1. **System prompt** — replace with the full text in
   `03_COMPANION_AGENT_CARD.md` § "The full system prompt".
   Two critical changes ride inside that prompt:

   - **The action grammar** — `__ACTION__ {…} __END_ACTION__` blocks
     for six action types, consumed by kask's `simops-actions.js`
     dispatcher. This is what makes the SimOps fleet actually close.

   - **Emergent design (no intake)** — the section
     "When the process is empty (emergent design)" at the bottom of
     the prompt body. Replaces the v2 idea of a fixed 6-turn intake
     interview with a behaviour rule: on an empty workspace, lead
     the user from blank to a visible first-draft process as quickly
     and as naturally as conversation allows. Propose stages on the
     user's first or second answer using sensible defaults; refine
     in place from there. No turn count, no script. Critical for
     the v3 alpha to feel natural — users should feel like the
     process is taking shape as they talk, not like they're walking
     through a form.

2. **`output_contract` block** — add the block from
   `03_COMPANION_AGENT_CARD.md` § "Capability declaration":
   ```json
   "output_contract": {
     "domain": "process_optimisation",
     "produces_schema": "kask_simops/action_block",
     "calibration": {
       "signal": "sosa_observation",
       "comparison": "predicted_vs_measured",
       "resolution_delay": "process_dependent"
     },
     "synthesis": "pipeline"
   }
   ```
   This is the Loop 5 hook. ABW reads but doesn't yet act on it; it's
   forward-compatible. Without this block, simops_companion isn't a
   genuine domain-constrained MoE strategist — just a router.

3. **Version bump** — `version: "1.0.0"` → `"2.0.0"`. Contract change,
   not a refinement.

Everything else in the agent card (model ladder, accepts/produces,
sample queries, tags, valence, etc.) stays as specified in
`03_COMPANION_AGENT_CARD.md` § "Capability declaration".

---

## How to verify it shipped

After republishing:

```sh
curl -s "https://agent-bestiary.world/api/agents?search=simops_companion&limit=1" \
  | python3 -c "
import sys, json
a = json.load(sys.stdin)['agents'][0]
sp = a.get('system_prompt', '')
print('version:', a.get('version'))
print('len(prompt):', len(sp))
print('output_contract present:', bool(a.get('output_contract')))
print('---first 400 chars---')
print(sp[:400])
"
```

**Pass (v3):**
- `version: "2.0.0"`
- `output_contract present: True`
- Prompt opens with "You are the SimOps Companion — the strategist
  agent inside the SimOps domain-constrained MoE app on kask.bio"
- Prompt mentions "action grammar" and `__ACTION__` blocks

**Fail (still v2):**
- `version: "1.0.0"` or missing
- `output_contract present: False`
- Prompt mentions "Intake / Compose / Scenarios / Experiments / Twin
  / Chat / Operate" modes

---

## Why this is the right unit of change

The agent card is **the contract**. The kask client speaks to ABW
exclusively through this agent — there are no new endpoints, no
schema changes, no DB migrations. The whole v3 cutover on the kask
side (4,939 lines deleted, 1,901 added) committed to honouring this
specific output_contract. Shipping the card is what makes the kask
investment into reality.

The action grammar in the prompt is also self-evolving from here on:
adding a new action type, refining the strategist's behaviour, or
tightening when it should ask vs auto-apply — every one of those is
a doc-03 + agent-card update, **not** a kask code change. The card
is the artifact that grows.

---

## What kask does once you ship

Nothing — kask v3 is already in production. The moment ABW serves
the v3 prompt, the next companion turn from any kask user begins
emitting `__ACTION__` blocks; `simops-actions.js` dispatches them;
the page renders the results inline. Zero kask-side coordination
needed.

The full 13-step smoke test (`05_SMOKE_TEST.md`) becomes runnable.

---

## Open questions / soft asks

These are nice-to-haves, not blockers:

1. **Tier ladder behaviour on premium-by-default.** The card requests
   Sonnet at premium and Haiku at standard. If your tier resolver
   has any quirks (e.g. silently falls back to OpenRouter free), let
   me know — premium-by-default is intentional for parsing-quality
   reasons, the action grammar is non-trivial JSON.

2. **Token budget per turn.** The card sets `max_tokens: 4096`. A
   typical companion turn is 200–800 tokens of prose + 50–500 tokens
   of action blocks. 4096 leaves headroom for compare-variations
   narratives. If you observe truncation, bump to 8192.

3. **Logging.** Every companion turn writes a `companion.turn` event
   in the workspace log on the kask side (event_append, cost_class
   free). If you want to record additional metadata server-side
   (Brier-like calibration prep), I can add fields to that event
   without code changes from your side.

4. **The Sensor Bridge.** No ABW work here yet. The bind affordance
   in kask is fully functional with zero registry — users paste URLs.
   When you're ready to think about federated discovery, see
   [`01_SENSOR_BRIDGE.md`](./01_SENSOR_BRIDGE.md) §"How the Sensor
   Bridge finds sources" for the `.well-known/sosa-sources`
   convention I'd like ABW to aggregate on opt-in.

---

## Contact / where to file issues

If anything is unclear or the action grammar refinement turns up
problems during real use, the spec docs in this directory are the
source of truth:

```
adaptogen/simops-v2/specs/v3/
  00_REFRAME.md           — mental model + page sketch
  01_SENSOR_BRIDGE.md     — tagged-union fields + binding contract
  02_WRAP_A_SOURCE.md     — public primer (also at kask.bio/docs/wrap-a-source)
  03_COMPANION_AGENT_CARD.md — this is the one you ship (THE artifact)
  04_CUTOVER_PLAN.md      — the kask side of the cut (for context)
  05_SMOKE_TEST.md        — the 13-step end-to-end verification
  06_ABW_HANDOFF.md       — this brief
```

Bugs / surprises / questions: open an issue in the kask repo with the
relevant doc + section referenced. The agent card prompt is the only
artifact whose contents the kask code depends on directly — keep that
one tight and the rest follows.
