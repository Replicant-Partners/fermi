//! Gas fee module — every A2A transaction has a cost.
//!
//! Two-layer economic model:
//!   Layer 1 (Credits): Platform gas — users buy credits, spend on actions. The product.
//!   Layer 2 (Crypto):  Agent-to-owner royalties — % tx fee on token transfers. Future.
//!
//! Gas fees fund the platform. Configurable via env vars with sensible defaults.

use fermi_auth::{credit_charge, credit_deposit, credit_deposit_typed, get_or_create_wallet};
use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Layer 1: Credit gas fee schedule (the product)
#[derive(Debug, Clone)]
pub struct GasFees {
    pub message_send: i32,
    pub agent_hire: i32,
    pub agent_add: i32,
    pub execution_min: i32,
    pub execution_gas_pct: f64,
    /// Fraction of the execution fee that flows to a community/curated
    /// agent's owner on hire. Applied only when the agent has an
    /// `owner_id` AND the caller is not the owner AND the tier is not
    /// `system` (platform-funded agents get no royalty). Expressed as
    /// a fraction (e.g. 0.85 = 85% to owner, 15% to platform). Applies
    /// to the `base` execution fee only — the gas surcharge stays
    /// with the platform since it covers infrastructure cost. See
    /// `charge_execution_with_royalty` for the atomic charge-and-
    /// route flow. v0.10.1.
    pub execution_owner_royalty_pct: f64,
    pub consolidation_cycle: i32,
    pub file_write: i32,
    pub avatar_generate: i32,
    pub embedding_import: i32,
    pub fork_base: i32,
    pub publish_fee: i32,
    pub eval_run: i32,
    pub file_upload_per_mb: i32,
    pub marketplace_listing_fee: i32,
    pub marketplace_match_base: i32,
    pub marketplace_platform_pct: f64,
    pub rabble_chat: i32,
    pub creature_mint: i32,
    pub creature_art: i32,
    pub voice_synthesis: i32,
    pub swarm_session_create: i32,
    pub swarm_telemetry_ingest: i32,
    pub observation_session_create: i32,
    pub observation_ingest: i32,
    pub formation_activate: i32,
    pub creature_animate: i32,
    pub flight_plan: i32,
    pub enemy_sensor_enable: i32,
    pub enemy_sensor_check: i32,
    pub genome_profiler_enable: i32,
    pub genome_profiler_check: i32,
    pub prey_locator_enable: i32,
    pub prey_locator_scan: i32,
    pub prey_locator_stalk: i32,
    pub host_rabble: i32,
    pub creature_dream: i32,
    /// Platform infrastructure read fee — charged when users read agent-produced data
    /// (visualization, history, projections). Agents don't get paid (they already learned).
    pub platform_read: i32,
    /// Layer 2: Platform transaction fee on crypto token transfers (agent→owner royalties).
    /// Expressed as a fraction (e.g. 0.025 = 2.5%). Applied to every token payout.
    /// Not yet wired — requires SIWE wallet connection + settlement layer.
    pub crypto_tx_fee_pct: f64,
}

impl GasFees {
    pub fn from_env() -> Self {
        Self {
            message_send: env_or("GAS_MESSAGE_SEND", 1),
            agent_hire: env_or("GAS_AGENT_HIRE", 5),
            agent_add: env_or("GAS_AGENT_ADD", 2),
            execution_min: env_or("GAS_EXECUTION_MIN", 1),
            execution_gas_pct: std::env::var("GAS_EXECUTION_PCT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.10),
            execution_owner_royalty_pct: std::env::var("GAS_EXECUTION_OWNER_ROYALTY_PCT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.85),
            consolidation_cycle: env_or("GAS_CONSOLIDATION", 3),
            file_write: env_or("GAS_FILE_WRITE", 1),
            avatar_generate: env_or("GAS_AVATAR_GENERATE", 3),
            embedding_import: env_or("GAS_EMBEDDING_IMPORT", 5),
            fork_base: env_or("GAS_FORK_BASE", 2),
            publish_fee: env_or("GAS_PUBLISH_FEE", 1),
            eval_run: env_or("GAS_EVAL_RUN", 2),
            file_upload_per_mb: env_or("GAS_FILE_UPLOAD_PER_MB", 1),
            marketplace_listing_fee: env_or("GAS_MARKETPLACE_LISTING", 3),
            marketplace_match_base: env_or("GAS_MARKETPLACE_MATCH", 1),
            marketplace_platform_pct: std::env::var("GAS_MARKETPLACE_PLATFORM_PCT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.15),
            rabble_chat: env_or("GAS_RABBLE_CHAT", 1),
            creature_mint: env_or("GAS_CREATURE_MINT", 3),
            creature_art: env_or("GAS_CREATURE_ART", 5),
            voice_synthesis: env_or("GAS_VOICE_SYNTHESIS", 2),
            swarm_session_create: env_or("GAS_SWARM_SESSION_CREATE", 2),
            swarm_telemetry_ingest: env_or("GAS_SWARM_TELEMETRY_INGEST", 1),
            observation_session_create: env_or("GAS_OBSERVATION_SESSION_CREATE", 2),
            observation_ingest: env_or("GAS_OBSERVATION_INGEST", 1),
            formation_activate: env_or("GAS_FORMATION_ACTIVATE", 3),
            creature_animate: env_or("GAS_CREATURE_ANIMATE", 10),
            flight_plan: env_or("GAS_FLIGHT_PLAN", 5),
            enemy_sensor_enable: env_or("GAS_ENEMY_SENSOR_ENABLE", 5),
            enemy_sensor_check: env_or("GAS_ENEMY_SENSOR_CHECK", 1),
            genome_profiler_enable: env_or("GAS_GENOME_PROFILER_ENABLE", 5),
            genome_profiler_check: env_or("GAS_GENOME_PROFILER_CHECK", 2),
            prey_locator_enable: env_or("GAS_PREY_LOCATOR_ENABLE", 5),
            prey_locator_scan: env_or("GAS_PREY_LOCATOR_SCAN", 2),
            prey_locator_stalk: env_or("GAS_PREY_LOCATOR_STALK", 5),
            host_rabble: env_or("GAS_HOST_RABBLE", 3),
            creature_dream: env_or("GAS_CREATURE_DREAM", 5),
            platform_read: env_or("GAS_PLATFORM_READ", 1),
            crypto_tx_fee_pct: std::env::var("CRYPTO_TX_FEE_PCT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.025),
        }
    }

    /// Calculate file upload fee: base file_write + per-MB surcharge (ceil, min 1 MB)
    pub fn upload_fee(&self, size_bytes: usize) -> i32 {
        let mb_ceil = ((size_bytes as f64) / (1024.0 * 1024.0)).ceil() as i32;
        self.file_write + mb_ceil.max(1) * self.file_upload_per_mb
    }

    /// Calculate execution fee: 1 credit per 1000 tokens (min execution_min) + gas surcharge
    pub fn execution_fee(&self, tokens: i32) -> (i32, i32) {
        let base = std::cmp::max(self.execution_min, tokens / 1000);
        let gas = std::cmp::max(1, (base as f64 * self.execution_gas_pct) as i32);
        (base, gas)
    }
}

impl Default for GasFees {
    fn default() -> Self {
        Self {
            message_send: 1,
            agent_hire: 5,
            agent_add: 2,
            execution_min: 1,
            execution_gas_pct: 0.10,
            execution_owner_royalty_pct: 0.85,
            consolidation_cycle: 3,
            file_write: 1,
            avatar_generate: 3,
            embedding_import: 5,
            fork_base: 2,
            publish_fee: 1,
            eval_run: 2,
            file_upload_per_mb: 1,
            marketplace_listing_fee: 3,
            marketplace_match_base: 1,
            marketplace_platform_pct: 0.15, // 15% platform cut on match payouts
            rabble_chat: 1,
            creature_mint: 3,
            creature_art: 5,
            voice_synthesis: 2,
            swarm_session_create: 2,
            swarm_telemetry_ingest: 1,
            observation_session_create: 2,
            observation_ingest: 1,
            formation_activate: 3,
            creature_animate: 10,
            flight_plan: 5,
            enemy_sensor_enable: 5,
            enemy_sensor_check: 1,
            genome_profiler_enable: 5,
            genome_profiler_check: 2,
            prey_locator_enable: 5,
            prey_locator_scan: 2,
            prey_locator_stalk: 5,
            host_rabble: 3,
            creature_dream: 5,
            platform_read: 1,
            crypto_tx_fee_pct: 0.025, // 2.5% on token transfers
        }
    }
}

fn env_or(key: &str, default: i32) -> i32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Low balance threshold (credits)
pub const LOW_BALANCE_THRESHOLD: i32 = 10;

/// Charge a flat gas fee. Returns amount charged.
pub async fn charge_gas(
    pool: &PgPool,
    wallet_id: Uuid,
    amount: i32,
    tx_type: &str,
    description: &str,
    related_id: Option<&str>,
) -> std::result::Result<i32, (axum::http::StatusCode, String)> {
    credit_charge(pool, wallet_id, amount, tx_type, description, related_id)
        .await
        .map(|_tx| amount)
        .map_err(|e| {
            (
                axum::http::StatusCode::PAYMENT_REQUIRED,
                format!("Gas fee failed: {}", e),
            )
        })
}

/// Charge the caller for an agent execution and route the owner
/// royalty in one call.
///
/// v0.10.1 credit-flow slice — completes the marketplace loop started
/// in v0.9.0–v0.9.2 (agent-owner API key routing + marketplace
/// substrate cleanup). Now that owner-owned agents run on the owner's
/// keys, a portion of the caller's execution fee flows back to the
/// owner as royalty. System-tier agents remain platform-funded (no
/// royalty). Self-hires (caller == owner) skip the wash-transaction.
///
/// Flow:
///   1. Debit `execution_fee` from `caller_wallet_id` (same total the
///      caller has always paid — no price increase to this change).
///   2. If eligible, deposit `execution_fee * royalty_pct` to the
///      owner's user wallet (`user`-typed) with `tx_type="agent_royalty_in"`.
///   3. Platform keeps the remainder (execution_fee minus the royalty).
///
/// **Not atomic.** Same pattern as `charge_and_distribute`: if the
/// deposit fails after the charge succeeded, we log and move on — the
/// platform absorbs the missed royalty for this call. Ledger
/// reconciliation can catch persistent failures. In practice the
/// deposit is a straightforward `credit_deposit_typed` and won't fail
/// unless the DB itself is unhealthy.
///
/// Returns `(execution_fee_charged, royalty_paid_to_owner)`. If the
/// caller lacks credits, the debit's PAYMENT_REQUIRED status
/// propagates via the same shape `charge_gas` returns.
///
/// `royalty_pct` is typically `state.gas_fees.execution_owner_royalty_pct`.
#[allow(clippy::too_many_arguments)]
pub async fn charge_execution_with_royalty(
    pool: &PgPool,
    caller_wallet_id: Uuid,
    caller_user_id: &str,
    execution_fee: i32,
    agent_owner_id: Option<&str>,
    agent_tier: &str,
    agent_id_str: &str,
    tokens: i32,
    episode_id: Option<&str>,
    royalty_pct: f64,
) -> std::result::Result<(i32, i32), (axum::http::StatusCode, String)> {
    // 1. Debit the caller. Same tx_type/description shape as before
    // (`execution_fee`) so existing ledger queries keep working.
    charge_gas(
        pool,
        caller_wallet_id,
        execution_fee,
        "execution_fee",
        &format!("Execute {} ({}tk)", agent_id_str, tokens),
        episode_id,
    )
    .await?;

    // 2. Decide whether a royalty flows. Three gates:
    //    - Agent must have an owner (system agents lack one).
    //    - Tier must not be "system" (platform-funded regardless).
    //    - Caller must not BE the owner (skip self-hire round-trip).
    let owner_id = match agent_owner_id {
        Some(o) if !o.is_empty() => o,
        _ => return Ok((execution_fee, 0)),
    };
    if agent_tier == "system" {
        return Ok((execution_fee, 0));
    }
    if owner_id == caller_user_id {
        return Ok((execution_fee, 0));
    }

    // 3. Compute royalty. Clamped so we always pay at least 1 credit
    // when the split rounds down to zero (avoids owners feeling the
    // marketplace is broken on small executions).
    let royalty_pct = royalty_pct.clamp(0.0, 1.0);
    let raw_amount = (execution_fee as f64 * royalty_pct) as i32;
    let royalty = raw_amount.clamp(1, execution_fee);

    // 4. Deposit to the owner's user wallet.
    let owner_wallet = match get_or_create_wallet(pool, "user", owner_id).await {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(
                owner_id = %owner_id,
                agent_id = %agent_id_str,
                error = %e,
                "[credit-flow] failed to resolve owner wallet — royalty NOT paid (platform absorbs)"
            );
            return Ok((execution_fee, 0));
        }
    };
    if let Err(e) = credit_deposit_typed(
        pool,
        owner_wallet.wallet_id,
        royalty,
        "agent_royalty_in",
        &format!(
            "Royalty from {} — {} ({}tk)",
            caller_user_id, agent_id_str, tokens
        ),
    )
    .await
    {
        tracing::warn!(
            owner_id = %owner_id,
            agent_id = %agent_id_str,
            error = %e,
            "[credit-flow] failed to deposit royalty — platform absorbs"
        );
        return Ok((execution_fee, 0));
    }

    tracing::info!(
        owner_id = %owner_id,
        agent_id = %agent_id_str,
        caller_id = %caller_user_id,
        execution_fee = execution_fee,
        royalty = royalty,
        platform_cut = execution_fee - royalty,
        "[credit-flow] royalty paid"
    );

    Ok((execution_fee, royalty))
}

/// Check wallet balance and return true if low.
///
/// Fails TOWARD warning. The previous implementation chained
/// `.ok().flatten().and_then(…ok()).unwrap_or(false)` — three swallows that
/// all resolved to "balance is fine". A dead database, a missing wallet, or a
/// type mismatch each produced a confident "no problem", which is the worst
/// possible answer for a balance warning: the user finds out when execution
/// starts failing instead.
///
/// An unnecessary low-balance banner is cheap; a suppressed one is not.
pub async fn check_low_balance(pool: &PgPool, wallet_id: Uuid) -> bool {
    match sqlx::query("SELECT balance FROM wallets WHERE wallet_id = $1")
        .bind(wallet_id)
        .fetch_optional(pool)
        .await
    {
        Ok(Some(row)) => match sqlx::Row::try_get::<i32, _>(&row, "balance") {
            Ok(b) => b < LOW_BALANCE_THRESHOLD,
            Err(e) => {
                tracing::error!(%wallet_id, error = %e, "could not decode wallet balance");
                true
            }
        },
        Ok(None) => {
            tracing::warn!(%wallet_id, "low-balance check: wallet does not exist");
            true
        }
        Err(e) => {
            tracing::error!(%wallet_id, error = %e, "low-balance check failed");
            true
        }
    }
}

/// Charge a user and distribute the fee among workspace agents.
///
/// Flow:
/// 1. Debit total_amount from source_wallet (atomic)
/// 2. Platform keeps gas_pct (10%) as platform fee
/// 3. Remaining 90% split equally among agents, deposited to each agent's wallet
/// 4. Record payouts in agent_episode_payouts for audit
///
/// Returns total amount charged.
pub async fn charge_and_distribute(
    pool: &PgPool,
    source_wallet_id: Uuid,
    total_amount: i32,
    tx_type: &str,
    description: &str,
    agent_ids: &[Uuid],
    episode_id: Option<&str>,
    workspace_id: Option<Uuid>,
) -> std::result::Result<i32, (axum::http::StatusCode, String)> {
    // 1. Charge the user
    charge_gas(
        pool,
        source_wallet_id,
        total_amount,
        tx_type,
        description,
        episode_id,
    )
    .await?;

    if agent_ids.is_empty() {
        return Ok(total_amount);
    }

    // 2. Calculate platform cut vs agent share
    let platform_cut = std::cmp::max(1, (total_amount as f64 * 0.10) as i32);
    let agent_pool = total_amount - platform_cut;

    if agent_pool <= 0 {
        return Ok(total_amount);
    }

    // 3. Split equally among agents (last agent gets remainder)
    let per_agent = agent_pool / agent_ids.len() as i32;
    let remainder = agent_pool - (per_agent * agent_ids.len() as i32);

    for (i, agent_id) in agent_ids.iter().enumerate() {
        let payout = if i == agent_ids.len() - 1 {
            per_agent + remainder
        } else {
            per_agent
        };

        if payout <= 0 {
            continue;
        }

        // Get or create agent wallet.
        //
        // NOTE ON THIS WHOLE BLOCK: the caller has ALREADY been debited by
        // charge_gas() above. Every failure from here on means money left the
        // payer and did not arrive at the payee. None of it can be rolled back
        // (there is no enclosing transaction — only 12 exist in the codebase),
        // so the minimum bar is that every failure is LOUD and carries enough
        // identifiers to reconcile by hand. Previously every one was `let _ =`.
        let agent_id_str = agent_id.to_string();
        let agent_wallet = match get_or_create_wallet(pool, "agent", &agent_id_str).await {
            Ok(w) => w,
            Err(e) => {
                tracing::error!(
                    agent_id = %agent_id_str, payout, error = ?e,
                    "ROYALTY LOST: payer was charged but the agent wallet could not be resolved"
                );
                continue;
            }
        };

        if let Err(e) = credit_deposit(
            pool,
            agent_wallet.wallet_id,
            payout,
            &format!("Royalty: {}", description),
        )
        .await
        {
            tracing::error!(
                agent_id = %agent_id_str, wallet_id = %agent_wallet.wallet_id, payout,
                error = ?e,
                "ROYALTY LOST: payer was charged but the agent deposit failed"
            );
            // Do not run the audit row or auto-collect for a payout that
            // never landed — auto-collect would debit a wallet that was
            // never credited.
            continue;
        }

        // Record payout for audit
        let ep_id = episode_id.and_then(|e| Uuid::parse_str(e).ok());
        if let Err(e) = sqlx::query(
            "INSERT INTO agent_episode_payouts (episode_id, agent_id, workspace_id, amount, contribution_tier)
                 VALUES ($1, $2, $3, $4, 'equal')"
        )
        .bind(ep_id.unwrap_or_else(Uuid::nil))
        .bind(agent_id)
        .bind(workspace_id)
        .bind(payout)
        .execute(pool)
        .await
        {
            tracing::error!(
                agent_id = %agent_id_str, payout, error = %e,
                "royalty paid but the audit row failed — payout is real but unattributed"
            );
        }

        // Auto-collect: forward auto_collect_pct of the payout to the
        // agent's owner.
        //
        // This is a two-legged transfer with no transaction around it. The
        // original code debited the agent and then did `let _ =` on the owner
        // credit, so a failed second leg DESTROYED credits outright: gone from
        // the agent, never arrived at the owner, no record either way.
        //
        // We cannot make it atomic here, so we COMPENSATE: if the credit leg
        // fails, refund the debit leg. Credits are only genuinely lost when
        // both the credit and the refund fail, and that case says so.
        let row =
            match sqlx::query("SELECT auto_collect_pct, user_id FROM agents WHERE agent_id = $1")
                .bind(agent_id)
                .fetch_optional(pool)
                .await
            {
                Ok(Some(r)) => r,
                Ok(None) => continue,
                Err(e) => {
                    // Previously `if let Ok(Some(row))`: a read failure silently
                    // disabled auto-collect and the owner simply never got paid.
                    tracing::error!(
                        agent_id = %agent_id_str, error = %e,
                        "auto-collect config read failed; owner payout skipped"
                    );
                    continue;
                }
            };

        let auto_pct: i32 = row.try_get("auto_collect_pct").unwrap_or(0);
        let owner_id: Option<String> = row.try_get("user_id").unwrap_or(None);

        if auto_pct <= 0 {
            continue;
        }
        let oid = match owner_id {
            Some(o) => o,
            None => continue,
        };
        let auto_amount = std::cmp::max(1, payout * auto_pct / 100);

        // Leg 1: debit the agent.
        if let Err(e) = credit_charge(
            pool,
            agent_wallet.wallet_id,
            auto_amount,
            "agent_collect_out",
            "Auto-collect",
            None,
        )
        .await
        {
            tracing::warn!(
                agent_id = %agent_id_str, auto_amount, error = ?e,
                "auto-collect debit failed; nothing moved"
            );
            continue;
        }

        // Leg 2: credit the owner, refunding leg 1 on any failure.
        let credited = match get_or_create_wallet(pool, "user", &oid).await {
            Ok(owner_wallet) => credit_deposit_typed(
                pool,
                owner_wallet.wallet_id,
                auto_amount,
                "agent_collect_in",
                &format!("Auto-collect from {}", agent_id_str),
            )
            .await
            .map_err(|e| format!("{:?}", e)),
            Err(e) => Err(format!("owner wallet unavailable: {:?}", e)),
        };

        if let Err(reason) = credited {
            tracing::error!(
                agent_id = %agent_id_str, owner_id = %oid, auto_amount,
                reason = %reason,
                "auto-collect credit leg failed; refunding the agent"
            );
            if let Err(re) = credit_deposit_typed(
                pool,
                agent_wallet.wallet_id,
                auto_amount,
                "agent_collect_refund",
                &format!("Refund: auto-collect to {} failed", oid),
            )
            .await
            {
                tracing::error!(
                    agent_id = %agent_id_str, wallet_id = %agent_wallet.wallet_id,
                    auto_amount, error = ?re,
                    "CREDITS DESTROYED: agent debited, owner not credited, refund also failed"
                );
            }
        }
    }

    Ok(total_amount)
}

/// Look up all agent UUIDs in a workspace for fee distribution.
pub async fn get_workspace_agent_ids(pool: &PgPool, workspace_id: Uuid) -> Vec<Uuid> {
    sqlx::query("SELECT agent_id FROM workspace_agents WHERE workspace_id = $1")
        .bind(workspace_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|r| r.try_get::<Uuid, _>("agent_id").ok())
        .collect()
}
