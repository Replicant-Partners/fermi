# Data Dictionary — Agent Bestiary Workspace (ABW)

**Generated from:** migrations 004–121 (excluding `rollback_auth.sql`)
**Total tables:** 96 documented
**Companion doc:** [docs/ER_DIAGRAM.md](./ER_DIAGRAM.md) — visual relationship diagrams.

## How to use this doc

- Tables are grouped into 11 domains (same grouping as the ER diagram).
- For each table: purpose, source migration(s), and the full column list.
- Each column row carries: name · type · nullable · default · constraints · description.
- Foreign keys are noted in the *Constraints* column as `FK → <table>.<col>`.
- Inline `CHECK(...)` constraints are included verbatim where short; long ones are truncated with `…`.
- See the companion ER diagram for visual relationships.

## Conventions

- *Null* column: `NOT NULL` or `NULL` — derived from the column DDL.
- *Default* column: `—` when no default is specified.
- Constraint abbreviations: `PK` primary key · `UK` unique key · `FK` foreign key.
- Types use canonical Postgres names: `TEXT`, `UUID`, `JSONB`, `TIMESTAMPTZ`, `INTEGER`, `BIGINT`, `BOOLEAN`, `DOUBLE PRECISION`, `NUMERIC`, `VECTOR(...)`, `GEOGRAPHY(...)`.
- Column order in each table: PK columns first, then FK/identity columns, then content, then timestamps last.
- *Description* prefers `COMMENT ON COLUMN` text from the migration; otherwise inferred from column name + table context.

## Domain 1: Users & Auth

_Identity, authentication, secrets, push delivery and waitlist._

### `users`

**Purpose:** User authentication identities - cached from Zitadel and Web3

**Created in:** `004_add_users_table.sql`
**Modified in:** `004b_migrate_users_for_auth.sql`, `018_agent_aliases.sql`, `020_stripe_and_profile.sql`, `029_fix_message_type_and_profile.sql`, `057_rabble_workspaces.sql`, `090_social_layer.sql`, `092_fix_social_layer.sql`, `093_users_user_id_unique.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `user_id` | TEXT | NOT NULL | — | PK | Zitadel user ID or legacy UUID as text |
| `zitadel_org_id` | TEXT | NULL | — | — |  |
| `github_id` | TEXT | NULL | — | — |  |
| `google_id` | TEXT | NULL | — | — |  |
| `stripe_customer_id` | TEXT | NULL | — | — |  |
| `personal_workspace_id` | UUID | NULL | — | — |  |
| `email` | TEXT | NOT NULL | — | UK |  |
| `display_name` | TEXT | NULL | — | — | Display name shown in UI. |
| `avatar_url` | TEXT | NULL | — | — |  |
| `role` | TEXT | NOT NULL | 'developer' | CHECK(role IN ('admin', 'developer', 'viewer')) |  |
| `auth_provider` | TEXT | NULL | — | CHECK(auth_provider IN ('email', 'github', 'google', 'ethereum', 'legacy')) | Authentication provider: email, github, google, ethereum, or legacy |
| `github_username` | TEXT | NULL | — | — |  |
| `ethereum_address` | TEXT | NULL | — | — | Checksummed Ethereum address for SIWE |
| `ens_name` | TEXT | NULL | — | — | ENS domain name if resolved (e.g., vitalik.eth) |
| `bio` | TEXT | NULL | — | — |  |
| `social_visibility` | TEXT | NOT NULL | 'public' | CHECK(social_visibility IN ('public', 'creature-only', 'private')) |  |
| `last_login_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `api_keys`

**Purpose:** API keys for programmatic access to Fermi services

**Created in:** `005_add_api_keys.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `key_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `user_id` | UUID | NOT NULL | — | FK → users.id | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `key_hash` | TEXT | NOT NULL | — | — | Argon2 hash - never store keys in plaintext |
| `key_prefix` | TEXT | NOT NULL | — | UK | First 12 characters of key for identification |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `scopes` | TEXT[] | NOT NULL | ARRAY['read'] | — | Permissions array: read, write, execute, admin |
| `request_count` | BIGINT | NOT NULL | 0 | — |  |
| `is_active` | BOOLEAN | NOT NULL | TRUE | — | Active/enabled flag. |
| `last_used_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `expires_at` | TIMESTAMPTZ | NULL | — | — | Expiry timestamp; row becomes invalid after this. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `siwe_nonces`

**Purpose:** Nonces for SIWE replay protection - expired nonces cleaned up automatically

**Created in:** `008_add_siwe_nonces.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `nonce` | TEXT | NOT NULL | — | PK |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `expires_at` | TIMESTAMPTZ | NOT NULL | — | — | Nonce validity period (typically 5 minutes from creation) |

### `user_secrets`

**Purpose:** Encrypted per-user secrets (e.g. provider API keys).

**Created in:** `039_user_secrets.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `secret_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `user_id` | TEXT | NOT NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `secret_name` | TEXT | NOT NULL | — | — |  |
| `encrypted_value` | BYTEA | NOT NULL | — | — |  |
| `nonce` | BYTEA | NOT NULL | — | — |  |
| `scope` | TEXT | NOT NULL | '*' | — |  |
| `label` | TEXT | NULL | — | — |  |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

**Table-level constraints:**
- `UNIQUE(user_id, secret_name)`

### `secret_access_log`

**Purpose:** Append-only audit log of user-secret reads/writes.

**Created in:** `039_user_secrets.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `log_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `user_id` | TEXT | NOT NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `workspace_id` | UUID | NULL | — | — |  |
| `secret_name` | TEXT | NOT NULL | — | — |  |
| `agent_name` | TEXT | NOT NULL | — | — |  |
| `action` | TEXT | NOT NULL | — | CHECK(action IN ('read', 'used', 'created', 'updated', 'deleted')) |  |
| `tool_name` | TEXT | NULL | — | — |  |
| `ip_address` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `push_subscriptions`

**Purpose:** Web-push subscriptions per user/device.

**Created in:** `098_push_subscriptions.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `user_id` | TEXT | NOT NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `endpoint` | TEXT | NOT NULL | — | — |  |
| `p256dh_key` | TEXT | NOT NULL | — | — |  |
| `auth_key` | TEXT | NOT NULL | — | — |  |
| `user_agent` | TEXT | NULL | — | — |  |
| `failed_count` | INTEGER | NOT NULL | 0 | — |  |
| `active` | BOOLEAN | NOT NULL | true | — | Boolean flag. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `last_used_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

**Table-level constraints:**
- `UNIQUE(user_id, endpoint)`

### `push_config`

**Purpose:** VAPID / push-service global configuration.

**Created in:** `098_push_subscriptions.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | INTEGER | NOT NULL | 1 | PK; CHECK(id = 1) | Surrogate primary key. |
| `vapid_public_key` | TEXT | NOT NULL | — | — |  |
| `vapid_private_key` | TEXT | NOT NULL | — | — |  |
| `vapid_subject` | TEXT | NOT NULL | 'mailto:hello@rabble.world' | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `waitlist`

**Purpose:** Pre-launch waitlist signups.

**Created in:** `023_waitlist.sql`
**Modified in:** `031_waitlist_status.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `email` | TEXT | NOT NULL | — | — |  |
| `source` | TEXT | NULL | 'landing' | — |  |
| `status` | TEXT | NULL | 'pending' | — | Lifecycle status (see CHECK constraint for allowed values). |
| `notes` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NULL | NOW() | — | Row creation timestamp. |
| `invited_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

**Table-level constraints:**
- `UNIQUE(email)`

## Domain 2: Agents & Memory

_Agent registry, versioning, episodes, semantic rules, knowledge graph (entities/facts/communities), consolidation jobs, HITL, observability state, alignments and inter-agent coherence._

### `agents`

**Purpose:** Agent registry — the central catalogue of every executable agent.

**Created in:** `010_add_adm_tables_and_dreaming.sql`
**Modified in:** `006_add_user_id_to_agents.sql`, `010_add_adm_tables_and_dreaming.sql`, `011_agent_crud_and_education.sql`, `018_agent_aliases.sql`, `019_agent_provider_fields.sql`, `022_sample_queries.sql`, `025_agent_lifecycle.sql`, `037_agent_valence_and_workflow_template.sql`, `038_prompt_template.sql`, `040_agent_requires_secrets.sql`, `059_agent_wallet_admin.sql`, `101_model_ladder.sql`, `103_observability_foundations.sql`, `105_cep_fermi_contract.sql`, `106_model_params.sql`, `114_agent_valence_column.sql`, `117_agent_output_contract.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `agent_id` | UUID | NOT NULL | gen_random_uuid() | PK | Owning/related agent (→ agents.agent_id). |
| `user_id` | TEXT | NULL | — | — | Owner of this agent - references users.user_id |
| `current_ontology_snapshot_id` | UUID | NULL | — | — |  |
| `forked_from` | UUID | NULL | — | FK → agents.agent_id |  |
| `is_public` | BOOLEAN | NULL | FALSE | — | Quick check for public visibility |
| `visibility` | TEXT | NULL | 'private' | CHECK(visibility IN ('private', 'unlisted', 'public')) | private: owner only, unlisted: link only, public: catalog listed |
| `agent_name` | TEXT | NOT NULL | — | UK |  |
| `agent_type` | TEXT | NOT NULL | — | — |  |
| `version` | TEXT | NOT NULL | '1.0.0' | — | Monotonic version counter for this row. |
| `tier` | TEXT | NOT NULL | 'curated' | — |  |
| `executor_type` | TEXT | NOT NULL | — | — |  |
| `model` | TEXT | NOT NULL | — | — |  |
| `temperature` | FLOAT | NOT NULL | 0.3 | — |  |
| `mcp_servers` | JSONB | NULL | — | — | JSONB blob. |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `author` | TEXT | NOT NULL | 'Fermi Team' | — |  |
| `current_ontology_commit` | TEXT | NULL | — | — |  |
| `total_executions` | INTEGER | NOT NULL | 0 | — |  |
| `successful_executions` | INTEGER | NOT NULL | 0 | — |  |
| `failed_executions` | INTEGER | NOT NULL | 0 | — |  |
| `total_cost_usd` | DECIMAL(10, 6) | NOT NULL | 0.0 | — |  |
| `avg_execution_time_ms` | BIGINT | NOT NULL | 0 | — |  |
| `dreaming_budget_credits` | INTEGER | NOT NULL | 0 | — |  |
| `dreaming_credits_used` | INTEGER | NOT NULL | 0 | — |  |
| `system_prompt` | TEXT | NULL | — | — |  |
| `tags` | TEXT[] | NULL | '{}' | — | Free-form string tags array. |
| `education_budget_credits` | INTEGER | NOT NULL | 0 | — |  |
| `education_credits_used` | INTEGER | NOT NULL | 0 | — |  |
| `display_alias` | TEXT | NULL | — | — |  |
| `llm_provider` | TEXT | NOT NULL | 'anthropic' | — |  |
| `embedding_provider` | TEXT | NOT NULL | 'anthropic' | — |  |
| `embedding_model` | TEXT | NOT NULL | 'voyage-2' | — |  |
| `embedding_dimension` | INTEGER | NOT NULL | 1024 | — |  |
| `sample_queries` | TEXT[] | NULL | '{}' | — |  |
| `status` | TEXT | NOT NULL | 'draft' | CHECK(status IN ('draft', 'published', 'archived')) | Lifecycle status (see CHECK constraint for allowed values). |
| `fork_pricing` | JSONB | NULL | '{"base_price": 0}' | — | JSONB blob. |
| `fork_count` | INTEGER | NOT NULL | 0 | — |  |
| `accepts` | TEXT[] | NULL | '{}' | — |  |
| `produces` | TEXT[] | NULL | '{}' | — |  |
| `workflow_template` | JSONB | NULL | — | — | JSONB blob. |
| `prompt_template` | TEXT | NULL | — | — |  |
| `requires_secrets` | JSONB | NULL | '[]' | — | JSONB blob. |
| `auto_collect_pct` | INTEGER | NOT NULL | 0 | — |  |
| `model_ladder` | JSONB | NOT NULL | '[]' | — | JSONB blob. |
| `min_tier` | TEXT | NOT NULL | 'free' | CHECK(min_tier IN ('free', 'standard', 'premium')) |  |
| `capability_gates` | JSONB | NOT NULL | '{}' | — | JSONB blob. |
| `persona_version` | INTEGER | NOT NULL | 1 | — |  |
| `fermi_contract` | JSONB | NULL | — | — | JSONB blob. |
| `model_params` | JSONB | NOT NULL | '{}' | — | JSONB blob. |
| `valence` | JSONB | NULL | — | — | JSONB blob. |
| `output_contract` | JSONB | NULL | — | — | JSONB blob. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `last_consolidated_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `dreaming_budget_reset_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

### `agent_versions`

**Purpose:** Immutable snapshots of agent prompts/config/visibility.

**Created in:** `024_agent_versions.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `version_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `version_number` | INTEGER | NOT NULL | — | — |  |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `system_prompt` | TEXT | NULL | — | — |  |
| `tags` | TEXT[] | NULL | '{}' | — | Free-form string tags array. |
| `model` | TEXT | NULL | — | — |  |
| `temperature` | DOUBLE PRECISION | NULL | — | — |  |
| `visibility` | TEXT | NULL | — | — | Visibility scope (public / private / contacts etc). |
| `display_alias` | TEXT | NULL | — | — |  |
| `changed_by` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `agent_avatars`

**Purpose:** Agent avatar images and metadata.

**Created in:** `053_creature_image_storage.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `agent_id` | TEXT | NOT NULL | — | PK | Owning/related agent (→ agents.agent_id). |
| `avatar_json` | JSONB | NOT NULL | — | — | JSONB blob. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `agent_alignments`

**Purpose:** Pairwise alignment state between two agents.

**Created in:** `049_akp_foundation.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `alignment_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `source_agent_id` | UUID | NOT NULL | — | FK → agents.agent_id |  |
| `target_agent_id` | UUID | NOT NULL | — | FK → agents.agent_id |  |
| `alignment_score` | FLOAT | NOT NULL | 0.0 | — |  |
| `shared_entity_count` | INTEGER | NOT NULL | 0 | — |  |
| `divergent_entity_count` | INTEGER | NOT NULL | 0 | — |  |
| `shared_entities` | JSONB | NOT NULL | '[]' | — | JSONB blob. |
| `divergent_entities` | JSONB | NOT NULL | '[]' | — | JSONB blob. |
| `last_computed_at` | TIMESTAMPTZ | NULL | NOW() | — | Timestamp. |

**Table-level constraints:**
- `UNIQUE(source_agent_id, target_agent_id)`

### `agent_interaction_policies`

**Purpose:** Policies governing how an agent interacts with users/others.

**Created in:** `049_akp_foundation.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `policy_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `target_agent_id` | UUID | NULL | — | — |  |
| `policy_type` | TEXT | NOT NULL | — | — |  |
| `enabled` | BOOLEAN | NOT NULL | true | — | Boolean flag. |
| `created_at` | TIMESTAMPTZ | NULL | NOW() | — | Row creation timestamp. |

**Table-level constraints:**
- `UNIQUE(agent_id, policy_type, target_agent_id)`

### `agent_observability_state`

**Purpose:** Latest observability summary for an agent.

**Created in:** `105_longitudinal_observability.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `agent_id` | UUID | NOT NULL | — | PK; FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `last_scanned_entry_id` | UUID | NULL | — | — |  |
| `last_scan_duration_ms` | BIGINT | NULL | — | — |  |
| `timeline_entry_count` | INTEGER | NOT NULL | 0 | — |  |
| `anomaly_event_count` | INTEGER | NOT NULL | 0 | — |  |
| `last_scan_started_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `last_scan_completed_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `agent_timeline_entries`

**Purpose:** Append-only timeline of notable agent events.

**Created in:** `105_longitudinal_observability.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `entry_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `episode_id` | UUID | NULL | — | FK → episodes.episode_id | Owning episode (→ episodes.episode_id). |
| `run_id` | UUID | NULL | — | FK → eval_runs.run_id |  |
| `dyad_id` | TEXT | NULL | — | — |  |
| `session_id` | TEXT | NULL | — | — | Owning session. |
| `persona_version` | INTEGER | NOT NULL | 1 | — |  |
| `provenance` | TEXT | NOT NULL | 'auto_pass' | — |  |
| `dim_scores` | JSONB | NOT NULL | '{}'::jsonb | — | JSONB blob. |
| `drift_norm` | DOUBLE PRECISION | NULL | — | — |  |
| `within_version_cosine` | DOUBLE PRECISION | NULL | — | — |  |
| `anomaly_flags` | JSONB | NOT NULL | '[]'::jsonb | — | JSONB blob. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `anomaly_events`

**Purpose:** Detected anomalies in agent behaviour or signals.

**Created in:** `105_longitudinal_observability.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `event_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `episode_id` | UUID | NULL | — | FK → episodes.episode_id | Owning episode (→ episodes.episode_id). |
| `run_id` | UUID | NULL | — | FK → eval_runs.run_id |  |
| `dyad_id` | TEXT | NULL | — | — |  |
| `kind` | TEXT | NOT NULL | — | CHECK(kind IN ('drift', 'rolling_conflict', 'rupture', 'safety')) | Discriminator / subtype tag. |
| `severity` | TEXT | NOT NULL | 'warning' | CHECK(severity IN ('info', 'warning', 'critical')) |  |
| `payload` | JSONB | NOT NULL | '{}'::jsonb | — | Free-form JSONB payload. |
| `requires_review` | BOOLEAN | NOT NULL | TRUE | — | Boolean flag. |
| `resolved_by` | TEXT | NULL | — | — |  |
| `resolved_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `hitl_actions`

**Purpose:** Human-in-the-loop reviewer actions on episodes/agents.

**Created in:** `106_hitl_actions.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `action_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `anomaly_event_id` | UUID | NOT NULL | — | FK → anomaly_events.event_id |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `reviewer_id` | TEXT | NOT NULL | — | — |  |
| `correction_id` | UUID | NULL | — | FK → episode_corrections.correction_id |  |
| `action` | TEXT | NOT NULL | — | CHECK(action IN ('approve', 'relabel', 'intervene')) |  |
| `notes` | TEXT | NULL | — | — |  |
| `score_overrides` | JSONB | NOT NULL | '{}'::jsonb | — | JSONB blob. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `episodes`

**Purpose:** Episodic memory — one row per agent execution.

**Created in:** `010_add_adm_tables_and_dreaming.sql`
**Modified in:** `007_add_user_id_to_memory.sql`, `028_episode_tags.sql`, `048b_voice_assets.sql`, `103_observability_foundations.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `episode_id` | UUID | NOT NULL | gen_random_uuid() | PK | Owning episode (→ episodes.episode_id). |
| `user_id` | TEXT | NULL | — | — | Owner - for multi-tenant isolation (derived from agent owner) |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `consolidation_job_id` | UUID | NULL | — | — |  |
| `cluster_id` | UUID | NULL | — | — |  |
| `dyad_id` | TEXT | NULL | — | — |  |
| `query` | TEXT | NOT NULL | — | — |  |
| `context` | JSONB | NOT NULL | — | — | JSONB blob. |
| `execution_status` | TEXT | NOT NULL | — | — |  |
| `error_details` | TEXT | NULL | — | — |  |
| `execution_time_ms` | BIGINT | NOT NULL | — | — |  |
| `tokens_used` | INTEGER | NULL | — | — |  |
| `cost_usd` | DECIMAL(10, 6) | NULL | — | — |  |
| `embedding` | vector(1024) | NULL | — | — | Vector embedding (pgvector). |
| `consolidated` | BOOLEAN | NOT NULL | FALSE | — | Boolean flag. |
| `tags` | TEXT[] | NOT NULL | '{}' | — | Free-form string tags array. |
| `audio_url` | TEXT | NULL | — | — |  |
| `provenance` | TEXT | NOT NULL | 'auto_pass' | — |  |
| `authority_weight` | DOUBLE PRECISION | NOT NULL | 0.5 | — |  |
| `persona_version_at_write` | INTEGER | NULL | — | — |  |
| `timestamp_ref` | TIMESTAMPTZ | NOT NULL | — | — | Timestamp. |
| `timestamp_created` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `episode_corrections`

**Purpose:** Append-only HITL corrections attached to episodes.

**Created in:** `103_observability_foundations.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `correction_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `episode_id` | UUID | NOT NULL | — | FK → episodes.episode_id | Owning episode (→ episodes.episode_id). |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `reviewer_id` | TEXT | NOT NULL | — | — |  |
| `synthetic_episode_id` | UUID | NULL | — | FK → episodes.episode_id |  |
| `reviewer_action` | TEXT | NOT NULL | — | CHECK(reviewer_action IN ('approve', 'relabel', 'intervene')) |  |
| `scope` | TEXT | NOT NULL | — | CHECK(scope IN ('episode', 'dyad', 'agent_wide')) |  |
| `classification` | TEXT | NULL | — | CHECK(classification IS NULL OR classification IN ('belief', 'behaviour')) |  |
| `dimension` | TEXT | NULL | — | — |  |
| `correction_text` | TEXT | NULL | — | — |  |
| `score_overrides` | JSONB | NOT NULL | '{}'::jsonb | — | JSONB blob. |
| `coherence_check` | JSONB | NULL | — | — | JSONB blob. |
| `minimum_update_set` | JSONB | NULL | — | — | JSONB blob. |
| `tensions_flagged` | JSONB | NULL | — | — | JSONB blob. |
| `authority_weight` | DOUBLE PRECISION | NOT NULL | 1.0 | CHECK(authority_weight >= 0.0 AND authority_weight <= 1.0) |  |
| `persona_version_bump` | INTEGER | NULL | — | — |  |
| `justification` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `consolidation_jobs`

**Purpose:** Dream/consolidation runs that build semantic memory.

**Created in:** `010_add_adm_tables_and_dreaming.sql`
**Modified in:** `010_add_adm_tables_and_dreaming.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `job_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `ontology_snapshot_id` | UUID | NULL | — | FK → ontology_snapshots.snapshot_id |  |
| `duration_ms` | BIGINT | NULL | — | — |  |
| `status` | TEXT | NOT NULL | 'running' | — | Lifecycle status (see CHECK constraint for allowed values). |
| `error_message` | TEXT | NULL | — | — |  |
| `episode_range_start` | UUID | NOT NULL | — | — |  |
| `episode_range_end` | UUID | NOT NULL | — | — |  |
| `episodes_processed` | INTEGER | NOT NULL | 0 | — |  |
| `clusters_identified` | INTEGER | NOT NULL | 0 | — |  |
| `rules_extracted` | INTEGER | NOT NULL | 0 | — |  |
| `rules_verified` | INTEGER | NOT NULL | 0 | — |  |
| `rules_rejected` | INTEGER | NOT NULL | 0 | — |  |
| `entities_created` | INTEGER | NOT NULL | 0 | — |  |
| `facts_created` | INTEGER | NOT NULL | 0 | — |  |
| `dream_synopsis` | TEXT | NULL | — | — |  |
| `started_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Start timestamp. |
| `completed_at` | TIMESTAMPTZ | NULL | — | — | Completion timestamp; NULL while running. |

### `consolidation_locks`

**Purpose:** Per-agent exclusive lock to prevent concurrent consolidation.

**Created in:** `010_add_adm_tables_and_dreaming.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `agent_id` | UUID | NOT NULL | — | PK; FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `locked_by` | TEXT | NOT NULL | — | — |  |
| `locked_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |
| `expires_at` | TIMESTAMPTZ | NOT NULL | — | — | Expiry timestamp; row becomes invalid after this. |

### `entities`

**Purpose:** Knowledge graph entities (per-agent).

**Created in:** `010_add_adm_tables_and_dreaming.sql`
**Modified in:** `007_add_user_id_to_memory.sql`, `104_cep_kg_columns.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `entity_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `user_id` | TEXT | NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `replaces_entity_id` | UUID | NULL | — | FK → entities.entity_id |  |
| `entity_name` | TEXT | NOT NULL | — | — |  |
| `entity_type` | TEXT | NOT NULL | — | — |  |
| `summary` | TEXT | NULL | — | — |  |
| `source_episodes` | UUID[] | NULL | — | — |  |
| `extraction_confidence` | FLOAT | NOT NULL | — | — |  |
| `embedding` | vector(1024) | NULL | — | — | Vector embedding (pgvector). |
| `version` | INTEGER | NOT NULL | 1 | — | Monotonic version counter for this row. |
| `properties` | JSONB | NULL | — | — | JSONB blob. |
| `t_valid` | TIMESTAMPTZ | NOT NULL | — | — | Timestamp. |
| `t_invalid` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `t_created` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |
| `t_expired` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

### `facts`

**Purpose:** Knowledge graph edges between entities (per-agent).

**Created in:** `010_add_adm_tables_and_dreaming.sql`
**Modified in:** `007_add_user_id_to_memory.sql`, `104_cep_kg_columns.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `fact_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `user_id` | TEXT | NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `source_entity_id` | UUID | NOT NULL | — | FK → entities.entity_id |  |
| `target_entity_id` | UUID | NOT NULL | — | FK → entities.entity_id |  |
| `replaces_fact_id` | UUID | NULL | — | FK → facts.fact_id |  |
| `relation_type` | TEXT | NOT NULL | — | — |  |
| `relation_cardinality` | TEXT | NOT NULL | — | — |  |
| `confidence` | FLOAT | NOT NULL | — | — | Model/extraction confidence score in [0,1]. |
| `reasoning` | TEXT | NULL | — | — |  |
| `source_episodes` | UUID[] | NULL | — | — |  |
| `version` | INTEGER | NOT NULL | 1 | — | Monotonic version counter for this row. |
| `data` | JSONB | NULL | — | — | Free-form JSONB data blob. |
| `t_valid` | TIMESTAMPTZ | NOT NULL | — | — | Timestamp. |
| `t_invalid` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `t_created` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |
| `t_expired` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

### `semantic_rules`

**Purpose:** Distilled semantic rules learned during consolidation.

**Created in:** `010_add_adm_tables_and_dreaming.sql`
**Modified in:** `007_add_user_id_to_memory.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `rule_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `user_id` | TEXT | NULL | — | — | Owner - for multi-tenant isolation (derived from agent owner) |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `rule_content` | TEXT | NOT NULL | — | — |  |
| `rule_description` | TEXT | NULL | — | — |  |
| `confidence_score` | FLOAT | NOT NULL | — | — |  |
| `verification_status` | TEXT | NOT NULL | 'pending' | — |  |
| `verification_method` | TEXT | NULL | — | — |  |
| `verification_details` | JSONB | NULL | — | — | JSONB blob. |
| `source_episode_cluster` | UUID[] | NULL | — | — |  |
| `episode_count` | INTEGER | NOT NULL | — | — |  |
| `application_count` | INTEGER | NOT NULL | 0 | — |  |
| `successful_applications` | INTEGER | NOT NULL | 0 | — |  |
| `failed_applications` | INTEGER | NOT NULL | 0 | — |  |
| `embedding` | vector(1024) | NULL | — | — | Vector embedding (pgvector). |
| `is_active` | BOOLEAN | NOT NULL | TRUE | — | Active/enabled flag. |
| `invalidation_reason` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `last_validated_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `invalidated_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

### `communities`

**Purpose:** Entity clusters / communities detected per agent.

**Created in:** `010_add_adm_tables_and_dreaming.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `community_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `community_name` | TEXT | NULL | — | — |  |
| `summary` | TEXT | NULL | — | — |  |
| `member_entity_ids` | UUID[] | NULL | — | — |  |
| `member_count` | INTEGER | NOT NULL | 0 | — |  |
| `embedding` | vector(1024) | NULL | — | — | Vector embedding (pgvector). |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `last_propagation_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |

### `ontology_snapshots`

**Purpose:** Versioned ontology snapshots produced by consolidation.

**Created in:** `010_add_adm_tables_and_dreaming.sql`
**Modified in:** `010_add_adm_tables_and_dreaming.sql`, `048b_voice_assets.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `snapshot_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `consolidation_job_id` | UUID | NULL | — | — |  |
| `previous_snapshot_id` | UUID | NULL | — | FK → ontology_snapshots.snapshot_id |  |
| `git_commit_sha` | TEXT | NOT NULL | — | — |  |
| `git_repository` | TEXT | NOT NULL | — | — |  |
| `git_path` | TEXT | NOT NULL | — | — |  |
| `github_url` | TEXT | NULL | — | — |  |
| `pushed_to_remote` | BOOLEAN | NOT NULL | FALSE | — | Boolean flag. |
| `entity_count` | INTEGER | NOT NULL | — | — |  |
| `fact_count` | INTEGER | NOT NULL | — | — |  |
| `community_count` | INTEGER | NOT NULL | — | — |  |
| `rule_count` | INTEGER | NOT NULL | — | — |  |
| `mermaid_content` | TEXT | NOT NULL | — | — |  |
| `version` | INTEGER | NOT NULL | — | — | Monotonic version counter for this row. |
| `dream_synopsis` | TEXT | NULL | — | — |  |
| `consolidation_stats` | JSONB | NULL | — | — | JSONB blob. |
| `audio_url` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `dyad_state`

**Purpose:** (Agent, human) dyad longitudinal state.

**Created in:** `105_longitudinal_observability.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `dyad_id` | TEXT | NOT NULL | — | PK |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `human_id` | TEXT | NOT NULL | — | — |  |
| `rapport` | DOUBLE PRECISION | NOT NULL | 0.5 | CHECK(rapport >= 0.0 AND rapport <= 1.0) |  |
| `trust` | DOUBLE PRECISION | NOT NULL | 0.5 | CHECK(trust >= 0.0 AND trust <= 1.0) |  |
| `reciprocity` | DOUBLE PRECISION | NOT NULL | 0.5 | CHECK(reciprocity >= 0.0 AND reciprocity <= 1.0) |  |
| `episode_count` | INTEGER | NOT NULL | 0 | — |  |
| `recent_rapport` | JSONB | NOT NULL | '[]'::jsonb | — | JSONB blob. |
| `last_updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `pairwise_coherence`

**Purpose:** Pairwise coherence scores between agents.

**Created in:** `049_akp_foundation.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `coherence_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `agent_a_id` | UUID | NOT NULL | — | FK → agents.agent_id |  |
| `agent_b_id` | UUID | NOT NULL | — | FK → agents.agent_id |  |
| `workspace_id` | UUID | NULL | — | — |  |
| `global_score` | FLOAT | NOT NULL | — | — |  |
| `principle_scores` | JSONB | NOT NULL | '{}' | — | JSONB blob. |
| `episode_count` | INTEGER | NOT NULL | 1 | — |  |
| `computed_at` | TIMESTAMPTZ | NULL | NOW() | — | Timestamp. |

### `knowledge_transfers`

**Purpose:** Records of knowledge transferred between agents.

**Created in:** `049_akp_foundation.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `transfer_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `source_agent_id` | UUID | NOT NULL | — | FK → agents.agent_id |  |
| `target_agent_id` | UUID | NOT NULL | — | FK → agents.agent_id |  |
| `transfer_type` | TEXT | NOT NULL | — | — |  |
| `item_count` | INTEGER | NOT NULL | 0 | — |  |
| `accepted_count` | INTEGER | NOT NULL | 0 | — |  |
| `rejected_count` | INTEGER | NOT NULL | 0 | — |  |
| `conflict_count` | INTEGER | NOT NULL | 0 | — |  |
| `details` | JSONB | NOT NULL | '{}' | — | JSONB blob. |
| `transferred_at` | TIMESTAMPTZ | NULL | NOW() | — | Timestamp. |

### `agent_episode_payouts`

**Purpose:** Payouts distributed to an agent for an episode.

**Created in:** `057_rabble_workspaces.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `payout_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `episode_id` | UUID | NOT NULL | — | — | Owning episode (→ episodes.episode_id). |
| `agent_id` | UUID | NOT NULL | — | — | Owning/related agent (→ agents.agent_id). |
| `workspace_id` | UUID | NULL | — | — |  |
| `amount` | INTEGER | NOT NULL | — | — |  |
| `contribution_tier` | TEXT | NULL | 'equal' | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

## Domain 3: Workspaces & Compositions

_Teams, members, workspace chat, agent participants, compositions and sharing._

### `teams`

**Purpose:** Teams for collaborative sharing of objects

**Created in:** `009_add_teams_and_sharing.sql`
**Modified in:** `013_workspace_fields.sql`, `017_workspace_git.sql`, `018_agent_aliases.sql`, `036_workspace_workflow.sql`, `112_workspace_origin.sql`, `113_composition_as_first_class.sql`, `119_teams_mission_defensive.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `owner_id` | TEXT | NOT NULL | — | — | Creator/owner - references users.user_id |
| `coordination_strategist_id` | UUID | NULL | — | — | Pointer to an agent tagged 'coordination_strategy' that embodies |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `slug` | TEXT | NOT NULL | — | UK | URL-safe unique identifier for the team |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `workspace_budget` | INTEGER | NOT NULL | 0 | — |  |
| `workspace_spent` | INTEGER | NOT NULL | 0 | — |  |
| `git_repo_path` | TEXT | NULL | — | — |  |
| `git_latest_commit` | TEXT | NULL | — | — |  |
| `git_commit_count` | INTEGER | NOT NULL | 0 | — |  |
| `avatar_url` | TEXT | NULL | — | — |  |
| `workflow_mermaid` | TEXT | NULL | — | — |  |
| `workflow_meta` | JSONB | NULL | — | — | JSONB blob. |
| `origin` | TEXT | NOT NULL | 'bestiary_workspace' | — |  |
| `mission` | TEXT | NULL | — | — | Free-text declaration of what this composition accomplishes. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |
| `strategist_assigned_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

### `team_members`

**Purpose:** Team membership - users and agents can both be members

**Created in:** `009_add_teams_and_sharing.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `team_id` | UUID | NOT NULL | — | — | Owning team / workspace (→ teams.id). |
| `member_id` | TEXT | NOT NULL | — | — | users.user_id or agent_id depending on member_type |
| `member_type` | TEXT | NOT NULL | 'user' | CHECK(member_type IN ('user', 'agent')) | user or agent |
| `role` | TEXT | NOT NULL | 'member' | CHECK(role IN ('owner', 'admin', 'member', 'viewer')) | owner/admin/member/viewer within this team |
| `invited_by` | TEXT | NULL | — | — |  |
| `joined_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |

**Table-level constraints:**
- `PRIMARY KEY (team_id, member_id)`

### `workspace_messages`

**Purpose:** Chat messages inside a workspace.

**Created in:** `014_workspace_messages.sql`
**Modified in:** `029_fix_message_type_and_profile.sql`, `048b_voice_assets.sql`, `077_expand_message_type_constraint.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `message_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `workspace_id` | UUID | NOT NULL | — | FK → teams.id |  |
| `sender_id` | TEXT | NOT NULL | — | — |  |
| `sender_type` | TEXT | NOT NULL | — | CHECK(sender_type IN ('user', 'agent', 'system')) |  |
| `sender_name` | TEXT | NULL | — | — |  |
| `content` | TEXT | NOT NULL | — | — |  |
| `message_type` | TEXT | NOT NULL | 'chat' | CHECK(message_type IN ('chat', 'execution_result', 'coherence_update', 'system_event')) |  |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `audio_url` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `workspace_agents`

**Purpose:** Agents participating in a workspace.

**Created in:** `015_workspace_agents.sql`
**Modified in:** `057_rabble_workspaces.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `workspace_id` | UUID | NOT NULL | — | FK → teams.id |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `added_by` | TEXT | NOT NULL | — | — |  |
| `relationship` | TEXT | NOT NULL | 'hired' | CHECK(relationship IN ('hired', 'owned', 'created_here')) |  |
| `added_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |

**Table-level constraints:**
- `PRIMARY KEY (workspace_id, agent_id)`

### `object_shares`

**Purpose:** Polymorphic sharing: any object to any team or user

**Created in:** `009_add_teams_and_sharing.sql`
**Modified in:** `060_fix_object_shares_rabble.sql`, `118_object_type_workspace.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `object_id` | TEXT | NOT NULL | — | — | ID of the shared object (UUID as text or string ID) |
| `object_type` | TEXT | NOT NULL | — | CHECK(object_type IN ( 'agent', 'capability', 'forecast', 'index', 'repo', 'file' )) |  |
| `share_type` | TEXT | NOT NULL | — | CHECK(share_type IN ('team', 'user')) |  |
| `share_target` | TEXT | NOT NULL | — | — | teams.id (as text) or users.user_id |
| `permission` | TEXT | NOT NULL | 'view' | CHECK(permission IN ('view', 'edit', 'admin')) |  |
| `granted_by` | TEXT | NOT NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

**Table-level constraints:**
- `UNIQUE (object_type, object_id, share_type, share_target)`

### `composition_versions`

**Purpose:** Snapshot history of (mission + strategist + members + weights).

**Created in:** `113_composition_as_first_class.sql`
**Modified in:** `120_composition_versions_rejection.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `composition_version_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `workspace_id` | UUID | NOT NULL | — | FK → teams.id |  |
| `coordination_strategist_id` | UUID | NULL | — | — |  |
| `version_number` | INTEGER | NOT NULL | — | — |  |
| `mission` | TEXT | NULL | — | — |  |
| `member_agent_ids` | UUID[] | NULL | — | — |  |
| `member_weights` | JSONB | NULL | — | — | JSONB blob. |
| `diff_summary` | TEXT | NULL | — | — |  |
| `proposed_by` | TEXT | NULL | — | — |  |
| `accepted_by` | TEXT | NULL | — | — |  |
| `rejected_by` | TEXT | NULL | — | — |  |
| `rejection_note` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `coherence_evaluations`

**Purpose:** Coherence-evaluator runs against workspaces/agents.

**Created in:** `016_coherence_evaluations.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `eval_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `workspace_id` | UUID | NOT NULL | — | FK → teams.id |  |
| `global_score` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `quality_label` | TEXT | NOT NULL | — | — |  |
| `principle_scores` | JSONB | NOT NULL | '{}' | — | JSONB blob. |
| `health_indicators` | JSONB | NOT NULL | '{}' | — | JSONB blob. |
| `utterance_count` | INTEGER | NOT NULL | 0 | — |  |
| `message_window` | JSONB | NULL | — | — | JSONB blob. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

## Domain 4: Eval & Observability

_Evaluation runs, evaluator signals, test cases and two-reviewer requests._

### `eval_runs`

**Purpose:** Evaluator pipeline runs.

**Created in:** `027_eval_framework.sql`
**Modified in:** `104_evaluator_signals.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `run_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `triggered_by` | TEXT | NOT NULL | — | — |  |
| `status` | TEXT | NOT NULL | 'running' | CHECK(status IN ('running', 'completed', 'failed')) | Lifecycle status (see CHECK constraint for allowed values). |
| `judge_enabled` | BOOLEAN | NOT NULL | FALSE | — | Boolean flag. |
| `total_cases` | INTEGER | NOT NULL | 0 | — |  |
| `passed` | INTEGER | NOT NULL | 0 | — |  |
| `failed` | INTEGER | NOT NULL | 0 | — |  |
| `avg_latency_ms` | BIGINT | NULL | — | — |  |
| `avg_tokens` | INTEGER | NULL | — | — |  |
| `avg_judge_score` | DOUBLE PRECISION | NULL | — | — |  |
| `total_cost_credits` | INTEGER | NOT NULL | 0 | — |  |
| `case_results` | JSONB | NOT NULL | '[]' | — | JSONB blob. |
| `regression_detected` | BOOLEAN | NOT NULL | FALSE | — | Boolean flag. |
| `regression_details` | JSONB | NULL | — | — | JSONB blob. |
| `duration_ms` | BIGINT | NULL | — | — |  |
| `aggregated_signal` | JSONB | NULL | — | — | JSONB blob. |
| `conflict_flags` | JSONB | NOT NULL | '[]'::jsonb | — | JSONB blob. |
| `prefilter_blocked` | BOOLEAN | NOT NULL | FALSE | — | Boolean flag. |
| `started_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Start timestamp. |
| `completed_at` | TIMESTAMPTZ | NULL | — | — | Completion timestamp; NULL while running. |

### `eval_signals`

**Purpose:** Per-dimension signals produced by evaluators.

**Created in:** `104_evaluator_signals.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `signal_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `run_id` | UUID | NULL | — | FK → eval_runs.run_id |  |
| `episode_id` | UUID | NULL | — | FK → episodes.episode_id | Owning episode (→ episodes.episode_id). |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `evaluator_name` | TEXT | NOT NULL | — | — |  |
| `evaluator_version` | TEXT | NOT NULL | 'v1' | — |  |
| `evaluator_tier` | TEXT | NOT NULL | 'dimensional' | CHECK(evaluator_tier IN ('pre_filter', 'dimensional')) |  |
| `dimension` | TEXT | NOT NULL | — | — |  |
| `score` | DOUBLE PRECISION | NOT NULL | — | CHECK(score >= 0.0 AND score <= 1.0) | Numeric score. |
| `confidence` | DOUBLE PRECISION | NOT NULL | 1.0 | CHECK(confidence >= 0.0 AND confidence <= 1.0) | Model/extraction confidence score in [0,1]. |
| `flags` | JSONB | NOT NULL | '[]'::jsonb | — | JSONB blob. |
| `bundle_provenance` | TEXT | NOT NULL | 'auto_pass' | — |  |
| `persona_version` | INTEGER | NULL | — | — |  |
| `model_used` | TEXT | NULL | — | — |  |
| `cost_credits` | INTEGER | NOT NULL | 0 | — |  |
| `latency_ms` | BIGINT | NOT NULL | 0 | — |  |
| `rationale` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `eval_test_cases`

**Purpose:** Test cases used by evaluators.

**Created in:** `027_eval_framework.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `test_case_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `query` | TEXT | NOT NULL | — | — |  |
| `expected_output` | TEXT | NULL | — | — |  |
| `rubric` | TEXT | NULL | — | — |  |
| `tags` | TEXT[] | NULL | '{}' | — | Free-form string tags array. |
| `is_active` | BOOLEAN | NOT NULL | TRUE | — | Active/enabled flag. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `two_reviewer_requests`

**Purpose:** Requests for a second human reviewer.

**Created in:** `108_intervention_feedback_loop.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `request_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `anomaly_event_id` | UUID | NOT NULL | — | FK → anomaly_events.event_id |  |
| `agent_id` | UUID | NOT NULL | — | — | Owning/related agent (→ agents.agent_id). |
| `first_reviewer_id` | TEXT | NOT NULL | — | — |  |
| `second_reviewer_id` | TEXT | NULL | — | — |  |
| `correction_id` | UUID | NULL | — | — |  |
| `synthetic_episode_id` | UUID | NULL | — | — |  |
| `encoded_intervention` | JSONB | NOT NULL | — | — | JSONB blob. |
| `second_approved` | BOOLEAN | NULL | — | — | Boolean flag. |
| `status` | TEXT | NOT NULL | 'pending' | CHECK(status IN ('pending', 'approved', 'rejected', 'expired')) | Lifecycle status (see CHECK constraint for allowed values). |
| `notes` | TEXT | NULL | — | — |  |
| `first_reviewed_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |
| `second_reviewed_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

## Domain 5: Wallet & Billing

_Per-user wallets and double-entry credit ledger._

### `wallets`

**Purpose:** Per-user credit wallets.

**Created in:** `012_credit_ledger.sql`
**Modified in:** `057_rabble_workspaces.sql`, `066_wallet_balance_split.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `wallet_id` | UUID | NOT NULL | gen_random_uuid() | PK | Wallet identifier (→ wallets.wallet_id). |
| `owner_id` | TEXT | NOT NULL | — | UK | Owning user/team identifier. |
| `owner_type` | TEXT | NOT NULL | — | CHECK(owner_type IN ('user', 'workspace')) |  |
| `balance` | INTEGER | NOT NULL | 0 | — |  |
| `total_deposited` | INTEGER | NOT NULL | 0 | — |  |
| `total_spent` | INTEGER | NOT NULL | 0 | — |  |
| `granted_balance` | INTEGER | NOT NULL | 0 | — |  |
| `purchased_balance` | INTEGER | NOT NULL | 0 | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `credit_ledger`

**Purpose:** Append-only credit movements (double-entry).

**Created in:** `012_credit_ledger.sql`
**Modified in:** `020_stripe_and_profile.sql`, `026_fork_royalty_tx_type.sql`, `027_eval_framework.sql`, `030_shopping_marketplace.sql`, `032_fix_tx_type_constraint.sql`, `035_fix_tx_type_constraint.sql`, `042_rabble_creatures.sql`, `045_rabble_funding.sql`, `049_akp_foundation.sql`, `050_fix_tx_type_constraint_rabble.sql`, `051_swarm_telemetry.sql`, `052_sosa_observations.sql`, `057_rabble_workspaces.sql`, `059_agent_wallet_admin.sql`, `061_swarm_algorithms.sql`, `063_sub_flocks.sql`, `064_creature_animation_layers.sql`, `075_fix_tx_type_constraint.sql`, `076_drop_tx_type_constraint.sql`, `099_polymarket_observations.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `tx_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `wallet_id` | UUID | NOT NULL | — | FK → wallets.wallet_id | Wallet identifier (→ wallets.wallet_id). |
| `related_id` | TEXT | NULL | — | — |  |
| `stripe_session_id` | TEXT | NULL | — | — |  |
| `amount` | INTEGER | NOT NULL | — | — |  |
| `balance_after` | INTEGER | NOT NULL | — | — |  |
| `tx_type` | TEXT | NOT NULL | — | CHECK(tx_type IN ( 'deposit', 'withdrawal', 'execution_fee', 'gas_fee', 'education_al…) |  |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

## Domain 6: Marketplace

_Marketplace listings, transactions and shopping profiles._

### `marketplace_listings`

**Purpose:** Listings of agents/compositions for sale or fork.

**Created in:** `030_shopping_marketplace.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `listing_id` | UUID | NOT NULL | gen_random_uuid() | PK | Owning marketplace listing (→ marketplace_listings.id). |
| `profile_id` | UUID | NOT NULL | — | FK → shopping_profiles.profile_id |  |
| `seller_id` | TEXT | NOT NULL | — | — |  |
| `price_credits` | INTEGER | NOT NULL | 2 | — |  |
| `max_queries_per_buyer` | INTEGER | NULL | NULL | — |  |
| `total_queries` | INTEGER | NOT NULL | 0 | — |  |
| `total_earned` | INTEGER | NOT NULL | 0 | — |  |
| `status` | TEXT | NOT NULL | 'active' | CHECK(status IN ('active', 'paused', 'delisted')) | Lifecycle status (see CHECK constraint for allowed values). |
| `category_tags` | TEXT[] | NULL | '{}' | — |  |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `marketplace_transactions`

**Purpose:** Marketplace purchases / fork transactions.

**Created in:** `030_shopping_marketplace.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `tx_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `listing_id` | UUID | NOT NULL | — | FK → marketplace_listings.listing_id | Owning marketplace listing (→ marketplace_listings.id). |
| `buyer_id` | TEXT | NOT NULL | — | — |  |
| `seller_id` | TEXT | NOT NULL | — | — |  |
| `similarity_score` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `product_embedding_hash` | TEXT | NULL | — | — |  |
| `credits_charged` | INTEGER | NOT NULL | — | — |  |
| `credits_to_seller` | INTEGER | NOT NULL | — | — |  |
| `platform_fee` | INTEGER | NOT NULL | 0 | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `shopping_profiles`

**Purpose:** Buyer profile / preferences.

**Created in:** `030_shopping_marketplace.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `profile_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `user_id` | TEXT | NOT NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `agent_id` | UUID | NOT NULL | — | FK → agents.agent_id | Owning/related agent (→ agents.agent_id). |
| `profile_name` | TEXT | NOT NULL | 'default' | — |  |
| `composite_embedding` | vector(1024) | NULL | — | — | pgvector embedding. |
| `embedding_version` | INTEGER | NOT NULL | 1 | — |  |
| `episode_count` | INTEGER | NOT NULL | 0 | — |  |
| `category_tags` | TEXT[] | NULL | '{}' | — |  |
| `price_sensitivity` | DOUBLE PRECISION | NULL | — | — |  |
| `quality_bias` | DOUBLE PRECISION | NULL | — | — |  |
| `brand_affinities` | JSONB | NULL | '{}' | — | JSONB blob. |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `is_listed` | BOOLEAN | NOT NULL | FALSE | — | Boolean flag. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

**Table-level constraints:**
- `UNIQUE(user_id, agent_id, profile_name)`

## Domain 7: Rabble Core

_Creatures (virtual drones), genome versions, state, conditions, flights, animation, devices, tethers, favourites, blocks, swarm sessions/events/algorithms and telemetry._

### `creatures`

**Purpose:** Rabble creatures — user-owned virtual drones.

**Created in:** `042_rabble_creatures.sql`
**Modified in:** `052_sosa_observations.sql`, `054_creature_management.sql`, `058_creature_presence.sql`, `063_sub_flocks.sql`, `064_creature_animation_layers.sql`, `065_creature_visibility.sql`, `080_drop_redundant_creature_columns.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `creature_id` | UUID | NOT NULL | gen_random_uuid() | PK | Owning creature (→ creatures.id). |
| `owner_id` | TEXT | NOT NULL | — | — | Owning user/team identifier. |
| `workspace_id` | UUID | NULL | — | — |  |
| `gbif_key` | BIGINT | NULL | — | — |  |
| `scientific_name` | TEXT | NOT NULL | — | — |  |
| `common_name` | TEXT | NULL | — | — |  |
| `species_group` | TEXT | NOT NULL | 'butterfly' | — |  |
| `taxonomy` | JSONB | NULL | '{}' | — | JSONB blob. |
| `specimen_name` | TEXT | NULL | — | — |  |
| `asset_path` | TEXT | NOT NULL | — | — |  |
| `flight_silhouette_path` | TEXT | NULL | — | — |  |
| `variation_notes` | TEXT | NULL | — | — |  |
| `generation_params` | JSONB | NULL | '{}' | — | JSONB blob. |
| `mint_number` | INTEGER | NOT NULL | 1 | — |  |
| `total_flights` | INTEGER | NOT NULL | 0 | — |  |
| `total_flight_time_seconds` | BIGINT | NOT NULL | 0 | — |  |
| `data_card` | JSONB | NULL | '{}' | — | JSONB blob. |
| `status` | TEXT | NOT NULL | 'active' | — | Lifecycle status (see CHECK constraint for allowed values). |
| `flagged` | BOOLEAN | NOT NULL | false | — | Boolean flag. |
| `flag_reason` | TEXT | NULL | — | — |  |
| `attraction_score` | INTEGER | NOT NULL | 0 | — |  |
| `animation_status` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

**Table-level constraints:**
- `unique_locations INT NOT NULL DEFAULT 0`

### `creature_collections`

**Purpose:** Named groupings of creatures owned by a user.

**Created in:** `042_rabble_creatures.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `collection_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `owner_id` | TEXT | NOT NULL | — | — | Owning user/team identifier. |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `creature_ids` | JSONB | NOT NULL | '[]' | — | JSONB blob. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `creature_versions`

**Purpose:** Versioned genome snapshots of a creature.

**Created in:** `078_creature_versioned_state.sql`
**Modified in:** `085_rename_creature_states.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `version_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `rabble_id` | UUID | NULL | — | — |  |
| `workspace_id` | UUID | NULL | — | — |  |
| `version_number` | INTEGER | NOT NULL | — | — |  |
| `state` | TEXT | NOT NULL | — | CHECK(state IN ('perch_solo', 'fly', 'perch_rabble')) |  |
| `previous_state` | TEXT | NULL | — | — |  |
| `location_lat` | DOUBLE PRECISION | NULL | — | — |  |
| `location_lng` | DOUBLE PRECISION | NULL | — | — |  |
| `h3_cell` | TEXT | NULL | — | — |  |
| `transition_type` | TEXT | NOT NULL | — | — |  |
| `triggered_by` | TEXT | NOT NULL | — | — |  |
| `episode_ids` | UUID[] | NULL | — | — |  |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `valid_from` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |
| `recorded_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |

**Table-level constraints:**
- `UNIQUE(creature_id, version_number)`

### `creature_state`

**Purpose:** Mutable per-creature state.

**Created in:** `078_creature_versioned_state.sql`
**Modified in:** `078_creature_versioned_state.sql`, `084_drop_creature_state_rabble_fk.sql`, `085_rename_creature_states.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `creature_id` | UUID | NOT NULL | — | PK; FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `rabble_id` | UUID | NULL | — | FK → swarm_events.swarm_id |  |
| `workspace_id` | UUID | NULL | — | — |  |
| `version_id` | UUID | NULL | — | — |  |
| `state` | TEXT | NOT NULL | 'perch_solo' | CHECK(state IN ('perch_solo', 'fly', 'perch_rabble')) |  |
| `location_lat` | DOUBLE PRECISION | NULL | — | — |  |
| `location_lng` | DOUBLE PRECISION | NULL | — | — |  |
| `h3_cell` | TEXT | NULL | — | — |  |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `creature_conditions`

**Purpose:** Active conditions / status effects on a creature.

**Created in:** `078_creature_versioned_state.sql`
**Modified in:** `079_conditions_presence.sql`, `081_fix_visibility_contacts.sql`, `083_genome_profile_cache.sql`, `100_cognition_tier.sql`, `102_cognition_tier_nullable.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `creature_id` | UUID | NOT NULL | — | PK; FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `visibility` | TEXT | NOT NULL | 'public' | CHECK(visibility IN ('public', 'contacts_only', 'private')) | Visibility scope (public / private / contacts etc). |
| `walk_in_price` | INTEGER | NULL | — | — |  |
| `sosa_opt_in` | BOOLEAN | NOT NULL | false | — | Boolean flag. |
| `active_modules` | TEXT[] | NOT NULL | '{}' | — |  |
| `presence` | TEXT | NOT NULL | 'active' | — |  |
| `genome_profile` | JSONB | NULL | — | — | JSONB blob. |
| `cognition_tier` | TEXT | NULL | 'free' | CHECK(cognition_tier IN ('free', 'standard', 'premium')) |  |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `creature_flights`

**Purpose:** Active or historical flight sessions.

**Created in:** `042_rabble_creatures.sql`
**Modified in:** `047_flight_path_samples.sql`, `063_sub_flocks.sql`, `065_creature_visibility.sql`, `067_flight_environment.sql`, `068_flight_data_source.sql`, `086_creature_flights_metadata.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `flight_id` | UUID | NOT NULL | gen_random_uuid() | PK | Flight session (→ creature_flights.id). |
| `creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `beacon_id` | UUID | NULL | — | FK → ar_beacons.beacon_id |  |
| `owner_id` | TEXT | NOT NULL | — | — | Owning user/team identifier. |
| `choreo_id` | UUID | NULL | — | — |  |
| `swarm_id` | UUID | NULL | — | — |  |
| `sub_flock_id` | UUID | NULL | — | FK → swarm_sub_flocks.sub_flock_id |  |
| `attracted_by_creature_id` | UUID | NULL | — | — |  |
| `h3_cell` | TEXT | NOT NULL | — | — |  |
| `h3_resolution` | INTEGER | NOT NULL | 12 | — |  |
| `center_lat` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `center_lng` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `location_name` | TEXT | NULL | — | — |  |
| `country_code` | TEXT | NULL | — | — |  |
| `flight_pattern` | TEXT | NOT NULL | 'wander' | — |  |
| `duration_seconds` | INTEGER | NULL | — | — |  |
| `path_samples` | JSONB | NULL | NULL | — | JSONB blob. |
| `visibility` | TEXT | NOT NULL | 'public' | — | Visibility scope (public / private / contacts etc). |
| `environment` | JSONB | NULL | — | — | JSONB blob. |
| `data_source` | TEXT | NOT NULL | 'synthetic' | — |  |
| `metadata` | JSONB | NULL | — | — | Free-form JSONB metadata blob. |
| `started_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Start timestamp. |
| `ended_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

### `creature_images`

**Purpose:** Creature image assets.

**Created in:** `053_creature_image_storage.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `creature_id` | UUID | NOT NULL | — | PK; FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `image_bytes` | BYTEA | NOT NULL | — | — |  |
| `mime_type` | TEXT | NOT NULL | 'image/png' | — |  |
| `file_size` | INTEGER | NOT NULL | 0 | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `creature_animation_layers`

**Purpose:** Per-creature animation layers.

**Created in:** `064_creature_animation_layers.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `layer_name` | TEXT | NOT NULL | — | — |  |
| `image_bytes` | BYTEA | NOT NULL | — | — |  |
| `mime_type` | TEXT | NOT NULL | 'image/png' | — |  |
| `file_size` | INTEGER | NOT NULL | 0 | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

**Table-level constraints:**
- `PRIMARY KEY (creature_id, layer_name)`

### `creature_devices`

**Purpose:** Physical/virtual devices bound to a creature.

**Created in:** `056_devices.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `device_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `owner_id` | TEXT | NOT NULL | — | — | Owning user/team identifier. |
| `device_type` | TEXT | NOT NULL | — | — |  |
| `device_identifier` | TEXT | NOT NULL | — | — |  |
| `device_name` | TEXT | NULL | — | — |  |
| `is_active` | BOOLEAN | NOT NULL | true | — | Active/enabled flag. |
| `last_lat` | DOUBLE PRECISION | NULL | — | — |  |
| `last_lng` | DOUBLE PRECISION | NULL | — | — |  |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `last_seen_at` | TIMESTAMPTZ | NULL | — | — | Last time entity was observed. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

**Table-level constraints:**
- `UNIQUE(owner_id, device_identifier)`

### `creature_tethers`

**Purpose:** Tether/anchor links between creatures or to a perch.

**Created in:** `074_creature_tethers.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `tether_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `owner_id` | TEXT | NOT NULL | — | — | Owning user/team identifier. |
| `tether_type` | TEXT | NOT NULL | 'phone_gps' | — |  |
| `device_label` | TEXT | NULL | — | — |  |
| `config` | JSONB | NULL | '{}' | — | Free-form JSONB configuration. |
| `active` | BOOLEAN | NOT NULL | true | — | Boolean flag. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `deactivated_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

### `creature_favourites`

**Purpose:** Per-user favourite creatures.

**Created in:** `087_creature_favourites.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `user_id` | TEXT | NOT NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

**Table-level constraints:**
- `PRIMARY KEY (user_id, creature_id)`

### `creature_blocks`

**Purpose:** Block lists at the creature level.

**Created in:** `097_governance.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `blocker_creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id |  |
| `blocked_creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id |  |
| `blocker_user_id` | TEXT | NOT NULL | — | — |  |
| `blocked_user_id` | TEXT | NOT NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

**Table-level constraints:**
- `UNIQUE(blocker_creature_id, blocked_creature_id)`
- `CHECK(blocker_creature_id != blocked_creature_id)`

### `swarm_events`

**Purpose:** Discrete events fired inside a swarm session.

**Created in:** `042_rabble_creatures.sql`
**Modified in:** `045_rabble_funding.sql`, `046_rabble_visibility.sql`, `062_anchor_creature.sql`, `072_perch_model.sql`, `073_walk_in_budget.sql`, `082_rabble_radius.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `swarm_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `creator_id` | TEXT | NOT NULL | — | — |  |
| `workspace_id` | UUID | NULL | — | — |  |
| `grid_map_id` | UUID | NULL | — | FK → ar_grid_maps.map_id |  |
| `anchor_creature_id` | UUID | NULL | — | — |  |
| `h3_cell` | TEXT | NOT NULL | — | — |  |
| `h3_resolution` | INTEGER | NOT NULL | 12 | — |  |
| `center_lat` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `center_lng` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `location_name` | TEXT | NULL | — | — |  |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `species_filter` | TEXT | NULL | — | — |  |
| `max_participants` | INTEGER | NULL | — | — |  |
| `status` | TEXT | NOT NULL | 'scheduled' | — | Lifecycle status (see CHECK constraint for allowed values). |
| `participant_count` | INTEGER | NOT NULL | 0 | — |  |
| `creature_count` | INTEGER | NOT NULL | 0 | — |  |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `funding_mode` | TEXT | NOT NULL | 'hosted' | — |  |
| `invite_pool` | INTEGER | NOT NULL | 0 | — |  |
| `invite_pool_remaining` | INTEGER | NOT NULL | 0 | — |  |
| `suggested_contribution` | INTEGER | NOT NULL | 1 | — |  |
| `total_contributions` | INTEGER | NOT NULL | 0 | — |  |
| `qr_token` | TEXT | NULL | — | — |  |
| `visibility` | TEXT | NOT NULL | 'public' | — | Visibility scope (public / private / contacts etc). |
| `walk_in_price` | INTEGER | NULL | — | — |  |
| `walk_in_budget` | INTEGER | NOT NULL | 0 | — |  |
| `walk_in_budget_remaining` | INTEGER | NOT NULL | 0 | — |  |
| `radius_meters` | INTEGER | NOT NULL | 100 | — |  |
| `starts_at` | TIMESTAMPTZ | NOT NULL | — | — | Timestamp. |
| `ends_at` | TIMESTAMPTZ | NOT NULL | — | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `anchor_transferred_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

### `swarm_participants`

**Purpose:** Tracks user participation in rabble (swarm) events

**Created in:** `091_swarm_participants.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `participant_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `swarm_id` | UUID | NOT NULL | — | FK → swarm_events.swarm_id | The rabble event this user is participating in |
| `user_id` | TEXT | NOT NULL | — | FK → users.user_id | The user participating in the rabble |
| `creature_id` | UUID | NULL | — | FK → creatures.creature_id | The creature the user brought to the rabble (optional) |
| `status` | TEXT | NOT NULL | 'active' | CHECK(status IN ('active', 'left', 'kicked')) | Participation status: active, left, or kicked |
| `role` | TEXT | NULL | 'member' | CHECK(role IN ('host', 'cohost', 'member')) | User role in the swarm: host, cohost, or member |
| `joined_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |
| `left_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `swarm_sessions`

**Purpose:** Coordinated swarm flight session.

**Created in:** `051_swarm_telemetry.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `session_id` | UUID | NOT NULL | gen_random_uuid() | PK | Owning session. |
| `owner_id` | TEXT | NOT NULL | — | — | Owning user/team identifier. |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `agent_count` | INTEGER | NOT NULL | 0 | — |  |
| `formation_type` | TEXT | NULL | — | — |  |
| `mission_type` | TEXT | NULL | — | — |  |
| `environment` | JSONB | NULL | '{}' | — | JSONB blob. |
| `status` | TEXT | NOT NULL | 'active' | — | Lifecycle status (see CHECK constraint for allowed values). |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `started_at` | TIMESTAMPTZ | NULL | NOW() | — | Start timestamp. |
| `ended_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

### `swarm_telemetry`

**Purpose:** Telemetry samples from a swarm session.

**Created in:** `051_swarm_telemetry.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `telemetry_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `session_id` | UUID | NOT NULL | — | FK → swarm_sessions.session_id | Owning session. |
| `agent_label` | TEXT | NOT NULL | — | — |  |
| `agent_type` | TEXT | NOT NULL | 'artificial' | — |  |
| `timestamp_ms` | BIGINT | NOT NULL | — | — |  |
| `x_location` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `y_location` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `z_location` | DOUBLE PRECISION | NULL | 0.0 | — |  |
| `heading` | DOUBLE PRECISION | NULL | — | — | Compass heading in degrees. |
| `speed` | DOUBLE PRECISION | NULL | — | — | Speed (units depend on column). |
| `energy` | DOUBLE PRECISION | NULL | — | — |  |
| `distance_to_goal` | DOUBLE PRECISION | NULL | — | — |  |
| `team_alignment` | DOUBLE PRECISION | NULL | — | — |  |
| `team_cohesion` | DOUBLE PRECISION | NULL | — | — |  |
| `team_separation` | DOUBLE PRECISION | NULL | — | — |  |
| `influence` | DOUBLE PRECISION | NULL | — | — |  |
| `action` | TEXT | NULL | — | — |  |
| `temperament` | TEXT | NULL | — | — |  |
| `extra` | JSONB | NULL | '{}' | — | JSONB blob. |

### `swarm_sub_flocks`

**Purpose:** Sub-flock groupings within a swarm.

**Created in:** `063_sub_flocks.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `sub_flock_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `swarm_id` | UUID | NOT NULL | — | FK → swarm_events.swarm_id |  |
| `owner_id` | TEXT | NOT NULL | — | — | Owning user/team identifier. |
| `formation_algorithm_id` | UUID | NULL | — | FK → swarm_algorithms.algorithm_id |  |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `species_filter` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `swarm_algorithms`

**Purpose:** Available swarm coordination algorithms.

**Created in:** `061_swarm_algorithms.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `algorithm_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `name` | TEXT | NOT NULL | — | UK | Human-readable name. |
| `display_name` | TEXT | NOT NULL | — | — | Display name shown in UI. |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `category` | TEXT | NOT NULL | — | — |  |
| `onto4mat_class` | TEXT | NOT NULL | — | — |  |
| `formation_spec` | JSONB | NOT NULL | — | — | JSONB blob. |
| `tier` | TEXT | NOT NULL | 'premium' | — |  |
| `cost_credits` | INTEGER | NOT NULL | 3 | — |  |
| `icon` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | now() | — | Row creation timestamp. |

### `swarm_activations`

**Purpose:** Bindings of an algorithm to a swarm session.

**Created in:** `061_swarm_algorithms.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `activation_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `algorithm_id` | UUID | NOT NULL | — | FK → swarm_algorithms.algorithm_id |  |
| `user_id` | TEXT | NOT NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `swarm_id` | UUID | NOT NULL | — | — |  |
| `activated_at` | TIMESTAMPTZ | NOT NULL | now() | — | Timestamp. |

### `rabble_messages`

**Purpose:** Creature/user messages inside Rabble.

**Created in:** `044_rabble_messages.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `message_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `swarm_id` | UUID | NOT NULL | — | FK → swarm_events.swarm_id |  |
| `sender_id` | TEXT | NOT NULL | — | — |  |
| `creature_id` | UUID | NULL | — | FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `creature_name` | TEXT | NULL | — | — |  |
| `species_name` | TEXT | NULL | — | — |  |
| `species_group` | TEXT | NULL | — | — |  |
| `content` | TEXT | NOT NULL | — | — |  |
| `message_type` | TEXT | NOT NULL | 'chat' | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `rabble_co_presence`

**Purpose:** Co-presence events (two creatures near each other).

**Created in:** `090_social_layer.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `rabble_id` | UUID | NOT NULL | — | FK → swarm_events.swarm_id |  |
| `creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `owner_id` | TEXT | NOT NULL | — | — | Owning user/team identifier. |
| `overlap_seconds` | INTEGER | NULL | 0 | — |  |
| `joined_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |
| `left_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

**Table-level constraints:**
- `UNIQUE(rabble_id, creature_id)`

### `rabble_follows`

**Purpose:** Follow relationships between users.

**Created in:** `094_rabble_follows.sql`
**Modified in:** `094_rabble_follows.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `user_id` | TEXT | NOT NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `swarm_id` | UUID | NOT NULL | — | — |  |
| `notify_on_join` | BOOLEAN | NOT NULL | TRUE | — | Boolean flag. |
| `notify_on_start` | BOOLEAN | NOT NULL | TRUE | — | Boolean flag. |
| `notify_on_end` | BOOLEAN | NOT NULL | TRUE | — | Boolean flag. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

**Table-level constraints:**
- `UNIQUE(user_id, swarm_id)`

### `rabble_ejections`

**Purpose:** Moderation ejections from rabble sessions.

**Created in:** `097_governance.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `swarm_id` | UUID | NOT NULL | — | FK → swarm_events.swarm_id |  |
| `ejected_creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id |  |
| `ejected_user_id` | TEXT | NOT NULL | — | — |  |
| `ejected_by_user` | TEXT | NOT NULL | — | — |  |
| `reason` | TEXT | NULL | — | — |  |
| `permanent` | BOOLEAN | NOT NULL | false | — | Boolean flag. |
| `cooldown_until` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `ejected_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |

### `flight_telemetry`

**Purpose:** Per-flight telemetry samples.

**Created in:** `078_creature_versioned_state.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `telemetry_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `version_id` | UUID | NOT NULL | — | FK → creature_versions.version_id |  |
| `creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `device_id` | UUID | NULL | — | — |  |
| `lat` | DOUBLE PRECISION | NOT NULL | — | — | Latitude (WGS-84). |
| `lng` | DOUBLE PRECISION | NOT NULL | — | — | Longitude (WGS-84). |
| `altitude_m` | DOUBLE PRECISION | NULL | — | — | Altitude in metres. |
| `heading` | DOUBLE PRECISION | NULL | — | — | Compass heading in degrees. |
| `data_source` | TEXT | NOT NULL | 'app' | — |  |
| `observed_at` | TIMESTAMPTZ | NOT NULL | — | — | Timestamp. |
| `recorded_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |

### `telemetry_points`

**Purpose:** Generic geo-tagged telemetry point store.

**Created in:** `074_creature_tethers.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `point_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `tether_id` | UUID | NOT NULL | — | FK → creature_tethers.tether_id |  |
| `creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id | Owning creature (→ creatures.id). |
| `lat` | DOUBLE PRECISION | NOT NULL | — | — | Latitude (WGS-84). |
| `lng` | DOUBLE PRECISION | NOT NULL | — | — | Longitude (WGS-84). |
| `altitude` | DOUBLE PRECISION | NULL | — | — | Altitude (units depend on column). |
| `accuracy` | DOUBLE PRECISION | NULL | — | — |  |
| `speed` | DOUBLE PRECISION | NULL | — | — | Speed (units depend on column). |
| `heading` | DOUBLE PRECISION | NULL | — | — | Compass heading in degrees. |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `recorded_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |

## Domain 8: Spatial / AR / Telemetry

_AR beacons, choreographies, grid maps, SOSA platforms/sensors/observations, observation sessions, saved locations and voice assets._

### `ar_beacons`

**Purpose:** AR beacons (geo-anchored markers).

**Created in:** `041_ar_beacons.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `beacon_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `workspace_id` | UUID | NOT NULL | — | — |  |
| `creator_id` | TEXT | NOT NULL | — | — |  |
| `agent_name` | TEXT | NOT NULL | 'ar_beacon' | — |  |
| `h3_cell` | TEXT | NOT NULL | — | — |  |
| `h3_resolution` | INTEGER | NOT NULL | 12 | — |  |
| `center_lat` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `center_lng` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `asset_path` | TEXT | NOT NULL | — | — |  |
| `asset_type` | TEXT | NOT NULL | 'image' | — |  |
| `azimuth_deg` | DOUBLE PRECISION | NOT NULL | 0 | — |  |
| `elevation_deg` | DOUBLE PRECISION | NOT NULL | 0 | — |  |
| `billboard` | BOOLEAN | NOT NULL | true | — | Boolean flag. |
| `scale` | DOUBLE PRECISION | NOT NULL | 1.0 | — |  |
| `ttl_seconds` | INTEGER | NOT NULL | 86400 | — |  |
| `decay_style` | TEXT | NOT NULL | 'fade' | — |  |
| `visibility` | TEXT | NOT NULL | 'public' | — | Visibility scope (public / private / contacts etc). |
| `tags` | JSONB | NULL | '[]' | — | Free-form string tags array. |
| `interaction` | JSONB | NULL | '{}' | — | JSONB blob. |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `expires_at` | TIMESTAMPTZ | NOT NULL | — | — | Expiry timestamp; row becomes invalid after this. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `ar_choreographies`

**Purpose:** Choreographed AR sequences over beacons.

**Created in:** `041_ar_beacons.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `choreo_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `beacon_id` | UUID | NOT NULL | — | FK → ar_beacons.beacon_id |  |
| `workspace_id` | UUID | NOT NULL | — | — |  |
| `name` | TEXT | NULL | — | — | Human-readable name. |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `motion` | JSONB | NOT NULL | — | — | JSONB blob. |
| `duration_total_ms` | INTEGER | NULL | — | — |  |
| `loop_motion` | BOOLEAN | NOT NULL | true | — | Boolean flag. |
| `active` | BOOLEAN | NOT NULL | true | — | Boolean flag. |
| `priority` | INTEGER | NOT NULL | 1 | — |  |
| `triggers` | JSONB | NULL | '{}' | — | JSONB blob. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `ar_grid_maps`

**Purpose:** AR grid map registrations.

**Created in:** `041_ar_beacons.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `map_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `workspace_id` | UUID | NOT NULL | — | — |  |
| `creator_id` | TEXT | NOT NULL | — | — |  |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `center_lat` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `center_lng` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `center_h3` | TEXT | NOT NULL | — | — |  |
| `center_resolution` | INTEGER | NOT NULL | 9 | — |  |
| `grid_resolution` | INTEGER | NOT NULL | 12 | — |  |
| `radius_rings` | INTEGER | NOT NULL | 5 | — |  |
| `total_cells` | INTEGER | NOT NULL | 0 | — |  |
| `quadrants` | JSONB | NOT NULL | '[]' | — | JSONB blob. |
| `zones` | JSONB | NOT NULL | '[]' | — | JSONB blob. |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `sosa_platforms`

**Purpose:** SOSA Platform descriptors (a physical/virtual device hosting sensors).

**Created in:** `052_sosa_observations.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `platform_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `owner_id` | TEXT | NOT NULL | — | — | Owning user/team identifier. |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `platform_type` | TEXT | NOT NULL | — | — |  |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `location` | JSONB | NULL | '{}' | — | JSONB blob. |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `created_at` | TIMESTAMPTZ | NULL | NOW() | — | Row creation timestamp. |

### `sosa_sensors`

**Purpose:** SOSA Sensor descriptors hosted on a platform.

**Created in:** `052_sosa_observations.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `sensor_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `platform_id` | UUID | NOT NULL | — | FK → sosa_platforms.platform_id |  |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `observable_property` | TEXT | NOT NULL | — | — |  |
| `unit` | TEXT | NULL | — | — |  |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `created_at` | TIMESTAMPTZ | NULL | NOW() | — | Row creation timestamp. |

### `sosa_observations`

**Purpose:** SOSA observations produced by a sensor.

**Created in:** `052_sosa_observations.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `observation_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `session_id` | UUID | NOT NULL | — | FK → observation_sessions.session_id | Owning session. |
| `sensor_id` | UUID | NULL | — | FK → sosa_sensors.sensor_id |  |
| `platform_id` | UUID | NOT NULL | — | — |  |
| `observable_property` | TEXT | NOT NULL | — | — |  |
| `feature_of_interest` | TEXT | NULL | — | — |  |
| `result_value` | DOUBLE PRECISION | NOT NULL | — | — |  |
| `result_unit` | TEXT | NULL | — | — |  |
| `phenomenon_time` | BIGINT | NOT NULL | — | — |  |
| `result_time` | BIGINT | NULL | — | — |  |
| `procedure` | TEXT | NULL | — | — |  |
| `extra` | JSONB | NULL | '{}' | — | JSONB blob. |

### `observation_sessions`

**Purpose:** Bracketed observation sessions grouping observations.

**Created in:** `052_sosa_observations.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `session_id` | UUID | NOT NULL | gen_random_uuid() | PK | Owning session. |
| `owner_id` | TEXT | NOT NULL | — | — | Owning user/team identifier. |
| `platform_id` | UUID | NOT NULL | — | FK → sosa_platforms.platform_id |  |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `status` | TEXT | NOT NULL | 'active' | — | Lifecycle status (see CHECK constraint for allowed values). |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `started_at` | TIMESTAMPTZ | NULL | NOW() | — | Start timestamp. |
| `ended_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

### `saved_locations`

**Purpose:** User-saved geographic locations.

**Created in:** `095_saved_locations.sql`
**Modified in:** `095_saved_locations.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `user_id` | TEXT | NOT NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `source_id` | UUID | NULL | — | — |  |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `lat` | DOUBLE PRECISION | NOT NULL | — | — | Latitude (WGS-84). |
| `lng` | DOUBLE PRECISION | NOT NULL | — | — | Longitude (WGS-84). |
| `radius_meters` | INTEGER | NOT NULL | 500 | — |  |
| `h3_cell` | TEXT | NULL | — | — |  |
| `source` | TEXT | NOT NULL | 'pin' | — |  |
| `notes` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `voice_assets`

**Purpose:** Generated audio assets from text-to-speech synthesis

**Created in:** `048b_voice_assets.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `asset_id` | UUID | NOT NULL | gen_random_uuid() | PK |  |
| `object_id` | TEXT | NOT NULL | — | — |  |
| `voice_id` | TEXT | NULL | — | — |  |
| `object_type` | TEXT | NOT NULL | — | — | Type of object that owns this audio (episode, message, etc.) |
| `provider` | TEXT | NOT NULL | — | — |  |
| `duration_ms` | INTEGER | NULL | — | — |  |
| `character_count` | INTEGER | NOT NULL | — | — |  |
| `storage_url` | TEXT | NOT NULL | — | — | URL to audio file storage (Cloudflare R2 or S3) |
| `created_at` | TIMESTAMPTZ | NULL | NOW() | — | Row creation timestamp. |

## Domain 9: Social

_Friendships, invites, activity events, notifications, contacts, blocks and reports._

### `creature_friendships`

**Purpose:** Symmetric/asymmetric friendship between creatures.

**Created in:** `090_social_layer.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `creature_a` | UUID | NOT NULL | — | FK → creatures.creature_id |  |
| `creature_b` | UUID | NOT NULL | — | FK → creatures.creature_id |  |
| `initiated_by` | UUID | NOT NULL | — | FK → creatures.creature_id |  |
| `met_in_rabble` | UUID | NULL | — | FK → swarm_events.swarm_id |  |
| `status` | TEXT | NOT NULL | 'pending' | CHECK(status IN ('pending', 'accepted', 'declined', 'blocked')) | Lifecycle status (see CHECK constraint for allowed values). |
| `met_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

**Table-level constraints:**
- `UNIQUE(creature_a, creature_b)`
- `CHECK (creature_a < creature_b)`

### `creature_invites`

**Purpose:** Pending invites between creatures.

**Created in:** `090_social_layer.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `from_creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id |  |
| `to_creature_id` | UUID | NOT NULL | — | FK → creatures.creature_id |  |
| `rabble_id` | UUID | NOT NULL | — | FK → swarm_events.swarm_id |  |
| `status` | TEXT | NOT NULL | 'pending' | CHECK(status IN ('pending', 'accepted', 'declined', 'expired')) | Lifecycle status (see CHECK constraint for allowed values). |
| `message` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `responded_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `expires_at` | TIMESTAMPTZ | NOT NULL | (NOW() + INTERVAL '24 hours') | — | Expiry timestamp; row becomes invalid after this. |

### `activity_events`

**Purpose:** Activity-feed events surfaced to users.

**Created in:** `090_social_layer.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `actor_user_id` | TEXT | NOT NULL | — | — |  |
| `actor_creature_id` | UUID | NULL | — | FK → creatures.creature_id |  |
| `rabble_id` | UUID | NULL | — | FK → swarm_events.swarm_id |  |
| `target_creature_id` | UUID | NULL | — | FK → creatures.creature_id |  |
| `event_type` | TEXT | NOT NULL | — | CHECK(event_type IN ( 'creature_minted', 'creature_perched', 'creature_flew', 'creatu…) |  |
| `title` | TEXT | NOT NULL | — | — | Title string. |
| `body` | TEXT | NULL | — | — |  |
| `metadata` | JSONB | NULL | '{}' | — | Free-form JSONB metadata blob. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `notifications`

**Purpose:** Per-user in-app notifications.

**Created in:** `021_notifications.sql`
**Modified in:** `092_fix_social_layer.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `user_id` | TEXT | NOT NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `type` | TEXT | NOT NULL | — | — | Discriminator / subtype tag. |
| `title` | TEXT | NOT NULL | — | — | Title string. |
| `message` | TEXT | NULL | — | — |  |
| `read` | BOOLEAN | NOT NULL | FALSE | — | Boolean flag. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `contacts`

**Purpose:** User-defined contacts.

**Created in:** `055_contacts.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `user_id` | TEXT | NOT NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `contact_id` | TEXT | NOT NULL | — | — |  |
| `nickname` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

**Table-level constraints:**
- `UNIQUE(user_id, contact_id)`

### `user_blocks`

**Purpose:** User-level blocks for moderation.

**Created in:** `097_governance.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `blocker_user_id` | TEXT | NOT NULL | — | — |  |
| `blocked_user_id` | TEXT | NOT NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

**Table-level constraints:**
- `UNIQUE(blocker_user_id, blocked_user_id)`
- `CHECK(blocker_user_id != blocked_user_id)`

### `reports`

**Purpose:** Abuse / moderation reports.

**Created in:** `097_governance.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `reporter_user_id` | TEXT | NOT NULL | — | — |  |
| `target_id` | TEXT | NOT NULL | — | — |  |
| `report_type` | TEXT | NOT NULL | — | — |  |
| `target_type` | TEXT | NOT NULL | — | — |  |
| `reason` | TEXT | NOT NULL | — | — |  |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `context` | JSONB | NULL | '{}' | — | JSONB blob. |
| `status` | TEXT | NOT NULL | 'pending' | — | Lifecycle status (see CHECK constraint for allowed values). |
| `reviewed_by` | TEXT | NULL | — | — |  |
| `review_notes` | TEXT | NULL | — | — |  |
| `action_taken` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `reviewed_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |

**Table-level constraints:**
- `CHECK(report_type IN ('creature', 'message', 'user', 'rabble'))`
- `CHECK(reason IN ('inappropriate_content', 'harassment', 'spam', 'impersonation', 'other'))`
- `CHECK(status IN ('pending', 'reviewed', 'action_taken', 'dismissed'))`

## Domain 10: Forecasting & Calibration

_Fermi notebooks, portfolios, forecasts, forecast updates, portfolio-forecast joins, market observations and scheduling._

### `fermi_notebooks`

**Purpose:** Fermi-style estimation notebooks.

**Created in:** `048_fermi_notebooks.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | TEXT | NOT NULL | gen_random_uuid()::text | PK | Surrogate primary key. |
| `owner_id` | TEXT | NOT NULL | — | FK → users.user_id | Owning user/team identifier. |
| `team_id` | UUID | NULL | — | FK → teams.id | Owning team / workspace (→ teams.id). |
| `org_id` | TEXT | NULL | — | — |  |
| `title` | TEXT | NOT NULL | — | — | Title string. |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `visibility` | TEXT | NOT NULL | 'private' | CHECK(visibility IN ('private', 'shared', 'public')) | Visibility scope (public / private / contacts etc). |
| `cells` | JSONB | NOT NULL | '[]'::jsonb | — | JSONB blob. |
| `execution_state` | TEXT | NULL | 'idle' | CHECK(execution_state IN ('idle', 'running', 'complete', 'error')) |  |
| `fpl_source` | TEXT | NULL | — | — |  |
| `last_executed_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `fermi_portfolios`

**Purpose:** Portfolios of forecasts owned by a user.

**Created in:** `048_fermi_notebooks.sql`
**Modified in:** `107_fermi_tables_catchup.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | TEXT | NOT NULL | gen_random_uuid()::text | PK | Surrogate primary key. |
| `owner_id` | TEXT | NOT NULL | — | FK → users.user_id | Owning user/team identifier. |
| `team_id` | UUID | NULL | — | FK → teams.id | Owning team / workspace (→ teams.id). |
| `org_id` | TEXT | NULL | — | — |  |
| `title` | TEXT | NOT NULL | — | — | Title string. |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `visibility` | TEXT | NOT NULL | 'private' | CHECK(visibility IN ('private', 'shared', 'public')) | Visibility scope (public / private / contacts etc). |
| `notebook_ids` | TEXT[] | NOT NULL | ARRAY[]::TEXT[] | — |  |
| `metadata` | JSONB | NOT NULL | '{}'::jsonb | — | Free-form JSONB metadata blob. |
| `domain` | TEXT | NULL | — | — |  |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `fermi_forecasts`

**Purpose:** Individual forecasts with probability/value estimates.

**Created in:** `048_fermi_notebooks.sql`
**Modified in:** `107_fermi_tables_catchup.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | TEXT | NOT NULL | gen_random_uuid()::text | PK | Surrogate primary key. |
| `notebook_id` | TEXT | NOT NULL | — | FK → fermi_notebooks.id | Owning Fermi notebook (→ fermi_notebooks.id). |
| `owner_id` | TEXT | NOT NULL | — | FK → users.user_id | Owning user/team identifier. |
| `team_id` | UUID | NULL | — | FK → teams.id | Owning team / workspace (→ teams.id). |
| `question_text` | TEXT | NOT NULL | — | — |  |
| `predicted_probability` | REAL | NOT NULL | — | CHECK(predicted_probability >= 0 AND predicted_probability <= 1) |  |
| `confidence_interval_low` | REAL | NULL | — | CHECK(confidence_interval_low >= 0 AND confidence_interval_low <= 1) |  |
| `confidence_interval_high` | REAL | NULL | — | CHECK(confidence_interval_high >= 0 AND confidence_interval_high <= 1) |  |
| `actual_outcome` | BOOLEAN | NULL | — | — | Boolean flag. |
| `brier_score` | REAL | NULL | — | — |  |
| `metadata` | JSONB | NOT NULL | '{}'::jsonb | — | Free-form JSONB metadata blob. |
| `domain` | TEXT | NULL | — | — |  |
| `resolution_criteria` | TEXT | NULL | — | — |  |
| `fpl_source` | TEXT | NULL | — | — |  |
| `simulation_results` | JSONB | NULL | — | — | JSONB blob. |
| `iterations` | INTEGER | NULL | 10000 | — |  |
| `drivers` | JSONB | NOT NULL | '[]'::jsonb | — | JSONB blob. |
| `evidence` | JSONB | NOT NULL | '[]'::jsonb | — | JSONB blob. |
| `agents_used` | JSONB | NOT NULL | '[]'::jsonb | — | JSONB blob. |
| `status` | TEXT | NOT NULL | 'draft' | CHECK(status IN ('draft', 'active', 'resolved', 'voided')) | Lifecycle status (see CHECK constraint for allowed values). |
| `resolved_by` | TEXT | NULL | — | — |  |
| `resolution_notes` | TEXT | NULL | — | — |  |
| `visibility` | TEXT | NOT NULL | 'private' | CHECK(visibility IN ('private', 'shared', 'public')) | Visibility scope (public / private / contacts etc). |
| `tags` | TEXT[] | NOT NULL | ARRAY[]::TEXT[] | — | Free-form string tags array. |
| `resolution_date` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `resolved_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `target_date` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `fermi_forecast_updates`

**Purpose:** Append-only updates to a forecast.

**Created in:** `094_fermi_forecasting.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | TEXT | NOT NULL | gen_random_uuid()::text | PK | Surrogate primary key. |
| `forecast_id` | TEXT | NOT NULL | — | FK → fermi_forecasts.id | Owning forecast (→ fermi_forecasts.id). |
| `agent_id` | TEXT | NULL | — | — | Owning/related agent (→ agents.agent_id). |
| `previous_probability` | REAL | NOT NULL | — | — |  |
| `new_probability` | REAL | NOT NULL | — | — |  |
| `reason` | TEXT | NULL | — | — |  |
| `evidence_added` | JSONB | NULL | — | — | JSONB blob. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `fermi_portfolio_forecasts`

**Purpose:** Many-to-many join: portfolios ↔ forecasts.

**Created in:** `048_fermi_notebooks.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `portfolio_id` | TEXT | NOT NULL | — | FK → fermi_portfolios.id | Owning Fermi portfolio (→ fermi_portfolios.id). |
| `forecast_id` | TEXT | NOT NULL | — | FK → fermi_forecasts.id | Owning forecast (→ fermi_forecasts.id). |
| `added_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |

**Table-level constraints:**
- `PRIMARY KEY (portfolio_id, forecast_id)`

### `fermi_market_observations`

**Purpose:** Observed market prices (e.g. Polymarket) for calibration.

**Created in:** `099_polymarket_observations.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | TEXT | NOT NULL | gen_random_uuid()::text | PK | Surrogate primary key. |
| `forecast_id` | TEXT | NULL | — | FK → fermi_forecasts.id | Owning forecast (→ fermi_forecasts.id). |
| `pm_event_id` | TEXT | NOT NULL | — | — |  |
| `pm_market_id` | TEXT | NOT NULL | — | — |  |
| `pm_condition_id` | TEXT | NULL | — | — |  |
| `observer_id` | TEXT | NOT NULL | — | FK → users.user_id |  |
| `pm_slug` | TEXT | NULL | — | — |  |
| `pm_question` | TEXT | NOT NULL | — | — |  |
| `pm_event_title` | TEXT | NULL | — | — |  |
| `market_price` | REAL | NOT NULL | — | CHECK(market_price >= 0 AND market_price <= 1) |  |
| `bid_price` | REAL | NULL | — | CHECK(bid_price IS NULL OR (bid_price >= 0 AND bid_price <= 1)) |  |
| `ask_price` | REAL | NULL | — | CHECK(ask_price IS NULL OR (ask_price >= 0 AND ask_price <= 1)) |  |
| `midpoint_price` | REAL | NULL | — | CHECK(midpoint_price IS NULL OR (midpoint_price >= 0 AND midpoint_price <= 1)) |  |
| `spread` | REAL | NULL | — | — |  |
| `volume_total` | REAL | NULL | — | — |  |
| `volume_24h` | REAL | NULL | — | — |  |
| `liquidity` | REAL | NULL | — | — |  |
| `price_change_1h` | REAL | NULL | — | — |  |
| `price_change_1d` | REAL | NULL | — | — |  |
| `price_change_1w` | REAL | NULL | — | — |  |
| `price_change_1m` | REAL | NULL | — | — |  |
| `pm_active` | BOOLEAN | NOT NULL | true | — | Boolean flag. |
| `pm_closed` | BOOLEAN | NOT NULL | false | — | Boolean flag. |
| `pm_resolved` | BOOLEAN | NOT NULL | false | — | Boolean flag. |
| `pm_outcome` | TEXT | NULL | — | — |  |
| `fermi_probability` | REAL | NULL | — | CHECK(fermi_probability IS NULL OR (fermi_probability >= 0 AND fermi_probability <= 1)) |  |
| `divergence_pp` | REAL | NULL | — | — |  |
| `confidence_signal` | TEXT | NULL | — | CHECK(confidence_signal IS NULL OR confidence_signal IN ( 'very_high', 'high', 'mediu…) |  |
| `observation_type` | TEXT | NOT NULL | 'search' | CHECK(observation_type IN ( 'search', 'import', 'manual_link', 'refresh', 'scheduled'…) |  |
| `tags` | TEXT[] | NOT NULL | ARRAY[]::TEXT[] | — | Free-form string tags array. |
| `metadata` | JSONB | NOT NULL | '{}'::jsonb | — | Free-form JSONB metadata blob. |
| `pm_end_date` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |

### `fermi_forecast_schedules`

**Purpose:** Scheduled re-forecasting cadences.

**Created in:** `109_forecast_agent_schedules.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `forecast_id` | TEXT | NOT NULL | — | — | Owning forecast (→ fermi_forecasts.id). |
| `agent_id` | TEXT | NOT NULL | — | — | Owning/related agent (→ agents.agent_id). |
| `driver_name` | TEXT | NOT NULL | — | — |  |
| `query` | TEXT | NOT NULL | — | — |  |
| `interval_hours` | INTEGER | NOT NULL | 24 | — |  |
| `enabled` | BOOLEAN | NOT NULL | true | — | Boolean flag. |
| `last_run_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `next_run_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

## Domain 11: Apps & Sessions

_App registry and Xaman Ek (navigator) sessions._

### `apps`

**Purpose:** Registered first/third-party apps that can act inside the platform.

**Created in:** `116_apps.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `id` | UUID | NOT NULL | gen_random_uuid() | PK | Surrogate primary key. |
| `owner_user_id` | TEXT | NOT NULL | — | — |  |
| `owner_team_id` | UUID | NULL | — | FK → teams.id |  |
| `slug` | TEXT | NOT NULL | — | UK; CHECK(slug ~ '^[a-z][a-z0-9_]{2,63}$') | URL-safe identifier. |
| `name` | TEXT | NOT NULL | — | — | Human-readable name. |
| `tagline` | TEXT | NULL | — | — |  |
| `homepage_url` | TEXT | NULL | — | — |  |
| `icon_url` | TEXT | NULL | — | — |  |
| `composition_slug` | TEXT | NULL | — | — |  |
| `schema_slug` | TEXT | NULL | — | — |  |
| `schema_json` | JSONB | NULL | — | — | JSONB blob. |
| `workspace_template` | JSONB | NOT NULL | '{}'::jsonb | — | JSONB blob. |
| `revenue_share` | JSONB | NULL | NULL | — | JSONB blob. |
| `pricing_policy` | TEXT | NOT NULL | 'platform_default' | CHECK(pricing_policy IN ( 'platform_default', 'subscription', 'metered', 'free' )) |  |
| `visibility` | TEXT | NOT NULL | 'private' | CHECK(visibility IN ('private', 'unlisted', 'public')) | Visibility scope (public / private / contacts etc). |
| `description` | TEXT | NULL | — | — | Free-text description. |
| `metadata` | JSONB | NOT NULL | '{}'::jsonb | — | Free-form JSONB metadata blob. |
| `published_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `archived_at` | TIMESTAMPTZ | NULL | — | — | Timestamp. |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `updated_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last-mutation timestamp (auto-set by trigger where present). |

### `xaman_sessions`

**Purpose:** Xaman Ek navigator session state.

**Created in:** `115_xaman_sessions.sql`

| Column | Type | Null | Default | Constraints | Description |
|---|---|---|---|---|---|
| `session_id` | UUID | NOT NULL | gen_random_uuid() | PK | Owning session. |
| `user_id` | TEXT | NOT NULL | — | — | Owning user (→ users.user_id when textual; otherwise FK noted in Constraints). |
| `session_type` | TEXT | NOT NULL | 'free' | CHECK(session_type IN ( 'agent_design', 'composition_design', 'workspace_help', 'free…) |  |
| `title` | TEXT | NULL | — | — | Title string. |
| `in_progress` | JSONB | NOT NULL | '{}'::jsonb | — | JSONB blob. |
| `messages` | JSONB | NOT NULL | '[]'::jsonb | — | JSONB blob. |
| `page_context` | TEXT | NULL | — | — |  |
| `status` | TEXT | NOT NULL | 'active' | CHECK(status IN ('active', 'completed', 'abandoned')) | Lifecycle status (see CHECK constraint for allowed values). |
| `created_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Row creation timestamp. |
| `last_active_at` | TIMESTAMPTZ | NOT NULL | NOW() | — | Last time entity was active. |

## Appendix A: Alphabetical Table Index

- [`activity_events`](#activity-events) — *Social*
- [`agent_alignments`](#agent-alignments) — *Agents & Memory*
- [`agent_avatars`](#agent-avatars) — *Agents & Memory*
- [`agent_episode_payouts`](#agent-episode-payouts) — *Agents & Memory*
- [`agent_interaction_policies`](#agent-interaction-policies) — *Agents & Memory*
- [`agent_observability_state`](#agent-observability-state) — *Agents & Memory*
- [`agent_timeline_entries`](#agent-timeline-entries) — *Agents & Memory*
- [`agent_versions`](#agent-versions) — *Agents & Memory*
- [`agents`](#agents) — *Agents & Memory*
- [`anomaly_events`](#anomaly-events) — *Agents & Memory*
- [`api_keys`](#api-keys) — *Users & Auth*
- [`apps`](#apps) — *Apps & Sessions*
- [`ar_beacons`](#ar-beacons) — *Spatial / AR / Telemetry*
- [`ar_choreographies`](#ar-choreographies) — *Spatial / AR / Telemetry*
- [`ar_grid_maps`](#ar-grid-maps) — *Spatial / AR / Telemetry*
- [`coherence_evaluations`](#coherence-evaluations) — *Workspaces & Compositions*
- [`communities`](#communities) — *Agents & Memory*
- [`composition_versions`](#composition-versions) — *Workspaces & Compositions*
- [`consolidation_jobs`](#consolidation-jobs) — *Agents & Memory*
- [`consolidation_locks`](#consolidation-locks) — *Agents & Memory*
- [`contacts`](#contacts) — *Social*
- [`creature_animation_layers`](#creature-animation-layers) — *Rabble Core*
- [`creature_blocks`](#creature-blocks) — *Rabble Core*
- [`creature_collections`](#creature-collections) — *Rabble Core*
- [`creature_conditions`](#creature-conditions) — *Rabble Core*
- [`creature_devices`](#creature-devices) — *Rabble Core*
- [`creature_favourites`](#creature-favourites) — *Rabble Core*
- [`creature_flights`](#creature-flights) — *Rabble Core*
- [`creature_friendships`](#creature-friendships) — *Social*
- [`creature_images`](#creature-images) — *Rabble Core*
- [`creature_invites`](#creature-invites) — *Social*
- [`creature_state`](#creature-state) — *Rabble Core*
- [`creature_tethers`](#creature-tethers) — *Rabble Core*
- [`creature_versions`](#creature-versions) — *Rabble Core*
- [`creatures`](#creatures) — *Rabble Core*
- [`credit_ledger`](#credit-ledger) — *Wallet & Billing*
- [`dyad_state`](#dyad-state) — *Agents & Memory*
- [`entities`](#entities) — *Agents & Memory*
- [`episode_corrections`](#episode-corrections) — *Agents & Memory*
- [`episodes`](#episodes) — *Agents & Memory*
- [`eval_runs`](#eval-runs) — *Eval & Observability*
- [`eval_signals`](#eval-signals) — *Eval & Observability*
- [`eval_test_cases`](#eval-test-cases) — *Eval & Observability*
- [`facts`](#facts) — *Agents & Memory*
- [`fermi_forecast_schedules`](#fermi-forecast-schedules) — *Forecasting & Calibration*
- [`fermi_forecast_updates`](#fermi-forecast-updates) — *Forecasting & Calibration*
- [`fermi_forecasts`](#fermi-forecasts) — *Forecasting & Calibration*
- [`fermi_market_observations`](#fermi-market-observations) — *Forecasting & Calibration*
- [`fermi_notebooks`](#fermi-notebooks) — *Forecasting & Calibration*
- [`fermi_portfolio_forecasts`](#fermi-portfolio-forecasts) — *Forecasting & Calibration*
- [`fermi_portfolios`](#fermi-portfolios) — *Forecasting & Calibration*
- [`flight_telemetry`](#flight-telemetry) — *Rabble Core*
- [`hitl_actions`](#hitl-actions) — *Agents & Memory*
- [`knowledge_transfers`](#knowledge-transfers) — *Agents & Memory*
- [`marketplace_listings`](#marketplace-listings) — *Marketplace*
- [`marketplace_transactions`](#marketplace-transactions) — *Marketplace*
- [`notifications`](#notifications) — *Social*
- [`object_shares`](#object-shares) — *Workspaces & Compositions*
- [`observation_sessions`](#observation-sessions) — *Spatial / AR / Telemetry*
- [`ontology_snapshots`](#ontology-snapshots) — *Agents & Memory*
- [`pairwise_coherence`](#pairwise-coherence) — *Agents & Memory*
- [`push_config`](#push-config) — *Users & Auth*
- [`push_subscriptions`](#push-subscriptions) — *Users & Auth*
- [`rabble_co_presence`](#rabble-co-presence) — *Rabble Core*
- [`rabble_ejections`](#rabble-ejections) — *Rabble Core*
- [`rabble_follows`](#rabble-follows) — *Rabble Core*
- [`rabble_messages`](#rabble-messages) — *Rabble Core*
- [`reports`](#reports) — *Social*
- [`saved_locations`](#saved-locations) — *Spatial / AR / Telemetry*
- [`secret_access_log`](#secret-access-log) — *Users & Auth*
- [`semantic_rules`](#semantic-rules) — *Agents & Memory*
- [`shopping_profiles`](#shopping-profiles) — *Marketplace*
- [`siwe_nonces`](#siwe-nonces) — *Users & Auth*
- [`sosa_observations`](#sosa-observations) — *Spatial / AR / Telemetry*
- [`sosa_platforms`](#sosa-platforms) — *Spatial / AR / Telemetry*
- [`sosa_sensors`](#sosa-sensors) — *Spatial / AR / Telemetry*
- [`swarm_activations`](#swarm-activations) — *Rabble Core*
- [`swarm_algorithms`](#swarm-algorithms) — *Rabble Core*
- [`swarm_events`](#swarm-events) — *Rabble Core*
- [`swarm_participants`](#swarm-participants) — *Rabble Core*
- [`swarm_sessions`](#swarm-sessions) — *Rabble Core*
- [`swarm_sub_flocks`](#swarm-sub-flocks) — *Rabble Core*
- [`swarm_telemetry`](#swarm-telemetry) — *Rabble Core*
- [`team_members`](#team-members) — *Workspaces & Compositions*
- [`teams`](#teams) — *Workspaces & Compositions*
- [`telemetry_points`](#telemetry-points) — *Rabble Core*
- [`two_reviewer_requests`](#two-reviewer-requests) — *Eval & Observability*
- [`user_blocks`](#user-blocks) — *Social*
- [`user_secrets`](#user-secrets) — *Users & Auth*
- [`users`](#users) — *Users & Auth*
- [`voice_assets`](#voice-assets) — *Spatial / AR / Telemetry*
- [`waitlist`](#waitlist) — *Users & Auth*
- [`wallets`](#wallets) — *Wallet & Billing*
- [`workspace_agents`](#workspace-agents) — *Workspaces & Compositions*
- [`workspace_messages`](#workspace-messages) — *Workspaces & Compositions*
- [`xaman_sessions`](#xaman-sessions) — *Apps & Sessions*

## Appendix B: Enums and CHECK Constraints

Below lists every column-level `CHECK (... IN (...))` constraint discovered, i.e. enum-like fields.

| Table | Column | CHECK |
|---|---|---|
| `activity_events` | `event_type` | `event_type IN ( 'creature_minted', 'creature_perched', 'creature_flew', 'creature_landed', 'rabble_created', 'rabble_joined', 'rabble_left', 'rabble_completed', 'friendship_requested', 'friendship_accepted', 'creature_invited', 'creature_i…` |
| `agents` | `min_tier` | `min_tier IN ('free', 'standard', 'premium')` |
| `agents` | `status` | `status IN ('draft', 'published', 'archived')` |
| `agents` | `visibility` | `visibility IN ('private', 'unlisted', 'public')` |
| `anomaly_events` | `kind` | `kind IN ('drift', 'rolling_conflict', 'rupture', 'safety')` |
| `anomaly_events` | `severity` | `severity IN ('info', 'warning', 'critical')` |
| `apps` | `pricing_policy` | `pricing_policy IN ( 'platform_default', 'subscription', 'metered', 'free' )` |
| `apps` | `visibility` | `visibility IN ('private', 'unlisted', 'public')` |
| `creature_conditions` | `cognition_tier` | `cognition_tier IN ('free', 'standard', 'premium')` |
| `creature_conditions` | `visibility` | `visibility IN ('public', 'contacts_only', 'private')` |
| `creature_friendships` | `status` | `status IN ('pending', 'accepted', 'declined', 'blocked')` |
| `creature_invites` | `status` | `status IN ('pending', 'accepted', 'declined', 'expired')` |
| `creature_state` | `state` | `state IN ('perch_solo', 'fly', 'perch_rabble')` |
| `creature_versions` | `state` | `state IN ('perch_solo', 'fly', 'perch_rabble')` |
| `credit_ledger` | `tx_type` | `tx_type IN ( 'deposit', 'withdrawal', 'execution_fee', 'gas_fee', 'education_alloc', 'education_spend', 'transfer_out', 'transfer_in', 'grant', 'refund' )` |
| `episode_corrections` | `classification` | `classification IS NULL OR classification IN ('belief', 'behaviour')` |
| `episode_corrections` | `reviewer_action` | `reviewer_action IN ('approve', 'relabel', 'intervene')` |
| `episode_corrections` | `scope` | `scope IN ('episode', 'dyad', 'agent_wide')` |
| `eval_runs` | `status` | `status IN ('running', 'completed', 'failed')` |
| `eval_signals` | `evaluator_tier` | `evaluator_tier IN ('pre_filter', 'dimensional')` |
| `fermi_forecasts` | `status` | `status IN ('draft', 'active', 'resolved', 'voided')` |
| `fermi_forecasts` | `visibility` | `visibility IN ('private', 'shared', 'public')` |
| `fermi_market_observations` | `confidence_signal` | `confidence_signal IS NULL OR confidence_signal IN ( 'very_high', 'high', 'medium', 'low' )` |
| `fermi_market_observations` | `observation_type` | `observation_type IN ( 'search', 'import', 'manual_link', 'refresh', 'scheduled', 'agent_research', 'resolution_check' )` |
| `fermi_notebooks` | `execution_state` | `execution_state IN ('idle', 'running', 'complete', 'error')` |
| `fermi_notebooks` | `visibility` | `visibility IN ('private', 'shared', 'public')` |
| `fermi_portfolios` | `visibility` | `visibility IN ('private', 'shared', 'public')` |
| `hitl_actions` | `action` | `action IN ('approve', 'relabel', 'intervene')` |
| `marketplace_listings` | `status` | `status IN ('active', 'paused', 'delisted')` |
| `object_shares` | `object_type` | `object_type IN ( 'agent', 'capability', 'forecast', 'index', 'repo', 'file' )` |
| `object_shares` | `permission` | `permission IN ('view', 'edit', 'admin')` |
| `object_shares` | `share_type` | `share_type IN ('team', 'user')` |
| `secret_access_log` | `action` | `action IN ('read', 'used', 'created', 'updated', 'deleted')` |
| `swarm_participants` | `role` | `role IN ('host', 'cohost', 'member')` |
| `swarm_participants` | `status` | `status IN ('active', 'left', 'kicked')` |
| `team_members` | `member_type` | `member_type IN ('user', 'agent')` |
| `team_members` | `role` | `role IN ('owner', 'admin', 'member', 'viewer')` |
| `two_reviewer_requests` | `status` | `status IN ('pending', 'approved', 'rejected', 'expired')` |
| `users` | `auth_provider` | `auth_provider IN ('email', 'github', 'google', 'ethereum', 'legacy')` |
| `users` | `role` | `role IN ('admin', 'developer', 'viewer')` |
| `users` | `social_visibility` | `social_visibility IN ('public', 'creature-only', 'private')` |
| `wallets` | `owner_type` | `owner_type IN ('user', 'workspace')` |
| `workspace_agents` | `relationship` | `relationship IN ('hired', 'owned', 'created_here')` |
| `workspace_messages` | `message_type` | `message_type IN ('chat', 'execution_result', 'coherence_update', 'system_event')` |
| `workspace_messages` | `sender_type` | `sender_type IN ('user', 'agent', 'system')` |
| `xaman_sessions` | `session_type` | `session_type IN ( 'agent_design', 'composition_design', 'workspace_help', 'free' )` |
| `xaman_sessions` | `status` | `status IN ('active', 'completed', 'abandoned')` |

## Appendix C: Postgres Functions

Names of every `CREATE FUNCTION` / `CREATE OR REPLACE FUNCTION` encountered across migrations, in creation order:

- `update_updated_at_column()`
- `cleanup_expired_siwe_nonces()`
- `auto_add_team_owner()`
- `get_my_rabbles_with_status()`
- `get_nearby_rabbles()`
- `get_creatures_with_deployment()`
- `check_boundary_violations()`
- `canonical_creature_pair()`
- `get_creatures_met_in_rabble()`
- `get_pending_friendship_requests()`
- `get_creature_friends()`
- `get_pending_creature_invites()`
- `get_activity_feed()`
- `expire_old_creature_invites()`
- `compute_brier_score()`
- `resolve_forecast()`
- `refresh_fermi_leaderboard()`
- `get_followed_rabbles()`
- `get_rabble_followers()`
- `get_saved_locations()`
- `get_nearby_creatures()`
- `is_blocked()`
- `is_user_blocked()`
- `is_ejected()`
- `get_user_blocks()`
- `episode_corrections_immutable()`
- `bump_agent_persona_version()`
- `hitl_actions_immutable()`
- `touch_two_reviewer_requests()`
- `touch_apps_updated_at()`

## Appendix D: Migrations That Don't Create Tables

Migrations that only ALTER/fix/backfill — no new tables — but tracked for completeness:

- `004b_migrate_users_for_auth.sql`
- `006_add_user_id_to_agents.sql`
- `007_add_user_id_to_memory.sql`
- `011_agent_crud_and_education.sql`
- `013_workspace_fields.sql`
- `017_workspace_git.sql`
- `018_agent_aliases.sql`
- `019_agent_provider_fields.sql`
- `020_stripe_and_profile.sql`
- `022_sample_queries.sql`
- `025_agent_lifecycle.sql`
- `026_fork_royalty_tx_type.sql`
- `028_episode_tags.sql`
- `029_fix_message_type_and_profile.sql`
- `031_waitlist_status.sql`
- `032_fix_tx_type_constraint.sql`
- `033_backfill_team_owners.sql`
- `034_xaman_ek_system_ontology.sql`
- `035_fix_tx_type_constraint.sql`
- `036_workspace_workflow.sql`
- `037_agent_valence_and_workflow_template.sql`
- `038_prompt_template.sql`
- `040_agent_requires_secrets.sql`
- `043_seed_starter_creatures.sql`
- `045_rabble_funding.sql`
- `046_rabble_visibility.sql`
- `047_flight_path_samples.sql`
- `050_fix_tx_type_constraint_rabble.sql`
- `054_creature_management.sql`
- `058_creature_presence.sql`
- `059_agent_wallet_admin.sql`
- `060_fix_object_shares_rabble.sql`
- `062_anchor_creature.sql`
- `065_creature_visibility.sql`
- `066_wallet_balance_split.sql`
- `067_flight_environment.sql`
- `068_flight_data_source.sql`
- `069_one_active_flight.sql`
- `070_cleanup_stale_flights.sql`
- `071_add_flight_plan_tx_type.sql`
- `072_perch_model.sql`
- `073_walk_in_budget.sql`
- `075_fix_tx_type_constraint.sql`
- `076_drop_tx_type_constraint.sql`
- `077_expand_message_type_constraint.sql`
- `079_conditions_presence.sql`
- `080_drop_redundant_creature_columns.sql`
- `081_fix_visibility_contacts.sql`
- `082_rabble_radius.sql`
- `083_genome_profile_cache.sql`
- `084_drop_creature_state_rabble_fk.sql`
- `085_rename_creature_states.sql`
- `086_creature_flights_metadata.sql`
- `088_backfill_creature_versions.sql`
- `089_dashboard_spatial_queries.sql`
- `093_users_user_id_unique.sql`
- `096_performance_indexes.sql`
- `100_cognition_tier.sql`
- `101_model_ladder.sql`
- `102_cognition_tier_nullable.sql`
- `104_cep_kg_columns.sql`
- `105_cep_fermi_contract.sql`
- `106_model_params.sql`
- `107_fermi_tables_catchup.sql`
- `110_unassign_curated_agents.sql`
- `111_restore_admin_ownership_of_curated.sql`
- `112_workspace_origin.sql`
- `114_agent_valence_column.sql`
- `117_agent_output_contract.sql`
- `118_object_type_workspace.sql`
- `119_teams_mission_defensive.sql`
- `120_composition_versions_rejection.sql`
- `121_fix_xaman_ek_ontology_mermaid.sql`

---

_End of data dictionary._
