//! Legacy chart rendering — plotters to RGB pixel buffers.
//!
//! # Status: being retired
//!
//! Only the index chart still lives here. The histogram, the driver
//! sparkline and the trajectory worm have moved to [`crate::viz`],
//! which paints vectors straight into the GPUI scene.
//!
//! The reason for the migration is mechanical, not aesthetic. Wrapping
//! a rasterised buffer in an `Arc<RenderImage>` mints a fresh `ImageId`
//! every call, `Window::paint_image` allocates a sprite-atlas tile per
//! id, and `ImageSource::Render` is excluded from asset cleanup — so
//! every re-render leaks a tile that is never reclaimed. Under hover,
//! which re-renders continuously, the atlas churns and the chart
//! flickers. Bitmaps also ignore the display scale factor (blurry on
//! HiDPI) and cost a full CPU rasterisation per frame.
//!
//! The index chart is the last holdout; see
//! `docs/fermi/VISUALIZATION_ARCHITECTURE.md` for the plan.
//!
//! Tufte rules still apply: no fill, no gradient, no decoration.

use plotters::prelude::*;
use std::sync::Arc;

// Canvas background — matches theme::BG (0x1F2430) exactly.
const BG: RGBColor = RGBColor(31, 36, 48);
const CHROME: RGBColor = RGBColor(50, 58, 72);
const LABEL: RGBColor = RGBColor(92, 103, 115);

// Data colors — one per meaning
const CYAN: RGBColor = RGBColor(92, 207, 230); // inside view / your model
const GOLD: RGBColor = RGBColor(255, 204, 102); // base rate / reference
const PURPLE: RGBColor = RGBColor(212, 191, 255); // crowd price (Polymarket)

// ════════════════════════════════════════════════════════════════════
// Public data types
// ════════════════════════════════════════════════════════════════════

pub struct IndexPoint {
    /// Version label. Carried for parity with the interactive overlay's
    /// per-column tooltips, which render it alongside the chart.
    #[allow(dead_code)]
    pub label: String,
    pub inside_view: f64,
    pub outside_view: f64,
    pub crowd_price: Option<f64>, // Polymarket crowd-implied probability
}

// The trajectory data types that used to live here now live in
// `fermi_console::plot::trajectory`, alongside the geometry that
// interprets them — and, crucially, alongside tests that can actually
// run. See `viz::trajectory`.

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
