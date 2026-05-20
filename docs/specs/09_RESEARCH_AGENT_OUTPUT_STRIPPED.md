# 09 — Research-tier agent outputs stripped to "Evidence:" template

**For:** the ABW maintainer (agent execution runtime)
**From:** kask team (companion piece to `06_ABW_HANDOFF.md`,
`08_FILES_API_DIVERGENCE.md`)
**Status:** confirmed by direct API probe; affects every
research-tier agent invocation in the workspace
**Severity:** high — every comparator, supply_chain_oracle,
sidestream_miner result is content-free despite consuming full
token budgets

## Symptom

Research-tier agents (`agent_type: "research"`) return
`execution_result` messages whose `content` is always the literal
18-character string `"\n\n**Evidence:**\n- "` — regardless of what
the agent's LLM actually produced. Metadata says the execution
succeeded and consumed tens of thousands of tokens.

Sample (workspace `7214b1e2-d1ba-421d-89e1-fca9ae0968c3`):

```json
{
  "content": "\n\n**Evidence:**\n- ",
  "message_type": "execution_result",
  "sender_id": "supply_chain_oracle",
  "metadata": {
    "agent_name": "supply_chain_oracle",
    "status": "Success",
    "execution_time_ms": 43496,
    "tokens_used": 49200,
    "confidence": 0.5,
    "evidence_count": 1
  }
}
```

Three distinct invocations in this workspace, three different
research agents, all 18-char empty responses:

| Agent | Tokens used | Execution ms | Content length |
|---|---:|---:|---:|
| `supply_chain_oracle` | 49,200 | 43,496 | 18 |
| `supply_chain_oracle` | 71,933 | 51,535 | 18 |
| `comparator`          | 71,833 | 61,211 | 18 |

193,000 tokens consumed across three calls. Zero usable output.

## Reproduction

```bash
WS=7214b1e2-d1ba-421d-89e1-fca9ae0968c3
API_KEY=$(grep -oP 'api_key = "\K[^"]+' ~/.abw/credentials)

curl -sS -H "Authorization: Bearer $API_KEY" \
  "https://agent-bestiary.world/api/workspaces/$WS/messages?limit=50" \
  | jq '.messages[] | select(.message_type == "execution_result") |
        {agent: .metadata.agent_name,
         tokens: .metadata.tokens_used,
         status: .metadata.status,
         content_len: (.content | length),
         content_first_50: .content[:50]}'
```

Expected: each entry has substantial `content_len` containing the
agent's actual response. Observed: every entry has
`content_len: 18` and `content_first_50: "\n\n**Evidence:**\n- "`.

## Hypothesis on the mechanism

The agent execution runtime appears to be:

1. Calling the LLM with the agent's system prompt (correctly —
   tokens are consumed)
2. Receiving the LLM response (presumably non-empty)
3. Attempting to parse structured fields (evidence items, confidence
   score, etc.) out of the response
4. **Discarding the raw LLM output** and constructing the
   `execution_result` message from a template like
   `"\n\n**Evidence:**\n- {evidence_items_joined}"`
5. Writing that template into `content` even when no
   evidence items were extracted (so `evidence_count: 1` but the
   list of items is empty → template renders as `"- "` with no
   item text after the bullet)

The `evidence_count: 1` and `confidence: 0.5` in metadata are
suspicious — `confidence: 0.5` looks like a default-when-unknown,
and `evidence_count: 1` with empty content suggests the parser
created one phantom item placeholder and then couldn't fill it in.

## Impact

### On kask (current production)

- **`supply_chain_oracle`** — the BoM resolve workflow is broken.
  Users hit "Resolve via supply_chain_oracle" on a stage, the agent
  runs for ~45 seconds, kask's `_extractBomItems` finds nothing
  parseable, and surfaces "Oracle responded but no parseable BoM
  items were extracted." User assumes the kask parser is broken;
  it isn't — there's literally no content to parse.
- **`comparator`** — simulation results have empty narratives.
  The kask page falls back to "(no analysis)" and the user can't
  understand why their compared variations have no recommendation.
  See `simops/simulations/00MPDR3GYPH97IRLSF70.yaml` in the cited
  workspace — narrative is verbatim `"\n\n**Evidence:**\n- "`.
- **`sidestream_miner`** — companion proposes sidestreams from
  this agent's output. Affected.
- **All other research-tier agents** — same shape, same problem.

### Beyond kask

Any ABW app that consumes research-tier agent output via
`message.content` will hit this. Apps that read `metadata.confidence`
and `metadata.evidence_count` without checking content might
silently treat "agent responded" as "agent provided useful info."

The companion (`simops_companion`, agent_type likely
"conversational" or similar) is NOT affected — its messages come
back as direct chat replies, not `execution_result` envelopes, so
the LLM output is preserved.

## What ABW likely needs to fix

The runtime path that turns an LLM raw response into an
`execution_result` for research-tier agents. Specifically, whatever
constructs the `content` field. Three possibilities in descending
preference:

### A — Pass through the raw LLM response

Easiest: drop the templating step entirely. Put the LLM's full
output in `content`. Let the calling app parse whatever structure
it needs (kask's `_extractBomItems` is already defensive against
multiple shapes). The metadata fields (`confidence`,
`evidence_count`) can remain best-effort signals derived alongside.

This is also the simplest mental model: an agent execution result
*is* what the agent said.

### B — Keep the template but populate it correctly

If there's a real reason to template (e.g. UI consistency for
research outputs), then the template construction must actually
emit the parsed evidence items. Empty evidence list ⇒ empty
template ⇒ caller fails. At minimum, when the template would be
empty, fall back to including the raw LLM response.

### C — Emit raw + structured side-by-side

Add `metadata.raw_response` containing the LLM's verbatim output.
Callers like kask can read raw when the templated content is
useless. Backward-compatible because old callers ignore unknown
metadata keys.

## Recommendation

**A.** Simpler is better. The agents' system prompts already
instruct them on output format (supply_chain_oracle: "Return a
valid JSON object — no prose outside it"). Honoring that contract
means the LLM IS producing the structured output the caller wants;
no templating layer needed. Templating only makes sense when the
agent's output is conversational and the platform needs to add
research-result framing — but for tool-shape agents that emit JSON
contracts, templating destroys the contract.

If keeping templating is non-negotiable, **C** is the safe escape
hatch: add `metadata.raw_response` so callers can fall through.

## Post-fix verification

```bash
WS=<workspace-id>

# 1. Invoke supply_chain_oracle with a real BoM payload
abw workspace message $WS '{"task":"resolve_bom","stage":{...}}' -a supply_chain_oracle

# 2. Wait ~45 seconds for execution, then read messages
curl -sS -H "Authorization: Bearer $API_KEY" \
  "https://agent-bestiary.world/api/workspaces/$WS/messages?limit=5" \
  | jq '.messages[] | select(.sender_id=="supply_chain_oracle" and
                              .message_type=="execution_result") |
        {len: (.content | length),
         contains_items: (.content | contains("\"items\""))}'

# Expected: len > 100, contains_items: true
# Currently: len: 18, contains_items: false
```

## Workaround on kask side (interim)

None — without the underlying content, no parsing can recover it.
Kask can only surface the failure honestly ("Oracle responded but
no parseable items found") which it already does. The 49k-token
spend per failed invocation is wasted billable cost; users will
notice that quickly if they look at the budget meter.

## Cross-references

- `08_FILES_API_DIVERGENCE.md` — sibling platform-level bug
  discovered the same week
- `03_COMPANION_AGENT_CARD.md` — companion is unaffected because
  it's not research-tier
- Sample broken workspace: `7214b1e2-d1ba-421d-89e1-fca9ae0968c3`
- Affected agent cards in `agents/curated/`:
  `supply_chain_oracle/`, `comparator/`, `sidestream_miner/`,
  `regulatory_scanner/`, `product_scout/`
