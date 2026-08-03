//! Library face of the Fermi Console.
//!
//! # Why this exists
//!
//! `fermi-console` is primarily a GPUI binary (`src/main.rs`). GPUI's
//! element-builder chains are deeply nested generic method calls, and
//! under `--test` rustc's macro expansion of the full UI tree
//! **exhausts its stack and segfaults** — which is why `main.rs`
//! carries `#![recursion_limit = "4096"]` and why every `#[cfg(test)]`
//! module inside the binary target is, in practice, unrunnable:
//!
//! ```text
//! $ cargo test -p fermi-console --bin fermi-console
//! note: rustc unexpectedly overflowed its stack!
//! ```
//!
//! That's a pre-existing condition, not a regression — but it means
//! logic worth testing must live somewhere rustc can reach without
//! expanding the UI. This lib target is that somewhere.
//!
//! Only modules with **no GPUI dependency** belong here. Today that's
//! [`updater`], which talks to the GitHub Releases API and does pure
//! version arithmetic — exactly the kind of thing that should never
//! have shipped untested. `cargo test -p fermi-console --lib` compiles
//! in seconds.
//!
//! The binary consumes these modules as `fermi_console::updater`
//! rather than declaring `mod updater;` itself, so there is a single
//! definition and no duplicate compilation.
//!
//! [`wire`] holds client-side enforcement of API contracts the server
//! validates and 400s on — the kind of arithmetic whose failure mode
//! is silent data loss, and which therefore has no business being
//! untestable.
//!
//! [`plot`] holds the geometry and statistics behind every chart —
//! scales, frames, density estimation, Sobol layout. It is deliberately
//! GPUI-free so that "is the hover overlay actually on top of the dot?"
//! and "does this curve show both modes?" are assertions rather than
//! things someone eyeballs at 2am. The binary's `viz` module does the
//! painting and owns all the `gpui` types.
//!
//! Candidates to migrate here as they're decoupled from GPUI: the FPL
//! action-marker parser in `chat.rs` and the Anthropic error extractor
//! in `cockpit.rs`, both of which currently have tests that can't run.

pub mod plot;
pub mod updater;
pub mod wire;
