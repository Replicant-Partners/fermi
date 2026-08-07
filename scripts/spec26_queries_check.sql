-- ─────────────────────────────────────────────────────────────────────
-- Spec 26: plan + behaviour checks for every query the feature adds
-- ─────────────────────────────────────────────────────────────────────
--
-- Two halves:
--
--   PART A — PREPARE every query verbatim. PREPARE fully parses, name-
--            resolves and type-checks against the live catalogue, so it
--            catches typos, wrong `::text` casts, UNION branch arity/type
--            mismatches and unresolvable columns. It's the offline
--            equivalent of the runtime failure these `sqlx::query`
--            strings would otherwise produce on first call.
--
--   PART B — seed a small collaboration graph and assert the INHERITANCE
--            rule and its LEAK GUARD behave as Spec 26 §2.1 specifies.
--            This is the part that actually matters for safety: the guard
--            is what stops "add a colleague's private forecast to my
--            portfolio, share the portfolio" from being a
--            privilege-escalation primitive.
--
--   PART C — Spec 32's orphan reconcile. The driver name set is computed
--            in Rust (drivers are FPL declarations, not rows), so what SQL
--            can prove is that the pair of statements it drives is
--            REVERSIBLE: orphaning is undone when the driver comes back.
--            Without that, a Spec 31 revert would restore a driver but
--            leave its objections dead — undo that loses something.
--
-- Run with: scripts/spec26_sql_check.sh

\set ON_ERROR_STOP on

-- ═════════════════════════════════════════════════════════════════════
-- PART A — every query, PREPAREd
-- ═════════════════════════════════════════════════════════════════════

\echo '--- A1: inheritance relation (visibility.rs INHERITED_ACCESS_RELATION_SQL, via by_ids wrapper)'
PREPARE a1 (text[], text) AS
SELECT DISTINCT ON (r.forecast_id)
        r.forecast_id, r.permission, r.portfolio_id,
        r.portfolio_title, r.team_id
 FROM (
SELECT src.forecast_id,
       src.permission,
       src.portfolio_id,
       src.portfolio_title,
       src.team_id
FROM (
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
            (os.share_type = 'user' AND os.share_target = $2)
         OR (os.share_type = 'team' AND EXISTS (
                SELECT 1 FROM team_members tm
                WHERE tm.team_id::text = os.share_target
                  AND tm.member_id     = $2))
          )

    UNION ALL

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
                  WHERE tm.team_id = p.team_id AND tm.member_id = $2)
) src
WHERE (
        src.forecast_owner = src.portfolio_owner
     OR (src.share_type = 'team' AND EXISTS (
            SELECT 1 FROM team_members tm2
            WHERE tm2.team_id::text = src.share_target
              AND tm2.member_id     = src.forecast_owner))
      )
 ) r
 WHERE r.forecast_id = ANY($1)
 ORDER BY r.forecast_id,
          CASE r.permission
             WHEN 'admin' THEN 3
             WHEN 'edit'  THEN 2
             ELSE 1
          END DESC;

\echo '--- A2: forecast provenance (collab.rs provenance_sql)'
PREPARE a2 (text[], text) AS
SELECT o.id::text                                        AS object_id,
       o.owner_id::text                                  AS owner_id,
       o.visibility                                      AS visibility,
       o.team_id::text                                   AS team_id,
       us.permission                                     AS user_perm,
       us.granted_by                                     AS user_grantor,
       us.created_at                                     AS user_at,
       ts.permission                                     AS team_perm,
       ts.granted_by                                     AS team_grantor,
       ts.created_at                                     AS team_at,
       ts.share_target                                   AS team_share_target,
       (o.team_id IS NOT NULL AND EXISTS (
            SELECT 1 FROM team_members m
            WHERE m.team_id = o.team_id AND m.member_id = $2))  AS team_owned_member,
       (SELECT COUNT(*) FROM object_shares s
        WHERE s.object_type = 'forecast' AND s.object_id = o.id::text) AS share_count
FROM fermi_forecasts o
LEFT JOIN object_shares us
       ON us.object_type = 'forecast'
      AND us.object_id   = o.id::text
      AND us.share_type  = 'user'
      AND us.share_target = $2
LEFT JOIN LATERAL (
    SELECT os.permission, os.granted_by, os.created_at, os.share_target
    FROM object_shares os
    JOIN team_members tm ON tm.team_id::text = os.share_target
                        AND tm.member_id     = $2
    WHERE os.object_type = 'forecast'
      AND os.object_id   = o.id::text
      AND os.share_type  = 'team'
    ORDER BY CASE os.permission
                WHEN 'admin' THEN 3 WHEN 'edit' THEN 2 ELSE 1 END DESC
    LIMIT 1
) ts ON TRUE
WHERE o.id::text = ANY($1);

\echo '--- A3: portfolio provenance (same shape, portfolio table)'
PREPARE a3 (text[], text) AS
SELECT o.id::text AS object_id, o.owner_id::text AS owner_id, o.visibility,
       o.team_id::text AS team_id,
       us.permission AS user_perm, us.granted_by AS user_grantor, us.created_at AS user_at,
       ts.permission AS team_perm, ts.granted_by AS team_grantor, ts.created_at AS team_at,
       ts.share_target AS team_share_target,
       (o.team_id IS NOT NULL AND EXISTS (
            SELECT 1 FROM team_members m
            WHERE m.team_id = o.team_id AND m.member_id = $2)) AS team_owned_member,
       (SELECT COUNT(*) FROM object_shares s
        WHERE s.object_type = 'portfolio' AND s.object_id = o.id::text) AS share_count
FROM fermi_portfolios o
LEFT JOIN object_shares us
       ON us.object_type = 'portfolio' AND us.object_id = o.id::text
      AND us.share_type = 'user' AND us.share_target = $2
LEFT JOIN LATERAL (
    SELECT os.permission, os.granted_by, os.created_at, os.share_target
    FROM object_shares os
    JOIN team_members tm ON tm.team_id::text = os.share_target AND tm.member_id = $2
    WHERE os.object_type = 'portfolio' AND os.object_id = o.id::text
      AND os.share_type = 'team'
    ORDER BY CASE os.permission WHEN 'admin' THEN 3 WHEN 'edit' THEN 2 ELSE 1 END DESC
    LIMIT 1
) ts ON TRUE
WHERE o.id::text = ANY($1);

\echo '--- A4: portfolio memberships (collab.rs forecast_portfolio_memberships)'
PREPARE a4 (text[], text) AS
SELECT pf.forecast_id,
        p.id::text        AS portfolio_id,
        p.title,
        p.owner_id::text  AS owner_id,
        p.team_id::text   AS team_id,
        pf.added_at,
        pf.added_by
 FROM fermi_portfolio_forecasts pf
 JOIN fermi_portfolios p ON p.id = pf.portfolio_id
 WHERE pf.forecast_id = ANY($1)
   AND (
         p.owner_id::text = $2
      OR p.visibility IN ('shared', 'public')
      OR (p.team_id IS NOT NULL AND EXISTS (
            SELECT 1 FROM team_members m
            WHERE m.team_id = p.team_id AND m.member_id = $2))
      OR EXISTS (
            SELECT 1 FROM object_shares s
            LEFT JOIN team_members tm
                   ON s.share_type = 'team'
                  AND s.share_target = tm.team_id::text
                  AND tm.member_id = $2
            WHERE s.object_type = 'portfolio'
              AND s.object_id   = p.id::text
              AND ((s.share_type = 'user' AND s.share_target = $2)
                OR (s.share_type = 'team' AND tm.member_id IS NOT NULL)))
       )
 ORDER BY pf.added_at DESC;

\echo '--- A5: forecast event UNION (collab.rs FORECAST_EVENTS_SQL) — 6 branches, 16 cols'
PREPARE a5 (text[]) AS
SELECT f.created_at                     AS ts,
       CASE WHEN f.status = 'draft' THEN 'created' ELSE 'published' END AS kind,
       f.owner_id::text                 AS actor,
       NULL::text                       AS agent_id,
       f.id                             AS forecast_id,
       f.question_text                  AS question_text,
       NULL::real                       AS prev_probability,
       f.predicted_probability          AS new_probability,
       NULL::text                       AS reason,
       NULL::text                       AS revision_trigger,
       NULL::boolean                    AS outcome,
       NULL::real                       AS brier_score,
       NULL::text                       AS ref_type,
       NULL::text                       AS ref_id,
       NULL::text                       AS ref_label,
       NULL::text                       AS permission
FROM fermi_forecasts f
WHERE f.id = ANY($1)
UNION ALL
SELECT u.created_at, 'revised', u.actor_user_id, u.agent_id, u.forecast_id,
       f.question_text, u.previous_probability, u.new_probability, u.reason,
       u.revision_trigger, NULL::boolean, NULL::real, NULL::text, NULL::text,
       NULL::text, NULL::text
FROM fermi_forecast_updates u
JOIN fermi_forecasts f ON f.id = u.forecast_id
WHERE u.forecast_id = ANY($1)
UNION ALL
SELECT f.resolved_at, 'resolved', f.resolved_by, NULL::text, f.id,
       f.question_text, NULL::real, f.predicted_probability, f.resolution_notes,
       NULL::text, f.actual_outcome, f.brier_score, NULL::text, NULL::text,
       NULL::text, NULL::text
FROM fermi_forecasts f
WHERE f.id = ANY($1) AND f.resolved_at IS NOT NULL
UNION ALL
SELECT s.created_at, 'shared', s.granted_by, NULL::text, s.object_id,
       f.question_text, NULL::real, NULL::real, NULL::text, NULL::text,
       NULL::boolean, NULL::real, s.share_type, s.share_target, NULL::text,
       s.permission
FROM object_shares s
JOIN fermi_forecasts f ON f.id = s.object_id
WHERE s.object_type = 'forecast' AND s.object_id = ANY($1)
UNION ALL
SELECT pf.added_at, 'portfolio_add', pf.added_by, NULL::text, pf.forecast_id,
       f.question_text, NULL::real, NULL::real, NULL::text, NULL::text,
       NULL::boolean, NULL::real, 'portfolio', pf.portfolio_id, p.title,
       NULL::text
FROM fermi_portfolio_forecasts pf
JOIN fermi_forecasts   f ON f.id = pf.forecast_id
JOIN fermi_portfolios  p ON p.id = pf.portfolio_id
WHERE pf.forecast_id = ANY($1)
UNION ALL
SELECT i.created_at, 'invited', i.inviter_id, NULL::text, i.target_id,
       f.question_text, NULL::real, NULL::real, i.message, i.status,
       NULL::boolean, NULL::real, 'invite', i.id::text,
       COALESCE(i.invitee_email, i.invitee_user_id), i.permission
FROM forecast_invites i
JOIN fermi_forecasts f ON f.id = i.target_id
WHERE i.target_type = 'forecast' AND i.target_id = ANY($1)
ORDER BY ts DESC;

\echo '--- A6: portfolio-level events (collab.rs portfolio_level_events)'
PREPARE a6 (text[]) AS
SELECT p.created_at            AS ts,
       'portfolio_created'     AS kind,
       p.owner_id::text        AS actor,
       p.id::text              AS object_id,
       p.title                 AS object_title,
       NULL::text              AS ref_type,
       NULL::text              AS ref_id,
       NULL::text              AS ref_label,
       NULL::text              AS permission
FROM fermi_portfolios p
WHERE p.id::text = ANY($1)
UNION ALL
SELECT s.created_at, 'shared', s.granted_by, s.object_id, p.title,
       s.share_type, s.share_target, NULL::text, s.permission
FROM object_shares s
JOIN fermi_portfolios p ON p.id = s.object_id
WHERE s.object_type = 'portfolio' AND s.object_id = ANY($1)
ORDER BY ts DESC;

\echo '--- A7: team surface, portfolios (collab.rs team_surface)'
PREPARE a7 (uuid, text) AS
SELECT p.id::text AS id FROM fermi_portfolios p WHERE p.team_id = $1
 UNION
 SELECT s.object_id FROM object_shares s
 WHERE s.object_type = 'portfolio'
   AND s.share_type  = 'team'
   AND s.share_target = $2;

\echo '--- A8: team surface, forecasts (leak-guarded)'
PREPARE a8 (uuid, text, text[]) AS
SELECT f.id AS id FROM fermi_forecasts f WHERE f.team_id = $1
 UNION
 SELECT s.object_id FROM object_shares s
 WHERE s.object_type = 'forecast'
   AND s.share_type  = 'team'
   AND s.share_target = $2
 UNION
 SELECT pf.forecast_id
 FROM fermi_portfolio_forecasts pf
 JOIN fermi_portfolios p ON p.id = pf.portfolio_id
 JOIN fermi_forecasts  f ON f.id = pf.forecast_id
 WHERE p.id::text = ANY($3)
   AND (f.owner_id::text = p.owner_id::text
     OR EXISTS (SELECT 1 FROM team_members tm
                WHERE tm.team_id = $1 AND tm.member_id = f.owner_id::text));

\echo '--- A9: team membership events'
PREPARE a9 (uuid) AS
SELECT tm.joined_at AS ts, tm.member_id, tm.member_type, tm.role,
        tm.invited_by, t.name AS team_name
 FROM team_members tm
 JOIN teams t ON t.id = tm.team_id
 WHERE tm.team_id = $1
 ORDER BY tm.joined_at DESC;

\echo '--- A10: team contributions roll-up (GREATEST over 3 correlated MAXes)'
PREPARE a10 (uuid, text[], text[]) AS
SELECT tm.member_id,
       tm.member_type,
       tm.role,
       tm.joined_at,
       tm.invited_by,
       (SELECT COUNT(*) FROM fermi_forecast_updates u
        WHERE u.actor_user_id = tm.member_id AND u.forecast_id = ANY($2)) AS revisions,
       (SELECT COUNT(*) FROM fermi_forecasts f
        WHERE f.resolved_by = tm.member_id AND f.id = ANY($2))            AS resolutions,
       (SELECT COUNT(*) FROM fermi_forecasts f
        WHERE f.owner_id::text = tm.member_id AND f.id = ANY($2))         AS authored,
       (SELECT COUNT(*) FROM object_shares s
        WHERE s.granted_by = tm.member_id
          AND ((s.object_type = 'forecast'  AND s.object_id = ANY($2))
            OR (s.object_type = 'portfolio' AND s.object_id = ANY($3))))  AS shares_granted,
       (SELECT COUNT(*) FROM fermi_portfolio_forecasts pf
        WHERE pf.added_by = tm.member_id AND pf.portfolio_id = ANY($3))   AS curations,
       GREATEST(
           (SELECT MAX(u.created_at) FROM fermi_forecast_updates u
            WHERE u.actor_user_id = tm.member_id AND u.forecast_id = ANY($2)),
           (SELECT MAX(f.updated_at) FROM fermi_forecasts f
            WHERE f.owner_id::text = tm.member_id AND f.id = ANY($2)),
           (SELECT MAX(s.created_at) FROM object_shares s
            WHERE s.granted_by = tm.member_id
              AND ((s.object_type = 'forecast'  AND s.object_id = ANY($2))
                OR (s.object_type = 'portfolio' AND s.object_id = ANY($3))))
       )                                                                  AS last_active_at
FROM team_members tm
WHERE tm.team_id = $1
ORDER BY tm.joined_at;

\echo '--- A11: team shared, portfolios'
PREPARE a11 (uuid, text) AS
SELECT p.id::text        AS id,
       p.title,
       p.description,
       p.owner_id::text   AS owner_id,
       p.visibility,
       p.domain,
       p.team_id::text    AS team_id,
       p.created_at,
       p.updated_at,
       CASE WHEN p.team_id = $1 THEN 'team_owned' ELSE 'team_share' END AS via,
       os.permission      AS permission,
       os.granted_by      AS shared_by,
       os.created_at      AS shared_at,
       (SELECT COUNT(*) FROM fermi_portfolio_forecasts pf
        WHERE pf.portfolio_id = p.id)                                   AS forecast_count,
       (SELECT COUNT(*) FROM fermi_portfolio_forecasts pf
        JOIN fermi_forecasts f ON f.id = pf.forecast_id
        WHERE pf.portfolio_id = p.id AND f.status = 'resolved')         AS resolved_count,
       (SELECT AVG(f.brier_score)::float8 FROM fermi_portfolio_forecasts pf
        JOIN fermi_forecasts f ON f.id = pf.forecast_id
        WHERE pf.portfolio_id = p.id AND f.brier_score IS NOT NULL)     AS avg_brier
FROM fermi_portfolios p
LEFT JOIN object_shares os
       ON os.object_type  = 'portfolio'
      AND os.object_id    = p.id::text
      AND os.share_type   = 'team'
      AND os.share_target = $2
WHERE p.team_id = $1 OR os.id IS NOT NULL
ORDER BY p.updated_at DESC;

\echo '--- A12: team shared, direct forecasts'
PREPARE a12 (uuid, text) AS
SELECT f.id, f.question_text, f.owner_id::text AS owner_id,
       f.predicted_probability, f.status, f.brier_score, f.actual_outcome,
       f.visibility, f.domain, f.tags, f.target_date, f.created_at,
       f.updated_at, f.resolved_at, f.team_id::text AS team_id,
       CASE WHEN f.team_id = $1 THEN 'team_owned' ELSE 'team_share' END AS via,
       os.permission AS permission, os.granted_by AS shared_by,
       os.created_at AS shared_at,
       NULL::text AS via_portfolio_id, NULL::text AS via_portfolio_title,
       (SELECT COUNT(*) FROM fermi_forecast_updates u
        WHERE u.forecast_id = f.id
          AND u.created_at > NOW() - INTERVAL '7 days') AS n_recent_updates
FROM fermi_forecasts f
LEFT JOIN object_shares os
       ON os.object_type  = 'forecast' AND os.object_id = f.id
      AND os.share_type   = 'team'     AND os.share_target = $2
WHERE f.team_id = $1 OR os.id IS NOT NULL;

\echo '--- A13: team shared, inherited forecasts'
PREPARE a13 (uuid, text, text[]) AS
SELECT DISTINCT ON (f.id)
       f.id, f.question_text, f.owner_id::text AS owner_id,
       f.predicted_probability, f.status, f.brier_score, f.actual_outcome,
       f.visibility, f.domain, f.tags, f.target_date, f.created_at,
       f.updated_at, f.resolved_at, f.team_id::text AS team_id,
       'portfolio'       AS via,
       COALESCE(os.permission, 'edit') AS permission,
       COALESCE(os.granted_by, p.owner_id::text) AS shared_by,
       COALESCE(os.created_at, pf.added_at)      AS shared_at,
       p.id::text        AS via_portfolio_id,
       p.title           AS via_portfolio_title,
       (SELECT COUNT(*) FROM fermi_forecast_updates u
        WHERE u.forecast_id = f.id
          AND u.created_at > NOW() - INTERVAL '7 days') AS n_recent_updates
FROM fermi_portfolio_forecasts pf
JOIN fermi_portfolios p ON p.id = pf.portfolio_id
JOIN fermi_forecasts  f ON f.id = pf.forecast_id
LEFT JOIN object_shares os
       ON os.object_type  = 'portfolio' AND os.object_id = p.id::text
      AND os.share_type   = 'team'      AND os.share_target = $2
WHERE p.id::text = ANY($3)
  AND (f.owner_id::text = p.owner_id::text
    OR EXISTS (SELECT 1 FROM team_members tm
               WHERE tm.team_id = $1 AND tm.member_id = f.owner_id::text))
ORDER BY f.id, pf.added_at DESC;

\echo '--- A14: inherited shares for one forecast (collab.rs forecast_access_handler)'
PREPARE a14 (text) AS
SELECT p.id::text     AS portfolio_id,
       p.title        AS portfolio_title,
       os.id::text    AS share_id,
       os.share_type,
       os.share_target,
       os.permission,
       os.granted_by,
       os.created_at
FROM fermi_portfolio_forecasts pf
JOIN fermi_portfolios p ON p.id = pf.portfolio_id
JOIN fermi_forecasts  f ON f.id = pf.forecast_id
JOIN object_shares   os ON os.object_type = 'portfolio' AND os.object_id = p.id::text
WHERE pf.forecast_id = $1
  AND (f.owner_id::text = p.owner_id::text
    OR (os.share_type = 'team' AND EXISTS (
          SELECT 1 FROM team_members tm
          WHERE tm.team_id::text = os.share_target
            AND tm.member_id     = f.owner_id::text)))
ORDER BY os.created_at;

\echo '--- A15: cascades_to count (collab.rs portfolio_access_handler)'
PREPARE a15 (text) AS
SELECT COUNT(*) AS n
 FROM fermi_portfolio_forecasts pf
 JOIN fermi_portfolios p ON p.id = pf.portfolio_id
 JOIN fermi_forecasts  f ON f.id = pf.forecast_id
 WHERE pf.portfolio_id = $1
   AND (f.owner_id::text = p.owner_id::text
     OR EXISTS (SELECT 1 FROM object_shares os
                JOIN team_members tm ON tm.team_id::text = os.share_target
                WHERE os.object_type = 'portfolio'
                  AND os.object_id   = p.id::text
                  AND os.share_type  = 'team'
                  AND tm.member_id   = f.owner_id::text));

\echo '--- A16: enriched shares + roster (collab.rs enriched_shares)'
PREPARE a16 (text, text) AS
SELECT id::text AS id, object_type, object_id, share_type, share_target,
        permission, granted_by, created_at
 FROM object_shares
 WHERE object_type = $1 AND object_id = $2
 ORDER BY created_at;

PREPARE a16b (text[]) AS
SELECT tm.team_id::text AS team_id, tm.member_id, tm.member_type, tm.role
 FROM team_members tm
 WHERE tm.team_id::text = ANY($1)
 ORDER BY tm.joined_at;

\echo '--- A17: list_portfolio_forecasts projection (forecasts.rs, with added_by JOIN)'
PREPARE a17 (text) AS
SELECT f.id, f.question_text, f.predicted_probability, f.status, f.brier_score,
       f.actual_outcome, f.resolved_at, f.visibility, f.updated_at, f.metadata,
       f.tags, f.team_id,
       f.owner_id::text AS owner_id,
       COALESCE(ou.display_name, ou.name, ou.email, ou.user_id) AS owner_display_name,
       pf.added_at, pf.added_by,
       COALESCE(au.display_name, au.name, au.email, au.user_id) AS added_by_display_name,
       (SELECT COUNT(*) FROM fermi_forecast_updates u
        WHERE u.forecast_id = f.id
          AND u.created_at > NOW() - INTERVAL '7 days') AS n_recent_updates,
       (SELECT COUNT(*) FROM object_shares s
        WHERE s.object_type = 'forecast' AND s.object_id = f.id) AS share_count
 FROM fermi_portfolio_forecasts pf
 JOIN fermi_forecasts f ON f.id = pf.forecast_id
 LEFT JOIN users ou ON ou.user_id = f.owner_id::text
 LEFT JOIN users au ON au.user_id = pf.added_by
 WHERE pf.portfolio_id = $1
 ORDER BY pf.added_at DESC;

\echo '--- A18: list_forecasts ACL clause including the inheritance branch'
PREPARE a18 (text) AS
SELECT f.id
FROM fermi_forecasts f
WHERE (f.owner_id = $1
       OR f.visibility IN ('shared', 'public')
       OR (f.team_id IS NOT NULL
           AND EXISTS (SELECT 1 FROM team_members m
                       WHERE m.team_id = f.team_id AND m.member_id = $1))
       OR EXISTS (SELECT 1 FROM object_shares s
                  WHERE s.object_type = 'forecast'
                    AND s.object_id = f.id::text
                    AND s.share_type = 'user'
                    AND s.share_target = $1)
       OR f.id IN (SELECT r.forecast_id FROM (
SELECT src.forecast_id, src.permission, src.portfolio_id, src.portfolio_title, src.team_id
FROM (
    SELECT pf.forecast_id AS forecast_id, os.permission AS permission,
           p.id::text AS portfolio_id, p.title AS portfolio_title,
           CASE WHEN os.share_type = 'team' THEN os.share_target END AS team_id,
           os.share_type AS share_type, os.share_target AS share_target,
           f.owner_id::text AS forecast_owner, p.owner_id::text AS portfolio_owner
    FROM fermi_portfolio_forecasts pf
    JOIN fermi_portfolios p ON p.id = pf.portfolio_id
    JOIN fermi_forecasts   f ON f.id = pf.forecast_id
    JOIN object_shares    os ON os.object_type = 'portfolio' AND os.object_id = p.id::text
    WHERE ((os.share_type = 'user' AND os.share_target = $1)
        OR (os.share_type = 'team' AND EXISTS (
               SELECT 1 FROM team_members tm
               WHERE tm.team_id::text = os.share_target AND tm.member_id = $1)))
    UNION ALL
    SELECT pf.forecast_id AS forecast_id, 'edit' AS permission,
           p.id::text AS portfolio_id, p.title AS portfolio_title,
           p.team_id::text AS team_id, 'team' AS share_type,
           p.team_id::text AS share_target,
           f.owner_id::text AS forecast_owner, p.owner_id::text AS portfolio_owner
    FROM fermi_portfolio_forecasts pf
    JOIN fermi_portfolios p ON p.id = pf.portfolio_id
    JOIN fermi_forecasts   f ON f.id = pf.forecast_id
    WHERE p.team_id IS NOT NULL
      AND EXISTS (SELECT 1 FROM team_members tm
                  WHERE tm.team_id = p.team_id AND tm.member_id = $1)
) src
WHERE (src.forecast_owner = src.portfolio_owner
    OR (src.share_type = 'team' AND EXISTS (
           SELECT 1 FROM team_members tm2
           WHERE tm2.team_id::text = src.share_target
             AND tm2.member_id = src.forecast_owner)))
 ) r));

\echo '--- A19: attributed writes (forecasts.rs / bayesops.rs INSERTs)'
PREPARE a19 (text, text, real, real, text) AS
INSERT INTO fermi_forecast_updates
 (id, forecast_id, previous_probability, new_probability, reason,
  actor_user_id, revision_trigger, created_at)
 VALUES ($1, $2, $3, $4, 'Manual update via API', $5, 'manual', NOW());

PREPARE a20 (text, text, text) AS
INSERT INTO fermi_portfolio_forecasts (portfolio_id, forecast_id, added_by)
 VALUES ($1, $2, $3) ON CONFLICT DO NOTHING;

\echo ''
\echo '=== PART A: all queries parsed and type-checked ==='
\echo ''

-- ═════════════════════════════════════════════════════════════════════
-- PART B — behaviour: inheritance and the leak guard
-- ═════════════════════════════════════════════════════════════════════
--
-- Graph:
--   alice owns portfolio 'book' and forecast 'f_alice'  (both in 'book')
--   carol owns 'f_carol', which alice ALSO put in 'book'
--   'book' is shared with team 'analysts' (edit); bob is a member
--   dave is on no team, has no shares
--
-- Expected, per Spec 26 §2.1:
--   bob   inherits f_alice   (forecast owner == portfolio owner)
--   bob   does NOT inherit f_carol  (carol owns it, carol is not on the
--                                    team → the leak guard blocks it)
--   dave  inherits nothing
--   If carol JOINS the team, bob then inherits f_carol (branch (b)).

\echo '=== PART B: inheritance + leak guard ==='

INSERT INTO users (user_id, display_name) VALUES
    ('alice','Alice'), ('bob','Bob'), ('carol','Carol'), ('dave','Dave');

INSERT INTO teams (id, name, slug, owner_id) VALUES
    ('11111111-1111-1111-1111-111111111111','Analysts','analysts','alice');

INSERT INTO team_members (team_id, member_type, member_id, role) VALUES
    ('11111111-1111-1111-1111-111111111111','user','alice','owner'),
    ('11111111-1111-1111-1111-111111111111','user','bob','member');

INSERT INTO fermi_portfolios (id, title, owner_id, visibility) VALUES
    ('book','WC 2026','alice','private');

INSERT INTO fermi_forecasts (id, owner_id, question_text, predicted_probability, status, visibility) VALUES
    ('f_alice','alice','Does Argentina win?',0.41,'active','private'),
    ('f_carol','carol','Does Brazil win?',0.33,'active','private');

INSERT INTO fermi_portfolio_forecasts (portfolio_id, forecast_id, added_by) VALUES
    ('book','f_alice','alice'),
    ('book','f_carol','alice');

INSERT INTO object_shares (object_type, object_id, share_type, share_target, permission, granted_by) VALUES
    ('portfolio','book','team','11111111-1111-1111-1111-111111111111','edit','alice');

\echo '-- bob: expect f_alice(edit) only. f_carol must be ABSENT (leak guard).'
EXECUTE a1(ARRAY['f_alice','f_carol'], 'bob');

\echo '-- dave (no team, no share): expect zero rows.'
EXECUTE a1(ARRAY['f_alice','f_carol'], 'dave');

\echo '-- carol joins the team → bob now inherits f_carol too (branch (b)).'
INSERT INTO team_members (team_id, member_type, member_id, role) VALUES
    ('11111111-1111-1111-1111-111111111111','user','carol','member');
EXECUTE a1(ARRAY['f_alice','f_carol'], 'bob');

\echo '-- provenance for bob: neither row may report a user share, a team'
\echo '-- share, team ownership, or bob as owner — which is what forces the'
\echo '-- Rust resolver down to the portfolio-inheritance fallback and makes'
\echo '-- it label these access_via=''portfolio''.'
EXECUTE a2(ARRAY['f_alice','f_carol'], 'bob');

\echo '-- list ACL for bob must now include BOTH inherited forecasts.'
EXECUTE a18('bob');

\echo '-- list ACL for dave must be empty.'
EXECUTE a18('dave');

\echo '-- team surface (forecasts) for analysts: leak-guarded portfolio members.'
EXECUTE a8('11111111-1111-1111-1111-111111111111',
           '11111111-1111-1111-1111-111111111111',
           ARRAY['book']);

\echo '-- cascades_to for book: 2 (alice''s own + carol''s, carol now on team).'
EXECUTE a15('book');

\echo '-- attributed revision write + read-back through the event UNION.'
EXECUTE a19('u1','f_alice',0.41,0.47,'bob');
EXECUTE a5(ARRAY['f_alice']);

\echo '-- contributions: bob should show 1 revision.'
EXECUTE a10('11111111-1111-1111-1111-111111111111',
            ARRAY['f_alice','f_carol'],
            ARRAY['book']);

\echo ''
\echo '=== PART B complete ==='

-- ═════════════════════════════════════════════════════════════════════
-- PART C — Spec 32 driver annotations
-- ═════════════════════════════════════════════════════════════════════

\echo ''
\echo '=== PART C: driver annotations ==='

-- ─── Spec 32: the orphan reconcile ───────────────────────────────────
--
-- The driver name set is computed in Rust (drivers are FPL declarations,
-- not rows, so establishing them means parsing `fpl_source`). What SQL can
-- still prove is that the two statements it drives behave as a pair: one
-- orphans names the program no longer declares, the other revives names it
-- declares again. The second is the load-bearing one — without it a Spec 31
-- revert restores a driver but leaves its objections dead, which would make
-- undo lossy in exactly the way the whole collaboration model says it
-- isn't.

\echo '=== Spec 32: orphan reconcile is reversible ==='

INSERT INTO public.fermi_forecasts (id, owner_id, question_text, predicted_probability, status)
VALUES ('fc-ann', 'u-owner', 'annotated?', 0.5, 'active');

INSERT INTO public.driver_annotations
    (forecast_id, driver_name, author_id, body, status, resolved_by, resolved_at)
VALUES ('fc-ann', 'elo_current', 'u-critic', 'base rate is wrong', 'open',     NULL,      NULL),
       ('fc-ann', 'home_adv',    'u-critic', 'stale',              'open',     NULL,      NULL),
       ('fc-ann', 'elo_current', 'u-owner',  'considered',         'declined', 'u-owner', NOW());

-- The program now declares only `home_adv`: `elo_current` was renamed away.
PREPARE ann_orphan (TEXT, TEXT[]) AS
  UPDATE driver_annotations
     SET status = 'orphaned', updated_at = NOW()
   WHERE forecast_id = $1
     AND status = 'open'
     AND driver_name IS NOT NULL
     AND NOT (driver_name = ANY($2));

PREPARE ann_revive (TEXT, TEXT[]) AS
  UPDATE driver_annotations
     SET status = 'open', updated_at = NOW()
   WHERE forecast_id = $1
     AND status = 'orphaned'
     AND driver_name = ANY($2);

EXECUTE ann_orphan('fc-ann', ARRAY['home_adv']);

DO $$
DECLARE n INT;
BEGIN
  SELECT count(*) INTO n FROM driver_annotations
   WHERE forecast_id = 'fc-ann' AND driver_name = 'elo_current' AND status = 'orphaned';
  IF n <> 1 THEN
    RAISE EXCEPTION 'orphan sweep should have orphaned the 1 OPEN elo_current annotation, got %', n;
  END IF;

  -- A human's judgement is not a derived observation. 'declined' means
  -- someone considered the objection and rejected it; the driver going
  -- away later does not un-make that, and overwriting it would destroy
  -- the record the status column exists to keep.
  SELECT count(*) INTO n FROM driver_annotations
   WHERE forecast_id = 'fc-ann' AND status = 'declined';
  IF n <> 1 THEN
    RAISE EXCEPTION 'orphan sweep must not touch resolved annotations, declined count = %', n;
  END IF;

  SELECT count(*) INTO n FROM driver_annotations
   WHERE forecast_id = 'fc-ann' AND driver_name = 'home_adv' AND status = 'open';
  IF n <> 1 THEN
    RAISE EXCEPTION 'orphan sweep hit a driver that still exists, open home_adv = %', n;
  END IF;
END $$;

-- Now the revert: the program declares `elo_current` again.
EXECUTE ann_revive('fc-ann', ARRAY['home_adv', 'elo_current']);

DO $$
DECLARE n INT;
BEGIN
  SELECT count(*) INTO n FROM driver_annotations
   WHERE forecast_id = 'fc-ann' AND status = 'orphaned';
  IF n <> 0 THEN
    RAISE EXCEPTION 'revert restored the driver but left % annotation(s) orphaned', n;
  END IF;

  SELECT count(*) INTO n FROM driver_annotations
   WHERE forecast_id = 'fc-ann' AND status = 'declined';
  IF n <> 1 THEN
    RAISE EXCEPTION 'revive resurrected a resolved annotation, declined count = %', n;
  END IF;
END $$;

-- A resolution must be attributable. This is the same gap Spec 26 closed
-- for revisions; the CHECK exists so it cannot reopen in a new table.
\echo '=== Spec 32: unattributable resolution is refused ==='
DO $$
BEGIN
  BEGIN
    INSERT INTO public.driver_annotations (forecast_id, driver_name, author_id, body, status)
    VALUES ('fc-ann', 'home_adv', 'u-critic', 'sneaky', 'accepted');
    RAISE EXCEPTION 'accepted annotation with no resolved_by/resolved_at was allowed';
  EXCEPTION WHEN check_violation THEN
    NULL;
  END;
END $$;

DEALLOCATE ann_orphan;
DEALLOCATE ann_revive;
DELETE FROM public.driver_annotations WHERE forecast_id = 'fc-ann';
DELETE FROM public.fermi_forecasts WHERE id = 'fc-ann';

-- ─── Detector 5: contested_assumption (ops.rs) ───────────────────────
--
-- The other four detectors are PREPAREd in PART A; this one lives here
-- because it is the only one that needs migration 183's table.

\echo '=== Spec 32: contested_assumption detector ==='
PREPARE c1 (text[]) AS
  SELECT a.forecast_id,
         ff.question_text,
         COUNT(*)                                  AS n_open,
         MIN(a.created_at)                         AS since,
         ARRAY_AGG(DISTINCT a.driver_name)
             FILTER (WHERE a.driver_name IS NOT NULL) AS drivers,
         ARRAY_AGG(DISTINCT a.author_id)           AS authors
    FROM public.driver_annotations a
    JOIN public.fermi_forecasts ff ON ff.id = a.forecast_id
   WHERE a.forecast_id = ANY($1)
     AND a.status = 'open'
     AND a.kind = 'challenge'
     AND ff.status = 'active'
   GROUP BY a.forecast_id, ff.question_text;

-- Behaviour: the detector must go quiet for the three reasons the design
-- says it should, because "the definition of done is the detector going
-- quiet" is the entire Spec 27 contract. If any of these still returned a
-- row, the board would show work nobody can clear.
INSERT INTO public.fermi_forecasts (id, owner_id, question_text, predicted_probability, status)
VALUES ('fc-det', 'u-owner', 'detected?', 0.5, 'active');

INSERT INTO public.driver_annotations
    (forecast_id, driver_name, author_id, body, kind, status, resolved_by, resolved_at)
VALUES
    -- resolved: answered, so no longer work
    ('fc-det', 'd_a', 'u1', 'accepted one',  'challenge', 'accepted', 'u-owner', NOW()),
    ('fc-det', 'd_b', 'u1', 'declined one',  'challenge', 'declined', 'u-owner', NOW()),
    -- orphaned: the driver is gone, so there is nothing to settle. This is
    -- what the orphan sweep buys the board.
    ('fc-det', 'd_c', 'u1', 'stranded',      'challenge', 'orphaned', NULL,      NULL),
    -- not a challenge: a note implies no action, a question is answered by
    -- talking, and neither should be ranked against a broken cascade.
    ('fc-det', 'd_d', 'u1', 'just context',  'note',      'open',     NULL,      NULL),
    ('fc-det', 'd_e', 'u1', 'what is this?', 'question',  'open',     NULL,      NULL);

DO $$
DECLARE n INT;
BEGIN
  SELECT count(*) INTO n FROM (
    SELECT a.forecast_id
      FROM public.driver_annotations a
      JOIN public.fermi_forecasts ff ON ff.id = a.forecast_id
     WHERE a.forecast_id = ANY(ARRAY['fc-det'])
       AND a.status = 'open' AND a.kind = 'challenge' AND ff.status = 'active'
     GROUP BY a.forecast_id, ff.question_text) q;
  IF n <> 0 THEN
    RAISE EXCEPTION 'detector fired with no open challenges (resolved/orphaned/non-challenge leaked)';
  END IF;
END $$;

-- One genuine open challenge, and it must fire — with the driver named,
-- since "which assumption" is the whole reason this beats `contested`.
INSERT INTO public.driver_annotations (forecast_id, driver_name, author_id, body, kind, status)
VALUES ('fc-det', 'elo_current', 'u2', 'base rate is wrong', 'challenge', 'open');

DO $$
DECLARE d TEXT[]; c BIGINT;
BEGIN
  SELECT ARRAY_AGG(DISTINCT a.driver_name) FILTER (WHERE a.driver_name IS NOT NULL), COUNT(*)
    INTO d, c
    FROM public.driver_annotations a
    JOIN public.fermi_forecasts ff ON ff.id = a.forecast_id
   WHERE a.forecast_id = ANY(ARRAY['fc-det'])
     AND a.status = 'open' AND a.kind = 'challenge' AND ff.status = 'active'
   GROUP BY a.forecast_id, ff.question_text;
  IF c IS DISTINCT FROM 1 OR d IS DISTINCT FROM ARRAY['elo_current'] THEN
    RAISE EXCEPTION 'detector should report exactly 1 challenge on elo_current, got % on %', c, d;
  END IF;
END $$;

-- A resolved forecast is not coordination work, whatever is open on it.
UPDATE public.fermi_forecasts SET status = 'resolved' WHERE id = 'fc-det';
DO $$
DECLARE n INT;
BEGIN
  SELECT count(*) INTO n FROM (
    SELECT a.forecast_id
      FROM public.driver_annotations a
      JOIN public.fermi_forecasts ff ON ff.id = a.forecast_id
     WHERE a.forecast_id = ANY(ARRAY['fc-det'])
       AND a.status = 'open' AND a.kind = 'challenge' AND ff.status = 'active'
     GROUP BY a.forecast_id) q;
  IF n <> 0 THEN
    RAISE EXCEPTION 'detector still fires on a resolved forecast';
  END IF;
END $$;

DEALLOCATE c1;
DELETE FROM public.driver_annotations WHERE forecast_id = 'fc-det';
DELETE FROM public.fermi_forecasts WHERE id = 'fc-det';

\echo ''
\echo '=== PART C complete ==='
