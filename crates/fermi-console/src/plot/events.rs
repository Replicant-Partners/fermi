//! Correlating events with the trajectory they explain.
//!
//! # The problem this solves
//!
//! The trajectory chart draws a worm (the forecast rate over time) and
//! scatters dots on it (agent runs, BayesOps fits, market ticks, rate
//! revisions). Today those dots are placed at `rate_at(ts)` — the
//! nearest point on the worm — which makes them *coincident* with the
//! trajectory but says nothing about their relationship to it. Every
//! dot looks equally consequential. A market tick that moved nothing
//! and an agent run that swung the forecast eleven points render
//! identically.
//!
//! That's the gap in "comparison of indices over time with key event
//! data overlays": the overlay is decorative rather than explanatory.
//! An event marker's job is to answer *what did this do?*, and the
//! answer is a **delta** — the rate immediately before versus
//! immediately after.
//!
//! This module computes that, plus the label-packing needed to show it
//! without the annotations colliding into mush.
//!
//! GPUI-free; lives in the lib target and is tested.

/// A time-ordered numeric series — the trajectory worm.
pub type Series = [(f64, f64)];

/// Linear interpolation of `series` at time `t`, clamped to the
/// endpoints. Returns `None` for an empty series.
pub fn interpolate(series: &Series, t: f64) -> Option<f64> {
    if series.is_empty() {
        return None;
    }
    let first = series[0];
    let last = series[series.len() - 1];
    if t <= first.0 {
        return Some(first.1);
    }
    if t >= last.0 {
        return Some(last.1);
    }
    for w in series.windows(2) {
        if t >= w[0].0 && t <= w[1].0 {
            let dt = w[1].0 - w[0].0;
            if dt.abs() < 1e-9 {
                return Some(w[0].1);
            }
            let frac = (t - w[0].0) / dt;
            return Some(w[0].1 + frac * (w[1].1 - w[0].1));
        }
    }
    None
}

/// Interpolate, but only where the series actually has coverage.
///
/// [`interpolate`] clamps to the endpoints, which is right for drawing
/// a line and wrong for answering a question. Asking "what was the
/// crowd price when I saved v1?" of a series that only starts a week
/// later must answer *don't know*, not today's price wearing last
/// week's date. Clamped extrapolation is how a chart ends up inventing
/// history.
pub fn sample_within(series: &Series, t: f64) -> Option<f64> {
    let first = series.first()?;
    let last = series.last()?;
    if t < first.0 || t > last.0 {
        return None;
    }
    interpolate(series, t)
}

/// An event once we've worked out what it did to the trajectory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Correlated {
    /// Index into the caller's original event list, so the chart and
    /// the event list can cross-highlight without re-sorting anything.
    pub index: usize,
    pub t: f64,
    /// Where to anchor the marker on the worm.
    pub y: f64,
    /// Rate immediately before the event, if the series covers it.
    pub before: Option<f64>,
    /// Rate immediately after.
    pub after: Option<f64>,
    /// `after - before`. `None` when the series doesn't straddle the
    /// event (e.g. an event that predates the first recorded rate).
    pub delta: Option<f64>,
    /// Which stacking lane this event's label occupies.
    pub lane: usize,
}

impl Correlated {
    /// Whether this event actually moved the forecast by more than
    /// `threshold`. Events that didn't should render quietly — they're
    /// context, not causes.
    pub fn is_consequential(&self, threshold: f64) -> bool {
        self.delta.map(|d| d.abs() >= threshold).unwrap_or(false)
    }
}

/// Attach before/after/delta and lane assignments to a set of events.
///
/// * `series` — the trajectory, ascending in time.
/// * `event_times` — event timestamps, in the caller's original order.
/// * `explicit_y` — optional per-event y override (a rate revision
///   carries its own resulting probability; an agent run doesn't).
/// * `window` — how far either side of the event to sample for the
///   before/after readings. Sampling *at* the event time is useless
///   because that's exactly where the step change lands.
/// * `min_px_spacing` / `px_per_unit` — label packing parameters; see
///   [`pack_lanes`].
pub fn correlate(
    series: &Series,
    event_times: &[f64],
    explicit_y: &[Option<f64>],
    window: f64,
    min_px_spacing: f64,
    px_per_unit: f64,
    lanes: usize,
) -> Vec<Correlated> {
    // Sort a view of the events by time for lane packing, but keep the
    // original index so callers can map back to their own list.
    let mut order: Vec<usize> = (0..event_times.len()).collect();
    order.sort_by(|a, b| {
        event_times[*a]
            .partial_cmp(&event_times[*b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let sorted_times: Vec<f64> = order.iter().map(|i| event_times[*i]).collect();
    let lane_of = pack_lanes(&sorted_times, min_px_spacing, px_per_unit, lanes);

    let mut out: Vec<Correlated> = Vec::with_capacity(event_times.len());
    for (slot, &idx) in order.iter().enumerate() {
        let t = event_times[idx];
        let before = interpolate(series, t - window);
        let after = interpolate(series, t + window);
        // Only report a delta when the series genuinely brackets the
        // event. Outside its span, `interpolate` clamps — which would
        // manufacture a delta of exactly zero and imply "this event did
        // nothing" when the truth is "we have no data here".
        let covered = series
            .first()
            .zip(series.last())
            .map(|(f, l)| t - window >= f.0 && t + window <= l.0)
            .unwrap_or(false);
        let delta = match (before, after, covered) {
            (Some(b), Some(a), true) => Some(a - b),
            _ => None,
        };
        let y = explicit_y
            .get(idx)
            .copied()
            .flatten()
            .or_else(|| interpolate(series, t))
            .unwrap_or(0.0);

        out.push(Correlated {
            index: idx,
            t,
            y,
            before,
            after,
            delta,
            lane: lane_of[slot],
        });
    }
    out
}

/// Assign each (time-sorted) event to a stacking lane so that labels
/// closer than `min_px_spacing` never share one.
///
/// Greedy first-fit: an event takes the lowest lane whose last
/// occupant is far enough behind it. Lanes wrap when `lanes` is
/// exhausted, which degrades to overlap rather than to dropping
/// markers — a hidden event is worse than a crowded one.
pub fn pack_lanes(
    sorted_times: &[f64],
    min_px_spacing: f64,
    px_per_unit: f64,
    lanes: usize,
) -> Vec<usize> {
    let lanes = lanes.max(1);
    let mut last_in_lane: Vec<Option<f64>> = vec![None; lanes];
    let mut out = Vec::with_capacity(sorted_times.len());

    for &t in sorted_times {
        let mut chosen = None;
        for (i, last) in last_in_lane.iter().enumerate() {
            let free = match last {
                None => true,
                Some(prev) => (t - prev).abs() * px_per_unit >= min_px_spacing,
            };
            if free {
                chosen = Some(i);
                break;
            }
        }
        // All lanes busy: fall back to the one used longest ago.
        let lane = chosen.unwrap_or_else(|| {
            last_in_lane
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    a.1.unwrap_or(f64::NEG_INFINITY)
                        .partial_cmp(&b.1.unwrap_or(f64::NEG_INFINITY))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .map(|(i, _)| i)
                .unwrap_or(0)
        });
        last_in_lane[lane] = Some(t);
        out.push(lane);
    }
    out
}

/// Find the event nearest a scrubbed time, within a pixel tolerance.
///
/// This is the chart→list half of brushing: the operator drags across
/// the trajectory and the event list scrolls to whatever they're over.
pub fn nearest_event(
    events: &[Correlated],
    t: f64,
    px_per_unit: f64,
    tolerance_px: f64,
) -> Option<&Correlated> {
    events
        .iter()
        .filter(|e| (e.t - t).abs() * px_per_unit <= tolerance_px)
        .min_by(|a, b| {
            (a.t - t)
                .abs()
                .partial_cmp(&(b.t - t).abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// Resample two series onto their shared time grid, restricted to the
/// span where **both** have coverage.
///
/// The divergence band between the model worm and the crowd worm is
/// only meaningful where both exist; extending it across a region where
/// one series is merely clamped to its endpoint draws a gap that isn't
/// there.
pub fn common_grid(a: &Series, b: &Series) -> Vec<(f64, f64, f64)> {
    if a.len() < 2 || b.len() < 2 {
        return Vec::new();
    }
    let lo = a[0].0.max(b[0].0);
    let hi = a[a.len() - 1].0.min(b[b.len() - 1].0);
    if !(hi > lo) {
        return Vec::new();
    }

    let mut ts: Vec<f64> = a
        .iter()
        .chain(b.iter())
        .map(|(t, _)| *t)
        .filter(|t| *t >= lo && *t <= hi)
        .collect();
    ts.push(lo);
    ts.push(hi);
    ts.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);

    ts.into_iter()
        .filter_map(|t| Some((t, interpolate(a, t)?, interpolate(b, t)?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// A worm that steps from 10% to 20% at t=100.
    fn stepped() -> Vec<(f64, f64)> {
        vec![(0.0, 10.0), (99.0, 10.0), (101.0, 20.0), (200.0, 20.0)]
    }

    #[test]
    fn interpolation_clamps_outside_the_span() {
        let s = stepped();
        assert!(close(interpolate(&s, -50.0).unwrap(), 10.0));
        assert!(close(interpolate(&s, 500.0).unwrap(), 20.0));
        assert!(close(interpolate(&s, 100.0).unwrap(), 15.0));
        assert!(interpolate(&[], 1.0).is_none());
    }

    #[test]
    fn sampling_refuses_to_extrapolate_beyond_the_data() {
        let s = [(100.0, 5.0), (200.0, 9.0)];
        // Inside the covered span: a real reading.
        assert!(close(sample_within(&s, 150.0).unwrap(), 7.0));
        assert!(close(sample_within(&s, 100.0).unwrap(), 5.0));
        assert!(close(sample_within(&s, 200.0).unwrap(), 9.0));
        // Outside it: don't know, rather than a clamped fabrication.
        assert_eq!(sample_within(&s, 99.0), None);
        assert_eq!(sample_within(&s, 201.0), None);
        // ...which is exactly where `interpolate` would have lied.
        assert!(close(interpolate(&s, 99.0).unwrap(), 5.0));
        assert!(close(interpolate(&s, 500.0).unwrap(), 9.0));
    }

    #[test]
    fn sampling_an_empty_series_is_unknown_not_a_panic() {
        assert_eq!(sample_within(&[], 1.0), None);
    }

    #[test]
    fn interpolation_survives_duplicate_timestamps() {
        let s = [(0.0, 1.0), (5.0, 2.0), (5.0, 9.0), (10.0, 9.0)];
        assert!(interpolate(&s, 5.0).unwrap().is_finite());
    }

    #[test]
    fn an_event_that_moved_the_rate_reports_its_delta() {
        let s = stepped();
        let c = correlate(&s, &[100.0], &[None], 10.0, 40.0, 1.0, 3);
        let e = c[0];
        assert!(close(e.before.unwrap(), 10.0));
        assert!(close(e.after.unwrap(), 20.0));
        assert!(close(e.delta.unwrap(), 10.0));
        assert!(e.is_consequential(0.5));
    }

    #[test]
    fn an_event_that_moved_nothing_is_marked_inconsequential() {
        let s = stepped();
        let c = correlate(&s, &[150.0], &[None], 10.0, 40.0, 1.0, 3);
        assert!(close(c[0].delta.unwrap(), 0.0));
        assert!(!c[0].is_consequential(0.5));
    }

    #[test]
    fn events_outside_the_series_span_report_no_delta_rather_than_zero() {
        // This is the important distinction: "we don't know" must not
        // render as "it did nothing".
        let s = stepped();
        let early = correlate(&s, &[-500.0], &[None], 10.0, 40.0, 1.0, 3);
        assert_eq!(early[0].delta, None);
        assert!(!early[0].is_consequential(0.0));

        let late = correlate(&s, &[9999.0], &[None], 10.0, 40.0, 1.0, 3);
        assert_eq!(late[0].delta, None);

        // Right at the boundary the window still overhangs.
        let edge = correlate(&s, &[5.0], &[None], 10.0, 40.0, 1.0, 3);
        assert_eq!(edge[0].delta, None);
    }

    #[test]
    fn explicit_y_wins_over_interpolation() {
        let s = stepped();
        let c = correlate(&s, &[150.0], &[Some(42.0)], 10.0, 40.0, 1.0, 3);
        assert!(close(c[0].y, 42.0));

        let d = correlate(&s, &[150.0], &[None], 10.0, 40.0, 1.0, 3);
        assert!(close(d[0].y, 20.0));
    }

    #[test]
    fn original_indices_survive_time_sorting() {
        let s = stepped();
        // Deliberately out of chronological order.
        let times = [150.0, 20.0, 90.0];
        let c = correlate(&s, &times, &[None, None, None], 5.0, 40.0, 1.0, 3);
        // Output is time-ordered...
        assert!(c[0].t < c[1].t && c[1].t < c[2].t);
        // ...but each entry still points at its source row.
        assert_eq!(c[0].index, 1);
        assert_eq!(c[1].index, 2);
        assert_eq!(c[2].index, 0);
        for e in &c {
            assert!(close(times[e.index], e.t));
        }
    }

    #[test]
    fn close_events_are_pushed_into_separate_lanes() {
        // Three events 5 units apart at 1px/unit = 5px, well under the
        // 40px minimum, so all three need their own lane.
        let lanes = pack_lanes(&[0.0, 5.0, 10.0], 40.0, 1.0, 3);
        assert_eq!(lanes, vec![0, 1, 2]);
    }

    #[test]
    fn well_spread_events_all_share_lane_zero() {
        let lanes = pack_lanes(&[0.0, 100.0, 200.0], 40.0, 1.0, 3);
        assert_eq!(lanes, vec![0, 0, 0]);
    }

    #[test]
    fn lane_exhaustion_overlaps_rather_than_dropping_events() {
        let times: Vec<f64> = (0..10).map(|i| i as f64).collect();
        let lanes = pack_lanes(&times, 40.0, 1.0, 2);
        assert_eq!(lanes.len(), 10, "no event may be dropped");
        assert!(lanes.iter().all(|l| *l < 2));
    }

    #[test]
    fn lane_packing_respects_the_pixel_scale_not_the_time_scale() {
        // Same times, different zoom: at 1px/unit they collide, at
        // 20px/unit they don't.
        let times = [0.0, 3.0];
        assert_eq!(pack_lanes(&times, 40.0, 1.0, 3), vec![0, 1]);
        assert_eq!(pack_lanes(&times, 40.0, 20.0, 3), vec![0, 0]);
    }

    #[test]
    fn zero_lanes_is_treated_as_one() {
        assert_eq!(pack_lanes(&[0.0, 1.0], 40.0, 1.0, 0), vec![0, 0]);
    }

    #[test]
    fn nearest_event_respects_the_pixel_tolerance() {
        let s = stepped();
        let c = correlate(&s, &[50.0, 150.0], &[None, None], 5.0, 40.0, 1.0, 3);
        assert_eq!(nearest_event(&c, 52.0, 1.0, 10.0).map(|e| e.index), Some(0));
        assert_eq!(
            nearest_event(&c, 148.0, 1.0, 10.0).map(|e| e.index),
            Some(1)
        );
        // Halfway between, outside tolerance of either.
        assert!(nearest_event(&c, 100.0, 1.0, 10.0).is_none());
        assert!(nearest_event(&[], 0.0, 1.0, 10.0).is_none());
    }

    #[test]
    fn common_grid_covers_only_the_overlap() {
        let model = [(0.0, 10.0), (100.0, 20.0)];
        let crowd = [(50.0, 30.0), (200.0, 40.0)];
        let g = common_grid(&model, &crowd);
        assert!(!g.is_empty());
        assert!(close(g[0].0, 50.0), "starts at the later start");
        assert!(close(g[g.len() - 1].0, 100.0), "ends at the earlier end");
        for (_, a, b) in &g {
            assert!(a.is_finite() && b.is_finite());
        }
    }

    #[test]
    fn common_grid_is_empty_when_the_series_do_not_overlap() {
        let a = [(0.0, 1.0), (10.0, 2.0)];
        let b = [(50.0, 3.0), (60.0, 4.0)];
        assert!(common_grid(&a, &b).is_empty());
        // And when either side is too short to interpolate.
        assert!(common_grid(&a, &[(5.0, 1.0)]).is_empty());
        assert!(common_grid(&[], &b).is_empty());
    }

    #[test]
    fn common_grid_includes_both_series_breakpoints() {
        // The band must bend where either curve bends, otherwise the
        // fill cuts corners the lines don't.
        let model = [(0.0, 0.0), (10.0, 10.0), (20.0, 0.0)];
        let crowd = [(0.0, 5.0), (15.0, 5.0), (20.0, 5.0)];
        let g = common_grid(&model, &crowd);
        let ts: Vec<f64> = g.iter().map(|(t, _, _)| *t).collect();
        for expected in [0.0, 10.0, 15.0, 20.0] {
            assert!(
                ts.iter().any(|t| close(*t, expected)),
                "grid missing breakpoint {expected}: {ts:?}"
            );
        }
    }
}
