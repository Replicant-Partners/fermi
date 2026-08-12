//! Canonical naming rules for driver-bound agents.
//!
//! # Why this exists
//!
//! When an agent is hired onto a driver the console mints a *bound*
//! FPL identifier so the same agent can be attached to several drivers
//! in one program, each with its own query and schedule:
//!
//! ```text
//! agent_id "efra_valuation" + driver "strength_factor"
//!     → bound name "efra_valuation_strength_factor"
//! ```
//!
//! The bound name is the agent's identity *inside the FPL program*. It
//! is **not** an ABW agent id, and posting it to
//! `/api/agents/:id/execute` 404s. Every place that talks to the
//! server, or that matches evidence back to the agent that produced
//! it, therefore needs the inverse of that construction.
//!
//! Until this module existed the inverse was guessed from a hardcoded
//! allowlist of ~29 curated agent ids (`base_agent_name`). Any agent
//! outside that list — i.e. every user-created or fine-tuned agent —
//! failed to split, with two consequences:
//!
//!   1. the bound name was sent to ABW as the agent id → `404 Agent
//!      'efra_valuation_strength_factor' not found`;
//!   2. evidence minted under the base id could not be matched back to
//!      the bound agent, so it appeared in global evidence views but
//!      not on the driver that paid for it.
//!
//! The split is not something to guess: `driver_refs` is on the
//! `AgentStmt` and round-trips through FPL, so the suffix is known
//! exactly. That's what [`base_agent_id`] uses. The allowlist is gone.
//!
//! This module is deliberately dependency-free (no `gpui`, no `fermi`
//! AST types) so the rules are unit-testable — see the note at the top
//! of `lib.rs`.

/// Sanitise a human string into an FPL identifier.
///
/// Lowercases, maps every non-alphanumeric character to `_`, and
/// guarantees the result is a legal leading-character identifier.
pub fn sanitize_name(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        format!("d_{}", s)
    } else if s.is_empty() {
        "unnamed".to_string()
    } else {
        s
    }
}

/// The FPL identifier for `agent_id` bound to `driver_name`.
///
/// This is the single definition of the bound-name convention. Every
/// assignment path must go through it so that [`base_agent_id`] is a
/// true inverse.
pub fn bound_agent_name(agent_id: &str, driver_name: &str) -> String {
    format!("{}_{}", agent_id, sanitize_name(driver_name))
}

/// Inverse of [`bound_agent_name`] when the driver is known.
///
/// Returns `bound_name` unchanged when the suffix doesn't match — the
/// name was produced by some path that doesn't follow the convention,
/// or is already a bare agent id.
pub fn base_agent_id_for_driver<'a>(bound_name: &'a str, driver_name: &str) -> &'a str {
    bound_name
        .strip_suffix(&format!("_{}", sanitize_name(driver_name)))
        .filter(|s| !s.is_empty())
        .unwrap_or(bound_name)
}

/// Inverse of [`bound_agent_name`] given the bound agent's `driver_refs`.
///
/// Tries each referenced driver, since an `AgentStmt` may list more
/// than one but only one of them contributed the suffix. Returns
/// `bound_name` unchanged when no driver matches, which is the right
/// answer for hand-authored FPL agents and for unbound agents like
/// `fermi`.
pub fn base_agent_id<'a>(bound_name: &'a str, driver_refs: &[String]) -> &'a str {
    for driver in driver_refs {
        let base = base_agent_id_for_driver(bound_name, driver);
        if base.len() < bound_name.len() {
            return base;
        }
    }
    bound_name
}

/// The id minted for the `index`-th evidence item produced by `owner`.
///
/// `owner` is the *bound* name for driver-bound agents, which is what
/// makes [`evidence_belongs_to_agent`] exact for new evidence.
pub fn evidence_id(owner: &str, index: usize) -> String {
    format!("{}_{}", owner, index)
}

/// Recover the owner portion of an id minted by [`evidence_id`].
///
/// `None` when the id doesn't have the `{owner}_{digits}` shape (manual
/// and URL-ingested evidence use different schemes).
pub fn evidence_owner(evidence_id: &str) -> Option<&str> {
    let (owner, index) = evidence_id.rsplit_once('_')?;
    if !owner.is_empty() && !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()) {
        Some(owner)
    } else {
        None
    }
}

/// Whether an evidence item was produced by the agent bound as
/// `bound_name`.
///
/// Two rules, both exact:
///
///   1. **Current** — ids are minted from the bound name, so
///      `{bound_name}_{n}` is a direct hit.
///   2. **Legacy** — forecasts saved before the bound name reached
///      [`evidence_id`] minted `{base_agent_id}_{n}`. Recovering the
///      owner and asking whether the bound name is that owner plus a
///      driver suffix links them back without an allowlist.
///
/// The old implementation also accepted `source.contains(base)`, which
/// matched any source string that happened to embed the agent id
/// (a URL, a PDF filename). That is replaced by exact/segment
/// comparison: ids carry the linkage, `source` is only a fallback for
/// the paths that stamp the agent id there.
pub fn evidence_belongs_to_agent(
    evidence_id: &str,
    evidence_source: &str,
    bound_name: &str,
) -> bool {
    if evidence_id == bound_name || evidence_id.starts_with(&format!("{}_", bound_name)) {
        return true;
    }
    if let Some(owner) = evidence_owner(evidence_id) {
        if owner == bound_name || bound_name.starts_with(&format!("{}_", owner)) {
            return true;
        }
    }
    // `source` is a linkage signal only when it *is* the agent's name —
    // `process_agent_evidence` stamps it verbatim when the API omits a
    // source. Anything looser pattern-matches titles, URLs and citations
    // into the wrong agent, and the id rules above already cover every
    // shape this codebase mints.
    evidence_source == bound_name
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn bound_name_round_trips_for_any_agent_id() {
        // The regression this module exists for: a fine-tuned agent that
        // is on no curated allowlist anywhere.
        let bound = bound_agent_name("efra_valuation", "strength_factor");
        assert_eq!(bound, "efra_valuation_strength_factor");
        assert_eq!(
            base_agent_id(&bound, &refs(&["strength_factor"])),
            "efra_valuation"
        );
    }

    #[test]
    fn bound_name_round_trips_for_curated_agents() {
        for (agent, driver) in [
            ("market_research", "song_quality"),
            ("macro_forecaster", "gdp growth"),
            ("nba_analyst", "Home Court Edge"),
        ] {
            let bound = bound_agent_name(agent, driver);
            assert_eq!(
                base_agent_id(&bound, &refs(&[driver])),
                agent,
                "round trip failed for {bound}"
            );
        }
    }

    #[test]
    fn driver_name_is_sanitized_on_both_sides() {
        let bound = bound_agent_name("efra_valuation", "Q4 revenue / mix");
        assert_eq!(bound, "efra_valuation_q4_revenue___mix");
        assert_eq!(
            base_agent_id_for_driver(&bound, "Q4 revenue / mix"),
            "efra_valuation"
        );
    }

    #[test]
    fn unbound_and_hand_authored_names_pass_through() {
        assert_eq!(base_agent_id("fermi", &[]), "fermi");
        assert_eq!(base_agent_id("fermi_base_rate", &[]), "fermi_base_rate");
        // An FPL-authored agent whose name has nothing to do with its
        // driver refs keeps its name.
        assert_eq!(
            base_agent_id("my_analyst", &refs(&["some_driver"])),
            "my_analyst"
        );
    }

    #[test]
    fn picks_the_driver_that_actually_contributed_the_suffix() {
        let bound = "equity_analyst_margin_risk";
        assert_eq!(
            base_agent_id(bound, &refs(&["other_driver", "margin_risk"])),
            "equity_analyst"
        );
    }

    #[test]
    fn never_strips_down_to_nothing() {
        // Degenerate: agent id equal to the driver name.
        assert_eq!(base_agent_id_for_driver("_dup", "dup"), "_dup");
    }

    #[test]
    fn evidence_minted_from_bound_name_links_to_that_agent() {
        let id = evidence_id("efra_valuation_strength_factor", 0);
        assert_eq!(id, "efra_valuation_strength_factor_0");
        assert!(evidence_belongs_to_agent(
            &id,
            "https://example.com",
            "efra_valuation_strength_factor"
        ));
    }

    #[test]
    fn legacy_evidence_minted_from_base_id_still_links() {
        // Saved forecasts have ids like "efra_valuation_0" — the whole
        // point of the bug report: previously unlinkable.
        assert!(evidence_belongs_to_agent(
            "efra_valuation_0",
            "",
            "efra_valuation_strength_factor"
        ));
        assert!(evidence_belongs_to_agent(
            "market_research_3",
            "",
            "market_research_economic_crisis"
        ));
    }

    #[test]
    fn evidence_does_not_leak_across_agents() {
        assert!(!evidence_belongs_to_agent(
            "market_research_0",
            "",
            "efra_valuation_strength_factor"
        ));
        assert!(!evidence_belongs_to_agent(
            "efra_valuation_0",
            "",
            "efra_thesis_strength_factor"
        ));
        // A source that merely embeds the agent id is no longer a match.
        assert!(!evidence_belongs_to_agent(
            "manual_strength_factor_2",
            "https://cdn.example.com/efra_valuation_deck.pdf",
            "efra_valuation_strength_factor"
        ));
        // Nor is a source that happens to be a prefix word.
        assert!(!evidence_belongs_to_agent(
            "manual_strength_factor_2",
            "market",
            "market_research_strength_factor"
        ));
        assert!(!evidence_belongs_to_agent(
            "manual_strength_factor_2",
            "Reuters",
            "reuters_wire_strength_factor"
        ));
    }

    #[test]
    fn source_stamped_with_the_agent_name_links() {
        // process_agent_evidence falls back to the agent's own name when
        // the API omits `source`.
        assert!(evidence_belongs_to_agent(
            "url_strength_factor_1",
            "efra_valuation_strength_factor",
            "efra_valuation_strength_factor"
        ));
        // A different agent's name never links.
        assert!(!evidence_belongs_to_agent(
            "url_strength_factor_1",
            "efra_thesis_strength_factor",
            "efra_valuation_strength_factor"
        ));
    }

    #[test]
    fn evidence_owner_only_accepts_the_minted_shape() {
        assert_eq!(
            evidence_owner("market_research_12"),
            Some("market_research")
        );
        assert_eq!(evidence_owner("market_research"), None);
        assert_eq!(evidence_owner("market_research_"), None);
        assert_eq!(evidence_owner("_0"), None);
    }

    #[test]
    fn sanitize_name_guards_identifier_rules() {
        assert_eq!(sanitize_name("GDP Growth"), "gdp_growth");
        assert_eq!(sanitize_name("2024 outcome"), "d_2024_outcome");
        assert_eq!(sanitize_name(""), "unnamed");
    }
}
