//! Workspace handlers — re-exports from the three focused sub-modules.
//!
//! Split for navigability. All public symbols remain at `handlers::workspace::*`
//! so no route registrations in api_server.rs need to change.
//!
//! - workspace_core.rs      CRUD (list, get, agents, fund), gas helper, shared utilities
//! - workspace_messages.rs  Chat, SSE stream, agent hire/add/remove
//! - workspace_coherence.rs Coherence eval, ontology, files, git log, workflow

mod core;
mod messages;
mod coherence;

pub use core::*;
pub use messages::*;
pub use coherence::*;
