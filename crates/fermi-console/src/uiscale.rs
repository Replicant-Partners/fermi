//! The UI scale factor and the type scale, as plain numbers.
//!
//! # Why this module exists
//!
//! The console was authored with ~3,400 hardcoded `px(...)` literals, and a
//! type scale whose three most-used tiers were 9px, 10px and 11px. That is
//! below the legible floor for a monospace face at 1×, it packs six nominal
//! "tiers" into a 3px range (so it reads as noise rather than hierarchy),
//! and — worst — it was unfixable by the operator, because nothing in the
//! app could scale.
//!
//! Two separable changes fix that:
//!
//! 1. **A type scale with a legible floor.** [`TEXT_BASE_PX`] and its
//!    neighbours replace the raw literals. The smallest tier is 10px, body
//!    text is 13px, and consecutive tiers are far enough apart to actually
//!    signal hierarchy.
//!
//! 2. **One global scale factor.** Every length in the UI — type, padding,
//!    gaps, radii, fixed column widths — resolves through this factor, so
//!    changing it scales the interface *proportionally*. Text grows, but so
//!    does the box around it, which is what keeps labels from truncating
//!    and rows from overflowing.
//!
//! Everything here is `f32` and dependency-free so it is testable: the bin
//! target can't run `#[cfg(test)]` at all (see the crate docs on why), and
//! clamping, snapping and the monotonicity of the type scale are exactly
//! the sort of arithmetic that should not be verified by squinting at a
//! screenshot.
//!
//! `crate::ui` in the binary is the typed face of this module — it wraps
//! these numbers in GPUI's `Rems` so they can be handed to `Styled`
//! setters. Feature code should use that, not this.

use std::sync::atomic::{AtomicU32, Ordering};

// ─── Scale factor ───────────────────────────────────────────────────────────

/// Reference rem size, in real pixels, at scale 1.0.
///
/// Design-pixel values are divided by this to produce `rems`. It is 16.0
/// because that is GPUI's own default `rem_size`, so a `Window` renders
/// correctly even before the root view has set anything.
pub const BASE_REM: f32 = 16.0;

/// Smallest selectable scale. Below this the type scale drops back under
/// the legible floor, which is the bug this module exists to fix.
pub const SCALE_MIN: f32 = 0.90;

/// Largest selectable scale.
///
/// Set by the widest fixed-width surface in the console: the 720px team
/// modal. At 1.60 that is 1152px, which still fits the 1280px default
/// window with margin; at 1.80 it would be 1296px and overhang. Modal
/// *heights* are separately pinned in real pixels for the same reason —
/// see the note on `render_shortcuts_modal`.
///
/// 1.60 is not a small ceiling: it puts body text at 20.8px, roughly
/// double the 11px the console shipped with.
pub const SCALE_MAX: f32 = 1.60;

/// Increment for one press of Increase/Decrease UI Scale.
pub const SCALE_STEP: f32 = 0.10;

/// Default scale for a fresh install.
///
/// Deliberately not 1.0. The complaint this addresses is that the console
/// is hard to read *out of the box*, and a preference nobody discovers
/// does not fix that. 1.15 puts body text at ~15px and the smallest badge
/// numerals at ~11.5px, while still fitting the full cockpit layout in the
/// default window.
pub const SCALE_DEFAULT: f32 = 1.15;

/// Current scale as `f32::to_bits`, so it fits in an atomic.
///
/// Global rather than a field on `FermiConsole` because the scale is read
/// from ~3,400 call sites across five modules, many of them free functions
/// with no access to the view. One relaxed load beats that much plumbing,
/// and there is exactly one writer.
///
/// Zero is the "never initialised" sentinel — it is not a reachable scale,
/// since every stored value passes through [`set_scale`]'s clamp.
static SCALE_BITS: AtomicU32 = AtomicU32::new(0);

/// The active scale factor, always within `[SCALE_MIN, SCALE_MAX]`.
pub fn scale() -> f32 {
    match SCALE_BITS.load(Ordering::Relaxed) {
        0 => SCALE_DEFAULT,
        bits => f32::from_bits(bits),
    }
}

/// Set the scale, clamping into range. Returns the value actually stored.
pub fn set_scale(value: f32) -> f32 {
    // Snap to whole percent. Without this, stepping down and back up
    // accumulates float error until the readout says "114%".
    let clamped = (value.clamp(SCALE_MIN, SCALE_MAX) * 100.0).round() / 100.0;
    SCALE_BITS.store(clamped.to_bits(), Ordering::Relaxed);
    clamped
}

/// Step the scale by `delta`, clamped. Returns the new value.
pub fn nudge_scale(delta: f32) -> f32 {
    set_scale(scale() + delta)
}

/// Restore [`SCALE_DEFAULT`]. Returns it.
pub fn reset_scale() -> f32 {
    set_scale(SCALE_DEFAULT)
}

/// `true` when a further decrease would be a no-op, so controls can render
/// as disabled rather than silently doing nothing.
pub fn at_min() -> bool {
    scale() <= SCALE_MIN
}

/// `true` when a further increase would be a no-op.
pub fn at_max() -> bool {
    scale() >= SCALE_MAX
}

/// Human-readable scale, e.g. `"115%"`.
pub fn scale_label() -> String {
    format!("{}%", (scale() * 100.0).round() as i32)
}

/// Rem size in real pixels for the current scale — what the root view
/// hands `Window::set_rem_size` each frame.
pub fn rem_size_px() -> f32 {
    BASE_REM * scale()
}

/// A design-pixel length resolved eagerly to real pixels.
///
/// Only for geometry that must agree with hand-computed canvas
/// coordinates: `crate::viz` paints vector shapes at offsets it derives
/// from a spec, so the wrapper `div` has to be sized in real pixels that
/// match that spec exactly. Deferred `rems` would let the two disagree.
pub fn scaled_px(design_px: f32) -> f32 {
    design_px * scale()
}

// ─── Type scale ─────────────────────────────────────────────────────────────
//
// Design pixels at scale 1.0; the comment on each is what it renders as at
// the 1.15 default.
//
// The previous scale used 9 / 9.5 / 10 / 10.5 / 11 as five distinct tiers.
// Half-pixel steps are invisible, so they fold together here. The mapping
// from the old literals is intentionally lossy — these are the steps that
// actually communicate hierarchy.

/// 10px → 11.5. Count badges and superscripts. Numerals only; too small to
/// carry a word.
pub const TEXT_MICRO_PX: f32 = 10.0;
/// 11px → 12.6. Dense tabular metadata, timestamps, key pills.
pub const TEXT_XS_PX: f32 = 11.0;
/// 12px → 13.8. Secondary labels, chips, column headers.
pub const TEXT_SM_PX: f32 = 12.0;
/// 13px → 15.0. **Body default.** Anything read as a sentence belongs here
/// or above.
pub const TEXT_BASE_PX: f32 = 13.0;
/// 14px → 16.1. Emphasised body, primary values in a stat block.
pub const TEXT_MD_PX: f32 = 14.0;
/// 15px → 17.3. Card titles.
pub const TEXT_LG_PX: f32 = 15.0;
/// 16px → 18.4. Section headings.
pub const TEXT_XL_PX: f32 = 16.0;
/// 18px → 20.7. Panel headings.
pub const TEXT_2XL_PX: f32 = 18.0;
/// 20px → 23.0. Modal titles.
pub const TEXT_3XL_PX: f32 = 20.0;
/// 22px → 25.3.
pub const TEXT_4XL_PX: f32 = 22.0;
/// 24px → 27.6.
pub const TEXT_5XL_PX: f32 = 24.0;
/// 26px → 29.9.
pub const TEXT_6XL_PX: f32 = 26.0;
/// 30px → 34.5. Hero numerals and large glyphs.
pub const TEXT_7XL_PX: f32 = 30.0;
/// 34px → 39.1.
pub const TEXT_8XL_PX: f32 = 34.0;
/// 38px → 43.7. The splash mark.
pub const TEXT_9XL_PX: f32 = 38.0;

/// The full scale, ascending. Exists for the monotonicity test and for any
/// future type-scale inspector.
pub const TYPE_SCALE_PX: [f32; 15] = [
    TEXT_MICRO_PX,
    TEXT_XS_PX,
    TEXT_SM_PX,
    TEXT_BASE_PX,
    TEXT_MD_PX,
    TEXT_LG_PX,
    TEXT_XL_PX,
    TEXT_2XL_PX,
    TEXT_3XL_PX,
    TEXT_4XL_PX,
    TEXT_5XL_PX,
    TEXT_6XL_PX,
    TEXT_7XL_PX,
    TEXT_8XL_PX,
    TEXT_9XL_PX,
];

// ─── Persistence ────────────────────────────────────────────────────────────

/// `~/.config/fermi-console/ui.json`, or the platform equivalent.
///
/// Hand-rolled rather than pulling in `dirs`: one file, one field, and the
/// console has no other user-preference state to amortise the dependency
/// against.
fn prefs_path() -> Option<std::path::PathBuf> {
    let home = || std::env::var_os("HOME").map(std::path::PathBuf::from);
    let dir = if cfg!(target_os = "macos") {
        home()?.join("Library/Application Support/FermiConsole")
    } else if cfg!(target_os = "windows") {
        std::path::PathBuf::from(std::env::var_os("APPDATA")?).join("FermiConsole")
    } else {
        match std::env::var_os("XDG_CONFIG_HOME") {
            Some(x) => std::path::PathBuf::from(x),
            None => home()?.join(".config"),
        }
        .join("fermi-console")
    };
    Some(dir.join("ui.json"))
}

/// Load the persisted scale into the global. Call once, before the first
/// frame; falls back to [`SCALE_DEFAULT`].
///
/// `FERMI_UI_SCALE` overrides the stored value and is never written back,
/// so a screenshot run or a demo can pin a scale without clobbering the
/// operator's preference.
pub fn load_scale() {
    if let Some(forced) = std::env::var("FERMI_UI_SCALE")
        .ok()
        .and_then(|v| v.trim().parse::<f32>().ok())
    {
        set_scale(forced);
        return;
    }

    let stored = prefs_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| v.get("scale")?.as_f64());

    set_scale(stored.map(|v| v as f32).unwrap_or(SCALE_DEFAULT));
}

/// Persist the current scale.
///
/// Best-effort. A read-only or unwritable config directory costs the
/// operator their preference on next launch, which does not warrant
/// interrupting them with an error.
pub fn save_scale() {
    let Some(path) = prefs_path() else { return };
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log::warn!("could not create {}: {e}", parent.display());
            return;
        }
    }
    let body = serde_json::json!({ "scale": scale() });
    if let Err(e) = std::fs::write(&path, body.to_string()) {
        log::warn!("could not persist UI scale to {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The scale is process-global and `cargo test` runs these on separate
    // threads, so any test that mutates it has to hold this lock or it
    // will race the others into a spurious failure. Poisoning is ignored:
    // a panic in one test should surface as that test's failure, not as a
    // cascade of unrelated ones.
    static SCALE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn exclusive<T>(body: impl FnOnce() -> T) -> T {
        let _guard = SCALE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let out = body();
        reset_scale();
        out
    }

    #[test]
    fn clamps_out_of_range_input() {
        exclusive(|| {
            assert_eq!(set_scale(0.1), SCALE_MIN);
            assert!(at_min());
            assert_eq!(set_scale(99.0), SCALE_MAX);
            assert!(at_max());
        });
    }

    #[test]
    fn snaps_to_whole_percent() {
        exclusive(|| {
            // Stepping must not leave the label reading "114%".
            set_scale(1.0);
            for _ in 0..3 {
                nudge_scale(SCALE_STEP);
            }
            assert_eq!(scale_label(), "130%");
            reset_scale();
            assert_eq!(scale_label(), "115%");
        });
    }

    #[test]
    fn stepping_is_reversible() {
        exclusive(|| {
            set_scale(1.20);
            nudge_scale(SCALE_STEP);
            nudge_scale(-SCALE_STEP);
            assert_eq!(scale(), 1.20);
        });
    }

    #[test]
    fn type_scale_is_strictly_increasing() {
        for pair in TYPE_SCALE_PX.windows(2) {
            assert!(
                pair[1] > pair[0],
                "type scale must be monotonic: {} !> {}",
                pair[1],
                pair[0]
            );
        }
    }

    #[test]
    fn tiers_are_at_least_one_pixel_apart() {
        // A half-pixel step is invisible and just fragments the scale —
        // the exact failure the old 9/9.5/10/10.5 tiers had.
        for pair in TYPE_SCALE_PX.windows(2) {
            assert!(
                pair[1] - pair[0] >= 1.0,
                "tiers {} and {} are indistinguishable",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn body_text_clears_the_legibility_floor_at_every_scale() {
        // Body text must never render below 11px, even at SCALE_MIN.
        assert!(TEXT_BASE_PX * SCALE_MIN >= 11.0);
        // And the smallest tier we ship — badge numerals only — stays at
        // or above 9px.
        assert!(TEXT_MICRO_PX * SCALE_MIN >= 9.0);
    }

    #[test]
    fn default_scale_is_within_selectable_range() {
        assert!((SCALE_MIN..=SCALE_MAX).contains(&SCALE_DEFAULT));
    }

    #[test]
    fn widest_surface_still_fits_the_default_window_at_max_scale() {
        // The console's widest fixed-width surface (the 720px team modal)
        // and its sidebar have to coexist inside the default window at
        // any scale the user can select — otherwise the far edge is
        // simply unreachable. If a wider panel is added, this fails and
        // `SCALE_MAX` needs revisiting, which is the point.
        const WIDEST_PANEL_PX: f32 = 720.0;
        const DEFAULT_WINDOW_W: f32 = 1280.0;
        assert!(
            WIDEST_PANEL_PX * SCALE_MAX <= DEFAULT_WINDOW_W,
            "{WIDEST_PANEL_PX}px at {SCALE_MAX}× is {}px, wider than the \
             {DEFAULT_WINDOW_W}px default window",
            WIDEST_PANEL_PX * SCALE_MAX
        );
    }

    #[test]
    fn every_step_from_min_to_max_is_reachable() {
        // Stepping up from the floor must actually terminate at the
        // ceiling. A step size that doesn't divide the range leaves the
        // top unreachable except by `reset`.
        exclusive(|| {
            set_scale(SCALE_MIN);
            let mut seen = 0;
            while !at_max() && seen < 100 {
                nudge_scale(SCALE_STEP);
                seen += 1;
            }
            assert!(at_max(), "never reached SCALE_MAX after {seen} steps");
            while !at_min() && seen < 200 {
                nudge_scale(-SCALE_STEP);
                seen += 1;
            }
            assert!(at_min(), "never returned to SCALE_MIN");
        });
    }

    #[test]
    fn scaled_px_tracks_the_factor() {
        exclusive(|| {
            set_scale(1.5);
            assert_eq!(scaled_px(100.0), 150.0);
            assert_eq!(rem_size_px(), 24.0);
        });
    }
}
