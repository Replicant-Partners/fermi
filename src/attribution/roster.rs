//! Resolving `fermi_forecasts.agents_used` entries to real agent identities.
//!
//! ## The problem this solves
//!
//! `agents_used` is written from the FPL program's agent statements, so each
//! entry carries the *statement* name:
//!
//! ```json
//! {"name": "weather_oracle_synoptic_pattern_august_2025",
//!  "query": "...", "agent_type": "research",
//!  "driver_refs": ["synoptic_pattern_august_2025"]}
//! ```
//!
//! Every calibration reader treats that `name` as an agent identity — it joins
//! `a.agent_name = e->>'name'`. Nothing has ever enforced the correspondence, so
//! an FPL author naming a statement descriptively silently detaches the whole
//! forecast from attribution. Measured on this deployment: one forecast with
//! five such statements cost all five agents their credit, and the Loop 5
//! mechanism probe correctly reported `WIRING BROKEN` as a result.
//!
//! mig-170 backfilled `agent_id` into existing rows and closed with the note:
//! *"Forward fix (separate, code side): the forecast write path should emit
//! agent_id into agents_used at creation time so this backfill never needs to
//! run again."* This module is that forward fix.
//!
//! ## Why resolve rather than reject
//!
//! Rejecting a statement name that does not match an agent would be cleaner in
//! principle and would break every existing FPL program that names statements
//! after what they compute — which is a reasonable thing to do, and arguably
//! more readable than repeating the agent name. So instead of constraining the
//! author, resolve the reference once, at write time, and store the resolved
//! `agent_id` alongside the name. Attribution then never depends on a
//! human-chosen label again.
//!
//! ## The two shapes
//!
//! 1. **Exact** — `name` is an agent name. Resolve directly.
//! 2. **Prefixed** — `name` is `<agent_name>_<driver_or_purpose>`, which is how
//!    the observed composites are formed. Resolve by longest matching agent
//!    prefix, so `weather_oracle_forecast_lead_time_skill` resolves to
//!    `weather_oracle` and not to a shorter agent that happens to share a
//!    prefix.
//!
//! Anything else is left untouched and reported, because guessing further would
//! attribute a Brier score to an agent that may not have earned it — which is
//! worse than the orphan it replaces.

use serde_json::Value;

/// One resolution outcome, for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// `name` matched an agent name exactly.
    Exact { name: String, agent_id: String },
    /// `name` was `<agent>_<suffix>`; resolved by longest agent prefix.
    Prefix {
        name: String,
        agent_name: String,
        agent_id: String,
    },
    /// Already carried an `agent_id`; left alone.
    AlreadyResolved { name: String },
    /// No agent could be resolved. Deliberately not guessed.
    Unresolved { name: String },
}

/// Resolve every entry in an `agents_used` array, stamping `agent_id` where it
/// can be determined.
///
/// `known_agents` is `(agent_name, agent_id)`. Returns the rewritten array and
/// one [`Resolution`] per entry, in order.
///
/// Never removes or reorders entries, and never overwrites an existing
/// `agent_id`: a caller that already knows the identity is more authoritative
/// than this inference.
pub fn resolve_agents_used(
    agents_used: &Value,
    known_agents: &[(String, String)],
) -> (Value, Vec<Resolution>) {
    let Some(entries) = agents_used.as_array() else {
        return (agents_used.clone(), Vec::new());
    };

    // Longest first, so `weather_oracle_forecast_lead_time_skill` cannot be
    // claimed by a hypothetical shorter agent `weather` before `weather_oracle`
    // has been tried.
    let mut by_length: Vec<&(String, String)> = known_agents.iter().collect();
    by_length.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));

    let mut out = Vec::with_capacity(entries.len());
    let mut log = Vec::with_capacity(entries.len());

    for entry in entries {
        let name = entry
            .get("name")
            .and_then(|v| v.as_str())
            .or_else(|| entry.get("agent_name").and_then(|v| v.as_str()))
            .unwrap_or("")
            .to_string();

        if entry.get("agent_id").and_then(|v| v.as_str()).is_some() {
            log.push(Resolution::AlreadyResolved { name });
            out.push(entry.clone());
            continue;
        }

        // 1. Exact match.
        if let Some((_, id)) = known_agents.iter().find(|(n, _)| *n == name) {
            log.push(Resolution::Exact {
                name,
                agent_id: id.clone(),
            });
            out.push(stamp(entry, id));
            continue;
        }

        // 2. Longest `<agent>_<suffix>` prefix. The `_` is required: without it
        //    `macro` would claim `macro_forecaster`, attributing one agent's
        //    work to another.
        if let Some((agent_name, id)) = by_length.iter().find(|(n, _)| {
            name.len() > n.len() + 1
                && name.starts_with(n.as_str())
                && name.as_bytes()[n.len()] == b'_'
        }) {
            log.push(Resolution::Prefix {
                name: name.clone(),
                agent_name: agent_name.clone(),
                agent_id: id.clone(),
            });
            out.push(stamp(entry, id));
            continue;
        }

        log.push(Resolution::Unresolved { name });
        out.push(entry.clone());
    }

    (Value::Array(out), log)
}

fn stamp(entry: &Value, agent_id: &str) -> Value {
    let mut obj = entry.clone();
    if let Some(map) = obj.as_object_mut() {
        map.insert("agent_id".to_string(), Value::String(agent_id.to_string()));
    }
    obj
}

/// Names that could not be resolved — for logging at the write site so an
/// unattributable forecast is visible when it is created rather than months
/// later when the mechanism probe counts it.
pub fn unresolved_names(log: &[Resolution]) -> Vec<&str> {
    log.iter()
        .filter_map(|r| match r {
            Resolution::Unresolved { name } => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn agents() -> Vec<(String, String)> {
        vec![
            (
                "weather_oracle".into(),
                "11111111-1111-1111-1111-111111111111".into(),
            ),
            (
                "macro_forecaster".into(),
                "22222222-2222-2222-2222-222222222222".into(),
            ),
            (
                "market_research".into(),
                "33333333-3333-3333-3333-333333333333".into(),
            ),
            (
                "entity_investigator".into(),
                "44444444-4444-4444-4444-444444444444".into(),
            ),
            // Deliberate prefix hazard: shorter agent sharing a prefix with a
            // longer one. `macro` must never claim `macro_forecaster_*`.
            (
                "macro".into(),
                "55555555-5555-5555-5555-555555555555".into(),
            ),
        ]
    }

    #[test]
    fn exact_name_resolves() {
        let input = json!([{"name": "weather_oracle", "query": "q"}]);
        let (out, log) = resolve_agents_used(&input, &agents());
        assert_eq!(out[0]["agent_id"], "11111111-1111-1111-1111-111111111111");
        assert!(matches!(log[0], Resolution::Exact { .. }));
    }

    /// The defect that broke Loop 5a. These five names are verbatim from the
    /// London 32 °C forecast on the production database.
    #[test]
    fn fpl_statement_names_resolve_by_prefix() {
        let input = json!([
            {"name": "weather_oracle_synoptic_pattern_august_2025"},
            {"name": "macro_forecaster_climate_trend_adjustment"},
            {"name": "market_research_urban_heat_island_intensity"},
            {"name": "weather_oracle_forecast_lead_time_skill"},
            {"name": "entity_investigator_exact_threshold_precision"},
        ]);
        let (out, log) = resolve_agents_used(&input, &agents());

        let ids: Vec<&str> = out
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["agent_id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids,
            vec![
                "11111111-1111-1111-1111-111111111111", // weather_oracle
                "22222222-2222-2222-2222-222222222222", // macro_forecaster
                "33333333-3333-3333-3333-333333333333", // market_research
                "11111111-1111-1111-1111-111111111111", // weather_oracle
                "44444444-4444-4444-4444-444444444444", // entity_investigator
            ]
        );
        assert!(log.iter().all(|r| matches!(r, Resolution::Prefix { .. })));
        assert!(unresolved_names(&log).is_empty());
    }

    /// Longest-prefix, not first-match. `macro_forecaster_climate_trend` must
    /// resolve to `macro_forecaster`, never to `macro` — attributing one
    /// agent's Brier score to another is worse than leaving it unattributed.
    #[test]
    fn longest_prefix_wins() {
        let input = json!([{"name": "macro_forecaster_climate_trend_adjustment"}]);
        let (_, log) = resolve_agents_used(&input, &agents());
        match &log[0] {
            Resolution::Prefix { agent_name, .. } => assert_eq!(agent_name, "macro_forecaster"),
            other => panic!("expected prefix resolution, got {other:?}"),
        }
    }

    /// The separator is required. `macro_forecaster` starts with `macro` but
    /// `macrofoo` does not belong to `macro`.
    #[test]
    fn prefix_requires_an_underscore_boundary() {
        let input = json!([{"name": "macrofoo_bar"}]);
        let (out, log) = resolve_agents_used(&input, &agents());
        assert!(out[0].get("agent_id").is_none());
        assert_eq!(unresolved_names(&log), vec!["macrofoo_bar"]);
    }

    /// An existing `agent_id` is authoritative and must survive untouched.
    #[test]
    fn existing_agent_id_is_never_overwritten() {
        let input = json!([{"name": "weather_oracle", "agent_id": "already-set"}]);
        let (out, log) = resolve_agents_used(&input, &agents());
        assert_eq!(out[0]["agent_id"], "already-set");
        assert!(matches!(log[0], Resolution::AlreadyResolved { .. }));
    }

    /// Genuinely unknown names are reported, not guessed. Attributing a score
    /// to the wrong agent is a worse outcome than an orphan.
    #[test]
    fn unknown_names_are_reported_not_guessed() {
        let input = json!([{"name": "some_agent_that_never_existed"}]);
        let (out, log) = resolve_agents_used(&input, &agents());
        assert!(out[0].get("agent_id").is_none());
        assert_eq!(
            unresolved_names(&log),
            vec!["some_agent_that_never_existed"]
        );
    }

    /// Order and arity are preserved — a roster is positional evidence.
    #[test]
    fn entries_are_never_dropped_or_reordered() {
        let input = json!([
            {"name": "weather_oracle"},
            {"name": "nonexistent"},
            {"name": "market_research"},
        ]);
        let (out, _) = resolve_agents_used(&input, &agents());
        let arr = out.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0]["name"], "weather_oracle");
        assert_eq!(arr[1]["name"], "nonexistent");
        assert_eq!(arr[2]["name"], "market_research");
    }

    /// Non-array input (null, object, absent) must pass through rather than
    /// panic — `agents_used` is client-supplied.
    #[test]
    fn non_array_input_passes_through() {
        for v in [json!(null), json!({}), json!("nope")] {
            let (out, log) = resolve_agents_used(&v, &agents());
            assert_eq!(out, v);
            assert!(log.is_empty());
        }
    }

    /// Readers accept `agent_name` too; resolve off it when `name` is absent.
    #[test]
    fn agent_name_key_is_also_resolved() {
        let input = json!([{"agent_name": "weather_oracle"}]);
        let (out, _) = resolve_agents_used(&input, &agents());
        assert_eq!(out[0]["agent_id"], "11111111-1111-1111-1111-111111111111");
    }
}
