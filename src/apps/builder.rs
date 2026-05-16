//! Re-export of the pure manifest builder.
//!
//! The actual code lives in the standalone crate `abw-apps-core` so callers
//! that don't need the rest of the fermi platform (notably the `abw` CLI,
//! and any future external integrations) can depend on a tiny pure-logic
//! library instead of pulling the whole server crate's transitive graph.
//!
//! Inside the fermi server we keep using `crate::apps::builder::*` everywhere
//! so refactoring this module is a one-line edit. The shim is just a glob
//! re-export from the standalone crate.

pub use abw_apps_core::*;
