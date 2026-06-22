//! Forecast/portfolio ACL & PATCH regression tests (Spec 24 §3.2 Wave 1).
//!
//! Run with: `cargo test --test forecast_acl -- --ignored --test-threads=1`
//! Requires `DATABASE_URL` (the deployed Neon DB — see `.env`).
//!
//! These tests cover bug fixes shipped in Sprint 1 of Spec 24
//! "Forecast Collaboration & Sharing":
//!
//!   1. The previous `get_forecast_handler` queried
//!      `team_members.user_id` but the actual column is `member_id`. The
//!      team-fallback branch was therefore dead — team members never gained
//!      access to a private forecast even when `fermi_forecasts.team_id`
//!      pointed at their team. We assert the canonical helper
//!      `fermi_auth::visibility::is_team_member` (which the handler now
//!      delegates to) returns `true` for an actual team member and `false`
//!      for a stranger.
//!
//!   2. `patch_portfolio_handler` accepted only `title`/`description` —
//!      every other field was silently dropped at the serde layer.
//!      `PatchPortfolioRequest` now carries `domain`, `visibility`, and
//!      `team_id`, and the SQL UPDATE COALESCEs them. We assert a portfolio
//!      flips from `private` to `public` (and gains a `team_id`) after the
//!      handler's UPDATE, and that an all-null PATCH leaves every column
//!      unchanged.
//!
//!   3. `list_forecasts_handler` and `list_portfolios_handler` ignored team
//!      membership. A private forecast/portfolio with `team_id` set was
//!      invisible to its own team — even though `get_forecast_handler`
//!      (post-fix-#1) granted access if you typed the URL. List and detail
//!      disagreed. The WHERE clauses now include a team_members EXISTS
//!      branch matching the detail handler. We assert: list-as-owner sees
//!      the row, list-as-team-member sees the row, list-as-stranger does
//!      NOT.
//!
//!   4. `list_portfolio_forecasts_handler`'s enriched projection now ships
//!      `share_count` (COUNT of `object_shares` for the forecast) so the
//!      console can render the visibility badge correctly without a second
//!      roundtrip. We assert: a fresh row reports 0; after inserting one
//!      `object_shares` row, it reports 1.
//!
//!   5. Sprint 2.1 migrations: `forecast_invites` table must exist with
//!      the spec'd CHECK constraints, and `object_shares.object_type`
//!      must include `'portfolio'`. We probe the schema directly so a
//!      missing or partial migration trips this test rather than a
//!      runtime 500 in production.
//!
//!   6. Sprint 2.2 share routes — the GET/POST/DELETE trio on
//!      `/api/forecasts/:id/shares` and `/api/portfolios/:id/shares`.
//!      We exercise the same `fermi_auth::teams::{share_object,
//!      list_object_shares, revoke_share}` helpers the handlers call,
//!      plus the lifted SQL of `verify_share_matches_target`. Covers
//!      the full lifecycle (POST → GET sees it → DELETE → GET empty)
//!      for both target types, plus cross-target safety (a share on
//!      forecast A must NOT be deletable from a forecast-B endpoint).
//!
//! Tests that need a live DB are marked `#[ignore]` so a vanilla
//! `cargo test` passes without one.

use std::str::FromStr;
use std::time::Duration;

use sqlx::postgres::PgConnectOptions;
use sqlx::PgPool;
use uuid::Uuid;

use fermi_auth::visibility::is_team_member;
use fermi_auth::{teams, ObjectType, Permission, ShareType};

/// Acquire a Neon pool. Returns `None` if `DATABASE_URL` isn't set so the
/// test can early-return silently — matches the pattern in
/// `tests/bayesops_refit.rs`.
async fn try_pool() -> Option<PgPool> {
    // Best-effort .env load (the deployed value lives in .env at repo root).
    let _ = std::fs::read_to_string(".env").map(|contents| {
        for line in contents.lines() {
            if let Some((key, val)) = line.split_once('=') {
                let key = key.trim();
                let val = val.trim().trim_matches('"');
                if !key.is_empty()
                    && !key.starts_with('#')
                    && std::env::var(key).is_err()
                {
                    std::env::set_var(key, val);
                }
            }
        }
    });
    let url = std::env::var("DATABASE_URL").ok()?;
    // Neon + PgBouncer transaction-mode hates prepared-statement caches.
    let opts = PgConnectOptions::from_str(&url).ok()?.statement_cache_capacity(0);
    sqlx::pool::PoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(30))
        .connect_with(opts)
        .await
        .ok()
}

/// Generate a uniquely-named test row so concurrent runs don't collide and
/// cleanup failures don't poison the next run.
fn unique_suffix() -> String {
    Uuid::new_v4().to_string()[..8].to_string()
}

/// Insert a `teams` row directly (skipping `create_team`'s auto-add-owner
/// trigger which would also write a `team_members` row we don't want).
/// Production has no FK from `teams.owner_id` to `users` (verified
/// 2026-06-19), so a synthetic owner_id is safe.
async fn insert_test_team(pool: &PgPool, suffix: &str) -> Uuid {
    let team_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO teams (id, name, slug, description, owner_id, origin)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(team_id)
    .bind(format!("acl-test-{}", suffix))
    .bind(format!("acl-test-{}", suffix))
    .bind(Some("Spec 24 ACL regression"))
    .bind(format!("acl-owner-{}", suffix))
    .bind("test")
    .execute(pool)
    .await
    .expect("insert teams row");

    // The schema ships an AFTER-INSERT trigger that auto-adds the owner as
    // role='owner' in team_members (migration 009:90-102). We rely on that;
    // members are added separately below.
    team_id
}

async fn add_member(pool: &PgPool, team_id: Uuid, member_id: &str, role: &str) {
    sqlx::query(
        "INSERT INTO team_members
            (team_id, member_type, member_id, role, invited_by)
         VALUES ($1, 'user', $2, $3, $2)
         ON CONFLICT (team_id, member_id) DO UPDATE SET role = EXCLUDED.role",
    )
    .bind(team_id)
    .bind(member_id)
    .bind(role)
    .execute(pool)
    .await
    .expect("insert team_members row");
}

async fn cleanup(pool: &PgPool, team_id: Uuid) {
    let _ = sqlx::query("DELETE FROM team_members WHERE team_id = $1")
        .bind(team_id)
        .execute(pool)
        .await;
    let _ = sqlx::query("DELETE FROM teams WHERE id = $1")
        .bind(team_id)
        .execute(pool)
        .await;
}

// ─── Tests ────────────────────────────────────────────────────────────

/// Sanity: the helper sees a real member as a member.
///
/// This is the regression test for the `team_members.user_id` typo: the
/// helper queries the correct column (`member_id`) and therefore returns
/// the value the handler always intended to compute.
#[tokio::test]
#[ignore]
async fn is_team_member_recognises_actual_member() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let team_id = insert_test_team(&pool, &suffix).await;
    let member_id = format!("acl-member-{}", suffix);
    add_member(&pool, team_id, &member_id, "member").await;

    let granted = is_team_member(&pool, team_id, &member_id)
        .await
        .expect("is_team_member call");
    assert!(
        granted,
        "real team member must be recognised — \
         this proves the team_members.member_id query works"
    );

    cleanup(&pool, team_id).await;
}

/// Sanity: the helper rejects a stranger.
#[tokio::test]
#[ignore]
async fn is_team_member_rejects_stranger() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let team_id = insert_test_team(&pool, &suffix).await;
    // No add_member call. The auto-trigger added the owner — an unrelated
    // user_id should NOT be a member.
    let stranger_id = format!("acl-stranger-{}", suffix);

    let granted = is_team_member(&pool, team_id, &stranger_id)
        .await
        .expect("is_team_member call");
    assert!(!granted, "stranger must NOT be recognised as a team member");

    cleanup(&pool, team_id).await;
}

/// Cross-team isolation: a member of team A is not a member of team B.
#[tokio::test]
#[ignore]
async fn is_team_member_does_not_cross_teams() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let team_a = insert_test_team(&pool, &format!("a-{}", suffix)).await;
    let team_b = insert_test_team(&pool, &format!("b-{}", suffix)).await;

    let member_id = format!("acl-cross-{}", suffix);
    add_member(&pool, team_a, &member_id, "member").await;

    let in_a = is_team_member(&pool, team_a, &member_id).await.unwrap();
    let in_b = is_team_member(&pool, team_b, &member_id).await.unwrap();

    assert!(in_a, "must be member of A");
    assert!(!in_b, "must NOT leak access into B");

    cleanup(&pool, team_a).await;
    cleanup(&pool, team_b).await;
}

// ─── PATCH /api/portfolios/:id (Spec 24 §3.2 Wave 1 #2) ──────────────

/// Borrow an arbitrary existing user's UUID. `fermi_portfolios.owner_id`
/// has an FK to `users(id)` (verified 2026-06-19), so a synthetic UUID
/// would fail to insert. We don't mutate the user — only point at it as
/// owner of a throwaway test portfolio.
async fn pick_existing_user_id(pool: &PgPool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users LIMIT 1")
        .fetch_one(pool)
        .await
        .expect(
            "no users in DB — the test borrows an existing users.id to satisfy \
             the fermi_portfolios.owner_id FK",
        )
}

/// Insert a minimal portfolio row owned by `owner_id`, returning its id.
/// `team_id` starts NULL; visibility starts 'private'.
async fn insert_test_portfolio(pool: &PgPool, owner_id: Uuid, suffix: &str) -> String {
    let pid = format!("acl-pf-{}", suffix);
    sqlx::query(
        "INSERT INTO fermi_portfolios
            (id, title, description, owner_id, visibility, notebook_ids, metadata)
         VALUES ($1, $2, $3, $4, 'private', '{}', '{}'::jsonb)",
    )
    .bind(&pid)
    .bind(format!("ACL Test Portfolio {}", suffix))
    .bind(Some("created by tests/forecast_acl.rs — safe to delete"))
    .bind(owner_id)
    .execute(pool)
    .await
    .expect("insert fermi_portfolios row");
    pid
}

async fn delete_test_portfolio(pool: &PgPool, portfolio_id: &str) {
    let _ = sqlx::query("DELETE FROM fermi_portfolios WHERE id = $1")
        .bind(portfolio_id)
        .execute(pool)
        .await;
}

/// Run the EXACT SQL UPDATE that `patch_portfolio_handler` ships
/// (`src/handlers/forecasts.rs`). Lifting the query verbatim is the
/// honest way to cover the bug fix without constructing `AppState`,
/// which is `pub(crate)` and therefore unreachable from `tests/`. If the
/// handler's SQL drifts, this test must drift with it — that is the
/// intended pressure.
async fn run_patch_portfolio_sql(
    pool: &PgPool,
    portfolio_id: &str,
    title: Option<&str>,
    description: Option<&str>,
    domain: Option<&str>,
    visibility: Option<&str>,
    team_id: Option<Uuid>,
) {
    sqlx::query(
        "UPDATE fermi_portfolios
         SET title       = COALESCE($2, title),
             description = COALESCE($3, description),
             domain      = COALESCE($4, domain),
             visibility  = COALESCE($5, visibility),
             team_id     = COALESCE($6, team_id),
             updated_at  = NOW()
         WHERE id = $1",
    )
    .bind(portfolio_id)
    .bind(title)
    .bind(description)
    .bind(domain)
    .bind(visibility)
    .bind(team_id)
    .execute(pool)
    .await
    .expect("patch UPDATE");
}

/// Snapshot of the columns the PATCH path can touch. Used to assert
/// "all-null PATCH leaves everything unchanged."
#[derive(Debug, PartialEq, Eq)]
struct PortfolioSnapshot {
    title: String,
    description: Option<String>,
    domain: Option<String>,
    visibility: String,
    team_id: Option<Uuid>,
}

async fn snapshot_portfolio(pool: &PgPool, portfolio_id: &str) -> PortfolioSnapshot {
    let row = sqlx::query_as::<
        _,
        (String, Option<String>, Option<String>, String, Option<Uuid>),
    >(
        "SELECT title, description, domain, visibility, team_id
         FROM fermi_portfolios WHERE id = $1",
    )
    .bind(portfolio_id)
    .fetch_one(pool)
    .await
    .expect("snapshot SELECT");
    PortfolioSnapshot {
        title: row.0,
        description: row.1,
        domain: row.2,
        visibility: row.3,
        team_id: row.4,
    }
}

/// PATCH with `visibility='public'` flips the column.
/// PATCH with `team_id=Some(X)` populates a previously-NULL team.
/// Together they prove the bug fix: pre-fix, both fields would have been
/// silently dropped at the serde layer (PatchPortfolioRequest didn't
/// declare them), and the SQL UPDATE didn't reference them.
#[tokio::test]
#[ignore]
async fn patch_portfolio_persists_visibility_and_team_id() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let pid = insert_test_portfolio(&pool, owner, &suffix).await;
    let team_id = insert_test_team(&pool, &suffix).await;

    // Sanity: pre-PATCH state is the documented insert default.
    let before = snapshot_portfolio(&pool, &pid).await;
    assert_eq!(before.visibility, "private");
    assert_eq!(before.team_id, None);
    assert_eq!(before.domain, None);

    run_patch_portfolio_sql(
        &pool,
        &pid,
        None,
        None,
        Some("test-domain"),
        Some("public"),
        Some(team_id),
    )
    .await;

    let after = snapshot_portfolio(&pool, &pid).await;
    assert_eq!(after.visibility, "public", "visibility must flip to public");
    assert_eq!(after.team_id, Some(team_id), "team_id must be set");
    assert_eq!(after.domain.as_deref(), Some("test-domain"));
    // title + description were not in the PATCH; they must be unchanged.
    assert_eq!(after.title, before.title);
    assert_eq!(after.description, before.description);

    delete_test_portfolio(&pool, &pid).await;
    cleanup(&pool, team_id).await;
}

/// All-null PATCH (every Option = None) is a no-op.
/// This proves COALESCE preserves the existing value — the standard PATCH
/// contract everywhere else in ABW.
#[tokio::test]
#[ignore]
async fn patch_portfolio_with_all_null_is_noop() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let pid = insert_test_portfolio(&pool, owner, &suffix).await;

    // Establish a non-default state so the all-null PATCH actually has
    // something to (not) erase.
    let team_id = insert_test_team(&pool, &suffix).await;
    run_patch_portfolio_sql(
        &pool,
        &pid,
        Some("Renamed for noop test"),
        None,
        Some("noop-domain"),
        Some("shared"),
        Some(team_id),
    )
    .await;
    let baseline = snapshot_portfolio(&pool, &pid).await;

    run_patch_portfolio_sql(&pool, &pid, None, None, None, None, None).await;
    let after = snapshot_portfolio(&pool, &pid).await;

    assert_eq!(
        after, baseline,
        "all-null PATCH must leave every column unchanged"
    );

    delete_test_portfolio(&pool, &pid).await;
    cleanup(&pool, team_id).await;
}

// ─── List handlers honour team membership (Spec 24 §3.2 Wave 1 #3) ───

/// Borrow two distinct existing `users.id` values. Both `fermi_forecasts`
/// and `fermi_portfolios` have an FK on `owner_id` to `users(id)`, so we
/// need real ids. The pair is also the substrate for "owner vs team
/// member" tests below.
async fn pick_two_existing_user_ids(pool: &PgPool) -> (Uuid, Uuid) {
    let rows = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM users ORDER BY created_at LIMIT 2",
    )
    .fetch_all(pool)
    .await
    .expect("fetch two users.id");
    assert!(
        rows.len() >= 2,
        "tests need at least two users in the DB to model owner + team member"
    );
    (rows[0], rows[1])
}

/// Insert a minimal forecast row owned by `owner_id` with the given
/// `team_id` and `visibility`. Returns the auto-generated id.
async fn insert_test_forecast(
    pool: &PgPool,
    owner_id: Uuid,
    team_id: Option<Uuid>,
    visibility: &str,
    suffix: &str,
) -> String {
    let row = sqlx::query_scalar::<_, String>(
        "INSERT INTO fermi_forecasts
            (owner_id, question_text, predicted_probability, visibility, team_id, status)
         VALUES ($1, $2, $3, $4, $5, 'active')
         RETURNING id",
    )
    .bind(owner_id)
    .bind(format!("ACL test forecast {}", suffix))
    .bind(0.5_f32)
    .bind(visibility)
    .bind(team_id)
    .fetch_one(pool)
    .await
    .expect("insert fermi_forecasts row");
    row
}

async fn delete_test_forecast(pool: &PgPool, forecast_id: &str) {
    let _ = sqlx::query("DELETE FROM fermi_forecasts WHERE id = $1")
        .bind(forecast_id)
        .execute(pool)
        .await;
}

/// Run the EXACT WHERE clause that `list_forecasts_handler` ships, scoped
/// to a single forecast id so the test is deterministic regardless of
/// what else lives in the DB. Returns true if the row is visible to the
/// caller.
///
/// The clause is lifted verbatim from src/handlers/forecasts.rs — if the
/// handler drifts, this test must drift with it.
async fn forecast_visible_to(
    pool: &PgPool,
    forecast_id: &str,
    caller_user_id: &str,
) -> bool {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fermi_forecasts f
         WHERE f.id = $2
           AND (f.owner_id = $1::uuid
                OR f.visibility IN ('shared', 'public')
                OR (f.team_id IS NOT NULL
                    AND EXISTS (SELECT 1 FROM team_members m
                                WHERE m.team_id = f.team_id
                                  AND m.member_id = $1)))",
    )
    .bind(caller_user_id)
    .bind(forecast_id)
    .fetch_one(pool)
    .await
    .expect("forecast visibility probe");
    n == 1
}

/// Same shape against the portfolio list WHERE clause.
async fn portfolio_visible_to(
    pool: &PgPool,
    portfolio_id: &str,
    caller_user_id: &str,
) -> bool {
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fermi_portfolios p
         WHERE p.id = $2
           AND (p.owner_id = $1::uuid
                OR p.visibility IN ('shared', 'public')
                OR (p.team_id IS NOT NULL
                    AND EXISTS (SELECT 1 FROM team_members m
                                WHERE m.team_id = p.team_id
                                  AND m.member_id = $1)))",
    )
    .bind(caller_user_id)
    .bind(portfolio_id)
    .fetch_one(pool)
    .await
    .expect("portfolio visibility probe");
    n == 1
}

/// A private forecast with `team_id` set is visible to the owner, to a
/// member of that team, and NOT to a stranger.
///
/// Pre-fix, the team member's query returned 0 rows because the WHERE
/// clause never consulted team_members. List/detail disagreed:
/// `get_forecast_handler` (after Step 1) granted access if you typed the
/// URL, but the row was missing from /api/forecasts.
#[tokio::test]
#[ignore]
async fn list_forecasts_includes_team_private() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let (owner, member) = pick_two_existing_user_ids(&pool).await;
    let team_id = insert_test_team(&pool, &suffix).await;
    add_member(&pool, team_id, &member.to_string(), "member").await;
    let fid = insert_test_forecast(
        &pool,
        owner,
        Some(team_id),
        "private",
        &suffix,
    )
    .await;

    let stranger = Uuid::new_v4().to_string();

    assert!(
        forecast_visible_to(&pool, &fid, &owner.to_string()).await,
        "owner must see their own forecast"
    );
    assert!(
        forecast_visible_to(&pool, &fid, &member.to_string()).await,
        "team member must see the team's private forecast — \
         this proves the team_members EXISTS branch in the WHERE clause"
    );
    assert!(
        !forecast_visible_to(&pool, &fid, &stranger).await,
        "stranger must NOT see a private forecast"
    );

    delete_test_forecast(&pool, &fid).await;
    cleanup(&pool, team_id).await;
}

/// Same shape against `list_portfolios_handler`.
#[tokio::test]
#[ignore]
async fn list_portfolios_includes_team_private() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let (owner, member) = pick_two_existing_user_ids(&pool).await;
    let team_id = insert_test_team(&pool, &suffix).await;
    add_member(&pool, team_id, &member.to_string(), "member").await;
    let pid = insert_test_portfolio(&pool, owner, &suffix).await;
    // Promote the just-inserted private/no-team portfolio to team-private,
    // exercising the same UPDATE the new patch_portfolio_handler ships.
    run_patch_portfolio_sql(
        &pool,
        &pid,
        None,
        None,
        None,
        None, // visibility stays 'private'
        Some(team_id),
    )
    .await;

    let stranger = Uuid::new_v4().to_string();

    assert!(
        portfolio_visible_to(&pool, &pid, &owner.to_string()).await,
        "owner must see their own portfolio"
    );
    assert!(
        portfolio_visible_to(&pool, &pid, &member.to_string()).await,
        "team member must see the team's private portfolio"
    );
    assert!(
        !portfolio_visible_to(&pool, &pid, &stranger).await,
        "stranger must NOT see a private portfolio"
    );

    delete_test_portfolio(&pool, &pid).await;
    cleanup(&pool, team_id).await;
}

// ─── share_count in portfolio-list projection (Spec 24 §3.2 Wave 1 #4) ───

/// Probe the `share_count` subquery exactly as
/// `list_portfolio_forecasts_handler` ships it. If the SQL drifts in the
/// handler, this test must drift with it — same pressure as the other
/// verbatim-SQL probes.
async fn share_count_for_forecast(pool: &PgPool, forecast_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM object_shares s
         WHERE s.object_type = 'forecast'
           AND s.object_id = $1",
    )
    .bind(forecast_id)
    .fetch_one(pool)
    .await
    .expect("share_count probe")
}

/// Insert one `object_shares` row pointing at `forecast_id`, granting
/// `permission` to a synthetic user share_target. Returns the share id
/// for cleanup.
async fn insert_test_share(
    pool: &PgPool,
    forecast_id: &str,
    share_target: &str,
    permission: &str,
) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         VALUES ('forecast', $1, 'user', $2, $3, $2)
         RETURNING id",
    )
    .bind(forecast_id)
    .bind(share_target)
    .bind(permission)
    .fetch_one(pool)
    .await
    .expect("insert object_shares row")
}

async fn delete_test_share(pool: &PgPool, share_id: Uuid) {
    let _ = sqlx::query("DELETE FROM object_shares WHERE id = $1")
        .bind(share_id)
        .execute(pool)
        .await;
}

/// A freshly-inserted forecast has zero shares. Adding one
/// `object_shares` row bumps the count to 1. Sprint 4's badge logic
/// keys off this field directly.
#[tokio::test]
#[ignore]
async fn share_count_reflects_object_shares_rows() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;

    assert_eq!(
        share_count_for_forecast(&pool, &fid).await,
        0,
        "fresh forecast must have zero shares"
    );

    let target = format!("acl-share-target-{}", suffix);
    let share_id = insert_test_share(&pool, &fid, &target, "view").await;

    assert_eq!(
        share_count_for_forecast(&pool, &fid).await,
        1,
        "share_count must reflect the inserted object_shares row"
    );

    delete_test_share(&pool, share_id).await;

    assert_eq!(
        share_count_for_forecast(&pool, &fid).await,
        0,
        "share_count must drop back to zero after revocation"
    );

    delete_test_forecast(&pool, &fid).await;
}

// ─── Sprint 2.1: migration 151 & 152 schema presence ─────────────────

/// Assert the `forecast_invites` table exists with the spec'd CHECK
/// constraints. The boot path in `src/api_server.rs` runs migrations on
/// every start, so a missing or skipped 151 should show up here before
/// any handler code starts depending on the table.
#[tokio::test]
#[ignore]
async fn migration_151_forecast_invites_present() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables
         WHERE table_schema = 'public' AND table_name = 'forecast_invites')",
    )
    .fetch_one(&pool)
    .await
    .expect("table existence probe");
    assert!(
        table_exists,
        "forecast_invites table missing — migration 151 has not run. \
         Boot the api-server once or apply migrations/151_forecast_invites.sql manually."
    );

    // The exactly-one-of-recipient invariant is the most error-prone
    // part of the schema; assert it by name so a future drop+forget
    // trips this test.
    let invariant_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM pg_constraint
         WHERE conname = 'forecast_invites_recipient_exactly_one')",
    )
    .fetch_one(&pool)
    .await
    .expect("constraint existence probe");
    assert!(
        invariant_exists,
        "forecast_invites_recipient_exactly_one CHECK is missing — \
         migration 151 applied partially?"
    );
}

/// Assert `object_shares.object_type` accepts `'portfolio'`. We test by
/// behavior (insert + rollback) rather than parsing pg_constraint
/// because the CHECK definition string ordering isn't stable across
/// PostgreSQL versions.
#[tokio::test]
#[ignore]
async fn migration_152_object_shares_accepts_portfolio() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    // Use a savepoint so the row never persists. We don't care about
    // the data — only that the INSERT survives the CHECK constraint.
    let mut tx = pool.begin().await.expect("begin tx");
    let result = sqlx::query(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         VALUES ('portfolio', $1, 'user', $2, 'view', $3)",
    )
    .bind(format!("migration-152-probe-{}", unique_suffix()))
    .bind("probe-target")
    .bind("probe-granter")
    .execute(&mut *tx)
    .await;
    tx.rollback().await.expect("rollback");

    assert!(
        result.is_ok(),
        "object_shares CHECK does not accept 'portfolio' — \
         migration 152 has not run. Boot the api-server once or apply \
         migrations/152_object_shares_portfolio.sql manually. Error: {:?}",
        result.err()
    );
}

// ─── Sprint 2.2: per-target share routes ──────────────────────────────

/// Lift of `verify_share_matches_target` from src/handlers/shares.rs —
/// if the handler's SQL drifts, this test must drift with it.
async fn share_belongs_to_target(
    pool: &PgPool,
    share_id: Uuid,
    object_type: &str,
    object_id: &str,
) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM object_shares
         WHERE id = $1 AND object_type = $2 AND object_id = $3)",
    )
    .bind(share_id)
    .bind(object_type)
    .bind(object_id)
    .fetch_one(pool)
    .await
    .expect("verify_share_matches_target probe")
}

/// Full lifecycle for forecast shares:
/// POST equivalent (teams::share_object) → list sees it →
/// verify_share_matches_target succeeds → revoke → list is empty.
#[tokio::test]
#[ignore]
async fn forecast_shares_full_lifecycle() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;

    // Pre-state: zero shares.
    let before = teams::list_object_shares(&pool, ObjectType::Forecast, &fid)
        .await
        .expect("list pre");
    assert_eq!(before.len(), 0, "fresh forecast has no shares");

    // POST equivalent — same call the handler makes.
    let share_target = format!("share-target-{}", suffix);
    let share = teams::share_object(
        &pool,
        ObjectType::Forecast,
        &fid,
        ShareType::User,
        &share_target,
        Permission::View,
        &owner.to_string(),
    )
    .await
    .expect("share_object");

    // GET equivalent.
    let listed = teams::list_object_shares(&pool, ObjectType::Forecast, &fid)
        .await
        .expect("list post");
    assert_eq!(listed.len(), 1, "one share visible after POST");
    assert_eq!(listed[0].id, share.id);
    assert_eq!(listed[0].share_target, share_target);
    assert_eq!(listed[0].permission, Permission::View);

    // verify_share_matches_target — the guard that prevents
    // cross-object DELETE attacks.
    assert!(
        share_belongs_to_target(&pool, share.id, "forecast", &fid).await,
        "share must be claimed by its forecast"
    );
    assert!(
        !share_belongs_to_target(&pool, share.id, "forecast", "some-other-forecast").await,
        "share must NOT be claimed by a different forecast id"
    );
    assert!(
        !share_belongs_to_target(&pool, share.id, "portfolio", &fid).await,
        "share must NOT be claimed by the portfolio object_type"
    );

    // DELETE equivalent.
    teams::revoke_share(&pool, share.id).await.expect("revoke");

    let after = teams::list_object_shares(&pool, ObjectType::Forecast, &fid)
        .await
        .expect("list final");
    assert_eq!(after.len(), 0, "no shares after revoke");

    delete_test_forecast(&pool, &fid).await;
}

/// Same matrix against portfolios. Critically exercises the migration
/// 152 path: object_type='portfolio' must round-trip cleanly through
/// share_object / list_object_shares / revoke_share.
#[tokio::test]
#[ignore]
async fn portfolio_shares_full_lifecycle() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let pid = insert_test_portfolio(&pool, owner, &suffix).await;

    let before = teams::list_object_shares(&pool, ObjectType::Portfolio, &pid)
        .await
        .expect("list pre");
    assert_eq!(before.len(), 0);

    let share_target = format!("pf-share-target-{}", suffix);
    let share = teams::share_object(
        &pool,
        ObjectType::Portfolio,
        &pid,
        ShareType::User,
        &share_target,
        Permission::Edit,
        &owner.to_string(),
    )
    .await
    .expect("share_object portfolio");

    let listed = teams::list_object_shares(&pool, ObjectType::Portfolio, &pid)
        .await
        .expect("list post");
    assert_eq!(listed.len(), 1);
    // Round-trip of ObjectType through the helper is the subtle bit:
    // list_object_shares calls ObjectType::from_str on the column value.
    // For 'portfolio' to come back as ObjectType::Portfolio, the enum
    // arm we added to fermi-auth/src/types.rs must be wired up.
    assert_eq!(
        listed[0].object_type,
        ObjectType::Portfolio,
        "object_type must round-trip as Portfolio — \
         this proves the fermi-auth enum has the Portfolio variant"
    );
    assert_eq!(listed[0].permission, Permission::Edit);

    teams::revoke_share(&pool, share.id).await.expect("revoke");
    let after = teams::list_object_shares(&pool, ObjectType::Portfolio, &pid)
        .await
        .expect("list final");
    assert_eq!(after.len(), 0);

    delete_test_portfolio(&pool, &pid).await;
}

/// Repeat POST upgrades the existing share's permission rather than
/// creating a duplicate (ON CONFLICT DO UPDATE in share_object).
/// Important for the console UX: clicking "Make admin" on an existing
/// "View" share is just another POST.
#[tokio::test]
#[ignore]
async fn forecast_share_upsert_changes_permission() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let target = format!("upsert-target-{}", suffix);

    let s1 = teams::share_object(
        &pool,
        ObjectType::Forecast,
        &fid,
        ShareType::User,
        &target,
        Permission::View,
        &owner.to_string(),
    )
    .await
    .expect("initial share");

    let s2 = teams::share_object(
        &pool,
        ObjectType::Forecast,
        &fid,
        ShareType::User,
        &target,
        Permission::Admin,
        &owner.to_string(),
    )
    .await
    .expect("upsert share");

    assert_eq!(s1.id, s2.id, "upsert must reuse the same row");

    let listed = teams::list_object_shares(&pool, ObjectType::Forecast, &fid)
        .await
        .expect("list");
    assert_eq!(listed.len(), 1, "no duplicate row");
    assert_eq!(
        listed[0].permission,
        Permission::Admin,
        "permission must be upgraded by the second POST"
    );

    teams::revoke_share(&pool, s2.id).await.expect("revoke");
    delete_test_forecast(&pool, &fid).await;
}

// ─── Sprint 2.2: lookup_user_by_email ────────────────────────────────

/// Lifted SQL from `lookup_user_by_email_handler` so the handler can
/// drift the test if the column list or matching strategy changes.
async fn lookup_by_email(pool: &PgPool, email: &str) -> Option<(String, Option<String>)> {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT user_id, display_name
         FROM users
         WHERE LOWER(email) = $1
           AND user_id IS NOT NULL
         LIMIT 1",
    )
    .bind(email.to_lowercase())
    .fetch_optional(pool)
    .await
    .expect("lookup query")
}

/// Look up an existing user by their actual email; assert the user_id
/// comes back. Then look up a nonsense email; assert None.
#[tokio::test]
#[ignore]
async fn lookup_user_by_email_finds_existing_and_404s_others() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    // Borrow a real (email, user_id) pair so the assertion is honest.
    // We don't trust .env to have a specific user — we just take the
    // first one with a non-null email and user_id.
    let real: Option<(String, String)> = sqlx::query_as(
        "SELECT email, user_id FROM users
         WHERE email IS NOT NULL AND user_id IS NOT NULL
         ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .expect("borrow real user");
    let Some((email, expected_user_id)) = real else {
        eprintln!("skip: no user with both email and user_id");
        return;
    };

    // Hit: exact match (case-insensitive — the handler lowercases, and
    // we test mixed-case to verify the lowering really happens).
    let scrambled = mixed_case(&email);
    let found = lookup_by_email(&pool, &scrambled).await;
    assert_eq!(
        found.as_ref().map(|(uid, _)| uid.as_str()),
        Some(expected_user_id.as_str()),
        "lookup must match case-insensitively"
    );

    // Miss: a guaranteed-nonexistent email.
    let phantom = format!("does-not-exist-{}@example.invalid", unique_suffix());
    let none = lookup_by_email(&pool, &phantom).await;
    assert!(none.is_none(), "phantom email must not resolve");
}

/// Tiny helper: alternate uppercase/lowercase characters to verify the
/// handler's LOWER() comparison actually matches.
fn mixed_case(s: &str) -> String {
    s.chars()
        .enumerate()
        .map(|(i, c)| if i % 2 == 0 { c.to_ascii_uppercase() } else { c.to_ascii_lowercase() })
        .collect()
}
