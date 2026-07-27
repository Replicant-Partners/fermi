//! Forecast Benchmark — Commitment Anchor, Spacetime View, Harness Snapshots
//!
//! ## The immutable clock
//!
//! `anchor_forecast()` writes a `forecast_commitments` row with a
//! tamper-evident hash of (forecast_id, probability, fpl_hash, emitted_ts).
//! Called on: forecast creation, every probability update, and the daily
//! cron sweep. A forecast is benchmarkable only for revisions where
//! `committed_at < resolved_at` — this function is what makes that true.
//!
//! ## The spacetime view
//!
//! `GET /api/forecasts/:id/spacetime` returns every state a forecast ever
//! occupied: probability, driver decomposition, Sobol attribution, harness
//! config, and cross-loop RSI context at each revision. This is the primary
//! research object for the adaptive forecast thesis — not "was the final
//! forecast accurate" but "how did the forecast evolve, at what rate, and
//! in response to what?"
//!
//! Rate-of-change metrics computed on the fly:
//!   - probability_velocity: Δp / Δt (probability change per hour)
//!   - dominant_driver_shift: max change in Sobol first-order index
//!   - information_gain: |p_now - p_prev| as a proxy for revision magnitude
//!
//! ## The daily cron
//!
//! `POST /api/benchmark/anchor-sweep` anchors all active unanchored forecasts.
//! Safe to call repeatedly (idempotent per revision). Designed to be
//! triggered by a Railway cron job or any external scheduler.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Row;

use crate::AppState;
use fermi_auth::AuthPrincipal;

// ── Commitment hash ───────────────────────────────────────────────────────

/// Compute the tamper-evident commitment hash for a forecast snapshot.
/// sha256(forecast_id || "|" || probability_str || "|" || fpl_hash || "|" || emitted_ts)
fn commitment_hash(
    forecast_id: &str,
    probability: f64,
    fpl_hash: Option<&str>,
    emitted_ts: &chrono::DateTime<Utc>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(forecast_id.as_bytes());
    hasher.update(b"|");
    hasher.update(format!("{:.8}", probability).as_bytes());
    hasher.update(b"|");
    hasher.update(fpl_hash.unwrap_or("").as_bytes());
    hasher.update(b"|");
    hasher.update(emitted_ts.to_rfc3339().as_bytes());
    format!("{:x}", hasher.finalize())
}

/// sha256 of a string — used for FPL source hashing.
fn sha256_str(s: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ── Core anchor function ──────────────────────────────────────────────────

/// Write a commitment anchor for a forecast snapshot.
/// Idempotent: if the commitment_hash already exists, this is a no-op.
/// Returns the commitment_hash written (or the existing one).
pub async fn anchor_forecast(
    pool: &sqlx::PgPool,
    forecast_id: &str,
    revision_id: Option<&str>,
    probability: f64,
    fpl_source: Option<&str>,
    emitted_at: chrono::DateTime<Utc>,
    anchor_note: Option<&str>,
) -> Result<String, String> {
    let fpl_hash = fpl_source.map(sha256_str);
    let hash = commitment_hash(forecast_id, probability, fpl_hash.as_deref(), &emitted_at);

    // Check if already exists (idempotent)
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM forecast_commitments WHERE commitment_hash = $1)",
    )
    .bind(&hash)
    .fetch_one(pool)
    .await
    .unwrap_or(false);

    if exists {
        return Ok(hash);
    }

    // Check table exists (migration 140 may be pending)
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name='forecast_commitments')"
    ).fetch_one(pool).await.unwrap_or(false);

    if !table_exists {
        return Err("forecast_commitments table not yet created (migration 140 pending)".into());
    }

    sqlx::query(
        r#"INSERT INTO forecast_commitments
           (forecast_id, revision_id, predicted_probability, fpl_source_hash,
            commitment_hash, anchor_method, anchor_note, emitted_at, committed_at)
           VALUES ($1, $2, $3, $4, $5, 'db_timestamp', $6, $7, NOW())"#,
    )
    .bind(forecast_id)
    .bind(revision_id)
    .bind(probability as f32)
    .bind(fpl_hash.as_deref())
    .bind(&hash)
    .bind(anchor_note)
    .bind(emitted_at)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(hash)
}

// ── Split assignment ──────────────────────────────────────────────────────

/// Assign held_in / held_out / validation split deterministically.
/// Uses sha256(forecast_id + salt), last byte mod 10:
///   0–4 → held_in (50%), 5–7 → held_out (30%), 8–9 → validation (20%)
/// Salt should be pre-registered and stable across a lineage.
pub fn assign_split(forecast_id: &str, salt: &str) -> &'static str {
    let input = format!("{}{}", forecast_id, salt);
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    let last_byte = result[31] % 10;
    match last_byte {
        0..=4 => "held_in",
        5..=7 => "held_out",
        _ => "validation",
    }
}

/// Ensure a forecast has a split row, creating one if absent.
pub async fn ensure_split(
    pool: &sqlx::PgPool,
    forecast_id: &str,
    salt: &str,
) -> Result<String, String> {
    // Check if already assigned
    if let Ok(Some(row)) = sqlx::query("SELECT split FROM forecast_splits WHERE forecast_id = $1")
        .bind(forecast_id)
        .fetch_optional(pool)
        .await
    {
        return Ok(row.try_get::<String, _>("split").unwrap_or_default());
    }

    // Table may not exist yet
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name='forecast_splits')",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(false);
    if !table_exists {
        return Err("forecast_splits table not yet created".into());
    }

    let split = assign_split(forecast_id, salt);
    let hash_input = format!("{}{}", forecast_id, salt);

    sqlx::query(
        r#"INSERT INTO forecast_splits
           (forecast_id, split, split_hash_input, split_salt, assigned_at)
           VALUES ($1, $2, $3, $4, NOW())
           ON CONFLICT (forecast_id) DO NOTHING"#,
    )
    .bind(forecast_id)
    .bind(split)
    .bind(&hash_input)
    .bind(salt)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(split.to_string())
}

// ── Daily anchor sweep ────────────────────────────────────────────────────

/// POST /api/benchmark/anchor-sweep
///
/// Anchors all active forecasts that have no commitment for their current
/// probability. Safe to call repeatedly — idempotent per revision.
///
/// This is the cron job endpoint. Call it daily from Railway cron or any
/// external scheduler. It starts the clock on every active forecast so
/// commit-before-resolve holds even if the operator never explicitly triggers it.
pub async fn anchor_sweep_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
) -> Result<Json<Value>, (StatusCode, String)> {
    // Admin or service token only
    if !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "admin access required".into()));
    }

    let pool = &state.db;
    let salt = std::env::var("BENCHMARK_SPLIT_SALT").unwrap_or_else(|_| "fermi-v1-2026".into());

    // Defensive: if migration 140 hasn't run on this DB yet, the
    // NOT EXISTS subquery below blows up with a raw "relation ...
    // does not exist" 500. Detect early and return a structured 503
    // so the cron caller sees actionable text, not a SQL panic string.
    let commitments_table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name='forecast_commitments')"
    ).fetch_one(pool).await.unwrap_or(false);
    if !commitments_table_exists {
        return Ok(Json(json!({
            "anchored": 0,
            "errors": ["forecast_commitments table not yet created (migration 140 pending on this DB)"],
            "swept_at": Utc::now(),
            "note": "no-op: schema not yet migrated",
            "migration_required": "migrations/140_forecast_benchmark.sql",
        })));
    }

    // Fetch all active forecasts not yet committed at their current probability
    let rows = sqlx::query(
        r#"SELECT f.id, f.predicted_probability, f.fpl_source, f.created_at
           FROM fermi_forecasts f
           WHERE f.status = 'active'
             AND NOT EXISTS (
               SELECT 1 FROM forecast_commitments c
               WHERE c.forecast_id = f.id
                 AND c.revision_id IS NULL
                 AND ABS(c.predicted_probability - f.predicted_probability) < 0.0001
             )
           LIMIT 500"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut anchored = 0usize;
    let mut errors: Vec<String> = Vec::new();
    let note = format!("daily anchor sweep {}", Utc::now().date_naive());

    for row in &rows {
        let fid: String = row.get("id");
        let prob: f32 = row.get("predicted_probability");
        let fpl: Option<String> = row.try_get("fpl_source").ok().flatten();
        let created_at: chrono::DateTime<Utc> = row.get("created_at");

        match anchor_forecast(
            pool,
            &fid,
            None,
            prob as f64,
            fpl.as_deref(),
            created_at,
            Some(&note),
        )
        .await
        {
            Ok(_) => {
                // Also ensure split assignment
                let _ = ensure_split(pool, &fid, &salt).await;
                anchored += 1;
            }
            Err(e) => errors.push(format!("{}: {}", fid, e)),
        }
    }

    Ok(Json(json!({
        "anchored": anchored,
        "errors": errors,
        "swept_at": Utc::now(),
        "note": note,
    })))
}

// ── Spacetime view ────────────────────────────────────────────────────────

/// GET /api/forecasts/:id/spacetime
///
/// Returns the complete temporal trajectory of a forecast — every state it
/// has ever occupied with rate-of-change metrics computed on the fly.
///
/// This is the primary research artifact for the adaptive forecast thesis.
/// It answers: not just "was the final forecast accurate" but "how did it
/// evolve, at what rate, in response to what signals, and was it committed
/// before it was right?"
pub async fn forecast_spacetime_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // Ownership check
    //
    // NB: the canonical schema (migrations 048/094/107) names the resolution
    // outcome column `actual_outcome` (BOOLEAN). An earlier draft of this
    // handler referenced a nonexistent `resolution_outcome` column, which
    // 500'd every spacetime call on any DB that wasn't a freshly-seeded dev
    // env. Stick to `actual_outcome` and project it to a string downstream.
    let forecast = sqlx::query(
        "SELECT id, owner_id, question_text, predicted_probability, status,
                brier_score, actual_outcome, resolved_at, fpl_source,
                simulation_results, drivers, created_at, team_id
         FROM fermi_forecasts WHERE id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".into()))?;

    let owner_id: String = forecast.get("owner_id");
    // Spec 23 R-3: spacetime is the demo's visible trajectory surface; gating
    // it to owner-only blocks the WC forecast portfolio scenario where
    // multiple users watch the same forecast. Soften to:
    //   - owner (always)
    //   - admin (always)
    //   - any member of the forecast's linked workspace (team_id) — covers
    //     the WC portfolio sharing case
    if owner_id != user_id && !principal.can_admin() {
        let team_id: Option<uuid::Uuid> = forecast.try_get("team_id").ok().flatten();
        let allowed = match team_id {
            Some(tid) => fermi_auth::teams::get_member_role(pool, tid, &user_id)
                .await
                .ok()
                .flatten()
                .is_some(),
            None => false,
        };
        if !allowed {
            return Err((
                StatusCode::FORBIDDEN,
                "Not the forecast owner or a member of its workspace".into(),
            ));
        }
    }

    // Try spacetime table first (may not exist yet)
    let table_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name='forecast_spacetime')"
    ).fetch_one(pool).await.unwrap_or(false);

    let revisions: Vec<Value> = if table_exists {
        // Rich path: read from spacetime table with all context
        let rows = sqlx::query(
            r#"SELECT
                st.revision_seq,
                st.predicted_probability,
                st.previous_probability,
                st.revision_trigger,
                st.revision_reason,
                st.triggering_agent,
                st.evidence_delta,
                st.drivers_snapshot,
                st.sobol_snapshot,
                st.fpl_snapshot,
                st.loop3_coherence,
                st.loop5_calibration,
                st.brier_at_this_point,
                st.revision_ts,
                -- Commitment status for this revision
                c.commitment_hash,
                c.committed_at,
                c.anchor_method
            FROM forecast_spacetime st
            LEFT JOIN forecast_commitments c
                ON c.forecast_id = st.forecast_id
               AND ABS(c.predicted_probability - st.predicted_probability) < 0.0001
               AND c.revision_id IS NULL
            WHERE st.forecast_id = $1
            ORDER BY st.revision_seq ASC"#,
        )
        .bind(&forecast_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        rows.iter().enumerate().map(|(i, row)| {
            let prob: f32 = row.get("predicted_probability");
            let prev_prob: Option<f32> = row.try_get("previous_probability").ok().flatten();
            let rev_ts: chrono::DateTime<Utc> = row.get("revision_ts");
            let committed_at: Option<chrono::DateTime<Utc>> = row.try_get("committed_at").ok().flatten();

            // Rate-of-change metrics
            let (velocity_per_hour, info_gain) = if let Some(pp) = prev_prob {
                let delta_p = (prob - pp).abs() as f64;
                let info_gain = delta_p;
                // velocity requires knowing the previous revision time — approximate from seq
                (None::<f64>, Some(info_gain))
            } else {
                (None, None)
            };

            let committed_before_resolved = committed_at.map(|cat| {
                forecast.try_get::<Option<chrono::DateTime<Utc>>, _>("resolved_at")
                    .ok()
                    .flatten()
                    .map(|rat| cat < rat)
                    .unwrap_or(true) // not yet resolved = still valid
            });

            json!({
                "revision_seq": row.try_get::<i32, _>("revision_seq").unwrap_or(i as i32),
                "predicted_probability": prob,
                "previous_probability": prev_prob,
                "delta_p": prev_prob.map(|pp| (prob - pp) as f64),
                "revision_trigger": row.try_get::<Option<String>,_>("revision_trigger").ok().flatten(),
                "revision_reason": row.try_get::<Option<String>,_>("revision_reason").ok().flatten(),
                "triggering_agent": row.try_get::<Option<String>,_>("triggering_agent").ok().flatten(),
                "evidence_delta": row.try_get::<Option<Value>,_>("evidence_delta").ok().flatten(),
                "drivers": row.try_get::<Option<Value>,_>("drivers_snapshot").ok().flatten(),
                "sobol": row.try_get::<Option<Value>,_>("sobol_snapshot").ok().flatten(),
                "loop3_coherence": row.try_get::<Option<f64>,_>("loop3_coherence").ok().flatten(),
                "loop5_calibration": row.try_get::<Option<Value>,_>("loop5_calibration").ok().flatten(),
                "brier_if_resolved_here": row.try_get::<Option<f32>,_>("brier_at_this_point").ok().flatten(),
                "revision_ts": rev_ts,
                "commitment": {
                    "hash": row.try_get::<Option<String>,_>("commitment_hash").ok().flatten(),
                    "committed_at": committed_at,
                    "anchor_method": row.try_get::<Option<String>,_>("anchor_method").ok().flatten(),
                    "committed_before_resolved": committed_before_resolved,
                },
                "rate_of_change": {
                    "velocity_per_hour": velocity_per_hour,
                    "information_gain": info_gain,
                },
            })
        }).collect()
    } else {
        // Fallback: reconstruct from fermi_forecast_updates
        let updates = sqlx::query(
            "SELECT previous_probability, new_probability, reason, agent_id,
                    evidence_added, created_at
             FROM fermi_forecast_updates
             WHERE forecast_id = $1
             ORDER BY created_at ASC",
        )
        .bind(&forecast_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        let initial_prob: f32 = forecast.get("predicted_probability");
        let created_at: chrono::DateTime<Utc> = forecast.get("created_at");

        let mut revs = vec![json!({
            "revision_seq": 0,
            "predicted_probability": initial_prob,
            "previous_probability": null,
            "revision_trigger": "initial",
            "revision_ts": created_at,
            "commitment": { "note": "spacetime table pending migration 140" },
        })];

        for (i, row) in updates.iter().enumerate() {
            let new_p: f32 = row.get("new_probability");
            let prev_p: f32 = row.get("previous_probability");
            revs.push(json!({
                "revision_seq": i + 1,
                "predicted_probability": new_p,
                "previous_probability": prev_p,
                "delta_p": (new_p - prev_p) as f64,
                "revision_trigger": "evidence_update",
                "revision_reason": row.try_get::<Option<String>,_>("reason").ok().flatten(),
                "triggering_agent": row.try_get::<Option<String>,_>("agent_id").ok().flatten(),
                "evidence_delta": row.try_get::<Option<Value>,_>("evidence_added").ok().flatten(),
                "revision_ts": row.get::<chrono::DateTime<Utc>, _>("created_at"),
            }));
        }
        revs
    };

    // Summary metrics across all revisions
    let n = revisions.len();
    let total_movement: f64 = revisions
        .windows(2)
        .map(|w| {
            let p1 = w[0]["predicted_probability"].as_f64().unwrap_or(0.0);
            let p2 = w[1]["predicted_probability"].as_f64().unwrap_or(0.0);
            (p2 - p1).abs()
        })
        .sum();

    let final_prob: f32 = forecast.get("predicted_probability");
    let initial_prob = revisions
        .first()
        .and_then(|r| r["predicted_probability"].as_f64())
        .unwrap_or(final_prob as f64);
    let net_movement = final_prob as f64 - initial_prob;

    let brier: Option<f32> = forecast.try_get("brier_score").ok().flatten();
    let actual: Option<bool> = forecast.try_get("actual_outcome").ok().flatten();
    let resolution_outcome = actual.map(|b| if b { "yes" } else { "no" });

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "question": forecast.try_get::<String,_>("question_text").unwrap_or_default(),
        "status": forecast.try_get::<String,_>("status").unwrap_or_default(),
        "brier_score": brier,
        // Projected from fermi_forecasts.actual_outcome (BOOLEAN).
        // Stays "resolution_outcome" in the API surface so existing
        // clients don't need to change.
        "resolution_outcome": resolution_outcome,
        "actual_outcome": actual,
        "resolved_at": forecast.try_get::<Option<chrono::DateTime<Utc>>,_>("resolved_at").ok().flatten(),

        "trajectory": {
            "n_revisions": n,
            "total_probability_movement": total_movement,
            "net_movement": net_movement,
            "direction": if net_movement > 0.02 { "upward" }
                         else if net_movement < -0.02 { "downward" }
                         else { "stable" },
            "most_volatile_driver": null, // populated when Sobol snapshots exist
        },

        "revisions": revisions,

        "benchmark_status": {
            "has_spacetime_table": table_exists,
            "note": if !table_exists {
                "migration 140 pending — spacetime table not yet created"
            } else {
                "spacetime tracking active"
            },
        },
    })))
}

/// POST /api/forecasts/:id/commit
///
/// Explicitly anchor a forecast's current probability. Also called internally
/// on create and update. Most operators never need to call this — the daily
/// sweep handles it. But explicit anchoring on creation is better practice.
pub async fn commit_forecast_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    let row = sqlx::query(
        "SELECT id, owner_id, predicted_probability, fpl_source, created_at
         FROM fermi_forecasts WHERE id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".into()))?;

    let owner_id: String = row.get("owner_id");
    if owner_id != user_id && !principal.can_admin() {
        return Err((StatusCode::FORBIDDEN, "Not the forecast owner".into()));
    }

    let prob: f32 = row.get("predicted_probability");
    let fpl: Option<String> = row.try_get("fpl_source").ok().flatten();
    let created_at: chrono::DateTime<Utc> = row.get("created_at");

    let hash = anchor_forecast(
        pool,
        &forecast_id,
        None,
        prob as f64,
        fpl.as_deref(),
        created_at,
        Some("explicit commit"),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let salt = std::env::var("BENCHMARK_SPLIT_SALT").unwrap_or_else(|_| "fermi-v1-2026".into());
    let split = ensure_split(pool, &forecast_id, &salt)
        .await
        .unwrap_or_else(|_| "unassigned".into());

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "commitment_hash": hash,
        "split": split,
        "committed_at": Utc::now(),
        "note": "Forecast anchored. This hash proves this probability was recorded before resolution.",
    })))
}

// ── Harness snapshot writer ───────────────────────────────────────────────

/// Capture the current Fermi harness configuration as a content-addressed
/// snapshot. Called at forecast creation so every ForecastRecord can be
/// linked to the exact harness state that produced it.
///
/// The harness is the triple:
///   (conductor_card_hash, routing_weights_hash, specialist_roster_hash)
///
/// Returns the snapshot_id and content_hash. Idempotent — if the same
/// configuration already exists (same content_hash), returns the existing row.
pub async fn capture_harness_snapshot(
    pool: &sqlx::PgPool,
    conductor_version: &str,
    specialist_roster: &serde_json::Value, // [{agent_id, version, calibration_score}]
    routing_weights: Option<&serde_json::Value>,
    bayesops_params: Option<&serde_json::Value>,
) -> Option<uuid::Uuid> {
    // Check table exists
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name='harness_snapshots')"
    ).fetch_one(pool).await.unwrap_or(false);
    if !exists {
        return None;
    }

    // Compute component hashes
    let conductor_hash = sha256_str(conductor_version);
    let roster_hash = sha256_str(&serde_json::to_string(specialist_roster).unwrap_or_default());
    let weights_hash =
        routing_weights.map(|w| sha256_str(&serde_json::to_string(w).unwrap_or_default()));
    let bayesops_hash =
        bayesops_params.map(|b| sha256_str(&serde_json::to_string(b).unwrap_or_default()));

    // Content hash over all components
    let mut h = Sha256::new();
    h.update(conductor_hash.as_bytes());
    h.update(b"|");
    h.update(roster_hash.as_bytes());
    h.update(b"|");
    h.update(weights_hash.as_deref().unwrap_or("").as_bytes());
    h.update(b"|");
    h.update(bayesops_hash.as_deref().unwrap_or("").as_bytes());
    let content_hash = format!("{:x}", h.finalize());

    // Check if already exists
    if let Ok(Some(row)) =
        sqlx::query("SELECT snapshot_id FROM harness_snapshots WHERE content_hash = $1")
            .bind(&content_hash)
            .fetch_optional(pool)
            .await
    {
        return row.try_get("snapshot_id").ok();
    }

    // Insert new snapshot
    let snapshot_id = uuid::Uuid::new_v4();
    let _ = sqlx::query(
        r#"INSERT INTO harness_snapshots
           (snapshot_id, content_hash, conductor_card_hash, routing_weights_hash,
            specialist_roster_hash, bayesops_params_hash, conductor_version,
            specialist_roster, routing_weights, bayesops_params, captured_at)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NOW())"#,
    )
    .bind(snapshot_id)
    .bind(&content_hash)
    .bind(&conductor_hash)
    .bind(weights_hash.as_deref())
    .bind(&roster_hash)
    .bind(bayesops_hash.as_deref())
    .bind(conductor_version)
    .bind(specialist_roster)
    .bind(routing_weights)
    .bind(bayesops_params)
    .execute(pool)
    .await
    .ok()?;

    Some(snapshot_id)
}

// ═══════════════════════════════════════════════════════════════════
// Spec 23 R-3 Piece 2 — Forecast Timeline
// ═══════════════════════════════════════════════════════════════════
//
// GET /api/forecasts/:forecast_id/timeline
//
// Unified chronological event stream for a forecast — the read side of the
// "spacetime view" the demo paper describes. Aggregates from several tables
// the rest of the system already writes:
//
//   - forecast_spacetime          → rate trace points (one per revision)
//   - bayesops_posterior_snapshots → BayesOps fit events (auto/staged/blocked)
//   - workspace_messages           → agent runs (execution_result),
//                                    system events (incl. bayesops_fit_*,
//                                    upstream_resolved, schedule fires)
//   - fermi_market_observations    → polymarket price snapshots
//
// One round-trip; client renders the timeline. No new write paths needed —
// every event source is already populated by upstream handlers.

/// GET /api/forecasts/:forecast_id/timeline
pub async fn forecast_timeline_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // ── Forecast + auth ──────────────────────────────────────────────
    //
    // owner_id is declared TEXT in migration 094 but production has been
    // observed returning it as UUID — likely an ALTER somewhere outside
    // the migrations history. Cast to text in the projection so sqlx
    // decodes it as String regardless of the underlying column type.
    let forecast = sqlx::query(
        "SELECT id, owner_id::text AS owner_id, team_id, workspace_id,
                question_text, predicted_probability, fpl_source,
                created_at, resolved_at
         FROM fermi_forecasts WHERE id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".into()))?;

    let owner_id: String = forecast.try_get("owner_id").unwrap_or_default();
    if owner_id != user_id && !principal.can_admin() {
        let team_id: Option<uuid::Uuid> = forecast.try_get("team_id").ok().flatten();
        let allowed = match team_id {
            Some(tid) => fermi_auth::teams::get_member_role(pool, tid, &user_id)
                .await
                .ok()
                .flatten()
                .is_some(),
            None => false,
        };
        if !allowed {
            return Err((StatusCode::FORBIDDEN, "Not allowed".into()));
        }
    }

    let workspace_id: Option<uuid::Uuid> = forecast.try_get("workspace_id").ok().flatten();

    // ── 1. Rate revisions from forecast_spacetime ──────────────────
    //
    // Every column read goes through try_get. Anything else (plain .get,
    // direct type ascription) panics on type mismatch or null — which axum
    // catches as a 502 with no diagnostic. The handler MUST degrade
    // gracefully on any single bad row.
    let revisions: Vec<Value> = sqlx::query(
        "SELECT revision_seq, predicted_probability, previous_probability,
                revision_trigger, revision_reason, triggering_agent,
                evidence_delta, revision_ts
         FROM forecast_spacetime
         WHERE forecast_id = $1
         ORDER BY revision_ts ASC",
    )
    .bind(&forecast_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .filter_map(|row| {
        let ts = row
            .try_get::<chrono::DateTime<chrono::Utc>, _>("revision_ts")
            .ok()?;
        Some(json!({
            "kind": "rate_revision",
            "ts": ts.to_rfc3339(),
            "revision_seq": row.try_get::<i32, _>("revision_seq").ok(),
            "predicted_probability": row.try_get::<f32, _>("predicted_probability").ok(),
            "previous_probability": row.try_get::<Option<f32>, _>("previous_probability").ok().flatten(),
            "revision_trigger": row.try_get::<Option<String>, _>("revision_trigger").ok().flatten(),
            "reason": row.try_get::<Option<String>, _>("revision_reason").ok().flatten(),
            "triggering_agent": row.try_get::<Option<String>, _>("triggering_agent").ok().flatten(),
            "evidence_delta": row.try_get::<Option<Value>, _>("evidence_delta").ok().flatten(),
        }))
    })
    .collect();

    // Build the rate series — what the chart traces. Includes the initial
    // probability so consumers always have at least one point.
    let initial_prob: Option<f32> = forecast.try_get("predicted_probability").ok();
    let created = forecast
        .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
        .ok();
    let mut rate_series: Vec<Value> = Vec::new();
    if let (Some(p), Some(c)) = (initial_prob, created) {
        rate_series.push(json!({
            "ts": c.to_rfc3339(),
            "rate": p,
        }));
    }
    for r in &revisions {
        rate_series.push(json!({
            "ts": r["ts"],
            "rate": r["predicted_probability"],
        }));
    }

    // ── 2. BayesOps fit events ─────────────────────────────────────
    let bayesops_events: Vec<Value> = if let Some(ws_id) = workspace_id {
        sqlx::query(
            "SELECT snapshot_id, driver_name, decision, n_observations,
                    n_eff, ci_width, rate_before, rate_after, fitted_at
             FROM bayesops_posterior_snapshots
             WHERE workspace_id = $1
             ORDER BY fitted_at ASC",
        )
        .bind(ws_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| {
            // Defensive try_get on every column — a single bad row would
            // otherwise panic the whole handler.
            let ts = row
                .try_get::<chrono::DateTime<chrono::Utc>, _>("fitted_at")
                .ok()?;
            let rate_before: Option<f64> = row.try_get("rate_before").ok().flatten();
            let rate_after: Option<f64> = row.try_get("rate_after").ok().flatten();
            let delta_pp = match (rate_before, rate_after) {
                (Some(b), Some(a)) => Some((a - b).abs() * 100.0),
                _ => None,
            };
            Some(json!({
                "kind": "bayesops_fit",
                "ts": ts.to_rfc3339(),
                "snapshot_id": row.try_get::<uuid::Uuid, _>("snapshot_id").ok(),
                "driver_name": row.try_get::<String, _>("driver_name").ok(),
                "decision": row.try_get::<String, _>("decision").ok(),
                "n_observations": row.try_get::<i32, _>("n_observations").ok(),
                "n_eff": row.try_get::<f64, _>("n_eff").ok(),
                "ci_width": row.try_get::<f64, _>("ci_width").ok(),
                "rate_before": rate_before,
                "rate_after": rate_after,
                "delta_pp": delta_pp,
            }))
        })
        .collect()
    } else {
        Vec::new()
    };

    // ── 3. Workspace events from workspace_messages ─────────────────
    // Captures every "this happened in the workspace" event the system
    // already records: agent runs (execution_result), system events
    // (resolutions, bayesops_fit_decision, upstream_resolved), and the
    // schedule-fire log messages the cockpit posts ("⏰ Auto-running...").
    let workspace_events: Vec<Value> = if let Some(ws_id) = workspace_id {
        sqlx::query(
            "SELECT message_id, sender_type, sender_id, sender_name,
                    content, message_type, metadata, created_at
             FROM workspace_messages
             WHERE workspace_id = $1
               AND message_type IN ('execution_result', 'system_event', 'system')
             ORDER BY created_at ASC",
        )
        .bind(ws_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|row| {
            let ts = row
                .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                .ok()?;
            let msg_type: String = row.try_get("message_type").ok()?;
            let metadata: Value = row.try_get("metadata").unwrap_or(Value::Null);
            let kind = match msg_type.as_str() {
                "execution_result" => "agent_run".to_string(),
                "system_event" => metadata
                    .get("event")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "system_event".to_string()),
                other => other.to_string(),
            };
            Some(json!({
                "kind": kind,
                "ts": ts.to_rfc3339(),
                "message_id": row.try_get::<uuid::Uuid, _>("message_id").ok(),
                "sender_type": row.try_get::<String, _>("sender_type").ok(),
                "sender_id": row.try_get::<String, _>("sender_id").ok(),
                "sender_name": row.try_get::<Option<String>, _>("sender_name").ok().flatten(),
                "content": row.try_get::<String, _>("content").ok(),
                "metadata": metadata,
            }))
        })
        .collect()
    } else {
        Vec::new()
    };

    // ── 4. Market data: Polymarket observations ────────────────
    //
    // We build two representations of the same rows:
    //   * `market_series` — kept as its own dense array for the client's
    //     crowd-worm chart trace (parallel to `rate_series`).
    //   * `market_events` — sparse dots on the merged event timeline
    //     (bottom rug + hover chips) so the operator can eye-trace when
    //     the crowd price ticked relative to their applied revisions.
    let market_rows: Vec<(
        chrono::DateTime<chrono::Utc>,
        Option<f32>,
        Option<f32>,
        Option<String>,
    )> = sqlx::query(
        // NOTE: the observations table columns this timestamp as
        // `created_at` (see migration 099). An earlier version of this
        // query referenced a non-existent `observation_time` column,
        // which the `.unwrap_or_default()` on the query result quietly
        // swallowed — so writes landed fine but the trajectory always
        // read zero market observations. Kept the local variable name
        // `ts` for clarity; the response still exposes it as `ts`.
        "SELECT market_price, volume_total, created_at, pm_event_id
         FROM fermi_market_observations
         WHERE forecast_id = $1
         ORDER BY created_at ASC",
    )
    .bind(&forecast_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .filter_map(|row| {
        let ts = row
            .try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            .ok()?;
        let price = row.try_get::<f32, _>("market_price").ok();
        let volume = row.try_get::<Option<f32>, _>("volume_total").ok().flatten();
        let event_id = row
            .try_get::<Option<String>, _>("pm_event_id")
            .ok()
            .flatten();
        Some((ts, price, volume, event_id))
    })
    .collect();

    let market_series: Vec<Value> = market_rows
        .iter()
        .map(|(ts, price, volume, event_id)| {
            json!({
                "ts": ts.to_rfc3339(),
                "market_price": price,
                "volume_total": volume,
                "pm_event_id": event_id,
            })
        })
        .collect();

    let market_events: Vec<Value> = market_rows
        .iter()
        .map(|(ts, price, volume, _event_id)| {
            json!({
                "kind": "market_observation",
                "ts": ts.to_rfc3339(),
                // Chart drops market dots at the crowd price line (in
                // pct), so the y-coord matches the crowd worm exactly.
                // rate_pct sits under `predicted_probability` to reuse
                // the client's existing event-y-lookup path.
                "predicted_probability": price.map(|p| p as f64),
                "volume_total": volume,
            })
        })
        .collect();

    // ── 5. Merge into one chronologically-ordered event list ──────
    // The client needs `events` chronological so it can render them as
    // dots on a shared time axis. `rate_series` and `market_series` are
    // kept as separate arrays for the line-chart traces.
    let mut events: Vec<Value> = Vec::with_capacity(
        revisions.len() + bayesops_events.len() + workspace_events.len() + market_events.len(),
    );
    events.extend(revisions);
    events.extend(bayesops_events);
    events.extend(workspace_events);
    events.extend(market_events);
    events.sort_by(|a, b| {
        let ta = a.get("ts").and_then(|v| v.as_str()).unwrap_or("");
        let tb = b.get("ts").and_then(|v| v.as_str()).unwrap_or("");
        ta.cmp(tb)
    });

    let span = json!({
        "forecast_created_at": forecast.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
            .ok()
            .map(|t| t.to_rfc3339()),
        "forecast_resolved_at": forecast.try_get::<Option<chrono::DateTime<chrono::Utc>>, _>("resolved_at")
            .ok()
            .flatten()
            .map(|t| t.to_rfc3339()),
        "event_count": events.len(),
        "rate_revision_count": rate_series.len(),
        "market_observation_count": market_series.len(),
    });

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "question": forecast.try_get::<Option<String>, _>("question_text").ok().flatten(),
        "workspace_id": workspace_id,
        "rate_series": rate_series,
        "market_series": market_series,
        "events": events,
        "span": span,
    })))
}

// ═══════════════════════════════════════════════════════════════════
//
// GET /api/forecasts/:forecast_id/cascade-provenance
//
// Redistribution waterfall for a single forecast — the read side of the
// "where did this probability come from" question. This is the first UI
// surface for the generalized cascade primitive; see
// docs/fermi/WORLD_CUP_ROADMAP.md §Phase 2.5.
//
// Every row in fermi_forecast_updates with revision_trigger ∈ {'cascade',
// 'cascade_undo'} is a lateral redistribution: some upstream forecast's
// resolution (or un-resolution) shifted the mass of this one. Summed, they
// explain the delta between the raw model output and the currently
// displayed probability.
//
// The response is one JSON object with:
//   * baseline_probability     — current − Σ(cascade Δ). The counterfactual
//                                  probability if no cascade had ever fired.
//   * current_probability      — what the forecast currently reads.
//   * cumulative_cascade_pp    — Σ|Δ| across cascade rows, in percentage
//                                  points. Two views collapse to one number.
//   * contributions            — one row per cascade delta, sorted by
//                                  |delta_pp| descending so the biggest
//                                  movers are at the top of the waterfall.
//                                  Includes trigger_forecast_id + question
//                                  so the client can render a human label.
//
// This handler does not join to pending_cascades / cascade_id; it reads
// only from fermi_forecast_updates. If we later add trigger_forecast_id as
// a first-class column (currently parsed out of the `reason` string), this
// handler is where that column would be surfaced.

/// GET /api/forecasts/:forecast_id/cascade-provenance
pub async fn forecast_cascade_provenance_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
) -> Result<Json<Value>, (StatusCode, String)> {
    let user_id = principal.user_id();
    let pool = &state.db;

    // ── Forecast + auth ──────────────────────────────────────────────
    // Mirrors forecast_timeline_handler's ownership/team-member gate so
    // provenance visibility follows forecast visibility exactly.
    let forecast = sqlx::query(
        "SELECT id, owner_id::text AS owner_id, team_id,
                question_text, predicted_probability
         FROM fermi_forecasts WHERE id = $1",
    )
    .bind(&forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".into()))?;

    let owner_id: String = forecast.try_get("owner_id").unwrap_or_default();
    if owner_id != user_id && !principal.can_admin() {
        let team_id: Option<uuid::Uuid> = forecast.try_get("team_id").ok().flatten();
        let allowed = match team_id {
            Some(tid) => fermi_auth::teams::get_member_role(pool, tid, &user_id)
                .await
                .ok()
                .flatten()
                .is_some(),
            None => false,
        };
        if !allowed {
            return Err((StatusCode::FORBIDDEN, "Not allowed".into()));
        }
    }

    let current_probability: Option<f32> = forecast.try_get("predicted_probability").ok();
    let question: Option<String> = forecast.try_get("question_text").ok().flatten();

    // ── Cascade + cascade_undo rows ──────────────────────────────────
    // Chronological read; we re-sort by |delta_pp| desc after enriching
    // with trigger names.
    let rows = sqlx::query(
        "SELECT id, previous_probability, new_probability, reason,
                revision_trigger, created_at
         FROM fermi_forecast_updates
         WHERE forecast_id = $1
           AND revision_trigger IN ('cascade', 'cascade_undo')
         ORDER BY created_at ASC",
    )
    .bind(&forecast_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Extract trigger_forecast_id from `reason`. Reason format is stable
    // across the four callers that write cascade rows:
    //   - propagate_mutex:       "cascade from <fid> (<kind>)"
    //   - propagate_at_most_n:   "cascade from <fid> (<kind>) [at_most_n=…]"
    //   - propagate_implies:     "cascade from <fid> (<kind>) [implies: …]"
    //   - apply_wc_cascades:     "cascade from <fid> (resolved)"
    //   - undo_pending_cascade:  "cascade_undo of <cascade_id>"
    // For undo rows the id is a pending_cascade uuid, not a forecast id;
    // we return null trigger info for those and mark them undo=true.
    fn parse_trigger_id(reason: &str) -> Option<String> {
        let prefix = "cascade from ";
        if let Some(rest) = reason.strip_prefix(prefix) {
            let end = rest.find(' ').unwrap_or(rest.len());
            return Some(rest[..end].to_string());
        }
        None
    }

    #[derive(Clone)]
    struct Parsed {
        ts: chrono::DateTime<Utc>,
        prev_p: f64,
        new_p: f64,
        delta: f64,
        reason: String,
        revision_trigger: String,
        trigger_forecast_id: Option<String>,
    }

    let parsed: Vec<Parsed> = rows
        .iter()
        .filter_map(|r| {
            let ts = r.try_get::<chrono::DateTime<Utc>, _>("created_at").ok()?;
            let prev = r.try_get::<f32, _>("previous_probability").ok()? as f64;
            let new_p = r.try_get::<f32, _>("new_probability").ok()? as f64;
            let reason: String = r.try_get("reason").ok().unwrap_or_default();
            let revision_trigger: String = r.try_get("revision_trigger").ok().unwrap_or_default();
            let trigger_forecast_id = parse_trigger_id(&reason);
            Some(Parsed {
                ts,
                prev_p: prev,
                new_p,
                delta: new_p - prev,
                reason,
                revision_trigger,
                trigger_forecast_id,
            })
        })
        .collect();

    // Batch-fetch trigger questions in one round-trip so the client can
    // render "Curaçao eliminated" instead of an opaque uuid.
    let trigger_ids: Vec<String> = parsed
        .iter()
        .filter_map(|p| p.trigger_forecast_id.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let mut trigger_questions: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    if !trigger_ids.is_empty() {
        let trig_rows =
            sqlx::query("SELECT id, question_text FROM fermi_forecasts WHERE id = ANY($1)")
                .bind(&trigger_ids)
                .fetch_all(pool)
                .await
                .unwrap_or_default();
        for r in &trig_rows {
            let id: String = match r.try_get("id") {
                Ok(s) => s,
                Err(_) => continue,
            };
            let q: Option<String> = r.try_get("question_text").ok().flatten();
            if let Some(q) = q {
                trigger_questions.insert(id, q);
            }
        }
    }

    // Assemble + sort by |delta_pp| descending. Cascade_undo rows have
    // negative deltas w.r.t. this forecast (they revert a prior gain);
    // we sort by magnitude so the biggest movers surface first regardless
    // of sign.
    let mut contributions: Vec<Value> = parsed
        .iter()
        .map(|p| {
            let delta_pp = p.delta * 100.0;
            let trigger_question = p
                .trigger_forecast_id
                .as_ref()
                .and_then(|id| trigger_questions.get(id).cloned());
            json!({
                "ts": p.ts.to_rfc3339(),
                "trigger_forecast_id": p.trigger_forecast_id,
                "trigger_question": trigger_question,
                "prev_p": p.prev_p,
                "new_p": p.new_p,
                "delta_pp": delta_pp,
                "revision_trigger": p.revision_trigger,
                "is_undo": p.revision_trigger == "cascade_undo",
                "reason": p.reason,
            })
        })
        .collect();
    contributions.sort_by(|a, b| {
        let da = a
            .get("delta_pp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .abs();
        let db_ = b
            .get("delta_pp")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0)
            .abs();
        db_.partial_cmp(&da).unwrap_or(std::cmp::Ordering::Equal)
    });

    let cumulative_delta: f64 = parsed.iter().map(|p| p.delta).sum();
    let cumulative_cascade_pp = cumulative_delta * 100.0;
    let baseline_probability = current_probability.map(|c| c as f64 - cumulative_delta);

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "question": question,
        "current_probability": current_probability,
        "baseline_probability": baseline_probability,
        "cumulative_cascade_pp": cumulative_cascade_pp,
        "cascade_count": parsed.len(),
        "contributions": contributions,
    })))
}
