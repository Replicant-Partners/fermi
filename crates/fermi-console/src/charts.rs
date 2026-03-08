//! Chart rendering using plotters — produces RGB pixel buffers.
//! All charts use the Ayu Mirage theme palette for consistency with the GPUI UI.

use plotters::prelude::*;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════
// Ayu Mirage Theme Palette
// ═══════════════════════════════════════════════════════════════════

const BG_DEEP: RGBColor = RGBColor(13, 18, 30); // #0D121E
const BG: RGBColor = RGBColor(20, 27, 43); // #141B2B
const BG_ELEVATED: RGBColor = RGBColor(28, 37, 54); // #1C2536
const GRID_LINE: RGBColor = RGBColor(35, 46, 65); // #232E41
const AXIS: RGBColor = RGBColor(50, 63, 85); // #323F55
const TEXT_DIM: RGBColor = RGBColor(90, 105, 130); // #5A6982
const TEXT: RGBColor = RGBColor(140, 155, 175); // #8C9BAF

// Accent colors
const CYAN: RGBColor = RGBColor(54, 215, 183); // #36D7B7 — inside view, primary
const CYAN_DIM: RGBColor = RGBColor(30, 120, 100); // muted cyan for fills
const GOLD: RGBColor = RGBColor(245, 166, 35); // #F5A623 — outside view, warnings
const GOLD_DIM: RGBColor = RGBColor(140, 95, 20); // muted gold for fills
const GREEN: RGBColor = RGBColor(16, 185, 129); // #10B981 — good/high
const GREEN_DIM: RGBColor = RGBColor(10, 100, 70); // muted green
const RED: RGBColor = RGBColor(239, 68, 68); // #EF4444 — bad/low
const RED_DIM: RGBColor = RGBColor(130, 35, 35); // muted red
const BLUE: RGBColor = RGBColor(96, 165, 250); // #60A5FA — secondary accent
const PURPLE: RGBColor = RGBColor(167, 139, 250); // #A78BFA — tertiary

// ═══════════════════════════════════════════════════════════════════
// Data Types
// ═══════════════════════════════════════════════════════════════════

pub struct DriverViz {
    pub name: String,
    pub impact: f64,
    pub quality: f64,          // 0.0-1.0 overall evidence quality
    pub evidence: Vec<String>, // individual evidence summaries
}

pub struct IndexPoint {
    pub label: String,
    pub inside_view: f64,
    pub outside_view: f64,
}

/// Map evidence quality (0.0-1.0) to a color.
fn quality_color(q: f64) -> RGBColor {
    if q < 0.33 {
        RED
    } else if q < 0.66 {
        GOLD
    } else {
        GREEN
    }
}

/// Map evidence quality to a dim background color.
fn quality_bg(q: f64) -> RGBColor {
    if q < 0.33 {
        RED_DIM
    } else if q < 0.66 {
        GOLD_DIM
    } else {
        GREEN_DIM
    }
}

// ═══════════════════════════════════════════════════════════════════
// Index Comparison Chart (Inside vs Outside over versions)
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
        let _ = root.fill(&BG_DEEP);

        if !history.is_empty() {
            let min_v = history
                .iter()
                .flat_map(|p| [p.inside_view, p.outside_view])
                .fold(f64::INFINITY, f64::min)
                - 5.0;
            let max_v = history
                .iter()
                .flat_map(|p| [p.inside_view, p.outside_view])
                .fold(f64::NEG_INFINITY, f64::max)
                + 5.0;

            if let Ok(mut chart) = ChartBuilder::on(&root)
                .margin(8)
                .x_label_area_size(18)
                .y_label_area_size(36)
                .build_cartesian_2d(0usize..history.len().max(1), min_v..max_v)
            {
                let _ = chart
                    .configure_mesh()
                    .x_labels(5)
                    .y_labels(4)
                    .label_style(("sans-serif", 9).into_font().color(&TEXT_DIM))
                    .axis_style(ShapeStyle::from(AXIS).stroke_width(1))
                    .light_line_style(ShapeStyle::from(GRID_LINE).stroke_width(1))
                    .draw();

                // Outside view area fill (subtle)
                let _ = chart.draw_series(AreaSeries::new(
                    history.iter().enumerate().map(|(i, p)| (i, p.outside_view)),
                    min_v,
                    RGBAColor(245, 166, 35, 0.08),
                ));

                // Inside view area fill (subtle)
                let _ = chart.draw_series(AreaSeries::new(
                    history.iter().enumerate().map(|(i, p)| (i, p.inside_view)),
                    min_v,
                    RGBAColor(54, 215, 183, 0.12),
                ));

                // Outside view line (gold, dashed feel via thinner)
                let _ = chart.draw_series(LineSeries::new(
                    history.iter().enumerate().map(|(i, p)| (i, p.outside_view)),
                    ShapeStyle::from(GOLD).stroke_width(2),
                ));

                // Inside view line (cyan, bold)
                let _ = chart.draw_series(LineSeries::new(
                    history.iter().enumerate().map(|(i, p)| (i, p.inside_view)),
                    ShapeStyle::from(CYAN).stroke_width(2),
                ));

                // Current position markers
                if current_idx < history.len() {
                    let p = &history[current_idx];
                    let _ = chart.draw_series(std::iter::once(Circle::new(
                        (current_idx, p.inside_view),
                        5,
                        ShapeStyle::from(CYAN).filled(),
                    )));
                    let _ = chart.draw_series(std::iter::once(Circle::new(
                        (current_idx, p.outside_view),
                        4,
                        ShapeStyle::from(GOLD).filled(),
                    )));
                }

                // Version dots on inside line
                for (i, p) in history.iter().enumerate() {
                    if i != current_idx {
                        let _ = chart.draw_series(std::iter::once(Circle::new(
                            (i, p.inside_view),
                            2,
                            ShapeStyle::from(CYAN_DIM).filled(),
                        )));
                    }
                }
            }
        }
        let _ = root.present();
    }
    buf
}

// ═══════════════════════════════════════════════════════════════════
// Histogram (Simulation Distribution)
// ═══════════════════════════════════════════════════════════════════

pub fn render_histogram_chart(bins: &[u32], width: u32, height: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let _ = root.fill(&BG_DEEP);

        if !bins.is_empty() {
            let max_count = *bins.iter().max().unwrap_or(&1) as f64;
            let n = bins.len();

            if let Ok(mut chart) = ChartBuilder::on(&root)
                .margin(6)
                .x_label_area_size(14)
                .y_label_area_size(24)
                .build_cartesian_2d(0usize..n, 0.0..max_count * 1.05)
            {
                let _ = chart
                    .configure_mesh()
                    .x_labels(5)
                    .y_labels(3)
                    .label_style(("sans-serif", 8).into_font().color(&TEXT_DIM))
                    .axis_style(ShapeStyle::from(AXIS).stroke_width(1))
                    .light_line_style(ShapeStyle::from(GRID_LINE).stroke_width(1))
                    .draw();

                // Gradient from tails (dim) through body (blue) to peak (cyan)
                let _ = chart.draw_series(bins.iter().enumerate().map(|(i, &count)| {
                    let t = if n > 1 {
                        (i as f64 / (n - 1) as f64 - 0.5).abs() * 2.0
                    } else {
                        0.0
                    };
                    // t=0 at center, t=1 at edges
                    let color = if t > 0.7 {
                        // Tails — muted
                        BG_ELEVATED
                    } else if t > 0.4 {
                        // Shoulders — blue
                        BLUE
                    } else {
                        // Core — cyan
                        CYAN
                    };
                    Rectangle::new(
                        [(i, 0.0), (i + 1, count as f64)],
                        ShapeStyle::from(color).filled(),
                    )
                }));

                // Outline on top of bars for definition
                let _ = chart.draw_series(bins.iter().enumerate().map(|(i, &count)| {
                    Rectangle::new(
                        [(i, 0.0), (i + 1, count as f64)],
                        ShapeStyle::from(BG_DEEP).stroke_width(1),
                    )
                }));
            }
        }
        let _ = root.present();
    }
    buf
}

// ═══════════════════════════════════════════════════════════════════
// Treemap (Driver Impact × Evidence Quality Grid)
//
// A proper 2D squarified treemap where:
//   - Each driver gets a rectangle sized proportional to its IMPACT
//   - Inside each rectangle, a grid of small squares = individual evidence items
//   - Color of each evidence square = quality of that evidence
//   - Empty cells (no evidence) are dark/red-tinted
// ═══════════════════════════════════════════════════════════════════

/// Squarified treemap layout — assigns 2D rectangles to weighted items.
struct TreemapRect {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
}

fn layout_treemap(weights: &[f64], width: f64, height: f64) -> Vec<TreemapRect> {
    let n = weights.len();
    if n == 0 {
        return vec![];
    }
    if n == 1 {
        return vec![TreemapRect {
            x: 0.0,
            y: 0.0,
            w: width,
            h: height,
        }];
    }

    // Simple slice-and-dice: alternate horizontal and vertical splits
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return weights
            .iter()
            .enumerate()
            .map(|(i, _)| TreemapRect {
                x: 0.0,
                y: (i as f64 / n as f64) * height,
                w: width,
                h: height / n as f64,
            })
            .collect();
    }

    let mut rects = Vec::with_capacity(n);
    let horizontal = width >= height;

    if horizontal {
        // Split left-to-right
        let mut x = 0.0;
        for w in weights {
            let frac = w / total;
            let cell_w = frac * width;
            rects.push(TreemapRect {
                x,
                y: 0.0,
                w: cell_w,
                h: height,
            });
            x += cell_w;
        }
    } else {
        // Split top-to-bottom
        let mut y = 0.0;
        for w in weights {
            let frac = w / total;
            let cell_h = frac * height;
            rects.push(TreemapRect {
                x: 0.0,
                y,
                w: width,
                h: cell_h,
            });
            y += cell_h;
        }
    }

    // For better aspect ratios with 3+ items, do a 2-level split
    if n >= 4 && rects.iter().any(|r| r.w / r.h > 4.0 || r.h / r.w > 4.0) {
        // Split into two groups and recurse
        let mid = n / 2;
        let left_total: f64 = weights[..mid].iter().sum();
        let right_total: f64 = weights[mid..].iter().sum();
        let left_frac = left_total / total;

        let (lw, lh, rx, ry, rw, rh) = if horizontal {
            let lw = left_frac * width;
            (lw, height, lw, 0.0, width - lw, height)
        } else {
            let lh = left_frac * height;
            (width, lh, 0.0, lh, width, height - lh)
        };

        let left_rects = layout_treemap(&weights[..mid], lw, lh);
        let right_rects = layout_treemap(&weights[mid..], rw, rh);

        rects.clear();
        for r in left_rects {
            rects.push(r);
        }
        for r in right_rects {
            rects.push(TreemapRect {
                x: r.x + rx,
                y: r.y + ry,
                w: r.w,
                h: r.h,
            });
        }
    }

    rects
}

pub fn render_treemap(drivers: &[DriverViz], width: u32, height: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let _ = root.fill(&BG_DEEP);

        if !drivers.is_empty() {
            let pad = 4.0;
            let w = width as f64 - pad * 2.0;
            let h = height as f64 - pad * 2.0;

            let weights: Vec<f64> = drivers.iter().map(|d| d.impact.max(0.1)).collect();
            let rects = layout_treemap(&weights, w, h);

            for (i, (driver, rect)) in drivers.iter().zip(rects.iter()).enumerate() {
                let x0 = (pad + rect.x + 2.0) as i32;
                let y0 = (pad + rect.y + 2.0) as i32;
                let x1 = (pad + rect.x + rect.w - 2.0) as i32;
                let y1 = (pad + rect.y + rect.h - 2.0) as i32;

                if x1 <= x0 || y1 <= y0 {
                    continue;
                }

                // Cell background — slightly brighter than chart bg
                let _ = root.draw(&Rectangle::new(
                    [(x0, y0), (x1, y1)],
                    ShapeStyle::from(BG_ELEVATED).filled(),
                ));

                // Border colored by overall quality
                let border_col = quality_color(driver.quality);
                let _ = root.draw(&Rectangle::new(
                    [(x0, y0), (x1, y1)],
                    ShapeStyle::from(border_col).stroke_width(1),
                ));

                // Evidence grid inside the cell
                let ev_count = driver.evidence.len().max(1);
                let inner_x = x0 + 3;
                let inner_y;
                let inner_w = (x1 - x0 - 6) as f64;
                let inner_h;

                // Draw driver name at top if enough space
                let cell_h = (y1 - y0) as f64;
                let cell_w = (x1 - x0) as f64;
                if cell_h > 20.0 && cell_w > 30.0 {
                    let label: String = if driver.name.len() as f64 * 6.5 > cell_w {
                        driver
                            .name
                            .chars()
                            .take((cell_w / 6.5) as usize)
                            .collect::<String>()
                            + "…"
                    } else {
                        driver.name.clone()
                    };
                    let _ = root.draw(&Text::new(
                        label,
                        (x0 + 4, y0 + 3),
                        ("sans-serif", 10u32).into_font().color(&TEXT),
                    ));
                    inner_y = y0 + 16;
                    inner_h = (y1 - inner_y - 3) as f64;
                } else {
                    inner_y = y0 + 2;
                    inner_h = (y1 - inner_y - 2) as f64;
                }

                if inner_h <= 2.0 || inner_w <= 2.0 {
                    continue;
                }

                // Draw evidence squares in a grid
                // Each square represents one piece of evidence
                // Color = quality of that evidence
                let max_squares = 12; // max evidence items to show
                let count = ev_count.min(max_squares);

                // Calculate grid dimensions
                let cols = if inner_w > inner_h {
                    ((count as f64).sqrt().ceil() as usize).max(1)
                } else {
                    ((count as f64).sqrt().floor() as usize).max(1)
                };
                let rows = ((count as f64 / cols as f64).ceil() as usize).max(1);

                let sq_w = (inner_w / cols as f64).min(16.0);
                let sq_h = (inner_h / rows as f64).min(16.0);
                let sq_size = sq_w.min(sq_h).max(3.0);
                let gap = 2.0;

                for idx in 0..count {
                    let col = idx % cols;
                    let row = idx / cols;
                    let sx = inner_x as f64 + col as f64 * (sq_size + gap);
                    let sy = inner_y as f64 + row as f64 * (sq_size + gap);

                    if sx + sq_size > x1 as f64 || sy + sq_size > y1 as f64 {
                        break;
                    }

                    // Color: if we have this evidence item, color by quality; else dark
                    let sq_quality = if idx < driver.evidence.len() {
                        driver.quality // use overall quality for now
                    } else {
                        0.1 // no evidence — dark
                    };

                    let sq_col = quality_bg(sq_quality);
                    let _ = root.draw(&Rectangle::new(
                        [
                            (sx as i32, sy as i32),
                            ((sx + sq_size - 1.0) as i32, (sy + sq_size - 1.0) as i32),
                        ],
                        ShapeStyle::from(sq_col).filled(),
                    ));
                    // Bright border
                    let _ = root.draw(&Rectangle::new(
                        [
                            (sx as i32, sy as i32),
                            ((sx + sq_size - 1.0) as i32, (sy + sq_size - 1.0) as i32),
                        ],
                        ShapeStyle::from(quality_color(sq_quality)).stroke_width(1),
                    ));
                }

                // Impact value label in bottom-right if space
                if cell_h > 30.0 && cell_w > 50.0 {
                    let impact_label = format!("{:.1}", driver.impact);
                    let _ = root.draw(&Text::new(
                        impact_label,
                        (x1 - 28, y1 - 13),
                        ("sans-serif", 9u32).into_font().color(&TEXT_DIM),
                    ));
                }

                // Use alternating subtle colors for even/odd cells for visual distinction
                if i % 2 == 0 {
                    let _ = root.draw(&Rectangle::new(
                        [(x0, y0), (x1, y1)],
                        ShapeStyle::from(RGBAColor(255, 255, 255, 0.02)).filled(),
                    ));
                }
            }
        } // end if !drivers.is_empty()

        let _ = root.present();
    }
    buf
}

// ═══════════════════════════════════════════════════════════════════
// Utilities
// ═══════════════════════════════════════════════════════════════════

/// Convert an RGB pixel buffer to a GPUI RenderImage.
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

// ═══════════════════════════════════════════════════════════════════
// Distribution Sparkline (mini triangular PDF)
// ═══════════════════════════════════════════════════════════════════

pub fn render_distribution_sparkline(
    p5: f64,
    p50: f64,
    p95: f64,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height)).into_drawing_area();
        let _ = root.fill(&BG_DEEP);

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
                    .margin(2)
                    .build_cartesian_2d(p5..p95, 0.0..max_y * 1.1)
                {
                    // Area fill — cyan tint
                    let _ = chart.draw_series(AreaSeries::new(
                        points.iter().cloned(),
                        0.0,
                        RGBAColor(54, 215, 183, 0.15),
                    ));
                    // Line — cyan
                    let _ = chart.draw_series(LineSeries::new(
                        points.iter().cloned(),
                        ShapeStyle::from(CYAN).stroke_width(1),
                    ));
                    // p50 marker — green vertical line
                    let _ = chart.draw_series(std::iter::once(PathElement::new(
                        vec![(p50, 0.0), (p50, max_y)],
                        ShapeStyle::from(GREEN).stroke_width(1),
                    )));
                }
            }
        }
        let _ = root.present();
    }
    buf
}
