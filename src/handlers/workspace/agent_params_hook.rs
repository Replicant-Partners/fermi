//! Post-agent hook: extract multiplier recommendations from agent evidence
//! and write them to the workspace's params output.
//!
//! Called from the execution handler after a successful agent run. Scans the
//! agent's output evidence for `[MULTIPLIER] Suggested p50: X (p5: Y, p95: Z)`
//! patterns and maps them to the correct driver param keys via the agent's
//! `driver_refs` declarations.
//!
//! If the agent ran in a workspace context AND produced a multiplier, this
//! hook writes `{ <driver>_p5, <driver>_p50, <driver>_p95 }` to the
//! workspace's `params` output and triggers a refit.
//!
//! # Binding an agent to the drivers it researches
//!
//! The FPL already states this, authoritatively:
//!
//! ```text
//! agent football_analyst {
//!     driver_refs: ["dynamic_performance", "squad_quality", "tactical_efficiency"]
//! }
//! ```
//!
//! So [`resolve_driver_prefixes`] reads the workspace's own program and uses
//! that. The compile-time table it replaced was a second copy of the same
//! fact, and it failed in the two ways a closed-world table always does:
//!
//! * **It locked out every agent not enumerated at compile time.** An agent
//!   admitted to the orchestra and declaring `driver_refs` still had its
//!   multiplier silently discarded, because the `_ => &[]` arm returned no
//!   prefixes and the caller treated that as "nothing to do". No error, no
//!   log, no claim row — so the forecast was also unattributable to it.
//!
//! * **It mis-fired on substring collisions.** The arm was
//!   `n if n.contains("analyst")`, so `weather_market_analyst` bound to the
//!   World Cup drivers `dynamic`/`squad`/`tactical` and wrote football params
//!   into an unrelated workspace. This is the same defect class documented in
//!   `fermi-console`'s `routing.rs` (`"pre-industrial".contains("trial")`
//!   routed a climate driver to `biotech_analyst`); that file moved to
//!   whole-word matching, this one had not.
//!
//! The legacy table survives only as [`legacy_driver_prefixes`], reached when
//! the program cannot be loaded or declares no `driver_refs` for this agent —
//! and it now matches on whole words, so the collision above cannot recur.

use fermi::lexer::Lexer;
use fermi::parser::Parser;
use regex::Regex;
use serde_json::{json, Value as JsonValue};
use sqlx::PgPool;
use std::sync::LazyLock;
use uuid::Uuid;

/// Whole-word containment: `needle` must be delimited by non-alphanumerics.
///
/// `"weather_market_analyst"` contains the *word* `analyst`, so this alone
/// does not fix the collision — [`legacy_driver_prefixes`] additionally
/// requires the name to be one of the known World Cup agents. This helper
/// exists so a needle like `institution` cannot match `institutional_reform`
/// mid-word, matching the discipline `routing.rs` already adopted.
fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    let mut from = 0;
    while let Some(rel) = haystack[from..].find(needle) {
        let start = from + rel;
        let end = start + needle.len();
        let before_ok = start == 0 || !bytes[start - 1].is_ascii_alphanumeric();
        let after_ok = end == bytes.len() || !bytes[end].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

/// The World Cup `team_prior` naming convention, as a last resort.
///
/// Scoped to the four agents that template actually declares. An agent
/// outside that set gets nothing from here even if its name shares a word —
/// which is what stops `weather_market_analyst` inheriting football drivers.
fn legacy_driver_prefixes(agent_name: &str) -> &'static [&'static str] {
    const WC_AGENTS: &[&str] = &[
        "macro_data_agent",
        "football_institution_agent",
        "football_analyst",
        "fixture_context_agent",
    ];
    if !WC_AGENTS.contains(&agent_name) {
        return &[];
    }
    match agent_name {
        n if contains_word(n, "macro_data") => &["socio"],
        n if contains_word(n, "institution") => &["institutional"],
        n if contains_word(n, "analyst") => &["dynamic", "squad", "tactical"],
        n if contains_word(n, "fixture") => &["fixture"],
        _ => &[],
    }
}

/// Map an FPL driver name to the param prefix its distribution reads.
///
/// `team_prior.fpl` writes `driver socio_capital { triangular(socio_p5, …) }`,
/// so the prefix is the leading token of the driver name rather than the whole
/// name. Where a driver's params are named after the driver itself, the full
/// name is already the prefix and this is the identity.
fn param_prefix_for_driver(driver_name: &str) -> String {
    match driver_name {
        "socio_capital" => "socio".to_string(),
        "institutional_capacity" => "institutional".to_string(),
        "dynamic_performance" => "dynamic".to_string(),
        "squad_quality" => "squad".to_string(),
        "tactical_efficiency" => "tactical".to_string(),
        "fixture_context" => "fixture".to_string(),
        other => other.to_string(),
    }
}

/// Which driver param prefixes this agent's multiplier should update.
///
/// Declaration first: the workspace's own FPL is authoritative. Falls back to
/// the legacy World Cup table only when the program is unavailable or silent
/// about this agent, and logs which path was taken so a binding that quietly
/// resolves to nothing is findable rather than invisible.
async fn resolve_driver_prefixes(
    pool: &PgPool,
    workspace_id: Uuid,
    agent_name: &str,
) -> Vec<String> {
    let declared = load_declared_driver_refs(pool, workspace_id, agent_name).await;

    match declared {
        Some(refs) if !refs.is_empty() => {
            let prefixes: Vec<String> = refs.iter().map(|d| param_prefix_for_driver(d)).collect();
            tracing::info!(
                workspace = %workspace_id, agent = %agent_name,
                drivers = ?refs, prefixes = ?prefixes,
                "[multiplier] bound by FPL driver_refs"
            );
            prefixes
        }
        _ => {
            let legacy: Vec<String> = legacy_driver_prefixes(agent_name)
                .iter()
                .map(|s| s.to_string())
                .collect();
            if legacy.is_empty() {
                // The actionable case: an agent produced a multiplier and
                // there is nowhere to put it. Previously silent.
                tracing::warn!(
                    workspace = %workspace_id, agent = %agent_name,
                    "[multiplier] agent produced a multiplier but no driver binding \
                     was found — the FPL declares no `driver_refs` for it and it is \
                     not a legacy World Cup agent. The claim will not be recorded and \
                     the forecast will not be attributable to this agent. Declare an \
                     `agent <name> {{ driver_refs: [...] }}` block in the program."
                );
            } else {
                tracing::info!(
                    workspace = %workspace_id, agent = %agent_name, prefixes = ?legacy,
                    "[multiplier] bound by legacy World Cup table (FPL declared no driver_refs)"
                );
            }
            legacy
        }
    }
}

/// Read `agent <name> { driver_refs: [...] }` from the workspace's program.
///
/// Returns `None` when there is no linked forecast, no `fpl_source`, or the
/// source does not parse — all of which mean "fall back", not "no drivers".
async fn load_declared_driver_refs(
    pool: &PgPool,
    workspace_id: Uuid,
    agent_name: &str,
) -> Option<Vec<String>> {
    let fpl_source: Option<String> = sqlx::query_scalar(
        "SELECT fpl_source FROM fermi_forecasts \
         WHERE workspace_id = $1 AND fpl_source IS NOT NULL \
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let source = fpl_source?;
    let tokens = Lexer::new(&source).tokenize().ok()?;
    let program = Parser::new(tokens).parse().ok()?;

    let refs: Vec<String> = program
        .agents()
        .iter()
        .filter(|a| a.name == agent_name)
        .flat_map(|a| a.driver_refs.iter().cloned())
        .collect();

    Some(refs)
}

/// Try to extract a `(p5, p50, p95)` multiplier from an evidence summary.
///
/// Delegates to [`fermi::assertions::extract_from_prose`], which owns the only
/// pattern in the codebase. It used to own a second one — narrower, unable to
/// read the markdown emphasis the model actually writes, and therefore
/// disagreeing with the extractor about whether a line contained a claim at all.
/// Two readers of one format is two answers to the same question, and the one
/// that disagrees is whichever the caller happens to reach first.
///
/// Retained as a function because it is the shape callers want and because its
/// tests are worth keeping; it is no longer a second implementation.
pub fn extract_multiplier(summary: &str) -> Option<(f64, f64, f64)> {
    let (found, _rejected) = fermi::assertions::extract_from_prose(summary);
    found
        .first()
        .map(|a| (a.value.p5, a.value.p50, a.value.p95))
}

/// Write an agent's multiplier evidence into the workspace's params output.
///
/// Called after a successful agent execution. Returns true if params were
/// actually updated.
/// `episode_id` correlates the claim to the execution that produced it
/// (migration 197). Allocated by the caller *before* this hook is spawned,
/// because the hook and the episode write race and the claim usually lands
/// first — so the id cannot be read back from the episode row. `None` only
/// for callers with no episode to point at.
pub async fn apply_agent_multipliers(
    pool: &PgPool,
    registry: &posterior::ExtractorRegistry,
    workspace_id: Uuid,
    agent_name: &str,
    evidence: &[fermi::ast::EvidenceStmt],
    episode_id: Option<Uuid>,
) -> Result<bool, String> {
    let driver_prefixes = resolve_driver_prefixes(pool, workspace_id, agent_name).await;
    if driver_prefixes.is_empty() {
        return Ok(false);
    }

    // Recover the agent's quantified judgements through the shared extractor
    // (mig-205), so the claim written here and the assertion recorded on the
    // episode are the same object seen twice rather than two regex results that
    // can disagree.
    //
    // The old code matched on `EvidenceStmt.summary` with a pattern that could
    // not read markdown emphasis: 12 of 22 lines this platform produced were
    // unreadable, every one because the model wrote `**1.15**`. It also took the
    // FIRST match and `break`, then stamped that single triple onto every driver
    // the agent covered — so `football_analyst`, asked for three distinct
    // factors, recorded three claims of one number and the comment said so
    // outright. Both are fixed here: every match is recovered, and each claim
    // carries the `assertion_id` of the judgement it came from, so three
    // bindings of one judgement stay distinguishable from three judgements.
    let mut assertions: Vec<fermi::assertions::Assertion> = Vec::new();
    for ev in evidence {
        if let Some(summary) = ev.summary.as_deref() {
            let (found, _rejected) = fermi::assertions::extract_from_prose(summary);
            assertions.extend(found);
        }
        for finding in &ev.key_findings {
            // The `[MULTIPLIER]` line is a key finding, not a summary, on the
            // executors that split them. Reading only `summary` is how a
            // correctly-formatted claim could still be missed.
            let (found, _rejected) = fermi::assertions::extract_from_prose(finding);
            assertions.extend(found);
        }
    }

    // Still a single triple applied to the agent's drivers, because that is what
    // the output format can carry. What changed is that the platform now records
    // WHICH judgement it was, so the day an agent emits one assertion per factor
    // this binds them separately without a schema change.
    let Some(primary) = assertions.first().cloned() else {
        return Ok(false);
    };
    let (p5, p50, p95) = (primary.value.p5, primary.value.p50, primary.value.p95);
    let assertion_id = primary.assertion_id;
    if assertions.len() > 1 {
        tracing::info!(
            agent = %agent_name,
            recovered = assertions.len(),
            bound = 1,
            "multiple assertions recovered; the format carries one, so the rest \
             are recorded on the episode and not bound to a driver"
        );
    }

    // ── Retain the claim itself (mig-187) ─────────────────────────────────
    //
    // The params UPSERT below is CURRENT STATE: the next agent's write, or the
    // next run, overwrites it. That made every resolved forecast permanently
    // unattributable at the agent level, because the per-agent inputs that
    // produced it no longer existed.
    //
    // This ledger is what makes per-agent credit possible at all: knowing what
    // each agent individually claimed lets the attribution engine synthesise
    // the forecast for any SUBSET of agents (applying that subset's claims,
    // neutralising the rest) and so compute exact Shapley credit from a single
    // real forecast — no need for real-world composition permutations. See
    // src/attribution/ and migrations/187_forecast_agent_claims.sql.
    //
    // Append-only and best-effort: a failure here must never fail the agent
    // run, but it is logged at warn because a silent gap here is unrecoverable
    // later — claims cannot be reconstructed after the fact.
    let claim_agent_id: Option<Uuid> =
        sqlx::query_scalar::<_, Uuid>("SELECT agent_id FROM agents WHERE agent_name = $1 LIMIT 1")
            .bind(agent_name)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();

    let raw_evidence: Option<&str> = evidence
        .iter()
        .filter_map(|e| e.summary.as_deref())
        .find(|s| extract_multiplier(s).is_some());

    for prefix in &driver_prefixes {
        let res = sqlx::query(
            "INSERT INTO forecast_agent_claims
                 (workspace_id, agent_id, agent_name, driver,
                  p5, p50, p95, neutral_value, source, raw_evidence, episode_id,
                  assertion_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 1.0, 'multiplier_hook', $8, $9,
                     $10)",
        )
        .bind(workspace_id)
        .bind(claim_agent_id)
        .bind(agent_name)
        .bind(prefix)
        .bind(p5 as f32)
        .bind(p50 as f32)
        .bind(p95 as f32)
        .bind(raw_evidence)
        // mig-197: the exact correlation id. Replaces the (agent_id, driver,
        // time-window) heuristic that could not distinguish two runs of the
        // same agent on the same driver — the case that matters most, since
        // a re-run after a correction is exactly when attribution is asked for.
        .bind(episode_id)
        // mig-205: which judgement this binding applies. A claim is an assertion
        // bound to a driver, so the same assertion appearing on three drivers is
        // now visibly one judgement rather than three.
        .bind(assertion_id)
        .execute(pool)
        .await;

        if let Err(e) = res {
            tracing::warn!(
                workspace = %workspace_id, agent = %agent_name, driver = %prefix, error = %e,
                "[claims] failed to record agent claim — this forecast will not be \
                 attributable per-agent and cannot be backfilled"
            );
        }
    }

    // Build the update: { <driver>_p5, <driver>_p50, <driver>_p95 } for each driver.
    let mut update = JsonValue::Object(serde_json::Map::new());
    for prefix in &driver_prefixes {
        update[format!("{}_p5", prefix)] = json!(p5);
        update[format!("{}_p50", prefix)] = json!(p50);
        update[format!("{}_p95", prefix)] = json!(p95);
    }

    // Read current params.
    let current = sqlx::query_as::<_, (JsonValue,)>(
        "SELECT value FROM workspace_outputs WHERE workspace_id = $1 AND key = 'params'",
    )
    .bind(workspace_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut merged = match current {
        Some((val,)) => val.as_object().cloned().unwrap_or_default(),
        None => serde_json::Map::new(),
    };

    // Merge: agent's values take precedence.
    if let Some(obj) = update.as_object() {
        for (k, v) in obj {
            merged.insert(k.clone(), v.clone());
        }
    }

    let merged_val = JsonValue::Object(merged);

    // UPSERT the params output.
    sqlx::query(
        r#"INSERT INTO workspace_outputs (workspace_id, key, value, updated_at)
           VALUES ($1, 'params', $2, NOW())
           ON CONFLICT (workspace_id, key)
           DO UPDATE SET value = $2, updated_at = NOW()"#,
    )
    .bind(workspace_id)
    .bind(&merged_val)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    // Trigger a refit so the FPL re-evaluates with the new driver params.
    // Fire-and-forget — the caller doesn't need to wait for the refit to complete.
    let pool_clone = pool.clone();
    tokio::spawn(async move {
        let trigger = crate::handlers::workspace::refit::TriggerReason::Manual {
            user_id: "agent_hook".into(),
        };
        if let Err(e) = crate::handlers::workspace::refit::refit_workspace(
            &pool_clone,
            &Default::default(),
            workspace_id,
            trigger,
        )
        .await
        {
            tracing::warn!(workspace = %workspace_id, error = %e, "post-agent refit failed");
        }
    });

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_multiplier_simple() {
        let summary = "Some text [MULTIPLIER] Suggested p50: 1.25 (p5: 1.10, p95: 1.45) more text";
        let result = extract_multiplier(summary);
        assert_eq!(result, Some((1.10, 1.25, 1.45)));
    }

    /// The regression this module's rewrite exists to prevent.
    ///
    /// `weather_market_analyst` contains the word `analyst`, so the old
    /// `n if n.contains("analyst")` arm bound it to the World Cup drivers and
    /// wrote `dynamic_p50` / `squad_p50` / `tactical_p50` into an unrelated
    /// workspace's params. Any third-party agent named `*_analyst` inherited
    /// football drivers the same way.
    #[test]
    fn foreign_analyst_agents_do_not_inherit_world_cup_drivers() {
        for agent in [
            "weather_market_analyst",
            "equity_analyst",
            "biotech_analyst",
            "sentiment_analyzer",
            "some_third_party_analyst",
        ] {
            assert!(
                legacy_driver_prefixes(agent).is_empty(),
                "{agent} must not bind to World Cup drivers via the legacy table"
            );
        }
    }

    #[test]
    fn legacy_table_still_serves_the_world_cup_agents_it_was_written_for() {
        assert_eq!(legacy_driver_prefixes("macro_data_agent"), &["socio"]);
        assert_eq!(
            legacy_driver_prefixes("football_institution_agent"),
            &["institutional"]
        );
        assert_eq!(
            legacy_driver_prefixes("football_analyst"),
            &["dynamic", "squad", "tactical"]
        );
        assert_eq!(
            legacy_driver_prefixes("fixture_context_agent"),
            &["fixture"]
        );
    }

    #[test]
    fn contains_word_respects_boundaries() {
        // The defect class routing.rs documented: substrings inside longer words.
        assert!(!contains_word("pre-industrial", "trial"));
        assert!(!contains_word("development", "elo"));
        assert!(!contains_word("institutional_reform", "institution"));
        // Delimited by underscores or hyphens still counts as a word.
        assert!(contains_word("macro_data_agent", "macro_data"));
        assert!(contains_word("football-analyst", "analyst"));
        assert!(contains_word("analyst", "analyst"));
        assert!(!contains_word("anything", ""));
    }

    /// The FPL is the authority, so its driver names must map onto the param
    /// prefixes the template's distributions actually read. `team_prior.fpl`
    /// declares `triangular(socio_p5, ...)` for a driver called
    /// `socio_capital`, so the prefix is not the driver name.
    #[test]
    fn declared_driver_names_map_to_team_prior_param_prefixes() {
        assert_eq!(param_prefix_for_driver("socio_capital"), "socio");
        assert_eq!(
            param_prefix_for_driver("institutional_capacity"),
            "institutional"
        );
        assert_eq!(param_prefix_for_driver("dynamic_performance"), "dynamic");
        assert_eq!(param_prefix_for_driver("squad_quality"), "squad");
        assert_eq!(param_prefix_for_driver("tactical_efficiency"), "tactical");
        assert_eq!(param_prefix_for_driver("fixture_context"), "fixture");
        // An agent-authored driver with no special case is its own prefix,
        // so a new domain needs no entry here.
        assert_eq!(param_prefix_for_driver("station_bias"), "station_bias");
    }

    /// An arbitrary agent becomes bindable purely by being declared in the
    /// program — no Rust change, which is the whole point of reading the FPL.
    #[test]
    fn driver_refs_are_readable_from_an_arbitrary_program() {
        let source = r#"
question "Will the LaGuardia high land in the 86-87F bucket on 2026-08-16?"

driver station_bias continuous {
    distribution: triangular(0.9, 1.0, 1.1)
    rationale: "Residual correction at the settlement gauge"
}

agent weather_oracle {
    type: "research"
    query: "Calibrated bucket probability for KLGA"
    executor: "llm"
    driver_refs: ["station_bias"]
}

model: station_bias

simulate 10000 iterations
"#;
        let tokens = Lexer::new(source).tokenize().expect("lexes");
        let program = Parser::new(tokens).parse().expect("parses");

        let refs: Vec<String> = program
            .agents()
            .iter()
            .filter(|a| a.name == "weather_oracle")
            .flat_map(|a| a.driver_refs.iter().cloned())
            .collect();

        assert_eq!(refs, vec!["station_bias".to_string()]);
        // And it would have got nothing from the legacy table.
        assert!(legacy_driver_prefixes("weather_oracle").is_empty());
    }

    #[test]
    fn test_extract_multiplier_no_match() {
        let summary = "No multiplier here";
        assert_eq!(extract_multiplier(summary), None);
    }

    /// Whitespace is no longer required, and the old expectation was wrong.
    ///
    /// This test previously asserted `None` with the comment "space required
    /// after colon". `p50:1.15(p5:1.05,p95:1.28)` is completely unambiguous, and
    /// discarding it was the same brittleness that lost 12 of 22 real claims to
    /// markdown emphasis: a format quibble throwing away a judgement the agent
    /// clearly made. Since the reader is now shared with
    /// `assertions::extract_from_prose`, the tolerance is deliberate and lives in
    /// one place.
    #[test]
    fn spacing_no_longer_decides_whether_a_claim_counts() {
        let summary = "[MULTIPLIER] Suggested p50:1.15(p5:1.05,p95:1.28)";
        assert_eq!(extract_multiplier(summary), Some((1.05, 1.15, 1.28)));
    }

    /// Tolerance must not become credulity: a sentence with no spread in it
    /// still has to yield nothing.
    #[test]
    fn tolerance_does_not_invent_a_multiplier() {
        assert_eq!(extract_multiplier("Suggested p50: probably higher"), None);
        assert_eq!(extract_multiplier("p50 1.15 p5 1.05 p95 1.28"), None);
        assert_eq!(
            extract_multiplier("Arsenal are 4W-1D-0L with xGD +2.1"),
            None
        );
    }
}
