# Environment Variables Reference

Comprehensive reference for all environment variables used by the Agent Bestiary platform.

**Legend**
- **Required**: The server will panic or the feature will be completely unavailable without this variable.
- **Default**: Value used when the variable is not set. A dash (`-`) means the feature is simply disabled.

---

## 1. Database

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `DATABASE_URL` | Yes | _(none -- panics)_ | PostgreSQL connection string. Used by the main API server, MemoryStore, and all integration tests. Neon-compatible (PgBouncer transaction mode; prepared statement cache is disabled automatically). |

---

## 2. Auth (JWT, OAuth Providers)

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `JWT_SECRET` | Yes (prod) | `insecure-dev-secret-change-me-in-production` | HMAC-SHA256 secret for signing self-issued session JWTs. Must be a strong random string in production. |
| `GOOGLE_CLIENT_ID` | No | - | Google OAuth2 client ID. Required for Google login. |
| `GOOGLE_CLIENT_SECRET` | No | - | Google OAuth2 client secret. Required for Google login. |
| `GITHUB_CLIENT_ID` | No | - | GitHub OAuth2 client ID. Required for GitHub login. |
| `GITHUB_CLIENT_SECRET` | No | - | GitHub OAuth2 client secret. Required for GitHub login. |
| `OAUTH_REDIRECT_URI` | No | - | Shared OAuth callback URL for both Google and GitHub (e.g. `https://agent-bestiary.world/auth/callback`). Required if any OAuth provider is configured. Also used as a fallback to derive the SIWE domain. |
| `SIWE_DOMAIN` | No | Derived from `OAUTH_REDIRECT_URI`, or `agent-bestiary.world` | Domain string used in Sign-In With Ethereum (SIWE) challenge messages. |

---

## 3. Stripe / Billing

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `STRIPE_SECRET_KEY` | No | `""` (disabled) | Stripe API secret key (`sk_test_...` or `sk_live_...`). When empty, credit purchases via Stripe are disabled. |
| `STRIPE_PUBLISHABLE_KEY` | No | `""` | Stripe publishable key (`pk_test_...` or `pk_live_...`). Exposed to frontend for Checkout Sessions. |
| `STRIPE_WEBHOOK_SECRET` | No | `""` | Stripe webhook signing secret (`whsec_...`). Used to verify `POST /webhooks/stripe` payloads. |

---

## 4. LLM / Embedding Providers

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `ANTHROPIC_API_KEY` | No | - | Anthropic API key. Enables: (1) LLM agent execution via Claude models, (2) Voyage-2 embeddings, (3) ontology/prompt generation in the creation wizard. Without it the server falls back to mock executor and mock embeddings. |
| `GEMINI_API_KEY` | No | `""` (disabled) | Google Gemini API key. Used for avatar generation, `generate_image` tool, and `edit_image` tool (all via `gemini-2.5-flash-image`). When empty, image features return errors. |
| `MISTRAL_API_KEY` | No | - | Mistral AI API key. Enables Mistral models (Large, Medium, Nemo) and `mistral-embed` embeddings in the model catalogue. |
| `OPENROUTER_API_KEY` | No | - | OpenRouter API key. Enables routing to various models (Claude, Llama, Gemini, Mixtral) via OpenRouter in the model catalogue. |
| `QWEN_API_KEY` | No | - | Qwen (Alibaba Cloud) API key. Enables Qwen models (Max, Plus, Turbo) and `text-embedding-v3` embeddings in the model catalogue. |
| `QWEN_BASE_URL` | No | `https://dashscope.aliyuncs.com/compatible-mode/v1` | Override base URL for Qwen API (e.g. for private deployments). |
| `OPENAI_API_KEY` | No | - | OpenAI API key. Enables `text-embedding-3-large` embeddings in the model catalogue. |
| `GLM_API_KEY` | No | - | Zhipu AI GLM API key. Enables GLM models in the model catalogue. |
| `GLM_BASE_URL` | No | `https://open.bigmodel.cn/api/paas/v4` | Override base URL for GLM API. |
| `DEEPSEEK_API_KEY` | No | - | DeepSeek API key. Enables DeepSeek V3 (`deepseek-chat`) and DeepSeek R1 (`deepseek-reasoner`) in the model catalogue. Both are OpenAI-compatible. |
| `DEEPSEEK_BASE_URL` | No | `https://api.deepseek.com/v1` | Override base URL for DeepSeek API (e.g. for Azure-hosted DeepSeek). |
| `KIMI_API_KEY` | No | - | Moonshot AI API key. Enables Kimi models (128k, 32k, 8k context variants) in the model catalogue. OpenAI-compatible. |
| `KIMI_BASE_URL` | No | `https://api.moonshot.cn/v1` | Override base URL for Kimi API. |
| `REDUCT_API_KEY` | No | - | Reduct.video API key (v3). Enables video transcript analysis and highlight reel creation tools (`reduct_*`). Auth via `X-Auth-Key` header. |

---

## 5. Gas Fees (Credit Costs per Action)

All gas fee variables are optional integers (parsed via `env_or()`). They control how many credits each platform action costs.

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `GAS_MESSAGE_SEND` | No | `1` | Credits charged per workspace message sent. |
| `GAS_AGENT_HIRE` | No | `5` | Credits charged to hire (fork) an agent into a workspace. |
| `GAS_AGENT_ADD` | No | `2` | Credits charged to add an existing agent to a workspace. |
| `GAS_EXECUTION_MIN` | No | `1` | Minimum credits charged per agent execution (base fee before token scaling). |
| `GAS_EXECUTION_PCT` | No | `0.10` | Gas surcharge as a fraction of the base execution fee (e.g. `0.10` = 10%). Float. |
| `GAS_CONSOLIDATION` | No | `3` | Credits charged per consolidation (dreaming) cycle. |
| `GAS_FILE_WRITE` | No | `1` | Credits charged per workspace file write operation. |
| `GAS_AVATAR_GENERATE` | No | `3` | Credits charged for avatar image generation. |
| `GAS_EMBEDDING_IMPORT` | No | `5` | Credits charged per embedding import batch. |
| `CRYPTO_TX_FEE_PCT` | No | `0.025` | Layer-2 platform transaction fee on crypto token transfers (e.g. `0.025` = 2.5%). Float. Not yet wired -- reserved for future SIWE settlement layer. |

---

## 6. Rate Limiting

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `RATE_LIMIT_PUBLIC` | No | `100` | Max requests per minute per IP address on public routes. |
| `RATE_LIMIT_AUTH` | No | `300` | Max requests per minute per authenticated user on protected routes. |
| `RATE_LIMIT_LLM` | No | `10` | Max requests per minute per user on LLM-intensive endpoints (execute, generate-ontology, generate-prompt). |
| `COHERENCE_AUTO_EVAL_INTERVAL` | No | `10` | Number of workspace messages between automatic coherence evaluations (background, best-effort). |

---

## 7. Git / Workspace

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `GIT_REPOS_PATH` | No | `./repos` | Base filesystem path where per-workspace git repositories are stored. |
| `GITHUB_ORG` | No | - | GitHub organization name for pushing workspace repos. Optional -- only needed if git push is desired. |
| `GIT_GITHUB_TOKEN` | No | - | GitHub personal access token for pushing workspace repos to the configured org. |
| `GIT_AUTO_PUSH` | No | `false` | When set to `true` or `1`, workspace git commits are automatically pushed to the remote. |

---

## 8. Server Config

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `PORT` | No | `3000` | TCP port the HTTP server listens on. Railway sets this automatically. |
| `AGENTS_DIR` | No | `agents/curated` | Filesystem directory containing curated agent card folders. Scanned on startup to seed agents into the database. |
| `RUST_LOG` | No | `info,agent_web_ui=debug` | Standard `tracing`/`env_logger` filter directive. Controls log verbosity. |

---

## Quick-Start (Minimal Development)

```bash
# The only truly required variable
export DATABASE_URL="postgresql://user:pass@host:5432/dbname?sslmode=require"

# Recommended for local dev
export JWT_SECRET="some-dev-secret"
export ANTHROPIC_API_KEY="sk-ant-..."
```

## Production Checklist

```bash
# Core
DATABASE_URL=postgresql://...
JWT_SECRET=<strong-random-64-chars>
PORT=8080  # or let Railway inject it

# Auth (at least one provider)
GOOGLE_CLIENT_ID=...
GOOGLE_CLIENT_SECRET=...
GITHUB_CLIENT_ID=...
GITHUB_CLIENT_SECRET=...
OAUTH_REDIRECT_URI=https://agent-bestiary.world/auth/callback

# LLM
ANTHROPIC_API_KEY=sk-ant-...

# Billing
STRIPE_SECRET_KEY=sk_live_...
STRIPE_PUBLISHABLE_KEY=pk_live_...
STRIPE_WEBHOOK_SECRET=whsec_...

# Optional but recommended
GEMINI_API_KEY=...  # Also powers generate_image and edit_image tools
GIT_REPOS_PATH=/data/repos
```
