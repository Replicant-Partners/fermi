//! Design tokens, typed for GPUI.
//!
//! This is the face feature code uses. It wraps the plain numbers in
//! [`fermi_console::uiscale`] — which owns the scale factor, the type
//! scale and the rationale for both — in the `Rems` GPUI wants, so they
//! can go straight into `Styled` setters.
//!
//! ```ignore
//! div()
//!     .px(ui::s(12.0))          // layout, in design pixels
//!     .gap(ui::s(6.0))
//!     .text_size(ui::TEXT_BASE) // type, from the scale
//! ```
//!
//! # Design pixels
//!
//! The unit for [`s`] is the **design pixel**: the value the layout was
//! drawn at, i.e. what it renders as when the scale is exactly 1.0. Call
//! sites therefore read the way they always did (`s(12.0)` where they used
//! to say `px(12.0)`) and [`uiscale::BASE_REM`] never leaks into feature
//! code.
//!
//! # Why `rems` and not `px`
//!
//! GPUI resolves `rems` against `Window::rem_size` at layout time, and the
//! root view sets that from [`rem_size`] on every frame. Expressing *all*
//! lengths this way — not just type — is what makes scaling proportional:
//! the text grows and so does the box around it, so nothing truncates,
//! wraps unexpectedly, or overflows its row. Scaling only the font would
//! have traded one readability bug for a layout one.
//!
//! # The exception
//!
//! `crate::viz` paints vector geometry into a canvas at coordinates it
//! computes from a spec, so its wrapper `div` must be sized in real pixels
//! that match that spec exactly. Those call sites size the spec with
//! [`sp`] — same factor, resolved eagerly to `f32` — and keep `px(...)`
//! on the wrapper.

use fermi_console::uiscale;
use gpui::{px, Pixels, Rems};

// ─── Lengths ────────────────────────────────────────────────────────────────

/// A scalable length, authored in design pixels.
///
/// The drop-in replacement for `px(..)` on every `Styled` setter: padding,
/// gaps, margins, corner radii, fixed widths and heights. Use it for
/// *layout*; use the `TEXT_*` constants for type, so the legibility floor
/// stays defined in one place.
pub const fn s(design_px: f32) -> Rems {
    Rems(design_px / uiscale::BASE_REM)
}

/// A scaled length resolved eagerly to real pixels, for chart geometry
/// that has to agree with hand-computed canvas coordinates. See the module
/// docs; everywhere else, prefer [`s`].
pub fn sp(design_px: f32) -> f32 {
    uiscale::scaled_px(design_px)
}

/// The value to hand `Window::set_rem_size`. Called once per frame by the
/// root view; every `rems` length in the tree resolves against it.
pub fn rem_size() -> Pixels {
    px(uiscale::rem_size_px())
}

// ─── Type scale ─────────────────────────────────────────────────────────────
//
// Each tier's rationale and its rendered size at the default scale live on
// the corresponding `*_PX` constant in `uiscale`.

/// Count badges and superscripts. Numerals only.
pub const TEXT_MICRO: Rems = s(uiscale::TEXT_MICRO_PX);
/// Dense tabular metadata, timestamps, key pills.
pub const TEXT_XS: Rems = s(uiscale::TEXT_XS_PX);
/// Secondary labels, chips, column headers.
pub const TEXT_SM: Rems = s(uiscale::TEXT_SM_PX);
/// **Body default.** Anything read as a sentence belongs here or above.
pub const TEXT_BASE: Rems = s(uiscale::TEXT_BASE_PX);
/// Emphasised body, primary values in a stat block.
pub const TEXT_MD: Rems = s(uiscale::TEXT_MD_PX);
/// Card titles.
pub const TEXT_LG: Rems = s(uiscale::TEXT_LG_PX);
/// Section headings.
pub const TEXT_XL: Rems = s(uiscale::TEXT_XL_PX);
/// Panel headings.
pub const TEXT_2XL: Rems = s(uiscale::TEXT_2XL_PX);
/// Modal titles.
pub const TEXT_3XL: Rems = s(uiscale::TEXT_3XL_PX);
/// Display.
pub const TEXT_4XL: Rems = s(uiscale::TEXT_4XL_PX);
/// Display.
pub const TEXT_5XL: Rems = s(uiscale::TEXT_5XL_PX);
/// Display.
pub const TEXT_6XL: Rems = s(uiscale::TEXT_6XL_PX);
/// Hero numerals and large glyphs.
pub const TEXT_7XL: Rems = s(uiscale::TEXT_7XL_PX);
/// Display.
pub const TEXT_8XL: Rems = s(uiscale::TEXT_8XL_PX);
/// The splash mark.
pub const TEXT_9XL: Rems = s(uiscale::TEXT_9XL_PX);
