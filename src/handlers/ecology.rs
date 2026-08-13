//! Ecology — what lives on the platform, how it's organised, how it got there.
//!
//! # Why a separate lens
//!
//! The Observatory is a *clinical* view: one agent at a time, is it healthy,
//! is it behaving. That works because it has a point of view. The generic
//! agent browser has none, so it degrades into a list — and a list of 104
//! agents answers no question anyone actually has.
//!
//! This is the ecological view: population, habitats, niches, and
//! relationships. Not "show me all agents" but "what kind of place is this,
//! and is anything out of order".
//!
//! # Governance is the organising signal, not a footnote
//!
//! Every member carries `membership_source` (SPEC_29): `approved` has a
//! reviewed request behind it, `curated_seed` is platform boot-seeded, and
//! `admin_grant` is an override. Rendering that as first-class visual state
//! is the point of the view.
//!
//! Concretely: nine third-party agents entered the Fermi orchestra without
//! review, and it took a code audit to notice, because every surface showed
//! them identically to legitimately-seeded platform agents. On a map where
//! provenance is a colour, that is visible at a glance. A view worth
//! building is one where the anomaly finds *you*.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};
use sqlx::Row;

use crate::AppState;

/// GET /api/ecology/specimens
///
/// Every published agent as a full dossier, in one call, so the register
/// and the specimen sheet need no per-agent round trips.
///
/// Built from `agents::build_agent_json` — the same merge the catalogue
/// uses — because the interesting material lives in the on-disk
/// `agent_card.json`, not the `agents` table: the seven-rank taxonomy,
/// `valence` (affect + personality traits), `domain_knowledge`, and the
/// `accepts`/`produces` interfaces. A second hand-rolled merge here would
/// drift from the catalogue within a release.
///
/// Orchestra membership and its provenance are joined on, so the register
/// can colour a specimen by how it was admitted.
pub async fn ecology_specimens_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let agents = state
        .memory_store
        .list_agents()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Membership + provenance per agent, one query rather than N.
    let grants =
        sqlx::query("SELECT agent_id, orchestra_name, source FROM public.orchestra_members")
            .fetch_all(&state.db)
            .await
            .unwrap_or_default();
    let mut membership: std::collections::HashMap<uuid::Uuid, Vec<(String, String)>> =
        Default::default();
    for g in &grants {
        if let (Ok(id), Ok(orch), Ok(src)) = (
            g.try_get::<uuid::Uuid, _>("agent_id"),
            g.try_get::<String, _>("orchestra_name"),
            g.try_get::<String, _>("source"),
        ) {
            membership.entry(id).or_default().push((orch, src));
        }
    }

    // Evolution badges for the whole fleet in one round trip. Per-agent
    // computation here would be ~100 queries on a page load; see
    // `evolution::fleet_evolution`.
    let badges = crate::handlers::evolution::fleet_evolution(&state.db).await;

    // The population this lens describes.
    //
    // Integration-test scaffolding (`test_agent_<uuid>`) is hidden from the
    // ecological view for the same reason it is hidden from rosters: it is
    // not part of the population anyone is asking about, and it dominates
    // the counts. Policy is hide-not-delete.
    let population: Vec<_> = agents
        .iter()
        .filter(|a| a.status.eq_ignore_ascii_case("published"))
        .filter(|a| !crate::handlers::is_test_cruft(&a.agent_name))
        .collect();

    // Measured execution stats for the whole population in one round trip.
    //
    // This lens *does* render run counts: the specimen sheet's "Vital signs"
    // panel shows Runs, Succeeded and Cost to date straight off
    // `execution_stats`. Passing `None` to `build_agent_json` fell back to
    // `agents.total_executions` / `successful_executions` / `total_cost_usd`,
    // which no code path writes — so every specimen in the register read 0
    // runs and $0.0000 spent, including agents with hundreds of real
    // episodes. See migrations/192 and src/rollup_trust.rs.
    //
    // Agents with no episodes are absent from the rollup, so a missing entry
    // becomes `MeasuredExecStats::default()` — "never ran", which is true.
    let specimen_ids: Vec<uuid::Uuid> = population.iter().map(|a| a.agent_id).collect();
    let exec_stats = crate::agent_economics::measured_exec_stats(&state.db, &specimen_ids).await;

    let specimens: Vec<Value> = population
        .iter()
        .map(|a| {
            let measured = exec_stats.get(&a.agent_id).copied().unwrap_or_default();
            let mut v =
                crate::handlers::agents::build_agent_json(&state, a, None, 0, Some(&measured));

            // mig-186 — taxonomy from the DB row wins over the on-disk card.
            //
            // `build_agent_json` merges the card, which for curated agents
            // carries a taxonomy. But the DB column is what agents authored
            // through the API have, and it is also what the boot seeder
            // refreshes derived ranks into — so the row is the fresher of the
            // two. Reading only the card is what left every DB-native agent
            // undescribed.
            if let Some(tax) = a.taxonomy.clone() {
                if let Some(obj) = v.as_object_mut() {
                    let md = obj.entry("metadata").or_insert_with(|| json!({}));
                    if let Some(md_obj) = md.as_object_mut() {
                        md_obj.insert("taxonomy".into(), tax);
                    }
                }
            }

            let habitats: Vec<Value> = membership
                .get(&a.agent_id)
                .map(|ms| {
                    ms.iter()
                        .map(|(o, s)| json!({ "orchestra": o, "source": s }))
                        .collect()
                })
                .unwrap_or_default();
            if let Some(obj) = v.as_object_mut() {
                // Every published agent is implicitly in xaman_ek; only
                // explicit habitats carry a provenance worth showing.
                obj.insert("habitats".into(), Value::Array(habitats));

                // Evolution: the earned rank, plus the forecasting track record
                // that backs it. Public shape only — `public_badge_json` is the
                // single constructor, so regression and the high-water mark
                // cannot leak onto an anonymous-readable surface.
                //
                // An agent nobody has exercised comes back `ranked: false` with
                // no rank name rather than the bottom rung: untried is not the
                // same claim as measured-and-failing.
                if let Some(f) = badges.get(&a.agent_id) {
                    let ev = crate::handlers::evolution::compute_evolution(f.inputs, f.peak_level);
                    obj.insert(
                        "evolution".into(),
                        crate::handlers::evolution::public_badge_json(&ev, f),
                    );
                }
            }
            v
        })
        .collect();

    Ok(Json(json!({ "specimens": specimens })))
}

/// GET /api/ecology/overview
///
/// One aggregated payload. Anonymous-readable: this is public-facing
/// information about the shape of the catalogue, the same population data
/// the marketplace already exposes per-agent.
pub async fn ecology_overview_handler(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let db = &state.db;

    // ── Population ────────────────────────────────────────────────
    // Published, non-test. `NOT LIKE 'test\_agent\_%'` with escaped
    // underscores — unescaped, `_` is a single-char wildcard and would
    // also swallow innocent names.
    //
    // `runs` comes from `agent_execution_rollup` (i.e. from `episodes`), not
    // from `SUM(agents.total_executions)` — nothing writes that column, so
    // this aggregate reported 0 lifetime runs for the entire population
    // while `episodes` held thousands. See migrations/192 and
    // src/rollup_trust.rs.
    //
    // LEFT JOIN because an agent with no episodes is absent from the view
    // rather than present with a zero; an inner join would silently drop
    // never-run agents out of the `n` census counts as well. The outer SUM
    // over bigint yields NUMERIC, hence the explicit ::bigint for the i64
    // read below.
    let population = sqlx::query(
        "SELECT a.tier, a.agent_type, COALESCE(a.llm_provider,'anthropic') AS provider,
                COUNT(*) AS n,
                COALESCE(SUM(COALESCE(r.executions, 0)), 0)::bigint AS runs
           FROM public.agents a
           LEFT JOIN public.agent_execution_rollup r ON r.agent_id = a.agent_id
          WHERE a.status = 'published'
            AND a.agent_name NOT LIKE 'test\\_agent\\_%'
          GROUP BY a.tier, a.agent_type, provider",
    )
    .fetch_all(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut by_tier: std::collections::BTreeMap<String, i64> = Default::default();
    let mut by_niche: std::collections::BTreeMap<String, i64> = Default::default();
    let mut by_provider: std::collections::BTreeMap<String, i64> = Default::default();
    let mut total: i64 = 0;
    let mut total_runs: i64 = 0;
    for r in &population {
        let n: i64 = r.try_get("n").unwrap_or(0);
        total += n;
        // `runs` is a non-null bigint (COALESCE + explicit cast in the
        // query). The view's `executions` is bigint where the column it
        // replaced was INTEGER — an i32 read here would fail to decode and
        // fall back to 0, reproducing the bug this query was changed to fix.
        total_runs += r.try_get::<i64, _>("runs").unwrap_or(0);
        *by_tier
            .entry(r.try_get("tier").unwrap_or_default())
            .or_default() += n;
        *by_niche
            .entry(r.try_get("agent_type").unwrap_or_default())
            .or_default() += n;
        *by_provider
            .entry(r.try_get("provider").unwrap_or_default())
            .or_default() += n;
    }

    // ── Habitats ──────────────────────────────────────────────────
    let mut habitats: Vec<Value> = Vec::new();

    // Fermi: explicit membership, so provenance is meaningful.
    //
    // Runs come from `agent_execution_rollup`. This used to join `agents`
    // solely to read `a.total_executions`, which nothing writes — so the
    // roster showed 0 runs against every member. `orchestra_fermi_members`
    // is itself defined over `agents` (migrations/180, 185), so that join
    // was only ever a detour to reach the dead counter and is dropped here;
    // the LEFT JOIN to the rollup keeps members with no episodes on the
    // roster, reported as 0 rather than omitted. See migrations/192 and
    // src/rollup_trust.rs.
    let fermi_rows = sqlx::query(
        "SELECT m.agent_name, m.agent_type, m.tier, m.description,
                m.membership_source, m.membership_granted_at, m.owner_user_id,
                COALESCE(r.executions, 0) AS runs
           FROM public.orchestra_fermi_members m
           LEFT JOIN public.agent_execution_rollup r ON r.agent_id = m.agent_id
          ORDER BY m.membership_source, m.agent_name",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let mut fermi_prov: std::collections::BTreeMap<String, i64> = Default::default();
    let fermi_members: Vec<Value> = fermi_rows
        .iter()
        .map(|r| {
            let src: String = r
                .try_get::<Option<String>, _>("membership_source")
                .ok()
                .flatten()
                .unwrap_or_else(|| "unknown".into());
            *fermi_prov.entry(src.clone()).or_default() += 1;
            json!({
                "agent_name": r.try_get::<String,_>("agent_name").ok(),
                "agent_type": r.try_get::<String,_>("agent_type").ok(),
                "tier":       r.try_get::<String,_>("tier").ok(),
                "description":r.try_get::<Option<String>,_>("description").ok().flatten(),
                // bigint from the view, not the INTEGER column it replaced:
                // an i32 read here fails to decode and silently falls back
                // to 0, which is the bug this query was changed to fix.
                "runs":       r.try_get::<i64,_>("runs").unwrap_or(0),
                "membership_source": src,
                "granted_at": r.try_get::<Option<chrono::DateTime<chrono::Utc>>,_>("membership_granted_at")
                                .ok().flatten().map(|d| d.to_rfc3339()),
                "owner":      r.try_get::<Option<String>,_>("owner_user_id").ok().flatten(),
            })
        })
        .collect();

    habitats.push(json!({
        "name": "fermi",
        "kind": "explicit",
        "rule": "admitted by review — a grant in orchestra_members",
        "population": fermi_members.len(),
        "provenance": fermi_prov,
        "members": fermi_members,
    }));

    // xaman_ek: implicit. Everyone published lives here, so listing the
    // roster adds nothing the population figures don't already say.
    let xaman: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM public.orchestra_xaman_ek_members")
        .fetch_one(db)
        .await
        .unwrap_or(0);
    habitats.push(json!({
        "name": "xaman_ek",
        "kind": "implicit",
        "rule": "publishing is joining — no admission decision exists",
        "population": xaman,
        "provenance": json!({ "implicit": xaman }),
        "members": Value::Array(vec![]),
    }));

    // ── Governance health ─────────────────────────────────────────
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM public.orchestra_membership_requests WHERE status = 'pending'",
    )
    .fetch_one(db)
    .await
    .unwrap_or(0);
    let approvals_ever: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM public.orchestra_membership_requests WHERE status = 'approved'",
    )
    .fetch_one(db)
    .await
    .unwrap_or(0);

    // Third-party members holding a place with no review behind them.
    // Platform tiers are expected to be curated_seed; community agents are
    // not — those are the ones that entered through a gap.
    let unreviewed = sqlx::query(
        "SELECT m.agent_name, m.tier, m.owner_user_id, m.membership_source
           FROM public.orchestra_fermi_members m
          WHERE m.membership_source <> 'approved'
            AND lower(m.tier) NOT IN ('system','curated')
          ORDER BY m.agent_name",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();
    let unreviewed: Vec<Value> = unreviewed
        .iter()
        .map(|r| {
            json!({
                "agent_name": r.try_get::<String,_>("agent_name").ok(),
                "tier":       r.try_get::<String,_>("tier").ok(),
                "owner":      r.try_get::<Option<String>,_>("owner_user_id").ok().flatten(),
                "membership_source": r.try_get::<Option<String>,_>("membership_source").ok().flatten(),
            })
        })
        .collect();

    // ── Co-habitation ─────────────────────────────────────────────
    // Agents convened together. The closest thing to an observed
    // relationship the platform currently records — `dyad_state` and
    // `composition_versions` are both empty.
    //
    // Counted over DISTINCT ROSTERS, not workspace instances. That
    // distinction is the whole measurement: 185 of ~199 workspaces are
    // exact template clones (111 share one roster hash, 74 another), so
    // counting instances reports every pair in the default template as
    // co-occurring 111 times. That number measures how often the template
    // was spawned, not whether two agents have any affinity — and it looks
    // like a strong signal, which makes it worse than no number at all.
    //
    // Deduplicated, the honest current answer is that almost no pair
    // recurs across genuinely different teams. The panel says so.
    //
    // Capped at rosters of <= 12: pairs grow quadratically, and a
    // 40-agent workspace says nothing about any particular two of them.
    let cohab = sqlx::query(
        "WITH rosters AS (
             SELECT workspace_id,
                    md5(string_agg(agent_id::text, ',' ORDER BY agent_id)) AS roster_hash
               FROM public.workspace_agents
              GROUP BY workspace_id
             HAVING COUNT(*) BETWEEN 2 AND 12
         ),
         distinct_rosters AS (
             SELECT DISTINCT ON (roster_hash) roster_hash, workspace_id FROM rosters
         )
         SELECT a1.agent_name AS a, a2.agent_name AS b, COUNT(*) AS distinct_teams
           FROM distinct_rosters dr
           JOIN public.workspace_agents w1 ON w1.workspace_id = dr.workspace_id
           JOIN public.workspace_agents w2
             ON w2.workspace_id = dr.workspace_id AND w1.agent_id < w2.agent_id
           JOIN public.agents a1 ON a1.agent_id = w1.agent_id
           JOIN public.agents a2 ON a2.agent_id = w2.agent_id
          WHERE a1.agent_name NOT LIKE 'test\\_agent\\_%'
            AND a2.agent_name NOT LIKE 'test\\_agent\\_%'
          GROUP BY 1,2 HAVING COUNT(*) > 1
          ORDER BY distinct_teams DESC, a, b LIMIT 40",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();
    let cohabitation: Vec<Value> = cohab
        .iter()
        .map(|r| {
            json!({
                "a": r.try_get::<String,_>("a").ok(),
                "b": r.try_get::<String,_>("b").ok(),
                "distinct_teams": r.try_get::<i64,_>("distinct_teams").unwrap_or(0),
            })
        })
        .collect();

    // How much of the workspace corpus is template clones. Surfaced so the
    // thinness of the co-habitation panel is explained rather than
    // mistaken for a bug.
    let (distinct_rosters, total_rosters): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(DISTINCT roster_hash), COUNT(*) FROM (
             SELECT md5(string_agg(agent_id::text, ',' ORDER BY agent_id)) AS roster_hash
               FROM public.workspace_agents GROUP BY workspace_id
         ) x",
    )
    .fetch_one(db)
    .await
    .unwrap_or((0, 0));

    // ── Evolution census: how much of the population has actually earned ────
    //
    // The distinction that makes this honest is `unranked`. Most agents have
    // never been exercised, and folding them into the bottom rank would read as
    // a population of failures rather than a population of untried specimens.
    // Test scaffolding is excluded, as everywhere else in this view.
    let badges = crate::handlers::evolution::fleet_evolution(&state.db).await;
    let mut by_rank: std::collections::BTreeMap<String, i64> = Default::default();
    let mut unranked = 0i64;
    let mut with_forecasting_record = 0i64;
    let mut beating_base_rate = 0i64;
    for (_, f) in badges.iter() {
        let ev = crate::handlers::evolution::compute_evolution(f.inputs, f.peak_level);
        if ev.ranked {
            *by_rank
                .entry(ev.rank.clone().unwrap_or_else(|| "unknown".into()))
                .or_insert(0) += 1;
        } else {
            unranked += 1;
        }
        if f.inputs.n_forecasts > 0 {
            with_forecasting_record += 1;
            if f.brier_skill.map(|s| s > 0.0).unwrap_or(false) {
                beating_base_rate += 1;
            }
        }
    }

    Ok(Json(json!({
        "population": {
            "published": total,
            "total_runs": total_runs,
            "by_tier": by_tier,
            "by_niche": by_niche,
            "by_provider": by_provider,
        },
        "evolution": {
            "by_rank": by_rank,
            // Not a rank of zero: these have produced no usage data at all, so
            // there is nothing to grade yet.
            "unranked_pending_usage": unranked,
            "with_forecasting_record": with_forecasting_record,
            "beating_base_rate": beating_base_rate,
            "note": "Ranks are earned from outcomes, not activity. `unranked_pending_usage` \
                     agents have never been exercised — untried, not failing.",
        },
        "habitats": habitats,
        "governance": {
            "pending_requests":   pending,
            "approvals_ever":     approvals_ever,
            "unreviewed_members": unreviewed,
        },
        "cohabitation": {
            "pairs": cohabitation,
            "distinct_rosters": distinct_rosters,
            "total_workspaces": total_rosters,
        },
    })))
}
