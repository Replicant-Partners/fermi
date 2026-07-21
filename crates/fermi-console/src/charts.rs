//! Chart rendering — plotters to RGB pixel buffers.
//!
//! Tufte rules: no fill, no gradient, no decoration.
//! Data is bright lines on a dark canvas. That's it.

use plotters::prelude::*;
use std::sync::Arc;

// Canvas backgrounds — match GPUI theme values exactly
const BG: RGBColor = RGBColor(31, 36, 48); // matches theme::BG (0x1F2430) — main content area
const BG_CARD: RGBColor = RGBColor(39, 45, 56); // matches theme::BG_ELEVATED — cards, panels
const CHROME: RGBColor = RGBColor(50, 58, 72);
const LABEL: RGBColor = RGBColor(92, 103, 115);

// Data colors — one per meaning
const CYAN: RGBColor = RGBColor(92, 207, 230); // inside view / your model
const GOLD: RGBColor = RGBColor(255, 204, 102); // base rate / reference
const GREEN: RGBColor = RGBColor(186, 230, 126); // p50 markers
const PURPLE: RGBColor = RGBColor(212, 191, 255); // crowd price (Polymarket)

// Muted cyan for bar fills — hand-picked to read as clearly cyan on dark BG.
const CYAN_BAR: RGBColor = RGBColor(35, 100, 120);
// Muted purple underlay for the crowd worm — gives visual weight parity
// with the CYAN_BAR underlay of the model worm so the two trails read
// as peers rather than "real data + faint hint".
const PURPLE_BAR: RGBColor = RGBColor(80, 60, 140);
// Divergence fill — muted grey-cyan mixing the two worm colors, sits
// between the model and crowd trails to make the *gap* itself palpable
// instead of leaving the eye to compare two thin lines. Deliberately
// desaturated so the worms themselves stay the visually dominant
// features.
const DIVERGENCE_FILL: RGBColor = RGBColor(58, 72, 92);
// Resolved-forecast marker — the same green used for resolved chips in
// the console, so a resolved forecast reads consistently between the
// portfolio list and the trajectory chart.
const RESOLVED: RGBColor = RGBColor(186, 230, 126);
// BayesOps fit / refit marker. Was GOLD, but GOLD is also the base-rate
// horizontal — the collision made 'gold dot' unreadable next to the
// gold dashed line. Orange is the console's warning/attention accent
// (matches theme::ORANGE = 0xFF8F40), so refit events read as 'model
// structure just changed' with no clash against the base-rate line.
const REFIT: RGBColor = RGBColor(255, 143, 64);

// ═══════════════════════════════════════════════════════════════════
// Public data types
// ═══════════════════════════════════════════════════════════════════

pub struct DriverViz {
    pub name: String,
    pub impact: f64,
    pub quality: f64,
    pub evidence: Vec<String>,
}

pub struct IndexPoint {
    pub label: String,
    pub inside_view: f64,
    pub outside_view: f64,
    pub crowd_price: Option<f64>, // Polymarket crowd-implied probability
}

/// One point on the trajectory worm. `t_seconds` is seconds since the
/// trajectory's first timestamp (or epoch if degenerate); `rate_pct` is
/// the inside-view probability at that moment, in 0–100 scale.
pub struct TrajectoryPoint {
    pub t_seconds: f64,
    pub rate_pct: f64,
}

/// One marker on the trajectory worm — an event that happened at a
/// specific time and (optionally) moved the rate. The renderer draws a
/// colored dot at (t_seconds, rate_pct_at_event) so the operator can see
/// when each Apply / BayesOps fit / agent run / market tick happened.
pub struct TrajectoryEvent {
    pub t_seconds: f64,
    pub rate_pct: f64, // y-position of the dot
    pub kind: TrajectoryEventKind,
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum TrajectoryEventKind {
    /// A rate revision (Apply, schedule rerun, etc.) — cyan dot, larger.
    RateRevision,
    /// A BayesOps fitted-distribution accept — gold dot.
    BayesOpsFit,
    /// An agent run that didn't directly move the rate — small grey dot.
    AgentRun,
    /// A Polymarket observation — purple dot.
    MarketObservation,
}

// ═══════════════════════════════════════════════════════════════════
// Index Chart — Inside vs Outside vs Crowd price over time
//
// Three lines: cyan (your model), gold (base rate), purple (crowd).
// ═══════════════════════════════════════════════════════════════════

pub fn render_index_chart(
    history: &[IndexPoint],
    current_idx: usize,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let _ = root.fill(&BG);

        if history.len() >= 2 {
            // Collect all values including crowd price for y-axis range
            let mut vals: Vec<f64> = history
                .iter()
                .flat_map(|p| {
                    let mut v = vec![p.inside_view, p.outside_view];
                    if let Some(cp) = p.crowd_price {
                        v.push(cp);
                    }
                    v
                })
                .collect();
            if vals.is_empty() {
                vals.push(50.0);
            }
            let min_v = vals.iter().cloned().fold(f64::INFINITY, f64::min) - 2.0;
            let max_v = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max) + 2.0;
            let n = history.len();

            if let Ok(mut chart) = ChartBuilder::on(&root)
                .margin_top(6)
                .margin_right(8)
                .margin_bottom(4)
                .margin_left(4)
                .x_label_area_size(14)
                .y_label_area_size(30)
                .build_cartesian_2d(0usize..n.saturating_sub(1), min_v..max_v)
            {
                let _ = chart
                    .configure_mesh()
                    .x_labels(4)
                    .y_labels(3)
                    .label_style(("sans-serif", 8).into_font().color(&LABEL))
                    .axis_style(ShapeStyle::from(CHROME).stroke_width(1))
                    .light_line_style(ShapeStyle::from(CHROME).stroke_width(1))
                    .bold_line_style(ShapeStyle::from(CHROME).stroke_width(1))
                    .y_label_formatter(&|v| format!("{:.0}%", v))
                    .draw();

                // Base rate — gold line, thin
                let _ = chart.draw_series(LineSeries::new(
                    history.iter().enumerate().map(|(i, p)| (i, p.outside_view)),
                    ShapeStyle::from(GOLD).stroke_width(1),
                ));

                // Crowd price — purple line (only where data exists)
                let has_crowd = history.iter().any(|p| p.crowd_price.is_some());
                if has_crowd {
                    let crowd_points: Vec<(usize, f64)> = history
                        .iter()
                        .enumerate()
                        .filter_map(|(i, p)| p.crowd_price.map(|cp| (i, cp)))
                        .collect();
                    if crowd_points.len() >= 2 {
                        let _ = chart.draw_series(LineSeries::new(
                            crowd_points.iter().cloned(),
                            ShapeStyle::from(PURPLE).stroke_width(2),
                        ));
                    }
                    // Crowd dots
                    for (i, cp) in &crowd_points {
                        let _ = chart.draw_series(std::iter::once(Circle::new(
                            (*i, *cp),
                            2,
                            ShapeStyle::from(PURPLE).filled(),
                        )));
                    }
                }

                // Inside view — cyan line, bold
                let _ = chart.draw_series(LineSeries::new(
                    history.iter().enumerate().map(|(i, p)| (i, p.inside_view)),
                    ShapeStyle::from(CYAN).stroke_width(2),
                ));

                // Dots on inside line
                for (i, p) in history.iter().enumerate() {
                    let (size, col) = if i == current_idx {
                        (4, CYAN)
                    } else {
                        (2, CHROME)
                    };
                    let _ = chart.draw_series(std::iter::once(Circle::new(
                        (i, p.inside_view),
                        size,
                        ShapeStyle::from(col).filled(),
                    )));
                }
            }
        } else if history.len() == 1 {
            let p = &history[0];
            let _ = root.draw(&Text::new(
                format!("{:.1}%", p.inside_view),
                (width as i32 / 2 - 15, height as i32 / 2 - 6),
                ("sans-serif", 12u32).into_font().color(&CYAN),
            ));
        }
        let _ = root.present();
    }
    buf
}

// ═══════════════════════════════════════════════════════════════════
// Histogram — single-color bars, optional percentile markers
// ═══════════════════════════════════════════════════════════════════

pub fn render_histogram_chart(bins: &[u32], width: u32, height: u32) -> Vec<u8> {
    render_histogram_with_percentiles(bins, None, width, height)
}

pub fn render_histogram_with_percentiles(
    bins: &[u32],
    percentiles: Option<(f64, f64, f64)>,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let _ = root.fill(&BG);

        if !bins.is_empty() {
            let max_count = *bins.iter().max().unwrap_or(&1) as f64;
            let n = bins.len();

            if let Ok(mut chart) = ChartBuilder::on(&root)
                .margin_top(4)
                .margin_right(4)
                .margin_bottom(4)
                .margin_left(4)
                .x_label_area_size(12)
                .y_label_area_size(0)
                .build_cartesian_2d(0f64..n as f64, 0.0..max_count * 1.08)
            {
                let _ = chart
                    .configure_mesh()
                    .disable_mesh()
                    .x_labels(0)
                    .y_labels(0)
                    .draw();

                // Bars — hand-picked muted cyan, NOT blended
                let _ = chart.draw_series(bins.iter().enumerate().map(|(i, &count)| {
                    Rectangle::new(
                        [(i as f64 + 0.08, 0.0), (i as f64 + 0.92, count as f64)],
                        ShapeStyle::from(CYAN_BAR).filled(),
                    )
                }));

                // Bar top edge — bright cyan line for definition
                let _ = chart.draw_series(bins.iter().enumerate().map(|(i, &count)| {
                    PathElement::new(
                        vec![
                            (i as f64 + 0.08, count as f64),
                            (i as f64 + 0.92, count as f64),
                        ],
                        ShapeStyle::from(CYAN).stroke_width(1),
                    )
                }));

                // Percentile lines
                if let Some((p5, p50, p95)) = percentiles {
                    for px in [p5 * n as f64, p95 * n as f64] {
                        let _ = chart.draw_series(std::iter::once(PathElement::new(
                            vec![(px, 0.0), (px, max_count * 1.05)],
                            ShapeStyle::from(GOLD).stroke_width(1),
                        )));
                    }
                    let _ = chart.draw_series(std::iter::once(PathElement::new(
                        vec![(p50 * n as f64, 0.0), (p50 * n as f64, max_count * 1.05)],
                        ShapeStyle::from(GREEN).stroke_width(1),
                    )));
                }
            }
        }
        let _ = root.present();
    }
    buf
}

// ═══════════════════════════════════════════════════════════════════
// Distribution Sparkline — line only, no fill
// ═══════════════════════════════════════════════════════════════════

pub fn render_distribution_sparkline(
    p5: f64,
    p50: f64,
    p95: f64,
    width: u32,
    height: u32,
) -> Vec<u8> {
    render_distribution_sparkline_on(p5, p50, p95, width, height, BG_CARD)
}

/// Render sparkline with a specific background color so it blends into its container.
pub fn render_distribution_sparkline_on(
    p5: f64,
    p50: f64,
    p95: f64,
    width: u32,
    height: u32,
    bg: RGBColor,
) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let _ = root.fill(&bg);

        if p95 > p5 {
            let range = p95 - p5;
            let steps = width as usize;
            let points: Vec<(f64, f64)> = (0..=steps)
                .map(|i| {
                    let x = p5 + (i as f64 / steps as f64) * range;
                    let y = if x < p50 {
                        2.0 * (x - p5) / (range * (p50 - p5).max(0.001))
                    } else {
                        2.0 * (p95 - x) / (range * (p95 - p50).max(0.001))
                    };
                    (x, y.max(0.0))
                })
                .collect();

            let max_y = points.iter().map(|(_, y)| *y).fold(0.0_f64, f64::max);
            if max_y > 0.0 {
                if let Ok(mut chart) = ChartBuilder::on(&root)
                    .margin(1)
                    .build_cartesian_2d(p5..p95, 0.0..max_y * 1.1)
                {
                    // Line only — no fill
                    let _ = chart.draw_series(LineSeries::new(
                        points.iter().cloned(),
                        ShapeStyle::from(CYAN).stroke_width(1),
                    ));
                    // p50 tick
                    let _ = chart.draw_series(std::iter::once(PathElement::new(
                        vec![(p50, 0.0), (p50, max_y * 0.35)],
                        ShapeStyle::from(GREEN).stroke_width(1),
                    )));
                }
            }
        }
        let _ = root.present();
    }
    buf
}

// ═══════════════════════════════════════════════════════════════════
// Trajectory Worm — rate over time with event markers
//
// Tells the story of how a forecast evolved. The trail (cyan line)
// connects every rate revision in chronological order. Event markers
// sit at (t, rate-at-event) showing what caused the rate to move:
//   • Apply → cyan dot (larger)
//   • BayesOps fit → gold dot
//   • Agent run → small grey dot (research happened, may not have moved rate)
//   • Market obs → purple dot (crowd price snapshot)
//
// Reference lines: gold horizontal at outside-view base rate; purple
// horizontal at the latest Polymarket crowd price (if linked). The
// operator's eye tracks: did my model walk toward, away from, or past
// the crowd price?
// ═══════════════════════════════════════════════════════════════════

/// Render the trajectory worm.
///
/// * `series` — the operator's inside-view rate points (cyan).
/// * `crowd_series` — optional Polymarket crowd-price points (purple).
///   When non-empty this replaces the flat `crowd_price_pct` horizontal
///   with a real worm so the operator can see whether the model is
///   walking TOWARD, AWAY from, or PAST the crowd over time — the
///   entire point of the trajectory view.
/// * `crowd_price_pct` — fallback horizontal shown only when there's no
///   `crowd_series` history yet (fresh forecast, no snapshots recorded).
pub fn render_trajectory_worm(
    series: &[TrajectoryPoint],
    crowd_series: &[TrajectoryPoint],
    events: &[TrajectoryEvent],
    base_rate_pct: Option<f64>,
    crowd_price_pct: Option<f64>,
    // Anchor timestamp for calendar-formatted x-axis labels. When Some,
    // the axis shows "Jun 17 / Jul 12" instead of the raw "+Nd" offsets
    // — much easier to correlate with real-world events when the forecast
    // spans days or weeks.
    earliest: Option<chrono::DateTime<chrono::Utc>>,
    // Seconds-since-earliest of the resolution event, if the forecast
    // has been resolved. Draws a vertical marker + label.
    resolved_at_secs: Option<f64>,
    width: u32,
    height: u32,
) -> Vec<u8> {
    // Reserve the bottom 14px for an event-density rug strip — vertical
    // ticks per event, tightly packed, so the operator can see WHEN
    // activity clustered even if the worm trail itself is flat.
    const RUG_HEIGHT: u32 = 14;
    let chart_height = height.saturating_sub(RUG_HEIGHT).max(40);

    let mut buf = vec![0u8; (width * height * 3) as usize];

    if series.is_empty() && crowd_series.is_empty() && events.is_empty() {
        // Degenerate: no data. Render a centered hint and bail.
        let root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let _ = root.fill(&BG);
        let _ = root.draw(&Text::new(
            "no trajectory yet — run an agent or accept a suggestion to begin",
            (width as i32 / 2 - 180, height as i32 / 2 - 6),
            ("sans-serif", 11u32).into_font().color(&LABEL),
        ));
        let _ = root.present();
        drop(root);
        return buf;
    }

    // ── Compute axis ranges before drawing so we can use the same
    //    coords for chart, rug, and event-position math.
    let mut all_y: Vec<f64> = series.iter().map(|p| p.rate_pct).collect();
    all_y.extend(crowd_series.iter().map(|p| p.rate_pct));
    all_y.extend(events.iter().map(|e| e.rate_pct));
    if let Some(b) = base_rate_pct {
        all_y.push(b);
    }
    if let Some(c) = crowd_price_pct {
        all_y.push(c);
    }
    if all_y.is_empty() {
        all_y.push(2.08);
    }
    let raw_min = all_y.iter().cloned().fold(f64::INFINITY, f64::min);
    let raw_max = all_y.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    // Pad ~10% of range so dots on the boundary don't sit on the axis.
    // Always show at least 0% on the bottom so the y-axis reads honestly.
    let y_pad = ((raw_max - raw_min) * 0.10).max(1.0);
    let y_min = (raw_min - y_pad).max(0.0);
    let y_max = raw_max + y_pad;

    let mut all_x: Vec<f64> = series.iter().map(|p| p.t_seconds).collect();
    all_x.extend(crowd_series.iter().map(|p| p.t_seconds));
    all_x.extend(events.iter().map(|e| e.t_seconds));
    let x_min = all_x.iter().cloned().fold(f64::INFINITY, f64::min).min(0.0);
    let x_max_raw = all_x.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let x_max = if x_max_raw <= x_min {
        x_min + 60.0 // 1-minute fallback so degenerate single-point doesn't crash
    } else {
        x_max_raw
    };

    // ── Pass 1: the main chart in the upper area ─────────────────────
    {
        let chart_root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let _ = chart_root.fill(&BG);

        // Carve out top region for the chart, leaving the rug strip
        // below. We previously used `.titled("", ...)` here but that
        // requires the font system to load a font just to render an
        // empty title, and panics on Linux/macOS systems where plotters'
        // default 'sans-serif' alias isn't registered. Use margin()
        // directly on the drawing area instead — same end result, no
        // font lookup. (The rest of the charts in this file do this.)
        let upper = chart_root.margin(0, 0, 0, RUG_HEIGHT as i32);

        if let Ok(mut chart) = ChartBuilder::on(&upper)
            .margin_top(10)
            .margin_right(60) // big right margin so the inline
            // base-rate / crowd-price labels fit
            .margin_bottom(8)
            .margin_left(6)
            .x_label_area_size(20)
            .y_label_area_size(40)
            .build_cartesian_2d(x_min..x_max, y_min..y_max)
        {
            let span = x_max - x_min;

            // Sparse grid — 4 horizontal, 4 vertical. Faint horizontal
            // gridlines only (the y-axis is what the operator's eye tracks;
            // vertical gridlines add noise without carrying meaning). Uses
            // `light_line_style` for a whisper-of-a-line — visible enough
            // to trace a value across the chart, dim enough not to compete
            // with the worms.
            let _ = chart
                .configure_mesh()
                .x_labels(4)
                .y_labels(4)
                .label_style(("sans-serif", 10).into_font().color(&LABEL))
                .axis_style(ShapeStyle::from(CHROME).stroke_width(1))
                .light_line_style(ShapeStyle::from(CHROME).stroke_width(1))
                // Keep the heavy bold mesh disabled; only the light
                // horizontal gridlines get drawn (see `.disable_x_mesh`
                // below which silences the vertical light lines).
                .disable_x_mesh()
                .max_light_lines(4)
                .y_label_formatter(&|v| format!("{:.0}%", v))
                .x_label_formatter(&|v| {
                    // Prefer calendar-formatted labels when we have an
                    // anchor timestamp and the span is wider than an
                    // hour — much easier to correlate with real-world
                    // events ("the Portugal loss on Jul 4") than raw
                    // "+27d" offsets. Falls back to relative time for
                    // short spans / when the anchor isn't provided.
                    let secs = *v;
                    let use_calendar = earliest.is_some() && span >= 60.0 * 60.0;
                    if use_calendar {
                        let ts = earliest.unwrap()
                            + chrono::Duration::milliseconds((secs * 1000.0) as i64);
                        if span >= 30.0 * 24.0 * 60.0 * 60.0 {
                            // Multi-month span: month-day works, year
                            // still implicit.
                            ts.format("%b %-d").to_string()
                        } else if span >= 24.0 * 60.0 * 60.0 {
                            // Multi-day span: month-day is best;
                            // shorter forecasts don't need year.
                            ts.format("%b %-d").to_string()
                        } else {
                            // Hours-only span: show hour + minute.
                            ts.format("%H:%M").to_string()
                        }
                    } else {
                        let rel = secs - x_min;
                        if span < 60.0 {
                            format!("{:.0}s", rel)
                        } else if span < 60.0 * 60.0 {
                            format!("+{:.0}m", rel / 60.0)
                        } else if span < 24.0 * 60.0 * 60.0 {
                            format!("+{:.1}h", rel / 3600.0)
                        } else {
                            format!("+{:.1}d", rel / 86400.0)
                        }
                    }
                })
                .draw();

            // ── Divergence fill — the story-telling layer ───────────────
            //
            // Fill the polygon bounded by the model curve and the crowd
            // curve. Makes the *gap* palpable at a glance instead of
            // leaving the eye to measure it between two thin lines. Only
            // fires when both worms have >=2 points; otherwise there's
            // no meaningful gap to shade.
            //
            // Timestamps in the two series don't line up (model updates
            // on Apply / re-sim, crowd updates on PM poll), so build a
            // union time grid and interpolate both curves onto it.
            if series.len() >= 2 && crowd_series.len() >= 2 {
                let mut grid: Vec<f64> = series
                    .iter()
                    .map(|p| p.t_seconds)
                    .chain(crowd_series.iter().map(|p| p.t_seconds))
                    .collect();
                grid.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                grid.dedup_by(|a, b| (*a - *b).abs() < 1e-6);

                let interp = |s: &[TrajectoryPoint], t: f64| -> Option<f64> {
                    if s.is_empty() {
                        return None;
                    }
                    let first = s.first().unwrap();
                    let last = s.last().unwrap();
                    if t <= first.t_seconds {
                        return Some(first.rate_pct);
                    }
                    if t >= last.t_seconds {
                        return Some(last.rate_pct);
                    }
                    for w in s.windows(2) {
                        if t >= w[0].t_seconds && t <= w[1].t_seconds {
                            let dt = w[1].t_seconds - w[0].t_seconds;
                            if dt.abs() < 1e-9 {
                                return Some(w[0].rate_pct);
                            }
                            let frac = (t - w[0].t_seconds) / dt;
                            return Some(w[0].rate_pct + frac * (w[1].rate_pct - w[0].rate_pct));
                        }
                    }
                    None
                };

                // Only include grid points where BOTH curves have
                // coverage — avoids the fill hanging out into empty
                // regions where one series hasn't started yet.
                let model_min = series.first().unwrap().t_seconds;
                let model_max = series.last().unwrap().t_seconds;
                let crowd_min = crowd_series.first().unwrap().t_seconds;
                let crowd_max = crowd_series.last().unwrap().t_seconds;
                let overlap_min = model_min.max(crowd_min);
                let overlap_max = model_max.min(crowd_max);

                let mut top: Vec<(f64, f64)> = Vec::new();
                let mut bot: Vec<(f64, f64)> = Vec::new();
                for &t in &grid {
                    if t < overlap_min || t > overlap_max {
                        continue;
                    }
                    if let (Some(m), Some(c)) = (interp(series, t), interp(crowd_series, t)) {
                        top.push((t, m));
                        bot.push((t, c));
                    }
                }
                if top.len() >= 2 {
                    // Build the closing polygon: forward along the model
                    // curve, backward along the crowd curve. plotters
                    // handles self-intersecting shapes reasonably for
                    // the alternating-crossings case that happens when
                    // model and crowd trade places.
                    let mut poly: Vec<(f64, f64)> = top.clone();
                    poly.extend(bot.iter().rev());
                    let _ = chart.draw_series(std::iter::once(Polygon::new(
                        poly,
                        ShapeStyle::from(DIVERGENCE_FILL).filled(),
                    )));
                }
            }

            // Reference: base-rate horizontal — dashed gold line. Drawn
            // before the worm so the worm sits visually on top.
            if let Some(b) = base_rate_pct {
                // Plotters has no native dashed style. Emulate by drawing
                // alternating short segments along the line.
                let span_x = x_max - x_min;
                let dash_n = 30;
                let dash_w = span_x / (dash_n as f64 * 2.0);
                let dashes: Vec<[(f64, f64); 2]> = (0..dash_n)
                    .map(|i| {
                        let x0 = x_min + (i as f64) * 2.0 * dash_w;
                        let x1 = x0 + dash_w;
                        [(x0, b), (x1, b)]
                    })
                    .collect();
                for d in &dashes {
                    let _ = chart.draw_series(LineSeries::new(
                        d.iter().cloned(),
                        ShapeStyle::from(GOLD).stroke_width(1),
                    ));
                }
            }
            // Reference / crowd worm.
            //
            // If we have a proper crowd time-series, draw it as a purple
            // worm (matches the visual weight of the model worm so the
            // two are directly comparable). Falls back to a flat purple
            // horizontal at the latest crowd price when we only have a
            // point-in-time reading — e.g. right after linking a market
            // and before the poll has accumulated history.
            if crowd_series.len() >= 2 {
                // Purple underlay for weight parity with the cyan worm.
                let _ = chart.draw_series(LineSeries::new(
                    crowd_series.iter().map(|p| (p.t_seconds, p.rate_pct)),
                    ShapeStyle::from(PURPLE_BAR).stroke_width(5),
                ));
                let _ = chart.draw_series(LineSeries::new(
                    crowd_series.iter().map(|p| (p.t_seconds, p.rate_pct)),
                    ShapeStyle::from(PURPLE).stroke_width(2),
                ));
            } else if let Some(c) = crowd_price_pct {
                let _ = chart.draw_series(LineSeries::new(
                    vec![(x_min, c), (x_max, c)],
                    ShapeStyle::from(PURPLE).stroke_width(2),
                ));
            }

            // The worm: cyan trail. Two-pass for visual weight — a
            // muted underlay first, then the bright core on top. Reads
            // as having heft instead of being a hairline. Drawn AFTER
            // the crowd worm so the operator's inside view is the
            // visually-dominant line — they should see their own model
            // first, then read the crowd context around it.
            if series.len() >= 2 {
                // Underlay — slightly thicker, dimmer cyan
                let _ = chart.draw_series(LineSeries::new(
                    series.iter().map(|p| (p.t_seconds, p.rate_pct)),
                    ShapeStyle::from(CYAN_BAR).stroke_width(5),
                ));
                // Core — bright cyan
                let _ = chart.draw_series(LineSeries::new(
                    series.iter().map(|p| (p.t_seconds, p.rate_pct)),
                    ShapeStyle::from(CYAN).stroke_width(2),
                ));
            }

            // ── Resolution marker ─────────────────────────────────────
            //
            // If the forecast has been resolved, drop a vertical green
            // line at the resolution timestamp with a small 'resolved'
            // label. Drawn after the worms so it visually caps the
            // trajectory — the operator sees at a glance "the story
            // stopped here".
            if let Some(t_res) = resolved_at_secs {
                if t_res >= x_min && t_res <= x_max {
                    let _ = chart.draw_series(LineSeries::new(
                        vec![(t_res, y_min), (t_res, y_max)],
                        ShapeStyle::from(RESOLVED).stroke_width(1),
                    ));
                }
            }

            // Event markers — bigger, with a darker outline ring so they
            // pop on the dark background. Render in priority order:
            // agent_run dots first (smallest, most numerous), then market
            // obs, then BayesOps fits, then rate revisions on top.
            //
            // Shape channel: each kind uses a distinct SHAPE in addition
            // to color, so the chart is legible without color perception
            // (protanopia/deuteranopia readers, screenshots, etc.):
            //   RateRevision       → filled circle (the primary event)
            //   BayesOpsFit        → diamond      (structure change)
            //   MarketObservation  → hollow ring  (external observation)
            //   AgentRun           → short tick   (activity, non-moving)
            let kind_priority = |k: &TrajectoryEventKind| -> u8 {
                match k {
                    TrajectoryEventKind::AgentRun => 0,
                    TrajectoryEventKind::MarketObservation => 1,
                    TrajectoryEventKind::BayesOpsFit => 2,
                    TrajectoryEventKind::RateRevision => 3,
                }
            };
            let mut sorted: Vec<&TrajectoryEvent> = events.iter().collect();
            sorted.sort_by_key(|e| kind_priority(&e.kind));

            // Data-to-pixel helper so we can drop shape primitives
            // directly (plotters' Circle handles data coords, but
            // Polygon/PathElement in data coords is fiddly — build the
            // diamond in data space here).
            let x_span = (x_max - x_min).max(1e-9);
            let y_span = (y_max - y_min).max(1e-9);
            // Pixel-to-data conversion for the ~800x226 chart area.
            // Diamond half-widths ~7px — convert to data units.
            let px_to_x = |px: f64| px * x_span / ((width as f64) - 60.0 - 46.0);
            let px_to_y = |py: f64| py * y_span / ((chart_height as f64) - 38.0);

            for ev in sorted {
                let (color, kind_size) = match ev.kind {
                    TrajectoryEventKind::RateRevision => (CYAN, 8),
                    TrajectoryEventKind::BayesOpsFit => (REFIT, 8),
                    TrajectoryEventKind::AgentRun => (LABEL, 4),
                    TrajectoryEventKind::MarketObservation => (PURPLE, 6),
                };
                match ev.kind {
                    TrajectoryEventKind::RateRevision => {
                        // Solid filled circle with BG outline ring.
                        let _ = chart.draw_series(std::iter::once(Circle::new(
                            (ev.t_seconds, ev.rate_pct),
                            kind_size + 1,
                            ShapeStyle::from(BG).filled(),
                        )));
                        let _ = chart.draw_series(std::iter::once(Circle::new(
                            (ev.t_seconds, ev.rate_pct),
                            kind_size,
                            ShapeStyle::from(color).filled(),
                        )));
                    }
                    TrajectoryEventKind::BayesOpsFit => {
                        // Diamond — rotated square built as a 4-vertex
                        // Polygon in data coords. Same visual weight as
                        // the RateRevision circle but distinguishable
                        // at a glance (and colorblind-safe).
                        let hx = px_to_x(kind_size as f64);
                        let hy = px_to_y(kind_size as f64);
                        // BG outline diamond (slightly larger)
                        let hx_out = px_to_x(kind_size as f64 + 1.5);
                        let hy_out = px_to_y(kind_size as f64 + 1.5);
                        let outline = vec![
                            (ev.t_seconds, ev.rate_pct + hy_out),
                            (ev.t_seconds + hx_out, ev.rate_pct),
                            (ev.t_seconds, ev.rate_pct - hy_out),
                            (ev.t_seconds - hx_out, ev.rate_pct),
                        ];
                        let _ = chart.draw_series(std::iter::once(Polygon::new(
                            outline,
                            ShapeStyle::from(BG).filled(),
                        )));
                        let core = vec![
                            (ev.t_seconds, ev.rate_pct + hy),
                            (ev.t_seconds + hx, ev.rate_pct),
                            (ev.t_seconds, ev.rate_pct - hy),
                            (ev.t_seconds - hx, ev.rate_pct),
                        ];
                        let _ = chart.draw_series(std::iter::once(Polygon::new(
                            core,
                            ShapeStyle::from(color).filled(),
                        )));
                    }
                    TrajectoryEventKind::MarketObservation => {
                        // Hollow ring (outer purple circle, BG inner
                        // punch). Distinguishes market obs from other
                        // filled markers without needing color.
                        let _ = chart.draw_series(std::iter::once(Circle::new(
                            (ev.t_seconds, ev.rate_pct),
                            kind_size,
                            ShapeStyle::from(color).filled(),
                        )));
                        let _ = chart.draw_series(std::iter::once(Circle::new(
                            (ev.t_seconds, ev.rate_pct),
                            (kind_size - 2).max(2),
                            ShapeStyle::from(BG).filled(),
                        )));
                    }
                    TrajectoryEventKind::AgentRun => {
                        // Short vertical tick anchored to the rate
                        // line: quiet enough to imply "activity here"
                        // without competing with the moving markers.
                        let tick_h = px_to_y(3.5);
                        let _ = chart.draw_series(LineSeries::new(
                            vec![
                                (ev.t_seconds, ev.rate_pct - tick_h),
                                (ev.t_seconds, ev.rate_pct + tick_h),
                            ],
                            ShapeStyle::from(color).stroke_width(2),
                        ));
                    }
                }
            }

            // Inline labels for reference lines, drawn at the right
            // edge of the chart in the right-margin area. Plotters
            // doesn't support 'put text in margin' directly so we
            // compute pixel coords manually after the fact.
        }
        let _ = chart_root.present();
        drop(chart_root);
    }

    // ── Pass 2: right-margin reference legend column ──────────────
    //
    // Instead of placing labels inline (which collided with the worm
    // whenever a value drifted near a reference line), reserve a
    // permanent 58px column on the right edge of the chart. For each
    // reference (inside/outside/crowd), draw a small color-coded chip
    // that sits at the *current y-value of that reference*, plus a
    // short leader tick pointing back at the plot area. The result is
    // an always-legible, non-colliding legend that also anchors the
    // eye to "this line means X%".
    //
    // Chip layout (per row):
    //   │ ┄ ┄   │ base 34.2%   →  a) 4px stub inside the plot area,
    //                                    color-matched to the line
    //                                 b) name + value in the same color,
    //                                    baseline-aligned to the stub
    //
    // The chips are then de-collided (bumped apart in y) so two lines
    // whose current values are within 12px of each other don't overlap.
    {
        let label_root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let plot_top = 10i32;
        let plot_bot = chart_height as i32 - 28;
        let plot_h = (plot_bot - plot_top).max(1);
        let y_to_px = |y: f64| -> i32 {
            if y_max <= y_min {
                return plot_top;
            }
            let frac = (y - y_min) / (y_max - y_min);
            plot_bot - (frac * plot_h as f64) as i32
        };

        // Right-hand column: plot_right (= width - 60) is the chart's
        // right edge; we sit chips just inside the margin so they
        // don't fall off the canvas on narrow renders.
        let plot_right = (width as i32) - 60;
        let chip_x = plot_right + 6;
        let stub_x0 = plot_right - 4;
        let stub_x1 = plot_right + 3;

        // Collect references to render, each with (label, value, color).
        // `crowd_pct_now` prefers the live worm's tail over the
        // point-in-time crowd_price_pct so the label always reflects
        // the freshest signal on the chart.
        let crowd_pct_now = crowd_series.last().map(|p| p.rate_pct).or(crowd_price_pct);
        let inside_pct_now = series.last().map(|p| p.rate_pct);

        let mut chips: Vec<(String, f64, RGBColor)> = Vec::new();
        if let Some(v) = inside_pct_now {
            chips.push((format!("you {:.1}%", v), v, CYAN));
        }
        if let Some(v) = crowd_pct_now {
            chips.push((format!("crowd {:.1}%", v), v, PURPLE));
        }
        if let Some(v) = base_rate_pct {
            chips.push((format!("base {:.1}%", v), v, GOLD));
        }

        // Compute y-pixels, then de-collide by bumping the closer-to-
        // midline chip. A 12px minimum spacing keeps two-line texts
        // legible without over-separating chips whose real values are
        // close together (we still want the chip's y to reflect the
        // actual value).
        let mut placed: Vec<(String, i32, RGBColor, i32)> = chips
            .iter()
            .map(|(label, v, color)| {
                let anchor_y = y_to_px(*v);
                (label.clone(), anchor_y, *color, anchor_y)
            })
            .collect();
        // Sort by anchor y (top to bottom) and enforce 14px min spacing
        // between chips, biased downward so the topmost chip stays
        // anchored to its true value.
        placed.sort_by_key(|c| c.1);
        const MIN_SPACING: i32 = 14;
        for i in 1..placed.len() {
            let prev_y = placed[i - 1].3;
            if placed[i].3 < prev_y + MIN_SPACING {
                placed[i].3 = prev_y + MIN_SPACING;
            }
        }

        for (label, anchor_y, color, chip_y) in &placed {
            // Short horizontal leader from the chip back to the plot
            // edge at the actual data y. If the chip had to be bumped
            // to de-collide, the leader points to the true value so the
            // relationship stays honest.
            let _ = label_root.draw(&PathElement::new(
                vec![(stub_x0, *anchor_y), (stub_x1, *chip_y)],
                ShapeStyle::from(*color).stroke_width(1),
            ));
            let _ = label_root.draw(&Text::new(
                label.clone(),
                (chip_x, chip_y - 5),
                ("sans-serif", 10u32).into_font().color(color),
            ));
        }
        let _ = label_root.present();
        drop(label_root);
    }

    // ── Pass 3: event-density rug at the bottom ─────────────────────
    //
    // Vertical tick per event. Density = where the operator's been
    // active. Reads at a glance even when the trail is flat.
    {
        let rug_root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let rug_top = height as i32 - RUG_HEIGHT as i32 + 2;
        let rug_bot = height as i32 - 2;
        // Padding mirroring the chart's margin so the rug aligns
        // visually with the worm trail above.
        let plot_left = 46i32;
        let plot_right = (width as i32) - 60;
        let plot_w = (plot_right - plot_left).max(1);

        // Subtle horizontal baseline so the strip is visually framed.
        let _ = rug_root.draw(&PathElement::new(
            vec![(plot_left, rug_bot), (plot_right, rug_bot)],
            ShapeStyle::from(CHROME).stroke_width(1),
        ));

        for ev in events {
            let frac = if x_max > x_min {
                ((ev.t_seconds - x_min) / (x_max - x_min)).clamp(0.0, 1.0)
            } else {
                0.5
            };
            let x_pix = plot_left + (frac * plot_w as f64) as i32;
            let color = match ev.kind {
                TrajectoryEventKind::RateRevision => CYAN,
                TrajectoryEventKind::BayesOpsFit => REFIT,
                TrajectoryEventKind::AgentRun => LABEL,
                TrajectoryEventKind::MarketObservation => PURPLE,
            };
            let _ = rug_root.draw(&PathElement::new(
                vec![(x_pix, rug_top), (x_pix, rug_bot)],
                ShapeStyle::from(color).stroke_width(1),
            ));
        }
        let _ = rug_root.present();
        drop(rug_root);
    }

    buf
}

/// Plot-area bounds — exposed so callers can compute time-anchored
/// hover positions ("mouse is at pixel x=234 → that's timestamp T").
/// Duplicating this info in the public API is preferable to making
/// every hover consumer re-derive the plot-area constants that the
/// renderer uses internally.
pub struct TrajectoryPlotBounds {
    /// Plot area pixel rect: (left, top, right, bottom). Anything
    /// outside this rect is chrome / labels / rug and should NOT be
    /// treated as chart-relative.
    pub plot_left: i32,
    pub plot_top: i32,
    pub plot_right: i32,
    pub plot_bot: i32,
    /// X data range in seconds-since-earliest.
    pub x_min: f64,
    pub x_max: f64,
    /// Y data range in percent.
    pub y_min: f64,
    pub y_max: f64,
}

impl TrajectoryPlotBounds {
    /// Map a pixel-x (in canvas-local coords, 0 = left edge of the
    /// bitmap) to the corresponding t_seconds value. Returns None when
    /// the pixel is outside the plot area.
    pub fn pixel_to_t_seconds(&self, x_pix: f32) -> Option<f64> {
        let x = x_pix as i32;
        if x < self.plot_left || x > self.plot_right {
            return None;
        }
        let w = (self.plot_right - self.plot_left).max(1) as f64;
        let frac = ((x - self.plot_left) as f64) / w;
        Some(self.x_min + frac * (self.x_max - self.x_min))
    }

    /// Map a t_seconds value to canvas-local pixel-x. Returns None if
    /// t is outside the data range — caller can clamp or skip.
    pub fn t_to_pixel_x(&self, t: f64) -> Option<i32> {
        if t < self.x_min || t > self.x_max {
            return None;
        }
        let w = (self.plot_right - self.plot_left).max(1) as f64;
        let span = (self.x_max - self.x_min).max(1e-9);
        Some(self.plot_left + ((t - self.x_min) / span * w) as i32)
    }

    /// Map a rate percent (0..100) to canvas-local pixel-y.
    pub fn rate_to_pixel_y(&self, rate_pct: f64) -> i32 {
        let h = (self.plot_bot - self.plot_top).max(1) as f64;
        let span = (self.y_max - self.y_min).max(1e-9);
        let frac = ((rate_pct - self.y_min) / span).clamp(0.0, 1.0);
        self.plot_bot - (frac * h) as i32
    }
}

/// Compute the plot-area bounds for a given trajectory dataset. Same
/// range-derivation logic as `render_trajectory_worm` uses internally,
/// exposed so hover callers can share a coordinate space.
pub fn trajectory_plot_bounds(
    series: &[TrajectoryPoint],
    crowd_series: &[TrajectoryPoint],
    events: &[TrajectoryEvent],
    base_rate_pct: Option<f64>,
    crowd_price_pct: Option<f64>,
    width: u32,
    height: u32,
) -> TrajectoryPlotBounds {
    const RUG_HEIGHT: u32 = 14;
    let chart_height = height.saturating_sub(RUG_HEIGHT).max(40);

    let mut all_y: Vec<f64> = series.iter().map(|p| p.rate_pct).collect();
    all_y.extend(crowd_series.iter().map(|p| p.rate_pct));
    all_y.extend(events.iter().map(|e| e.rate_pct));
    if let Some(b) = base_rate_pct {
        all_y.push(b);
    }
    if let Some(c) = crowd_price_pct {
        all_y.push(c);
    }
    if all_y.is_empty() {
        all_y.push(2.08);
    }
    let raw_min = all_y.iter().cloned().fold(f64::INFINITY, f64::min);
    let raw_max = all_y.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let y_pad = ((raw_max - raw_min) * 0.10).max(1.0);
    let y_min = (raw_min - y_pad).max(0.0);
    let y_max = raw_max + y_pad;

    let mut all_x: Vec<f64> = series.iter().map(|p| p.t_seconds).collect();
    all_x.extend(crowd_series.iter().map(|p| p.t_seconds));
    all_x.extend(events.iter().map(|e| e.t_seconds));
    let x_min = all_x.iter().cloned().fold(f64::INFINITY, f64::min).min(0.0);
    let x_max_raw = all_x.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let x_max = if x_max_raw <= x_min {
        x_min + 60.0
    } else {
        x_max_raw
    };

    TrajectoryPlotBounds {
        plot_left: 46,
        plot_right: (width as i32) - 60,
        plot_top: 10,
        plot_bot: chart_height as i32 - 28,
        x_min,
        x_max,
        y_min,
        y_max,
    }
}

/// Linear interpolation on a trajectory series. Returns None only when
/// the series is empty; clamps to the endpoint values outside the
/// series' domain (so hover past the last known point returns the last
/// known value — sensible for "what's my model saying right now").
pub fn interpolate_trajectory_rate(series: &[TrajectoryPoint], t: f64) -> Option<f64> {
    if series.is_empty() {
        return None;
    }
    let first = series.first().unwrap();
    let last = series.last().unwrap();
    if t <= first.t_seconds {
        return Some(first.rate_pct);
    }
    if t >= last.t_seconds {
        return Some(last.rate_pct);
    }
    for w in series.windows(2) {
        if t >= w[0].t_seconds && t <= w[1].t_seconds {
            let dt = w[1].t_seconds - w[0].t_seconds;
            if dt.abs() < 1e-9 {
                return Some(w[0].rate_pct);
            }
            let frac = (t - w[0].t_seconds) / dt;
            return Some(w[0].rate_pct + frac * (w[1].rate_pct - w[0].rate_pct));
        }
    }
    None
}

/// Compute the pixel coordinates of each event for an interactive
/// overlay. Returns one (x, y, width, height) box per event in the
/// SAME ORDER as the input events slice, so the caller can correlate
/// hover regions with the source event objects.
///
/// Used by the cockpit's trajectory tab to place invisible hover divs
/// over the rendered chart bitmap.
pub fn trajectory_event_pixel_positions(
    events: &[TrajectoryEvent],
    series: &[TrajectoryPoint],
    crowd_series: &[TrajectoryPoint],
    base_rate_pct: Option<f64>,
    crowd_price_pct: Option<f64>,
    width: u32,
    height: u32,
) -> Vec<(i32, i32)> {
    if events.is_empty() {
        return Vec::new();
    }
    let b = trajectory_plot_bounds(
        series,
        crowd_series,
        events,
        base_rate_pct,
        crowd_price_pct,
        width,
        height,
    );
    events
        .iter()
        .map(|ev| {
            let x_pix = b.t_to_pixel_x(ev.t_seconds).unwrap_or({
                // Clamp instead of dropping so downstream hover overlays
                // always render one hit-box per event, even if an event
                // is exactly at the boundary.
                if ev.t_seconds < b.x_min {
                    b.plot_left
                } else {
                    b.plot_right
                }
            });
            let y_pix = b.rate_to_pixel_y(ev.rate_pct);
            (x_pix, y_pix)
        })
        .collect()
}

// ═══════════════════════════════════════════════════════════════════
// Treemap — fallback for non-GPUI contexts (kept for compatibility)
// ═══════════════════════════════════════════════════════════════════

pub fn render_treemap(drivers: &[DriverViz], width: u32, height: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let _ = root.fill(&BG);

        if !drivers.is_empty() {
            let total: f64 = drivers.iter().map(|d| d.impact.max(0.1)).sum();
            let bar_y0 = 14i32;
            let bar_y1 = height as i32 - 4;
            let usable_w = width as f64 - 8.0;
            let mut x = 4.0f64;

            for d in drivers {
                let frac = d.impact.max(0.1) / total;
                let cell_w = frac * usable_w;

                // Fill — muted cyan, NOT blended
                let _ = root.draw(&Rectangle::new(
                    [(x as i32 + 1, bar_y0), ((x + cell_w) as i32 - 1, bar_y1)],
                    ShapeStyle::from(CYAN_BAR).filled(),
                ));
                // Border
                let _ = root.draw(&Rectangle::new(
                    [(x as i32 + 1, bar_y0), ((x + cell_w) as i32 - 1, bar_y1)],
                    ShapeStyle::from(CHROME).stroke_width(1),
                ));
                // Label
                if cell_w > 30.0 {
                    let max_chars = ((cell_w - 8.0) / 5.5) as usize;
                    let label: String = if d.name.len() > max_chars {
                        d.name
                            .chars()
                            .take(max_chars.saturating_sub(1))
                            .collect::<String>()
                            + "…"
                    } else {
                        d.name.clone()
                    };
                    let _ = root.draw(&Text::new(
                        label,
                        (x as i32 + 4, 3),
                        ("sans-serif", 8u32).into_font().color(&LABEL),
                    ));
                }
                x += cell_w;
            }
        }
        let _ = root.present();
    }
    buf
}

// ═══════════════════════════════════════════════════════════════════
// Utilities
// ═══════════════════════════════════════════════════════════════════

pub fn rgb_to_render_image(rgb_buf: &[u8], width: u32, height: u32) -> Arc<gpui::RenderImage> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for chunk in rgb_buf.chunks(3) {
        rgba.push(chunk.get(0).copied().unwrap_or(0));
        rgba.push(chunk.get(1).copied().unwrap_or(0));
        rgba.push(chunk.get(2).copied().unwrap_or(0));
        rgba.push(255);
    }
    let img_buf = image::RgbaImage::from_raw(width, height, rgba)
        .unwrap_or_else(|| image::RgbaImage::new(width, height));
    let frame = image::Frame::new(img_buf);
    Arc::new(gpui::RenderImage::new(vec![frame]))
}
