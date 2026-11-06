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
//! * Every credit the workspace receives is **transferred** from a wallet.
//!   The template budget comes from the platform sponsor wallet
//!   ([`SPONSOR_WALLET_OWNER`], migration 244) while the caller is within
//!   [`sponsored_spawn_quota`] sponsored spawns per 30 days and the sponsor can
//!   cover it; otherwise from the caller. `extra_budget` always comes from the
//!   caller. `Σ ledger` is unchanged by a spawn.
//! * Team row, owner membership, funding, budget display, params output and
//!   dependency edges commit in **one transaction** or not at all.
//! * A caller who must pay and cannot yields 402, and nothing is created.
//! * `extra_budget` is bounded by [`max_extra_budget`].
//! * Auto-hire applies the shared hire rule `apps::hire_policy::may_place`
//!   (never archived; drafts only for their owner; otherwise public,
//!   curated/system tier, or owned by the caller); anything else is reported
//!   in `skipped`, never silently inserted.
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
    /// A `depends_on` entry is malformed or names a workspace the caller is
    /// not a member of.
    InvalidDependency(String),
    Db(String),
}

impl ProvisionError {
    pub fn status(&self) -> u16 {
        match self {
            ProvisionError::InvalidBudget(_) => 422,
            ProvisionError::InsufficientCredits { .. } => 402,
            ProvisionError::Conflict => 409,
            ProvisionError::InvalidDependency(_) => 422,
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
            ProvisionError::InvalidDependency(m) => m.clone(),
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
                // AuthError flattens the sqlx error to text; the constraint
                // name is the only stable discriminator left in it.
                if m.contains("teams_slug_key") {
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
        match &e {
            sqlx::Error::Database(d) if d.constraint() == Some("teams_slug_key") => {
                ProvisionError::Conflict
            }
            _ => ProvisionError::Db(e.to_string()),
        }
    }
}

pub struct SpawnPlan<'a> {
    /// Wallet owner id that sponsors the template budget. Production passes
    /// [`SPONSOR_WALLET_OWNER`]; tests pass their own so they never spend or
    /// rewrite the shared sponsor wallet's history.
    pub sponsor_owner: &'a str,
    pub app_slug: &'a str,
    pub caller_id: &'a str,
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

/// Who pays a spawn's template budget.
enum Payer {
    Sponsor(Uuid),
    Caller(Uuid),
}

/// Sponsored spawns allowed per caller per rolling 30 days. Override with
/// `APP_SPONSORED_SPAWNS_PER_30D` (0 disables sponsorship).
pub const DEFAULT_SPONSORED_SPAWNS_PER_30D: i64 = 3;

pub fn sponsored_spawn_quota() -> i64 {
    std::env::var("APP_SPONSORED_SPAWNS_PER_30D")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v: &i64| *v >= 0)
        .unwrap_or(DEFAULT_SPONSORED_SPAWNS_PER_30D)
}

/// Memo marker on the sponsor's `transfer_out` ledger rows. The quota is
/// counted from these rows, so it needs no counter of its own: the ledger is
/// the record of what was sponsored.
const SPONSOR_MEMO_MARK: &str = "platform-sponsored template budget";

/// Choose the payer for the template budget.
///
/// The sponsor pays if the caller is within quota **and** the sponsor can
/// cover it; otherwise the caller pays. The sponsor row is locked
/// (`FOR UPDATE`) and the caller's quota is counted under that lock, so two
/// concurrent spawns by one caller cannot both take the last sponsored slot.
async fn template_payer(
    tx: &mut sqlx::PgConnection,
    plan: &SpawnPlan<'_>,
) -> Result<Payer, ProvisionError> {
    let quota = sponsored_spawn_quota();
    let sponsor: Option<(Uuid, i32)> = sqlx::query_as(
        "SELECT wallet_id, balance FROM wallets
          WHERE owner_type = 'platform' AND owner_id = $1 FOR UPDATE",
    )
    .bind(plan.sponsor_owner)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((sponsor_wallet, balance)) = sponsor {
        if quota > 0 && balance >= plan.initial_budget {
            // Sponsored spawns this caller already received: sponsor debits
            // whose paired workspace credit landed in a workspace the caller
            // owns. Both rows share `related_id` = workspace id.
            let used: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM credit_ledger l
                   JOIN teams t ON t.id::text = l.related_id
                  WHERE l.wallet_id = $1 AND l.tx_type = 'transfer_out'
                    AND l.description LIKE '%' || $2 || '%'
                    AND t.owner_id = $3
                    AND l.created_at > now() - interval '30 days'",
            )
            .bind(sponsor_wallet)
            .bind(SPONSOR_MEMO_MARK)
            .bind(plan.caller_id)
            .fetch_one(&mut *tx)
            .await?;
            if used < quota {
                return Ok(Payer::Sponsor(sponsor_wallet));
            }
        }
    }
    let caller = fermi_auth::credits::wallet_id_tx(&mut *tx, "user", plan.caller_id).await?;
    caller
        .map(Payer::Caller)
        .ok_or(ProvisionError::InsufficientCredits {
            have: 0,
            need: plan.initial_budget,
        })
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

    let mut upstreams: Vec<Uuid> = Vec::with_capacity(plan.depends_on.len());
    for raw in plan.depends_on {
        let u: Uuid = raw.parse().map_err(|_| {
            ProvisionError::InvalidDependency(format!("depends_on entry '{raw}' is not a UUID"))
        })?;
        if !upstreams.contains(&u) {
            upstreams.push(u);
        }
    }
    for upstream in &upstreams {
        // The caller must be a member of the upstream workspace: wiring a
        // dependency on someone else's workspace would expose its outputs.
        // Stricter than `add_dependency_handler` (workspace/outputs.rs), which
        // does not check upstream membership — tracked under G02's tenancy
        // sweep (#38).
        let member: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM team_members WHERE team_id = $1 AND member_id = $2)",
        )
        .bind(upstream)
        .bind(plan.caller_id)
        .fetch_one(&mut *tx)
        .await?;
        if !member {
            return Err(ProvisionError::InvalidDependency(format!(
                "depends_on workspace {upstream} is not one you are a member of"
            )));
        }
        sqlx::query(
            "INSERT INTO workspace_dependencies (upstream_id, downstream_id, dependency_type)
             VALUES ($1, $2, 'output')",
        )
        .bind(upstream)
        .bind(ws_id)
        .execute(&mut *tx)
        .await?;
    }
    let dependencies_wired = upstreams.len();

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
        let facts = crate::apps::hire_policy::AgentFacts {
            owner_id: owner.as_deref(),
            visibility: visibility.as_deref(),
            tier: &tier,
            status: &status,
        };
        if let Err(r) = crate::apps::hire_policy::may_place(&facts, plan.caller_id) {
            skipped.push((name.clone(), r.reason()));
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

    // Funding last: the sponsor wallet row is shared by every spawn, so its
    // row lock is taken as late as possible and held only until commit.
    if total > 0 {
        let ws_wallet =
            fermi_auth::credit_ensure_wallet_tx(&mut tx, "workspace", &ws_id_str).await?;
        let memo = format!("App {} workspace funding", plan.app_slug);
        // Template budget: sponsored by the platform wallet, within the
        // caller's sponsorship quota (owner decision 2026-11-06, option c +
        // quota; directive §4 G01 as amended). Over quota, or with the sponsor
        // short, the caller pays it. Never minted.
        if plan.initial_budget > 0 {
            let payer = template_payer(&mut tx, plan).await?;
            let (from, transferable_only, note) = match payer {
                Payer::Sponsor(w) => (w, false, "platform-sponsored template budget"),
                Payer::Caller(w) => (w, false, "template budget, paid by caller"),
            };
            fermi_auth::credit_transfer_tx(
                &mut tx,
                from,
                ws_wallet,
                plan.initial_budget,
                &format!("{memo} ({note})"),
                Some(&ws_id_str),
                transferable_only,
            )
            .await?;
        }
        // Extra budget: paid by the caller.
        if plan.extra_budget > 0 {
            let caller_wallet: Option<Uuid> =
                fermi_auth::credits::wallet_id_tx(&mut tx, "user", plan.caller_id).await?;
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
                // The caller always owns the new workspace, so granted credits
                // may fund it — the same *who-may* rule as owner self-funding
                // in `fund_workspace_handler` (src/handlers/workspace/core.rs:710).
                // Unlike that handler, the transfer keeps granted credits granted.
                false,
            )
            .await?;
        }
        sqlx::query("UPDATE teams SET workspace_budget = $1 WHERE id = $2")
            .bind(total)
            .bind(ws_id)
            .execute(&mut *tx)
            .await?;
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
