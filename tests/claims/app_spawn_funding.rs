//! SPEC 36 R0.1 / LLD G01 (#37): App spawn must fund from the caller.
//!
//! Against `origin/main` these fail: spawn deposited `initial + extra` into the
//! workspace wallet and debited no one (`src/handlers/apps.rs:484, 560-568` at
//! a3cea516), so a zero-balance caller got a funded workspace and the ledger
//! sum rose by the budget.

use crate::support;
use fermi::apps::provision::{provision_workspace, total_budget, SpawnPlan};
use sqlx::PgPool;
use uuid::Uuid;

struct Fixture {
    pool: PgPool,
    user: String,
    app: String,
    teams: Vec<Uuid>,
    agents: Vec<Uuid>,
    others: Vec<String>,
}

impl Fixture {
    async fn new(pool: PgPool) -> Self {
        let fx = Fixture {
            user: support::tag("user"),
            app: support::tag("app"),
            pool,
            teams: vec![],
            agents: vec![],
            others: vec![],
        };
        support::make_user(&fx.pool, &fx.user).await;
        fx
    }

    /// A user wallet holding exactly `purchased` transferable credits.
    async fn fund_user(&self, purchased: i32) -> Uuid {
        let w: Uuid = sqlx::query_scalar(
            "INSERT INTO wallets (owner_type, owner_id) VALUES ('user', $1) RETURNING wallet_id",
        )
        .bind(&self.user)
        .fetch_one(&self.pool)
        .await
        .unwrap();
        if purchased > 0 {
            fermi_auth::credit_deposit(&self.pool, w, purchased, "spec36 test funding")
                .await
                .unwrap();
        }
        w
    }

    fn plan<'a>(&'a self, slug: &'a str, extra: i32, hire: &'a [String]) -> SpawnPlan<'a> {
        SpawnPlan {
            app_slug: &self.app,
            caller_id: &self.user,
            caller_may_use_granted: false,
            ws_name: "spec36 test workspace",
            ws_slug: slug,
            description: None,
            initial_budget: 100,
            extra_budget: extra,
            params: None,
            depends_on: &[],
            auto_hire: hire,
        }
    }

    async fn ledger_sum(&self) -> i64 {
        sqlx::query_scalar("SELECT COALESCE(SUM(amount),0)::bigint FROM credit_ledger")
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn team_exists(&self, slug: &str) -> bool {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM teams WHERE slug = $1)")
            .bind(slug)
            .fetch_one(&self.pool)
            .await
            .unwrap()
    }

    async fn cleanup(self) {
        let p = &self.pool;
        for t in &self.teams {
            let ws = t.to_string();
            for q in [
                "DELETE FROM workspace_agents WHERE workspace_id = $1::uuid",
                "DELETE FROM workspace_outputs WHERE workspace_id = $1::uuid",
                "DELETE FROM team_members WHERE team_id = $1::uuid",
            ] {
                sqlx::query(q).bind(&ws).execute(p).await.unwrap();
            }
            sqlx::query("DELETE FROM credit_ledger WHERE wallet_id IN (SELECT wallet_id FROM wallets WHERE owner_id = $1)")
                .bind(&ws).execute(p).await.unwrap();
            sqlx::query("DELETE FROM wallets WHERE owner_id = $1")
                .bind(&ws)
                .execute(p)
                .await
                .unwrap();
            sqlx::query("DELETE FROM teams WHERE id = $1")
                .bind(t)
                .execute(p)
                .await
                .unwrap();
        }
        sqlx::query("DELETE FROM credit_ledger WHERE wallet_id IN (SELECT wallet_id FROM wallets WHERE owner_id = $1)")
            .bind(&self.user).execute(p).await.unwrap();
        sqlx::query("DELETE FROM wallets WHERE owner_id = $1")
            .bind(&self.user)
            .execute(p)
            .await
            .unwrap();
        for a in &self.agents {
            sqlx::query("DELETE FROM agents WHERE agent_id = $1")
                .bind(a)
                .execute(p)
                .await
                .unwrap();
        }
        for u in self.others.iter().chain(std::iter::once(&self.user)) {
            support::drop_user(p, u).await;
        }
    }
}

#[tokio::test]
async fn zero_balance_caller_asking_for_extra_gets_402_and_nothing_is_created() {
    let Some(pool) = support::pool().await else {
        return;
    };
    let fx = Fixture::new(pool).await;
    fx.fund_user(0).await;
    support::sponsor_wallet(&fx.pool, 100).await;
    let slug = support::tag("ws");
    let before = fx.ledger_sum().await;

    let err = provision_workspace(&fx.pool, &fx.plan(&slug, 25, &[]))
        .await
        .unwrap_err();

    assert_eq!(err.status(), 402, "{}", err.message());
    assert!(
        !fx.team_exists(&slug).await,
        "a refused spawn must not leave a team row"
    );
    assert_eq!(
        fx.ledger_sum().await,
        before,
        "a refused spawn must not move credits"
    );
    fx.cleanup().await;
}

#[tokio::test]
async fn spawn_is_funded_by_transfers_sponsor_pays_template_caller_pays_extra() {
    let Some(pool) = support::pool().await else {
        return;
    };
    let mut fx = Fixture::new(pool).await;
    let caller_wallet = fx.fund_user(300).await;
    let sponsor = support::sponsor_wallet(&fx.pool, 100).await;
    let sponsor_before = fermi_auth::credit_get_balance(&fx.pool, sponsor).await.unwrap();
    let slug = support::tag("ws");
    let before = fx.ledger_sum().await;

    let p = provision_workspace(&fx.pool, &fx.plan(&slug, 50, &[]))
        .await
        .unwrap();
    fx.teams.push(p.workspace_id);

    assert_eq!(p.total_budget, 150);
    let caller_balance = fermi_auth::credit_get_balance(&fx.pool, caller_wallet)
        .await
        .unwrap();
    assert_eq!(caller_balance, 250, "caller paid only the extra 50 of 300");
    let sponsor_after = fermi_auth::credit_get_balance(&fx.pool, sponsor).await.unwrap();
    assert_eq!(sponsor_before - sponsor_after, 100, "sponsor paid exactly the template budget");
    let ws_balance: i32 = sqlx::query_scalar("SELECT balance FROM wallets WHERE owner_id = $1")
        .bind(p.workspace_id.to_string())
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    assert_eq!(
        ws_balance, 150,
        "workspace received exactly what sponsor and caller paid"
    );
    assert_eq!(
        fx.ledger_sum().await,
        before,
        "a transfer nets to zero: no credits minted"
    );
    fx.cleanup().await;
}

#[tokio::test]
async fn huge_extra_budget_is_422_before_any_io() {
    assert_eq!(total_budget(100, 1_000_000_000).unwrap_err().status(), 422);
    let Some(pool) = support::pool().await else {
        return;
    };
    let fx = Fixture::new(pool).await;
    fx.fund_user(10_000).await;
    support::sponsor_wallet(&fx.pool, 100).await;
    let slug = support::tag("ws");
    let err = provision_workspace(&fx.pool, &fx.plan(&slug, 1_000_000_000, &[]))
        .await
        .unwrap_err();
    assert_eq!(err.status(), 422);
    assert!(!fx.team_exists(&slug).await);
    fx.cleanup().await;
}

#[tokio::test]
async fn another_users_draft_agent_is_skipped_not_hired() {
    let Some(pool) = support::pool().await else {
        return;
    };
    let mut fx = Fixture::new(pool).await;
    fx.fund_user(500).await;
    support::sponsor_wallet(&fx.pool, 100).await;
    let name = support::tag("agent");
    let other = support::tag("someone_else");
    support::make_user(&fx.pool, &other).await;
    fx.others.push(other.clone());
    let agent_id: Uuid = sqlx::query_scalar(
        "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model, temperature,
                             author, status, visibility, user_id)
         VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'test', 0.5, 'spec36', 'draft', 'private', $2)
         RETURNING agent_id",
    )
    .bind(&name)
    .bind(&other)
    .fetch_one(&fx.pool)
    .await
    .unwrap();
    fx.agents.push(agent_id);
    let slug = support::tag("ws");
    let hire = vec![name.clone()];

    let p = provision_workspace(&fx.pool, &fx.plan(&slug, 0, &hire))
        .await
        .unwrap();
    fx.teams.push(p.workspace_id);

    assert!(p.agents_hired.is_empty());
    assert_eq!(p.skipped, vec![(name, "not_visible_to_caller")]);
    let hired: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2)",
    )
    .bind(p.workspace_id)
    .bind(agent_id)
    .fetch_one(&fx.pool)
    .await
    .unwrap();
    assert!(!hired);
    fx.cleanup().await;
}
