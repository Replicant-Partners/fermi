//! Handler modules — organized by domain.
//!
//! New handlers go into focused modules here rather than api_server.rs.
//! Shared helpers (resolve_agent, resolve_agent_card, create_notification)
//! live in api_server.rs as pub(crate) functions.

pub mod admin;
pub mod apps;
pub mod bayesops;
pub mod simops;
pub mod agent_wallet;
pub mod agents;
pub mod auth;
pub mod beacons;
pub mod billing;
pub mod consolidation;
pub mod creatures;
pub mod dashboard;
pub mod eval;
pub mod eval_brier;
pub mod forecast_benchmark;
pub mod simops_benchmark;
pub mod eval_judge;
pub mod eval_projection;
pub mod execution;
pub mod execution_stream;
pub mod forecasts;
pub mod governance;
pub mod kg;
pub mod lifecycle;
pub mod marketplace;
pub mod mcp;
pub mod metrics;
pub mod misc;
pub mod notebooks;
pub mod observations;
pub mod composition;
pub mod observatory;
pub mod xaman;
pub mod ontology;
pub mod pages;
pub mod polymarket;
pub mod profile;
pub mod push;
pub mod qr_codes;
pub mod rabble_chat;
pub mod rabble_workspace;
pub mod social;
pub mod streams;
pub mod swarm_algorithms;
pub mod swarm_telemetry;
pub mod teams;
pub mod users;
pub mod wallet;
pub mod wizard;
pub mod workspace;
