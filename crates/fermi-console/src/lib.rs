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
//! [`uiscale`] holds the UI scale factor and the type scale as plain
//! numbers. Clamping, percent-snapping and the monotonicity of the type
//! scale are arithmetic with a wrong answer, so they get assertions rather
//! than a squint at a screenshot. The binary's `ui` module wraps these in
//! GPUI's `Rems` and is what feature code actually calls.
//!
//! [`agent_naming`] holds the bound-name convention for driver-bound
//! agents — the construction `{agent_id}_{driver}` and its inverse.
//! Getting the inverse wrong sends an FPL identifier to ABW as an agent
//! id (404) and orphans the evidence that identifier produced, so the
//! rules belong somewhere they can be asserted rather than guessed.
//!
//! [`calibration`] critiques the base rate — the one number every
//! driver multiplies, and therefore the one worth checking hardest. It
//! is structural (Wilson intervals, sample size, circular reference
//! classes) rather than semantic, so its answers are defensible.
//!
//! [`routing`] decides which research specialist answers which driver.
//! It got here the hard way: the ladder lived inline in `cockpit.rs`,
//! could not be tested, and silently sent every driver of a Premier
//! League question to `macro_forecaster`. Agent selection determines
//! what evidence a forecast is built on, so it is assertion-worthy by
//! definition.
//!
//! [`coverage`] grades the whole team a decomposition hires, which
//! [`routing`] cannot see one driver at a time. A generalist standing in
//! is a legitimate outcome; a generalist standing in that LOOKS like a
//! considered choice is not, and five identically-phrased assignment
//! lines were how four fallbacks passed for four decisions. It is also
//! where per-agent fitness lands when tournaments start producing it —
//! the ordering rule is written and asserted now, with an empty record,
//! so feeding it later is data rather than a rewrite.
//!
//! [`negotiate`] composes the query an agent is sent from what that agent's
//! card *declares*, rather than from a hardcoded match on its identifier. The
//! match was a closed world: an agent designed by someone else could only
//! ever receive the generic fallback, so the console could only ask
//! well-formed questions of agents enumerated at compile time — optimising
//! for known patterns and foreclosing new ones. It also contradicted the
//! declarations it duplicated. This is the seam where heterogeneous fleets
//! either compose or don't, so it is asserted rather than eyeballed.
//!
//! [`drivers`] answers "is this driver ready to spend money on?". A
//! forgotten `triangular(0, 0, 0)` placeholder used to be dispatched to a
//! real research agent, which burned its whole iteration budget on a driver
//! that was never named and billed the user for the failure. Both directions
//! of a wrong answer cost credits, so the predicate is asserted.
//!
//! [`flow`] decides what the research key does. Two of its three
//! outcomes are irreversible — decomposition discards the whole
//! forecast, running staged research bills real money — so the branch
//! is a pure function with tests rather than an `if` ladder inside an
//! event handler.
//!
//! [`mutations`] validates the model edits Fermi proposes in chat — the
//! symbolic write in the neuro-symbolic loop. These are writes to a
//! forecast authored by a language model, so every field is checked
//! before it reaches the AST: a backwards triangular distribution does
//! not fail loudly, it silently produces a nonsense forecast.
//!
//! [`abw_pacing`] models the server's per-user LLM budget on the client, so
//! a fan-out of five drivers queues into a legal cadence instead of racing
//! into three 429s that were never retried. It is clock-free — every entry
//! point takes `now` — because the alternative is a test suite that sleeps
//! for a minute to assert one wait.
//!
//! Candidates to migrate here as they're decoupled from GPUI: the FPL
//! action-marker parser in `chat.rs` and the Anthropic error extractor
//! in `cockpit.rs`, both of which currently have tests that can't run.

pub mod abw_pacing;
pub mod agent_naming;
pub mod calibration;
pub mod coverage;
pub mod drivers;
pub mod flow;
pub mod mutations;
pub mod negotiate;
pub mod plot;
pub mod roster;
pub mod routing;
pub mod trajectory_narrative;
pub mod uiscale;
pub mod updater;
pub mod wire;
