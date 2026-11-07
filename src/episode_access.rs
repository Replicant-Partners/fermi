//! Who may read an episode (SPEC 36 R0.2 item 4, invariant I9).
//!
//! Before: `/api/episodes/:id`, `/lineage`, `/api/episodes/recent`,
//! `/api/stream`, `/api/specimen/:agent` and agent metrics sat on the public
//! router and served episode queries, tool calls and errors to signed-out
//! callers, including episodes of private agents; `/trace` served any episode to
//! any signed-in user [V: probed on the dev prototype at a3cea516 and 4c9bac67,
//! `tests/claims/episode_read_tenancy.rs`].
//!
//! The rule, in one place so every route agrees. A caller may read an episode
//! when any of these hold:
//!
//! 1. the caller is a platform admin;
//! 2. the caller owns the agent that ran it (`agents.user_id`);
//! 3. the caller is the human the agent was talking to — the dyad's human
//!    (`episodes.dyad_id = dyad:<agent>:<user>` or `eval:<agent>:<user>`) or
//!    the A2A caller (`source_ref.caller_user_id`);
//! 4. the episode belongs to a workspace the caller is a member of
//!    (`episodes.workspace_id` or `source_ref.workspace_id`).
//!
//! Anyone else gets 404 for a by-ID read, so the route is not an oracle for
//! which ids exist. Bulk reads apply the same predicate as a SQL filter.
//!
//! The predicate is written once, as SQL, so the by-ID check and the list
//! filters cannot drift apart.

use uuid::Uuid;

/// The access predicate over `episodes e JOIN agents a`, with `$caller` and
/// `$admin` placeholders substituted by [`predicate`].
const PREDICATE: &str = "({caller} <> '' AND (
    {admin}
    OR a.user_id = {caller}
    OR e.dyad_id IN ('dyad:' || e.agent_id || ':' || {caller}, 'eval:' || e.agent_id || ':' || {caller})
    OR e.source_ref->>'caller_user_id' = {caller}
    OR EXISTS (
        SELECT 1 FROM team_members tm
         WHERE tm.member_id = {caller}
           AND tm.member_type = 'user'
           AND tm.team_id = COALESCE(
                 e.workspace_id,
                 CASE WHEN e.source_ref->>'workspace_id'
                           ~* '^[0-9a-f]{8}-([0-9a-f]{4}-){3}[0-9a-f]{12}$'
                      THEN (e.source_ref->>'workspace_id')::uuid END)
    )
))";

/// The predicate with the caller bound to placeholder `$n` (text) and admin to
/// `$m` (bool). Use inside a query that aliases `episodes e JOIN agents a`.
pub fn predicate(caller_param: usize, admin_param: usize) -> String {
    PREDICATE
        .replace("{caller}", &format!("${caller_param}"))
        .replace("{admin}", &format!("${admin_param}"))
}

/// The principal facts the rule needs. `None` = not signed in, which can read
/// nothing through these routes.
#[derive(Debug, Clone)]
pub struct Reader {
    pub user_id: String,
    pub admin: bool,
}

impl Reader {
    pub fn from_principal(p: &fermi_auth::AuthPrincipal) -> Self {
        Reader {
            user_id: p.user_id(),
            admin: p.can_admin(),
        }
    }
}

/// May `reader` read `episode_id`? `false` also when the episode does not
/// exist, so callers answer 404 either way.
pub async fn may_read(
    conn: &mut sqlx::PgConnection,
    episode_id: Uuid,
    reader: &Reader,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(&format!(
        "SELECT EXISTS (
            SELECT 1 FROM episodes e JOIN agents a ON a.agent_id = e.agent_id
             WHERE e.episode_id = $1 AND {}
         )",
        predicate(2, 3)
    ))
    .bind(episode_id)
    .bind(&reader.user_id)
    .bind(reader.admin)
    .fetch_one(&mut *conn)
    .await
}

/// May `reader` read a workspace's flow? Admins, and members of the workspace.
pub async fn may_read_workspace(
    conn: &mut sqlx::PgConnection,
    workspace_id: Uuid,
    reader: &Reader,
) -> Result<bool, sqlx::Error> {
    if reader.admin {
        return Ok(true);
    }
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM team_members
                         WHERE team_id = $1 AND member_id = $2 AND member_type = 'user')",
    )
    .bind(workspace_id)
    .bind(&reader.user_id)
    .fetch_one(&mut *conn)
    .await
}

/// May `reader` see an agent's eval runs and per-agent episode list? Admins,
/// the agent's owner, and anyone for a public published agent (eval scores of
/// public agents are catalogue data; episode lists are additionally filtered
/// row by row with [`predicate`]).
pub async fn may_read_agent_evals(
    conn: &mut sqlx::PgConnection,
    agent_id: Uuid,
    reader: Option<&Reader>,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM agents a WHERE a.agent_id = $1
               AND ($3 OR a.user_id = $2
                    OR (a.visibility = 'public' AND a.status = 'published'))
         )",
    )
    .bind(agent_id)
    .bind(reader.map(|r| r.user_id.as_str()))
    .bind(reader.map(|r| r.admin).unwrap_or(false))
    .fetch_one(&mut *conn)
    .await
}

/// The 404 every guarded by-ID route returns, so status and text cannot drift.
pub fn not_found() -> (axum::http::StatusCode, String) {
    (
        axum::http::StatusCode::NOT_FOUND,
        "Episode not found".to_string(),
    )
}

fn internal(e: sqlx::Error) -> (axum::http::StatusCode, String) {
    tracing::error!(error = %e, "episode access check failed");
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "access check failed".to_string(),
    )
}

/// `Ok(())` iff the principal may read `episode_id`; otherwise the shared 404.
/// Signed-out callers never reach this: the `AuthPrincipal` extractor answers
/// 401 when no principal was set by the auth middleware
/// (fermi-auth/src/middleware.rs, `impl FromRequestParts for AuthPrincipal`).
pub async fn require_readable(
    pool: &sqlx::PgPool,
    principal: &fermi_auth::AuthPrincipal,
    episode_id: Uuid,
) -> Result<(), (axum::http::StatusCode, String)> {
    let mut conn = pool.acquire().await.map_err(internal)?;
    if may_read(&mut conn, episode_id, &Reader::from_principal(principal))
        .await
        .map_err(internal)?
    {
        Ok(())
    } else {
        Err(not_found())
    }
}

/// `Ok(())` iff the reader (or a signed-out caller, `None`) may see the
/// agent's eval data; otherwise 404.
pub async fn require_agent_evals(
    pool: &sqlx::PgPool,
    principal: Option<&fermi_auth::AuthPrincipal>,
    agent_id: Uuid,
) -> Result<(), (axum::http::StatusCode, String)> {
    let mut conn = pool.acquire().await.map_err(internal)?;
    let reader = principal.map(Reader::from_principal);
    if may_read_agent_evals(&mut conn, agent_id, reader.as_ref())
        .await
        .map_err(internal)?
    {
        Ok(())
    } else {
        Err((
            axum::http::StatusCode::NOT_FOUND,
            "Agent not found".to_string(),
        ))
    }
}

/// The reader for list filters: a signed-out caller reads nothing
/// (`user_id` that matches no row, `admin = false`).
pub fn reader_or_nobody(principal: Option<&fermi_auth::AuthPrincipal>) -> Reader {
    principal.map(Reader::from_principal).unwrap_or(Reader {
        user_id: String::new(),
        admin: false,
    })
}
