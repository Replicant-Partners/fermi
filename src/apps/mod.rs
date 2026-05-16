//! App primitive — platform-level logic for the App registry.
//!
//! HTTP handlers live in `src/handlers/apps.rs` and call into this module.
//! Auto-seed from `apps/*.json` (in `api_server.rs::seed_apps_to_database`)
//! also calls into this module.
//!
//! The CLI (`abw-cli`), the xamanEK `app_design` session-mode flow, and the
//! "Save workspace as App" fork flow all share this same builder substrate
//! so validation and defaulting are consistent across every entry point.
//!
//! Public surface:
//!   - `builder::build_manifest()` — accepts a partial manifest, fills defaults,
//!     validates, and returns either a finalized `AppManifest` or a list of
//!     structured `Issue`s (errors and suggestions) the caller can render.
//!   - `builder::validate_slug()`, `builder::is_reserved()` — pure validators.
//!   - `builder::Issue`, `builder::Severity` — structured feedback shape.

pub mod builder;
