# Platform Read Gas Map

> **Principle**: Agents get paid to think, not to parrot. Display is infrastructure.
> If the agent won't learn from it, don't invoke the agent — charge platform_read gas instead.

## Three-Tier Gas Model

| Tier | Who gets paid | tx_type | When |
|------|--------------|---------|------|
| **Agent execution** | Agents (they learned) | `execution_fee` + `execution_royalty` | Agent invoked, episode created |
| **Platform read** | Platform only (infrastructure) | `platform_read` | Serving agent-produced data, no agent call |
| **Free** | Nobody | — | Public browse, catalogue listing, health checks |

## Why Platform Reads Matter for Optimization

Every `platform_read` charge creates a signal:
- **Which data do users actually look at?** (demand signal for agent training priorities)
- **Which agents produce data people pay to view?** (value signal for agent marketplace)
- **When do reads spike?** (capacity planning, cache strategy)
- **Read-to-execute ratio per agent** — high ratio = agent produces durable value; low = agent output is ephemeral
- **Read-to-execute ratio per user** — high ratio = consumer; low = producer/builder

Query: `SELECT agent_id, tx_type, COUNT(*) FROM credit_ledger WHERE tx_type IN ('execution_fee', 'platform_read') GROUP BY agent_id, tx_type`

---

## Platform Read Candidates by Tier

### Tier 0: FREE (no gas)

Public discovery — charging here kills growth.

| Route | Data | Rationale |
|-------|------|-----------|
| `GET /api/agents` | Agent catalogue listing | Discovery funnel, must be free |
| `GET /api/agents/curated` | Curated agent listing | Discovery |
| `GET /api/agents/:id` | Agent detail card | Pre-hire browsing |
| `GET /api/agents/:id/avatar` | Avatar image | Static asset |
| `GET /api/creatures` | Public creature browse | Discovery |
| `GET /api/creatures/:id` | Creature detail | Public profile |
| `GET /api/creatures/:id/image` | Creature image | Static asset |
| `GET /api/swarms` | Public swarm listing | Discovery |
| `GET /api/swarms/:id` | Swarm detail | Pre-join browsing |
| `GET /api/models/catalogue` | Model provider list | Reference data |
| `GET /api/billing/tiers` | Credit purchase tiers | Pre-purchase |
| `GET /api/auth/me` | Auth check | Infrastructure |
| `GET /api/wallet` | Own wallet balance | Must be free to check your own balance |
| `GET /api/wallet/transactions` | Own transaction history | Your own records |
| `GET /api/profile` | Own profile | Infrastructure |
| `GET /api/notifications` | Own notifications | Infrastructure |
| `GET /api/beacons/nearby` | AR beacon discovery | Location service |
| `GET /api/beacons/:id` | Beacon detail | Public |
| `GET /api/teams` | Own teams list | Navigation |

### Tier 1: PLATFORM_READ (1 credit) — Standard reads

Agent-produced data that required compute to create. Already built and stored.

| Route | Data | Signal value |
|-------|------|-------------|
| `GET /api/rabble/:id/flock-history` | Normalized creature positions | **DONE** - first implementation |
| `GET /api/agents/:id/episodes` | Episode execution history | Which agents' work gets reviewed |
| `GET /api/episodes/:id` | Episode detail (timing, tools, evidence) | Deep inspection demand |
| `GET /api/agents/:id/metrics` | Agent performance metrics (30-day) | Agent quality interest |
| `GET /api/metrics/platform` | Platform-wide aggregates | System health interest |
| `GET /api/agents/:id/kg` | Knowledge graph overview | KG value signal |
| `GET /api/agents/:id/kg/entities` | Entity listing | KG browsing depth |
| `GET /api/agents/:id/kg/entities/:eid` | Entity detail + relationships | Deep KG exploration |
| `GET /api/agents/:id/kg/entities/:eid/facts` | Entity facts | Fact-level interest |
| `GET /api/agents/:id/kg/facts` | All facts listing | KG browsing |
| `GET /api/agents/:id/kg/rules` | Semantic rules | Rule interest |
| `GET /api/agents/:id/kg/rules/:rid` | Rule detail | Deep rule exploration |
| `GET /api/agents/:id/kg/communities` | Community clusters | Cluster interest |
| `GET /api/agents/:id/ontology` | Latest ontology snapshot | Ontology demand |
| `GET /api/agents/:id/ontology/history` | Ontology version history | Evolution tracking |
| `GET /api/agents/:id/ontology/snapshots/:sid` | Specific snapshot | Historical interest |
| `GET /api/agents/:id/ontology/diff` | Ontology diff between versions | Change tracking |
| `GET /api/agents/:id/versions` | Agent version history | Agent evolution interest |
| `GET /api/agents/:id/versions/:num` | Specific agent version | Rollback consideration |
| `GET /api/agents/:id/eval/test-cases` | Eval test cases | QA interest |
| `GET /api/agents/:id/eval/runs` | Eval run results | QA demand |
| `GET /api/agents/:id/dependencies` | Agent dependency graph | Architecture interest |
| `GET /api/workspaces/:id/messages` | Workspace message history | Workspace replay demand |
| `GET /api/workspaces/:id/coherence` | Latest coherence eval | Coherence interest |
| `GET /api/workspaces/:id/coherence/history` | Coherence history | Coherence tracking |
| `GET /api/workspaces/:id/ontology` | Merged workspace ontology | Cross-agent knowledge |
| `GET /api/workspaces/:id/files` | Workspace file listing | Content interest |
| `GET /api/workspaces/:id/files/*path` | Workspace file content | Specific file demand |
| `GET /api/workspaces/:id/files-raw/*path` | Binary file content | Asset demand |
| `GET /api/workspaces/:id/git/log` | Git history | Change tracking |
| `GET /api/workspaces/:id/git/diff` | Git diff | Change inspection |
| `GET /api/rabble/:id/messages` | Rabble chat history | Chat replay demand |
| `GET /api/rabble/:id/members` | Rabble members | Member interest |
| `GET /api/marketplace/listings` | Marketplace browse | Marketplace demand |
| `GET /api/marketplace/history` | Purchase history | Transaction review |
| `GET /api/shopping/profile` | Shopping profile | Profile interest |
| `GET /api/me/workspace` | Personal menagerie | Menagerie demand |
| `GET /api/creatures/:id/flights` | Flight history | Movement tracking demand |
| `GET /api/creatures/:id/devices` | Paired devices | Device management |
| `GET /api/swarm/sessions` | Telemetry sessions | Telemetry demand |
| `GET /api/swarm/sessions/:id` | Session detail | Deep telemetry |

### Tier 2: PLATFORM_READ_HEAVY (2 credits) — Compute-intensive reads

Reads that involve non-trivial server-side computation (embedding projection, PCA, tSNE).

| Route | Data | Why heavier |
|-------|------|------------|
| `GET /api/agents/:id/projections` | PCA/tSNE embedding projection | Dimensionality reduction compute (cached 5min) |
| `GET /api/projections/bestiary` | Bestiary-wide projections | All-agent embedding aggregation |
| `GET /api/agents/:id/projections/temporal` | Temporal keyframed evolution | Multiple projection computations |
| `GET /api/agents/:id/dreaming/budget` | Dreaming budget state | Budget calculation |

---

## Implementation Notes

### Adding platform_read to a handler

```rust
// 1. Add AuthPrincipal to handler params
// 2. Get user wallet
// 3. Charge platform_read gas
// 4. Proceed with the read

pub async fn some_read_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    // ... other params
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let user_wallet = get_or_create_wallet(pool, "user", &user_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("Wallet error: {}", e)))?;
    charge_gas(
        pool,
        user_wallet.wallet_id,
        state.gas_fees.platform_read,
        "platform_read",
        "Description of what data is being served",
        Some(&related_id),
    )
    .await?;

    // ... existing handler logic
}
```

### Migration: already in 057

`platform_read` is already in the `credit_ledger_tx_type_check` constraint (migration 057).

### Rollout strategy

Don't do all at once. Phase it:

1. **Now**: `flock-history` (done)
2. **Next**: KG endpoints (8 routes) — highest signal value, agent knowledge is the product
3. **Then**: Ontology + projections (7 routes) — expensive reads
4. **Then**: Workspace reads (messages, files, coherence) — workspace engagement signal
5. **Last**: Episode/metrics reads — analytics demand signal

### Analytics queries

```sql
-- Most-read agents (which agent-produced data do users consume?)
SELECT a.agent_name, COUNT(*) as reads
FROM credit_ledger cl
JOIN agents a ON cl.related_id = a.agent_id::text
WHERE cl.tx_type = 'platform_read'
GROUP BY a.agent_name
ORDER BY reads DESC;

-- Read-to-execute ratio per agent
SELECT agent_name,
  SUM(CASE WHEN tx_type = 'platform_read' THEN 1 ELSE 0 END) as reads,
  SUM(CASE WHEN tx_type = 'execution_fee' THEN 1 ELSE 0 END) as executions,
  ROUND(SUM(CASE WHEN tx_type = 'platform_read' THEN 1 ELSE 0 END)::numeric /
        NULLIF(SUM(CASE WHEN tx_type = 'execution_fee' THEN 1 ELSE 0 END), 0), 2) as read_ratio
FROM credit_ledger cl
JOIN agents a ON cl.related_id = a.agent_id::text
WHERE cl.tx_type IN ('platform_read', 'execution_fee')
GROUP BY agent_name
ORDER BY read_ratio DESC;

-- Hourly read volume (capacity planning)
SELECT date_trunc('hour', created_at) as hour, COUNT(*) as reads
FROM credit_ledger
WHERE tx_type = 'platform_read'
GROUP BY hour
ORDER BY hour DESC
LIMIT 168; -- 1 week

-- User read patterns (consumer vs builder)
SELECT u.display_name,
  SUM(CASE WHEN cl.tx_type = 'platform_read' THEN 1 ELSE 0 END) as reads,
  SUM(CASE WHEN cl.tx_type = 'execution_fee' THEN 1 ELSE 0 END) as executions
FROM credit_ledger cl
JOIN wallets w ON w.wallet_id = cl.wallet_id
JOIN users u ON u.user_id = w.owner_id
WHERE cl.tx_type IN ('platform_read', 'execution_fee')
GROUP BY u.display_name
ORDER BY reads DESC;
```

---

## Not Platform Reads (Different Economics)

| Category | Why different |
|----------|--------------|
| Admin endpoints | Admin has separate economics, no gas |
| Auth/profile/wallet | Infrastructure, must be free |
| SSE streams (`/stream`) | Persistent connection — charge per-message or session, not per-read |
| Rabble chat POST | Already charges `rabble_chat` gas + potential agent execution |
| Agent execution POST | Already charges `execution_fee` + `execution_royalty` |
| Marketplace match POST | Already charges `marketplace_match_purchase` |
