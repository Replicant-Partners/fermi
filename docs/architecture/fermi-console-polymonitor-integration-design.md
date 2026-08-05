# Fermi Console — Polymonitor MCP Integration Design

**Status:** Draft for review
**Scope:** Position Health Card v1 + supporting ingestion layer
**External dependency:** `https://polymonitor.club/wm-api/mcp` (read-only, `mcp:read` key)

---

## 1. Purpose

Integrate polymonitor's read-only MCP endpoint (market / oracle / data-quality / briefing / runtime reads) into Fermi Console as the ground-truth substrate for:

- Reconstructed live pricing per position
- Oracle settlement/dispute status per position
- Category-level historical calibration context
- Data-confidence signaling

This document specifies the ingestion layer, the deterministic/agent boundary, caching tiers, provenance tracking, and failure modes, and proposes a phased build order.

**Non-goals for v1:** Fermi's own position/portfolio storage, order execution, and any write-path integration. This is read-only ingestion only.

---

## 2. Why this dependency needs special handling

Fermi's calibration signal (Brier scoring against resolved outcomes) is meant to be a **hard-verified signal class** — it resolves against ground truth and can't be gamed by plausible-sounding output. Pulling oracle settlement through a third party's API inserts an external dependency into that chain.

This is an acceptable trade (rebuilding Polygon indexing is not a good use of engineering time), **provided**:

1. Every value pulled from polymonitor is tagged with its source and checkpoint/timestamp at ingestion time.
2. Calibration scoring can distinguish "computed against fresh ground truth" from "computed against a stale or degraded read."
3. On endpoint failure, the system does **not** silently substitute a different (e.g. behavioral/price-based) signal in place of oracle ground truth. It pauses that specific calculation and flags it.

This is the single most important constraint in this document. Everything else is UI/plumbing; this is the thing that protects the credibility of Fermi's calibration claims.

---

## 3. Architecture overview

```
┌─────────────────────────────────────────────────────────┐
│                  Polymonitor MCP (external)               │
│   market reads │ oracle reads │ data-quality │ briefing │ runtime │
└───────────────────────────┬───────────────────────────────┘
                             │ mcp:read (scoped, revocable)
                             ▼
┌─────────────────────────────────────────────────────────┐
│              Deterministic Ingestion Layer                │
│  - thin MCP client wrapper                                │
│  - staleness check per call                                │
│  - price reconstruction (USDC/token ratio, drop mirrored) │
│  - local cache/mirror (Workspace-layer)                    │
│  - provenance tagging on every pulled value                │
└───────────────────────────┬───────────────────────────────┘
                             │
              ┌──────────────┴──────────────┐
              ▼                              ▼
   ┌─────────────────────┐       ┌─────────────────────────┐
   │  Loop 5a/5b path      │       │  Display/agent path       │
   │  (Brier scoring,      │       │  (position health card,   │
   │   calibration)        │       │   risk-triage narration)  │
   │  NO agent involvement │       │  agent narrates, never    │
   │  in this path          │       │  writes back into scoring │
   └─────────────────────┘       └─────────────────────────┘
```

---

## 4. Deterministic ingestion layer

### 4.1 Client wrapper

- Thin client on the versioned SDK, scoped to `mcp:read`.
- Treat the key/scope as a **ScopeLease**: time-bounded, revocable, logged grant — not a bare credential passed around.
- One wrapper module per read category (market, oracle, data-quality, briefing, runtime) rather than a single monolithic client — keeps failure isolation clean (e.g. oracle reads degrading shouldn't take down price reads).

### 4.2 Staleness check (every call)

- Compare polymonitor's reported sync checkpoint against wall-clock time.
- Define a staleness threshold per read category (oracle status likely needs a tighter threshold than category-level calibration stats).
- Attach `is_stale: bool` and `checkpoint_ts` to every returned record before it goes anywhere downstream.

### 4.3 Price reconstruction

- Do **not** use raw last-trade price directly.
- Reconstruct price from the USDC/token exchange ratio, per the polymonitor paper's own methodology.
- Explicitly filter known-polluted patterns (e.g. mirrored `BUY @ 1.0` rows) before any downstream consumption.
- This logic lives entirely in the deterministic layer — no agent involvement.

### 4.4 Local cache/mirror

- Maintain a local copy of pulled oracle settlement events (Workspace-layer materialized store).
- Purpose: Fermi's own historical calibration record does not depend on polymonitor's uptime for *past* scores — only new pulls depend on live availability.
- Cache invalidation:
  - Market metadata/category: cached at position-add time, rarely invalidated.
  - Category-level calibration stats: batch-refreshed (daily/weekly), not per-request.
  - Oracle status, live price: not cached beyond the staleness window; always re-pulled or explicitly marked stale.

### 4.5 Provenance tagging

Every value that reaches the calibration path or the UI must carry:

```
{
  value: ...,
  source: "polymonitor_mcp",
  source_checkpoint: <timestamp>,
  pulled_at: <timestamp>,
  is_stale: bool
}
```

This record should be written into Fermi's own provenance log (ΞPROV-equivalent) at ingestion time, not reconstructed after the fact. This is what allows retrospective audit of any calibration score: "was this computed against fresh ground truth?"

---

## 5. Deterministic vs. agent boundary

| Function | Layer | Rationale |
|---|---|---|
| Price reconstruction | Deterministic | Fixed rule, no judgment |
| Oracle status lookup, hours-since-anchor | Deterministic | Arithmetic on returned timestamps |
| Category calibration stats (Brier/ECE) | Deterministic | Precomputed batch stats |
| Data-confidence badge (linkage %, sync freshness) | Deterministic | Direct pass-through |
| **Brier scoring / Loop 5a/5b calibration** | Deterministic — no agent, ever | This is the hard-verified signal; an agent in this path reintroduces gameable, LLM-judged uncertainty |
| Market-family classification (ambiguous cases) | Agent | Requires semantic judgment on market titles/structure |
| Position risk-triage narration | Agent | Turns computed numbers into a readable sentence for the user |
| Briefing-endpoint synthesis (if raw text) | Agent, or pass-through if already synthesized — **check schema first** | Depends on what polymonitor's briefing reads actually return |

**Hard rule:** agent output is never persisted as if it were a fact feeding scoring. Agent narration is regenerated at render time from deterministic fields, not written back into the calibration path.

---

## 6. Position Health Card (v1 target feature)

### 6.1 Card panels

1. Reconstructed live price/probability
2. Category calibration context (historical efficiency of this market type)
3. Oracle state (resolved / pending / disputed + reentry-curve position if disputed)
4. Data-confidence badge (linkage %, sync freshness)

### 6.2 Call sequence per position (on load / refresh tick)

```
1. runtime.getFills(market_id, window=24h)         [live, 30-60s poll]
2. oracle.getStatus(market_id)                      [live, every open]
3. dataQuality.getLinkage(market_id)                [live, cheap]
4. market.getCategory(market_id)                    [cached at add-time]
5. IF category not in local calibration cache:
     → pull/compute historical calibration for category  [batch, cached]
6. IF oracle.getStatus == disputed:
     → oracle.getDisputeAnchor(market_id)            [live]
     → compute hours_since_anchor                     [deterministic, local]
```

### 6.3 Failure modes

| Condition | Behavior |
|---|---|
| MCP unreachable / rate-limited | Show last-cached price, visible "stale as of X" marker. No silent substitution of a different price source. |
| Low linkage on a specific market | Confidence badge downgrades; dispute-state panel shows "unconfirmed" rather than asserting a state. |
| Category has too few resolved markets for reliable calibration (paper's own threshold: <30 positive-fee markets excluded) | Panel shows "insufficient history" rather than a misleadingly precise stat. |
| Oracle sync lagging beyond staleness threshold | Loop 5a/5b calibration using this data pauses/flags rather than proceeding silently. |

---

## 7. Build phases

**Phase 1 (ship first — pure MCP pass-through + arithmetic, no batch job, no agent):**
- Panels 1 (price), 3 (oracle state), 4 (confidence badge)
- Client wrapper, staleness check, provenance tagging, cache/mirror scaffolding

**Phase 2 (needs batch calibration job + sufficient historical pulls):**
- Panel 2 (category calibration context)
- Category-level Brier/ECE computation, refreshed on schedule

**Phase 3 (agent layer):**
- Risk-triage narration for disputed positions
- Market-family classification for ambiguous cases (if/where needed beyond polymonitor's own categorization)
- Briefing-endpoint integration, pending schema review

---

## 8. Open questions for engineering to resolve before/during build

1. What SLA/uptime does polymonitor offer on the MCP endpoint? This matters because it's becoming a dependency for a signal Fermi is planning to sell trust on.
2. What does the `briefing` endpoint actually return — raw aggregates or already-synthesized text? This determines whether Phase 3's briefing integration needs an agent at all.
3. What's the appropriate staleness threshold per read category? (Likely tighter for oracle status than for category calibration stats — needs a number, not a placeholder.)
4. Rate limits on the `mcp:read` key — does polling cadence for live panels (price, oracle status) fit within them across Fermi's expected concurrent-user load?

---

## 9. Summary of hard constraints (do not relax without explicit sign-off)

- No agent output feeds the Brier/Loop 5a/5b scoring path, directly or indirectly.
- No silent fallback substitutes a different signal type when polymonitor data is stale/unavailable.
- Every value used in scoring carries a provenance record traceable to a specific polymonitor checkpoint.
