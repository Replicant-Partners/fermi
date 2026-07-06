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

            // Sparse grid — 3 horizontal lines, 4 vertical, no decoration.
            // Tufte: the data is the chart, gridlines are scaffolding.
            let _ = chart
                .configure_mesh()
                .x_labels(4)
                .y_labels(3)
                .label_style(("sans-serif", 10).into_font().color(&LABEL))
                .axis_style(ShapeStyle::from(CHROME).stroke_width(1))
                // Don't draw the heavy mesh lines — too noisy.
                .disable_mesh()
                .y_label_formatter(&|v| format!("{:.0}%", v))
                .x_label_formatter(&|v| {
                    let secs = *v - x_min;
                    if span < 60.0 {
                        format!("{:.0}s", secs)
                    } else if span < 60.0 * 60.0 {
                        format!("+{:.0}m", secs / 60.0)
                    } else if span < 24.0 * 60.0 * 60.0 {
                        format!("+{:.1}h", secs / 3600.0)
                    } else if span < 7.0 * 24.0 * 60.0 * 60.0 {
                        format!("+{:.1}d", secs / 86400.0)
                    } else {
                        format!("+{:.0}d", secs / 86400.0)
                    }
                })
                .draw();

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

            // Event markers — bigger, with a darker outline ring so they
            // pop on the dark background. Render in priority order:
            // agent_run dots first (smallest, most numerous), then market
            // obs, then BayesOps fits, then rate revisions on top.
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

            for ev in sorted {
                let (color, size) = match ev.kind {
                    TrajectoryEventKind::RateRevision => (CYAN, 6),
                    TrajectoryEventKind::BayesOpsFit => (GOLD, 6),
                    TrajectoryEventKind::AgentRun => (LABEL, 3),
                    TrajectoryEventKind::MarketObservation => (PURPLE, 5),
                };
                // Outline ring (BG color) — visually lifts the dot off
                // the trail line.
                let _ = chart.draw_series(std::iter::once(Circle::new(
                    (ev.t_seconds, ev.rate_pct),
                    size + 1,
                    ShapeStyle::from(BG).filled(),
                )));
                // Filled core
                let _ = chart.draw_series(std::iter::once(Circle::new(
                    (ev.t_seconds, ev.rate_pct),
                    size,
                    ShapeStyle::from(color).filled(),
                )));
            }

            // Inline labels for reference lines, drawn at the right
            // edge of the chart in the right-margin area. Plotters
            // doesn't support 'put text in margin' directly so we
            // compute pixel coords manually after the fact.
        }
        let _ = chart_root.present();
        drop(chart_root);
    }

    // ── Pass 2: inline reference labels (base rate / crowd) ──────────
    //
    // Plotters' chart-area margins don't expose the inner pixel
    // coordinates we'd need to put labels exactly on the axis lines.
    // Emulate by drawing into a fresh DrawingArea using the full canvas
    // and computing y-pixel from the same y_min..y_max range we used
    // above. The horizontal placement is fixed at right-edge - 56px.
    //
    // This is approximate (the chart's plot area starts ~46px left of
    // the right edge after the legend margin), but the operator's eye
    // tolerates a few-pixel offset. The alternative is rebuilding the
    // chart with explicit text annotations inside the data area, which
    // collides with the plot when y-values cluster near the references.
    {
        let label_root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let y_to_px = |y: f64| -> i32 {
            // chart's plot area: top 10px margin, bottom RUG_HEIGHT + 8 + 20 (x-label area).
            let plot_top = 10i32;
            let plot_bot = chart_height as i32 - 28;
            let plot_h = plot_bot - plot_top;
            if y_max <= y_min {
                return plot_top;
            }
            let frac = (y - y_min) / (y_max - y_min);
            plot_bot - (frac * plot_h as f64) as i32
        };
        let label_x = (width as i32) - 58;

        if let Some(b) = base_rate_pct {
            let _ = label_root.draw(&Text::new(
                format!("base {:.1}%", b),
                (label_x, y_to_px(b) - 6),
                ("sans-serif", 9u32).into_font().color(&GOLD),
            ));
        }
        if let Some(c) = crowd_price_pct {
            let _ = label_root.draw(&Text::new(
                format!("crowd {:.1}%", c),
                (label_x, y_to_px(c) - 6),
                ("sans-serif", 9u32).into_font().color(&PURPLE),
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
                TrajectoryEventKind::BayesOpsFit => GOLD,
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

    const RUG_HEIGHT: u32 = 14;
    let chart_height = height.saturating_sub(RUG_HEIGHT).max(40);

    // Rebuild the same y/x ranges as render_trajectory_worm — this is
    // duplicated logic but keeping it here avoids a multi-return-tuple
    // inside the renderer that would clutter that function further.
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

    // Mirror the chart's plot-area pixel inset. These constants come
    // from the chart.margin_* + label-area calls above.
    let plot_left = 46i32;
    let plot_right = (width as i32) - 60;
    let plot_top = 10i32;
    let plot_bot = chart_height as i32 - 28;
    let plot_w = (plot_right - plot_left).max(1);
    let plot_h = (plot_bot - plot_top).max(1);

    events
        .iter()
        .map(|ev| {
            let fx = if x_max > x_min {
                ((ev.t_seconds - x_min) / (x_max - x_min)).clamp(0.0, 1.0)
            } else {
                0.5
            };
            let fy = if y_max > y_min {
                ((ev.rate_pct - y_min) / (y_max - y_min)).clamp(0.0, 1.0)
            } else {
                0.5
            };
            let x_pix = plot_left + (fx * plot_w as f64) as i32;
            let y_pix = plot_bot - (fy * plot_h as f64) as i32;
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
