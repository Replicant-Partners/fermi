//! Loop 4 — composition evolution driven by attribution evidence.
//!
//! ## What was missing
//!
//! `composition_versions` has had an accept/reject flow since mig-113, and the
//! dashboard has always had a card for it. It permanently read "no pending
//! evolution proposals" because **nothing ever generated one**. The loop was
//! structurally complete and empty: a mechanism with no signal feeding it.
//!
//! The signal now exists. Exact Shapley attribution
//! (`src/attribution/`, mig-188) produces two things per resolved forecast:
//!
//! - `forecast_agent_credit` — how much each agent contributed (φ).
//! - `forecast_agent_interactions` — whether each *pair* is synergistic or
//!   redundant.
//!
//! Marginal credit alone cannot answer "who should be on this team": an agent
//! can be individually valuable yet wholly redundant with a cheaper one. The
//! pairwise interaction index is what makes team-shape decisions possible, and
//! it is why the attribution engine computes it.
//!
//! ## Why proposals are generated, not applied
//!
//! Loop 4's design has a human accept the change, and that is the right shape
//! here. Attribution measures contribution *through the current model*; a
//! negative φ can mean a weak agent, or a mis-specified driver exponent, or a
//! driver that is genuinely predictive but currently mis-weighted. Dropping an
//! agent automatically on that basis would let a modelling error silently prune
//! the roster.
//!
//! So this module writes `composition_versions` rows with `accepted_by = NULL`
//! and a `diff_summary` that states the evidence. They appear in the existing
//! Loop 4 card, and a human decides.
//!
//! ## Evidence thresholds
//!
//! Every proposal carries the sample size it rests on and is suppressed below
//! [`MIN_FORECASTS_FOR_PROPOSAL`]. This matters more than usual here: the loop
//! is young, and a confident-sounding proposal derived from two correlated
//! forecasts would be worse than no proposal, because Loop 4 acts on it.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::AppState;
use fermi_auth::AuthPrincipal;

/// Below this many attributed forecasts, say nothing. A roster change proposed
/// from one or two correlated forecasts is noise wearing a recommendation's
/// clothes.
pub const MIN_FORECASTS_FOR_PROPOSAL: i64 = 5;

/// A pair whose mean interaction is below this is treated as substitutable.
/// Negative but tiny values are ordinary sampling wobble, not redundancy.
pub const REDUNDANCY_THRESHOLD: f64 = -0.005;

/// Above this, a pair is genuinely complementary and worth protecting.
pub const SYNERGY_THRESHOLD: f64 = 0.005;

/// Per-agent evidence feeding the proposal logic.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentEvidence {
    pub agent_name: String,
    pub mean_credit: f64,
    pub n_forecasts: i64,
}

/// Per-pair evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct PairEvidence {
    pub agent_a: String,
    pub agent_b: String,
    pub mean_interaction: f64,
    pub n_forecasts: i64,
}

/// A recommended change to a composition, with the reasoning attached.
#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    /// `drop_negative` | `drop_redundant` | `keep_synergy`
    pub kind: &'static str,
    pub subject: String,
    pub rationale: String,
    /// Smallest sample any part of this proposal rests on.
    pub n_forecasts: i64,
}

/// Derive composition proposals from attribution evidence.
///
/// Pure so the judgement can be tested without a database. Ordering is
/// deterministic (worst-contributor first) so repeated runs produce stable
/// proposals rather than reshuffling the operator's queue.
pub fn derive_proposals(agents: &[AgentEvidence], pairs: &[PairEvidence]) -> Vec<Proposal> {
    let mut out = Vec::new();

    // ── Agents that actively hurt ────────────────────────────────────────────
    // Negative mean φ means that across resolved forecasts this agent moved the
    // forecast away from the truth on balance. That is the strongest single
    // reason to reconsider membership.
    let mut negatives: Vec<&AgentEvidence> = agents
        .iter()
        .filter(|a| a.mean_credit < 0.0 && a.n_forecasts >= MIN_FORECASTS_FOR_PROPOSAL)
        .collect();
    negatives.sort_by(|x, y| {
        x.mean_credit
            .partial_cmp(&y.mean_credit)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for a in negatives {
        out.push(Proposal {
            kind: "drop_negative",
            subject: a.agent_name.clone(),
            rationale: format!(
                "Mean Shapley contribution {:+.5} over {} attributed forecasts — this agent \
                 moved forecasts away from their outcomes on balance. Verify the driver \
                 weighting before removing: a mis-specified exponent looks identical to a \
                 weak agent.",
                a.mean_credit, a.n_forecasts
            ),
            n_forecasts: a.n_forecasts,
        });
    }

    // ── Redundant pairs ──────────────────────────────────────────────────────
    // Negative interaction means the two substitute for each other: together
    // they are worth less than the sum of their separate additions. Recommend
    // dropping the one contributing less, and name both so the operator can see
    // the trade.
    let credit_of = |name: &str| agents.iter().find(|a| a.agent_name == name);
    let mut redundant: Vec<&PairEvidence> = pairs
        .iter()
        .filter(|p| {
            p.mean_interaction < REDUNDANCY_THRESHOLD && p.n_forecasts >= MIN_FORECASTS_FOR_PROPOSAL
        })
        .collect();
    redundant.sort_by(|x, y| {
        x.mean_interaction
            .partial_cmp(&y.mean_interaction)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for p in redundant {
        let (ca, cb) = (credit_of(&p.agent_a), credit_of(&p.agent_b));
        // Drop the weaker of the two. If we have no credit for either, we
        // cannot say which to drop — report the redundancy without a verdict
        // rather than picking arbitrarily.
        let weaker = match (ca, cb) {
            (Some(a), Some(b)) => Some(if a.mean_credit <= b.mean_credit {
                (&a.agent_name, a.mean_credit, &b.agent_name, b.mean_credit)
            } else {
                (&b.agent_name, b.mean_credit, &a.agent_name, a.mean_credit)
            }),
            _ => None,
        };
        let rationale = match weaker {
            Some((lo_name, lo, hi_name, hi)) => format!(
                "Interaction {:+.5} over {} forecasts — {} and {} are substitutable rather \
                 than complementary. {} contributes less than {} ({:+.5} vs {:+.5}), so \
                 dropping it should cost little while reducing cost and coordination \
                 overhead.",
                p.mean_interaction, p.n_forecasts, p.agent_a, p.agent_b, lo_name, hi_name, lo, hi
            ),
            None => format!(
                "Interaction {:+.5} over {} forecasts — {} and {} are substitutable, but \
                 per-agent credit is missing for at least one of them, so which to drop is \
                 not yet determined.",
                p.mean_interaction, p.n_forecasts, p.agent_a, p.agent_b
            ),
        };
        out.push(Proposal {
            kind: "drop_redundant",
            subject: weaker
                .map(|(n, _, _, _)| n.clone())
                .unwrap_or_else(|| format!("{} + {}", p.agent_a, p.agent_b)),
            rationale,
            n_forecasts: p.n_forecasts,
        });
    }

    // ── Pairs worth protecting ───────────────────────────────────────────────
    // Not a change, but Loop 4 should be able to argue *against* a change too.
    // Without this, the only evidence in the queue is destructive.
    let mut synergies: Vec<&PairEvidence> = pairs
        .iter()
        .filter(|p| {
            p.mean_interaction > SYNERGY_THRESHOLD && p.n_forecasts >= MIN_FORECASTS_FOR_PROPOSAL
        })
        .collect();
    synergies.sort_by(|x, y| {
        y.mean_interaction
            .partial_cmp(&x.mean_interaction)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for p in synergies.into_iter().take(3) {
        out.push(Proposal {
            kind: "keep_synergy",
            subject: format!("{} + {}", p.agent_a, p.agent_b),
            rationale: format!(
                "Interaction {:+.5} over {} forecasts — these two are worth more together \
                 than apart. Keep the pair intact when trimming the roster.",
                p.mean_interaction, p.n_forecasts
            ),
            n_forecasts: p.n_forecasts,
        });
    }

    out
}

/// Load attribution evidence for a workspace's resolved forecasts.
///
/// Gated on both validity checks, exactly as the calibration read path is: a
/// proposal derived from credit polluted by Monte Carlo noise, or from a
/// reconstruction of a forecast that never existed, would be worse than none.
async fn load_evidence(
    pool: &PgPool,
    workspace_id: Uuid,
) -> Result<(Vec<AgentEvidence>, Vec<PairEvidence>), String> {
    let agent_rows = sqlx::query(
        "SELECT c.agent_name,
                AVG(c.shapley_value) AS mean_credit,
                COUNT(*)             AS n
           FROM forecast_agent_credit c
           JOIN forecast_attributions a
             ON a.forecast_id = c.forecast_id AND a.neutralisation = c.neutralisation
           JOIN fermi_forecasts f ON f.id = c.forecast_id
          WHERE f.workspace_id = $1
            AND c.neutralisation = 'identity'
            AND a.efficiency_residual < 1e-6
            AND (a.reconstruction_error IS NULL OR a.reconstruction_error < 0.01)
          GROUP BY c.agent_name",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("agent evidence query failed: {e}"))?;

    let agents: Vec<AgentEvidence> = agent_rows
        .iter()
        .map(|r| AgentEvidence {
            agent_name: r.try_get("agent_name").unwrap_or_default(),
            mean_credit: r.try_get("mean_credit").unwrap_or(0.0),
            n_forecasts: r.try_get("n").unwrap_or(0),
        })
        .collect();

    let pair_rows = sqlx::query(
        "SELECT i.agent_a, i.agent_b,
                AVG(i.interaction_index) AS mean_interaction,
                COUNT(*)                 AS n
           FROM forecast_agent_interactions i
           JOIN forecast_attributions a
             ON a.forecast_id = i.forecast_id AND a.neutralisation = i.neutralisation
           JOIN fermi_forecasts f ON f.id = i.forecast_id
          WHERE f.workspace_id = $1
            AND i.neutralisation = 'identity'
            AND a.efficiency_residual < 1e-6
            AND (a.reconstruction_error IS NULL OR a.reconstruction_error < 0.01)
          GROUP BY i.agent_a, i.agent_b",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("pair evidence query failed: {e}"))?;

    let pairs: Vec<PairEvidence> = pair_rows
        .iter()
        .map(|r| PairEvidence {
            agent_a: r.try_get("agent_a").unwrap_or_default(),
            agent_b: r.try_get("agent_b").unwrap_or_default(),
            mean_interaction: r.try_get("mean_interaction").unwrap_or(0.0),
            n_forecasts: r.try_get("n").unwrap_or(0),
        })
        .collect();

    Ok((agents, pairs))
}

fn proposals_json(proposals: &[Proposal]) -> Vec<Value> {
    proposals
        .iter()
        .map(|p| {
            json!({
                "kind": p.kind,
                "subject": p.subject,
                "rationale": p.rationale,
                "n_forecasts": p.n_forecasts,
            })
        })
        .collect()
}

/// GET /api/workspaces/:workspace_id/composition/suggestions
///
/// Read-only. Computes what Loop 4 *would* propose from current attribution
/// evidence, without writing anything.
pub async fn composition_suggestions_handler(
    State(state): State<AppState>,
    _principal: AuthPrincipal,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let (agents, pairs) = load_evidence(&state.db, workspace_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let proposals = derive_proposals(&agents, &pairs);

    let max_n = agents.iter().map(|a| a.n_forecasts).max().unwrap_or(0);
    Ok(Json(json!({
        "workspace_id": workspace_id,
        "proposals": proposals_json(&proposals),
        "evidence": {
            "agents_with_credit": agents.len(),
            "pairs_with_interaction": pairs.len(),
            "max_attributed_forecasts": max_n,
            "min_required": MIN_FORECASTS_FOR_PROPOSAL,
            "sufficient": max_n >= MIN_FORECASTS_FOR_PROPOSAL,
        },
        "note": if max_n < MIN_FORECASTS_FOR_PROPOSAL {
            "Not enough attributed forecasts to propose roster changes yet. Attribution only \
             covers forecasts resolved after the claim ledger shipped; historical ones cannot \
             be reconstructed."
        } else {
            "Proposals are evidence, not instructions. Attribution measures contribution through \
             the current model, so a negative contribution can also mean a mis-specified driver."
        },
    })))
}

/// POST /api/workspaces/:workspace_id/composition/suggestions/materialise
///
/// Writes the current suggestions into `composition_versions` as a pending
/// proposal so it surfaces in the Loop 4 queue for a human to accept or reject.
///
/// Deliberately explicit rather than automatic on resolution: a roster change
/// is consequential, and attribution can be confounded by model
/// mis-specification. Idempotent per version bump — re-running with unchanged
/// evidence produces a new version only if there is something to say.
pub async fn materialise_composition_proposal_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(workspace_id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();

    // Owner/admin only: this writes into the workspace's evolution history.
    let allowed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM team_members
                        WHERE team_id = $1 AND member_id = $2 AND role IN ('owner','admin'))",
    )
    .bind(workspace_id)
    .bind(&user_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);
    if !allowed {
        return Err((
            StatusCode::FORBIDDEN,
            "Only a workspace owner or admin can file composition proposals".into(),
        ));
    }

    let (agents, pairs) = load_evidence(&state.db, workspace_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    let proposals = derive_proposals(&agents, &pairs);

    if proposals.is_empty() {
        return Ok(Json(json!({
            "created": false,
            "reason": "No proposals derivable from current attribution evidence.",
            "proposals": [],
        })));
    }

    // Don't stack duplicates: if an identical pending proposal already exists,
    // filing another only buries the queue.
    let summary = proposals
        .iter()
        .map(|p| format!("[{}] {}: {}", p.kind, p.subject, p.rationale))
        .collect::<Vec<_>>()
        .join("\n");

    let already: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM composition_versions
                        WHERE workspace_id = $1 AND accepted_by IS NULL
                          AND diff_summary = $2)",
    )
    .bind(workspace_id)
    .bind(&summary)
    .fetch_one(&state.db)
    .await
    .unwrap_or(false);
    if already {
        return Ok(Json(json!({
            "created": false,
            "reason": "An identical proposal is already pending.",
            "proposals": proposals_json(&proposals),
        })));
    }

    let next_version: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(version_number), 0) + 1 FROM composition_versions WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(&state.db)
    .await
    .unwrap_or(1);

    sqlx::query(
        "INSERT INTO composition_versions
             (workspace_id, version_number, diff_summary, proposed_by, created_at)
         VALUES ($1, $2, $3, 'attribution_loop4', NOW())",
    )
    .bind(workspace_id)
    .bind(next_version)
    .bind(&summary)
    .execute(&state.db)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to file proposal: {e}"),
        )
    })?;

    Ok(Json(json!({
        "created": true,
        "version_number": next_version,
        "proposals": proposals_json(&proposals),
        "note": "Filed as a pending composition version. It now appears in the Loop 4 queue awaiting a human decision.",
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(name: &str, credit: f64, n: i64) -> AgentEvidence {
        AgentEvidence {
            agent_name: name.into(),
            mean_credit: credit,
            n_forecasts: n,
        }
    }
    fn p(x: &str, y: &str, ix: f64, n: i64) -> PairEvidence {
        PairEvidence {
            agent_a: x.into(),
            agent_b: y.into(),
            mean_interaction: ix,
            n_forecasts: n,
        }
    }

    /// Thin evidence must produce silence. Loop 4 acts on these, so a
    /// confident-sounding proposal from two correlated forecasts is worse than
    /// no proposal at all.
    #[test]
    fn thin_evidence_proposes_nothing() {
        let out = derive_proposals(
            &[a("weak", -0.5, 2), a("strong", 0.9, 2)],
            &[p("strong", "weak", -0.9, 2)],
        );
        assert!(out.is_empty(), "{out:?}");
    }

    /// A negative contributor is the strongest single reason to reconsider
    /// membership — but the rationale must warn that a mis-specified driver
    /// looks identical, so a human does not prune on a modelling error.
    #[test]
    fn negative_contributor_is_proposed_for_removal_with_a_caveat() {
        let out = derive_proposals(&[a("drag", -0.02, 12), a("good", 0.05, 12)], &[]);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "drop_negative");
        assert_eq!(out[0].subject, "drag");
        assert!(out[0].rationale.contains("mis-specified"), "{:?}", out[0]);
    }

    /// Redundancy must name the *weaker* member as the one to drop — dropping
    /// the stronger of a substitutable pair costs real accuracy.
    #[test]
    fn redundant_pair_drops_the_weaker_member() {
        let out = derive_proposals(
            &[a("cheap", 0.01, 20), a("strong", 0.08, 20)],
            &[p("cheap", "strong", -0.03, 20)],
        );
        let r = out.iter().find(|x| x.kind == "drop_redundant").unwrap();
        assert_eq!(r.subject, "cheap", "{r:?}");
        assert!(r.rationale.contains("substitutable"), "{r:?}");
    }

    /// Without per-agent credit we cannot say which of a redundant pair to
    /// drop, and must say so rather than pick arbitrarily.
    #[test]
    fn redundancy_without_credit_withholds_a_verdict() {
        let out = derive_proposals(&[], &[p("x", "y", -0.04, 9)]);
        let r = out.iter().find(|x| x.kind == "drop_redundant").unwrap();
        assert_eq!(r.subject, "x + y");
        assert!(r.rationale.contains("not yet determined"), "{r:?}");
    }

    /// Loop 4 must be able to argue against a change too, or the only evidence
    /// in the queue is destructive.
    #[test]
    fn synergy_is_reported_as_worth_protecting() {
        let out = derive_proposals(
            &[a("m", 0.02, 30), a("n", 0.03, 30)],
            &[p("m", "n", 0.04, 30)],
        );
        let s = out.iter().find(|x| x.kind == "keep_synergy").unwrap();
        assert_eq!(s.subject, "m + n");
        assert!(s.rationale.contains("together"), "{s:?}");
    }

    /// Tiny negative interactions are sampling wobble, not redundancy.
    #[test]
    fn near_zero_interaction_is_not_treated_as_redundancy() {
        let out = derive_proposals(
            &[a("m", 0.02, 30), a("n", 0.03, 30)],
            &[p("m", "n", -0.0001, 30)],
        );
        assert!(out.is_empty(), "{out:?}");
    }

    /// Ordering is deterministic and worst-first, so the operator's queue is
    /// stable across runs instead of reshuffling.
    #[test]
    fn worst_contributor_is_listed_first() {
        let out = derive_proposals(
            &[a("bad", -0.01, 10), a("worst", -0.09, 10), a("ok", 0.2, 10)],
            &[],
        );
        assert_eq!(out[0].subject, "worst");
        assert_eq!(out[1].subject, "bad");
    }

    /// The healthy case: everyone contributes, nothing is redundant, so the
    /// loop stays quiet rather than manufacturing churn.
    #[test]
    fn healthy_composition_produces_no_change_proposals() {
        let out = derive_proposals(
            &[a("x", 0.03, 40), a("y", 0.04, 40)],
            &[p("x", "y", 0.0001, 40)],
        );
        assert!(
            out.iter().all(|q| q.kind == "keep_synergy"),
            "no destructive proposals expected: {out:?}"
        );
        assert!(out.is_empty(), "{out:?}");
    }
}
