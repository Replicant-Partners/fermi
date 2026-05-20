# 10 — Research-tier agents reliably produce empty LLM output

**For:** the ABW maintainer (agent execution runtime)
**From:** kask team (follow-up to issue #2 which closed the
content-channel templating bug)
**Status:** confirmed by direct API probe on workspace
`7214b1e2-d1ba-421d-89e1-fca9ae0968c3`; affects every
`agent_type: "research"` invocation tested
**Severity:** high — every comparator, supply_chain_oracle,
sidestream_miner invocation burns 50–73k tokens producing zero
usable output. The platform's content channel works (verified
empirically below); the LLM is the layer producing emptiness.

## Distinction from #2

Issue #2 closed the **content channel** bug: research-tier agents
used to have their LLM output stripped and replaced by a static
`"\n\n**Evidence:**\n- "` template. That fix landed:

- `content` is no longer the 18-char template — it's now a literal
  `"(agent returned no content)"` placeholder
- `metadata.raw_response` is added as a fallback channel for
  callers that want the verbatim LLM string

But the new shape exposes a different problem: the LLM is reliably
returning *nothing* for research-tier agents. The channel can carry
content; the agent isn't producing any.

## Evidence

Same workspace (`7214b1e2-d1ba-421d-89e1-fca9ae0968c3`), all
seven `execution_result` messages tabulated:

| Agent | Type | Tokens | Exec ms | content_len | raw_response_len |
|---|---|---:|---:|---:|---:|
| `simops_companion` | strategist | 64,424 | 61,545 | **20,636** | 0 (no key) |
| `comparator` | research | 71,833 | 61,211 | 18 (pre-fix template) | 0 (no key) |
| `supply_chain_oracle` | research | 71,933 | 51,535 | 18 (pre-fix template) | 0 (no key) |
| `supply_chain_oracle` | research | 49,200 | 43,496 | 18 (pre-fix template) | 0 (no key) |
| `supply_chain_oracle` | research | 73,347 | 72,103 | 27 ("no content" placeholder) | **0** |
| `supply_chain_oracle` | research | 49,384 | 41,406 | 27 ("no content" placeholder) | **0** |
| `sidestream_miner` | research | 53,848 | 55,494 | 27 ("no content" placeholder) | **0** |

The first row (`simops_companion`, `agent_type: "strategist"`) is
the control case. It uses the same `execution_result` channel,
the same `status: "Success"`, the same `evidence_count: 1` and
`confidence: 0.5` (which appear to be defaults), 64k tokens
similar to the failing ones — and produces 20,636 characters of
real content including `__ACTION__` blocks.

Every research-tier agent invocation in this workspace returned
zero usable bytes — pre-fix (templated as Evidence) and post-fix
(literal placeholder + empty raw_response).

## Reproduction (~60 seconds)

```bash
WS=7214b1e2-d1ba-421d-89e1-fca9ae0968c3
API_KEY=$(grep -oP 'api_key = "\K[^"]+' ~/.abw/credentials)

abw workspace message $WS \
  '{"items":[{"name":"Tea","unit":"kg","qty":1}]}' \
  -a supply_chain_oracle

sleep 60

curl -sS -H "Authorization: Bearer $API_KEY" \
  "https://agent-bestiary.world/api/workspaces/$WS/messages?limit=5" \
  | jq '.messages[] | select(.message_type=="execution_result") |
        {agent: .metadata.agent_name,
         tokens: .metadata.tokens_used,
         status: .metadata.status,
         content,
         raw_response_len: (.metadata.raw_response | length)}' \
  | head -20
```

Expected: substantial content reflecting the agent's documented
output contract (`{items: [...], risks: [...], ...}`).
Observed: `content: "(agent returned no content)"`,
`raw_response_len: 0`, `tokens: 49,384` (or similar).

## Hypothesis ladder

In descending probability:

### (1) Research-tier executor wraps the LLM in a tool loop that exits without emitting final text

The runtime's research-tier path likely does something like:
1. Send prompt to LLM
2. LLM responds with tool calls (`web_search`, `execute_agent`)
3. Runtime executes tools, feeds results back
4. Loop until LLM emits a final assistant message
5. Return that final message as `content` + `raw_response`

If step 4 is hitting a max-iterations cap or an unexpected
finish-reason and the runtime doesn't preserve the last
assistant text it saw, the result is empty content with full
tokens consumed.

Note that `comparator` has `mcp_tools: []` (no tools available)
and still fails the same way — so it's not strictly about tool
loops, but possibly about the executor path that's used for
research-tier regardless of whether tools are configured.

### (2) Research-tier executor uses a different model than companion-tier and that model is failing

The agent card declares the same model ladder:

```json
"model_ladder": [
  {"tier": "premium", "provider": "anthropic", "model": "claude-sonnet-4-6"},
  {"tier": "standard", "provider": "anthropic", "model": "claude-haiku-4-5-20251001"},
  {"tier": "free", "provider": "openrouter", "model": "openrouter/free"}
]
```

But the runtime may be picking different tiers based on
`agent_type`. If research-tier is being routed to `free` or
`standard` and the chosen model can't actually fulfil the
JSON-only contract the system prompt demands, it might return
empty.

The 50-73k tokens-used figure argues against tier-degradation
(free-tier models would likely have shorter context windows
that wouldn't allow that token count), but it's worth checking.

### (3) System-prompt structured-output mode rejecting the response

If the research-tier path applies an output schema (Anthropic's
`response_format: {type: "json_object"}` or similar) and the
LLM's response doesn't validate, the runtime might be returning
empty content instead of falling back to the raw text.

The agents' system prompts explicitly say "Return a valid JSON
object — no prose outside it." If the LLM produces JSON with
leading prose, a strict mode could reject the whole response.

### (4) Token-budget guard tripping silently

The runtime may be enforcing per-invocation token caps and
killing the response mid-generation, returning empty rather
than the partial. 73k tokens is on the high side; if there's
a cap around 70k, this would explain the truncation.

## What would tell us which hypothesis is right

Best diagnostic addition for ABW to ship next:

1. **Log the LLM's actual finish_reason** in `execution_result.metadata`
   (`stop`, `max_tokens`, `tool_use`, `error`, etc.)
2. **Log the last few iterations** of any tool loop in
   `metadata.iterations` so we can see if the loop ran
   indefinitely or exited cleanly with no final text
3. **Log the resolved model + provider + tier** that was actually
   used (not just declared on the agent card)

With those three fields, a single failed invocation tells you
exactly which hypothesis fired.

## What ABW likely needs to fix

Depends on which hypothesis is correct. In every case, the
mitigation pattern is the same:

> When the research-tier executor finishes a run, if `content` is
> empty AND `raw_response` is empty AND tokens were consumed,
> log loudly server-side, attach a `failure_reason` to
> `execution_result.metadata`, and never return Success without
> non-empty output.

The current `status: "Success"` with zero content is misleading —
the agent demonstrably didn't succeed at its declared task. The
caller can detect this client-side (kask does), but the runtime
itself should treat "Success" and "produced output" as the same
condition.

## Impact

Same as #2 — every BoM auto-resolve, comparator narrative, and
sidestream suggestion burns tokens and produces nothing. Users
must enter BoM items manually, simulation narratives are missing,
companion-proposed sidestreams from the miner don't materialise.

Workspace `7214b1e2` has consumed approximately **440,000 tokens**
across the seven failed research-agent invocations documented above.

## Kask-side mitigation (already shipped in [ilabra-axo/kask@049f6cf](https://github.com/ilabra-axo/kask/commit/049f6cf))

- Detect both pre-fix (Evidence template) and post-fix (literal
  placeholder + empty raw_response) empty states
- Error message includes `tokens_used` so the user sees the actual
  cost of the failed invocation
- Workaround hint: "enter BoM items manually, or retry after
  re-hiring the agent"

This is honest surfacing only; no recovery is possible without
the underlying content.

## Post-fix verification

```bash
# Same workspace, fresh invocation
WS=<workspace-id>
abw workspace message $WS \
  '{"items":[{"name":"Tea","unit":"kg","qty":1}]}' \
  -a supply_chain_oracle
sleep 60

curl -sS -H "Authorization: Bearer $API_KEY" \
  "https://agent-bestiary.world/api/workspaces/$WS/messages?limit=5" \
  | jq '.messages[] | select(.sender_id=="supply_chain_oracle") |
        {content_len: (.content | length),
         raw_len: (.metadata.raw_response | length),
         looks_structured: (.content | contains("\"items\"") or
                            (.metadata.raw_response | contains("\"items\"")))}'

# Expected: content_len > 100, looks_structured: true
# Currently: content_len: 27, raw_len: 0, looks_structured: false
```

## Cross-references

- Issue #1 — files API divergence (closed)
- Issue #2 — content channel templating (closed; this is the
  follow-up)
- `09_RESEARCH_AGENT_OUTPUT_STRIPPED.md` — original handoff for #2
- Sample workspace: `7214b1e2-d1ba-421d-89e1-fca9ae0968c3`
- Affected agent cards in `agents/curated/`:
  `supply_chain_oracle/`, `comparator/`, `sidestream_miner/`,
  and presumably `regulatory_scanner/`, `product_scout/` (untested
  but same `agent_type: "research"`)
- Control case that works: `simops_companion`
  (`agent_type: "strategist"`) — same channel, full 20k-char output
