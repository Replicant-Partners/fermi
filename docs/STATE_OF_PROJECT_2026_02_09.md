# State of Project — 2026-02-09

## What's Built (Complete)

### Core Infrastructure
- **Auth**: Google/GitHub OAuth2, self-issued JWTs, API keys with Argon2 hashing
- **Credit system**: Wallets, append-only ledger, SELECT FOR UPDATE, 100 free credits on signup
- **Gas fees**: Configurable per-action fees (message, hire, add, execute, consolidate, file write, avatar, embedding import)
- **Database**: PostgreSQL on Neon (19 migrations), pgvector for embeddings

### ADM Pipeline (Autonomous Declarative Memory)
- Execute → Episodic memory → Consolidation → Semantic rules → Ontology evolution
- Dream budget: per-agent credits for consolidation cycles
- Dream synopses: LLM-generated narratives after consolidation
- Ontology snapshots with spacetime index, diff API

### Agent System
- Agent CRUD (POST/PUT/DELETE)
- Multi-provider model catalogue (Anthropic/Mistral/OpenRouter/Qwen — DB fields stored, execution still Anthropic-only)
- Agent import from agent_card.json
- Custom embeddings import with dimension validation (5cr)
- Agent creation wizard: 5-step with import toggle, provider tabs, ontology seeds
- Curated + community agent tiers
- Filesystem agent cards upserted to DB on startup

### Embedding Projector
- PCA dimensionality reduction via linfa
- Three.js 3D visualization with temporal scrubber
- DashMap cache with 5min TTL

### Workspaces
- Teams with budget, members (Owner/Admin/Member/Viewer roles)
- 3-panel UI: members sidebar | chat center | shelf right
- @ agent invocation with workspace context injection
- Workspace git repos: auto-commit on events, file browser, diff viewer
- Coherence shelf: TEC-based evaluation, auto-eval every Nth message

### Web UI
- Templates: index (catalogue), agent_detail, workspace, dashboard, agent_create, projector, ontology
- Dual themes: Hasui (dark) + OP-1 (light)
- Colored initial dots, tag pills, display aliases

### Auth Stubs (Code Exists, Not Wired)
- SIWE (Sign In With Ethereum) in fermi-auth/src/siwe.rs

---

## What's Missing (Production Gaps)

### Tier 1: Money In (BLOCKERS)
1. **Stripe integration** — credit purchase flow, checkout, webhooks, receipts
2. **SIWE wallet connection** — UI for wallet connect, /auth/ethereum route
3. **Credit top-up UX** — "Buy More" buttons wherever balance is shown

### Tier 2: User Identity & Trust
4. **User profile page** (/profile) — display name, avatar, bio, wallet, public agents, stats
5. **KYC process** — Stripe handles fiat identity; SIWE = wallet signature; may need Stripe Connect for royalties
6. **Settings page** (/settings) — API key management UI, connected accounts, notifications, danger zone

### Tier 3: Discovery & Engagement
7. **Search & filter** — search bar, clickable tags, sort, pagination
8. **Agent detail actions** — edit/delete buttons, execution history, embedding import UI, budget top-up
9. **Notifications** — low credit warning, execution failures, workspace invites

### Tier 4: Platform Safety
10. **Rate limiting** — per-user, per-endpoint (tower-governor or similar)
11. **Admin dashboard** — flag/remove agents, user management, credit ledger view
12. **Error pages** — custom 404, 500

### Tier 5: Polish
13. **Docs at /docs** — serve existing markdown
14. **Mobile nav** — hamburger menu, consistent breakpoints
15. **Analytics** — PostHog or Plausible

---

## Payment Architecture

| Path | How they pay | Credits | KYC |
|------|-------------|---------|-----|
| Fiat (Stripe) | Card / Apple Pay | Stripe Checkout → webhook → credit_grant() | Stripe card verification |
| Crypto (SIWE) | ETH/USDC on-chain | Smart contract deposit → oracle → credit_grant() | Wallet signature |
| Free tier | Nothing | 100 credits on signup | OAuth only |

Layer 2 (agent-to-owner royalties): agent pricing, settlement (on-chain or batched), 2.5% platform fee.

---

## Implementation Order

### Sprint A: Stripe Credit Purchase + User Profile
### Sprint B: SIWE Wallet + Settings Page
### Sprint C: Search/Filter + Agent Detail Actions
### Sprint D: Rate Limiting + Notifications
### Sprint E: Admin Dashboard + Error Pages
### Sprint F: Code Audit + Testing + Documentation Organization

---

## Key Files

| Area | Files |
|------|-------|
| Auth | fermi-auth/src/{oidc,jwt,api_keys,middleware,siwe}.rs |
| Credits | fermi-auth/src/lib.rs (credit_charge, credit_grant, get_or_create_wallet) |
| Gas | src/gas.rs |
| API Server | src/api_server.rs (~4500 lines) |
| Agent types | agent-bestiary/memory/src/types.rs |
| Agent store | agent-bestiary/memory/src/store.rs |
| Templates | templates/{index,agent_detail,workspace,dashboard,agent_create}.html |
| Migrations | migrations/001-019 |
| SIWE stub | fermi-auth/src/siwe.rs |
