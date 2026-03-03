//! Chart rendering using plotters — produces RGB pixel buffers.

use plotters::prelude::*;

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

fn quality_color(q: f64) -> RGBColor {
    if q < 0.33 { RGBColor(239, 68, 68) }
    else if q < 0.66 { RGBColor(245, 158, 11) }
    else { RGBColor(16, 185, 129) }
}

pub fn render_index_chart(
    history: &[IndexPoint],
    current_idx: usize,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height))
            .into_drawing_area();
        let _ = root.fill(&RGBColor(15, 23, 42));

        if !history.is_empty() {
            let min_v = history.iter()
                .flat_map(|p| [p.inside_view, p.outside_view])
                .fold(f64::INFINITY, f64::min) - 5.0;
            let max_v = history.iter()
                .flat_map(|p| [p.inside_view, p.outside_view])
                .fold(f64::NEG_INFINITY, f64::max) + 5.0;

            if let Ok(mut chart) = ChartBuilder::on(&root)
                .margin(10)
                .x_label_area_size(20)
                .y_label_area_size(40)
                .build_cartesian_2d(0usize..history.len().max(1), min_v..max_v)
            {
                let _ = chart.configure_mesh()
                    .x_labels(5).y_labels(5)
                    .label_style(("sans-serif", 10).into_font().color(&RGBColor(100, 116, 139)))
                    .axis_style(ShapeStyle::from(RGBColor(51, 65, 85)).stroke_width(1))
                    .light_line_style(ShapeStyle::from(RGBColor(30, 41, 59)).stroke_width(1))
                    .draw();

                let _ = chart.draw_series(LineSeries::new(
                    history.iter().enumerate().map(|(i, p)| (i, p.inside_view)),
                    ShapeStyle::from(RGBColor(59, 130, 246)).stroke_width(2),
                ));
                let _ = chart.draw_series(LineSeries::new(
                    history.iter().enumerate().map(|(i, p)| (i, p.outside_view)),
                    ShapeStyle::from(RGBColor(245, 158, 11)).stroke_width(2),
                ));

                if current_idx < history.len() {
                    let p = &history[current_idx];
                    let _ = chart.draw_series(std::iter::once(Circle::new(
                        (current_idx, p.inside_view), 5,
                        ShapeStyle::from(RGBColor(59, 130, 246)).filled(),
                    )));
                    let _ = chart.draw_series(std::iter::once(Circle::new(
                        (current_idx, p.outside_view), 5,
                        ShapeStyle::from(RGBColor(245, 158, 11)).filled(),
                    )));
                }
            }
        }
        let _ = root.present();
    }
    buf
}

pub fn render_histogram_chart(
    bins: &[u32],
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height))
            .into_drawing_area();
        let _ = root.fill(&RGBColor(15, 23, 42));

        if !bins.is_empty() {
            let max_count = *bins.iter().max().unwrap_or(&1) as f64;
            if let Ok(mut chart) = ChartBuilder::on(&root)
                .margin(8)
                .x_label_area_size(16)
                .y_label_area_size(28)
                .build_cartesian_2d(0usize..bins.len(), 0.0..max_count * 1.1)
            {
                let _ = chart.configure_mesh()
                    .x_labels(5).y_labels(4)
                    .label_style(("sans-serif", 9).into_font().color(&RGBColor(100, 116, 139)))
                    .axis_style(ShapeStyle::from(RGBColor(51, 65, 85)).stroke_width(1))
                    .light_line_style(ShapeStyle::from(RGBColor(30, 41, 59)).stroke_width(1))
                    .draw();

                let n = bins.len();
                let _ = chart.draw_series(
                    bins.iter().enumerate().map(|(i, &count)| {
                        let color = if i < n / 4 || i > n * 3 / 4 {
                            RGBColor(51, 65, 85)
                        } else if i < n * 2 / 5 || i > n * 3 / 5 {
                            RGBColor(59, 130, 246)
                        } else {
                            RGBColor(16, 185, 129)
                        };
                        Rectangle::new([(i, 0.0), (i + 1, count as f64)],
                            ShapeStyle::from(color).filled())
                    })
                );
            }
        }
        let _ = root.present();
    }
    buf
}

pub fn render_treemap(
    drivers: &[DriverViz],
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut buf = vec![0u8; (width * height * 3) as usize];
    {
        let root = BitMapBackend::with_buffer(&mut buf, (width, height))
            .into_drawing_area();
        let _ = root.fill(&RGBColor(15, 23, 42));

        if !drivers.is_empty() {
            let total: f64 = drivers.iter().map(|d| d.impact).sum();
            if total > 0.0 {
                let mut y = 4.0_f64;
                let w = width as f64 - 8.0;
                let h = height as f64 - 8.0;

                for driver in drivers {
                    let frac = driver.impact / total;
                    let cell_h = frac * h;
                    let col = quality_color(driver.quality);

                    let _ = root.draw(&Rectangle::new(
                        [(6, (y + 2.0) as i32), ((w + 2.0) as i32, (y + cell_h - 2.0) as i32)],
                        ShapeStyle::from(col).filled(),
                    ));

                    if cell_h > 24.0 {
                        let _ = root.draw(&Text::new(
                            driver.name.clone(),
                            (10, (y + 6.0) as i32),
                            ("sans-serif", 11u32).into_font().color(&WHITE),
                        ));
                    }

                    y += cell_h;
                }
            }
        }
        let _ = root.present();
    }
    buf
}
