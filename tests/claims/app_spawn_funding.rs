//! SPEC 36 R0.1 / LLD G01 (#37): App spawn must not mint credits.
//!
//! The old-code failure is shown by `deployed.rs` (run against the deployed
//! prototype at a3cea516: 201, workspace funded with 505, ledger +505). These
//! tests exercise `fermi::apps::provision`, which does not exist on old code.
//!
//! Funding rule under test (directive §4 G01 and SPEC 36 R0.1, as amended
//! 2026-11-06): the template budget is transferred from the platform sponsor
//! wallet while the caller is within quota and the sponsor can cover it;
//! otherwise from the caller, only with `accept_template_charge`.
//! `extra_budget` always comes from the caller.
//!
//! Every test runs inside **one transaction that is rolled back**: fixtures,
//! spawns and the append-only `app_sponsorships` rows all vanish, so nothing
//! is left on the shared dev database and no immutability rule is bent.

use crate::support;
use fermi::apps::provision::{
    provision_workspace_tx, total_budget, ProvisionError, Provisioned, SpawnPlan, TemplatePayer,
};
use sqlx::{PgConnection, Postgres, Transaction};

struct Env {
    user: String,
    sponsor: String,
}

/// Fixture rows inside the test transaction.
async fn user(tx: &mut PgConnection, purchased: i32) -> String {
    let id = support::tag("user");
    sqlx::query(
        "INSERT INTO users (email, password_hash, password_salt, user_id, auth_provider, display_name)
         VALUES ($1, 'x', 'x', $2, 'legacy', 'spec36 test user')",
    )
    .bind(format!("{id}@{}", support::TEST_EMAIL_DOMAIN))
    .bind(&id)
    .execute(&mut *tx)
    .await
    .unwrap();
    wallet(tx, "user", &id, purchased).await;
    id
}

async fn wallet(
    tx: &mut PgConnection,
    owner_type: &str,
    owner_id: &str,
    purchased: i32,
) -> uuid::Uuid {
    let w: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO wallets (owner_type, owner_id) VALUES ($1, $2) RETURNING wallet_id",
    )
    .bind(owner_type)
    .bind(owner_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    if purchased > 0 {
        fermi_auth::credit_deposit_tx(
            &mut *tx,
            w,
            purchased,
            "deposit",
            "spec36 test funding",
            None,
        )
        .await
        .unwrap();
    }
    w
}

async fn env(tx: &mut PgConnection, user_credits: i32, sponsor_credits: i32) -> Env {
    let user = user(tx, user_credits).await;
    let sponsor = support::tag("platform:sponsor");
    wallet(tx, "platform", &sponsor, sponsor_credits).await;
    Env { user, sponsor }
}

fn plan<'a>(e: &'a Env, slug: &'a str, extra: i32, hire: &'a [String]) -> SpawnPlan<'a> {
    SpawnPlan {
        sponsor_owner: &e.sponsor,
        sponsored_quota: 3,
        accept_template_charge: true,
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

/// One spawn as its own savepoint, so a refused spawn rolls back only itself
/// (as its own transaction would in production) and the test continues.
async fn spawn(
    tx: &mut Transaction<'_, Postgres>,
    p: &SpawnPlan<'_>,
) -> Result<Provisioned, ProvisionError> {
    let total = total_budget(p.initial_budget, p.extra_budget)?;
    let mut sp = sqlx::Acquire::begin(&mut **tx).await.unwrap();
    match provision_workspace_tx(&mut sp, p, total).await {
        Ok(v) => {
            sp.commit().await.unwrap();
            Ok(v)
        }
        Err(e) => {
            sp.rollback().await.unwrap();
            Err(e)
        }
    }
}

async fn bal(tx: &mut PgConnection, owner: &str) -> (i32, i32, i32) {
    sqlx::query_as(
        "SELECT balance, granted_balance, purchased_balance FROM wallets WHERE owner_id = $1",
    )
    .bind(owner)
    .fetch_one(&mut *tx)
    .await
    .unwrap()
}

async fn team_exists(tx: &mut PgConnection, slug: &str) -> bool {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM teams WHERE slug = $1)")
        .bind(slug)
        .fetch_one(&mut *tx)
        .await
        .unwrap()
}

async fn ledger_sum(tx: &mut PgConnection, owners: &[String]) -> i64 {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(l.amount),0)::bigint FROM credit_ledger l
         JOIN wallets w USING (wallet_id) WHERE w.owner_id = ANY($1)",
    )
    .bind(owners)
    .fetch_one(&mut *tx)
    .await
    .unwrap()
}

/// Open the test transaction (rolled back when dropped).
macro_rules! test_tx {
    () => {{
        let Some(pool) = support::pool().await else {
            return;
        };
        pool.begin().await.unwrap()
    }};
}

#[tokio::test]
async fn zero_balance_caller_asking_for_extra_gets_402_and_nothing_is_created() {
    let mut tx = test_tx!();
    let e = env(&mut tx, 0, 100).await;
    let owners = vec![e.user.clone(), e.sponsor.clone()];
    let slug = support::tag("ws");
    let before = ledger_sum(&mut tx, &owners).await;

    let err = spawn(&mut tx, &plan(&e, &slug, 25, &[])).await.unwrap_err();

    assert_eq!(err.status(), 402, "{}", err.message());
    assert!(
        !team_exists(&mut tx, &slug).await,
        "a refused spawn must not leave a team row"
    );
    assert_eq!(
        ledger_sum(&mut tx, &owners).await,
        before,
        "a refused spawn moves no credits"
    );
    assert_eq!(
        bal(&mut tx, &e.sponsor).await.0,
        100,
        "sponsor untouched on refusal"
    );
}

#[tokio::test]
async fn empty_sponsor_falls_back_to_the_caller_and_never_mints() {
    let mut tx = test_tx!();
    let e = env(&mut tx, 500, 0).await;
    let p = spawn(&mut tx, &plan(&e, &support::tag("ws"), 0, &[]))
        .await
        .unwrap();
    assert_eq!(p.template_paid_by, Some(TemplatePayer::Caller));
    assert_eq!(
        bal(&mut tx, &e.user).await.0,
        400,
        "caller paid the template"
    );

    let e2 = env(&mut tx, 0, 0).await;
    let slug2 = support::tag("ws");
    let err = spawn(&mut tx, &plan(&e2, &slug2, 0, &[]))
        .await
        .unwrap_err();
    assert_eq!(err.status(), 402, "{}", err.message());
    assert!(!team_exists(&mut tx, &slug2).await);
}

/// The caller's 4th spawn is caller-paid while a second caller on the same
/// sponsor is still sponsored.
#[tokio::test]
async fn sponsorship_quota_is_per_caller() {
    let mut tx = test_tx!();
    let e = env(&mut tx, 100, 100 * 5).await;
    for _ in 0..3 {
        let p = spawn(&mut tx, &plan(&e, &support::tag("ws"), 0, &[]))
            .await
            .unwrap();
        assert_eq!(p.template_paid_by, Some(TemplatePayer::Sponsor));
    }
    assert_eq!(bal(&mut tx, &e.user).await.0, 100, "first 3 were sponsored");
    let p = spawn(&mut tx, &plan(&e, &support::tag("ws"), 0, &[]))
        .await
        .unwrap();
    assert_eq!(
        p.template_paid_by,
        Some(TemplatePayer::Caller),
        "4th is caller-paid"
    );
    assert_eq!(bal(&mut tx, &e.user).await.0, 0);

    let other = Env {
        user: user(&mut tx, 0).await,
        sponsor: e.sponsor.clone(),
    };
    let p = spawn(&mut tx, &plan(&other, &support::tag("ws"), 0, &[]))
        .await
        .unwrap();
    assert_eq!(
        p.template_paid_by,
        Some(TemplatePayer::Sponsor),
        "quota is per caller"
    );
    assert_eq!(
        bal(&mut tx, &e.sponsor).await.0,
        100,
        "sponsor paid exactly 4"
    );
}

/// Deleting sponsored workspaces must not refund the quota. Fails on
/// 7f46c0ea, whose count joined `teams`.
#[tokio::test]
async fn deleting_a_sponsored_workspace_does_not_refund_the_quota() {
    let mut tx = test_tx!();
    let e = env(&mut tx, 0, 100 * 5).await;
    for _ in 0..3 {
        let p = spawn(&mut tx, &plan(&e, &support::tag("ws"), 0, &[]))
            .await
            .unwrap();
        fermi_auth::teams::delete_team_tx(&mut tx, p.workspace_id, &e.user)
            .await
            .unwrap();
    }
    let err = spawn(&mut tx, &plan(&e, &support::tag("ws"), 0, &[]))
        .await
        .unwrap_err();
    assert_eq!(
        err.status(),
        402,
        "quota must stay spent after deletes: {}",
        err.message()
    );
}

/// An unsponsored spawn without `accept_template_charge` is refused with the
/// price, and nothing is charged.
#[tokio::test]
async fn unsponsored_spawn_requires_accepting_the_charge() {
    let mut tx = test_tx!();
    let e = env(&mut tx, 500, 0).await;
    let slug = support::tag("ws");
    let mut pl = plan(&e, &slug, 0, &[]);
    pl.accept_template_charge = false;
    let err = spawn(&mut tx, &pl).await.unwrap_err();
    assert!(
        matches!(
            err,
            ProvisionError::TemplateChargeNotAccepted { price: 100 }
        ),
        "{err:?}"
    );
    assert_eq!(err.status(), 402);
    assert_eq!(bal(&mut tx, &e.user).await.0, 500, "nothing charged");
    assert!(!team_exists(&mut tx, &slug).await);
}

#[tokio::test]
async fn spawn_is_funded_by_transfers_sponsor_pays_template_caller_pays_extra() {
    let mut tx = test_tx!();
    let e = env(&mut tx, 300, 100).await;
    let p = spawn(&mut tx, &plan(&e, &support::tag("ws"), 50, &[]))
        .await
        .unwrap();
    let ws = p.workspace_id.to_string();
    let owners = vec![e.user.clone(), e.sponsor.clone(), ws.clone()];

    assert_eq!(p.total_budget, 150);
    assert_eq!(
        bal(&mut tx, &e.user).await.0,
        250,
        "caller paid only the extra 50"
    );
    assert_eq!(
        bal(&mut tx, &e.sponsor).await.0,
        0,
        "sponsor paid the template 100"
    );
    assert_eq!(
        bal(&mut tx, &ws).await.0,
        150,
        "workspace got exactly what was paid"
    );
    assert_eq!(
        ledger_sum(&mut tx, &owners).await,
        400,
        "300 + 100 deposited; spawn is pairs only"
    );
}

/// A transfer must not launder granted credits into transferable ones.
#[tokio::test]
async fn granted_credits_stay_granted_across_a_transfer() {
    let mut tx = test_tx!();
    let e = env(&mut tx, 0, 0).await;
    let sponsor: uuid::Uuid =
        sqlx::query_scalar("SELECT wallet_id FROM wallets WHERE owner_id = $1")
            .bind(&e.sponsor)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    fermi_auth::credits::grant_tx(&mut tx, sponsor, 100, "spec36 test grant")
        .await
        .unwrap();
    let p = spawn(&mut tx, &plan(&e, &support::tag("ws"), 0, &[]))
        .await
        .unwrap();
    assert_eq!(
        bal(&mut tx, &p.workspace_id.to_string()).await,
        (100, 100, 0),
        "granted in, granted out"
    );
}

#[tokio::test]
async fn huge_extra_budget_is_422_before_any_io() {
    assert_eq!(total_budget(100, 1_000_000_000).unwrap_err().status(), 422);
    assert_eq!(total_budget(100, -1).unwrap_err().status(), 422);
}

#[tokio::test]
async fn new_workspace_owner_has_role_default_capabilities() {
    let mut tx = test_tx!();
    let e = env(&mut tx, 0, 100).await;
    let p = spawn(&mut tx, &plan(&e, &support::tag("ws"), 0, &[]))
        .await
        .unwrap();
    // The AFTER INSERT trigger adds the owner with '{}'; create_team_tx must
    // merge the owner's role defaults in.
    let caps: Vec<String> = sqlx::query_scalar(
        "SELECT capabilities FROM team_members WHERE team_id = $1 AND member_id = $2 AND role = 'owner'",
    )
    .bind(p.workspace_id)
    .bind(&e.user)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(
        caps.iter().any(|c| c == "resolve"),
        "owner capabilities: {caps:?}"
    );
}

#[tokio::test]
async fn another_users_draft_agent_is_skipped_not_hired() {
    let mut tx = test_tx!();
    let e = env(&mut tx, 0, 100).await;
    let other = user(&mut tx, 0).await;
    let name = support::tag("agent");
    let agent_id: uuid::Uuid = sqlx::query_scalar(
        "INSERT INTO agents (agent_name, agent_type, version, tier, executor_type, model, temperature,
                             author, status, visibility, user_id)
         VALUES ($1, 'research', '1.0.0', 'community', 'llm', 'test', 0.5, 'spec36', 'draft', 'public', $2)
         RETURNING agent_id",
    )
    .bind(&name)
    .bind(&other)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let hire = vec![name.clone()];
    let p = spawn(&mut tx, &plan(&e, &support::tag("ws"), 0, &hire))
        .await
        .unwrap();

    assert!(p.agents_hired.is_empty());
    assert_eq!(p.skipped, vec![(name, "draft_not_owned")]);
    let hired: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM workspace_agents WHERE workspace_id = $1 AND agent_id = $2)",
    )
    .bind(p.workspace_id)
    .bind(agent_id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(!hired);
}

#[tokio::test]
async fn depends_on_a_workspace_the_caller_is_not_in_is_refused() {
    let mut tx = test_tx!();
    let e = env(&mut tx, 0, 200).await;
    let other = user(&mut tx, 0).await;
    let foreign = fermi_auth::teams::create_team_tx(
        &mut tx,
        "spec36 foreign",
        &support::tag("ws"),
        None,
        &other,
        "spec36",
    )
    .await
    .unwrap();
    let deps = vec![foreign.id.to_string()];
    let slug = support::tag("ws");
    let mut pl = plan(&e, &slug, 0, &[]);
    pl.depends_on = &deps;
    let err = spawn(&mut tx, &pl).await.unwrap_err();
    assert_eq!(err.status(), 422, "{}", err.message());
    assert!(!team_exists(&mut tx, &slug).await);
}
