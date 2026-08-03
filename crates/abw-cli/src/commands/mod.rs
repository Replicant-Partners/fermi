//! CLI command implementations.
//!
//! Each command takes a `Ctx` (the cross-cutting CLI state) plus its own
//! parsed args, and returns `anyhow::Result<()>`. Errors bubble up to `main`
//! which renders the chain.

pub mod admin;
pub mod deploy;
pub mod list;
pub mod login;
pub mod new;
pub mod publish;
pub mod spawn;
pub mod validate;
pub mod workspace;

/// Cross-cutting CLI state passed to every command.
pub struct Ctx {
    pub base_url: String,
    pub quiet: bool,
}

impl Ctx {
    /// Build a reqwest client with sensible defaults.
    pub fn http(&self) -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .user_agent(concat!("abw-cli/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("building reqwest client")
    }

    pub fn url(&self, path: &str) -> String {
        if path.starts_with('/') {
            format!("{}{}", self.base_url, path)
        } else {
            format!("{}/{}", self.base_url, path)
        }
    }
}
