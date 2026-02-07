pub mod error;
pub mod jwt;
pub mod middleware;
pub mod types;

// Re-export commonly used types
pub use error::AuthError;
pub use types::{ApiKey, AuthPrincipal, AuthProvider, User, UserRole};
