//! Axis and readout formatting.
//!
//! Lives in the lib target because "does a 240-million-dollar revenue
//! forecast render as `240.0M` or as `240000000.0`?" is a question with
//! a right answer, and the answer should be pinned by a test rather
//! than by squinting at a running app.

/// Compact number formatting for axis ends and hover readouts.
///
/// The console shows probabilities (`0.031`) and revenue figures
/// (`2.4e8`) in the same panel, so any single fixed precision is wrong
/// somewhere. Scale-adaptive formatting is the only honest option.
pub fn value(v: f64) -> String {
    if !v.is_finite() {
        return "—".to_string();
    }
    let a = v.abs();
    if a >= 1e9 {
        format!("{:.2}B", v / 1e9)
    } else if a >= 1e6 {
        format!("{:.1}M", v / 1e6)
    } else if a >= 1e3 {
        format!("{:.1}k", v / 1e3)
    } else if a >= 10.0 {
        format!("{:.1}", v)
    } else if a >= 0.01 {
        format!("{:.3}", v)
    } else if a > 0.0 {
        format!("{:.4}", v)
    } else {
        "0".to_string()
    }
}

/// Axis tick label for a time axis.
///
/// Calendar dates once the span is wide enough for them to carry
/// meaning, relative offsets when it isn't. A forecast that ran for
/// four minutes should not label its axis "Jun 17, Jun 17, Jun 17".
pub fn tick_time(
    t: f64,
    t0: f64,
    span: f64,
    epoch: Option<chrono::DateTime<chrono::Utc>>,
) -> String {
    if let Some(anchor) = epoch {
        if span >= 3600.0 {
            let ts = anchor + chrono::Duration::milliseconds((t * 1000.0) as i64);
            return if span >= 86400.0 {
                ts.format("%b %-d").to_string()
            } else {
                ts.format("%H:%M").to_string()
            };
        }
    }
    let rel = t - t0;
    if span < 60.0 {
        format!("{:.0}s", rel)
    } else if span < 3600.0 {
        format!("+{:.0}m", rel / 60.0)
    } else if span < 86400.0 {
        format!("+{:.1}h", rel / 3600.0)
    } else {
        format!("+{:.1}d", rel / 86400.0)
    }
}

/// Cursor-readout timestamp — as precise as the anchor allows.
pub fn cursor_time(t: f64, epoch: Option<chrono::DateTime<chrono::Utc>>) -> String {
    match epoch {
        Some(anchor) => (anchor + chrono::Duration::milliseconds((t * 1000.0) as i64))
            .format("%b %-d, %H:%M")
            .to_string(),
        None => format!("+{:.1}d", t / 86400.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn epoch() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-06-17T09:30:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc)
    }

    #[test]
    fn value_formatting_adapts_to_magnitude() {
        assert_eq!(value(0.0), "0");
        assert_eq!(value(0.0312), "0.031");
        assert_eq!(value(0.0009), "0.0009");
        assert_eq!(value(42.5), "42.5");
        assert_eq!(value(2_400.0), "2.4k");
        assert_eq!(value(240_000_000.0), "240.0M");
        assert_eq!(value(2.4e9), "2.40B");
    }

    #[test]
    fn value_formatting_handles_negatives_and_non_finite() {
        assert_eq!(value(-2.4e9), "-2.40B");
        assert_eq!(value(-0.031), "-0.031");
        assert_eq!(value(f64::NAN), "—");
        assert_eq!(value(f64::INFINITY), "—");
    }

    #[test]
    fn tick_time_uses_calendar_only_when_the_span_justifies_it() {
        // Multi-day span with an anchor → calendar date.
        assert_eq!(tick_time(0.0, 0.0, 200_000.0, Some(epoch())), "Jun 17");
        // Multi-hour but sub-day → wall-clock time.
        assert_eq!(tick_time(0.0, 0.0, 7_200.0, Some(epoch())), "09:30");
        // Sub-hour → relative, even though an anchor exists.
        assert_eq!(tick_time(30.0, 0.0, 50.0, Some(epoch())), "30s");
    }

    #[test]
    fn tick_time_falls_back_to_relative_without_an_anchor() {
        assert_eq!(tick_time(7200.0, 0.0, 200_000.0, None), "+0.1d");
        assert_eq!(tick_time(1800.0, 0.0, 3000.0, None), "+30m");
        assert_eq!(tick_time(7200.0, 0.0, 20_000.0, None), "+2.0h");
    }

    #[test]
    fn cursor_time_prefers_the_calendar_when_anchored() {
        assert_eq!(cursor_time(0.0, Some(epoch())), "Jun 17, 09:30");
        assert_eq!(cursor_time(3600.0, Some(epoch())), "Jun 17, 10:30");
        assert_eq!(cursor_time(86400.0, None), "+1.0d");
    }
}
