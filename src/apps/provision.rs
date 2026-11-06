//! Funding and provisioning a workspace spawned from an App (SPEC 36 R0.1).
//!
//! # What this replaced
//!
//! `spawn_workspace_handler` and `batch_spawn_workspaces_handler` used to
//! `credit_deposit` `initial_budget + extra_budget` into the new workspace
//! wallet and debit **nobody** [V: `src/handlers/apps.rs:484, 560-568,
//! 1280-1293` at a3cea516]. `extra_budget` had a floor of 0 and no ceiling.
//! The deployed dev prototype returned 201 and moved the ledger total by +505
//! for a zero-balance caller [V: `tests/claims/deployed.rs`, run 2026-11-06].
//!
//! # What holds now
//!
//! * Every credit the workspace receives is **transferred** from a wallet:
//!   the template budget from the platform sponsor wallet
//!   ([`SPONSOR_WALLET_OWNER`], migration 244), the `extra_budget` from the
//!   caller. `Σ ledger` is unchanged by a spawn.
//! * Team row, owner membership, funding, budget display, params output and
//!   dependency edges commit in **one transaction** or not at all.
//! * A caller who cannot afford the extra budget, or an exhausted sponsor
//!   wallet, yields 402 and nothing is created.
//! * `extra_budget` is bounded by [`max_extra_budget`].
//! * Auto-hire applies the same visibility rule as `hire_agent_handler`
//!   (public, curated/system tier, or owned by the caller); anything else is
//!   reported in `skipped`, never silently inserted.
//!
//! Git initial files are written by the caller **after** commit, because a git
//! commit cannot join a Postgres transaction. Their failure is reported, not
//! swallowed.

use serde_json::Value;
use uuid::Uuid;

/// The wallet that sponsors App template budgets (migration 244).
pub const SPONSOR_WALLET_OWNER: &str = "platform:app_sponsorship";

/// Default ceiling on `extra_budget`. Override with `APP_SPAWN_MAX_EXTRA_BUDGET`.
pub const DEFAULT_MAX_EXTRA_BUDGET: i32 = 1000;

pub fn max_extra_budget() -> i32 {
    std::env::var("APP_SPAWN_MAX_EXTRA_BUDGET")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v: &i32| *v >= 0)
        .unwrap_or(DEFAULT_MAX_EXTRA_BUDGET)
}

#[derive(Debug)]
pub enum ProvisionError {
    /// `extra_budget` outside `0..=max_extra_budget()`, or template budget negative.
    InvalidBudget(String),
    /// The caller's wallet cannot cover the budget. Nothing was created.
    InsufficientCredits {
        have: i32,
        need: i32,
    },
    /// Workspace slug already exists.
    Conflict,
    /// The platform sponsor wallet is missing or cannot cover the template
    /// budget. Nothing was created; an operator must top it up (ledgered grant).
    SponsorUnavailable,
    Db(String),
}

impl ProvisionError {
    pub fn status(&self) -> u16 {
        match self {
            ProvisionError::InvalidBudget(_) => 422,
            ProvisionError::InsufficientCredits { .. } => 402,
            ProvisionError::Conflict => 409,
            ProvisionError::SponsorUnavailable => 402,
            ProvisionError::Db(_) => 500,
        }
    }
    pub fn message(&self) -> String {
        match self {
            ProvisionError::InvalidBudget(m) => m.clone(),
            ProvisionError::InsufficientCredits { have, need } => {
                format!("Insufficient balance to fund this workspace: have {have}, need {need}")
            }
            ProvisionError::Conflict => "Slug collision — please retry".into(),
            ProvisionError::SponsorUnavailable => {
                "App sponsorship budget is exhausted; the template budget cannot be funded".into()
            }
            ProvisionError::Db(m) => m.clone(),
        }
    }
}

impl From<fermi_auth::AuthError> for ProvisionError {
    fn from(e: fermi_auth::AuthError) -> Self {
        match e {
            fermi_auth::AuthError::InsufficientCredits { have, need } => {
                ProvisionError::InsufficientCredits { have, need }
            }
            other => {
                let m = other.to_string();
                if m.contains("unique") || m.contains("duplicate") {
                    ProvisionError::Conflict
                } else {
                    ProvisionError::Db(m)
                }
            }
        }
    }
}

impl From<sqlx::Error> for ProvisionError {
    fn from(e: sqlx::Error) -> Self {
        ProvisionError::Db(e.to_string())
    }
}

pub struct SpawnPlan<'a> {
    pub app_slug: &'a str,
    pub caller_id: &'a str,
    /// Caller may spend granted credits (owner of the App or platform admin).
    /// Otherwise only purchased (transferable) credits fund the workspace.
    pub caller_may_use_granted: bool,
    pub ws_name: &'a str,
    pub ws_slug: &'a str,
    pub description: Option<&'a str>,
    pub initial_budget: i32,
    pub extra_budget: i32,
    pub params: Option<&'a Value>,
    pub depends_on: &'a [String],
    pub auto_hire: &'a [String],
}

#[derive(Debug, Clone)]
pub struct Provisioned {
    pub workspace_id: Uuid,
    pub total_budget: i32,
    pub agents_hired: Vec<String>,
    /// `(agent_name, reason)` for every auto-hire entry not inserted.
    pub skipped: Vec<(String, &'static str)>,
    pub dependencies_wired: usize,
}

/// Validate the budget before any I/O.
pub fn total_budget(initial: i32, extra: i32) -> Result<i32, ProvisionError> {
    if initial < 0 {
        return Err(ProvisionError::InvalidBudget(
            "App template initial_budget must not be negative".into(),
        ));
    }
    let max = max_extra_budget();
    if !(0..=max).contains(&extra) {
        return Err(ProvisionError::InvalidBudget(format!(
            "extra_budget must be between 0 and {max}"
        )));
    }
    initial
        .checked_add(extra)
        .ok_or_else(|| ProvisionError::InvalidBudget("budget overflow".into()))
}

/// Create, fund and wire one workspace in a single transaction.
pub async fn provision_workspace(
    pool: &sqlx::PgPool,
    plan: &SpawnPlan<'_>,
) -> Result<Provisioned, ProvisionError> {
    let total = total_budget(plan.initial_budget, plan.extra_budget)?;
    let mut tx = pool.begin().await?;

    let team = fermi_auth::teams::create_team_tx(
        &mut tx,
        plan.ws_name,
        plan.ws_slug,
        plan.description,
        plan.caller_id,
        plan.app_slug,
    )
    .await?;
    let ws_id = team.id;
    let ws_id_str = ws_id.to_string();

    if total > 0 {
        let ws_wallet =
            fermi_auth::credit_ensure_wallet_tx(&mut tx, "workspace", &ws_id_str).await?;
        let memo = format!("App {} workspace funding", plan.app_slug);
        // Template budget: sponsored by the platform wallet (owner decision,
        // option c — migration 244). A real transfer from a finite wallet, so
        // an empty sponsor wallet yields 402, never minted credits.
        if plan.initial_budget > 0 {
            let sponsor: Option<Uuid> =
                sqlx::query_scalar("SELECT wallet_id FROM wallets WHERE owner_id = $1")
                    .bind(SPONSOR_WALLET_OWNER)
                    .fetch_optional(&mut *tx)
                    .await?;
            let Some(sponsor) = sponsor else {
                return Err(ProvisionError::SponsorUnavailable);
            };
            fermi_auth::credit_transfer_tx(
                &mut tx,
                sponsor,
                ws_wallet,
                plan.initial_budget,
                &format!("{memo} (platform-sponsored template budget)"),
                Some(&ws_id_str),
                false,
            )
            .await
            .map_err(|e| match e {
                fermi_auth::AuthError::InsufficientCredits { .. } => {
                    ProvisionError::SponsorUnavailable
                }
                other => other.into(),
            })?;
        }
        // Extra budget: paid by the caller.
        if plan.extra_budget > 0 {
            let caller_wallet: Option<Uuid> =
                sqlx::query_scalar("SELECT wallet_id FROM wallets WHERE owner_id = $1")
                    .bind(plan.caller_id)
                    .fetch_optional(&mut *tx)
                    .await?;
            let Some(caller_wallet) = caller_wallet else {
                return Err(ProvisionError::InsufficientCredits {
                    have: 0,
                    need: plan.extra_budget,
                });
            };
            fermi_auth::credit_transfer_tx(
                &mut tx,
                caller_wallet,
                ws_wallet,
                plan.extra_budget,
                &format!("{memo} (extra budget)"),
                Some(&ws_id_str),
                !plan.caller_may_use_granted,
            )
            .await?;
        }
        sqlx::query("UPDATE teams SET workspace_budget = $1 WHERE id = $2")
            .bind(total)
            .bind(ws_id)
            .execute(&mut *tx)
            .await?;
    }

    if let Some(params) = plan.params {
        sqlx::query(
            "INSERT INTO workspace_outputs (workspace_id, key, value, version, updated_at, updated_by)
             VALUES ($1, 'params', $2, 1, NOW(), $3)",
        )
        .bind(ws_id)
        .bind(params)
        .bind(plan.caller_id)
        .execute(&mut *tx)
        .await?;
    }

    let mut dependencies_wired = 0usize;
    for upstream in plan.depends_on {
        let upstream: Uuid = upstream.parse().map_err(|_| {
            ProvisionError::InvalidBudget(format!("depends_on entry '{upstream}' is not a UUID"))
        })?;
        // The caller must be able to see the upstream workspace; wiring a
        // dependency on someone else's workspace would leak its outputs.
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM team_members WHERE team_id = $1 AND member_id = $2)",
        )
        .bind(upstream)
        .bind(plan.caller_id)
        .fetch_one(&mut *tx)
        .await?;
        if !member {
            return Err(ProvisionError::InvalidBudget(format!(
                "depends_on workspace {upstream} is not one you are a member of"
            )));
        }
        sqlx::query(
            "INSERT INTO workspace_dependencies (upstream_id, downstream_id, dependency_type)
             VALUES ($1, $2, 'output') ON CONFLICT (upstream_id, downstream_id) DO NOTHING",
        )
        .bind(upstream)
        .bind(ws_id)
        .execute(&mut *tx)
        .await?;
        dependencies_wired += 1;
    }

    let mut agents_hired = Vec::new();
    let mut skipped = Vec::new();
    for name in plan.auto_hire {
        let row: Option<(Uuid, Option<String>, Option<String>, String, String)> = sqlx::query_as(
            "SELECT agent_id, user_id, visibility, tier, status FROM agents WHERE agent_name = $1",
        )
        .bind(name)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((agent_id, owner, visibility, tier, status)) = row else {
            skipped.push((name.clone(), "not_found"));
            continue;
        };
        if status == "archived" {
            skipped.push((name.clone(), "archived"));
            continue;
        }
        let owned = owner.as_deref() == Some(plan.caller_id);
        let platform = tier == "curated" || tier == "system";
        let public = visibility.as_deref() == Some("public");
        // Same rule as `hire_agent_handler`, plus: a draft is only ever usable
        // by its owner.
        let allowed = owned || ((public || platform) && status != "draft");
        if !allowed {
            skipped.push((name.clone(), "not_visible_to_caller"));
            continue;
        }
        sqlx::query(
            "INSERT INTO workspace_agents (workspace_id, agent_id, added_by, relationship)
             VALUES ($1, $2, $3, 'system') ON CONFLICT (workspace_id, agent_id) DO NOTHING",
        )
        .bind(ws_id)
        .bind(agent_id)
        .bind(plan.caller_id)
        .execute(&mut *tx)
        .await?;
        agents_hired.push(name.clone());
    }

    tx.commit().await?;
    Ok(Provisioned {
        workspace_id: ws_id,
        total_budget: total,
        agents_hired,
        skipped,
        dependencies_wired,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_bounds() {
        assert_eq!(total_budget(100, 0).unwrap(), 100);
        assert_eq!(
            total_budget(100, DEFAULT_MAX_EXTRA_BUDGET).unwrap(),
            100 + DEFAULT_MAX_EXTRA_BUDGET
        );
        assert_eq!(total_budget(100, -1).unwrap_err().status(), 422);
        assert_eq!(total_budget(100, 1_000_000_000).unwrap_err().status(), 422);
        assert_eq!(total_budget(-5, 0).unwrap_err().status(), 422);
    }
}
