//! Sobol index layout — turning variance decomposition into something
//! an operator can act on.
//!
//! # Why this exists
//!
//! The console currently renders sensitivity as a single horizontal bar
//! per driver, whose length is the **total-order** index (or, before a
//! sim has run, the driver's raw `p95 − p5` spread — a completely
//! different quantity wearing the same clothes). Two problems:
//!
//! 1. **First-order is thrown away.** Total-order alone can't tell you
//!    *why* a driver matters. A driver with `S1 = 0.05, ST = 0.40` is
//!    not an important driver — it's an important *interaction*, and
//!    the correct response is to go find the driver it's coupled with,
//!    not to go tighten its prior. The current chart makes those two
//!    situations look identical.
//! 2. **No memory.** Sensitivity is recomputed each sim and overwrites
//!    the last. But "which driver dominates" *moving* is the single
//!    most informative signal in a live forecast — it means the model's
//!    structure changed, not just its numbers.
//!
//! This module computes the layout for both: the interaction split, and
//! the rank/magnitude delta against a previous run.
//!
//! GPUI-free; lives in the lib target and is tested.

/// One driver's variance contribution, decomposed.
#[derive(Debug, Clone, PartialEq)]
pub struct SobolBar {
    pub name: String,
    /// Variance explained by this driver alone, 0..1.
    pub first_order: f64,
    /// Variance explained by this driver including all its
    /// interactions, 0..1. Always `>= first_order` in theory; noisy
    /// estimators can violate that, so we clamp (see [`SobolBar::new`]).
    pub total_order: f64,
    /// `total_order - first_order`: the part of this driver's influence
    /// that only exists in combination with others.
    pub interaction: f64,
    /// Rank change vs the previous run — `Some(prev_rank - new_rank)`,
    /// so positive means "climbed the leaderboard".
    pub rank_delta: Option<i32>,
    /// Change in total-order index vs the previous run.
    pub total_delta: Option<f64>,
}

impl SobolBar {
    pub fn new(name: impl Into<String>, first_order: f64, total_order: f64) -> Self {
        let first = first_order.clamp(0.0, 1.0);
        // Monte Carlo Sobol estimators are noisy and can return
        // ST < S1 on small sample counts. Rendering a negative
        // interaction band would be nonsense ink, so clamp the total up
        // to the first-order rather than drawing a backwards bar.
        let total = total_order.clamp(first, 1.0);
        Self {
            name: name.into(),
            first_order: first,
            total_order: total,
            interaction: total - first,
            rank_delta: None,
            total_delta: None,
        }
    }

    /// True when most of this driver's influence is interactive. These
    /// are the drivers where tightening the prior in isolation will not
    /// move the forecast — the actionable insight the single-bar chart
    /// hides.
    pub fn is_interaction_dominated(&self) -> bool {
        self.total_order > 0.02 && self.interaction > self.first_order
    }
}

/// What the sensitivity profile as a whole is telling the operator.
///
/// This is the actionable summary, and it deliberately lives here
/// rather than in the renderer: "should I go tighten a prior?" is a
/// claim about the model, and getting it wrong sends someone off to do
/// useless work. It needs to be testable.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// No indices yet.
    NoData,
    /// One driver clearly leads *and* the model is mostly additive, so
    /// reducing uncertainty on it should actually move the forecast.
    Dominant { name: String, share: f64 },
    /// A driver leads, but most variance is interactive — tightening it
    /// in isolation will disappoint. Go find what it's coupled with.
    DominantButCoupled { name: String, additive: f64 },
    /// Influence is spread and largely interactive. There is no single
    /// prior to tighten; the structure is the problem.
    SpreadAndCoupled { additive: f64 },
    /// Influence is spread but additive — several independent drivers
    /// each matter a little.
    Spread,
}

/// Below this, the model's variance is mostly in couplings and
/// single-driver advice stops being sound.
const ADDITIVE_FLOOR: f64 = 0.6;

/// A full sensitivity readout, sorted by descending total-order index.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SobolLayout {
    pub bars: Vec<SobolBar>,
    /// Sum of first-order indices. Well below 1.0 means the model's
    /// variance lives mostly in interactions — a structural fact about
    /// the model worth surfacing as a single headline number.
    pub additive_fraction: f64,
}

impl SobolLayout {
    /// Build from `(name, first_order, total_order)` triples.
    pub fn new<I, S>(entries: I) -> Self
    where
        I: IntoIterator<Item = (S, f64, f64)>,
        S: Into<String>,
    {
        let mut bars: Vec<SobolBar> = entries
            .into_iter()
            .map(|(n, s1, st)| SobolBar::new(n, s1, st))
            .collect();
        sort_by_total(&mut bars);
        let additive_fraction = bars
            .iter()
            .map(|b| b.first_order)
            .sum::<f64>()
            .clamp(0.0, 1.0);
        Self {
            bars,
            additive_fraction,
        }
    }

    /// Annotate each bar with how it moved since `previous`. Drivers
    /// absent from `previous` get `None` deltas (newly added drivers
    /// shouldn't render a spurious "+3 ranks" badge).
    pub fn diff_against(mut self, previous: &SobolLayout) -> Self {
        for (new_rank, bar) in self.bars.iter_mut().enumerate() {
            if let Some(prev_rank) = previous.bars.iter().position(|p| p.name == bar.name) {
                bar.rank_delta = Some(prev_rank as i32 - new_rank as i32);
                bar.total_delta = Some(bar.total_order - previous.bars[prev_rank].total_order);
            }
        }
        self
    }

    /// The driver the operator should look at first, if any is clearly
    /// dominant. Returns `None` when influence is spread evenly — in
    /// which case "reduce uncertainty on X" is not sound advice.
    pub fn dominant(&self) -> Option<&SobolBar> {
        let first = self.bars.first()?;
        let second_total = self.bars.get(1).map(|b| b.total_order).unwrap_or(0.0);
        (first.total_order > 0.15 && first.total_order > second_total * 1.5).then_some(first)
    }

    /// Drivers contributing so little variance that they're noise in
    /// the display. Worth collapsing behind a "+N negligible" chip
    /// rather than drawing ten invisible bars.
    pub fn negligible(&self, cutoff: f64) -> Vec<&SobolBar> {
        self.bars
            .iter()
            .filter(|b| b.total_order < cutoff)
            .collect()
    }

    /// Summarise what the whole profile implies for the operator's
    /// next move.
    pub fn verdict(&self) -> Verdict {
        if self.bars.is_empty() {
            return Verdict::NoData;
        }
        let additive = self.additive_fraction;
        match self.dominant() {
            Some(top) if additive >= ADDITIVE_FLOOR => Verdict::Dominant {
                name: top.name.clone(),
                share: top.total_order,
            },
            Some(top) => Verdict::DominantButCoupled {
                name: top.name.clone(),
                additive,
            },
            None if additive < ADDITIVE_FLOOR => Verdict::SpreadAndCoupled { additive },
            None => Verdict::Spread,
        }
    }

    /// The largest total-order index, used as the bar-length
    /// denominator. Scaling to the max rather than to 1.0 keeps a
    /// flat-but-real sensitivity profile readable instead of rendering
    /// as eight identical slivers.
    pub fn max_total(&self) -> f64 {
        self.bars
            .iter()
            .map(|b| b.total_order)
            .fold(0.0_f64, f64::max)
    }
}

/// Descending by total-order, ties broken by name so the ordering is
/// stable across runs. Stability matters: a bar chart that reshuffles
/// on every re-render destroys the operator's spatial memory.
fn sort_by_total(bars: &mut [SobolBar]) {
    bars.sort_by(|a, b| {
        b.total_order
            .partial_cmp(&a.total_order)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn bars_sort_by_total_order_descending() {
        let l = SobolLayout::new([("a", 0.1, 0.2), ("b", 0.5, 0.6), ("c", 0.05, 0.4)]);
        let names: Vec<&str> = l.bars.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["b", "c", "a"]);
    }

    #[test]
    fn ties_break_by_name_for_stable_ordering() {
        let a = SobolLayout::new([("zeta", 0.1, 0.3), ("alpha", 0.1, 0.3)]);
        let b = SobolLayout::new([("alpha", 0.1, 0.3), ("zeta", 0.1, 0.3)]);
        assert_eq!(a.bars[0].name, "alpha");
        assert_eq!(a.bars, b.bars, "ordering must not depend on input order");
    }

    #[test]
    fn interaction_is_the_gap_between_total_and_first() {
        let b = SobolBar::new("d", 0.05, 0.40);
        assert!(close(b.interaction, 0.35));
        assert!(b.is_interaction_dominated());

        let additive = SobolBar::new("e", 0.38, 0.40);
        assert!(!additive.is_interaction_dominated());
    }

    #[test]
    fn noisy_estimates_never_render_a_backwards_bar() {
        // ST < S1 happens with few Monte Carlo draws.
        let b = SobolBar::new("noisy", 0.30, 0.22);
        assert!(b.interaction >= 0.0);
        assert!(b.total_order >= b.first_order);
        assert!(close(b.total_order, 0.30));
    }

    #[test]
    fn indices_are_clamped_into_range() {
        let b = SobolBar::new("wild", -0.5, 3.0);
        assert!(close(b.first_order, 0.0));
        assert!(close(b.total_order, 1.0));
    }

    #[test]
    fn tiny_totals_are_not_flagged_as_interaction_dominated() {
        // 0.001 vs 0.002 is estimator noise, not an insight.
        let b = SobolBar::new("dust", 0.001, 0.002);
        assert!(!b.is_interaction_dominated());
    }

    #[test]
    fn additive_fraction_reveals_a_mostly_interactive_model() {
        let interactive = SobolLayout::new([("a", 0.05, 0.5), ("b", 0.05, 0.5)]);
        assert!(close(interactive.additive_fraction, 0.10));

        let additive = SobolLayout::new([("a", 0.5, 0.52), ("b", 0.45, 0.47)]);
        assert!(additive.additive_fraction > 0.9);
    }

    #[test]
    fn diff_reports_rank_climbs_and_magnitude_change() {
        let before = SobolLayout::new([("a", 0.4, 0.5), ("b", 0.2, 0.3)]);
        let after = SobolLayout::new([("a", 0.1, 0.2), ("b", 0.5, 0.7)]).diff_against(&before);

        let b = after.bars.iter().find(|x| x.name == "b").unwrap();
        assert_eq!(b.rank_delta, Some(1), "b moved from rank 1 to rank 0");
        assert!(close(b.total_delta.unwrap(), 0.4));

        let a = after.bars.iter().find(|x| x.name == "a").unwrap();
        assert_eq!(a.rank_delta, Some(-1));
        assert!(close(a.total_delta.unwrap(), -0.3));
    }

    #[test]
    fn new_drivers_get_no_spurious_deltas() {
        let before = SobolLayout::new([("a", 0.4, 0.5)]);
        let after =
            SobolLayout::new([("a", 0.4, 0.5), ("brand_new", 0.3, 0.6)]).diff_against(&before);
        let n = after.bars.iter().find(|x| x.name == "brand_new").unwrap();
        assert_eq!(n.rank_delta, None);
        assert_eq!(n.total_delta, None);
    }

    #[test]
    fn dominant_requires_a_real_gap() {
        let clear = SobolLayout::new([("a", 0.6, 0.7), ("b", 0.1, 0.2)]);
        assert_eq!(clear.dominant().map(|b| b.name.as_str()), Some("a"));

        let flat = SobolLayout::new([("a", 0.3, 0.34), ("b", 0.3, 0.33)]);
        assert!(flat.dominant().is_none(), "no driver truly dominates here");

        let tiny = SobolLayout::new([("a", 0.01, 0.02)]);
        assert!(tiny.dominant().is_none());

        assert!(SobolLayout::default().dominant().is_none());
    }

    #[test]
    fn negligible_and_max_total_support_display_collapsing() {
        let l = SobolLayout::new([("a", 0.5, 0.6), ("b", 0.001, 0.002), ("c", 0.0, 0.001)]);
        assert!(close(l.max_total(), 0.6));
        let n = l.negligible(0.01);
        assert_eq!(n.len(), 2);
        assert!(n.iter().all(|b| b.name != "a"));
    }

    #[test]
    fn empty_layout_is_harmless() {
        let l = SobolLayout::new(Vec::<(String, f64, f64)>::new());
        assert!(l.bars.is_empty());
        assert!(close(l.max_total(), 0.0));
        assert!(close(l.additive_fraction, 0.0));
        assert_eq!(l.verdict(), Verdict::NoData);
    }

    #[test]
    fn verdict_recommends_tightening_only_when_that_would_work() {
        // One driver clearly ahead, model mostly additive — the one
        // case where "go reduce uncertainty on X" is sound advice.
        let l = SobolLayout::new([("a", 0.70, 0.75), ("b", 0.10, 0.12)]);
        match l.verdict() {
            Verdict::Dominant { name, share } => {
                assert_eq!(name, "a");
                assert!(close(share, 0.75));
            }
            other => panic!("expected Dominant, got {other:?}"),
        }
    }

    #[test]
    fn a_leader_whose_influence_is_interactive_is_flagged_as_coupled() {
        // 'a' leads on total-order but almost all of it is interaction,
        // so tightening 'a' alone will disappoint. This is the case the
        // old single-bar chart could not distinguish from the one above.
        let l = SobolLayout::new([("a", 0.06, 0.60), ("b", 0.05, 0.20)]);
        match l.verdict() {
            Verdict::DominantButCoupled { name, additive } => {
                assert_eq!(name, "a");
                assert!(additive < 0.6, "additive was {additive}");
            }
            other => panic!("expected DominantButCoupled, got {other:?}"),
        }
    }

    #[test]
    fn verdict_distinguishes_spread_additive_from_spread_coupled() {
        // Flat and additive: several independent drivers each matter.
        let additive = SobolLayout::new([("a", 0.34, 0.36), ("b", 0.33, 0.35), ("c", 0.30, 0.33)]);
        assert_eq!(additive.verdict(), Verdict::Spread);

        // Flat and coupled: the structure is the problem, not any prior.
        let coupled = SobolLayout::new([("a", 0.10, 0.50), ("b", 0.10, 0.48), ("c", 0.08, 0.45)]);
        match coupled.verdict() {
            Verdict::SpreadAndCoupled { additive } => {
                assert!(additive < 0.6, "additive was {additive}")
            }
            other => panic!("expected SpreadAndCoupled, got {other:?}"),
        }
    }

    #[test]
    fn a_single_negligible_driver_does_not_earn_a_dominance_claim() {
        // Everything is noise; promising that tightening 'a' will move
        // the forecast would be false.
        let l = SobolLayout::new([("a", 0.01, 0.02)]);
        assert!(matches!(
            l.verdict(),
            Verdict::Spread | Verdict::SpreadAndCoupled { .. }
        ));
        assert!(l.dominant().is_none());
    }
}
