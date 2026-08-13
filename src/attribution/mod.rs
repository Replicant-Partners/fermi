//! Combinatorial credit assignment for multi-agent forecasts.
//!
//! # The problem
//!
//! A forecast is produced by a team. The outcome scores the *team*. Loop 5
//! needs to score *agents*, and Loop 4 needs to know which *combinations*
//! work. Reading a team score as a per-agent score is the mistake this module
//! exists to remove: when every member is cited on every forecast, all members
//! receive an identical score forever, at any sample size, because the
//! membership matrix is rank-deficient and per-agent skill is simply not
//! identifiable from team outcomes.
//!
//! Observing more real-world compositions raises that rank, but slowly and
//! confoundedly — agent *i*'s score becomes "mean performance of teams
//! containing *i*", which credits *i* for its teammates' work. For a
//! quadrennial tournament it is not a viable primary mechanism.
//!
//! # The approach
//!
//! We do not need real-world permutations, because the forecast model is
//! re-runnable. Given each agent's recorded claim (its driver multipliers) we
//! can *synthesise* the counterfactual forecast for any subset `S` of agents by
//! applying the claims of agents in `S` and neutralising the rest. That yields
//! a value function `v(S)` over all `2^n` subsets from a single real forecast,
//! which is exactly the input Shapley attribution requires.
//!
//! `v(S)` is defined as an *improvement* over the no-agent baseline, so it is
//! positively oriented and `v(∅) = 0`:
//!
//! ```text
//!   v(S) = Brier(p_∅) - Brier(p_S)
//! ```
//!
//! where `p_S` is the probability the model produces using only the claims of
//! agents in `S`. Positive `v` means the subset moved the forecast toward the
//! truth.
//!
//! # Why Shapley specifically
//!
//! The Shapley value is the *unique* attribution satisfying all four of:
//!
//! - **Efficiency** — `Σᵢ φᵢ = v(N) - v(∅)`. Per-agent credit exactly
//!   decomposes the team's total improvement. Nothing is invented, nothing
//!   is lost. This is the property that makes the numbers honest accounting
//!   rather than a heuristic split.
//! - **Symmetry** — agents with identical marginal behaviour get identical
//!   credit, regardless of labels or ordering.
//! - **Dummy** — an agent that never changes the outcome gets exactly zero.
//!   A neutral or ignored specialist cannot accumulate credit.
//! - **Additivity** — attribution over a sum of games is the sum of
//!   attributions, which is what lets per-forecast credit be averaged across
//!   forecasts coherently.
//!
//! Any cheaper scheme (Sobol weighting, leave-one-out only, "split evenly")
//! violates at least one of these. Leave-one-out in particular ignores the
//! order in which agents are added and so misattributes all interaction
//! effects; Shapley averages over every ordering, which is precisely why it is
//! the rigorous choice for a *combinatorial* loop.
//!
//! # Interactions — the Loop 4 signal
//!
//! Marginal credit alone cannot answer "which team should we run". Two agents
//! may each be individually valuable yet redundant together, or individually
//! weak yet complementary. [`pairwise_interactions`] computes the Shapley
//! interaction index, whose sign is directly actionable:
//!
//! - `> 0` **synergy** — the pair is worth more together than apart. Keep both.
//! - `≈ 0` independent.
//! - `< 0` **redundancy** — they overlap. Consider dropping the cheaper one.
//!
//! # References
//!
//! - Shapley (1953), "A Value for n-Person Games".
//! - Grabisch & Roubens (1999), "An axiomatic approach to the concept of
//!   interaction among players in cooperative games" — the interaction index.
//! - Lundberg & Lee (2017), "A Unified Approach to Interpreting Model
//!   Predictions" — the same construction applied to feature attribution.

use std::collections::HashMap;

pub mod counterfactual;

/// Exact attribution for one cooperative game (in practice: one resolved
/// forecast).
#[derive(Debug, Clone, PartialEq)]
pub struct ShapleyAttribution {
    /// Credit per player, in the same order as the `players` slice supplied to
    /// [`exact_shapley`]. Positively oriented: higher is a larger contribution
    /// to the team's improvement over baseline.
    pub values: Vec<f64>,
    /// `v(N) - v(∅)` — the total the credits decompose. Kept alongside the
    /// values so callers can assert efficiency rather than trust it.
    pub total: f64,
}

impl ShapleyAttribution {
    /// Absolute efficiency residual `|Σφᵢ - (v(N) - v(∅))|`.
    ///
    /// Exact Shapley makes this zero up to floating-point summation error.
    /// Callers persisting attributions should record it: a residual that is
    /// not ~1e-9 means the value function was inconsistent (e.g. a
    /// non-deterministic model re-run), which invalidates the decomposition.
    pub fn efficiency_residual(&self) -> f64 {
        (self.values.iter().sum::<f64>() - self.total).abs()
    }
}

/// Exact Shapley values by enumeration over all `2^n` subsets.
///
/// `value_fn` receives a subset as a bitmask over `0..n` (bit `i` set means
/// player `i` is present) and must return `v(S)`. It must be **deterministic**:
/// the same mask must always yield the same value, or efficiency breaks. When
/// `v(S)` comes from a Monte Carlo model run, fix the seed per forecast.
///
/// Exact enumeration is deliberate. `n` here is the number of specialists on a
/// composition — 4 to 8 in practice, so 16 to 256 subsets, which is nothing
/// next to the model re-run itself. Sampling approximations exist for large
/// `n`, but they trade away the exactness of the efficiency property, and
/// exactness is the whole reason for choosing Shapley.
///
/// Returns `None` if `n == 0`, or if `n` exceeds [`MAX_EXACT_PLAYERS`] (guard
/// against a caller accidentally requesting `2^30` model runs).
pub fn exact_shapley<F>(n_players: usize, mut value_fn: F) -> Option<ShapleyAttribution>
where
    F: FnMut(u32) -> f64,
{
    if n_players == 0 || n_players > MAX_EXACT_PLAYERS {
        return None;
    }

    let n_subsets = 1usize << n_players;

    // Memoise v(S) once per subset. Without this the loop below would call
    // value_fn O(n · 2^n) times instead of 2^n, and each call may be a full
    // Monte Carlo re-run.
    let mut v = Vec::with_capacity(n_subsets);
    for mask in 0..n_subsets {
        v.push(value_fn(mask as u32));
    }

    // Weight for a marginal contribution measured against a coalition of size
    // s (excluding the player): s! (n-s-1)! / n!. Equivalently the probability
    // that a uniformly random ordering places exactly those s players first.
    let mut weights = Vec::with_capacity(n_players);
    for s in 0..n_players {
        weights.push(factorial(s) * factorial(n_players - s - 1) / factorial(n_players));
    }

    let mut values = vec![0.0f64; n_players];
    for (i, val) in values.iter_mut().enumerate() {
        let bit = 1usize << i;
        let mut acc = 0.0;
        for mask in 0..n_subsets {
            // Iterate coalitions NOT containing i; add i to each.
            if mask & bit != 0 {
                continue;
            }
            let s = (mask as u32).count_ones() as usize;
            acc += weights[s] * (v[mask | bit] - v[mask]);
        }
        *val = acc;
    }

    let full = (1usize << n_players) - 1;
    Some(ShapleyAttribution {
        values,
        total: v[full] - v[0],
    })
}

/// Largest `n` for which exact enumeration is permitted (`2^16` subsets).
/// Compositions are small; a request beyond this is a bug, not a workload.
pub const MAX_EXACT_PLAYERS: usize = 16;

/// Shapley interaction index for every unordered pair — the combinatorial
/// signal Loop 4 needs.
///
/// For players `i ≠ j` the index averages the second-order discrete
/// derivative over all coalitions excluding both:
///
/// ```text
///   Δᵢⱼ(S) = v(S∪{i,j}) - v(S∪{i}) - v(S∪{j}) + v(S)
/// ```
///
/// weighted by `s!(n-s-2)!/(n-1)!`. Positive means synergy (together they are
/// worth more than the sum of their separate additions), negative means
/// redundancy (they substitute for each other).
///
/// Returned map is keyed `(min, max)` so each pair appears once.
pub fn pairwise_interactions<F>(
    n_players: usize,
    mut value_fn: F,
) -> Option<HashMap<(usize, usize), f64>>
where
    F: FnMut(u32) -> f64,
{
    if !(2..=MAX_EXACT_PLAYERS).contains(&n_players) {
        return None;
    }

    let n_subsets = 1usize << n_players;
    let mut v = Vec::with_capacity(n_subsets);
    for mask in 0..n_subsets {
        v.push(value_fn(mask as u32));
    }

    // Coalition sizes now range over 0..=n-2 because two players are excluded.
    let mut weights = Vec::with_capacity(n_players.saturating_sub(1));
    for s in 0..=(n_players - 2) {
        weights.push(factorial(s) * factorial(n_players - s - 2) / factorial(n_players - 1));
    }

    let mut out = HashMap::new();
    for i in 0..n_players {
        for j in (i + 1)..n_players {
            let bi = 1usize << i;
            let bj = 1usize << j;
            let mut acc = 0.0;
            for mask in 0..n_subsets {
                if mask & bi != 0 || mask & bj != 0 {
                    continue;
                }
                let s = (mask as u32).count_ones() as usize;
                acc += weights[s] * (v[mask | bi | bj] - v[mask | bi] - v[mask | bj] + v[mask]);
            }
            out.insert((i, j), acc);
        }
    }
    Some(out)
}

/// Brier score for a probabilistic forecast of a binary outcome.
/// Negatively oriented (0 is perfect), which is why the value function below
/// works with differences rather than raw scores.
pub fn brier(probability: f64, outcome: bool) -> f64 {
    let y = if outcome { 1.0 } else { 0.0 };
    let p = probability.clamp(0.0, 1.0);
    (p - y) * (p - y)
}

/// Build the positively-oriented value function `v(S) = Brier(p_∅) - Brier(p_S)`
/// from a model that maps a subset to a probability.
///
/// `p_of_subset` must return the probability the model yields when only the
/// agents in the mask contribute their claims and all others are neutralised.
/// Guarantees `v(∅) = 0` by construction, which [`exact_shapley`] relies on for
/// its efficiency statement to be interpretable as "total improvement".
pub fn brier_improvement_value_fn<P>(outcome: bool, mut p_of_subset: P) -> impl FnMut(u32) -> f64
where
    P: FnMut(u32) -> f64,
{
    let baseline = brier(p_of_subset(0), outcome);
    move |mask| baseline - brier(p_of_subset(mask), outcome)
}

/// Deterministic seed for a forecast's attribution, derived from its id.
///
/// Attribution must be reproducible: re-running it months later has to yield
/// byte-identical credit, or a stored `φ` cannot be audited against a fresh
/// computation. That requires a seed that is a pure function of the forecast.
///
/// This is FNV-1a rather than `DefaultHasher` deliberately. `std`'s hasher is
/// explicitly not stable across Rust releases or process runs, so using it here
/// would silently invalidate every historical attribution on a toolchain bump.
/// FNV-1a is fully specified, so this function is frozen behaviour: changing it
/// is a breaking change that requires recomputing stored attributions.
///
/// The result is masked to 63 bits so it always fits a signed `BIGINT`. Postgres
/// has no unsigned integer type, so a full `u64` seed would have to be stored as
/// a wrapped negative number — which round-trips correctly but makes the stored
/// value unreadable and overflows any hand-written query against it. Sacrificing
/// one bit costs nothing for seeding an RNG and keeps the column honest.
pub fn stable_seed(forecast_id: &str) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut h = FNV_OFFSET;
    for b in forecast_id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    // Clear the sign bit: i64::MAX-safe, so `as i64` is lossless and positive.
    h & 0x7fff_ffff_ffff_ffff
}

/// Percentile confidence interval for a mean, by **cluster** bootstrap.
///
/// Resamples whole clusters with replacement rather than individual
/// observations, because forecasts are not independent draws. The 48 World Cup
/// tournament-winner forecasts are one tournament with a shared outcome
/// structure: exactly one resolves YES, and every team's result is constrained
/// by every other's. Treating them as 48 independent observations would report
/// an interval roughly √48 too narrow, which is precisely the kind of false
/// precision that lets an optimiser act on noise.
///
/// `values[i]` belongs to `clusters[i]`. Returns `(low, high)` at the given
/// two-sided `alpha` (e.g. `0.10` for a 90% interval).
///
/// Returns `None` when there are fewer than [`MIN_BOOTSTRAP_CLUSTERS`] distinct
/// clusters. This is the important case and it is deliberately not fudged: with
/// one cluster there is no replication to resample, so the data contain **no
/// information at all** about between-cluster variability. Reporting a tight
/// interval computed from within-cluster spread would be actively misleading.
/// Callers should surface "undefined" rather than substituting a naive interval.
///
/// Deterministic given `seed`, so the same inputs always yield the same interval
/// and an API response is reproducible.
pub fn cluster_bootstrap_ci(
    values: &[f64],
    clusters: &[String],
    n_resamples: usize,
    seed: u64,
    alpha: f64,
) -> Option<(f64, f64)> {
    if values.len() != clusters.len() || values.is_empty() || n_resamples == 0 {
        return None;
    }

    // Group value indices by cluster.
    let mut groups: std::collections::BTreeMap<&str, Vec<f64>> = Default::default();
    for (v, c) in values.iter().zip(clusters.iter()) {
        groups.entry(c.as_str()).or_default().push(*v);
    }
    let grouped: Vec<&Vec<f64>> = groups.values().collect();
    if grouped.len() < MIN_BOOTSTRAP_CLUSTERS {
        return None;
    }

    // xorshift64*: tiny, deterministic, and adequate for index selection. Avoids
    // pulling an RNG dependency into what is otherwise pure arithmetic.
    let mut state = seed | 1;
    let mut next = move || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };

    let k = grouped.len();
    let mut means = Vec::with_capacity(n_resamples);
    for _ in 0..n_resamples {
        let mut sum = 0.0;
        let mut count = 0usize;
        for _ in 0..k {
            let g = grouped[(next() % k as u64) as usize];
            sum += g.iter().sum::<f64>();
            count += g.len();
        }
        if count > 0 {
            means.push(sum / count as f64);
        }
    }
    if means.is_empty() {
        return None;
    }
    means.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let lo_idx = (((alpha / 2.0) * means.len() as f64).floor() as usize).min(means.len() - 1);
    let hi_idx = ((((1.0 - alpha / 2.0) * means.len() as f64).ceil() as usize).max(1) - 1)
        .min(means.len() - 1);
    Some((means[lo_idx], means[hi_idx]))
}

/// Below this many distinct clusters, a bootstrap interval carries no
/// information about between-cluster variability and is not reported.
pub const MIN_BOOTSTRAP_CLUSTERS: usize = 3;

/// `k!` as an `f64`. Only ever called with `k <= MAX_EXACT_PLAYERS`, where the
/// value is exactly representable (`16! ≈ 2.09e13`, far inside f64's 2^53
/// integer range), so the Shapley weights carry no rounding error.
fn factorial(k: usize) -> f64 {
    (1..=k).map(|x| x as f64).product::<f64>().max(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-12;

    /// Efficiency is the property that makes these numbers accounting rather
    /// than a heuristic: per-agent credit must sum exactly to the team's total
    /// improvement. Checked on a deliberately messy non-linear game with
    /// strong interactions, where a naive split would not sum correctly.
    #[test]
    fn efficiency_holds_on_a_game_with_interactions() {
        // v(S) rewards the pair {0,1} superadditively and penalises adding 2.
        let v = |mask: u32| -> f64 {
            let has = |i: usize| mask & (1 << i) != 0;
            let mut x = 0.0;
            if has(0) {
                x += 1.0;
            }
            if has(1) {
                x += 2.0;
            }
            if has(0) && has(1) {
                x += 5.0; // synergy
            }
            if has(2) {
                x -= 0.5; // actively harmful
            }
            x
        };
        let att = exact_shapley(3, v).unwrap();
        assert!(
            att.efficiency_residual() < EPS,
            "residual {} — credits must sum to v(N)-v(∅)",
            att.efficiency_residual()
        );
        assert!((att.total - 7.5).abs() < EPS, "total {}", att.total);
        // The harmful player must carry negative credit, not be flattered by
        // the team's overall success.
        assert!(att.values[2] < 0.0, "{:?}", att.values);
    }

    /// Symmetry: two players with identical marginal behaviour must receive
    /// identical credit. This is what stops label or ordering artefacts from
    /// leaking into agent scores.
    #[test]
    fn symmetric_players_receive_equal_credit() {
        let v = |mask: u32| -> f64 { (mask.count_ones() as f64).powi(2) };
        let att = exact_shapley(4, v).unwrap();
        for w in att.values.windows(2) {
            assert!((w[0] - w[1]).abs() < EPS, "{:?}", att.values);
        }
        assert!(att.efficiency_residual() < EPS);
    }

    /// Dummy: a player who never changes any coalition's value gets exactly
    /// zero. A neutral specialist must not accumulate credit just by being on
    /// the roster — this is the axiom that kills "everyone on the team scores
    /// the same".
    #[test]
    fn dummy_player_receives_exactly_zero() {
        // Player 2 is ignored entirely by v.
        let v = |mask: u32| -> f64 {
            let has = |i: usize| mask & (1 << i) != 0;
            (if has(0) { 3.0 } else { 0.0 }) + (if has(1) { 4.0 } else { 0.0 })
        };
        let att = exact_shapley(3, v).unwrap();
        assert!(att.values[2].abs() < EPS, "{:?}", att.values);
        assert!((att.values[0] - 3.0).abs() < EPS);
        assert!((att.values[1] - 4.0).abs() < EPS);
    }

    /// Additivity over games — what licenses averaging per-forecast credit
    /// across forecasts.
    #[test]
    fn additive_across_games() {
        let v1 = |m: u32| -> f64 { (m.count_ones() as f64) * 2.0 };
        let v2 = |m: u32| -> f64 {
            let has = |i: usize| m & (1 << i) != 0;
            if has(0) && has(1) {
                6.0
            } else {
                0.0
            }
        };
        let a1 = exact_shapley(3, v1).unwrap();
        let a2 = exact_shapley(3, v2).unwrap();
        let sum = exact_shapley(3, |m| v1(m) + v2(m)).unwrap();
        for i in 0..3 {
            assert!(
                (a1.values[i] + a2.values[i] - sum.values[i]).abs() < EPS,
                "player {}: {} + {} != {}",
                i,
                a1.values[i],
                a2.values[i],
                sum.values[i]
            );
        }
    }

    /// A purely additive game must give each player exactly its own marginal
    /// effect — no interaction credit invented out of nowhere.
    #[test]
    fn additive_game_recovers_exact_marginals() {
        let contrib = [0.5, -0.25, 1.75, 0.0];
        let v = move |mask: u32| -> f64 {
            (0..4)
                .filter(|i| mask & (1 << i) != 0)
                .map(|i| contrib[i])
                .sum()
        };
        let att = exact_shapley(4, v).unwrap();
        for (i, c) in contrib.iter().enumerate() {
            assert!((att.values[i] - c).abs() < EPS, "{:?}", att.values);
        }
    }

    /// The interaction index must separate synergy from redundancy by sign.
    /// This is the signal Loop 4 acts on when proposing team changes.
    #[test]
    fn interactions_distinguish_synergy_from_redundancy() {
        // {0,1} synergistic (+4). {0,2} redundant: 2 alone is worth 3, but
        // adds nothing once 0 is present.
        let v = |mask: u32| -> f64 {
            let has = |i: usize| mask & (1 << i) != 0;
            let mut x = 0.0;
            if has(0) {
                x += 3.0;
            }
            if has(1) {
                x += 1.0;
            }
            if has(0) && has(1) {
                x += 4.0;
            }
            if has(2) && !has(0) {
                x += 3.0;
            }
            x
        };
        let ix = pairwise_interactions(3, v).unwrap();
        assert!(ix[&(0, 1)] > 0.5, "expected synergy, got {}", ix[&(0, 1)]);
        assert!(
            ix[&(0, 2)] < -0.5,
            "expected redundancy, got {}",
            ix[&(0, 2)]
        );
    }

    /// Two agents whose claims are always identical are perfectly redundant.
    /// This is the WC failure mode expressed as a game: Shapley splits the
    /// shared credit instead of double-counting it, and the interaction index
    /// flags the pair as substitutable.
    #[test]
    fn duplicate_agents_split_credit_and_read_as_redundant() {
        // Value depends only on whether AT LEAST ONE of {0,1} is present.
        let v = |mask: u32| -> f64 {
            let has = |i: usize| mask & (1 << i) != 0;
            if has(0) || has(1) {
                10.0
            } else {
                0.0
            }
        };
        let att = exact_shapley(2, v).unwrap();
        assert!((att.values[0] - 5.0).abs() < EPS, "{:?}", att.values);
        assert!((att.values[1] - 5.0).abs() < EPS, "{:?}", att.values);
        assert!(att.efficiency_residual() < EPS);

        let ix = pairwise_interactions(2, v).unwrap();
        assert!(ix[&(0, 1)] < 0.0, "redundant pair must read negative");
    }

    // ── The value function ────────────────────────────────────────────────

    #[test]
    fn brier_is_zero_when_certain_and_right() {
        assert!(brier(1.0, true).abs() < EPS);
        assert!(brier(0.0, false).abs() < EPS);
        assert!((brier(0.0, true) - 1.0).abs() < EPS);
        // Clamped, so an out-of-range probability cannot manufacture a score
        // outside [0,1].
        assert!((brier(1.7, false) - 1.0).abs() < EPS);
    }

    /// `v(∅) = 0` by construction, so efficiency reads as "total improvement
    /// over forecasting without any agent".
    #[test]
    fn value_fn_is_zero_on_empty_set() {
        let mut v = brier_improvement_value_fn(true, |mask| if mask == 0 { 0.5 } else { 0.9 });
        assert!(v(0).abs() < EPS);
    }

    /// End-to-end on a realistic shape: a 4-specialist composition where each
    /// agent nudges the log-odds multiplicatively and one agent pushes the
    /// wrong way.
    ///
    /// The point of the two scenarios is that per-agent credit is signed
    /// correctly *independently of whether the team succeeded*. This is what a
    /// team-level Brier can never do: in the first scenario the composition as
    /// a whole makes the forecast worse, yet three of its four members still
    /// helped and must be credited for it; in the second the team improves
    /// while one member is still penalised for dragging against the outcome.
    /// Reading a team score as a per-agent score gets both cases exactly wrong.
    #[test]
    fn credit_sign_is_independent_of_team_outcome() {
        let outcome = true;
        let base_odds = 0.4f64 / 0.6;

        let run = |mult: [f64; 4]| {
            let p_of = move |mask: u32| -> f64 {
                let mut odds = base_odds;
                for (i, m) in mult.iter().enumerate() {
                    if mask & (1 << i) != 0 {
                        odds *= m;
                    }
                }
                odds / (1.0 + odds)
            };
            exact_shapley(4, brier_improvement_value_fn(outcome, p_of)).unwrap()
        };

        // Scenario 1 — agent 3 is badly wrong and sinks the team below the
        // no-agent baseline.
        let bad_team = run([1.30, 1.10, 1.05, 0.60]);
        assert!(
            bad_team.efficiency_residual() < 1e-9,
            "residual {}",
            bad_team.efficiency_residual()
        );
        assert!(
            bad_team.total < 0.0,
            "this scenario is meant to net-harm: total {}",
            bad_team.total
        );
        for i in 0..3 {
            assert!(
                bad_team.values[i] > 0.0,
                "agent {} helped and must be credited even though the team lost: {:?}",
                i,
                bad_team.values
            );
        }
        assert!(bad_team.values[3] < 0.0, "{:?}", bad_team.values);

        // Scenario 2 — same roster, agent 3 only mildly wrong: the team now
        // improves overall, but agent 3 is still penalised.
        let good_team = run([1.30, 1.10, 1.05, 0.95]);
        assert!(good_team.efficiency_residual() < 1e-9);
        assert!(
            good_team.total > 0.0,
            "expected net improvement: total {}",
            good_team.total
        );
        assert!(
            good_team.values[3] < 0.0,
            "agent 3 still drags against the outcome: {:?}",
            good_team.values
        );
        // The strongest correct agent should out-earn the weaker correct ones.
        assert!(good_team.values[0] > good_team.values[1]);
        assert!(good_team.values[1] > good_team.values[2]);
    }

    // ── Guards ────────────────────────────────────────────────────────────

    /// The seed must be a pure, stable function of the forecast id — same input
    /// always the same output, different inputs different outputs. If this ever
    /// changes, every stored attribution silently stops being reproducible.
    #[test]
    fn stable_seed_is_deterministic_and_distinguishing() {
        let a = stable_seed("11111111-1111-1111-1111-111111111101");
        assert_eq!(a, stable_seed("11111111-1111-1111-1111-111111111101"));
        assert_ne!(a, stable_seed("11111111-1111-1111-1111-111111111102"));
        assert_ne!(stable_seed(""), stable_seed("a"));
        // Frozen value: a change here invalidates every stored attribution, so
        // it must be a deliberate, visible edit rather than an accident.
        // Cross-checked against an independent FNV-1a implementation, then
        // masked to 63 bits.
        assert_eq!(stable_seed("fermi"), 692_178_715_948_733_132);

        // Every seed must survive the round-trip through a signed BIGINT
        // losslessly and stay positive, or the stored value misrepresents the
        // seed that was actually used.
        for id in ["fermi", "", "fc-1", "11111111-1111-1111-1111-111111111101"] {
            let s = stable_seed(id);
            assert!(s <= i64::MAX as u64, "{id} seed {s} overflows i64");
            assert_eq!(s as i64 as u64, s, "{id} seed does not round-trip");
        }
    }

    // ── Cluster bootstrap ──────────────────────────────────────────

    /// The World Cup case: every observation from one tournament. There is no
    /// replication to resample, so no interval can be estimated — and the
    /// honest answer is "undefined", not a tight interval computed from
    /// within-cluster spread.
    #[test]
    fn single_cluster_yields_no_interval() {
        let values = vec![0.10, 0.12, 0.09, 0.11, 0.13, 0.08];
        let clusters = vec!["wc2026".to_string(); 6];
        assert_eq!(cluster_bootstrap_ci(&values, &clusters, 500, 1, 0.10), None);
    }

    /// Two clusters is still too few to characterise between-cluster spread.
    #[test]
    fn two_clusters_yields_no_interval() {
        let values = vec![0.1, 0.2, 0.3, 0.4];
        let clusters = vec!["a".into(), "a".into(), "b".into(), "b".into()];
        assert!(cluster_bootstrap_ci(&values, &clusters, 500, 1, 0.10).is_none());
    }

    /// With enough clusters the interval brackets the sample mean and is
    /// deterministic for a given seed.
    #[test]
    fn interval_brackets_mean_and_is_reproducible() {
        let values: Vec<f64> = (0..40).map(|i| (i % 7) as f64 * 0.01).collect();
        let clusters: Vec<String> = (0..40).map(|i| format!("c{}", i % 8)).collect();
        let mean = values.iter().sum::<f64>() / values.len() as f64;

        let a = cluster_bootstrap_ci(&values, &clusters, 2000, 42, 0.10).unwrap();
        let b = cluster_bootstrap_ci(&values, &clusters, 2000, 42, 0.10).unwrap();
        assert_eq!(a, b, "same seed must give the same interval");
        assert!(
            a.0 <= mean && mean <= a.1,
            "{:?} should bracket {}",
            a,
            mean
        );
        assert!(a.0 < a.1);
    }

    /// Ignoring clustering understates uncertainty. Same values, but treating
    /// each observation as its own cluster produces a narrower interval than
    /// grouping them — which is exactly the false precision this function
    /// exists to prevent.
    #[test]
    fn clustering_widens_the_interval_versus_ignoring_it() {
        // Strong between-cluster differences, little within-cluster spread.
        let mut values = Vec::new();
        let mut clustered = Vec::new();
        let mut unclustered = Vec::new();
        for c in 0..6 {
            for i in 0..8 {
                values.push(c as f64 * 0.5 + i as f64 * 0.001);
                clustered.push(format!("c{c}"));
                unclustered.push(format!("c{c}_{i}"));
            }
        }
        let wide = cluster_bootstrap_ci(&values, &clustered, 3000, 7, 0.10).unwrap();
        let narrow = cluster_bootstrap_ci(&values, &unclustered, 3000, 7, 0.10).unwrap();
        assert!(
            (wide.1 - wide.0) > (narrow.1 - narrow.0),
            "clustered {:?} must be wider than unclustered {:?}",
            wide,
            narrow
        );
    }

    #[test]
    fn bootstrap_rejects_malformed_input() {
        assert!(cluster_bootstrap_ci(&[], &[], 100, 1, 0.1).is_none());
        assert!(cluster_bootstrap_ci(&[1.0], &["a".into(), "b".into()], 100, 1, 0.1).is_none());
        let v = vec![1.0, 2.0, 3.0];
        let c = vec!["a".to_string(), "b".into(), "c".into()];
        assert!(cluster_bootstrap_ci(&v, &c, 0, 1, 0.1).is_none());
    }

    #[test]
    fn rejects_degenerate_and_oversized_player_counts() {
        assert!(exact_shapley(0, |_| 0.0).is_none());
        assert!(exact_shapley(MAX_EXACT_PLAYERS + 1, |_| 0.0).is_none());
        assert!(pairwise_interactions(1, |_| 0.0).is_none());
        assert!(pairwise_interactions(MAX_EXACT_PLAYERS + 1, |_| 0.0).is_none());
    }

    /// The value function must be called exactly `2^n` times, not `O(n·2^n)`.
    /// Each call may be a full Monte Carlo re-run, so memoisation is a
    /// correctness-adjacent performance contract worth pinning.
    #[test]
    fn value_fn_evaluated_once_per_subset() {
        let mut calls = 0usize;
        let _ = exact_shapley(6, |_| {
            calls += 1;
            1.0
        })
        .unwrap();
        assert_eq!(calls, 64, "expected 2^6 evaluations, got {}", calls);
    }
}
