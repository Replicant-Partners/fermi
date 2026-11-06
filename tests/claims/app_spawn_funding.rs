//! SPEC 36 R0.1 / LLD G01 (#37): App spawn must not mint credits.
//!
//! The old-code failure is shown by `deployed.rs` (run against the deployed
//! prototype at a3cea516: 201, workspace funded with 505, ledger +505). These
//! tests exercise `fermi::apps::provision`, which does not exist on old code.
//!
//! Funding rule under test (owner decision 2026-11-06, option c; SPEC 36 R0.1
//! as amended): the template budget is transferred from a platform sponsor
//! wallet, `extra_budget` from the caller. Each test uses its **own** sponsor
//! wallet, so the shared `platform:app_sponsorship` history is never touched.

use crate::support::{self, Created};
use fermi::apps::provision::{provision_workspace, total_budget, SpawnPlan};
use std::sync::Arc;
use tokio::sync::Mutex;

struct Env {
    user: String,
    sponsor: String,
}

async fn env(
    pool: &sqlx::PgPool,
    c: &Arc<Mutex<Created>>,
    user_credits: i32,
    sponsor_credits: i32,
) -> Env {
    let mut c = c.lock().await;
    let user = support::make_user(pool, &mut c).await;
    support::make_wallet(pool, &mut c, "user", &user, user_credits).await;
    let sponsor = support::tag("platform:sponsor");
    support::make_wallet(pool, &mut c, "platform", &sponsor, sponsor_credits).await;
    Env { user, sponsor }
}

fn plan<'a>(e: &'a Env, slug: &'a str, extra: i32, hire: &'a [String]) -> SpawnPlan<'a> {
    SpawnPlan {
        sponsor_owner: &e.sponsor,
        app_slug: "spec36_test_app",
        caller_id: &e.user,
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

async fn team_exists(pool: &sqlx::PgPool, slug: &str) -> bool {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM teams WHERE slug = $1)")
        .bind(slug)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn zero_balance_caller_asking_for_extra_gets_402_and_nothing_is_created() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, c| async move {
        let e = env(&pool, &c, 0, 100).await;
        let owners = vec![e.user.clone(), e.sponsor.clone()];
        let slug = support::tag("ws");
        let before = support::ledger_sum(&pool, &owners).await;

        let err = provision_workspace(&pool, &plan(&e, &slug, 25, &[]))
            .await
            .unwrap_err();

        assert_eq!(err.status(), 402, "{}", err.message());
        assert!(
            !team_exists(&pool, &slug).await,
            "a refused spawn must not leave a team row"
        );
        assert_eq!(
            support::ledger_sum(&pool, &owners).await,
            before,
            "a refused spawn moves no credits"
        );
        assert_eq!(
            support::balances(&pool, &e.sponsor).await.0,
            100,
            "sponsor untouched on refusal"
        );
    })
    .await;
}

#[tokio::test]
async fn empty_sponsor_falls_back_to_the_caller_and_never_mints() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, c| async move {
        // Sponsor empty, caller has 500: caller pays the template 100.
        let e = env(&pool, &c, 500, 0).await;
        let slug = support::tag("ws");
        let p = provision_workspace(&pool, &plan(&e, &slug, 0, &[]))
            .await
            .unwrap();
        c.lock().await.teams.push(p.workspace_id);
        assert_eq!(
            support::balances(&pool, &e.user).await.0,
            400,
            "caller paid the template"
        );
        assert_eq!(support::balances(&pool, &e.sponsor).await.0, 0);

        // Sponsor empty, caller empty: 402, nothing created.
        let e2 = env(&pool, &c, 0, 0).await;
        let slug2 = support::tag("ws");
        let err = provision_workspace(&pool, &plan(&e2, &slug2, 0, &[]))
            .await
            .unwrap_err();
        assert_eq!(err.status(), 402, "{}", err.message());
        assert!(!team_exists(&pool, &slug2).await);
    })
    .await;
}

/// The per-caller quota (default 3 per 30 days): the 4th spawn is paid by the
/// caller even though the sponsor could cover it.
#[tokio::test]
async fn sponsorship_quota_is_per_caller() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, c| async move {
        let quota = fermi::apps::provision::sponsored_spawn_quota();
        let e = env(&pool, &c, 100, 100 * (quota as i32 + 2)).await;
        for _ in 0..quota {
            let slug = support::tag("ws");
            let p = provision_workspace(&pool, &plan(&e, &slug, 0, &[]))
                .await
                .unwrap();
            c.lock().await.teams.push(p.workspace_id);
        }
        assert_eq!(
            support::balances(&pool, &e.user).await.0,
            100,
            "first {quota} were sponsored"
        );
        let slug = support::tag("ws");
        let p = provision_workspace(&pool, &plan(&e, &slug, 0, &[]))
            .await
            .unwrap();
        c.lock().await.teams.push(p.workspace_id);
        assert_eq!(
            support::balances(&pool, &e.user).await.0,
            0,
            "over quota: caller paid"
        );
        assert_eq!(
            support::balances(&pool, &e.sponsor).await.0,
            100 * 2,
            "sponsor paid exactly {quota}"
        );
    })
    .await;
}

#[tokio::test]
async fn spawn_is_funded_by_transfers_sponsor_pays_template_caller_pays_extra() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, c| async move {
        let e = env(&pool, &c, 300, 100).await;
        let slug = support::tag("ws");
        let p = provision_workspace(&pool, &plan(&e, &slug, 50, &[]))
            .await
            .unwrap();
        c.lock().await.teams.push(p.workspace_id);
        let ws = p.workspace_id.to_string();
        let owners = vec![e.user.clone(), e.sponsor.clone(), ws.clone()];

        assert_eq!(p.total_budget, 150);
        assert_eq!(
            support::balances(&pool, &e.user).await.0,
            250,
            "caller paid only the extra 50"
        );
        assert_eq!(
            support::balances(&pool, &e.sponsor).await.0,
            0,
            "sponsor paid the template 100"
        );
        assert_eq!(
            support::balances(&pool, &ws).await.0,
            150,
            "workspace got exactly what was paid"
        );
        // 300 + 100 deposited by the fixture; every spawn movement is a pair.
        assert_eq!(
            support::ledger_sum(&pool, &owners).await,
            400,
            "no credits minted"
        );
    })
    .await;
}

/// Review finding 3: a transfer must not launder granted credits into
/// transferable ones.
#[tokio::test]
async fn granted_credits_stay_granted_across_a_transfer() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, c| async move {
        let e = env(&pool, &c, 0, 0).await;
        let sponsor_wallet: uuid::Uuid =
            sqlx::query_scalar("SELECT wallet_id FROM wallets WHERE owner_id = $1")
                .bind(&e.sponsor)
                .fetch_one(&pool)
                .await
                .unwrap();
        fermi_auth::credit_grant(&pool, sponsor_wallet, 100, "spec36 test grant")
            .await
            .unwrap();
        let slug = support::tag("ws");
        let p = provision_workspace(&pool, &plan(&e, &slug, 0, &[]))
            .await
            .unwrap();
        c.lock().await.teams.push(p.workspace_id);
        let (bal, granted, purchased) = support::balances(&pool, &p.workspace_id.to_string()).await;
        assert_eq!(
            (bal, granted, purchased),
            (100, 100, 0),
            "granted in, granted out"
        );
    })
    .await;
}

#[tokio::test]
async fn huge_extra_budget_is_422_before_any_io() {
    assert_eq!(total_budget(100, 1_000_000_000).unwrap_err().status(), 422);
    assert_eq!(total_budget(100, -1).unwrap_err().status(), 422);
}

#[tokio::test]
async fn new_workspace_owner_has_role_default_capabilities() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, c| async move {
        let e = env(&pool, &c, 0, 100).await;
        let slug = support::tag("ws");
        let p = provision_workspace(&pool, &plan(&e, &slug, 0, &[])).await.unwrap();
        c.lock().await.teams.push(p.workspace_id);
        // The AFTER INSERT trigger adds the owner with '{}'; create_team_tx
        // must merge the owner's role defaults in (review finding 1).
        let caps: Vec<String> = sqlx::query_scalar(
            "SELECT capabilities FROM team_members WHERE team_id = $1 AND member_id = $2 AND role = 'owner'",
        )
        .bind(p.workspace_id).bind(&e.user).fetch_one(&pool).await.unwrap();
        assert!(caps.iter().any(|c| c == "resolve"), "owner capabilities: {caps:?}");
    })
    .await;
}

#[tokio::test]
async fn another_users_draft_agent_is_skipped_not_hired() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, c| async move {
        let e = env(&pool, &c, 0, 100).await;
        let other = support::make_user(&pool, &mut *c.lock().await).await;
        let name = support::tag("agent");
        let agent_id: uuid::Uuid = sqlx::query_scalar(
            "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model, temperature,
                                 author, status, visibility, user_id)
             VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'test', 0.5, 'spec36', 'draft', 'public', $2)
             RETURNING agent_id",
        )
        .bind(&name).bind(&other).fetch_one(&pool).await.unwrap();
        c.lock().await.agents.push(agent_id);
        let slug = support::tag("ws");
        let hire = vec![name.clone()];

        let p = provision_workspace(&pool, &plan(&e, &slug, 0, &hire)).await.unwrap();
        c.lock().await.teams.push(p.workspace_id);

        assert!(p.agents_hired.is_empty());
        assert_eq!(p.skipped, vec![(name, "draft_not_owned")]);
        let hired: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2)",
        )
        .bind(p.workspace_id).bind(agent_id).fetch_one(&pool).await.unwrap();
        assert!(!hired);
    })
    .await;
}

#[tokio::test]
async fn depends_on_a_workspace_the_caller_is_not_in_is_refused() {
    let Some(pool) = support::pool().await else {
        return;
    };
    support::with_cleanup(&pool, |pool, c| async move {
        let e = env(&pool, &c, 0, 200).await;
        // Someone else's workspace, created by the same code path.
        let other = support::make_user(&pool, &mut *c.lock().await).await;
        let foreign = fermi_auth::teams::create_team(
            &pool,
            "spec36 foreign",
            &support::tag("ws"),
            None,
            &other,
            "spec36",
        )
        .await
        .unwrap();
        c.lock().await.teams.push(foreign.id);
        let deps = vec![foreign.id.to_string()];
        let slug = support::tag("ws");
        let mut pl = plan(&e, &slug, 0, &[]);
        pl.depends_on = &deps;
        let err = provision_workspace(&pool, &pl).await.unwrap_err();
        assert_eq!(err.status(), 422, "{}", err.message());
        assert!(!team_exists(&pool, &slug).await);
    })
    .await;
}
