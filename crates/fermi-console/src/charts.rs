//! Chart rendering — plotters to RGB pixel buffers.
//!
//! Tufte rules: no fill, no gradient, no decoration.
//! Data is bright lines on a dark canvas. That's it.

use plotters::prelude::*;
use std::sync::Arc;

// Canvas backgrounds — match GPUI theme values exactly
const BG: RGBColor = RGBColor(31, 36, 48);          // matches theme::BG (0x1F2430) — main content area
const BG_CARD: RGBColor = RGBColor(39, 45, 56);     // matches theme::BG_ELEVATED — cards, panels
const CHROME: RGBColor = RGBColor(50, 58, 72);
const LABEL: RGBColor = RGBColor(92, 103, 115);

// Data colors — one per meaning
const CYAN: RGBColor = RGBColor(92, 207, 230);      // inside view / your model
const GOLD: RGBColor = RGBColor(255, 204, 102);     // base rate / reference
const GREEN: RGBColor = RGBColor(186, 230, 126);    // p50 markers
const PURPLE: RGBColor = RGBColor(212, 191, 255);   // crowd price (Polymarket)

// Muted cyan for bar fills — hand-picked to read as clearly cyan on dark BG.
const CYAN_BAR: RGBColor = RGBColor(35, 100, 120);

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
    pub crowd_price: Option<f64>,  // Polymarket crowd-implied probability
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
    pub rate_pct: f64,         // y-position of the dot
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
            let mut vals: Vec<f64> = history.iter()
                .flat_map(|p| {
                    let mut v = vec![p.inside_view, p.outside_view];
                    if let Some(cp) = p.crowd_price { v.push(cp); }
                    v
                })
                .collect();
            if vals.is_empty() { vals.push(50.0); }
            let min_v = vals.iter().cloned().fold(f64::INFINITY, f64::min) - 2.0;
            let max_v = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max) + 2.0;
            let n = history.len();

            if let Ok(mut chart) = ChartBuilder::on(&root)
                .margin_top(6).margin_right(8).margin_bottom(4).margin_left(4)
                .x_label_area_size(14)
                .y_label_area_size(30)
                .build_cartesian_2d(0usize..n.saturating_sub(1), min_v..max_v)
            {
                let _ = chart.configure_mesh()
                    .x_labels(4).y_labels(3)
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
                    let crowd_points: Vec<(usize, f64)> = history.iter().enumerate()
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
                            (*i, *cp), 2, ShapeStyle::from(PURPLE).filled(),
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
                    let (size, col) = if i == current_idx { (4, CYAN) } else { (2, CHROME) };
                    let _ = chart.draw_series(std::iter::once(Circle::new(
                        (i, p.inside_view), size, ShapeStyle::from(col).filled(),
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
                .margin_top(4).margin_right(4).margin_bottom(4).margin_left(4)
                .x_label_area_size(12)
                .y_label_area_size(0)
                .build_cartesian_2d(0f64..n as f64, 0.0..max_count * 1.08)
            {
                let _ = chart.configure_mesh()
                    .disable_mesh()
                    .x_labels(0).y_labels(0)
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

pub fn render_trajectory_worm(
    series: &[TrajectoryPoint],
    events: &[TrajectoryEvent],
    base_rate_pct: Option<f64>,
    crowd_price_pct: Option<f64>,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let _ = root.fill(&BG);

        if series.is_empty() && events.is_empty() {
            // Degenerate: no data. Render a centered "no events" hint
            // — keeps the chart slot reserved at the right dimensions
            // so the layout doesn't jump. Skip the chart-build pass.
            let _ = root.draw(&Text::new(
                "no trajectory yet",
                (width as i32 / 2 - 50, height as i32 / 2 - 6),
                ("sans-serif", 11u32).into_font().color(&LABEL),
            ));
            let _ = root.present();
            // Drop root by returning from the inner block; falls through
            // to the function's tail return after the `{ … }` scope ends.
            return {
                drop(root);
                buf
            };
        }

        // Y range: span all rate values in series + events + reference
        // lines, with a 1pp padding band so dots aren't on the axis.
        let mut all_y: Vec<f64> = series.iter().map(|p| p.rate_pct).collect();
        all_y.extend(events.iter().map(|e| e.rate_pct));
        if let Some(b) = base_rate_pct {
            all_y.push(b);
        }
        if let Some(c) = crowd_price_pct {
            all_y.push(c);
        }
        if all_y.is_empty() {
            all_y.push(2.08); // base rate fallback
        }
        let y_min = (all_y.iter().cloned().fold(f64::INFINITY, f64::min) - 1.0).max(0.0);
        let y_max = all_y.iter().cloned().fold(f64::NEG_INFINITY, f64::max) + 1.0;

        // X range: 0 to (last - first) seconds. If only one point, give it
        // a 1-second window so the chart doesn't degenerate.
        let mut all_x: Vec<f64> = series.iter().map(|p| p.t_seconds).collect();
        all_x.extend(events.iter().map(|e| e.t_seconds));
        let x_min = all_x.iter().cloned().fold(f64::INFINITY, f64::min).min(0.0);
        let x_max_raw = all_x.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let x_max = if x_max_raw <= x_min {
            x_min + 1.0
        } else {
            x_max_raw
        };

        if let Ok(mut chart) = ChartBuilder::on(&root)
            .margin_top(8)
            .margin_right(12)
            .margin_bottom(8)
            .margin_left(8)
            .x_label_area_size(18)
            .y_label_area_size(34)
            .build_cartesian_2d(x_min..x_max, y_min..y_max)
        {
            // Format x-axis as days/hours since first event. Plotters
            // takes a closure that returns the formatted label.
            let span = x_max - x_min;
            let _ = chart
                .configure_mesh()
                .x_labels(5)
                .y_labels(4)
                .label_style(("sans-serif", 9).into_font().color(&LABEL))
                .axis_style(ShapeStyle::from(CHROME).stroke_width(1))
                .light_line_style(ShapeStyle::from(CHROME).stroke_width(1))
                .bold_line_style(ShapeStyle::from(CHROME).stroke_width(1))
                .y_label_formatter(&|v| format!("{:.0}%", v))
                .x_label_formatter(&|v| {
                    // Pick a sensible unit based on total span.
                    let secs = *v - x_min;
                    if span < 60.0 * 60.0 {
                        format!("{:.0}m", secs / 60.0)
                    } else if span < 24.0 * 60.0 * 60.0 {
                        format!("{:.0}h", secs / 3600.0)
                    } else {
                        format!("{:.0}d", secs / 86400.0)
                    }
                })
                .draw();

            // Reference: base-rate horizontal (outside view).
            if let Some(b) = base_rate_pct {
                let _ = chart.draw_series(LineSeries::new(
                    vec![(x_min, b), (x_max, b)],
                    ShapeStyle::from(GOLD).stroke_width(1),
                ));
            }
            // Reference: crowd price horizontal.
            if let Some(c) = crowd_price_pct {
                let _ = chart.draw_series(LineSeries::new(
                    vec![(x_min, c), (x_max, c)],
                    ShapeStyle::from(PURPLE).stroke_width(1),
                ));
            }

            // The worm: cyan trail through every rate revision in
            // chronological order. Width=2 so it reads as a deliberate
            // line, not just markers connected.
            if series.len() >= 2 {
                let _ = chart.draw_series(LineSeries::new(
                    series.iter().map(|p| (p.t_seconds, p.rate_pct)),
                    ShapeStyle::from(CYAN).stroke_width(2),
                ));
            }

            // Event markers — colored dots per kind.
            for ev in events {
                let (color, size) = match ev.kind {
                    TrajectoryEventKind::RateRevision => (CYAN, 4),
                    TrajectoryEventKind::BayesOpsFit => (GOLD, 4),
                    TrajectoryEventKind::AgentRun => (CHROME, 2),
                    TrajectoryEventKind::MarketObservation => (PURPLE, 3),
                };
                let _ = chart.draw_series(std::iter::once(Circle::new(
                    (ev.t_seconds, ev.rate_pct),
                    size,
                    ShapeStyle::from(color).filled(),
                )));
            }
        }
        let _ = root.present();
    }
    buf
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
                        d.name.chars().take(max_chars.saturating_sub(1)).collect::<String>() + "…"
                    } else {
                        d.name.clone()
                    };
                    let _ = root.draw(&Text::new(
                        label, (x as i32 + 4, 3),
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
