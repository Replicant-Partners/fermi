//! Index Comparison + Evidence Treemap — rendered to SVG via Plotters
//!
//! Usage:
//!   cargo run -- --time 12          # render at month index 12
//!   cargo run -- --time 12 --out my.svg
//!   cargo run -- --all              # render all 24 months to out/frame_XX.svg

use plotters::prelude::*;
use rand::{Rng, SeedableRng};
use rand::rngs::SmallRng;
use std::path::PathBuf;

// ─── constants ────────────────────────────────────────────────────────────────
const PERIODS: usize = 24;
const W: u32 = 1200;
const H: u32 = 900;

// ─── data generation ──────────────────────────────────────────────────────────
fn seeded_index(seed: u64, start: f64, volatility: f64) -> Vec<f64> {
    let mut rng = SmallRng::seed_from_u64(seed);
    let mut vals = vec![start];
    for i in 1..=PERIODS {
        let change = (rng.gen::<f64>() - 0.45) * volatility;
        vals.push(f64::max(10.0, vals[i - 1] + change));
    }
    vals
}

fn month_label(i: usize) -> String {
    let months = ["Jan","Feb","Mar","Apr","May","Jun",
                  "Jul","Aug","Sep","Oct","Nov","Dec"];
    let m = (i % 12) as usize;
    let y = 24 + (i / 12) as u32;
    format!("{} '{}", months[m], y)
}

// ─── driver model ─────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
struct Driver {
    id:      &'static str,
    label:   &'static str,
    impact:  f64,         // treemap size dimension
    quality: f64,         // 0.0 → 1.0  (low → high evidence quality)
    evidence: Vec<&'static str>,
}

static BASE_DRIVERS: &[(&str, &str, f64, &[&str])] = &[
    ("macro",     "Macro Environment",   28.0, &["Fed rate +50bp","CPI 3.1% YoY","GDP revision up","PMI 54.2","Yield curve steep"]),
    ("earnings",  "Earnings Quality",    22.0, &["Rev beat 8/10","Margin compress","EPS raised","Analyst upgrades","FCF improving"]),
    ("sentiment", "Market Sentiment",    18.0, &["AAII bulls 42%","Put/call 0.78","Skew flatten","Retail flows high","Social spike"]),
    ("liquidity", "Liquidity Conds.",    14.0, &["Spreads tighten","Dark pool 38%","Repo stable","MF inflows $4B","Short int. down"]),
    ("tech",      "Technical Signals",   10.0, &["50DMA crossover","RSI overbought","Vol confirm","Range breakout","MACD positive"]),
    ("risk",      "Tail Risk",            8.0, &["VIX contango","Geo premium low","Credit tighten","Corr. break","Hedge expensive"]),
];

fn drivers_for_time(t: usize) -> Vec<Driver> {
    let mut rng = SmallRng::seed_from_u64((t * 37 + 11) as u64);
    let mut drivers: Vec<Driver> = BASE_DRIVERS.iter().map(|(id, label, base, ev)| {
        let impact_mod = 0.7 + rng.gen::<f64>() * 0.6;
        let quality    = 0.2 + rng.gen::<f64>() * 0.8;
        let n_ev       = 2 + (rng.gen::<f64>() * 3.0) as usize;
        Driver {
            id, label,
            impact:   base * impact_mod,
            quality,
            evidence: ev[..n_ev.min(ev.len())].to_vec(),
        }
    }).collect();
    drivers.sort_by(|a, b| b.impact.partial_cmp(&a.impact).unwrap());
    drivers
}

// ─── colour helpers ────────────────────────────────────────────────────────────
fn quality_color(q: f64) -> RGBColor {
    if q < 0.33 { RGBColor(239, 68,  68)  }       // red
    else if q < 0.66 { RGBColor(245, 158, 11) }   // amber
    else { RGBColor(16,  185, 129) }               // emerald
}

fn darken(c: RGBColor, factor: f64) -> RGBColor {
    RGBColor(
        (c.0 as f64 * factor) as u8,
        (c.1 as f64 * factor) as u8,
        (c.2 as f64 * factor) as u8,
    )
}

// ─── treemap layout (simple strip / squarify) ─────────────────────────────────
#[derive(Debug, Clone)]
struct Rect { x: f64, y: f64, w: f64, h: f64 }

fn layout_treemap(drivers: &[Driver], bounds: Rect) -> Vec<(Driver, Rect)> {
    let total: f64 = drivers.iter().map(|d| d.impact).sum();
    let mut result = Vec::new();
    let mut items  = drivers.to_vec();
    let mut x = bounds.x;
    let mut y = bounds.y;
    let mut rem_w = bounds.w;
    let mut rem_h = bounds.h;

    while !items.is_empty() {
        let horizontal = rem_w >= rem_h;
        let dim_main   = if horizontal { rem_w } else { rem_h };
        let dim_cross  = if horizontal { rem_h } else { rem_w };

        // find best row
        let mut row_items: Vec<Driver> = Vec::new();
        let mut best_ratio = f64::INFINITY;

        for i in 0..items.len() {
            let candidate = &items[..=i];
            let c_impact: f64 = candidate.iter().map(|d| d.impact).sum();
            let frac = c_impact / total;
            let cross_len = frac * dim_cross;
            let max_ratio = candidate.iter().map(|d| {
                let main_len = (d.impact / c_impact) * dim_main;
                f64::max(cross_len / main_len, main_len / cross_len)
            }).fold(f64::NEG_INFINITY, f64::max);

            if max_ratio > best_ratio && i > 0 { break; }
            best_ratio = max_ratio;
            row_items  = candidate.to_vec();
        }

        let row_impact: f64 = row_items.iter().map(|d| d.impact).sum();
        let frac      = row_impact / total;
        let cross_len = frac * dim_cross;
        let mut pos   = 0.0_f64;

        for d in &row_items {
            let main_len = (d.impact / row_impact) * dim_main;
            let r = if horizontal {
                Rect { x: x + pos, y, w: main_len, h: cross_len }
            } else {
                Rect { x, y: y + pos, w: cross_len, h: main_len }
            };
            result.push((d.clone(), r));
            pos += main_len;
        }

        let n = row_items.len();
        items.drain(..n);
        if horizontal { y += cross_len; rem_h -= cross_len; }
        else          { x += cross_len; rem_w -= cross_len; }
    }
    result
}

// ─── drawing ──────────────────────────────────────────────────────────────────
fn render(time_idx: usize, out: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let root = SVGBackend::new(out, (W, H)).into_drawing_area();
    root.fill(&RGBColor(15, 23, 42))?;   // slate-900

    let idx_a = seeded_index(42, 100.0, 12.0);
    let idx_b = seeded_index(99,  90.0, 18.0);
    let drivers = drivers_for_time(time_idx);

    // ── layout zones ─────────────────────────────────────────────────────────
    let margin   = 30_i32;
    let chart_h  = 220_i32;
    let tree_h   = 350_i32;
    let evid_h   = 180_i32;
    let gap      = 16_i32;
    let inner_w  = W as i32 - margin * 2;

    // ── 1. Header ─────────────────────────────────────────────────────────────
    let header = root.titled(
        &format!("Index Comparison + Evidence Treemap  —  {}", month_label(time_idx)),
        ("sans-serif", 22).into_font().color(&RGBColor(241, 245, 249)),
    )?;
    let _ = header; // consumed by titled()

    // ── 2. Chart area ─────────────────────────────────────────────────────────
    let chart_top = margin + 36;
    let chart_area = root.margin(chart_top as u32, (H as i32 - chart_top - chart_h) as u32,
                                  margin as u32, margin as u32);

    let all_vals: Vec<f64> = idx_a.iter().chain(idx_b.iter()).cloned().collect();
    let min_v = all_vals.iter().cloned().fold(f64::INFINITY, f64::min) - 5.0;
    let max_v = all_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max) + 5.0;

    let mut chart = ChartBuilder::on(&chart_area)
        .margin(5)
        .x_label_area_size(28)
        .y_label_area_size(50)
        .build_cartesian_2d(0usize..PERIODS, min_v..max_v)?;

    chart.configure_mesh()
        .x_labels(7)
        .x_label_formatter(&|x| month_label(*x))
        .y_labels(5)
        .label_style(("sans-serif", 10).into_font().color(&RGBColor(100, 116, 139)))
        .axis_style(ShapeStyle::from(RGBColor(51, 65, 85)).stroke_width(1))
        .light_line_style(ShapeStyle::from(RGBColor(30, 41, 59)).stroke_width(1))
        .bold_line_style(ShapeStyle::from(RGBColor(30, 41, 59)).stroke_width(1))
        .draw()?;

    // area fills
    chart.draw_series(AreaSeries::new(
        (0..=PERIODS).map(|i| (i, idx_a[i])),
        min_v,
        RGBAColor(59, 130, 246, 0.15),
    ))?;
    chart.draw_series(AreaSeries::new(
        (0..=PERIODS).map(|i| (i, idx_b[i])),
        min_v,
        RGBAColor(245, 158, 11, 0.15),
    ))?;

    // lines
    chart.draw_series(LineSeries::new(
        (0..=PERIODS).map(|i| (i, idx_a[i])),
        ShapeStyle::from(RGBColor(59, 130, 246)).stroke_width(2),
    ))?.label("Index A")
      .legend(|(x, y)| PathElement::new(vec![(x,y),(x+20,y)],
          ShapeStyle::from(RGBColor(59,130,246)).stroke_width(2)));

    chart.draw_series(LineSeries::new(
        (0..=PERIODS).map(|i| (i, idx_b[i])),
        ShapeStyle::from(RGBColor(245, 158, 11)).stroke_width(2),
    ))?.label("Index B")
      .legend(|(x, y)| PathElement::new(vec![(x,y),(x+20,y)],
          ShapeStyle::from(RGBColor(245,158,11)).stroke_width(2)));

    // time cursor
    chart.draw_series(std::iter::once(
        PathElement::new(
            vec![(time_idx, min_v), (time_idx, max_v)],
            ShapeStyle::from(RGBColor(226, 232, 240)).stroke_width(1),
        )
    ))?;

    // dots at cursor
    chart.draw_series(std::iter::once(Circle::new(
        (time_idx, idx_a[time_idx]), 5,
        ShapeStyle::from(RGBColor(59, 130, 246)).filled(),
    )))?;
    chart.draw_series(std::iter::once(Circle::new(
        (time_idx, idx_b[time_idx]), 5,
        ShapeStyle::from(RGBColor(245, 158, 11)).filled(),
    )))?;

    chart.configure_series_labels()
        .background_style(RGBColor(30, 41, 59))
        .border_style(RGBColor(51, 65, 85))
        .label_font(("sans-serif", 11).into_font().color(&RGBColor(226, 232, 240)))
        .draw()?;

    // ── 3. Treemap ────────────────────────────────────────────────────────────
    let tree_top = chart_top + chart_h + gap;
    let pad = 4.0_f64;

    let tree_bounds = Rect {
        x: margin as f64,
        y: tree_top as f64,
        w: inner_w as f64,
        h: tree_h as f64,
    };
    let tiles = layout_treemap(&drivers, tree_bounds);

    for (driver, r) in &tiles {
        let col   = quality_color(driver.quality);
        let dark  = darken(col, 0.6);

        // fill rect
        root.draw(&Rectangle::new(
            [((r.x + pad) as i32, (r.y + pad) as i32),
             ((r.x + r.w - pad) as i32, (r.y + r.h - pad) as i32)],
            ShapeStyle::from(col).filled(),
        ))?;
        // border
        root.draw(&Rectangle::new(
            [((r.x + pad) as i32, (r.y + pad) as i32),
             ((r.x + r.w - pad) as i32, (r.y + r.h - pad) as i32)],
            ShapeStyle::from(dark).stroke_width(1),
        ))?;

        // labels (only if cell is big enough)
        if r.w > 80.0 && r.h > 40.0 {
            let cx = (r.x + r.w / 2.0) as i32;
            let cy = (r.y + r.h / 2.0) as i32;
            let font_sz = f64::min(14.0, r.w / 8.0) as u32;

            root.draw(&Text::new(
                driver.label.to_string(),
                (cx - (driver.label.len() as i32 * font_sz as i32 / 4), cy - 10),
                ("sans-serif", font_sz).into_font().color(&WHITE),
            ))?;
            let sub = format!("Impact: {:.0}  |  Confidence: {:.0}%",
                              driver.impact, driver.quality * 100.0);
            root.draw(&Text::new(
                sub,
                (cx - 60, cy + 8),
                ("sans-serif", 9u32).into_font().color(&RGBAColor(255,255,255,0.75)),
            ))?;

            // evidence chips (small text rows)
            for (j, ev) in driver.evidence.iter().enumerate() {
                let ey = cy + 22 + j as i32 * 14;
                if ey < (r.y + r.h - pad - 4.0) as i32 {
                    root.draw(&Text::new(
                        format!("• {}", ev),
                        (cx - 55, ey),
                        ("sans-serif", 8u32).into_font().color(&RGBAColor(255,255,255,0.6)),
                    ))?;
                }
            }
        }
    }

    // treemap legend
    let leg_y = tree_top + tree_h + 6;
    for (i, (label, col)) in [
        ("Low Evidence",    RGBColor(239,  68,  68)),
        ("Medium Evidence", RGBColor(245, 158,  11)),
        ("High Evidence",   RGBColor( 16, 185, 129)),
    ].iter().enumerate() {
        let lx = margin + i as i32 * 160;
        root.draw(&Rectangle::new(
            [(lx, leg_y), (lx + 12, leg_y + 12)],
            ShapeStyle::from(*col).filled(),
        ))?;
        root.draw(&Text::new(
            label.to_string(),
            (lx + 16, leg_y),
            ("sans-serif", 10u32).into_font().color(&RGBColor(148,163,184)),
        ))?;
    }
    root.draw(&Text::new(
        "Block size = driver impact".to_string(),
        (margin + 520, leg_y),
        ("sans-serif", 10u32).into_font().color(&RGBColor(100,116,139)),
    ))?;

    // ── 4. Evidence panel ─────────────────────────────────────────────────────
    let evid_top = tree_top + tree_h + gap + 20;

    // panel background
    root.draw(&Rectangle::new(
        [(margin, evid_top), (margin + inner_w, evid_top + evid_h)],
        ShapeStyle::from(RGBColor(17, 24, 39)).filled(),
    ))?;
    root.draw(&Rectangle::new(
        [(margin, evid_top), (margin + inner_w, evid_top + evid_h)],
        ShapeStyle::from(RGBColor(51, 65, 85)).stroke_width(1),
    ))?;

    root.draw(&Text::new(
        "Evidence Detail  (top drivers at this period)".to_string(),
        (margin + 10, evid_top + 10),
        ("sans-serif", 11u32).into_font().color(&RGBColor(148,163,184)),
    ))?;

    // show top-3 drivers as columns
    let col_w = inner_w / 3;
    for (col_i, driver) in drivers.iter().take(3).enumerate() {
        let col_x = margin + col_i as i32 * col_w + 8;
        let col_start = evid_top + 28;
        let col_color = quality_color(driver.quality);

        root.draw(&Rectangle::new(
            [(col_x, col_start), (col_x + col_w - 16, col_start + 4)],
            ShapeStyle::from(col_color).filled(),
        ))?;

        root.draw(&Text::new(
            format!("{} — impact {:.0}", driver.label, driver.impact),
            (col_x, col_start + 12),
            ("sans-serif", 11u32).into_font().color(&WHITE),
        ))?;

        let qual_label = if driver.quality < 0.33 { "Low" }
                         else if driver.quality < 0.66 { "Medium" } else { "High" };
        root.draw(&Text::new(
            format!("Evidence quality: {} ({:.0}%)", qual_label, driver.quality * 100.0),
            (col_x, col_start + 26),
            ("sans-serif", 9u32).into_font().color(&RGBAColor(
                col_color.0, col_color.1, col_color.2, 1.0)),
        ))?;

        // quality bar
        let bar_full = col_w - 24;
        let bar_fill = (bar_full as f64 * driver.quality) as i32;
        root.draw(&Rectangle::new(
            [(col_x, col_start + 36), (col_x + bar_full, col_start + 43)],
            ShapeStyle::from(RGBColor(30, 41, 59)).filled(),
        ))?;
        root.draw(&Rectangle::new(
            [(col_x, col_start + 36), (col_x + bar_fill, col_start + 43)],
            ShapeStyle::from(col_color).filled(),
        ))?;

        for (j, ev) in driver.evidence.iter().enumerate() {
            root.draw(&Text::new(
                format!("▸ {}", ev),
                (col_x, col_start + 54 + j as i32 * 16),
                ("sans-serif", 9u32).into_font().color(&RGBColor(203,213,225)),
            ))?;
        }
    }

    root.present()?;
    println!("✓ Rendered t={} → {}", time_idx, out.display());
    Ok(())
}

// ─── CLI ──────────────────────────────────────────────────────────────────────
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // --all  → render every frame
    if args.iter().any(|a| a == "--all") {
        std::fs::create_dir_all("out")?;
        for t in 0..=PERIODS {
            let path = PathBuf::from(format!("out/frame_{:02}.svg", t));
            render(t, &path)?;
        }
        println!("All {} frames written to out/", PERIODS + 1);
        return Ok(());
    }

    // --time N  (default 12)
    let time_idx = args.windows(2)
        .find(|w| w[0] == "--time")
        .and_then(|w| w[1].parse::<usize>().ok())
        .unwrap_or(12)
        .min(PERIODS);

    // --out path.svg  (default output.svg)
    let out = args.windows(2)
        .find(|w| w[0] == "--out")
        .map(|w| PathBuf::from(&w[1]))
        .unwrap_or_else(|| PathBuf::from("output.svg"));

    render(time_idx, &out)?;
    Ok(())
}