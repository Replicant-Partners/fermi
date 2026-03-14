# Polymarket Integration — Design Document

**Date:** 2026-03-10
**Status:** Design → Building
**Author:** Fermi Team
**Relates to:** Phase 11 (Portfolio Management), Evidence System, Agent Pipeline

---

## Executive Summary

Polymarket is the world's largest prediction market (~$300M+ daily volume). Its public APIs expose real-time crowd-implied probabilities on thousands of events — the same kinds of questions Fermi users decompose and forecast.

### Three Core Features

1. **Import & Decompose:** Browse Polymarket from Fermi's Portfolio view, import a question, and run full Fermi decomposition on it. The link to the Polymarket market and its live price is permanently preserved inside the forecast.

2. **Three-Number Outside View:** Every linked forecast shows three probabilities — historical base rate, Polymarket crowd price (live), and Fermi inside view. The divergence between your model and the crowd is your edge signal.

3. **Auto-Resolution Brier Flywheel:** When Polymarket's oracle resolves a market, the linked Fermi forecast auto-resolves. Brier score computed automatically. Calibration data compounds with zero manual effort.

### The Core Loop

```
Polymarket question → Import into Fermi → Decompose (drivers + agents)
         ↑                                        ↓
         │                               Probability estimate
         │                                        ↓
         │                          Divergence from crowd price = EDGE
         │                                        ↓
         └──── Resolution (automatic via oracle) → Brier score → Calibration
```

All server-side, stateless, append-only. Follows ABW patterns.

---

## Architecture Principles

These align with ABW's existing design:

| Principle | Application |
|-----------|-------------|
| **Server-side** | All Polymarket API calls happen on ABW, not in the console. Console calls ABW endpoints. |
| **Stateless handlers** | Each request is independent. No Polymarket session state on the server. |
| **Append-only** | Market observations go into an append-only table. Never mutate — insert new snapshots. |
| **Agent-mediated** | A `prediction_market` orchestra agent interprets market data. Raw API is plumbing; the agent provides meaning. |
| **Credit-charged** | Market lookups cost credits (small fee, like `platform_read`). Position imports are free. |
| **Existing infra** | Uses SIWE wallet auth (migration 008), credit ledger (`platform_read` tx_type), fermi_forecasts table, and the agent execution pipeline. |

---

## Polymarket API Landscape

Three separate APIs, all on Polygon (MATIC/USDC):

| API | Base URL | Auth | Rate Limits |
|-----|----------|------|-------------|
| **Gamma** | `https://gamma-api.polymarket.com` | None (public) | ~100 req/min |
| **Data** | `https://data-api.polymarket.com` | None (public) | ~100 req/min |
| **CLOB** | `https://clob.polymarket.com` | HMAC + L1/L2 headers (wallet) | Varies by endpoint |

### Key Gamma Endpoints (Mode 1 — read-only)

```
GET /events?limit=N&active=true&order=volume24hr&ascending=false
GET /events?slug={slug}
GET /events?id={id}
GET /markets?id={market_id}
GET /markets?slug={slug}
```

**Event response shape** (simplified):
```json
{
  "id": "67284",
  "title": "Fed decision in March?",
  "slug": "fed-decision-in-march-885",
  "description": "...",
  "active": true,
  "volume": 306457581.59,
  "volume24hr": 8866038.85,
  "liquidity": 13537430.84,
  "markets": [
    {
      "id": "654414",
      "question": "Will there be no change in Fed interest rates after the March 2026 meeting?",
      "outcomePrices": "[\"0.9885\", \"0.0115\"]",
      "volume": "43870967.10",
      "liquidity": "1334800.82",
      "lastTradePrice": 0.989,
      "bestBid": 0.988,
      "bestAsk": 0.989,
      "oneWeekPriceChange": 0.0485,
      "oneMonthPriceChange": 0.1385
    }
  ],
  "tags": [{"label": "Economy", "slug": "economy"}, ...]
}
```

### Key Data Endpoints (Mode 2 — portfolio)

```
GET /positions?user={wallet_address}
GET /activity?user={wallet_address}
GET /trades?user={wallet_address}
GET /value?user={wallet_address}
```

### Key CLOB Endpoints (Mode 2 — pricing)

```
GET /prices-history?market={token_id}&interval=1d&fidelity=60
GET /midpoint?token_id={token_id}
GET /book?token_id={token_id}
```

---

## Mode 1: Evidence Source (Market-Implied Base Rates)

### Concept

When a user creates a Fermi forecast like "Will the Fed cut rates in March?", we search Polymarket for matching markets and surface:

1. **Market price** as a crowd-implied probability (evidence for or against the user's base rate)
2. **Volume and liquidity** as confidence signals (high-volume markets are more informative)
3. **Price history** for trend analysis (is the crowd moving toward or away from the user's estimate?)
4. **Divergence** between Fermi's inside view and the market's crowd price (edge signal)

### Data Flow

```
User types question in Fermi Console
         │
         ▼
Console sends question to ABW
  POST /api/polymarket/search
         │
         ▼
ABW server searches Gamma API
  GET gamma-api.polymarket.com/events?...
         │
         ▼
ABW returns matched markets with prices
         │
         ▼
Console displays as "Outside View: Prediction Markets"
  alongside the existing base rate
         │
         ▼
User accepts/rejects as evidence
  → stored as EvidenceStmt in the FPL program
  → stored in fermi_market_observations (append-only)
```

### Database Schema

New migration — append-only market observation table:

```sql
-- Migration NNN: Polymarket market observations
--
-- Append-only table recording every time we snapshot a Polymarket
-- market price in the context of a Fermi forecast. Never mutated.
-- Enables: divergence tracking over time, calibration against
-- crowd wisdom, and historical price series for resolved forecasts.

CREATE TABLE IF NOT EXISTS fermi_market_observations (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,

    -- What forecast this observation is linked to (optional — can be exploratory)
    forecast_id TEXT REFERENCES fermi_forecasts(id) ON DELETE SET NULL,

    -- Polymarket identifiers
    pm_event_id TEXT NOT NULL,          -- Polymarket event ID (e.g. "67284")
    pm_market_id TEXT NOT NULL,         -- Polymarket market ID (e.g. "654414")
    pm_condition_id TEXT,               -- On-chain condition ID (for CLOB lookups)
    pm_slug TEXT,                       -- Human-readable slug

    -- The question being asked on Polymarket
    pm_question TEXT NOT NULL,
    pm_description TEXT,

    -- Snapshot of market state at observation time
    market_price REAL NOT NULL          -- Last trade price (0.0–1.0)
        CHECK (market_price >= 0 AND market_price <= 1),
    bid_price REAL,                     -- Best bid
    ask_price REAL,                     -- Best ask
    spread REAL,                        -- ask - bid
    volume_total REAL,                  -- Lifetime volume (USD)
    volume_24h REAL,                    -- 24-hour volume
    liquidity REAL,                     -- Current liquidity depth
    open_interest REAL,                 -- Open interest if available

    -- Price changes at observation time
    price_change_1h REAL,
    price_change_1d REAL,
    price_change_1w REAL,
    price_change_1m REAL,

    -- Fermi state at observation time (for divergence tracking)
    fermi_probability REAL,             -- Fermi's inside view at this moment
    divergence_pp REAL,                 -- fermi_probability - market_price (in pp)

    -- Who triggered this observation
    observer_id TEXT NOT NULL REFERENCES users(user_id),

    -- Context
    observation_type TEXT NOT NULL DEFAULT 'search'
        CHECK (observation_type IN (
            'search',           -- user searched, we found a match
            'manual_link',      -- user manually linked a PM market to forecast
            'scheduled',        -- automated periodic snapshot
            'agent_research'    -- an agent pulled this during execution
        )),

    -- Metadata
    tags TEXT[] NOT NULL DEFAULT ARRAY[]::TEXT[],
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_market_obs_forecast
    ON fermi_market_observations(forecast_id)
    WHERE forecast_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_market_obs_pm_event
    ON fermi_market_observations(pm_event_id);
CREATE INDEX IF NOT EXISTS idx_market_obs_pm_market
    ON fermi_market_observations(pm_market_id);
CREATE INDEX IF NOT EXISTS idx_market_obs_observer
    ON fermi_market_observations(observer_id);
CREATE INDEX IF NOT EXISTS idx_market_obs_time
    ON fermi_market_observations(created_at);
CREATE INDEX IF NOT EXISTS idx_market_obs_type
    ON fermi_market_observations(observation_type);
```

### ABW API Endpoints (New)

All server-side, called by the console:

#### `POST /api/polymarket/search`

Search Polymarket for markets matching a query.

```json
// Request
{
  "query": "Will the Fed cut rates in March 2026?",
  "limit": 5,
  "active_only": true
}

// Response
{
  "matches": [
    {
      "pm_event_id": "67284",
      "pm_market_id": "654414",
      "title": "Fed decision in March?",
      "question": "Will there be no change in Fed interest rates after the March 2026 meeting?",
      "market_price": 0.989,
      "bid": 0.988,
      "ask": 0.989,
      "volume_24h": 2187707.18,
      "volume_total": 43870967.10,
      "liquidity": 1334800.82,
      "price_change_1w": 0.0485,
      "price_change_1m": 0.1385,
      "end_date": "2026-03-18T00:00:00Z",
      "tags": ["Economy", "Fed Rates"],
      "slug": "will-there-be-no-change-in-fed-interest-rates-after-the-march-2026-meeting",
      "polymarket_url": "https://polymarket.com/event/fed-decision-in-march-885",
      "confidence_signal": "high"  // based on volume + liquidity
    }
  ],
  "search_query": "Will the Fed cut rates in March 2026?",
  "results_count": 1,
  "credits_charged": 1
}
```

**Implementation:** Server-side handler that:
1. Calls `GET gamma-api.polymarket.com/events?limit=10&active=true` (with text matching heuristic)
2. Also tries `GET gamma-api.polymarket.com/events?slug={slugified-query}` for exact match
3. Ranks results by relevance to the query (title similarity + tag overlap)
4. Records each returned match as a `fermi_market_observations` row
5. Charges 1 credit (`platform_read` tx_type)

#### `POST /api/polymarket/link`

Explicitly link a Polymarket market to a Fermi forecast. Creates a persistent observation.

```json
// Request
{
  "forecast_id": "abc123",
  "pm_event_id": "67284",
  "pm_market_id": "654414"
}

// Response
{
  "observation_id": "obs_xyz",
  "market_price": 0.989,
  "fermi_probability": 0.15,
  "divergence_pp": -83.9,
  "message": "Polymarket crowd prices 'No change' at 98.9%. Your Fermi model estimates 15.0%. Divergence: -83.9pp."
}
```

#### `GET /api/polymarket/observations?forecast_id={id}`

Get all market observations for a forecast (time series).

```json
// Response
{
  "observations": [
    {
      "id": "obs_1",
      "market_price": 0.985,
      "fermi_probability": 0.15,
      "divergence_pp": -83.5,
      "created_at": "2026-03-08T10:00:00Z"
    },
    {
      "id": "obs_2",
      "market_price": 0.989,
      "fermi_probability": 0.18,
      "divergence_pp": -80.9,
      "created_at": "2026-03-10T14:00:00Z"
    }
  ],
  "pm_market_id": "654414",
  "pm_question": "Will there be no change in Fed interest rates after the March 2026 meeting?",
  "trend": "market_strengthening"
}
```

#### `POST /api/polymarket/snapshot`

Trigger a fresh snapshot of a linked market (re-fetch price from Gamma API).

### Agent Roster for Polymarket Research

The `fermi` decomposition agent naturally produces **binary drivers for catalysts** — discrete upcoming events that could move a market. These map directly to the FPL binary driver type:

```fpl
driver cpi_surprise_march_12 binary {
    probability: 0.10
    impact_multiplier: 2.5
    rationale: "CPI release March 12 — deviation >0.3% from consensus would shift Fed pricing"
}

driver emergency_meeting_call binary {
    probability: 0.01
    impact_multiplier: 10.0
    rationale: "Unscheduled FOMC meeting — extreme tail event"
}
```

This means we do NOT need a separate `event_catalyst` agent — catalysts are binary drivers with low probability and high impact, already native to Fermi's model.

#### New Agent (build)

| Agent | Role |
|-------|------|
| **`prediction_market`** | Interprets PM crowd prices, volume/liquidity signals, bid-ask spread dynamics, smart money vs retail flow. Answers: "Is this market efficient? What does the volume pattern tell us? Is the price stale?" |

#### New Agent (later)

| Agent | Role |
|-------|------|
| `resolution_analyst` | Stress-tests resolution criteria. "How exactly does this resolve? What are the edge cases? Is the UMA oracle reliable for this question?" |

#### Existing Agents (already serve PM workflow)

| Agent | PM Research Role |
|-------|-----------------|
| `fermi` | Decomposes into binary catalysts + continuous drivers |
| `macro_forecaster` | Fed, CPI, employment — many high-volume PM categories |
| `equity_analyst` | Earnings, guidance, company-specific PM markets |
| `entity_investigator` | "Will person X do Y?" — decision-maker context |
| `sentiment_analyzer` | Narrative momentum, public opinion shifts |
| `biotech_analyst` | FDA approvals, trial readouts — high-volume PM category |
| `nba_analyst` | Sports markets (significant PM volume) |

#### Example Orchestration for a PM-Imported Forecast

```
"Will the Fed cut rates in March?" (PM price: 1.1% YES)

Base rate: 1.1% (Polymarket crowd-implied, backed by $300M+ volume)

Drivers:
  fed_communication_tone    continuous  p5=0.8  p50=1.0  p95=1.1   ← gradual
  labor_market_trajectory   continuous  p5=0.9  p50=1.0  p95=1.05  ← gradual
  cpi_surprise_march_12     binary      prob=0.10  impact=2.5       ← CATALYST
  geopolitical_shock        binary      prob=0.03  impact=4.0       ← CATALYST
  emergency_meeting_call    binary      prob=0.01  impact=10.0      ← CATALYST

Agents:
  macro_forecaster → indicator dashboard, scenario analysis
  prediction_market → crowd price analysis, volume signals, spread
  sentiment_analyzer → Fed commentary narrative, market positioning
```

The binary drivers with big impact multipliers and low probabilities ARE the catalysts. The Fermi simulation handles them natively (sampled as Bernoulli draws in Monte Carlo).

### Price Normalization: The Three-Number Outside View

The console's outside view section shows THREE probability anchors for linked forecasts:

```
┌─────────────────────────────────────────────────────────────────┐
│  Outside View (Historical)                                      │
│  15.00% — Pre-revenue satellite companies reaching $200M (n=47) │
│                                                                 │
│  Outside View (Prediction Market)            🔗 Polymarket      │
│  8.50% — "Will ASTS hit $200M revenue by 2026?"                │
│  $2.1M volume · $340K liquidity · Medium confidence             │
│  📈 +1.2pp this week                                            │
│  [Refresh] [Use as base rate]                                   │
│                                                                 │
│  Inside View (Your Model)                                       │
│  22.30% — Fermi simulation (6 drivers, 10K iterations)          │
│                                                                 │
│  ┌──────────────────────────────────────────────┐               │
│  │ DIVERGENCE: +13.8pp above crowd              │               │
│  │ Your model is more bullish than the market.  │               │
│  │ Is this alpha or overconfidence?              │               │
│  └──────────────────────────────────────────────┘               │
└─────────────────────────────────────────────────────────────────┘
```

#### Normalization Rules

1. **PM price IS probability** — `market_price` (0.0–1.0) = implied probability. No transformation needed. A market at 0.652 = 65.2% crowd-implied YES.

2. **Use midpoint for stability** — `(best_bid + best_ask) / 2` is more stable than `last_trade_price` which can be stale. Fall back to last trade if bid/ask unavailable.

3. **Multi-outcome aggregation** — Fed decision has 4 markets (50bp cut, 25bp cut, no change, 25bp hike). For "Will the Fed cut?", the server sums: `P(cut) = P(50bp cut) + P(25bp cut)`. This aggregation happens server-side before returning to the console.

4. **Spread → confidence signal** — tight spread (< 1%) = high confidence, wide spread (> 5%) = thin market. Maps to evidence quality scoring:

   | Spread | Volume 24h | Classification | Quality Score |
   |--------|-----------|----------------|--------------|
   | < 1% | > $1M | Very High | 0.95 |
   | < 2% | > $100K | High | 0.80 |
   | < 5% | > $10K | Medium | 0.60 |
   | > 5% | < $10K | Low | 0.30 |

5. **PM price can REPLACE base rate** — "Use as base rate" button sets `base_rate.historical_frequency = pm_price` with `source = "Polymarket crowd-implied"`. Then Fermi's drivers adjust FROM the crowd price, which is the natural Tetlock framing: anchor on the outside view (crowd), adjust with inside view (drivers).

6. **Divergence always computed and stored:**
```
divergence_pp = fermi_probability - market_price
```
Positive = you think MORE likely than crowd. Negative = LESS likely. This is your edge estimate.

#### Divergence Tracking

The most valuable signal is divergence — when your Fermi model disagrees with the crowd. This is either:

- **Alpha** (you know something the market doesn't) → your decomposition has edge
- **Error** (you're wrong and the crowd is right) → update toward market

Each time a linked forecast is opened, the PM price is re-fetched and stored as an observation. Over time this builds a time series per forecast. When it resolves, we compute:

- Was the crowd or your Fermi model more accurate?
- At what divergence levels do you have genuine alpha vs overconfidence?
- Per-domain: "In politics, when you diverge >20pp from Polymarket, you're right 30% of the time"

This feeds directly into Phase 13: Intelligence Features (calibration feedback).

---

## Mode 2: Portfolio Companion (Polymarket Position Sync)

### Concept

Users who trade on Polymarket can import their positions into Fermi:

1. Connect wallet via SIWE (already implemented in ABW)
2. Fetch positions from Data API using their wallet address
3. For each position, create or link a Fermi forecast
4. Run Fermi decomposition to assess whether the position has edge
5. Track P&L alongside Fermi probability updates

### Prerequisites

- SIWE wallet connection (migration 008, handlers in `auth.rs`) ✅ exists
- User's `ethereum_address` stored in `users` table ✅ exists
- Credit ledger for charging ✅ exists
- `fermi_forecasts` table with `metadata` JSONB field ✅ exists

### Data Flow

```
User clicks "Import Polymarket Positions" in Portfolio panel
         │
         ▼
Console calls ABW with user's JWT
  POST /api/polymarket/import-positions
         │
         ▼
ABW looks up user's ethereum_address
ABW calls Data API:
  GET data-api.polymarket.com/positions?user={address}
         │
         ▼
For each position:
  - Find or create fermi_forecast with pm_market_id link
  - Snapshot market price → fermi_market_observations
  - Return position data to console
         │
         ▼
Console shows positions in Portfolio panel:
  "Fed March: 98.9% · You hold YES · Size: $500 · P&L: +$47"
  [Run Fermi Analysis] [Link to Forecast] [Refresh]
```

### Database Schema (Additions)

```sql
-- Polymarket position snapshots (append-only)
-- Records user's position state at a point in time.
-- Enables P&L tracking and edge analysis over time.

CREATE TABLE IF NOT EXISTS fermi_pm_positions (
    id TEXT PRIMARY KEY DEFAULT gen_random_uuid()::text,
    user_id TEXT NOT NULL REFERENCES users(user_id) ON DELETE CASCADE,
    wallet_address TEXT NOT NULL,

    -- Polymarket identifiers
    pm_event_id TEXT NOT NULL,
    pm_market_id TEXT NOT NULL,
    pm_condition_id TEXT,

    -- Position details
    outcome TEXT NOT NULL,              -- "Yes" or "No"
    size REAL NOT NULL,                 -- Position size in USDC
    avg_price REAL NOT NULL,            -- Average entry price
    current_price REAL NOT NULL,        -- Market price at snapshot
    unrealized_pnl REAL,               -- current_price * size - avg_price * size
    realized_pnl REAL,                 -- From closed portions

    -- Link to Fermi forecast (if user has created one)
    forecast_id TEXT REFERENCES fermi_forecasts(id) ON DELETE SET NULL,

    -- Fermi edge analysis
    fermi_probability REAL,             -- Fermi model's probability at snapshot time
    edge_estimate REAL,                 -- fermi_probability - current_price (positive = Fermi says underpriced)

    -- Metadata
    snapshot_type TEXT NOT NULL DEFAULT 'import'
        CHECK (snapshot_type IN ('import', 'refresh', 'scheduled')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_pm_positions_user ON fermi_pm_positions(user_id);
CREATE INDEX IF NOT EXISTS idx_pm_positions_market ON fermi_pm_positions(pm_market_id);
CREATE INDEX IF NOT EXISTS idx_pm_positions_forecast ON fermi_pm_positions(forecast_id)
    WHERE forecast_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_pm_positions_time ON fermi_pm_positions(created_at);
```

### ABW API Endpoints (Mode 2)

#### `POST /api/polymarket/import-positions`

Requires authenticated user with `ethereum_address` set.

```json
// Response
{
  "wallet": "0x1234...5678",
  "positions": [
    {
      "pm_event_id": "67284",
      "pm_market_id": "654414",
      "title": "Fed decision in March?",
      "question": "Will there be no change in Fed interest rates?",
      "outcome": "Yes",
      "size": 500.00,
      "avg_price": 0.95,
      "current_price": 0.989,
      "unrealized_pnl": 19.50,
      "forecast_id": null,
      "edge_estimate": null
    }
  ],
  "total_value": 1247.50,
  "total_pnl": 87.30,
  "positions_count": 3,
  "snapshot_ids": ["snap_1", "snap_2", "snap_3"]
}
```

#### `POST /api/polymarket/analyze-position`

Run Fermi decomposition on a Polymarket position to assess edge.

```json
// Request
{
  "pm_market_id": "654414",
  "pm_question": "Will there be no change in Fed interest rates after the March 2026 meeting?"
}

// Response
{
  "forecast_id": "fc_newlycreated",
  "fermi_probability": 0.97,
  "market_price": 0.989,
  "edge_estimate": -0.019,
  "analysis": "Fermi model estimates 97.0% vs market 98.9%. Edge is -1.9pp — within noise. Market appears fairly priced.",
  "agents_used": ["macro_forecaster", "prediction_market"],
  "credits_charged": 32
}
```

### CLOB Integration (Future — Trading)

Not in scope for initial release, but the architecture supports it:

1. `rs-clob-client` crate (Rust, official) handles order signing and submission
2. Requires Rust 1.88+ — we're on 1.85 but can upgrade when needed
3. CLOB auth uses HMAC signatures derived from the user's Polygon wallet key
4. ABW already has SIWE — extending to CLOB auth is a key derivation step
5. Trading would go through a new `POST /api/polymarket/trade` endpoint
6. Each trade logged in the credit ledger as a new tx_type (`polymarket_trade`)

---

## Implementation Plan

### Build 1: Import → Decompose → Track (ships first — 3-5 days)

The core loop: import a PM question into Fermi, decompose it, keep the PM price linked.

1. **Migration**: `fermi_market_observations` table (append-only price snapshots)
2. **Server module**: `src/polymarket/mod.rs` — Gamma API client (`GammaClient`)
3. **Server handler**: `POST /api/polymarket/search` — proxy to Gamma, return matched events with prices
4. **Server handler**: `POST /api/polymarket/snapshot` — fetch current price for a linked market, append observation
5. **Console Portfolio**: "Import from Polymarket" button → search panel → select → creates `fermi_forecast` with `metadata.polymarket` link → opens Composer
6. **Console Composer**: Three-number outside view (historical + PM crowd + model) with divergence display
7. **Tool handler**: `polymarket_search` in `tools.rs` so orchestra agents can call it during research

### Build 2: Auto-Resolution Brier Flywheel (2-3 days)

Close the calibration loop automatically.

1. **Server handler**: `POST /api/polymarket/check-resolutions` — scan all linked forecasts, check if PM market resolved
2. **Resolution bridge**: When PM market shows `closed: true` with clear outcome, call `resolve_forecast()` stored procedure
3. **Append to `fermi_forecast_updates`**: `reason: "Auto-resolved via Polymarket oracle"`
4. **Brier score**: Computed automatically by existing stored procedure
5. **Background job**: Periodic resolution check (every 15 minutes, or triggered by user refresh)

### Build 3: prediction_market Agent (2 days)

Add the agent that interprets PM data for the orchestra.

1. **Agent card**: `agents/curated/prediction_market/agent_card.json`
2. **System prompt**: Market microstructure expertise — volume interpretation, spread analysis, smart money signals, efficiency assessment
3. **Tools**: `polymarket_search`, `polymarket_price`, `polymarket_event` — backed by server-side Gamma calls
4. **Register on ABW**: Via update script
5. **Console routing**: Questions imported from Polymarket auto-assign `prediction_market` agent alongside domain-specific agents

### Future: Position Import & Edge (Mode 2)

After the core loop works, extend to portfolio tracking.

1. **Migration**: `fermi_pm_positions` table
2. **Server handler**: `POST /api/polymarket/import-positions` (needs `ethereum_address` from SIWE)
3. **Console Portfolio**: Show PM positions with edge analysis (Fermi probability vs market price)
4. **Auto-link**: Match imported positions to existing Fermi forecasts by question similarity

### Future: CLOB Trading (Mode 3)

1. Upgrade to Rust 1.88+ (for `rs-clob-client`)
2. `POST /api/polymarket/trade` endpoint
3. Console trading UI (buy/sell based on Fermi edge signal)

---

## Integration with Existing Systems

### Credit Ledger

New tx_type needed in credit_ledger CHECK constraint:

```sql
'polymarket_search'   -- charged when searching Gamma API
'polymarket_import'   -- charged when importing positions
'polymarket_snapshot'  -- charged when refreshing a linked market
```

### Agent Pipeline

The `prediction_market` agent fits the existing orchestra pattern:
- Tagged `fermi-orchestra`
- Called during auto-assign if question matches a Polymarket market
- Evidence flows through the same `process_agent_evidence` path
- Quality scoring works on Polymarket evidence (high source quality, quantitative data)

### fermi_forecasts.metadata

Use the existing JSONB `metadata` field to store Polymarket linkage:

```json
{
  "polymarket": {
    "pm_event_id": "67284",
    "pm_market_id": "654414",
    "pm_slug": "fed-decision-in-march-885",
    "linked_at": "2026-03-10T14:00:00Z",
    "last_snapshot": "2026-03-10T20:00:00Z",
    "last_market_price": 0.989
  }
}
```

### fermi_forecast_updates

When Polymarket price is adopted as base rate:

```json
{
  "reason": "Adopted Polymarket crowd price (98.9%, $43.8M volume)",
  "agent_id": "prediction_market",
  "evidence_added": {
    "source": "Polymarket",
    "pm_market_id": "654414",
    "market_price": 0.989,
    "volume": 43870967
  }
}
```

### SIWE Wallet (Mode 2)

Already implemented:
- `migrations/008_add_siwe_nonces.sql` — nonce table for replay protection
- `handlers/auth.rs` — `siwe_challenge_handler` + `siwe_verify_handler`
- `users.ethereum_address` column — stores connected wallet
- `users.ens_name` column — human-readable ENS name

For Mode 2, we just need to read `ethereum_address` from the authenticated user's profile and pass it to the Data API. No new wallet infrastructure needed.

---

## Gamma API Client (Server-Side)

Lightweight HTTP client module — no external crate needed. The Gamma API is plain REST with no auth:

```rust
// src/polymarket/mod.rs

pub struct GammaClient {
    client: reqwest::Client,
    base_url: String,  // https://gamma-api.polymarket.com
}

impl GammaClient {
    pub async fn search_events(&self, query: &str, limit: usize) -> Result<Vec<PolyEvent>>;
    pub async fn get_event(&self, event_id: &str) -> Result<PolyEvent>;
    pub async fn get_event_by_slug(&self, slug: &str) -> Result<PolyEvent>;
    pub async fn get_market(&self, market_id: &str) -> Result<PolyMarket>;
}

pub struct DataClient {
    client: reqwest::Client,
    base_url: String,  // https://data-api.polymarket.com
}

impl DataClient {
    pub async fn get_positions(&self, wallet: &str) -> Result<Vec<Position>>;
    pub async fn get_trades(&self, wallet: &str, limit: usize) -> Result<Vec<Trade>>;
    pub async fn get_activity(&self, wallet: &str) -> Result<Vec<Activity>>;
}
```

### Response Types

```rust
pub struct PolyEvent {
    pub id: String,
    pub title: String,
    pub slug: String,
    pub description: String,
    pub active: bool,
    pub volume: f64,
    pub volume_24hr: f64,
    pub liquidity: f64,
    pub markets: Vec<PolyMarket>,
    pub tags: Vec<PolyTag>,
    pub end_date: Option<String>,
}

pub struct PolyMarket {
    pub id: String,
    pub question: String,
    pub slug: String,
    pub outcomes: Vec<String>,       // ["Yes", "No"]
    pub outcome_prices: Vec<f64>,    // [0.989, 0.011]
    pub volume: f64,
    pub liquidity: f64,
    pub last_trade_price: f64,
    pub best_bid: f64,
    pub best_ask: f64,
    pub price_change_1h: Option<f64>,
    pub price_change_1d: Option<f64>,
    pub price_change_1w: Option<f64>,
    pub price_change_1m: Option<f64>,
    pub end_date: Option<String>,
    pub closed: bool,
    pub condition_id: String,
}

pub struct PolyTag {
    pub label: String,
    pub slug: String,
}

pub struct Position {
    pub market_id: String,
    pub outcome: String,
    pub size: f64,
    pub avg_price: f64,
    pub current_price: f64,
    pub pnl: f64,
}
```

---

## Market-to-Forecast Matching

In the primary workflow, matching is trivial — the user SELECTS the Polymarket market from a search UI. No fuzzy matching needed.

For the secondary workflow (user types a question in Fermi, system suggests a PM market), matching is harder:

### Tier 1: User Selects from Search (primary — build now)

User browses/searches Polymarket from Portfolio panel. They pick the market. No ambiguity.

### Tier 2: Keyword Search Suggestion (secondary — build now)

When a Fermi forecast exists without a PM link, show "Search Polymarket for matching markets" in the outside view section. Server-side keyword extraction + Gamma API search. User confirms or dismisses.

### Tier 3: Embedding Similarity (future)

ABW already has `voyage-2` embeddings. Embed Fermi question + PM event titles, rank by cosine similarity. Auto-suggest best match.

### Honest Null

Many Fermi forecasts won't have a Polymarket match — "Will ASTS hit $200M revenue?" is a specific company question unlikely to be on PM. That's fine. The PM integration is highest-value for politics, macro, geopolitics, and sports — the categories where PM has deep liquidity. Don't over-invest in matching; let users manually link when it matters.

---

## Edge Analysis

**Polymarket price IS the crowd's probability. Fermi decomposition IS your probability. The difference is your edge.**

```
Edge = Fermi probability - Market price

If Edge > +5pp:  You're more bullish than the crowd — possible alpha
If Edge < -5pp:  You're more bearish than the crowd — possible alpha
If |Edge| < 5pp: No meaningful divergence — consensus view
```

Edge is computed every time a linked forecast is opened (fresh PM snapshot) and stored in `fermi_market_observations.divergence_pp`. Over many resolved forecasts, the system learns:

- What edge levels actually predict correct outcomes?
- Per-domain: "In macro forecasts, your edge signal is predictive when |divergence| > 15pp"
- Per-user: "Your forecasts with >20pp divergence have a 40% hit rate — you're overconfident at high divergence"

This is the calibration flywheel. Polymarket provides the resolution, Fermi provides the structured analysis, and the gap between them is the learning signal.

---

## Resolution Bridge

When a Polymarket market resolves, we can auto-resolve the linked Fermi forecast:

1. Periodic check: `GET gamma-api.polymarket.com/markets?id={market_id}`
2. If `closed: true` and `outcomePrices` shows `[1.0, 0.0]` or `[0.0, 1.0]` → resolved
3. Call `resolve_forecast(forecast_id, actual_outcome)` stored procedure
4. Computes Brier score automatically
5. Records in `fermi_forecast_updates` with `reason: "Auto-resolved via Polymarket"`

This is the best possible calibration data — we know both the user's prediction AND the actual outcome, resolved by an independent oracle (Polymarket's UMA oracle system).

---

## Open Questions

1. **Rate limiting**: Gamma API is public but rate-limited (~100 req/min). Cache event search results server-side (in-memory HashMap with 5-minute TTL) to avoid hammering on repeated searches. Price snapshots are per-forecast and infrequent enough to not hit limits.

2. **Multi-market events**: Fed decision has 4 markets. Surface all markets within an event, let user pick. For auto-aggregation (e.g., "Will the Fed cut?" = sum of cut markets), do server-side.

3. **Proxy wallet problem (Mode 2)**: Polymarket users often have a proxy Safe wallet distinct from their EOA. The Data API needs the proxy address. Solution: let users paste their Polymarket profile URL, or resolve EOA → proxy via CLOB API.

4. **rs-clob-client compatibility**: Requires Rust 1.88+, we're on 1.85. Only needed for trading (future). All read-only operations use Gamma/Data APIs via plain reqwest.

5. **Stale prices**: Some PM markets have low activity and stale last-trade prices. Always check `volume_24h` — if zero, flag the price as potentially stale in the UI.

---

## References

- [Polymarket API Docs](https://docs.polymarket.com/api-reference/introduction)
- [rs-clob-client](https://github.com/Polymarket/rs-clob-client) — Official Rust SDK
- [Gamma API](https://gamma-api.polymarket.com) — Public market data
- [Data API](https://data-api.polymarket.com) — Public position/trade data
- [CLOB API](https://clob.polymarket.com) — Orderbook + trading (auth required)
- Migration 008: SIWE nonces
- Migration 094: Fermi forecasting tables
- `src/handlers/auth.rs`: SIWE challenge/verify handlers
- `src/gas.rs`: Credit charging infrastructure