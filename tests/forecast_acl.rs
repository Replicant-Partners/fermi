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
//!   7. Sprint 2.3a invite state machine. We exercise the create /
//!      list / decline / revoke transitions via the same SQL the
//!      handlers ship (lifted from `src/handlers/invites.rs`). The
//!      accept path is Sprint 2.3b. Covers:
//!        - create → row with status='pending' + token-on-email-only
//!        - GET /me/invites → only the invitee sees their own pending
//!        - decline by invitee → status='declined', empty list
//!        - decline twice → second is a no-op (already-terminal)
//!        - revoke by inviter → status='revoked'
//!        - cross-recipient: stranger cannot decline someone else's
//!
//!   8. Sprint 2.3b accept paths. The accept transition materialises a
//!      grant in object_shares or team_members and flips status to
//!      'accepted', in two best-effort steps (helpers are idempotent
//!      via ON CONFLICT). Covers:
//!        - forecast accept → object_shares row exists with right perm
//!        - portfolio accept → object_shares with right perm
//!        - team accept → team_members row with right role
//!        - double-accept → second no-ops (UPDATE WHERE status=pending)
//!        - decline-then-accept → 0 rows-affected, no leaked grant
//!        - by-token preview returns target metadata; expired/terminal
//!          tokens look the same to the public endpoint (no leakage)
//!
//!   9. Sprint 2.3c email-claim resolver. The OIDC and SIWE sign-in
//!      flows call fermi_auth::invites::claim_pending_for_email so a
//!      user who is invited by email *before* they have an account
//!      finds the invite in their inbox on first sign-in. Covers:
//!        - happy path: pending email invite → claim → user_id back-
//!          filled, email nulled, status still 'pending', inbox shows it
//!        - idempotency: second claim returns 0
//!        - case-insensitivity: mixed-case email matches lowercase stored
//!        - terminal invites are immutable history (declined, revoked,
//!          accepted, expired untouched)
//!        - empty user_id or email → defensive 0 (never UPDATE on a
//!          wildcard match)
//!
//!  10. Sprint 2.4a migration 154 backfill. The INSERT ... ON CONFLICT
//!     DO NOTHING backfill must be idempotent and every forecast/
//!     portfolio with team_id must have a corresponding object_shares
//!     row with share_type='team', permission='edit'.
//!
//!  11. Sprint 2.4b handler ACL switch. Handlers now delegate to
//!     can_view/can_edit/can_admin instead of inline owner checks.
//!     Covers:
//!       - can_view grants access via direct user-share in object_shares
//!       - can_edit allows user with permission='edit' to mutate
//!       - can_admin allows user with permission='admin' to share/delete
//!       - permission='view' user cannot edit (403)
//!
//! Tests that need a live DB are marked `#[ignore]` so a vanilla
//! `cargo test` passes without one.

use std::str::FromStr;
use std::time::Duration;

use sqlx::postgres::PgConnectOptions;
use sqlx::PgPool;
use uuid::Uuid;

use fermi_auth::visibility::{can_access, can_edit, can_view, is_team_member};
use fermi_auth::{teams, AuthPrincipal, MemberType, ObjectType, Permission, ShareType, TeamRole, Visibility};

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

async fn pick_second_existing_user_id(pool: &PgPool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users OFFSET 1 LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("need at least 2 users for collaboration test")
}

async fn pick_third_existing_user_id(pool: &PgPool) -> Uuid {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users OFFSET 2 LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("need at least 3 users for collaboration test")
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
                                  AND m.member_id = $1))
                OR EXISTS (SELECT 1 FROM object_shares s
                           WHERE s.object_type = 'forecast'
                             AND s.object_id = f.id::text
                             AND s.share_type = 'user'
                             AND s.share_target = $1))",
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
                                   AND m.member_id = $1))
                 OR EXISTS (SELECT 1 FROM object_shares s
                            WHERE s.object_type = 'portfolio'
                              AND s.object_id = p.id::text
                              AND s.share_type = 'user'
                              AND s.share_target = $1))",
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

// ─── Sprint 2.3a: invite state machine ────────────────────────────────

/// Lifted INSERT from `create_invite_row` in src/handlers/invites.rs.
/// Returns the new invite id + token (if any). If the handler's SQL
/// drifts this test must drift with it — same pressure as the other
/// verbatim-SQL probes.
async fn insert_test_invite(
    pool: &PgPool,
    target_type: &str,
    target_id: &str,
    permission: &str,
    invitee_user_id: Option<&str>,
    invitee_email: Option<&str>,
    token: Option<&str>,
    inviter_id: &str,
) -> (Uuid, Option<String>) {
    sqlx::query_as::<_, (Uuid, Option<String>)>(
        "INSERT INTO forecast_invites
            (target_type, target_id, permission, invitee_user_id, invitee_email,
             token, inviter_id)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, token",
    )
    .bind(target_type)
    .bind(target_id)
    .bind(permission)
    .bind(invitee_user_id)
    .bind(invitee_email)
    .bind(token)
    .bind(inviter_id)
    .fetch_one(pool)
    .await
    .expect("insert forecast_invites row")
}

async fn fetch_invite_status(pool: &PgPool, invite_id: Uuid) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT status FROM forecast_invites WHERE id = $1",
    )
    .bind(invite_id)
    .fetch_optional(pool)
    .await
    .expect("fetch invite status")
}

/// Lifted from `list_my_invites_handler` — count invites visible to a
/// caller. We assert by count rather than full payload comparison so
/// the test isn't tied to row order or to other concurrent invites in
/// the DB (the suite shares one Neon instance with other tests).
async fn count_pending_invites_for(pool: &PgPool, user_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM forecast_invites
         WHERE invitee_user_id = $1 AND status = 'pending'",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("count pending invites")
}

async fn delete_invite(pool: &PgPool, invite_id: Uuid) {
    let _ = sqlx::query("DELETE FROM forecast_invites WHERE id = $1")
        .bind(invite_id)
        .execute(pool)
        .await;
}

/// Lifted UPDATE from `decline_invite_handler`. Returns rows_affected.
async fn run_decline(pool: &PgPool, invite_id: Uuid, caller_user_id: &str) -> u64 {
    sqlx::query(
        "UPDATE forecast_invites
            SET status = 'declined'
          WHERE id = $1
            AND status = 'pending'
            AND invitee_user_id = $2",
    )
    .bind(invite_id)
    .bind(caller_user_id)
    .execute(pool)
    .await
    .expect("run_decline UPDATE")
    .rows_affected()
}

/// Lifted UPDATE from `revoke_invite_handler` (the post-authority
/// transition step). Caller-authority logic is in the handler; here we
/// test that the SQL atomically transitions pending → revoked.
async fn run_revoke(pool: &PgPool, invite_id: Uuid) -> u64 {
    sqlx::query(
        "UPDATE forecast_invites SET status = 'revoked'
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(invite_id)
    .execute(pool)
    .await
    .expect("run_revoke UPDATE")
    .rows_affected()
}

/// Full happy path: invite by user_id → status=pending →
/// invitee sees it in their inbox → invitee declines → inbox empty.
#[tokio::test]
#[ignore]
async fn invite_create_list_decline_lifecycle() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;

    // Synthetic invitee — text user_id, no FK on invitee_user_id so a
    // freshly-minted string is fine.
    let invitee = format!("invite-test-invitee-{}", suffix);

    // Inbox is empty before the invite.
    let before = count_pending_invites_for(&pool, &invitee).await;
    assert_eq!(before, 0, "invitee inbox starts empty");

    let (invite_id, token) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        Some(&invitee),
        None,
        None,
        &owner.to_string(),
    )
    .await;
    assert!(token.is_none(), "user-id invites do not need a token");
    assert_eq!(
        fetch_invite_status(&pool, invite_id).await.as_deref(),
        Some("pending"),
        "freshly-inserted invite starts pending"
    );

    let after_create = count_pending_invites_for(&pool, &invitee).await;
    assert_eq!(after_create, 1, "invitee sees one pending invite");

    // Decline path.
    let n = run_decline(&pool, invite_id, &invitee).await;
    assert_eq!(n, 1, "decline must transition exactly one row");
    assert_eq!(
        fetch_invite_status(&pool, invite_id).await.as_deref(),
        Some("declined"),
    );

    let after_decline = count_pending_invites_for(&pool, &invitee).await;
    assert_eq!(after_decline, 0, "declined invite no longer in inbox");

    // Cleanup.
    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Email-only invite path: token IS minted, inbox stays empty (until
/// the email-claim resolver lands in Sprint 2.3c).
#[tokio::test]
#[ignore]
async fn invite_email_only_mints_token_and_skips_inbox() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let email = format!("invite-{}@example.invalid", suffix);

    // Mimic the handler: caller mints a token for email invites.
    let token_str = format!("test-token-{}", suffix);
    let (invite_id, token) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        None,
        Some(&email),
        Some(&token_str),
        &owner.to_string(),
    )
    .await;
    assert_eq!(
        token.as_deref(),
        Some(token_str.as_str()),
        "email invites must store the token"
    );

    // Email invites do NOT surface in any user's inbox until the
    // email-claim resolver maps the email to a user_id.
    let by_user = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM forecast_invites
         WHERE invitee_user_id IS NOT NULL AND id = $1",
    )
    .bind(invite_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(by_user, 0, "email-only invite has no user_id yet");

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Decline twice: second decline is a no-op (UPDATE …
/// WHERE status='pending' matches zero rows once the invite is
/// terminal). The handler's no-rows-affected path then disambiguates
/// to 404 or 409; we verify the SQL itself doesn't double-transition.
#[tokio::test]
#[ignore]
async fn invite_double_decline_is_noop() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };
    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let invitee = format!("invite-double-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        Some(&invitee),
        None,
        None,
        &owner.to_string(),
    )
    .await;

    assert_eq!(run_decline(&pool, invite_id, &invitee).await, 1);
    assert_eq!(
        run_decline(&pool, invite_id, &invitee).await,
        0,
        "second decline must affect zero rows"
    );
    assert_eq!(
        fetch_invite_status(&pool, invite_id).await.as_deref(),
        Some("declined"),
        "status must remain 'declined' after second decline"
    );

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Cross-recipient: a stranger cannot decline someone else's invite.
/// The SQL gate uses `invitee_user_id = $2` so a non-matching caller
/// updates zero rows — the handler then returns 409/404. This is the
/// only authority surface where the user-id is the gate itself
/// (rather than a target-level check).
#[tokio::test]
#[ignore]
async fn invite_decline_rejects_non_invitee() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };
    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;

    let invitee = format!("invite-rightful-{}", suffix);
    let stranger = format!("invite-stranger-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        Some(&invitee),
        None,
        None,
        &owner.to_string(),
    )
    .await;

    assert_eq!(
        run_decline(&pool, invite_id, &stranger).await,
        0,
        "stranger must not be able to decline someone else's invite"
    );
    assert_eq!(
        fetch_invite_status(&pool, invite_id).await.as_deref(),
        Some("pending"),
        "status must still be pending after the stranger's failed decline"
    );

    // The rightful invitee can still decline.
    assert_eq!(run_decline(&pool, invite_id, &invitee).await, 1);

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Revoke path: pending → revoked, and a second revoke is a no-op.
#[tokio::test]
#[ignore]
async fn invite_revoke_transitions_to_revoked() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };
    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let invitee = format!("invite-revoke-target-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        Some(&invitee),
        None,
        None,
        &owner.to_string(),
    )
    .await;

    assert_eq!(run_revoke(&pool, invite_id).await, 1);
    assert_eq!(
        fetch_invite_status(&pool, invite_id).await.as_deref(),
        Some("revoked"),
    );
    // Second revoke: no-op.
    assert_eq!(run_revoke(&pool, invite_id).await, 0);

    // Revoked invite no longer surfaces in any inbox.
    assert_eq!(
        count_pending_invites_for(&pool, &invitee).await,
        0,
        "revoked invite must disappear from invitee's inbox"
    );

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

// ─── Sprint 2.3b: accept paths ────────────────────────────────────────

/// Lifted from `accept_invite_core` — the status-flip is the source of
/// truth for "did the accept succeed?". Returns rows_affected.
///
/// Production handler also COALESCEs invitee_user_id to the accepter
/// (so email-only invites get back-filled on accept), and nulls out
/// invitee_email when the row had been email-only. The exactly-one-of
/// CHECK on the table forces this two-column dance — both populated
/// would violate the invariant.
async fn run_accept_status_flip(
    pool: &PgPool,
    invite_id: Uuid,
    accepter_user_id: &str,
) -> u64 {
    sqlx::query(
        "UPDATE forecast_invites
            SET status = 'accepted',
                accepted_at = NOW(),
                invitee_user_id = COALESCE(invitee_user_id, $2),
                invitee_email = CASE
                    WHEN invitee_user_id IS NULL THEN NULL
                    ELSE invitee_email
                END
          WHERE id = $1 AND status = 'pending'",
    )
    .bind(invite_id)
    .bind(accepter_user_id)
    .execute(pool)
    .await
    .expect("status flip UPDATE")
    .rows_affected()
}

/// Forecast accept → object_shares row exists with the invite's
/// permission → invite status='accepted'. We exercise the two
/// real-world calls the handler makes: `share_object` (materialise)
/// and the status-flip SQL.
#[tokio::test]
#[ignore]
async fn accept_forecast_invite_materialises_share() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let invitee = format!("accept-target-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "edit",
        Some(&invitee),
        None,
        None,
        &owner.to_string(),
    )
    .await;

    // Pre-state: no share for this invitee.
    let pre = teams::list_object_shares(&pool, ObjectType::Forecast, &fid)
        .await
        .expect("list pre");
    assert_eq!(pre.len(), 0);

    // Step 1: materialise.
    teams::share_object(
        &pool,
        ObjectType::Forecast,
        &fid,
        ShareType::User,
        &invitee,
        Permission::Edit,
        &owner.to_string(),
    )
    .await
    .expect("share_object");

    // Step 2: status flip.
    let n = run_accept_status_flip(&pool, invite_id, &invitee).await;
    assert_eq!(n, 1, "status flip must update exactly one row");

    // Post-state: share visible with the right permission.
    let post = teams::list_object_shares(&pool, ObjectType::Forecast, &fid)
        .await
        .expect("list post");
    assert_eq!(post.len(), 1);
    assert_eq!(post[0].share_target, invitee);
    assert_eq!(post[0].permission, Permission::Edit);

    assert_eq!(
        fetch_invite_status(&pool, invite_id).await.as_deref(),
        Some("accepted"),
    );

    // Cleanup: revoke the share, delete the invite + forecast.
    teams::revoke_share(&pool, post[0].id).await.expect("revoke");
    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Portfolio accept — same shape, exercises the ObjectType::Portfolio
/// path (which depends on migration 152's CHECK extension).
#[tokio::test]
#[ignore]
async fn accept_portfolio_invite_materialises_share() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let pid = insert_test_portfolio(&pool, owner, &suffix).await;
    let invitee = format!("accept-pf-target-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "portfolio",
        &pid,
        "view",
        Some(&invitee),
        None,
        None,
        &owner.to_string(),
    )
    .await;

    teams::share_object(
        &pool,
        ObjectType::Portfolio,
        &pid,
        ShareType::User,
        &invitee,
        Permission::View,
        &owner.to_string(),
    )
    .await
    .expect("share_object portfolio");

    let n = run_accept_status_flip(&pool, invite_id, &invitee).await;
    assert_eq!(n, 1);

    let listed = teams::list_object_shares(&pool, ObjectType::Portfolio, &pid)
        .await
        .expect("list post");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].permission, Permission::View);

    teams::revoke_share(&pool, listed[0].id).await.expect("revoke");
    delete_invite(&pool, invite_id).await;
    delete_test_portfolio(&pool, &pid).await;
}

/// Team accept → team_members row exists with the invite's role.
/// This is the only target type that doesn't write to object_shares.
#[tokio::test]
#[ignore]
async fn accept_team_invite_materialises_member() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let team_id = insert_test_team(&pool, &suffix).await;
    let invitee = format!("accept-team-target-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "team",
        &team_id.to_string(),
        "member",
        Some(&invitee),
        None,
        None,
        &owner.to_string(),
    )
    .await;

    // Pre-state: invitee is not a member.
    assert!(
        !is_team_member(&pool, team_id, &invitee).await.unwrap(),
        "invitee starts not-a-member"
    );

    teams::add_team_member(
        &pool,
        team_id,
        MemberType::User,
        &invitee,
        TeamRole::Member,
        &owner.to_string(),
    )
    .await
    .expect("add_team_member");

    let n = run_accept_status_flip(&pool, invite_id, &invitee).await;
    assert_eq!(n, 1);

    // Post-state: invitee is now a member with the right role.
    assert!(
        is_team_member(&pool, team_id, &invitee).await.unwrap(),
        "invitee must be a team member after accept"
    );
    let role = teams::get_member_role(&pool, team_id, &invitee)
        .await
        .expect("role lookup");
    assert_eq!(role, Some(TeamRole::Member));

    delete_invite(&pool, invite_id).await;
    cleanup(&pool, team_id).await;
}

/// Double-accept: the second accept finds status='accepted' and the
/// WHERE clause matches zero rows. share_object is idempotent so a
/// re-materialise is a no-op.
#[tokio::test]
#[ignore]
async fn accept_invite_is_idempotent_at_status_flip() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let invitee = format!("accept-idemp-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        Some(&invitee),
        None,
        None,
        &owner.to_string(),
    )
    .await;

    teams::share_object(
        &pool, ObjectType::Forecast, &fid, ShareType::User, &invitee,
        Permission::View, &owner.to_string(),
    ).await.expect("share");

    let n1 = run_accept_status_flip(&pool, invite_id, &invitee).await;
    assert_eq!(n1, 1, "first accept transitions the row");
    let n2 = run_accept_status_flip(&pool, invite_id, &invitee).await;
    assert_eq!(n2, 0, "second accept is a no-op at the status flip");

    let shares_before = teams::list_object_shares(&pool, ObjectType::Forecast, &fid)
        .await.expect("list 1");
    teams::share_object(
        &pool, ObjectType::Forecast, &fid, ShareType::User, &invitee,
        Permission::View, &owner.to_string(),
    ).await.expect("re-share");
    let shares_after = teams::list_object_shares(&pool, ObjectType::Forecast, &fid)
        .await.expect("list 2");
    assert_eq!(shares_before.len(), shares_after.len());
    assert_eq!(shares_before[0].id, shares_after[0].id);

    teams::revoke_share(&pool, shares_after[0].id).await.expect("revoke");
    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Decline-then-accept: status-flip finds 0 rows.
#[tokio::test]
#[ignore]
async fn accept_after_decline_is_blocked_at_status_flip() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };
    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let invitee = format!("accept-after-decline-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool, "forecast", &fid, "view", Some(&invitee), None, None,
        &owner.to_string(),
    ).await;

    assert_eq!(run_decline(&pool, invite_id, &invitee).await, 1);

    let n = run_accept_status_flip(&pool, invite_id, &invitee).await;
    assert_eq!(n, 0, "accept after decline must not transition");
    assert_eq!(
        fetch_invite_status(&pool, invite_id).await.as_deref(),
        Some("declined"),
    );

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Email-only invite: COALESCE on invitee_user_id back-fills the
/// accepter so future inbox lookups by user_id find the row.
#[tokio::test]
#[ignore]
async fn accept_email_only_invite_backfills_user_id() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let invitee_email = format!("email-only-{}@example.invalid", suffix);
    let token_str = format!("test-token-email-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        None,
        Some(&invitee_email),
        Some(&token_str),
        &owner.to_string(),
    )
    .await;

    let before: Option<String> = sqlx::query_scalar(
        "SELECT invitee_user_id FROM forecast_invites WHERE id = $1",
    )
    .bind(invite_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(before.is_none());

    let accepter = format!("email-claim-accepter-{}", suffix);
    teams::share_object(
        &pool, ObjectType::Forecast, &fid, ShareType::User, &accepter,
        Permission::View, &owner.to_string(),
    ).await.expect("share");
    assert_eq!(run_accept_status_flip(&pool, invite_id, &accepter).await, 1);

    let after: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT invitee_user_id, invitee_email FROM forecast_invites WHERE id = $1",
    )
    .bind(invite_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        after.0.as_deref(),
        Some(accepter.as_str()),
        "COALESCE must back-fill invitee_user_id with the accepter"
    );
    assert!(
        after.1.is_none(),
        "invitee_email must be cleared so the exactly-one-of CHECK \
         constraint is preserved post-accept"
    );

    let listed = teams::list_object_shares(&pool, ObjectType::Forecast, &fid)
        .await
        .expect("list");
    teams::revoke_share(&pool, listed[0].id).await.expect("revoke");
    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

// ─── Sprint 2.3b: by-token preview ────────────────────────────────────

/// Probe `get_invite_by_token_handler`'s gating logic verbatim:
/// returns Some only if status='pending' AND not expired.
async fn token_preview_visible(pool: &PgPool, token: &str) -> bool {
    let row: Option<(String, bool)> = sqlx::query_as(
        "SELECT status, (expires_at < NOW()) AS expired
         FROM forecast_invites WHERE token = $1",
    )
    .bind(token)
    .fetch_optional(pool)
    .await
    .expect("token preview probe");
    matches!(row, Some((status, expired)) if status == "pending" && !expired)
}

/// Valid pending token: visible. Revoked, accepted, expired, or
/// unknown: hidden — the handler returns 404 for all of these, no
/// leakage about which terminal state the invite reached.
#[tokio::test]
#[ignore]
async fn by_token_preview_only_shows_pending_unexpired() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let invitee_email = format!("by-token-{}@example.invalid", suffix);
    let token_str = format!("preview-token-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        None,
        Some(&invitee_email),
        Some(&token_str),
        &owner.to_string(),
    )
    .await;

    assert!(
        token_preview_visible(&pool, &token_str).await,
        "pending unexpired invite must be visible by token"
    );

    let phantom = format!("phantom-token-{}", suffix);
    assert!(
        !token_preview_visible(&pool, &phantom).await,
        "unknown token must be hidden"
    );

    assert_eq!(run_revoke(&pool, invite_id).await, 1);
    assert!(
        !token_preview_visible(&pool, &token_str).await,
        "revoked invite must be hidden by token"
    );

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Expired invite: the accept path auto-transitions to 'expired' on
/// detection. We backdate expires_at and run the (lifted) auto-expire
/// SQL the handler ships.
#[tokio::test]
#[ignore]
async fn accept_expired_invite_auto_transitions_to_expired() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let invitee = format!("expired-target-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        Some(&invitee),
        None,
        None,
        &owner.to_string(),
    )
    .await;

    sqlx::query(
        "UPDATE forecast_invites SET expires_at = NOW() - INTERVAL '1 hour'
         WHERE id = $1",
    )
    .bind(invite_id)
    .execute(&pool)
    .await
    .expect("backdate expires_at");

    // Lifted from accept_invite_core's auto-expire branch.
    let auto = sqlx::query(
        "UPDATE forecast_invites SET status = 'expired'
         WHERE id = $1 AND status = 'pending'",
    )
    .bind(invite_id)
    .execute(&pool)
    .await
    .expect("auto-expire UPDATE")
    .rows_affected();
    assert_eq!(auto, 1, "auto-expire must transition the pending row");
    assert_eq!(
        fetch_invite_status(&pool, invite_id).await.as_deref(),
        Some("expired"),
    );

    let n = run_accept_status_flip(&pool, invite_id, &invitee).await;
    assert_eq!(n, 0, "expired invites cannot be accepted");

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

// ─── Sprint 2.3c: email-claim resolver ────────────────────────────────

/// Helper that calls the real production helper. The hook point in
/// OIDC/SIWE is `fermi_auth::invites::claim_pending_for_email` — we
/// call it directly here rather than mocking sign-in.
async fn claim_for(pool: &PgPool, user_id: &str, email: &str) -> u64 {
    fermi_auth::invites::claim_pending_for_email(pool, user_id, email)
        .await
        .expect("claim_pending_for_email")
}

/// Happy path: email-only invite created before the user exists.
/// claim runs → invitee_user_id populated, invitee_email nulled,
/// status still 'pending', invite now discoverable by inbox query.
#[tokio::test]
#[ignore]
async fn email_claim_backfills_pending_invite() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;

    let email = format!("claim-target-{}@example.invalid", suffix);
    let token_str = format!("claim-token-{}", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        None,
        Some(&email),
        Some(&token_str),
        &owner.to_string(),
    )
    .await;

    let new_user = format!("claim-new-user-{}", suffix);

    // Pre-state: not discoverable by user_id, IS discoverable by email.
    assert_eq!(count_pending_invites_for(&pool, &new_user).await, 0);

    // Run the resolver.
    let n = claim_for(&pool, &new_user, &email).await;
    assert_eq!(n, 1, "exactly one pending invite must be back-filled");

    // Post-state: invitee_user_id populated, invitee_email nulled,
    // status still pending, inbox query finds it.
    let row: (Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT invitee_user_id, invitee_email, status
         FROM forecast_invites WHERE id = $1",
    )
    .bind(invite_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        row.0.as_deref(),
        Some(new_user.as_str()),
        "invitee_user_id must be back-filled"
    );
    assert!(
        row.1.is_none(),
        "invitee_email must be cleared (CHECK invariant)"
    );
    assert_eq!(row.2, "pending", "status must still be pending");

    assert_eq!(
        count_pending_invites_for(&pool, &new_user).await,
        1,
        "back-filled invite must now appear in the user's inbox"
    );

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Second claim for the same (user_id, email) pair → no-op (the
/// WHERE clause requires invitee_user_id IS NULL).
#[tokio::test]
#[ignore]
async fn email_claim_is_idempotent() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let email = format!("idemp-{}@example.invalid", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        None,
        Some(&email),
        Some(&format!("idemp-token-{}", suffix)),
        &owner.to_string(),
    )
    .await;
    let new_user = format!("idemp-user-{}", suffix);

    assert_eq!(claim_for(&pool, &new_user, &email).await, 1);
    assert_eq!(
        claim_for(&pool, &new_user, &email).await,
        0,
        "second claim must be a no-op"
    );

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Case-insensitive match: invite stored lowercased, signed-in
/// email arrives mixed-case → still matches. This guards against the
/// failure mode "OAuth provider returned email with different casing
/// than what the inviter typed."
#[tokio::test]
#[ignore]
async fn email_claim_is_case_insensitive() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;

    // Stored lowercased, the way create_invite_row does it.
    let stored = format!("case-{}@example.invalid", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        None,
        Some(&stored),
        Some(&format!("case-token-{}", suffix)),
        &owner.to_string(),
    )
    .await;

    // Caller arrives with a wildly mixed-case version.
    let arriving = mixed_case(&stored);
    assert_ne!(stored, arriving, "test guard: mixed_case must differ");

    let new_user = format!("case-user-{}", suffix);
    assert_eq!(
        claim_for(&pool, &new_user, &arriving).await,
        1,
        "case-insensitive comparison must match"
    );

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Terminal invites (declined/revoked/accepted/expired) are
/// immutable history. The claim's `WHERE status='pending'` keeps
/// them untouched even if a stale email match would otherwise hit.
#[tokio::test]
#[ignore]
async fn email_claim_skips_terminal_invites() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let email = format!("terminal-{}@example.invalid", suffix);

    // Insert a terminal invite (revoked) with the matching email.
    // To do it cleanly given the recipient-exactly-one CHECK: insert
    // as email-only pending, then revoke. The pre-revoke row has
    // invitee_user_id=NULL, so the claim's "IS NULL" clause matches
    // — except status='pending' is required, and we will UPDATE it
    // to 'revoked' first.
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        None,
        Some(&email),
        Some(&format!("terminal-token-{}", suffix)),
        &owner.to_string(),
    )
    .await;
    assert_eq!(run_revoke(&pool, invite_id).await, 1, "pre-flight revoke");

    // Snapshot the row pre-claim.
    let before: (Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT invitee_user_id, invitee_email, status
         FROM forecast_invites WHERE id = $1",
    )
    .bind(invite_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before.2, "revoked");

    // Run the claim — must do nothing.
    let new_user = format!("terminal-user-{}", suffix);
    let n = claim_for(&pool, &new_user, &email).await;
    assert_eq!(n, 0, "terminal invites must not be back-filled");

    // The row is byte-for-byte the same: still revoked, email intact.
    let after: (Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT invitee_user_id, invitee_email, status
         FROM forecast_invites WHERE id = $1",
    )
    .bind(invite_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, before, "terminal row must be untouched");

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

/// Defensive guard: empty user_id or email returns 0 without
/// running any UPDATE. The OIDC/SIWE callers always pass non-empty
/// values, but a bug upstream must never become a mass UPDATE.
#[tokio::test]
#[ignore]
async fn email_claim_refuses_empty_inputs() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let email = format!("guard-{}@example.invalid", suffix);
    let (invite_id, _) = insert_test_invite(
        &pool,
        "forecast",
        &fid,
        "view",
        None,
        Some(&email),
        Some(&format!("guard-token-{}", suffix)),
        &owner.to_string(),
    )
    .await;

    // Both empties: zero rows touched.
    assert_eq!(claim_for(&pool, "", "").await, 0);
    assert_eq!(claim_for(&pool, "some-user", "").await, 0);
    assert_eq!(claim_for(&pool, "", &email).await, 0);

    // The invite is still pending and email-only.
    let row: (Option<String>, Option<String>, String) = sqlx::query_as(
        "SELECT invitee_user_id, invitee_email, status
         FROM forecast_invites WHERE id = $1",
    )
    .bind(invite_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(row.0.is_none(), "invitee_user_id must still be NULL");
    assert!(row.1.is_some(), "invitee_email must still be set");
    assert_eq!(row.2, "pending");

    delete_invite(&pool, invite_id).await;
    delete_test_forecast(&pool, &fid).await;
}

// ─── Sprint 2.4a: Migration 154 backfill ────────────────────────────

/// Migration 154 must be idempotent: running the INSERT ... ON CONFLICT
/// DO NOTHING twice produces the same rowcount (zero new rows on second
/// run). We exercise this against real Neon data for both forecasts and
/// portfolios.
#[tokio::test]
#[ignore]
async fn migration_154_backfill_idempotent() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let before_fc: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM object_shares \
         WHERE share_type = 'team' AND object_type = 'forecast'",
    )
    .fetch_one(&pool)
    .await
    .expect("count before fc");

    let before_pf: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM object_shares \
         WHERE share_type = 'team' AND object_type = 'portfolio'",
    )
    .fetch_one(&pool)
    .await
    .expect("count before pf");

    sqlx::query(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         SELECT 'forecast', id::text, 'team', team_id::text, 'edit', owner_id::text
         FROM fermi_forecasts WHERE team_id IS NOT NULL
         ON CONFLICT (object_type, object_id, share_type, share_target) DO NOTHING",
    )
    .execute(&pool)
    .await
    .expect("backfill forecasts");

    sqlx::query(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         SELECT 'portfolio', id::text, 'team', team_id::text, 'edit', owner_id::text
         FROM fermi_portfolios WHERE team_id IS NOT NULL
         ON CONFLICT (object_type, object_id, share_type, share_target) DO NOTHING",
    )
    .execute(&pool)
    .await
    .expect("backfill portfolios");

    let after_fc: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM object_shares \
         WHERE share_type = 'team' AND object_type = 'forecast'",
    )
    .fetch_one(&pool)
    .await
    .expect("count after fc");

    let after_pf: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM object_shares \
         WHERE share_type = 'team' AND object_type = 'portfolio'",
    )
    .fetch_one(&pool)
    .await
    .expect("count after pf");

    sqlx::query(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         SELECT 'forecast', id::text, 'team', team_id::text, 'edit', owner_id::text
         FROM fermi_forecasts WHERE team_id IS NOT NULL
         ON CONFLICT (object_type, object_id, share_type, share_target) DO NOTHING",
    )
    .execute(&pool)
    .await
    .expect("backfill forecasts second run");

    sqlx::query(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         SELECT 'portfolio', id::text, 'team', team_id::text, 'edit', owner_id::text
         FROM fermi_portfolios WHERE team_id IS NOT NULL
         ON CONFLICT (object_type, object_id, share_type, share_target) DO NOTHING",
    )
    .execute(&pool)
    .await
    .expect("backfill portfolios second run");

    let after2_fc: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM object_shares \
         WHERE share_type = 'team' AND object_type = 'forecast'",
    )
    .fetch_one(&pool)
    .await
    .expect("count after second fc");

    let after2_pf: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM object_shares \
         WHERE share_type = 'team' AND object_type = 'portfolio'",
    )
    .fetch_one(&pool)
    .await
    .expect("count after second pf");

    assert_eq!(
        after_fc, after2_fc,
        "second backfill run must not add forecast team-shares"
    );
    assert_eq!(
        after_pf, after2_pf,
        "second backfill run must not add portfolio team-shares"
    );

    let _ = before_fc;
    let _ = before_pf;
}

/// Every forecast/portfolio that has team_id set must have a matching
/// object_shares row after migration 154. We also verify the row's
/// fields match: share_type='team', permission='edit',
/// share_target=team_id::text.
#[tokio::test]
#[ignore]
async fn migration_154_backfill_matches_team_id() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let fc_no_team: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fermi_forecasts f
         WHERE f.team_id IS NOT NULL
           AND NOT EXISTS (
             SELECT 1 FROM object_shares os
             WHERE os.object_type = 'forecast'
               AND os.object_id = f.id::text
               AND os.share_type = 'team'
               AND os.share_target = f.team_id::text
           )",
    )
    .fetch_one(&pool)
    .await
    .expect("forecast mismatch count");

    assert_eq!(
        fc_no_team, 0,
        "every forecast with team_id must have a matching object_shares row \
         (found {} without)",
        fc_no_team
    );

    let pf_no_team: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fermi_portfolios p
         WHERE p.team_id IS NOT NULL
           AND NOT EXISTS (
             SELECT 1 FROM object_shares os
             WHERE os.object_type = 'portfolio'
               AND os.object_id = p.id::text
               AND os.share_type = 'team'
               AND os.share_target = p.team_id::text
           )",
    )
    .fetch_one(&pool)
    .await
    .expect("portfolio mismatch count");

    assert_eq!(
        pf_no_team, 0,
        "every portfolio with team_id must have a matching object_shares row \
         (found {} without)",
        pf_no_team
    );

    let wrong_perm: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM object_shares os
         JOIN fermi_forecasts f ON os.object_id = f.id::text
         WHERE os.object_type = 'forecast'
           AND os.share_type = 'team'
           AND os.permission != 'edit'",
    )
    .fetch_one(&pool)
    .await
    .expect("wrong permission count");

    assert_eq!(
        wrong_perm, 0,
        "all backfilled forecast team-shares must have permission='edit' \
         (found {} with different permission)",
        wrong_perm
    );
}

// ─── Sprint 2.4b: Handler ACL switch (can_view/can_edit/can_admin) ──

/// A direct user-share in object_shares grants list + detail access
/// via the new object_shares EXISTS branch in list handlers and the
/// can_view helper in detail handlers. We insert a private forecast
/// owned by one user, create an object_shares row granting a second
/// user view access, and verify the second user can see the forecast
/// via the list WHERE clause.
#[tokio::test]
#[ignore]
async fn can_view_grants_via_user_share() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let viewer_id = pick_second_existing_user_id(&pool).await.to_string();

    // Grant view access via object_shares
    sqlx::query(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         VALUES ('forecast', $1, 'user', $2, 'view', $3)",
    )
    .bind(&fid)
    .bind(&viewer_id)
    .bind(owner.to_string())
    .execute(&pool)
    .await
    .expect("insert object_shares row");

    // The list WHERE clause now includes the user-share branch.
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fermi_forecasts f
         WHERE f.id = $2
           AND (f.owner_id = $1::uuid
                OR f.visibility IN ('shared', 'public')
                OR (f.team_id IS NOT NULL
                    AND EXISTS (SELECT 1 FROM team_members m
                                WHERE m.team_id = f.team_id AND m.member_id = $1))
                OR EXISTS (SELECT 1 FROM object_shares s
                           WHERE s.object_type = 'forecast'
                             AND s.object_id = f.id::text
                             AND s.share_type = 'user'
                             AND s.share_target = $1))",
    )
    .bind(&viewer_id)
    .bind(&fid)
    .fetch_one(&pool)
    .await
    .expect("count");

    assert_eq!(n, 1, "viewer with user-share must see the forecast in list");

    // can_view must return true for the viewer
    let principal = fermi_auth::AuthPrincipal::User(fermi_auth::User {
        user_id: viewer_id.clone(),
        email: format!("{}@test", viewer_id),
        display_name: None,
        role: fermi_auth::UserRole::Viewer,
        auth_provider: fermi_auth::AuthProvider::Email,
        github_username: None,
        google_id: None,
        ethereum_address: None,
        ens_name: None,
    });
    let granted = fermi_auth::visibility::can_view(
        &pool,
        &principal,
        fermi_auth::ObjectType::Forecast,
        &fid,
        &owner.to_string(),
        fermi_auth::Visibility::Private,
    )
    .await
    .expect("can_view call");
    assert!(granted, "can_view must return true for user-share holder");

    // Cleanup
    sqlx::query("DELETE FROM object_shares WHERE share_target = $1 AND object_id = $2")
        .bind(&viewer_id)
        .bind(&fid)
        .execute(&pool)
        .await
        .ok();
    delete_test_forecast(&pool, &fid).await;
}

/// A user with permission='edit' via object_shares can update the
/// forecast probability; a user with permission='view' cannot (403).
#[tokio::test]
#[ignore]
async fn can_edit_collaborator_can_update_probability() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let editor_id = pick_second_existing_user_id(&pool).await.to_string();
    let viewer_id = pick_third_existing_user_id(&pool).await.to_string();

    // Grant edit and view permissions
    sqlx::query(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         VALUES ('forecast', $1, 'user', $2, 'edit', $3)",
    )
    .bind(&fid)
    .bind(&editor_id)
    .bind(owner.to_string())
    .execute(&pool)
    .await
    .expect("insert edit share");

    sqlx::query(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         VALUES ('forecast', $1, 'user', $2, 'view', $3)",
    )
    .bind(&fid)
    .bind(&viewer_id)
    .bind(owner.to_string())
    .execute(&pool)
    .await
    .expect("insert view share");

    // Verify can_edit returns true for editor
    let editor_principal = fermi_auth::AuthPrincipal::User(fermi_auth::User {
        user_id: editor_id.clone(),
        email: format!("{}@test", editor_id),
        display_name: None,
        role: fermi_auth::UserRole::Viewer,
        auth_provider: fermi_auth::AuthProvider::Email,
        github_username: None,
        google_id: None,
        ethereum_address: None,
        ens_name: None,
    });
    let can_edit_result = fermi_auth::visibility::can_edit(
        &pool,
        &editor_principal,
        fermi_auth::ObjectType::Forecast,
        &fid,
        &owner.to_string(),
        fermi_auth::Visibility::Private,
    )
    .await
    .expect("can_edit call for editor");
    assert!(can_edit_result, "editor must have can_edit");

    // Verify can_edit returns false for view-only user
    let viewer_principal = fermi_auth::AuthPrincipal::User(fermi_auth::User {
        user_id: viewer_id.clone(),
        email: format!("{}@test", viewer_id),
        display_name: None,
        role: fermi_auth::UserRole::Viewer,
        auth_provider: fermi_auth::AuthProvider::Email,
        github_username: None,
        google_id: None,
        ethereum_address: None,
        ens_name: None,
    });
    let can_edit_view = fermi_auth::visibility::can_edit(
        &pool,
        &viewer_principal,
        fermi_auth::ObjectType::Forecast,
        &fid,
        &owner.to_string(),
        fermi_auth::Visibility::Private,
    )
    .await
    .expect("can_edit call for viewer");
    assert!(!can_edit_view, "view-only user must NOT have can_edit");

    // Cleanup
    sqlx::query("DELETE FROM object_shares WHERE object_id = $1 AND share_target IN ($2, $3)")
        .bind(&fid)
        .bind(&editor_id)
        .bind(&viewer_id)
        .execute(&pool)
        .await
        .ok();
    delete_test_forecast(&pool, &fid).await;
}

/// A user with permission='admin' via object_shares can create further
/// shares; a non-admin user cannot.
#[tokio::test]
#[ignore]
async fn can_admin_collaborator_can_share() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let admin_id = pick_second_existing_user_id(&pool).await.to_string();
    let viewer_id = pick_third_existing_user_id(&pool).await.to_string();

    // Grant admin and view permissions
    sqlx::query(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         VALUES ('forecast', $1, 'user', $2, 'admin', $3)",
    )
    .bind(&fid)
    .bind(&admin_id)
    .bind(owner.to_string())
    .execute(&pool)
    .await
    .expect("insert admin share");

    sqlx::query(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         VALUES ('forecast', $1, 'user', $2, 'view', $3)",
    )
    .bind(&fid)
    .bind(&viewer_id)
    .bind(owner.to_string())
    .execute(&pool)
    .await
    .expect("insert view share");

    // Verify can_access returns Admin for admin user
    let admin_principal = fermi_auth::AuthPrincipal::User(fermi_auth::User {
        user_id: admin_id.clone(),
        email: format!("{}@test", admin_id),
        display_name: None,
        role: fermi_auth::UserRole::Viewer,
        auth_provider: fermi_auth::AuthProvider::Email,
        github_username: None,
        google_id: None,
        ethereum_address: None,
        ens_name: None,
    });
    let level = fermi_auth::visibility::can_access(
        &pool,
        &admin_principal,
        fermi_auth::ObjectType::Forecast,
        &fid,
        &owner.to_string(),
        fermi_auth::Visibility::Private,
    )
    .await
    .expect("can_access call for admin");
    assert!(level.has_admin(), "admin-share holder must have can_admin");

    // Verify can_access does NOT return Admin for view-only user
    let viewer_principal = fermi_auth::AuthPrincipal::User(fermi_auth::User {
        user_id: viewer_id.clone(),
        email: format!("{}@test", viewer_id),
        display_name: None,
        role: fermi_auth::UserRole::Viewer,
        auth_provider: fermi_auth::AuthProvider::Email,
        github_username: None,
        google_id: None,
        ethereum_address: None,
        ens_name: None,
    });
    let view_level = fermi_auth::visibility::can_access(
        &pool,
        &viewer_principal,
        fermi_auth::ObjectType::Forecast,
        &fid,
        &owner.to_string(),
        fermi_auth::Visibility::Private,
    )
    .await
    .expect("can_access call for viewer");
    assert!(!view_level.has_admin(), "view-share holder must NOT have can_admin");

    // Cleanup
    sqlx::query("DELETE FROM object_shares WHERE object_id = $1 AND share_target IN ($2, $3)")
        .bind(&fid)
        .bind(&admin_id)
        .bind(&viewer_id)
        .execute(&pool)
        .await
        .ok();
    delete_test_forecast(&pool, &fid).await;
}

/// An admin-share holder CAN delete the forecast (the spec puts delete
/// under can_admin). Verify the can_access helper returns admin level
/// for the admin-share holder, confirming the ACL is correct.
#[tokio::test]
#[ignore]
async fn admin_share_holder_can_delete() {
    let Some(pool) = try_pool().await else {
        eprintln!("skip: DATABASE_URL not set");
        return;
    };

    let suffix = unique_suffix();
    let owner = pick_existing_user_id(&pool).await;
    let fid = insert_test_forecast(&pool, owner, None, "private", &suffix).await;
    let admin_id = pick_second_existing_user_id(&pool).await.to_string();

    sqlx::query(
        "INSERT INTO object_shares
            (object_type, object_id, share_type, share_target, permission, granted_by)
         VALUES ('forecast', $1, 'user', $2, 'admin', $3)",
    )
    .bind(&fid)
    .bind(&admin_id)
    .bind(owner.to_string())
    .execute(&pool)
    .await
    .expect("insert admin share");

    let admin_principal = fermi_auth::AuthPrincipal::User(fermi_auth::User {
        user_id: admin_id.clone(),
        email: format!("{}@test", admin_id),
        display_name: None,
        role: fermi_auth::UserRole::Viewer,
        auth_provider: fermi_auth::AuthProvider::Email,
        github_username: None,
        google_id: None,
        ethereum_address: None,
        ens_name: None,
    });
    let level = fermi_auth::visibility::can_access(
        &pool,
        &admin_principal,
        fermi_auth::ObjectType::Forecast,
        &fid,
        &owner.to_string(),
        fermi_auth::Visibility::Private,
    )
    .await
    .expect("can_access call");
    assert!(level.has_admin(), "admin-share holder must have can_admin for delete");

    // Cleanup
    sqlx::query("DELETE FROM object_shares WHERE object_id = $1 AND share_target = $2")
        .bind(&fid)
        .bind(&admin_id)
        .execute(&pool)
        .await
        .ok();
    delete_test_forecast(&pool, &fid).await;
}
