//! Chart rendering — plotters to RGB pixel buffers.
//!
//! Tufte rules: no fill, no gradient, no decoration.
//! Data is bright lines on a dark canvas. That's it.

use plotters::prelude::*;
use std::sync::Arc;

// Canvas backgrounds — match GPUI theme values exactly
const BG: RGBColor = RGBColor(23, 27, 36);          // standalone charts (index, histogram)
const BG_CARD: RGBColor = RGBColor(39, 45, 56);     // charts inside cards (sparklines) — matches theme::BG_ELEVATED
const CHROME: RGBColor = RGBColor(40, 47, 58);
const LABEL: RGBColor = RGBColor(92, 103, 115);

// Data colors — one per meaning
const CYAN: RGBColor = RGBColor(92, 207, 230);
const GOLD: RGBColor = RGBColor(255, 204, 102);
const GREEN: RGBColor = RGBColor(186, 230, 126);

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
}

// ═══════════════════════════════════════════════════════════════════
// Index Chart — Inside vs Outside view over versions
//
// Two clean lines. No fill. Dots at each version.
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
            let vals: Vec<f64> = history.iter()
                .flat_map(|p| [p.inside_view, p.outside_view])
                .collect();
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

                // Outside view — gold line, thinner
                let _ = chart.draw_series(LineSeries::new(
                    history.iter().enumerate().map(|(i, p)| (i, p.outside_view)),
                    ShapeStyle::from(GOLD).stroke_width(1),
                ));

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
