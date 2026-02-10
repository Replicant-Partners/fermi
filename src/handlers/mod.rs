//! Handler modules — organized by domain.
//!
//! New handlers go into focused modules here rather than api_server.rs.
//! Shared helpers (resolve_agent, resolve_agent_card, create_notification)
//! live in api_server.rs as pub(crate) functions.

pub mod lifecycle;
pub mod workspace;
