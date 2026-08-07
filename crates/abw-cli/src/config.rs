//! Credential storage at ~/.abw/credentials.
//!
//! Format is a tiny TOML-flavoured key=value file so users can inspect /
//! hand-edit if needed. Permissions set to 0600 on Unix.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Credentials {
    /// API key obtained from `abw login` or pasted from the dashboard.
    pub api_key: Option<String>,
    /// The base URL the key was issued for (so a key minted for prod
    /// doesn't get pointed at a staging server).
    pub base_url: Option<String>,
    /// User identity (display name or email) — purely for `abw whoami`.
    pub user: Option<String>,
}

fn config_dir() -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .ok_or_else(|| anyhow!("could not determine home directory"))?
        .join(".abw");
    if !dir.exists() {
        std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    Ok(dir)
}

fn credentials_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("credentials"))
}

pub fn load() -> Result<Credentials> {
    let path = credentials_path()?;
    if !path.exists() {
        return Ok(Credentials::default());
    }
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut creds = Credentials::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"').to_string();
            match k.trim() {
                "api_key" => creds.api_key = Some(v),
                "base_url" => creds.base_url = Some(v),
                "user" => creds.user = Some(v),
                _ => {} // ignore unknown keys for forward-compat
            }
        }
    }
    Ok(creds)
}

pub fn save(creds: &Credentials) -> Result<()> {
    let path = credentials_path()?;
    let mut out = String::new();
    out.push_str("# ABW CLI credentials — written by `abw login`.\n");
    out.push_str("# To rotate: `abw logout` then `abw login`.\n\n");
    if let Some(v) = &creds.api_key {
        out.push_str(&format!("api_key = \"{}\"\n", v));
    }
    if let Some(v) = &creds.base_url {
        out.push_str(&format!("base_url = \"{}\"\n", v));
    }
    if let Some(v) = &creds.user {
        out.push_str(&format!("user = \"{}\"\n", v));
    }
    std::fs::write(&path, out).with_context(|| format!("writing {}", path.display()))?;

    // Tighten permissions on Unix so a stray world-readable creds file
    // doesn't leak a long-lived API key.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perms)
            .with_context(|| format!("chmod 600 {}", path.display()))?;
    }
    Ok(())
}

pub fn remove() -> Result<()> {
    let path = credentials_path()?;
    if path.exists() {
        std::fs::remove_file(&path).with_context(|| format!("removing {}", path.display()))?;
    }
    Ok(())
}

/// Resolve the active API key, preferring (in order):
///   1. $ABW_API_TOKEN env var (script-friendly override)
///   2. ~/.abw/credentials api_key field
pub fn resolve_api_key() -> Result<String> {
    if let Ok(v) = std::env::var("ABW_API_TOKEN") {
        if !v.is_empty() {
            return Ok(v);
        }
    }
    let creds = load()?;
    creds.api_key.ok_or_else(|| {
        anyhow!(
            "not authenticated — run `abw login` first, or set $ABW_API_TOKEN \
             to an API key minted at /settings/api-keys"
        )
    })
}
