//! Agent evolution — an earned, multi-dimensional progression badge.
//!
//! # Why not just a number
//!
//! The platform already has a maturity ladder
//! (`observatory::maturity_stage`), and it advances on **eval run count**:
//! 50 runs makes an agent `established`. That rewards pressing a button, not
//! improving. The same trap is everywhere in this domain — dream cycles that
//! extract nothing still advance `last_consolidated_at`, forecasts still
//! accumulate `n_resolved` whether or not the agent had any skill.
//!
//! This deployment paid for that literally: 91 consolidation cycles across
//! ~1,500 episodes produced zero entities, facts and rules, and every surface
//! reported them as healthy activity.
//!
//! So the rule for this module is: **activity earns nothing. Only outcomes
//! earn.** A level here means the agent demonstrably got better at something.
//!
//! # Four dimensions, deliberately not averaged
//!
//! | Dimension | Loop | Earned by |
//! |---|---|---|
//! | `memory`   | 1 | durable ontology the ontologist actually extracted |
//! | `judgment` | 5 | forecast skill *over the base rate*, or Shapley credit |
//! | `conduct`  | 2 | anomalies reviewed and resolved, corrections absorbed |
//! | `craft`    | — | measured eval dimension scores (not run count) |
//!
//! They are reported separately and never averaged into one figure. Averaging
//! is what lets a strong dimension hide a broken one — an agent that dreams
//! beautifully and forecasts worse than a coin should not read as "level 3".
//!
//! # Anti-farming
//!
//! Rank requires **breadth**: you cannot reach the upper ranks by maximising a
//! single dimension. And `judgment` is a gate, not just a contributor — an
//! agent with demonstrated negative contribution is capped below the top ranks
//! no matter how good everything else looks. Grinding one metric plateaus.
//!
//! # Anti-regression
//!
//! Each agent carries a high-water mark. Dropping below it sets `regressed`
//! with the dimension that fell, which is the signal an owner is meant to
//! react to. Losing a rank has to be visible, or "don't regress" is not an
//! incentive — it is a hope.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;

use crate::{resolve_agent, AppState};
use fermi_auth::AuthPrincipal;

/// One axis of progression. Tiers are 0–3: absent, emerging, solid, deep.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Dimension {
    Memory,
    Judgment,
    Conduct,
    Craft,
}

impl Dimension {
    pub fn as_str(&self) -> &'static str {
        match self {
            Dimension::Memory => "memory",
            Dimension::Judgment => "judgment",
            Dimension::Conduct => "conduct",
            Dimension::Craft => "craft",
        }
    }
}

/// Raw signals. Every field is an *outcome*; deliberately none of them is a
/// count of attempts.
#[derive(Debug, Clone, Copy, Default)]
pub struct EvolutionInputs {
    // ── memory (Loop 1) ──────────────────────────────────────────────────
    /// Durable knowledge extracted and persisted: entities + facts + rules.
    pub ontology_size: i64,
    /// Rules that survived verification. Unverified rules are claims, not
    /// knowledge, so they carry less weight.
    pub verified_rules: i64,

    // ── judgment (Loop 5) ────────────────────────────────────────────────
    /// Brier skill over a base-rate forecaster. `None` when undetermined.
    /// Negative means worse than knowing nothing.
    pub brier_skill: Option<f64>,
    /// Mean Shapley contribution across attributed forecasts, when available.
    /// Preferred over `brier_skill` because it is per-agent rather than
    /// per-team.
    pub mean_contribution: Option<f64>,
    /// Resolved forecasts backing the above.
    pub n_forecasts: i64,

    // ── conduct (Loop 2) ─────────────────────────────────────────────────
    pub anomalies_resolved: i64,
    pub anomalies_open: i64,
    /// Human-authority corrections absorbed into persona.
    pub persona_version: i32,
    /// Every anomaly ever raised against this agent, resolved or not.
    ///
    /// Needed to tell a genuinely clean record from an agent that misbehaved and
    /// was cleaned up. Both earn conduct, but by different routes.
    pub anomalies_ever: i64,
    /// Observed behaviour available to have been flagged: the denominator for a
    /// clean record. Zero anomalies over 3 episodes says nothing; zero over 500
    /// is a safety record.
    pub total_episodes: i64,

    // ── craft (evals) ────────────────────────────────────────────────────
    /// Mean of measured eval dimension scores in [0,1]. `None` = never scored.
    /// NOT the number of eval runs.
    pub eval_mean_score: Option<f64>,
    pub eval_dimensions_tracked: i64,
}

/// A dimension's earned tier plus the evidence for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DimensionScore {
    pub dimension: String,
    /// 0 = no evidence, 1 = emerging, 2 = solid, 3 = deep.
    pub tier: u8,
    pub evidence: String,
    /// What would raise this tier. The actionable half of a badge.
    pub next: Option<String>,
}

/// The badge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentEvolution {
    /// False when the agent has produced no usage data at all.
    ///
    /// An untried agent is not a failing agent, and giving it the bottom rung of
    /// a ladder implies it was measured and found wanting. On this platform that
    /// would mislabel the overwhelming majority — 537 of 731 agents have never
    /// produced an episode, and most of those are test scaffolding. Unranked is
    /// the honest state: pending usage data, not rank zero.
    pub ranked: bool,
    /// `"pending_usage_data"` until there is something to measure.
    pub status: String,
    /// 0–5. Meaningless unless `ranked`.
    pub level: u8,
    /// `None` while unranked — deliberately absent rather than "dormant", so a
    /// UI cannot accidentally render an untried agent as the lowest rank.
    pub rank: Option<String>,
    pub dimensions: Vec<DimensionScore>,
    /// The single most valuable thing the owner could do next.
    pub next_step: String,
    /// True when the agent has fallen below its own high-water mark.
    pub regressed: bool,
    pub peak_level: u8,
    /// Why the rank is capped, when it is. Present only when a gate is biting.
    pub capped_by: Option<String>,
}

/// Ecological rank names, matching the bestiary framing.
fn rank_name(level: u8) -> &'static str {
    match level {
        0 => "dormant",
        1 => "hatchling",
        2 => "fledgling",
        3 => "adept",
        4 => "specialist",
        _ => "exemplar",
    }
}

fn memory_tier(i: &EvolutionInputs) -> DimensionScore {
    // Ontology size alone can be inflated by heuristic entity extraction, so
    // the top tier additionally requires rules that survived verification —
    // generalisation, not just accumulation.
    let (tier, evidence, next) = match (i.ontology_size, i.verified_rules) {
        (0, _) => (
            0,
            "No durable knowledge extracted yet.".to_string(),
            Some(
                "Run a consolidation cycle with a funded extractor to build the first entities."
                    .to_string(),
            ),
        ),
        (n, _) if n < 25 => (
            1,
            format!("{n} entities/facts/rules extracted."),
            Some("Reach 25 durable items to consolidate this into a solid memory.".to_string()),
        ),
        (n, 0) => (
            2,
            format!("{n} durable items, but no verified rules yet."),
            Some(
                "Get rules through verification — accumulation without generalisation stalls here."
                    .to_string(),
            ),
        ),
        (n, v) if n < 100 || v < 5 => (
            2,
            format!("{n} durable items including {v} verified rule(s)."),
            Some("Reach 100 items and 5 verified rules for a deep memory.".to_string()),
        ),
        (n, v) => (3, format!("{n} durable items, {v} verified rules."), None),
    };
    DimensionScore {
        dimension: Dimension::Memory.as_str().into(),
        tier,
        evidence,
        next,
    }
}

fn judgment_tier(i: &EvolutionInputs) -> DimensionScore {
    // Prefer per-agent Shapley credit; fall back to team-level skill. Both are
    // measured against a baseline, so neither rewards volume.
    if let Some(phi) = i.mean_contribution {
        let (tier, evidence, next) = if phi <= 0.0 {
            (0, format!("Mean contribution {phi:+.4} — this agent moved forecasts away from their outcomes."),
             Some("Contribution must turn positive before judgment can be earned.".to_string()))
        } else if i.n_forecasts < 5 {
            (
                1,
                format!(
                    "Contribution {phi:+.4} over {} forecast(s) — promising but thin.",
                    i.n_forecasts
                ),
                Some("Accumulate 5+ attributed forecasts to make this solid.".to_string()),
            )
        } else if i.n_forecasts < 20 {
            (
                2,
                format!("Contribution {phi:+.4} over {} forecasts.", i.n_forecasts),
                Some("Reach 20 attributed forecasts for a deep judgment score.".to_string()),
            )
        } else {
            (
                3,
                format!("Contribution {phi:+.4} over {} forecasts.", i.n_forecasts),
                None,
            )
        };
        return DimensionScore {
            dimension: Dimension::Judgment.as_str().into(),
            tier,
            evidence,
            next,
        };
    }

    let (tier, evidence, next) = match (i.brier_skill, i.n_forecasts) {
        (None, _) | (_, 0) => (
            0,
            "No resolved forecasts to judge.".to_string(),
            Some("Resolve forecasts this agent contributed to.".to_string()),
        ),
        (Some(s), _) if s <= 0.0 => (
            0,
            format!("Skill {s:+.2} — no better than predicting the base rate."),
            Some("Raw calibration does not count; skill must exceed the base rate.".to_string()),
        ),
        (Some(s), n) if n < 5 => (
            1,
            format!("Skill {s:+.2} over {n} forecast(s) — provisional."),
            Some("Resolve 5+ forecasts to firm this up.".to_string()),
        ),
        (Some(s), n) if n < 20 => (
            2,
            format!("Skill {s:+.2} over {n} forecasts."),
            Some("Reach 20 resolved forecasts for a deep judgment score.".to_string()),
        ),
        (Some(s), n) => (3, format!("Skill {s:+.2} over {n} forecasts."), None),
    };
    DimensionScore {
        dimension: Dimension::Judgment.as_str().into(),
        tier,
        evidence,
        next,
    }
}

/// Episodes of low-incident observation required for each reliability step.
const CLEAN_EXPOSURE_TIERS: [i64; 3] = [25, 100, 500];

/// Anomalies per episode still considered reliable — one in a hundred.
///
/// Not zero: an agent that was flagged, corrected, and has run cleanly since is
/// more trustworthy than one that was never exercised hard enough to trip, and a
/// zero-tolerance rule inverted that.
const MAX_INCIDENT_RATE: f64 = 0.01;

fn conduct_tier(i: &EvolutionInputs) -> DimensionScore {
    let dim = Dimension::Conduct.as_str().to_string();

    // Open anomalies suppress conduct regardless of history: an agent with
    // unreviewed flags is not demonstrating good conduct right now. Conduct is
    // present-tense, which is why this gate ignores an otherwise clean past.
    if i.anomalies_open > 0 {
        return DimensionScore {
            dimension: dim,
            tier: 0,
            evidence: format!("{} unreviewed anomal(ies).", i.anomalies_open),
            next: Some("Clear the review queue to restore conduct.".to_string()),
        };
    }

    // v1 is the starting persona, so corrections = version - 1. Clamped at 0:
    // `saturating_sub` on a signed int saturates at i32::MIN, not zero, so an
    // unset persona_version of 0 produced -1 corrections and spuriously earned
    // a conduct tier — free progress for an agent that had done nothing.
    let corrections = (i.persona_version.max(1) - 1).max(0) as i64;
    let review_outcomes = i.anomalies_resolved + corrections;

    // Route 1 — governability: it went wrong, review happened, the fix stuck.
    let correction_tier = match review_outcomes {
        0 => 0,
        1..=2 => 1,
        3..=9 => 2,
        _ => 3,
    };

    // Route 2 — reliability: observed repeatedly with a low incident rate.
    //
    // This route exists because the correction route alone made conduct
    // unreachable for well-behaved agents: every path to it required the agent
    // to misbehave first, so "level up" meant "go wrong, then get fixed". All
    // four of the platform's strongest agents sat at tier 0 for the sole reason
    // that they had never been in trouble.
    //
    // Rate, not "never". An earlier version required `anomalies_ever == 0`,
    // which permanently locked out any agent that had ever been flagged — so an
    // agent that hit four anomalies, absorbed the corrections, and then ran 500
    // clean episodes was treated as less reliable than one that had simply never
    // been exercised hard enough to trip. Being fixed is not a permanent stain.
    //
    // Exposure is the denominator, not the achievement: a clean run of 3
    // episodes says nothing, 500 is a safety record. Episodes only accrue by
    // actually running the agent, so this incentive points the same way as
    // adoption rather than against it.
    let incident_rate = if i.total_episodes > 0 {
        i.anomalies_ever as f64 / i.total_episodes as f64
    } else {
        // No exposure: no rate to speak of, and the exposure gate below will
        // award nothing anyway.
        0.0
    };
    let reliable = incident_rate <= MAX_INCIDENT_RATE;
    let clean_points = if !reliable {
        0
    } else if i.total_episodes >= CLEAN_EXPOSURE_TIERS[2] {
        3
    } else if i.total_episodes >= CLEAN_EXPOSURE_TIERS[1] {
        2
    } else if i.total_episodes >= CLEAN_EXPOSURE_TIERS[0] {
        1
    } else {
        0
    };

    // Governability and reliability are ORTHOGONAL properties, so they add
    // rather than compete. "It doesn't go wrong" and "it can be corrected when
    // it does" are different virtues, and an agent with both has demonstrated
    // more than an agent with either — taking the max threw that away and left
    // a reliable agent with no reason to engage with review at all.
    //
    // Consequence worth stating: the DEEPEST conduct requires both routes. A
    // spotless agent that has never been corrected tops out at solid (which is
    // still enough for the highest rank), because "never tested under
    // correction" is genuinely less evidence than "tested and governable".
    let conduct_points = correction_tier + clean_points;
    let tier = match conduct_points {
        0 => 0,
        1 => 1,
        2..=3 => 2,
        _ => 3,
    };

    // Name both contributions, so an owner can see which virtue is carrying the
    // score and which one is available to grow.
    let mut parts: Vec<String> = Vec::new();
    if correction_tier > 0 {
        parts.push(format!(
            "{} anomal(ies) resolved and {} persona correction(s) absorbed",
            i.anomalies_resolved, corrections
        ));
    }
    if clean_points > 0 {
        parts.push(if i.anomalies_ever == 0 {
            format!(
                "{} episodes observed with no anomaly raised",
                i.total_episodes
            )
        } else {
            format!(
                "{} episodes observed at a {:.1}% incident rate",
                i.total_episodes,
                incident_rate * 100.0
            )
        });
    }

    let evidence = if !parts.is_empty() {
        format!("{}.", parts.join("; "))
    } else if !reliable {
        format!(
            "{} anomal(ies) over {} episodes — a {:.1}% incident rate, above the {:.0}% \
             reliability bar.",
            i.anomalies_ever,
            i.total_episodes,
            incident_rate * 100.0,
            MAX_INCIDENT_RATE * 100.0
        )
    } else {
        format!(
            "Clean so far, but only {} episode(s) of observed behaviour.",
            i.total_episodes
        )
    };

    // Point at whichever route is cheapest to advance from here.
    let next = if tier >= 3 {
        None
    } else if clean_points == 0 && correction_tier == 0 {
        Some(format!(
            "Two ways to earn conduct, and they add: absorb a review correction, or reach \
             {} episodes at a low incident rate.",
            CLEAN_EXPOSURE_TIERS[0]
        ))
    } else if clean_points < 3 {
        let target = CLEAN_EXPOSURE_TIERS[clean_points.min(2) as usize];
        Some(format!(
            "Reach {target} observed episodes, or absorb more review corrections — both count."
        ))
    } else {
        Some(
            "Reliability is maxed; the deepest conduct also requires demonstrated \
             governability, so absorb a review correction."
                .to_string(),
        )
    };

    DimensionScore {
        dimension: dim,
        tier,
        evidence,
        next,
    }
}

fn craft_tier(i: &EvolutionInputs) -> DimensionScore {
    // Scores, never run counts. An agent evaluated 500 times at 0.4 has not
    // earned craft; one evaluated 5 times at 0.9 has.
    let (tier, evidence, next) = match (i.eval_mean_score, i.eval_dimensions_tracked) {
        (None, _) => (
            0,
            "Never scored by an evaluator.".to_string(),
            Some("Run an eval to establish a baseline.".to_string()),
        ),
        (Some(s), _) if s < 0.55 => (
            0,
            format!(
                "Mean eval score {:.0}% — below a working standard.",
                s * 100.0
            ),
            Some("Raise mean eval score above 55%.".to_string()),
        ),
        (Some(s), d) if s < 0.75 || d < 2 => (
            1,
            format!("Mean eval score {:.0}% across {d} dimension(s).", s * 100.0),
            Some("Reach 75% across 2+ dimensions for solid craft.".to_string()),
        ),
        (Some(s), d) if s < 0.9 || d < 3 => (
            2,
            format!("Mean eval score {:.0}% across {d} dimensions.", s * 100.0),
            Some("Reach 90% across 3+ dimensions for deep craft.".to_string()),
        ),
        (Some(s), d) => (
            3,
            format!("Mean eval score {:.0}% across {d} dimensions.", s * 100.0),
            None,
        ),
    };
    DimensionScore {
        dimension: Dimension::Craft.as_str().into(),
        tier,
        evidence,
        next,
    }
}

/// Compute the badge.
///
/// `peak_level` is the agent's previously recorded high-water mark; pass 0 for
/// a first computation.
pub fn compute_evolution(i: EvolutionInputs, peak_level: u8) -> AgentEvolution {
    let dims = vec![
        memory_tier(&i),
        judgment_tier(&i),
        conduct_tier(&i),
        craft_tier(&i),
    ];

    // Nothing has happened to this agent yet. Not measured and found wanting —
    // simply never exercised. Return early so it carries no rank at all rather
    // than the bottom rung, which would read as a verdict.
    let no_usage_data = i.ontology_size == 0
        && i.n_forecasts == 0
        && i.eval_mean_score.is_none()
        && i.anomalies_resolved == 0
        && i.anomalies_open == 0
        // Episodes are usage data in their own right now that a clean record
        // earns conduct: an agent that has been run 30 times has been observed,
        // even if nothing else about it has been measured yet.
        && i.total_episodes == 0
        && i.persona_version <= 1;
    if no_usage_data {
        return AgentEvolution {
            ranked: false,
            status: "pending_usage_data".to_string(),
            level: 0,
            rank: None,
            dimensions: dims,
            next_step: "Not yet exercised — run this agent so there is something to measure."
                .to_string(),
            // An unranked agent cannot have regressed, and must not inherit a
            // stale peak that would make it look like it fell from grace.
            regressed: false,
            peak_level,
            capped_by: None,
        };
    }

    let with_evidence = dims.iter().filter(|d| d.tier >= 1).count();
    let solid = dims.iter().filter(|d| d.tier >= 2).count();
    let deep = dims.iter().filter(|d| d.tier >= 3).count();

    // Breadth-gated ladder: each rank needs more dimensions in play, so no
    // single metric can be ground to the top.
    let mut level: u8 = match (with_evidence, solid, deep) {
        (0, _, _) => 0,
        (1, _, _) => 1,
        (2, 0, _) => 1,
        (2, _, _) => 2,
        (3, s, _) if s < 2 => 2,
        (3, _, _) => 3,
        // Exemplar requires ALL FOUR dimensions solid, not three. With conduct
        // now reachable by a clean record, `s >= 3` would have promoted three
        // agents straight to the top rank the moment that route opened — the
        // ladder needs headroom, and the highest rank should mean genuinely
        // well-rounded rather than "strong in most things".
        (_, s, d) if s >= 4 && d >= 1 => 5,
        (_, s, _) if s >= 3 => 4,
        (4, _, _) => 3,
        _ => 3,
    };

    // Judgment is a gate, not merely a contributor. An agent demonstrably
    // making forecasts worse cannot be a specialist or exemplar however
    // impressive its memory looks — that is exactly the failure mode a badge
    // built on averages would conceal.
    let judgment = dims
        .iter()
        .find(|d| d.dimension == "judgment")
        .expect("judgment present");
    let negative_judgment = i
        .mean_contribution
        .map(|p| p < 0.0)
        .or_else(|| i.brier_skill.map(|s| s < 0.0))
        .unwrap_or(false);

    let mut capped_by = None;
    if negative_judgment && level > 2 {
        level = 2;
        capped_by = Some(
            "Capped at fledgling: measured contribution is negative. Breadth elsewhere \
             cannot offset an agent that makes forecasts worse."
                .to_string(),
        );
    } else if judgment.tier == 0 && level >= 5 {
        level = 4;
        capped_by = Some(
            "Capped at specialist: exemplar requires demonstrated forecasting judgment."
                .to_string(),
        );
    }

    // The single highest-value next action: the lowest-tier dimension that has
    // advice attached. Concrete beats comprehensive — one instruction gets
    // acted on, four get ignored.
    let next_step = dims
        .iter()
        .filter(|d| d.next.is_some())
        .min_by_key(|d| d.tier)
        .and_then(|d| d.next.clone())
        .unwrap_or_else(|| "Fully evolved across all four dimensions.".to_string());

    AgentEvolution {
        ranked: true,
        status: "ranked".to_string(),
        level,
        rank: Some(rank_name(level).to_string()),
        dimensions: dims,
        next_step,
        regressed: level < peak_level,
        peak_level: peak_level.max(level),
        capped_by,
    }
}

/// Everything needed to render a badge, per agent, for a whole fleet.
pub struct FleetEvolution {
    pub inputs: EvolutionInputs,
    pub peak_level: u8,
    /// Raw forecasting figures, surfaced publicly alongside the rank.
    pub brier_mean: Option<f64>,
    pub brier_baseline: Option<f64>,
    pub brier_skill: Option<f64>,
    pub outcome_base_rate: Option<f64>,
}

/// Load evolution inputs for every agent in one round trip.
///
/// One statement rather than a badge computation per agent: the ecology view
/// renders the whole published catalogue, and a per-agent query there would be
/// ~100 round trips on a page load. Correlated subqueries keep the per-agent
/// counts exact — joining episodes, entities and facts in a single FROM would
/// multiply rows and inflate every figure.
pub async fn fleet_evolution(
    db: &sqlx::PgPool,
) -> std::collections::HashMap<uuid::Uuid, FleetEvolution> {
    let rows = sqlx::query(
        "SELECT a.agent_id, a.agent_name, a.persona_version,
                (SELECT COUNT(*) FROM entities x       WHERE x.agent_id = a.agent_id)
              + (SELECT COUNT(*) FROM facts x          WHERE x.agent_id = a.agent_id)
              + (SELECT COUNT(*) FROM semantic_rules x WHERE x.agent_id = a.agent_id) AS ontology_size,
                (SELECT COUNT(*) FROM semantic_rules x
                  WHERE x.agent_id = a.agent_id AND x.verification_status = 'verified') AS verified_rules,
                (SELECT COUNT(*) FROM anomaly_events ae
                  WHERE ae.agent_id = a.agent_id AND ae.resolved_at IS NOT NULL) AS anomalies_resolved,
                (SELECT COUNT(*) FROM anomaly_events ae
                  WHERE ae.agent_id = a.agent_id AND ae.resolved_at IS NULL
                    AND ae.requires_review) AS anomalies_open,
                (SELECT COUNT(*) FROM anomaly_events ae
                  WHERE ae.agent_id = a.agent_id) AS anomalies_ever,
                (SELECT COUNT(*) FROM episodes e WHERE e.agent_id = a.agent_id) AS total_episodes,
                (SELECT AVG(s.score) FROM eval_signals s WHERE s.agent_id = a.agent_id) AS eval_mean_score,
                (SELECT COUNT(DISTINCT s.dimension) FROM eval_signals s
                  WHERE s.agent_id = a.agent_id) AS eval_dimensions,
                (SELECT AVG(c.shapley_value) FROM forecast_agent_credit c
                   JOIN forecast_attributions at
                     ON at.forecast_id = c.forecast_id AND at.neutralisation = c.neutralisation
                  WHERE c.agent_id = a.agent_id AND c.neutralisation = 'identity'
                    AND at.efficiency_residual < 1e-6
                    AND (at.reconstruction_error IS NULL OR at.reconstruction_error < 0.01)
                ) AS mean_contribution,
                (SELECT COUNT(*) FROM fermi_forecasts f
                  WHERE f.status = 'resolved' AND f.brier_score IS NOT NULL
                    AND (f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', a.agent_id::text))
                      OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', a.agent_name))
                      OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', a.agent_name)))
                ) AS n_forecasts,
                (SELECT AVG(f.brier_score)::float8 FROM fermi_forecasts f
                  WHERE f.status = 'resolved' AND f.brier_score IS NOT NULL
                    AND (f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', a.agent_id::text))
                      OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', a.agent_name))
                      OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', a.agent_name)))
                ) AS brier_mean,
                (SELECT COUNT(*) FROM fermi_forecasts f
                  WHERE f.status = 'resolved' AND f.brier_score IS NOT NULL AND f.actual_outcome
                    AND (f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', a.agent_id::text))
                      OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', a.agent_name))
                      OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', a.agent_name)))
                ) AS n_yes,
                COALESCE(ev.peak_level, 0) AS peak_level
           FROM agents a
           LEFT JOIN agent_evolution ev ON ev.agent_id = a.agent_id
          WHERE a.status <> 'archived'",
    )
    .fetch_all(db)
    .await
    .unwrap_or_default();

    let mut out = std::collections::HashMap::new();
    for r in &rows {
        let Ok(id) = r.try_get::<uuid::Uuid, _>("agent_id") else {
            continue;
        };
        let gi = |k: &str| -> i64 { r.try_get::<i64, _>(k).unwrap_or(0) };
        let brier_mean = r.try_get::<Option<f64>, _>("brier_mean").ok().flatten();
        let n_forecasts = gi("n_forecasts");
        let (base_rate, baseline, skill) = crate::handlers::agents::brier_skill(
            brier_mean,
            gi("n_yes") as usize,
            n_forecasts as usize,
        );
        out.insert(
            id,
            FleetEvolution {
                inputs: EvolutionInputs {
                    ontology_size: gi("ontology_size"),
                    verified_rules: gi("verified_rules"),
                    brier_skill: skill,
                    mean_contribution: r
                        .try_get::<Option<f64>, _>("mean_contribution")
                        .ok()
                        .flatten(),
                    n_forecasts,
                    anomalies_resolved: gi("anomalies_resolved"),
                    anomalies_open: gi("anomalies_open"),
                    anomalies_ever: gi("anomalies_ever"),
                    total_episodes: gi("total_episodes"),
                    persona_version: r.try_get::<i32, _>("persona_version").unwrap_or(1),
                    eval_mean_score: r
                        .try_get::<Option<f64>, _>("eval_mean_score")
                        .ok()
                        .flatten(),
                    eval_dimensions_tracked: gi("eval_dimensions"),
                },
                peak_level: r.try_get::<i16, _>("peak_level").unwrap_or(0).clamp(0, 5) as u8,
                brier_mean,
                brier_baseline: baseline,
                brier_skill: skill,
                outcome_base_rate: base_rate,
            },
        );
    }
    out
}

/// The PUBLIC badge shape, shared by every surface that renders one.
///
/// Deliberately omits `peak_level` and `regressed`: regression is owner-only,
/// and the peak reveals a fall by implication. Keeping one constructor means a
/// new public surface cannot accidentally leak them.
pub fn public_badge_json(ev: &AgentEvolution, f: &FleetEvolution) -> serde_json::Value {
    json!({
        "ranked": ev.ranked,
        "status": ev.status,
        "level": ev.level,
        "rank": ev.rank,
        "capped_by": ev.capped_by,
        "dimensions": ev.dimensions,
        "forecasting": {
            "n_resolved_forecasts": f.inputs.n_forecasts,
            "brier_mean": f.brier_mean,
            "brier_baseline": f.brier_baseline,
            "brier_skill_score": f.brier_skill,
            "outcome_base_rate": f.outcome_base_rate,
            "beats_base_rate": f.brier_skill.map(|s| s > 0.0),
        },
    })
}

/// GET /api/agents/:agent_id/evolution
///
/// The badge, computed live from outcomes, plus the stored high-water mark.
///
/// Note this GET performs a ratchet write: when the agent has reached a new
/// best rank, `peak_level` is raised. That is deliberate — the peak is a
/// property of history rather than of the request, and recording it on read is
/// what lets regression be detected at all without a separate scheduled job.
/// The write is idempotent and monotonic, so repeated reads cannot drift it.
pub async fn agent_evolution_handler(
    State(state): State<AppState>,
    _principal: Option<AuthPrincipal>,
    Path(agent_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let db_agent = resolve_agent(&state, &agent_id).await?;
    let aid = db_agent.agent_id;
    let db = &state.db;

    // Every input below is an outcome. Counts of attempts are deliberately
    // absent: see the module docs on why activity must earn nothing.
    let row = sqlx::query(
        "SELECT
           (SELECT COUNT(*) FROM entities x       WHERE x.agent_id = $1)
         + (SELECT COUNT(*) FROM facts x          WHERE x.agent_id = $1)
         + (SELECT COUNT(*) FROM semantic_rules x WHERE x.agent_id = $1) AS ontology_size,
           (SELECT COUNT(*) FROM semantic_rules x
             WHERE x.agent_id = $1 AND x.verification_status = 'verified') AS verified_rules,
           (SELECT COUNT(*) FROM anomaly_events ae
             WHERE ae.agent_id = $1 AND ae.resolved_at IS NOT NULL) AS anomalies_resolved,
           (SELECT COUNT(*) FROM anomaly_events ae
             WHERE ae.agent_id = $1 AND ae.resolved_at IS NULL
               AND ae.requires_review) AS anomalies_open,
           (SELECT COUNT(*) FROM anomaly_events ae WHERE ae.agent_id = $1) AS anomalies_ever,
           (SELECT COUNT(*) FROM episodes e WHERE e.agent_id = $1) AS total_episodes,
           (SELECT AVG(s.score) FROM eval_signals s
             WHERE s.agent_id = $1) AS eval_mean_score,
           (SELECT COUNT(DISTINCT s.dimension) FROM eval_signals s
             WHERE s.agent_id = $1) AS eval_dimensions,
           (SELECT AVG(c.shapley_value) FROM forecast_agent_credit c
             JOIN forecast_attributions a
               ON a.forecast_id = c.forecast_id AND a.neutralisation = c.neutralisation
            WHERE c.agent_id = $1 AND c.neutralisation = 'identity'
              AND a.efficiency_residual < 1e-6
              AND (a.reconstruction_error IS NULL OR a.reconstruction_error < 0.01)
           ) AS mean_contribution,
           (SELECT COUNT(*) FROM fermi_forecasts f
             WHERE f.status = 'resolved' AND f.brier_score IS NOT NULL
               AND (f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', $2::text))
                 OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', $3))
                 OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', $3)))
           ) AS n_forecasts,
           -- Brier is shown publicly when available, so compute the raw mean and
           -- the YES count needed to derive skill against the base rate.
           (SELECT AVG(f.brier_score)::float8 FROM fermi_forecasts f
             WHERE f.status = 'resolved' AND f.brier_score IS NOT NULL
               AND (f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', $2::text))
                 OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', $3))
                 OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', $3)))
           ) AS brier_mean,
           (SELECT COUNT(*) FROM fermi_forecasts f
             WHERE f.status = 'resolved' AND f.brier_score IS NOT NULL AND f.actual_outcome
               AND (f.agents_used @> jsonb_build_array(jsonb_build_object('agent_id', $2::text))
                 OR f.agents_used @> jsonb_build_array(jsonb_build_object('agent_name', $3))
                 OR f.agents_used @> jsonb_build_array(jsonb_build_object('name', $3)))
           ) AS n_yes",
    )
    .bind(aid)
    .bind(aid.to_string())
    .bind(&db_agent.agent_name)
    .fetch_one(db)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let gi = |k: &str| -> i64 { row.try_get::<i64, _>(k).unwrap_or(0) };

    // Reuse the calibration endpoint's skill computation rather than restating
    // it: two implementations of "is this better than the base rate" would
    // eventually disagree, and the badge would contradict the agent page.
    let brier_mean: Option<f64> = row.try_get::<Option<f64>, _>("brier_mean").ok().flatten();
    let n_forecasts = gi("n_forecasts");
    let (outcome_base_rate, brier_baseline, brier_skill_score) =
        crate::handlers::agents::brier_skill(
            brier_mean,
            gi("n_yes") as usize,
            n_forecasts as usize,
        );

    let inputs = EvolutionInputs {
        ontology_size: gi("ontology_size"),
        verified_rules: gi("verified_rules"),
        brier_skill: brier_skill_score,
        mean_contribution: row
            .try_get::<Option<f64>, _>("mean_contribution")
            .ok()
            .flatten(),
        n_forecasts: gi("n_forecasts"),
        anomalies_resolved: gi("anomalies_resolved"),
        anomalies_open: gi("anomalies_open"),
        anomalies_ever: gi("anomalies_ever"),
        total_episodes: gi("total_episodes"),
        persona_version: db_agent.persona_version,
        eval_mean_score: row
            .try_get::<Option<f64>, _>("eval_mean_score")
            .ok()
            .flatten(),
        eval_dimensions_tracked: gi("eval_dimensions"),
    };

    let peak_level: i16 =
        sqlx::query_scalar("SELECT peak_level FROM agent_evolution WHERE agent_id = $1")
            .bind(aid)
            .fetch_optional(db)
            .await
            .ok()
            .flatten()
            .unwrap_or(0);

    let ev = compute_evolution(inputs, peak_level.clamp(0, 5) as u8);

    // Ratchet the peak and record whether the agent is currently below it.
    // Best-effort: a badge is worth showing even if the bookkeeping write fails.
    if let Err(e) = sqlx::query(
        "INSERT INTO agent_evolution
             (agent_id, peak_level, peak_at, current_level, last_computed_at, regressed_since)
         VALUES ($1, $2, NOW(), $3, NOW(), CASE WHEN $3 < $2 THEN NOW() END)
         ON CONFLICT (agent_id) DO UPDATE SET
             peak_level = GREATEST(agent_evolution.peak_level, EXCLUDED.peak_level),
             peak_at = CASE WHEN EXCLUDED.peak_level > agent_evolution.peak_level
                            THEN NOW() ELSE agent_evolution.peak_at END,
             current_level = EXCLUDED.current_level,
             last_computed_at = NOW(),
             regressed_since = CASE
                 WHEN EXCLUDED.current_level
                      < GREATEST(agent_evolution.peak_level, EXCLUDED.peak_level)
                 THEN COALESCE(agent_evolution.regressed_since, NOW())
                 ELSE NULL END",
    )
    .bind(aid)
    .bind(ev.peak_level as i16)
    .bind(ev.level as i16)
    .execute(db)
    .await
    {
        tracing::warn!(agent_id = %aid, error = %e, "[evolution] peak write failed");
    }

    // Regression is private. It is a strong incentive precisely because it
    // stings, and a public one would be a walk of shame attached to someone
    // else's agent. Owners and admins see it; nobody else does — including
    // `peak_level`, which reveals a fall by implication.
    let is_owner = _principal
        .as_ref()
        .map(|p| {
            p.can_admin()
                || db_agent
                    .owner_id
                    .as_deref()
                    .map(|o| o == p.user_id())
                    .unwrap_or(false)
        })
        .unwrap_or(false);

    let mut out = json!({
        "agent_id": aid,
        "agent_name": db_agent.agent_name,
        "ranked": ev.ranked,
        "status": ev.status,
        "level": ev.level,
        "rank": ev.rank,
        "capped_by": ev.capped_by,
        "next_step": ev.next_step,
        "dimensions": ev.dimensions,
        // Public by request: a forecasting track record is the agent's
        // credential, and hiding it would defeat the point of publishing one.
        // `skill` is the honest headline — `brier_mean` alone reads as ~99% on
        // an outcome-skewed question set where a coin would score the same.
        "forecasting": {
            "n_resolved_forecasts": n_forecasts,
            "brier_mean": brier_mean,
            "brier_baseline": brier_baseline,
            "brier_skill_score": brier_skill_score,
            "outcome_base_rate": outcome_base_rate,
            "beats_base_rate": brier_skill_score.map(|s| s > 0.0),
        },
        "note": "Levels are earned from outcomes, never from activity. Eval runs, dream \
                 cycles and forecast counts contribute nothing on their own — only measured \
                 scores, extracted knowledge and skill over the base rate do.",
    });

    if is_owner {
        out["peak_level"] = json!(ev.peak_level);
        out["peak_rank"] = json!(if ev.ranked || ev.peak_level > 0 {
            Some(rank_name(ev.peak_level))
        } else {
            None
        });
        out["regressed"] = json!(ev.regressed);
        out["visibility"] = json!("owner");
    } else {
        out["visibility"] = json!("public");
    }

    Ok(Json(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strong_memory() -> EvolutionInputs {
        EvolutionInputs {
            ontology_size: 500,
            verified_rules: 40,
            ..Default::default()
        }
    }

    /// The core promise: activity earns nothing. An agent with a huge history
    /// of runs but no measured outcome stays at zero.
    #[test]
    fn activity_without_outcomes_earns_nothing() {
        let e = compute_evolution(
            EvolutionInputs {
                // Every one of these is "we did a lot of things".
                n_forecasts: 500,
                eval_dimensions_tracked: 9,
                persona_version: 1,
                ..Default::default()
            },
            0,
        );
        assert_eq!(e.level, 0, "{:?}", e.dimensions);
        // 500 forecasts is usage data, so it IS ranked — just at the bottom.
        // Having been measured and earned nothing is a different statement from
        // never having been measured.
        assert!(e.ranked);
        assert_eq!(e.rank.as_deref(), Some("dormant"));
    }

    /// An agent nobody has exercised carries NO rank. Giving it the bottom rung
    /// would read as a verdict on an agent that was never tried — and on this
    /// platform that would mislabel 537 of 731 agents, most of them test
    /// scaffolding.
    #[test]
    fn untried_agent_is_unranked_not_bottom_ranked() {
        let e = compute_evolution(
            EvolutionInputs {
                persona_version: 1,
                ..Default::default()
            },
            0,
        );
        assert!(!e.ranked);
        assert_eq!(e.status, "pending_usage_data");
        assert_eq!(
            e.rank, None,
            "an untried agent must not be given a rank name"
        );
        assert!(e.next_step.contains("run this agent"), "{}", e.next_step);
    }

    /// An unranked agent must never be reported as regressed, and must not
    /// inherit a stale peak that would imply it fell from grace.
    #[test]
    fn untried_agent_is_never_flagged_as_regressed() {
        let e = compute_evolution(
            EvolutionInputs {
                persona_version: 1,
                ..Default::default()
            },
            4,
        );
        assert!(!e.ranked);
        assert!(!e.regressed, "unranked agents cannot regress");
    }

    /// A single real signal is enough to enter the ladder.
    #[test]
    fn one_measured_outcome_makes_an_agent_ranked() {
        let e = compute_evolution(
            EvolutionInputs {
                ontology_size: 3,
                persona_version: 1,
                ..Default::default()
            },
            0,
        );
        assert!(e.ranked);
        assert_eq!(e.rank.as_deref(), Some("hatchling"));
    }

    /// Grinding one dimension must plateau, not reach the top.
    #[test]
    fn maxing_a_single_dimension_plateaus_low() {
        let e = compute_evolution(strong_memory(), 0);
        assert!(
            e.level <= 1,
            "single-dimension agent reached level {}",
            e.level
        );
    }

    /// Breadth is what raises rank.
    #[test]
    fn breadth_raises_rank() {
        let e = compute_evolution(
            EvolutionInputs {
                ontology_size: 500,
                verified_rules: 40,
                mean_contribution: Some(0.02),
                n_forecasts: 40,
                anomalies_resolved: 12,
                persona_version: 4,
                eval_mean_score: Some(0.93),
                eval_dimensions_tracked: 4,
                ..Default::default()
            },
            0,
        );
        assert_eq!(e.level, 5, "{:?}", e);
        assert_eq!(e.rank.as_deref(), Some("exemplar"));
        assert!(e.capped_by.is_none());
    }

    /// The gate that matters: an agent that makes forecasts WORSE cannot climb
    /// on the strength of its other dimensions. A badge that averaged would
    /// have hidden this.
    #[test]
    fn negative_contribution_caps_rank_despite_excellence_elsewhere() {
        let e = compute_evolution(
            EvolutionInputs {
                ontology_size: 900,
                verified_rules: 60,
                mean_contribution: Some(-0.03),
                n_forecasts: 60,
                anomalies_resolved: 20,
                persona_version: 6,
                eval_mean_score: Some(0.97),
                eval_dimensions_tracked: 5,
                ..Default::default()
            },
            0,
        );
        assert_eq!(e.level, 2, "{:?}", e);
        assert!(e.capped_by.as_deref().unwrap().contains("negative"));
    }

    /// Raw calibration without skill earns nothing — the World Cup trap.
    #[test]
    fn high_raw_calibration_without_skill_earns_no_judgment() {
        let e = compute_evolution(
            EvolutionInputs {
                brier_skill: Some(0.0), // 99% raw, zero skill
                n_forecasts: 48,
                ..Default::default()
            },
            0,
        );
        let j = e
            .dimensions
            .iter()
            .find(|d| d.dimension == "judgment")
            .unwrap();
        assert_eq!(j.tier, 0, "{j:?}");
        assert!(j.evidence.contains("base rate"));
    }

    /// Accumulation without generalisation stalls at solid, not deep.
    #[test]
    fn ontology_without_verified_rules_stalls_at_tier_two() {
        let e = compute_evolution(
            EvolutionInputs {
                ontology_size: 5000,
                verified_rules: 0,
                ..Default::default()
            },
            0,
        );
        let m = e
            .dimensions
            .iter()
            .find(|d| d.dimension == "memory")
            .unwrap();
        assert_eq!(m.tier, 2, "{m:?}");
        assert!(m.next.as_deref().unwrap().contains("verification"));
    }

    /// The perverse incentive this dimension used to carry: conduct could only
    /// be earned by misbehaving and then being corrected, so a well-behaved
    /// agent was permanently stuck at zero. A clean record under real exposure
    /// must count as evidence of trustworthy behaviour too.
    #[test]
    fn clean_record_under_exposure_earns_conduct_without_ever_misbehaving() {
        let e = compute_evolution(
            EvolutionInputs {
                total_episodes: 120,
                anomalies_ever: 0,
                anomalies_resolved: 0,
                persona_version: 1, // never corrected
                ..Default::default()
            },
            0,
        );
        let c = e
            .dimensions
            .iter()
            .find(|d| d.dimension == "conduct")
            .unwrap();
        assert_eq!(c.tier, 2, "{c:?}");
        assert!(c.evidence.contains("no anomaly raised"), "{c:?}");
    }

    /// Exposure is the denominator, not the achievement. A handful of clean
    /// episodes proves nothing, so it must not earn conduct.
    #[test]
    fn a_few_clean_episodes_prove_nothing() {
        let e = compute_evolution(
            EvolutionInputs {
                total_episodes: 5,
                anomalies_ever: 0,
                persona_version: 1,
                ..Default::default()
            },
            0,
        );
        let c = e
            .dimensions
            .iter()
            .find(|d| d.dimension == "conduct")
            .unwrap();
        assert_eq!(c.tier, 0, "{c:?}");
        // Names both routes, since either can get this agent moving.
        assert!(c.next.as_deref().unwrap().contains("they add"), "{c:?}");
        assert!(
            c.next.as_deref().unwrap().contains("review correction"),
            "{c:?}"
        );
    }

    fn conduct_of(e: &AgentEvolution) -> DimensionScore {
        e.dimensions
            .iter()
            .find(|d| d.dimension == "conduct")
            .unwrap()
            .clone()
    }

    /// Being corrected is not a permanent stain. An agent flagged four times
    /// across a thousand episodes, all resolved, is MORE trustworthy than one
    /// never exercised hard enough to trip — an earlier zero-tolerance rule
    /// inverted that and locked it out of the reliability route forever.
    #[test]
    fn a_corrected_agent_still_earns_reliability_at_a_low_incident_rate() {
        let e = compute_evolution(
            EvolutionInputs {
                total_episodes: 1000,
                anomalies_ever: 4, // 0.4% — under the 1% bar
                anomalies_resolved: 4,
                persona_version: 3, // +2 corrections
                ..Default::default()
            },
            0,
        );
        let c = conduct_of(&e);
        // Reliability (3) + governability (2) both fire and add.
        assert_eq!(c.tier, 3, "{c:?}");
        assert!(c.evidence.contains("resolved"), "{c:?}");
        assert!(c.evidence.contains("incident rate"), "{c:?}");
    }

    /// A high incident rate still fails reliability, however much exposure
    /// there is — volume must not launder a bad record.
    #[test]
    fn a_high_incident_rate_fails_reliability() {
        let e = compute_evolution(
            EvolutionInputs {
                total_episodes: 100,
                anomalies_ever: 20, // 20% — far above the bar
                anomalies_resolved: 0,
                persona_version: 1,
                ..Default::default()
            },
            0,
        );
        let c = conduct_of(&e);
        assert_eq!(c.tier, 0, "{c:?}");
        assert!(c.evidence.contains("above the"), "{c:?}");
    }

    /// The two properties are orthogonal, so they ADD. An agent that is both
    /// reliable and governable has demonstrated more than one that is either.
    ///
    /// Tiers are coarse (0–3), so a single added point does not always cross a
    /// boundary; this checks the boundary case where the increment is
    /// observable, and separately that the evidence names both contributions so
    /// progress is legible between boundaries.
    #[test]
    fn the_two_routes_add_rather_than_compete() {
        // Reliability alone at the first step: 1 point -> tier 1.
        let reliable_only = conduct_of(&compute_evolution(
            EvolutionInputs {
                total_episodes: 25, // reliability 1
                persona_version: 1, // governability 0
                ..Default::default()
            },
            0,
        ));
        assert_eq!(reliable_only.tier, 1, "{reliable_only:?}");

        // Same exposure plus two review outcomes: 1 + 1 = 2 points -> tier 2.
        // Under the previous max() rule this stayed at 1, so a reliable agent
        // gained nothing from engaging with review.
        let both = conduct_of(&compute_evolution(
            EvolutionInputs {
                total_episodes: 25,
                anomalies_ever: 0,
                anomalies_resolved: 1,
                persona_version: 2, // 1 resolved + 1 correction = 2 outcomes
                ..Default::default()
            },
            0,
        ));
        assert_eq!(
            both.tier, 2,
            "both virtues must beat one: {both:?} vs {reliable_only:?}"
        );

        // And where the sum stays inside a tier, both contributions are still
        // named, so the owner can see what is carrying the score.
        let inside = conduct_of(&compute_evolution(
            EvolutionInputs {
                total_episodes: 120,
                anomalies_ever: 1,
                anomalies_resolved: 1,
                persona_version: 2,
                ..Default::default()
            },
            0,
        ));
        assert!(inside.evidence.contains("resolved"), "{inside:?}");
        assert!(inside.evidence.contains("episodes observed"), "{inside:?}");
    }

    /// The deepest conduct requires BOTH routes. A spotless agent that has never
    /// been corrected tops out at solid — still enough for the highest rank, but
    /// "never tested under correction" is genuinely less evidence than "tested
    /// and governable".
    #[test]
    fn spotless_but_never_corrected_tops_out_at_solid() {
        let c = conduct_of(&compute_evolution(
            EvolutionInputs {
                total_episodes: 5000, // maximum reliability
                anomalies_ever: 0,
                anomalies_resolved: 0,
                persona_version: 1, // never corrected
                ..Default::default()
            },
            0,
        ));
        assert_eq!(c.tier, 2, "{c:?}");
        assert!(
            c.next.as_deref().unwrap().contains("governability"),
            "{c:?}"
        );
    }

    /// Exemplar requires all four dimensions solid. Three-of-four is
    /// `specialist` — the top rank has to mean well-rounded, or it would have
    /// been handed to three agents the moment the clean-conduct route opened.
    #[test]
    fn three_solid_dimensions_is_specialist_not_exemplar() {
        let e = compute_evolution(
            EvolutionInputs {
                ontology_size: 1071, // memory 2 (no verified rules)
                verified_rules: 0,
                mean_contribution: Some(0.02),
                n_forecasts: 48, // judgment 3
                eval_mean_score: Some(0.95),
                eval_dimensions_tracked: 4, // craft 3
                total_episodes: 55,
                anomalies_ever: 0, // conduct 1 (clean, modest exposure)
                persona_version: 1,
                ..Default::default()
            },
            0,
        );
        assert_eq!(e.level, 4, "{:?}", e.dimensions);
        assert_eq!(e.rank.as_deref(), Some("specialist"));
    }

    /// Open anomalies suppress conduct even with a long clean history —
    /// conduct is a present-tense property.
    #[test]
    fn open_anomalies_suppress_conduct() {
        let e = compute_evolution(
            EvolutionInputs {
                anomalies_resolved: 50,
                persona_version: 9,
                anomalies_open: 1,
                ..Default::default()
            },
            0,
        );
        let c = e
            .dimensions
            .iter()
            .find(|d| d.dimension == "conduct")
            .unwrap();
        assert_eq!(c.tier, 0, "{c:?}");
    }

    /// Regression must be visible, or "don't regress" is not an incentive.
    #[test]
    fn falling_below_the_high_water_mark_flags_regression() {
        let e = compute_evolution(
            EvolutionInputs {
                ontology_size: 10,
                ..Default::default()
            },
            4,
        );
        assert!(e.regressed);
        assert_eq!(e.peak_level, 4);
        assert!(e.level < 4);
    }

    /// The peak only ever ratchets up.
    #[test]
    fn peak_never_decreases() {
        let e = compute_evolution(
            EvolutionInputs {
                ontology_size: 500,
                verified_rules: 40,
                mean_contribution: Some(0.02),
                n_forecasts: 40,
                anomalies_resolved: 12,
                persona_version: 4,
                eval_mean_score: Some(0.93),
                eval_dimensions_tracked: 4,
                ..Default::default()
            },
            2,
        );
        assert_eq!(e.peak_level, 5);
        assert!(!e.regressed);
    }

    /// A badge that only says "level 2" is decoration. It has to say what to
    /// do next, and that advice must target the weakest dimension.
    #[test]
    fn next_step_targets_the_weakest_dimension() {
        let e = compute_evolution(
            EvolutionInputs {
                ontology_size: 500,
                verified_rules: 40,
                eval_mean_score: Some(0.95),
                eval_dimensions_tracked: 4,
                ..Default::default() // no judgment, no conduct
            },
            0,
        );
        assert!(!e.next_step.is_empty());
        let weakest: Vec<_> = e.dimensions.iter().filter(|d| d.tier == 0).collect();
        assert!(!weakest.is_empty());
        assert!(
            weakest
                .iter()
                .any(|d| d.next.as_deref() == Some(e.next_step.as_str())),
            "next_step {:?} should come from a tier-0 dimension",
            e.next_step
        );
    }

    /// Every rank must be reachable, or the ladder has dead rungs that make
    /// progress feel arbitrary. Fixtures are explicit per rank rather than
    /// swept, so a ladder change that strands a rung fails here loudly.
    #[test]
    fn every_rank_is_reachable() {
        // 0 — nothing measured at all.
        let l0 = EvolutionInputs {
            persona_version: 1,
            ..Default::default()
        };
        // 1 — a single emerging dimension.
        let l1 = EvolutionInputs {
            ontology_size: 10,
            persona_version: 1,
            ..Default::default()
        };
        // 2 — two dimensions, at least one solid.
        let l2 = EvolutionInputs {
            ontology_size: 30,
            persona_version: 1,
            eval_mean_score: Some(0.80),
            eval_dimensions_tracked: 2,
            ..Default::default()
        };
        // 3 — three dimensions, two of them solid.
        let l3 = EvolutionInputs {
            ontology_size: 30,
            persona_version: 1,
            mean_contribution: Some(0.01),
            n_forecasts: 10,
            eval_mean_score: Some(0.80),
            eval_dimensions_tracked: 2,
            ..Default::default()
        };
        // 4 — all four in play, three solid, none deep.
        let l4 = EvolutionInputs {
            ontology_size: 30,
            persona_version: 1,
            anomalies_resolved: 2,
            mean_contribution: Some(0.01),
            n_forecasts: 10,
            eval_mean_score: Some(0.80),
            eval_dimensions_tracked: 2,
            ..Default::default()
        };
        // 5 — all four solid, one at depth.
        let l5 = EvolutionInputs {
            ontology_size: 500,
            verified_rules: 40,
            persona_version: 1,
            anomalies_resolved: 5,
            mean_contribution: Some(0.01),
            n_forecasts: 10,
            eval_mean_score: Some(0.80),
            eval_dimensions_tracked: 2,
            ..Default::default()
        };

        for (expected, inputs) in [(0, l0), (1, l1), (2, l2), (3, l3), (4, l4), (5, l5)] {
            let e = compute_evolution(inputs, 0);
            assert_eq!(
                e.level,
                expected,
                "expected rank {expected} ({}), got {} ({}) — dims {:?}",
                rank_name(expected),
                e.level,
                e.rank.as_deref().unwrap_or("unranked"),
                e.dimensions
                    .iter()
                    .map(|d| (d.dimension.as_str(), d.tier))
                    .collect::<Vec<_>>()
            );
        }
    }
}
