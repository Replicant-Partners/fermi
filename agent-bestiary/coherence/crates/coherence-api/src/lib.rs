//! REST API server for the Collaboration Coherence Evaluator Agent.
//!
//! Provides HTTP endpoints for:
//! - Starting a new coherence evaluation session
//! - Submitting messages to an active session
//! - Retrieving the current coherence snapshot
//! - Listing active sessions
//!
//! # Endpoints
//!
//! | Method | Path | Description |
//! |--------|------|-------------|
//! | `POST` | `/sessions` | Start a new evaluation session |
//! | `POST` | `/sessions/:id/messages` | Submit a message |
//! | `POST` | `/sessions/:id/evaluate` | Trigger evaluation |
//! | `GET` | `/sessions/:id` | Get current snapshot |
//! | `GET` | `/sessions` | List active sessions |
//! | `GET` | `/health` | Health check |

mod routes;
mod state;

pub use routes::create_router;
pub use state::AppState;
