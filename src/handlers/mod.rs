//! Handler modules — organized by domain.
//!
//! New handlers go into focused modules here rather than api_server.rs.
//! Shared helpers (resolve_agent, resolve_agent_card, create_notification)
//! live in api_server.rs as pub(crate) functions.

pub mod admin;
pub mod agents;
pub mod auth;
pub mod beacons;
pub mod billing;
pub mod consolidation;
pub mod eval;
pub mod execution;
pub mod kg;
pub mod lifecycle;
pub mod marketplace;
pub mod mcp;
pub mod metrics;
pub mod misc;
pub mod ontology;
pub mod pages;
pub mod profile;
pub mod teams;
pub mod users;
pub mod wallet;
pub mod wizard;
pub mod workspace;
