//! Boot-time migration runner (SPEC 36 G16 / #120).
//!
//! Before this, every listed migration re-ran on every boot and a failure
//! was printed and recorded but never stopped the boot
//! [V: `run_migrations` loop, src/api_server.rs:1477-1537 @ 59d76b00]. 30 files were
//! failing on every boot, 26 of them had never succeeded, and nothing
//! acted on it.
//!
//! Now, per file, in list order:
//! * recorded success and the same checksum → **skipped**;
//! * recorded success and a different checksum → **boot fails** (an applied
//!   migration was edited; write a new one instead);
//! * otherwise it runs. A failure **fails boot**, unless the file is in
//!   `migrations/KNOWN_FAILING.txt` with the same checksum. That list holds
//!   the files that have never applied, may only shrink (ratchet metric
//!   `known_failing_migrations`), and is the owner-approved interim
//!   (#120, option 2) until each is superseded;
//! * an unreadable listed file, or a ledger that cannot be read or written,
//!   fails boot: without the ledger the skip decision cannot be made.

use std::collections::{BTreeMap, HashMap};

use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};

pub const LEDGER: &str = "public.schema_migrations";
pub const KNOWN_FAILING_PATH: &str = "migrations/KNOWN_FAILING.txt";

/// `filename sha256 class` per line; `#` comments.
#[derive(Debug, Default, Clone)]
pub struct KnownFailing(BTreeMap<String, (String, String)>);

impl KnownFailing {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut m = BTreeMap::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let (Some(f), Some(sha), Some(class)) = (parts.next(), parts.next(), parts.next())
            else {
                return Err(format!("line {}: want `filename sha256 class`", n + 1));
            };
            if m.insert(f.to_string(), (sha.to_string(), class.to_string()))
                .is_some()
            {
                return Err(format!("line {}: {f} listed twice", n + 1));
            }
        }
        Ok(Self(m))
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    fn tolerates(&self, file: &str, sha: &str) -> bool {
        self.0.get(file).is_some_and(|(s, _)| s == sha)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Skipped,
    Applied,
    KnownFailure,
}

#[derive(Debug)]
pub enum BootError {
    Unreadable {
        file: String,
        error: String,
    },
    ChecksumChanged {
        file: String,
        recorded: String,
        now: String,
    },
    Failed {
        file: String,
        error: String,
    },
    Ledger(String),
}

impl std::fmt::Display for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootError::Unreadable { file, error } => write!(f, "{file}: unreadable: {error}"),
            BootError::ChecksumChanged {
                file,
                recorded,
                now,
            } => write!(
                f,
                "{file}: applied with sha256 {recorded}, file is now {now}. \
                 Do not edit an applied migration; add a new one."
            ),
            BootError::Failed { file, error } => write!(f, "{file}: failed: {error}"),
            BootError::Ledger(e) => write!(f, "migration ledger: {e}"),
        }
    }
}

impl std::error::Error for BootError {}

pub fn sha256_hex(sql: &str) -> String {
    format!("{:x}", Sha256::digest(sql.as_bytes()))
}

/// Run `files` in order against `conn`, recording into `ledger`.
/// `read` loads a file's SQL (the filesystem in production).
pub async fn run<R>(
    conn: &mut PgConnection,
    ledger: &str,
    files: &[&str],
    read: R,
    known: &KnownFailing,
) -> Result<Vec<(String, Outcome)>, BootError>
where
    R: Fn(&str) -> std::io::Result<String>,
{
    ensure_ledger(conn, ledger).await?;
    let applied = load_applied(conn, ledger).await?;
    let mut out = Vec::with_capacity(files.len());

    for &file in files {
        let started = std::time::Instant::now();
        let sql = match read(file) {
            Ok(s) => s,
            Err(e) => {
                // An applied file keeps its ledger row untouched: recording
                // `unreadable` would replace its checksum with '' and every
                // later boot would fail on `ChecksumChanged`.
                if !applied.contains_key(file) {
                    record(
                        conn,
                        ledger,
                        file,
                        "",
                        "unreadable",
                        Some(&e.to_string()),
                        0,
                    )
                    .await?;
                }
                return Err(BootError::Unreadable {
                    file: file.into(),
                    error: e.to_string(),
                });
            }
        };
        let sha = sha256_hex(&sql);
        if let Some(recorded) = applied.get(file) {
            if *recorded == sha {
                out.push((file.to_string(), Outcome::Skipped));
                continue;
            }
            // Not recorded: the ledger keeps the checksum it applied.
            return Err(BootError::ChecksumChanged {
                file: file.into(),
                recorded: recorded.clone(),
                now: sha,
            });
        }
        let res = sqlx::raw_sql(&sql).execute(&mut *conn).await;
        let ms = started.elapsed().as_millis() as i32;
        match res {
            Ok(_) => {
                record(conn, ledger, file, &sha, "ok", None, ms).await?;
                out.push((file.to_string(), Outcome::Applied));
            }
            Err(e) => {
                let msg = e.to_string();
                record(conn, ledger, file, &sha, "failed", Some(&msg), ms).await?;
                if known.tolerates(file, &sha) {
                    tracing::warn!(file, error = %msg, "known-failing migration still fails");
                    out.push((file.to_string(), Outcome::KnownFailure));
                } else {
                    return Err(BootError::Failed {
                        file: file.into(),
                        error: msg,
                    });
                }
            }
        }
    }
    Ok(out)
}

/// The ledger table. `migrations/207_migration_ledger.sql` is the
/// authoritative declaration; this bootstraps it so the first run can be
/// recorded.
async fn ensure_ledger(conn: &mut PgConnection, ledger: &str) -> Result<(), BootError> {
    let ddl = format!(
        "CREATE TABLE IF NOT EXISTS {ledger} (
            filename          TEXT PRIMARY KEY,
            content_sha256    TEXT NOT NULL,
            attempts          INTEGER NOT NULL DEFAULT 0,
            successes         INTEGER NOT NULL DEFAULT 0,
            failures          INTEGER NOT NULL DEFAULT 0,
            consecutive_failures INTEGER NOT NULL DEFAULT 0,
            first_succeeded_at   TIMESTAMPTZ,
            last_attempt_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
            last_status       TEXT NOT NULL,
            last_error        TEXT,
            last_duration_ms  INTEGER)"
    );
    sqlx::raw_sql(&ddl)
        .execute(&mut *conn)
        .await
        .map(|_| ())
        .map_err(|e| BootError::Ledger(e.to_string()))
}

/// Files with a recorded success, and the checksum recorded for them.
async fn load_applied(
    conn: &mut PgConnection,
    ledger: &str,
) -> Result<HashMap<String, String>, BootError> {
    let rows = sqlx::query(&format!(
        "SELECT filename, content_sha256 FROM {ledger} WHERE first_succeeded_at IS NOT NULL"
    ))
    .fetch_all(&mut *conn)
    .await
    .map_err(|e| BootError::Ledger(e.to_string()))?;
    rows.iter()
        .map(|r| {
            Ok((
                r.try_get::<String, _>("filename")
                    .map_err(|e| BootError::Ledger(e.to_string()))?,
                r.try_get::<String, _>("content_sha256")
                    .map_err(|e| BootError::Ledger(e.to_string()))?,
            ))
        })
        .collect()
}

async fn record(
    conn: &mut PgConnection,
    ledger: &str,
    filename: &str,
    sha: &str,
    status: &str,
    error: Option<&str>,
    duration_ms: i32,
) -> Result<(), BootError> {
    let ok = status == "ok";
    sqlx::query(&format!(
        "INSERT INTO {ledger}
             (filename, content_sha256, attempts, successes, failures,
              consecutive_failures, first_succeeded_at, last_attempt_at,
              last_status, last_error, last_duration_ms)
         VALUES ($1, $2, 1, $3, $4, $4, CASE WHEN $5 THEN now() END, now(), $6, $7, $8)
         ON CONFLICT (filename) DO UPDATE SET
             -- The checksum of an applied file is the one it applied with.
             content_sha256 = CASE WHEN {t}.first_succeeded_at IS NULL
                                   THEN EXCLUDED.content_sha256
                                   ELSE {t}.content_sha256 END,
             attempts  = {t}.attempts + 1,
             successes = {t}.successes + EXCLUDED.successes,
             failures  = {t}.failures + EXCLUDED.failures,
             consecutive_failures = CASE WHEN $5 THEN 0 ELSE {t}.consecutive_failures + 1 END,
             first_succeeded_at = COALESCE({t}.first_succeeded_at, EXCLUDED.first_succeeded_at),
             last_attempt_at = now(),
             last_status = EXCLUDED.last_status,
             last_error = EXCLUDED.last_error,
             last_duration_ms = EXCLUDED.last_duration_ms",
        t = ledger.rsplit('.').next().unwrap_or(ledger),
    ))
    .bind(filename)
    .bind(sha)
    .bind(i32::from(ok))
    .bind(i32::from(!ok))
    .bind(ok)
    .bind(status)
    .bind(error)
    .bind(duration_ms)
    .execute(&mut *conn)
    .await
    .map(|_| ())
    .map_err(|e| BootError::Ledger(format!("recording {filename}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_failing_parses_and_rejects_bad_lines() {
        let k = KnownFailing::parse("# c\nmigrations/a.sql abc class_x\n\n").unwrap();
        assert_eq!(k.len(), 1);
        assert!(k.tolerates("migrations/a.sql", "abc"));
        assert!(!k.tolerates("migrations/a.sql", "def"));
        assert!(KnownFailing::parse("migrations/a.sql abc").is_err());
        assert!(KnownFailing::parse("a x c\na y c").is_err());
    }
}
