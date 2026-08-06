use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AuthError;
use crate::types::{AuthPrincipal, ObjectType, Permission, Visibility};

/// Result of an access check — either denied or a specific permission level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessLevel {
    Denied,
    Granted(Permission),
}

impl AccessLevel {
    pub fn is_denied(&self) -> bool {
        matches!(self, AccessLevel::Denied)
    }

    pub fn has_view(&self) -> bool {
        matches!(self, AccessLevel::Granted(_))
    }

    pub fn has_edit(&self) -> bool {
        matches!(
            self,
            AccessLevel::Granted(Permission::Edit) | AccessLevel::Granted(Permission::Admin)
        )
    }

    pub fn has_admin(&self) -> bool {
        matches!(self, AccessLevel::Granted(Permission::Admin))
    }
}

/// Check what permission level a principal has on an object.
///
/// Priority chain:
/// 1. System admins → Admin
/// 2. Owner → Admin
/// 3. Public visibility → View
/// 4. Direct user share in object_shares → share's permission
/// 5. Team share (via team_members membership) → share's permission
/// 5b. **Forecasts only:** inherited from a shared portfolio that
///     contains the forecast (Spec 26 §2) → the portfolio share's
///     permission
/// 6. Deny
///
/// Step 5b is the whole reason this function is the single canonical
/// gate: "share a portfolio with a team and its forecasts come along"
/// has to hold identically in `get_forecast_handler`, the update/resolve
/// paths, `shares.rs`'s guards, polymarket linking and invite
/// materialisation. Implementing it here rather than at ~13 call sites
/// means no handler can drift out of agreement with the others.
///
/// It runs last on purpose — only after admin/owner/public/user-share/
/// team-share have all missed — so the hot paths pay nothing for it.
pub async fn can_access(
    pool: &PgPool,
    principal: &AuthPrincipal,
    object_type: ObjectType,
    object_id: &str,
    owner_id: &str,
    visibility: Visibility,
) -> Result<AccessLevel, AuthError> {
    let user_id = principal.user_id();

    // 1. System admins → Admin
    if principal.can_admin() {
        return Ok(AccessLevel::Granted(Permission::Admin));
    }

    // 2. Owner → Admin
    if user_id == owner_id {
        return Ok(AccessLevel::Granted(Permission::Admin));
    }

    // 3. Public visibility → View
    if visibility == Visibility::Public {
        return Ok(AccessLevel::Granted(Permission::View));
    }

    // 4. Direct user share
    let direct = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT permission FROM object_shares
        WHERE object_type = $1 AND object_id = $2
          AND share_type = 'user' AND share_target = $3
        "#,
    )
    .bind(object_type.as_str())
    .bind(object_id)
    .bind(&user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    if let Some(row) = direct {
        return Ok(AccessLevel::Granted(Permission::from_str(&row.0)));
    }

    // 5. Team share — find highest permission across all teams the user belongs to
    let team_perm = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT os.permission
        FROM object_shares os
        JOIN team_members tm ON os.share_target = tm.team_id::text
        WHERE os.object_type = $1 AND os.object_id = $2
          AND os.share_type = 'team'
          AND tm.member_id = $3
        ORDER BY CASE os.permission
            WHEN 'admin' THEN 3
            WHEN 'edit'  THEN 2
            WHEN 'view'  THEN 1
        END DESC
        LIMIT 1
        "#,
    )
    .bind(object_type.as_str())
    .bind(object_id)
    .bind(&user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    if let Some(row) = team_perm {
        return Ok(AccessLevel::Granted(Permission::from_str(&row.0)));
    }

    // 5b. Portfolio-inherited access (Spec 26 §2). Forecasts only — a
    //     portfolio doesn't inherit from anything.
    if object_type == ObjectType::Forecast {
        if let Some(inherited) = forecast_inherited_access(pool, object_id, &user_id).await? {
            return Ok(AccessLevel::Granted(inherited.permission));
        }
    }

    // 6. Deny
    Ok(AccessLevel::Denied)
}

/// One forecast's access inherited from a portfolio that contains it.
/// Carries the source portfolio so callers can render provenance
/// ("via portfolio ‹WC 2026›") rather than an unexplained grant.
#[derive(Debug, Clone)]
pub struct InheritedAccess {
    pub permission: Permission,
    pub portfolio_id: String,
    pub portfolio_title: String,
    /// The team the enabling share targets, when it was a team share.
    pub team_id: Option<String>,
}

/// Resolve portfolio-inherited access for one forecast (Spec 26 §2).
///
/// Returns the *strongest* inherited grant, or `None`.
///
/// ## The leak guard
///
/// A portfolio is a curation surface: I can add someone else's forecast
/// to my portfolio. Sharing my portfolio must not re-share their private
/// work. So a share on portfolio P only reaches forecast F when either:
///
/// * **(a)** `F.owner_id = P.owner_id` — the ordinary "my portfolio of my
///   forecasts" case, or
/// * **(b)** the enabling share is a *team* share and `F.owner_id` is a
///   member of that same team — joint team work in a team portfolio.
///
/// Without (a)/(b), adding a colleague's private forecast to a portfolio
/// and sharing it would be a privilege-escalation primitive.
///
/// ## Enabling shares
///
/// Three ways the caller can reach the portfolio, all folded into one
/// query via the `src` union:
///   * a direct user share on the portfolio,
///   * a team share on the portfolio where the caller is a member,
///   * the portfolio being team-*owned* (`fermi_portfolios.team_id`) with
///     the caller a member. Migration 154 backfilled `object_shares`
///     rows for existing `team_id` values, but the column remains the
///     primary pointer and new team-owned portfolios may only set it, so
///     we read both rather than trusting the backfill to stay current.
///
/// Portfolio `visibility IN ('shared','public')` is deliberately NOT an
/// enabling path: those already grant the forecast list broad access via
/// the forecast's own visibility, and treating "discoverable" as
/// "inherits edit" would be a surprise.
pub async fn forecast_inherited_access(
    pool: &PgPool,
    forecast_id: &str,
    user_id: &str,
) -> Result<Option<InheritedAccess>, AuthError> {
    let ids = vec![forecast_id.to_string()];
    let row = sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
        &inherited_access_by_ids_sql(),
    )
    .bind(&ids)
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(row.map(
        |(_fid, permission, portfolio_id, portfolio_title, team_id)| InheritedAccess {
            permission: Permission::from_str(&permission),
            portfolio_id,
            portfolio_title,
            team_id,
        },
    ))
}

/// **The** portfolio-inheritance relation (Spec 26 §2) — single source of
/// truth for a rule that three very different queries need:
///
/// | consumer | how |
/// |---|---|
/// | the ACL ([`forecast_inherited_access`], via `can_access`) | filtered to one id |
/// | the provenance resolver (`fermi::handlers::collab`) | filtered to a page of ids |
/// | the list `WHERE` clause (`list_forecasts_handler`) | unfiltered, as `f.id IN (…)` |
///
/// Duplicating the rule per consumer is how ACLs and the UI that
/// explains them drift apart, so instead this is a self-contained SELECT
/// with one placeholder token, [`USER_PLACEHOLDER`], that each consumer
/// substitutes for its own bind position via
/// [`inherited_access_relation`]. A token rather than `$1` because the
/// list handler builds its bind indices dynamically and cannot control
/// what number the user_id lands on.
///
/// Yields `(forecast_id, permission, portfolio_id, portfolio_title,
/// team_id)` — one row per (forecast, enabling share), duplicates and
/// all. Consumers that need the *strongest* grant per forecast wrap it
/// with `DISTINCT ON`; see [`INHERITED_ACCESS_BY_IDS_SQL`].
pub const INHERITED_ACCESS_RELATION_SQL: &str = r#"
SELECT src.forecast_id,
       src.permission,
       src.portfolio_id,
       src.portfolio_title,
       src.team_id
FROM (
    -- (i) direct user share, or team share the caller belongs to, on a
    --     portfolio containing the forecast.
    SELECT pf.forecast_id                           AS forecast_id,
           os.permission                            AS permission,
           p.id::text                               AS portfolio_id,
           p.title                                  AS portfolio_title,
           CASE WHEN os.share_type = 'team'
                THEN os.share_target END            AS team_id,
           os.share_type                            AS share_type,
           os.share_target                          AS share_target,
           f.owner_id::text                         AS forecast_owner,
           p.owner_id::text                         AS portfolio_owner
    FROM fermi_portfolio_forecasts pf
    JOIN fermi_portfolios p ON p.id = pf.portfolio_id
    JOIN fermi_forecasts   f ON f.id = pf.forecast_id
    JOIN object_shares    os ON os.object_type = 'portfolio'
                            AND os.object_id   = p.id::text
    WHERE (
            (os.share_type = 'user' AND os.share_target = {USER})
         OR (os.share_type = 'team' AND EXISTS (
                SELECT 1 FROM team_members tm
                WHERE tm.team_id::text = os.share_target
                  AND tm.member_id     = {USER}))
          )

    UNION ALL

    -- (ii) the portfolio is team-OWNED and the caller is a member.
    --      Team ownership implies joint management, so 'edit'.
    SELECT pf.forecast_id                           AS forecast_id,
           'edit'                                   AS permission,
           p.id::text                               AS portfolio_id,
           p.title                                  AS portfolio_title,
           p.team_id::text                          AS team_id,
           'team'                                   AS share_type,
           p.team_id::text                          AS share_target,
           f.owner_id::text                         AS forecast_owner,
           p.owner_id::text                         AS portfolio_owner
    FROM fermi_portfolio_forecasts pf
    JOIN fermi_portfolios p ON p.id = pf.portfolio_id
    JOIN fermi_forecasts   f ON f.id = pf.forecast_id
    WHERE p.team_id IS NOT NULL
      AND EXISTS (SELECT 1 FROM team_members tm
                  WHERE tm.team_id = p.team_id AND tm.member_id = {USER})
) src
-- The leak guard (Spec 26 §2.1): a portfolio share only reaches
-- forecasts that are in-scope for it. Without this, adding a
-- colleague's private forecast to a portfolio and sharing the portfolio
-- would be a privilege-escalation primitive.
WHERE (
        src.forecast_owner = src.portfolio_owner
     OR (src.share_type = 'team' AND EXISTS (
            SELECT 1 FROM team_members tm2
            WHERE tm2.team_id::text = src.share_target
              AND tm2.member_id     = src.forecast_owner))
      )
"#;

/// The token [`INHERITED_ACCESS_RELATION_SQL`] uses wherever the caller's
/// `user_id` bind belongs.
pub const USER_PLACEHOLDER: &str = "{USER}";

/// Materialise [`INHERITED_ACCESS_RELATION_SQL`] with `user_id` bound at
/// `$n`.
///
/// `n` is a bind position, never user input — all three call sites pass
/// a literal or a locally-computed index — so there is no injection
/// surface here despite the string substitution.
pub fn inherited_access_relation(n: u32) -> String {
    INHERITED_ACCESS_RELATION_SQL.replace(USER_PLACEHOLDER, &format!("${}", n))
}

/// [`INHERITED_ACCESS_RELATION_SQL`] narrowed to a set of forecast ids,
/// keeping only the strongest grant per forecast.
///
/// `$1` = `TEXT[]` of forecast ids, `$2` = caller's user_id. Batched
/// because the list endpoints resolve provenance for a whole page at
/// once; the single-forecast ACL check binds a one-element array.
pub fn inherited_access_by_ids_sql() -> String {
    format!(
        "SELECT DISTINCT ON (r.forecast_id)
                r.forecast_id, r.permission, r.portfolio_id,
                r.portfolio_title, r.team_id
         FROM ({relation}) r
         WHERE r.forecast_id = ANY($1)
         ORDER BY r.forecast_id,
                  CASE r.permission
                     WHEN 'admin' THEN 3
                     WHEN 'edit'  THEN 2
                     ELSE 1
                  END DESC",
        relation = inherited_access_relation(2)
    )
}

/// Just the ids: `SELECT forecast_id FROM (relation)`, for use as an
/// `f.id IN (…)` branch inside a larger ACL `WHERE` clause. `n` is the
/// bind position the caller has reserved for `user_id`.
pub fn inherited_access_ids_sql(n: u32) -> String {
    format!(
        "SELECT r.forecast_id FROM ({relation}) r",
        relation = inherited_access_relation(n)
    )
}

/// The complete "can this principal VIEW this forecast" test, as a SQL
/// boolean fragment for embedding in a larger `WHERE`.
///
/// Mirrors [`can_access`]'s branch set exactly — owner, public/shared
/// visibility, team-owned, direct user share, portfolio inheritance — so
/// a list query and a single-row check can never disagree about what the
/// caller may see.
///
/// ## Why this exists
///
/// This predicate had been hand-copied into `list_forecasts_handler` and
/// was about to be copied a third and fourth time (the cascade queue and
/// the ops detectors). Every copy is a place for the ACL to rot: the
/// team-share branch in `list_forecasts_handler` was missing for a whole
/// release because someone added it to the detail handler only.
///
/// ## Arguments
///
/// * `alias` — the `fermi_forecasts` alias in the enclosing query
///   (`"f"`, `"ff"`, …). Not user input at any call site.
/// * `n` — the bind position holding the caller's `user_id` (TEXT).
///
/// The generated fragment is parenthesised, so it can be `AND`-ed into an
/// existing clause without precedence surprises. Its inner subqueries use
/// their own aliases (`pf`/`p`/`f`/`os`/`tm`), which shadow rather than
/// collide with the outer query's.
///
/// Note this is the VIEW test. Write paths must still go through
/// [`can_edit`] on the specific row — a predicate can gate a list, but a
/// list is not the place to decide who may change something.
pub fn forecast_view_predicate(alias: &str, n: u32) -> String {
    format!(
        "({a}.owner_id = ${n} \
          OR {a}.visibility IN ('shared', 'public') \
          OR ({a}.team_id IS NOT NULL \
              AND EXISTS (SELECT 1 FROM team_members vm \
                          WHERE vm.team_id = {a}.team_id AND vm.member_id = ${n})) \
          OR EXISTS (SELECT 1 FROM object_shares vs \
                     WHERE vs.object_type = 'forecast' \
                       AND vs.object_id = {a}.id::text \
                       AND vs.share_type = 'user' \
                       AND vs.share_target = ${n}) \
          OR {a}.id IN ({inherited}))",
        a = alias,
        n = n,
        inherited = inherited_access_ids_sql(n)
    )
}

/// Convenience: can the principal view this object?
pub async fn can_view(
    pool: &PgPool,
    principal: &AuthPrincipal,
    object_type: ObjectType,
    object_id: &str,
    owner_id: &str,
    visibility: Visibility,
) -> Result<bool, AuthError> {
    let level = can_access(
        pool,
        principal,
        object_type,
        object_id,
        owner_id,
        visibility,
    )
    .await?;
    Ok(level.has_view())
}

/// Convenience: can the principal edit this object?
pub async fn can_edit(
    pool: &PgPool,
    principal: &AuthPrincipal,
    object_type: ObjectType,
    object_id: &str,
    owner_id: &str,
    visibility: Visibility,
) -> Result<bool, AuthError> {
    let level = can_access(
        pool,
        principal,
        object_type,
        object_id,
        owner_id,
        visibility,
    )
    .await?;
    Ok(level.has_edit())
}

/// Check access for unauthenticated users — only public objects are visible.
pub fn can_access_anonymous(visibility: Visibility) -> AccessLevel {
    if visibility == Visibility::Public {
        AccessLevel::Granted(Permission::View)
    } else {
        AccessLevel::Denied
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The placeholder substitution is load-bearing: a missed occurrence
    /// ships SQL containing a literal `{USER}`, which Postgres rejects at
    /// runtime with a syntax error rather than at compile time. This
    /// pins that every occurrence is replaced and that at least one
    /// actually was (guarding against someone renaming the token in the
    /// const but not here).
    #[test]
    fn inherited_relation_substitutes_every_placeholder() {
        let sql = inherited_access_relation(7);
        assert!(
            !sql.contains(USER_PLACEHOLDER),
            "unsubstituted {} left in generated SQL",
            USER_PLACEHOLDER
        );
        assert!(sql.contains("$7"), "bind position was not injected");
        // The relation reaches the caller's identity in three places:
        // the user-share test, the team-share membership test, and the
        // team-owned membership test. If a refactor drops one of them,
        // inheritance silently stops working for that path.
        assert_eq!(
            sql.matches("$7").count(),
            3,
            "expected 3 user_id references in the inheritance relation"
        );
    }

    /// The by-ids wrapper binds ids at $1 and the user at $2. Getting
    /// these backwards yields a type error at runtime (text[] vs text),
    /// so pin the contract the two callers rely on.
    #[test]
    fn inherited_by_ids_binds_ids_first_user_second() {
        let sql = inherited_access_by_ids_sql();
        assert!(!sql.contains(USER_PLACEHOLDER));
        assert!(sql.contains("r.forecast_id = ANY($1)"));
        assert_eq!(sql.matches("$2").count(), 3);
        // DISTINCT ON requires its expression to lead ORDER BY; if that
        // invariant breaks Postgres errors out at runtime.
        let distinct = sql.find("DISTINCT ON (r.forecast_id)");
        let order = sql.find("ORDER BY r.forecast_id");
        assert!(distinct.is_some() && order.is_some());
        assert!(distinct < order);
    }

    /// The ids-only variant is embedded inside a larger `WHERE ... IN (…)`
    /// clause, so it must not carry its own ORDER BY or LIMIT (both are
    /// meaningless there, and ORDER BY inside IN is wasted work).
    #[test]
    fn inherited_ids_sql_is_embeddable() {
        let sql = inherited_access_ids_sql(1);
        assert!(!sql.contains(USER_PLACEHOLDER));
        assert!(sql.trim_start().starts_with("SELECT r.forecast_id"));
        assert!(!sql.contains("LIMIT"));
    }

    /// The leak guard (Spec 26 §2.1) is the difference between a feature
    /// and a privilege-escalation primitive. Behavioural coverage lives
    /// in `scripts/spec26_sql_check.sh` (it needs a real planner); this
    /// asserts the clause is at least still present, so nobody deletes it
    /// while tidying the SQL.
    #[test]
    fn inheritance_retains_the_leak_guard() {
        let sql = INHERITED_ACCESS_RELATION_SQL;
        assert!(
            sql.contains("src.forecast_owner = src.portfolio_owner"),
            "leak guard branch (a) missing"
        );
        assert!(
            sql.contains("tm2.member_id     = src.forecast_owner"),
            "leak guard branch (b) missing"
        );
    }
}

/// Check if a principal is a member of a specific team (any role).
pub async fn is_team_member(
    pool: &PgPool,
    team_id: Uuid,
    member_id: &str,
) -> Result<bool, AuthError> {
    let row = sqlx::query_as::<_, (i64,)>(
        "SELECT COUNT(*) FROM team_members WHERE team_id = $1 AND member_id = $2",
    )
    .bind(team_id)
    .bind(member_id)
    .fetch_one(pool)
    .await
    .map_err(|e| AuthError::DatabaseError(e.to_string()))?;

    Ok(row.0 > 0)
}
