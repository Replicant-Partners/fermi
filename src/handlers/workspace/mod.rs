//! Workspace handlers — re-exports from the focused sub-modules.
//!
//! Split for navigability. All public symbols remain at `handlers::workspace::*`
//! so no route registrations in api_server.rs need to change.
//!
//! - core.rs      CRUD (list, get, agents, fund), gas helper, shared utilities
//! - messages.rs  Chat, SSE stream, agent hire/add/remove
//! - coherence.rs Coherence eval, ontology, files, git log, workflow
//! - actions.rs   Generalised App action protocol (mutate_document, fork_state, etc.)

pub mod actions;
pub mod agent_params_hook;
mod coherence;
mod core;
pub mod lens_actions;
mod messages;
pub mod outputs;
pub mod refit;
pub mod resolution;

pub use agent_params_hook::*;
pub use coherence::*;
pub use core::*;
pub use messages::*;
pub use outputs::*;
pub use refit::*;
pub use resolution::*;
