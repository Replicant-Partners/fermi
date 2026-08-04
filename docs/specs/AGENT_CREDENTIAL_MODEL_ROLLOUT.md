# Rollout Guide — Agent Credential Model (P0–P3)

Companion to `docs/specs/AGENT_CREDENTIAL_MODEL.md`. This is the operational
runbook for shipping the credential re-model + the `ontologist` extraction
service. Branch: `feat/agent-credential-model`.

Commits (mine, on the branch):
- `9dbe146d` — spec (source of truth)
- `47ac0455` — **P0**: `agent_credentials` store + `abw-system` principal + env→store bootstrap
- `954310e7` — **P1–P3**: `resolve_credential` + `ProviderType::OpenAI` + `ontologist` + consolidation routing + xaman_ek registration

> A stray `508a3ba4 v0.10.29` (unrelated console release) is interleaved on
> the branch from the concurrent session. It reconciles at merge; it does not
> touch anything in this change.

---

## 1. What ships

| Piece | Effect |
|---|---|
| mig-171 | Creates `agent_credentials (principal_id, provider, scope)` + seeds the non-login `abw-system` principal. Idempotent, PgBouncer-safe. |
| Boot bootstrap | Encrypts `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` env into `(abw-system, <provider>, '*')` **iff absent**. Env becomes a one-time seed; the store is authoritative. |
| `ontologist` agent | System-tier extraction service (`openai`/`gpt-4o-mini`), seeded from `agents/curated/ontologist/`. |
| `resolve_credential` | Single credential path: system-tier → `abw-system`; owner-owned → `owner_id`; agent-scope beats `*`; legacy `user_secrets` fallback. |
| Consolidation routing | Dream-cycle extraction LLM is built from the `ontologist` card + store key (no hardcoded Anthropic/env path). |

---

## 2. Prerequisites (Railway env)

| Var | Required? | Purpose |
|---|---|---|
| `SECRETS_ENCRYPTION_KEY` | **yes** (already set) | 32-byte hex AES key. Without it the encryptor is `None`, the bootstrap is skipped, and `resolve_credential` returns `None` → extraction falls back to pattern-based (no crash, but no LLM extraction). |
| `OPENAI_API_KEY` | **yes** (already set) | Funds the `ontologist` (and the v0.10.26 embedder). Bootstrapped into `abw-system`'s store on first boot. |
| `ANTHROPIC_API_KEY` | optional | Bootstrapped too; not required for the milestone (dead account is fine — dreaming no longer depends on it). |

No new env vars are introduced. The whole point is that agent keys move **out**
of env and into the store.

---

## 3. Deploy sequence (all automatic on boot)

Push `main` → Railway builds `--bin api-server` → on startup, in order:

1. `run_migrations()` runs mig-171 → look for:
   `Running migration: migrations/171_agent_credentials.sql` … `completed`
2. `AppState` builds the encryptor → `Secrets encryption configured`
3. Bootstrap seeds the store → look for:
   `Bootstrapped abw-system 'openai' credential from OPENAI_API_KEY env var`
   (absent on subsequent boots — the store already holds it; that's correct)
4. `seed_agents_to_database()` seeds `ontologist` → `Seeded agent ontologist → <uuid>`
   and `Using OpenAI embeddings (text-embedding-3-large @ 1024)` (embedder)
5. xaman_ek drift check passes silently (no `ONTOLOGY DRIFT` warning).

Nothing manual is required. If `SECRETS_ENCRYPTION_KEY` is somehow unset,
step 3 is skipped and you'll see the mock-embeddings warning — stop and fix env.

---

## 4. Post-deploy verification

**Credential store seeded:**
```sql
SELECT principal_id, provider, scope, length(encrypted_value) AS bytes
FROM agent_credentials WHERE principal_id = 'abw-system';
-- → at least (abw-system, openai, *)
```

**ontologist seeded correctly:**
```sql
SELECT agent_name, tier, llm_provider, model
FROM agents WHERE agent_name = 'ontologist';
-- → ontologist | system | openai | gpt-4o-mini
```

**Close a dream cycle (the real test):**
```bash
curl -s -X POST -H "Authorization: Bearer $IVAN_TOKEN" \
  https://agent-bestiary.world/api/agents/macro_data_agent/consolidate
# → { status: "accepted", job_id, ... }
```
Then confirm (poll endpoint is unreliable pending the job-id fix — check state):
```sql
SELECT last_consolidated_at,
       (SELECT COUNT(*) FROM episodes e
         WHERE e.agent_id=a.agent_id AND e.consolidated=false) AS unconsolidated
FROM agents a WHERE agent_name='macro_data_agent';
-- → last_consolidated_at recent, unconsolidated dropped

SELECT rules_extracted, entities_created, status
FROM consolidation_jobs
WHERE agent_id=(SELECT agent_id FROM agents WHERE agent_name='macro_data_agent')
ORDER BY started_at DESC LIMIT 1;
-- → rules/entities > 0 (extraction ran on OpenAI via the ontologist)
```
Server log during the run should show **no** `credit balance too low`
(Anthropic) and **no** char-boundary panic — extraction now runs on OpenAI.

---

## 5. The dreaming affordance = a compound flow

The UX **"dreaming"** affordance (→ `POST /api/agents/:id/consolidate`) is now a
declarative **compound agent**: **`dream_coordinator`**
(`agents/curated/dream_coordinator/agent_card.json`, tier=system), which
declares `ontologist` + `dream_narrator` as its `dependencies.required` and
orchestrates:

```
dream_coordinator (compound, system)
  ├─ embed unconsolidated episodes        (OpenAI embeddings, platform key)
  ├─ cluster (DBSCAN)
  ├─ EXTRACT  → ontologist                (system agent, OpenAI, abw-system key)
  │              entities + semantic rules → agent's knowledge graph
  └─ NARRATE  → dream_narrator             (system agent) → dream_synopsis
```

Members are resolved **from the card, not hardcoded**: consolidation picks the
coordinator's member that produces `semantic-rules` (extract) and the one that
produces `dream-synopsis` (narrate). Swap the members in the card and the
pipeline follows (`dream_member()` in `handlers/consolidation.rs`, with safe
fallbacks to `ontologist` / `dream_narrator`).

- **`ontologist`** is the extraction brain (this change). **`dream_narrator`**
  is the narration voice (already wired). Both are platform-service agents.
- Because extraction routes through the credential model, dreaming is
  observable, evaluable, provider-portable, and platform-funded — not a
  hardcoded in-process LLM call.
- The **"eval"** affordance is the sibling loop: it runs the EvaluatorRegistry
  and writes `eval_signals`. Any eval agent that calls an LLM resolves its key
  through the same `resolve_credential` path once P5 lands, so eval and
  dreaming share one funding/credential substrate.

> Done: `dream_coordinator` is the first-class compound orchestrator card,
> registered in `xaman_ek` (roster + Compound Agent Dependency Graph:
> `dream_coordinator → [ontologist, dream_narrator]`) so it's visible to the
> navigator and the observability UI. Its `workflow_template` documents the
> 4-stage pipeline (embed → cluster → extract → narrate).

---

## 6. Rollback

- mig-171 is **additive** (new table + one non-login row). Safe to leave in
  place even if the app rolls back; nothing else depends on it existing.
- To fully revert behaviour: redeploy the pre-branch binary. The dead code
  path (hardcoded Anthropic-Haiku env) returns. `agent_credentials` rows are
  inert. No destructive change to existing tables.

---

## 7. Fast-follows (NOT in this rollout)

- **P4** — ladder graceful degradation: on OpenAI failure (401/quota/5xx),
  step down the ontologist's `model_ladder` (`gpt-4o` → `openrouter/free`),
  emit a `provider_degradation` signal.
- **P5** — migrate `dream_narrator` / `cohere_and_coordinate` / `fermi` /
  `xaman_ek` ownership to `abw-system`; retire the executor's Anthropic-env
  fallback; move the **embedder** onto the credential model (it still reads
  env `OPENAI_API_KEY` directly).
- **Job-id unification** + observatory `n=0` honesty (carried from earlier):
  makes the `/consolidation/jobs/:id` poll endpoint truthful.
- **Per-agent key funding UI** — the `scope=agent_name` write path + admin UI
  so owners can fund a specific agent on its own key (the primitive is already
  in the schema).
