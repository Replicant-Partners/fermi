//! Forecast ACL regression tests (Spec 24 §3.2 Wave 1).
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
//!      pointed at their team. This test asserts the canonical helper
//!      `fermi_auth::visibility::is_team_member` (which the handler now
//!      delegates to) returns `true` for an actual team member and `false`
//!      for a stranger.
//!
//! Tests that need a live DB are marked `#[ignore]` so a vanilla
//! `cargo test` passes without one.

use std::str::FromStr;
use std::time::Duration;

use sqlx::postgres::PgConnectOptions;
use sqlx::PgPool;
use uuid::Uuid;

use fermi_auth::visibility::is_team_member;

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
