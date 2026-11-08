//! `episode_reaper`: fails episodes left `running` (SPEC 36 R0.5.2 / G07).
//!
//! [`crate::episode_boundary::Pulse`] ends every reservation it owns, but a
//! process that dies mid-run (deploy, OOM, crash) takes the drop guard with
//! it. This sweep is the backstop: a row `running` for longer than
//! `EPISODE_MAX_RUNTIME_SECS` (default 1800 s, above the worst-case loop of
//! about 11 min) becomes `failure` with `abandoned_timeout:`.
//!
//! Reservations owned by an open, unexpired grounding run are skipped
//! (`MemoryStore::stale_running_episodes`); that run has its own TTL.
//!
//! Money holds: R0.5.2 also asks the sweep to release any hold (R1.3). No
//! hold exists yet; when R1.3 adds one, its release belongs here, in the
//! same transaction as the status change.

use std::sync::Arc;
use std::time::Duration;

use agent_bestiary_memory::MemoryStore;
use uuid::Uuid;

pub const DEFAULT_MAX_RUNTIME: Duration = Duration::from_secs(1800);
const SWEEP_EVERY: Duration = Duration::from_secs(300);

pub fn max_runtime_from_env() -> Duration {
    std::env::var("EPISODE_MAX_RUNTIME_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|s| *s > 0)
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_MAX_RUNTIME)
}

/// One sweep. Returns the ids it failed.
pub async fn sweep_once(
    store: &MemoryStore,
    max_runtime: Duration,
) -> agent_bestiary_memory::Result<Vec<Uuid>> {
    let stale = store.stale_running_episodes(max_runtime).await?;
    if stale.is_empty() {
        return Ok(stale);
    }
    let reason = format!(
        "abandoned_timeout: running longer than {} s with no close",
        max_runtime.as_secs()
    );
    store.abandon_running(&stale, &reason).await
}

/// Sweep now, then every five minutes.
pub fn spawn(store: Arc<MemoryStore>) {
    let max_runtime = max_runtime_from_env();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(SWEEP_EVERY);
        loop {
            tick.tick().await;
            match sweep_once(&store, max_runtime).await {
                Ok(ids) if ids.is_empty() => {}
                Ok(ids) => tracing::warn!(
                    count = ids.len(),
                    "episode_reaper failed stale running episodes",
                ),
                Err(e) => tracing::error!(error = %e, "episode_reaper sweep failed"),
            }
        }
    });
}
