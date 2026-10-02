# Ground your agent

Your agent runs wherever you run it. ABW grounds its output: every value the
agent claims came from a tool is checked against the tools it actually called
through ABW, and anything no tool could have supplied is removed before you use
it.

Checking is free. ABW tools called during a run are charged to your wallet
(`GAS_GROUND_TOOL_CALL`, 1 credit by default, only when the tool answers).

## What you need

1. An ABW account.
2. The agent registered on ABW, owned by you, with an `output_contract` on its
   card that says where each output field comes from
   (`sourced` from a named tool, `inferred`, `narrative` or `unavailable`).
   The publish check validates it. An agent with no contract can still run,
   but its output is graded `undetermined`: nothing about its content can be
   checked, and that is reported rather than passed.
3. An API key with the `ground:*` scope (or `ground:<agent_name>`), from
   Settings → API Keys → "Ground my agents".

## The three calls

```bash
# 1. Open a run. Returns a run token scoped to this run and the tools your
#    agent's card declares.
curl -X POST $ABW/v1/ground/runs \
  -H "Authorization: Bearer ferm_..." -H "Content-Type: application/json" \
  -d '{"agent": "my_species_agent", "query": "Profile Lucanus cervus"}'
# → { "run_id": "…", "run_token": "grun_…", "tools": ["gbif_taxonomy_tree", …],
#     "expires_at": "…", "contract": "declared" }

# 2. Call ABW tools through the run, as many times as the agent needs.
curl -X POST $ABW/v1/ground/runs/$RUN_ID/tools/gbif_taxonomy_tree \
  -H "Authorization: Bearer grun_..." -H "Content-Type: application/json" \
  -d '{"species": "Lucanus cervus"}'
# → { "tool": "…", "output": "…", "credits_charged": 1 }

# 3. Submit the agent's final output. The run closes.
curl -X POST $ABW/v1/ground/runs/$RUN_ID/output \
  -H "Authorization: Bearer grun_..." -H "Content-Type: application/json" \
  -d '{"response": "<the agent final text, JSON inside it is found>"}'
```

## What comes back

| field | meaning |
|---|---|
| `document` | **Use this.** The output with ungrounded values nulled. |
| `grounding.stripped` | The paths that were removed. |
| `grounding.provenance` | Per block: `tool_verified`, `tool_no_match`, `model_inference`, `unavailable_no_tool_source`. |
| `grounding.tools_called` | The tools this run called through ABW, which is what the verdict is based on. |
| `validation.status` | `valid`, `invalid`, or `unverified_*` (nothing to check against; never a pass). |
| `completeness.owed` | Fields the agent was asked for and left empty although a tool could have supplied them. |
| `reliance` | One word on whether the answer can be used, and why. |
| `trace_url` | The page showing each checkpoint and what it decided. |

## Rules worth knowing

- The run token works for one run only, for the tools your card declares, and
  stops working when the output is submitted or after an hour.
- Only tool calls made through the run count as evidence. A value your agent
  fetched some other way is graded as if no tool supplied it.
- What your agent originally wrote is kept on the episode, unchanged, so a
  later review can see what was claimed.
- Grounding does not make an output correct. It removes one class of error,
  values with no possible source. A sourced value can still be paraphrased
  wrongly.
