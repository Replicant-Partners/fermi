//! Forecast version history, backed by the workspace git substrate.
//!
//! Spec 31. See `docs/specs/SPEC_31_FORECAST_HISTORY.md`.
//!
//! ## Why git and not a table
//!
//! ABW already gives every workspace a real git repo
//! (`agent_bestiary_ontology::WorkspaceGitManager`, git2-backed: commit,
//! log, diff, read-at-SHA). Every forecast already has a workspace. The
//! substrate was **built and idle** — 48 forecast workspaces, zero
//! `git_repo_path` values, one commit in total — so forecast versioning was
//! about to be reimplemented in SQL next to a finished implementation of
//! it.
//!
//! ## What gets versioned
//!
//! The FPL program is *generated* from structured state (drivers, evidence,
//! agent output), so versioning `fpl_source` as a column would version a
//! build artifact while its inputs stayed unversioned. Instead each commit
//! materialises the whole state as files:
//!
//! | file | from |
//! |---|---|
//! | `forecast.fpl` | `fpl_source` — the diffable artifact |
//! | `drivers.json` | `drivers` — the real inputs |
//! | `evidence.json` | `evidence` |
//! | `agents.json` | `agents_used` |
//! | `state.json` | probability, status, target date, visibility |
//! | `README.md` | question, resolution criteria |
//!
//! `git diff` over that set is a genuinely readable account of what a
//! teammate changed, which is the thing that was impossible before: an FPL
//! edit that didn't move the probability previously left **no trace at
//! all** (`update_forecast_handler` only wrote a revision row when the
//! probability moved >0.001, and `forecast_spacetime` is populated by a
//! trigger on *that* insert).
//!
//! ## The collaboration model this enables
//!
//! Ward Cunningham's wiki bet: **reversibility beats prevention.** Shared
//! write, complete history, trivial revert — no locking, no merge, no
//! review gates. A clobber stops being data loss and becomes "a commit;
//! here's the diff; revert it".
//!
//! Which is why this module is the prerequisite for handing out `edit`
//! freely. `edit` was only ever frightening because there was no undo.
//!
//! Terminal actions (resolve/void) stay gated on Spec 30 capabilities,
//! because those genuinely cannot be undone — mig-174 freezes the scoring
//! tuple. The line that matters is **revertible vs terminal**, not
//! viewer/editor/owner.

use agent_bestiary_ontology::{CommitAuthor, WorkspaceCommit, WorkspaceGitManager};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use fermi_auth::visibility::{can_edit, can_view};
use fermi_auth::{AuthPrincipal, ObjectType, Visibility};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use sqlx::{PgPool, Row};

use crate::AppState;

/// Files the forecast repo carries. Order is stable so diffs read the same
/// way every time.
const FPL_FILE: &str = "forecast.fpl";
const DRIVERS_FILE: &str = "drivers.json";
const EVIDENCE_FILE: &str = "evidence.json";
const AGENTS_FILE: &str = "agents.json";
const STATE_FILE: &str = "state.json";
const README_FILE: &str = "README.md";

// ═══════════════════════════════════════════════════════════════════════
// Repo resolution
// ═══════════════════════════════════════════════════════════════════════

/// The workspace slug backing a forecast's repo, creating the workspace if
/// the forecast doesn't have one yet.
///
/// 38% of forecasts had no `workspace_id` when this shipped, so "every
/// forecast is its own workspace" was aspiration rather than fact. Rather
/// than backfill once and drift again, resolution is lazy and idempotent:
/// the first versioned action on a forecast provisions its workspace.
///
/// Also writes `teams.git_repo_path`, which nothing populated before —
/// the manager derives paths from the slug, so the column was dead, and
/// nothing could tell whether a repo existed without touching the disk.
async fn ensure_forecast_repo(
    pool: &PgPool,
    git: &WorkspaceGitManager,
    forecast_id: &str,
) -> Result<String, String> {
    let row = sqlx::query(
        "SELECT f.workspace_id::text AS workspace_id,
                f.question_text,
                f.owner_id::text AS owner_id,
                t.slug            AS slug,
                t.git_repo_path   AS git_repo_path
           FROM fermi_forecasts f
           LEFT JOIN teams t ON t.id = f.workspace_id
          WHERE f.id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "forecast not found".to_string())?;

    let existing_slug: Option<String> = row.try_get("slug").ok().flatten();
    if let Some(slug) = existing_slug {
        // Record the repo path the first time we touch it, so callers can
        // answer "is this versioned yet" from the DB.
        if row
            .try_get::<Option<String>, _>("git_repo_path")
            .ok()
            .flatten()
            .is_none()
        {
            let _ = sqlx::query(
                "UPDATE teams SET git_repo_path = $1 WHERE id = (
                     SELECT workspace_id FROM fermi_forecasts WHERE id = $2)",
            )
            .bind(format!("workspaces/{}", slug))
            .bind(forecast_id)
            .execute(pool)
            .await;
        }
        let _ = git.init_or_open(&slug).map_err(|e| e.to_string())?;
        return Ok(slug);
    }

    // No workspace: mint one. `origin` marks it as forecast-backing so the
    // console's team filters keep excluding it from the collaboration team
    // list — these are per-forecast plumbing, not human teams.
    let question: String = row.try_get("question_text").unwrap_or_default();
    let owner_id: String = row.try_get("owner_id").unwrap_or_default();
    let short: String = forecast_id.chars().take(8).collect();
    let slug = format!("forecast-{}", short);
    let name = format!(
        "Forecast — {}",
        question.chars().take(60).collect::<String>()
    );

    sqlx::query(
        "INSERT INTO teams (name, slug, owner_id, origin, git_repo_path)
         VALUES ($1, $2, $3, 'fermi_forecast_repo', $4)
         ON CONFLICT (slug) DO NOTHING",
    )
    .bind(&name)
    .bind(&slug)
    .bind(&owner_id)
    .bind(format!("workspaces/{}", slug))
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    sqlx::query(
        "UPDATE fermi_forecasts
            SET workspace_id = (SELECT id FROM teams WHERE slug = $1)
          WHERE id = $2 AND workspace_id IS NULL",
    )
    .bind(&slug)
    .bind(forecast_id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    let _ = git.init_or_open(&slug).map_err(|e| e.to_string())?;
    Ok(slug)
}

// ═══════════════════════════════════════════════════════════════════════
// The commit hook
// ═══════════════════════════════════════════════════════════════════════

/// Materialise a forecast's current state and commit it, attributed.
///
/// **The single hook every mutating path calls.** One helper on purpose:
/// the same discipline as the ACL predicate. Nine writers already touch
/// `predicted_probability`; if each had to remember to commit, the history
/// would have holes exactly where the interesting edits are.
///
/// Best-effort by contract. A git failure must never fail the user's save:
/// the DB is truth, and the repo is a derived record of it. Losing a commit
/// costs a line of history; failing the save costs the operator their work.
/// Failures are logged loudly because a silently unversioned forecast
/// defeats the whole point.
pub async fn commit_forecast_state(
    pool: &PgPool,
    git: &WorkspaceGitManager,
    forecast_id: &str,
    actor: Option<&CommitAuthor>,
    action: &str,
) -> Option<WorkspaceCommit> {
    let slug = match ensure_forecast_repo(pool, git, forecast_id).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(forecast = %forecast_id, error = %e, "[forecast-git] no repo; state not versioned");
            return None;
        }
    };

    let files = match materialise(pool, forecast_id).await {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(forecast = %forecast_id, error = %e, "[forecast-git] could not materialise state");
            return None;
        }
    };

    // "Alice Labra: revised probability" — the author is a git first-class
    // field, so `git log` alone answers "who changed what".
    let message = match actor {
        Some(a) => format!("{}: {}", a.name, action),
        None => format!("system: {}", action),
    };

    match git
        .commit_files_as_async(slug, files, message, actor.cloned())
        .await
    {
        Ok(Some(c)) => {
            let _ = sqlx::query(
                "UPDATE teams SET git_latest_commit = $1,
                                  git_commit_count = git_commit_count + 1
                  WHERE id = (SELECT workspace_id FROM fermi_forecasts WHERE id = $2)",
            )
            .bind(&c.sha)
            .bind(forecast_id)
            .execute(pool)
            .await;
            Some(c)
        }
        // Unchanged tree — the action didn't alter versioned state (a
        // metadata-only touch, or a re-save of identical content). Not an
        // error and not worth a phantom revision.
        Ok(None) => None,
        Err(e) => {
            tracing::warn!(forecast = %forecast_id, error = %e, "[forecast-git] commit failed");
            None
        }
    }
}

// ─── Call-site conveniences ───────────────────────────────────
//
// The hook has to be trivial to call or writers will skip it, and a history
// with holes exactly where cascades and refits happened is worse than no
// history — it looks complete while omitting the interesting edits.

/// Commit on behalf of a human principal. The common case.
pub async fn commit_for(
    state: &AppState,
    forecast_id: &str,
    principal: &AuthPrincipal,
    action: &str,
) {
    let author = author_for(&state.db, principal).await;
    commit_forecast_state(
        &state.db,
        &state.workspace_git,
        forecast_id,
        Some(&author),
        action,
    )
    .await;
}

/// Commit an genuinely systemic change — a cron sweep, an auto-resolution
/// from a settled market. Attributed to the platform identity, which is
/// honest: no human decided it.
pub async fn commit_system(state: &AppState, forecast_id: &str, action: &str) {
    commit_forecast_state(&state.db, &state.workspace_git, forecast_id, None, action).await;
}

/// Commit a set of forecasts touched by one act — a cascade.
///
/// Called at the handler boundary rather than inside the propagation
/// recursion: the recursive helpers take a bare `PgPool`, and threading the
/// git manager through them would spread the hook across the very code
/// paths most likely to be refactored. One call per affected forecast,
/// each landing in its own repo, all carrying the same action string so the
/// cascade is recognisable across the histories it touched.
pub async fn commit_cascade(
    state: &AppState,
    forecast_ids: &[String],
    principal: Option<&AuthPrincipal>,
    action: &str,
) {
    let author = match principal {
        Some(p) => Some(author_for(&state.db, p).await),
        None => None,
    };
    for id in forecast_ids {
        commit_forecast_state(&state.db, &state.workspace_git, id, author.as_ref(), action).await;
    }
}

/// Resolve the acting principal into a git author.
///
/// Falls back to the user id when a display name is missing, and always
/// carries the email — which is what disambiguates the three accounts all
/// displaying as the same name (the collision that made a 403 impossible to
/// diagnose). Git's author field is `Name <email>`, so this is exactly the
/// slot for it.
pub async fn author_for(pool: &PgPool, principal: &AuthPrincipal) -> CommitAuthor {
    let uid = principal.user_id();
    let row = sqlx::query(
        "SELECT COALESCE(display_name, name, email, user_id) AS label, email
           FROM users WHERE user_id = $1",
    )
    .bind(&uid)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    match row {
        Some(r) => CommitAuthor {
            name: r
                .try_get::<String, _>("label")
                .unwrap_or_else(|_| uid.clone()),
            email: r
                .try_get::<Option<String>, _>("email")
                .ok()
                .flatten()
                // git requires an email; a stable synthetic one keeps
                // commits attributable even for accounts without one.
                .unwrap_or_else(|| format!("{}@users.noreply.fermi", uid)),
        },
        None => CommitAuthor {
            name: uid.clone(),
            email: format!("{}@users.noreply.fermi", uid),
        },
    }
}

/// Render the forecast's current DB state as the repo's file set.
async fn materialise(pool: &PgPool, forecast_id: &str) -> Result<Vec<(String, String)>, String> {
    let r = sqlx::query(
        "SELECT question_text, resolution_criteria, domain, target_date,
                fpl_source, drivers, evidence, agents_used,
                predicted_probability, confidence_interval_low, confidence_interval_high,
                status, visibility, tags, owner_id::text AS owner_id
           FROM fermi_forecasts WHERE id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "forecast not found".to_string())?;

    let question: String = r.try_get("question_text").unwrap_or_default();
    let criteria: Option<String> = r.try_get("resolution_criteria").ok().flatten();
    let domain: Option<String> = r.try_get("domain").ok().flatten();
    let fpl: Option<String> = r.try_get("fpl_source").ok().flatten();
    let prob: f32 = r.try_get("predicted_probability").unwrap_or(0.0);
    let status: String = r.try_get("status").unwrap_or_default();
    let visibility: String = r.try_get("visibility").unwrap_or_default();
    let tags: Vec<String> = r.try_get("tags").unwrap_or_default();
    let target: Option<chrono::DateTime<chrono::Utc>> = r.try_get("target_date").ok().flatten();

    // Pretty-printed JSON on purpose: a minified blob produces a one-line
    // diff for every change, which defeats the point of committing it.
    let pretty = |v: Option<JsonValue>| -> String {
        serde_json::to_string_pretty(&v.unwrap_or(JsonValue::Array(vec![])))
            .unwrap_or_else(|_| "[]".into())
    };

    let state = json!({
        "predicted_probability": prob,
        "confidence_interval": {
            "low":  r.try_get::<Option<f32>, _>("confidence_interval_low").ok().flatten(),
            "high": r.try_get::<Option<f32>, _>("confidence_interval_high").ok().flatten(),
        },
        "status":      status,
        "visibility":  visibility,
        "target_date": target.map(|t| t.to_rfc3339()),
        "tags":        tags,
    });

    let readme = format!(
        "# {}\n\n\
         - **Domain:** {}\n\
         - **Target date:** {}\n\
         - **Status:** {}\n\n\
         ## Resolution criteria\n\n{}\n\n\
         ---\n\
         Maintained by Fermi. `forecast.fpl` is generated from `drivers.json`;\n\
         edit the drivers, not the program.\n",
        question,
        domain.unwrap_or_else(|| "—".into()),
        target
            .map(|t| t.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "—".into()),
        status,
        criteria.unwrap_or_else(|| "_Not specified._".into()),
    );

    Ok(vec![
        (FPL_FILE.into(), fpl.unwrap_or_default()),
        (DRIVERS_FILE.into(), pretty(r.try_get("drivers").ok())),
        (EVIDENCE_FILE.into(), pretty(r.try_get("evidence").ok())),
        (AGENTS_FILE.into(), pretty(r.try_get("agents_used").ok())),
        (
            STATE_FILE.into(),
            serde_json::to_string_pretty(&state).unwrap_or_else(|_| "{}".into()),
        ),
        (README_FILE.into(), readme),
    ])
}

// ═══════════════════════════════════════════════════════════════════════
// GET /api/forecasts/:id/history
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, Default)]
pub struct HistoryQuery {
    pub limit: Option<usize>,
}

/// The forecast's commit history — who changed it, when, and why.
///
/// Gated on `view`: if you can read the forecast you can read how it got
/// that way. Provenance is not a privilege; a forecast whose history is
/// hidden from its readers is just an assertion.
pub async fn forecast_history_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_view(pool, &forecast_id, &principal).await?;

    let slug = match forecast_workspace_slug(pool, &forecast_id).await {
        Some(s) => s,
        // Never versioned. An honest empty answer with the reason, rather
        // than an error the UI would render as a failure.
        None => {
            return Ok(Json(json!({
                "forecast_id": forecast_id,
                "versioned":   false,
                "commits":     [],
                "note": "This forecast has no version history yet. \
                         History starts at its next save.",
            })))
        }
    };

    let limit = q.limit.unwrap_or(50).clamp(1, 200);
    let commits = state
        .workspace_git
        .get_log_async(slug, limit)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let out: Vec<JsonValue> = commits
        .iter()
        .map(|c| {
            json!({
                "sha":       c.sha,
                "short_sha": c.sha.chars().take(8).collect::<String>(),
                "message":   c.message,
                "author":    c.author,
                "timestamp": c.timestamp.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "versioned":   true,
        "commits":     out,
        "count":       out.len(),
    })))
}

// ═══════════════════════════════════════════════════════════════════════
// GET /api/forecasts/:id/history/:sha
// ═══════════════════════════════════════════════════════════════════════

/// What one commit changed, as a unified diff against its parent.
///
/// `?against=<sha>` compares two arbitrary revisions instead, which is how
/// "what has Mario changed since Monday" gets answered.
pub async fn forecast_diff_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path((forecast_id, sha)): Path<(String, String)>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    require_view(pool, &forecast_id, &principal).await?;

    let slug = forecast_workspace_slug(pool, &forecast_id)
        .await
        .ok_or((StatusCode::NOT_FOUND, "No history for this forecast".into()))?;

    // Default comparison is against the parent, which is what "what did
    // this commit do" means. `git` spells the parent `<sha>^`; resolving it
    // here keeps that syntax out of the client.
    let from = params
        .get("against")
        .cloned()
        .unwrap_or_else(|| format!("{}^", sha));

    let diff = state
        .workspace_git
        .diff_commits_async(slug, from.clone(), sha.clone())
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    Ok(Json(json!({
        "forecast_id": forecast_id,
        "from": from,
        "to":   sha,
        "diff": diff,
    })))
}

// ═══════════════════════════════════════════════════════════════════════
// POST /api/forecasts/:id/revert
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct RevertRequest {
    /// The commit to restore state from.
    pub sha: String,
    pub reason: Option<String>,
}

/// Restore a forecast's analysis to an earlier revision.
///
/// **This is the load-bearing feature of the whole model.** Shared `edit`
/// is only safe to hand out because any change can be undone; without
/// revert, history is an audit log and `edit` is still frightening.
///
/// Two deliberate limits:
///
/// 1. **Gated on `edit`, not admin.** Reverting is itself revertible — it
///    writes a new commit rather than rewriting history — so it belongs
///    with the other reversible powers. Treating undo as more privileged
///    than the edit it undoes would be backwards.
///
/// 2. **Restores the analysis, never the lifecycle.** Probability,
///    drivers, evidence and FPL come back; `status`, `resolved_at`,
///    `actual_outcome` and `brier_score` do not. A resolved forecast cannot
///    be un-resolved — mig-174 freezes the scoring tuple precisely so a
///    score can't be quietly rewritten, and revert must not be a hole in
///    that. Reverting a resolved forecast is refused outright rather than
///    silently doing half the job.
pub async fn forecast_revert_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(forecast_id): Path<String>,
    Json(body): Json<RevertRequest>,
) -> Result<Json<JsonValue>, (StatusCode, String)> {
    let pool = &state.db;
    let (_owner, _vis, status) = require_edit(pool, &forecast_id, &principal).await?;

    if status == "resolved" || status == "voided" {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "Cannot revert a {} forecast. Its score is frozen — reverting the \
                 analysis behind a recorded Brier would make the score unreproducible.",
                status
            ),
        ));
    }

    let slug = forecast_workspace_slug(pool, &forecast_id)
        .await
        .ok_or((StatusCode::NOT_FOUND, "No history for this forecast".into()))?;

    let git = state.workspace_git.clone();
    let read = |path: &'static str| {
        let git = git.clone();
        let slug = slug.clone();
        let sha = body.sha.clone();
        async move {
            git.read_file_at_async(slug, path.into(), sha)
                .await
                .ok()
                .flatten()
        }
    };

    let fpl = read(FPL_FILE).await;
    let drivers = read(DRIVERS_FILE).await;
    let evidence = read(EVIDENCE_FILE).await;
    let state_json = read(STATE_FILE).await;

    if fpl.is_none() && drivers.is_none() && state_json.is_none() {
        return Err((
            StatusCode::NOT_FOUND,
            format!("Commit {} carries no forecast state", body.sha),
        ));
    }

    let prev_prob: Option<f32> = state_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<JsonValue>(s).ok())
        .and_then(|v| v.get("predicted_probability").and_then(|p| p.as_f64()))
        .map(|p| p as f32);

    let parse_json = |s: Option<String>| -> Option<JsonValue> {
        s.and_then(|t| serde_json::from_str::<JsonValue>(&t).ok())
    };

    // Read the live probability first so the revision row records a true
    // before/after pair — the trajectory must show the revert as the move
    // it actually was.
    let current_prob: f32 =
        sqlx::query_scalar("SELECT predicted_probability FROM fermi_forecasts WHERE id = $1")
            .bind(&forecast_id)
            .fetch_one(pool)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    sqlx::query(
        "UPDATE fermi_forecasts
            SET fpl_source            = COALESCE($2, fpl_source),
                drivers               = COALESCE($3, drivers),
                evidence              = COALESCE($4, evidence),
                predicted_probability = COALESCE($5, predicted_probability),
                updated_at            = NOW()
          WHERE id = $1",
    )
    .bind(&forecast_id)
    .bind(&fpl)
    .bind(parse_json(drivers))
    .bind(parse_json(evidence))
    .bind(prev_prob)
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let user_id = principal.user_id();
    let short: String = body.sha.chars().take(8).collect();
    let reason = body
        .reason
        .clone()
        .unwrap_or_else(|| format!("Reverted to {}", short));

    if let Some(p) = prev_prob {
        if (p - current_prob).abs() > 0.001 {
            let _ = sqlx::query(
                "INSERT INTO fermi_forecast_updates
                   (id, forecast_id, previous_probability, new_probability,
                    reason, actor_user_id, revision_trigger, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, 'manual', NOW())",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(&forecast_id)
            .bind(current_prob)
            .bind(p)
            .bind(&reason)
            .bind(&user_id)
            .execute(pool)
            .await;
        }
    }

    // Forward commit, never a history rewrite: the revert is itself an
    // event in the record, and can be reverted in turn.
    let author = author_for(pool, &principal).await;
    let commit = commit_forecast_state(
        pool,
        &state.workspace_git,
        &forecast_id,
        Some(&author),
        &format!("revert to {} — {}", short, reason),
    )
    .await;

    Ok(Json(json!({
        "forecast_id":   forecast_id,
        "reverted_to":   body.sha,
        "new_commit":    commit.map(|c| c.sha),
        "probability":   prev_prob,
    })))
}

// ═══════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════

async fn forecast_workspace_slug(pool: &PgPool, forecast_id: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT t.slug FROM fermi_forecasts f
           JOIN teams t ON t.id = f.workspace_id
          WHERE f.id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

async fn require_view(
    pool: &PgPool,
    forecast_id: &str,
    principal: &AuthPrincipal,
) -> Result<(), (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility FROM fermi_forecasts WHERE id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".to_string()))?;

    let owner: String = row.try_get("owner_id").unwrap_or_default();
    let vis: String = row.try_get("visibility").unwrap_or_default();
    let ok = can_view(
        pool,
        principal,
        ObjectType::Forecast,
        forecast_id,
        &owner,
        Visibility::from_legacy(&vis),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !ok {
        return Err((StatusCode::FORBIDDEN, "Access denied".into()));
    }
    Ok(())
}

/// Returns `(owner_id, visibility, status)`.
async fn require_edit(
    pool: &PgPool,
    forecast_id: &str,
    principal: &AuthPrincipal,
) -> Result<(String, String, String), (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT owner_id::text AS owner_id, visibility, status
           FROM fermi_forecasts WHERE id = $1",
    )
    .bind(forecast_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, "Forecast not found".to_string()))?;

    let owner: String = row.try_get("owner_id").unwrap_or_default();
    let vis: String = row.try_get("visibility").unwrap_or_default();
    let status: String = row.try_get("status").unwrap_or_default();

    let ok = can_edit(
        pool,
        principal,
        ObjectType::Forecast,
        forecast_id,
        &owner,
        Visibility::from_legacy(&vis),
    )
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if !ok {
        return Err((
            StatusCode::FORBIDDEN,
            "Edit access required. Ask the owner to share this forecast with you \
             at edit, or with a team you're on."
                .into(),
        ));
    }
    Ok((owner, vis, status))
}
