//! Workspace introspection → draft App manifest.
//!
//! The third frontend over the apps::builder substrate. Reads a workspace's
//! current state and synthesizes a *draft* `PartialManifest` plus a list of
//! suggestions about what looks intentional vs incidental.
//!
//! Conservative heuristics by design — false positives are worse than missing
//! cleanup opportunities, because the user reviews the draft before publish.
//! Each "incidental?" finding is surfaced as a Suggestion with a Fix patch,
//! never silently dropped.
//!
//! Inputs:
//!   - workspace_id (teams.id)
//!   - owner_id (caller user_id, used to verify access)
//!
//! Outputs (returned together via `introspect_workspace_to_draft`):
//!   - A `PartialManifest` with slug, name, fleet (auto_hire), workspace_template
//!     (initial_budget, initial_files), and a draft description/tagline.
//!   - A list of `Issue`s — mostly Suggestions about what to review:
//!       - "Agent X was hired but never used — recommend removing from auto_hire"
//!       - "File scratch.txt looks incidental — recommend excluding"
//!       - "Workspace coherence score is N — consider running an evaluation
//!         pass before publishing"
//!
//! The caller (the `Save workspace as App` endpoint) returns the draft to the
//! UI for review; the UI applies any patches the user accepts and POSTs to
//! `/api/apps` (the public create-app endpoint) once finalized.

use crate::apps::builder::{Fix, Issue, PartialManifest, Severity};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};

/// Result of forking a workspace to a draft App manifest.
#[derive(Debug, Clone)]
pub struct WorkspaceForkDraft {
    pub manifest: PartialManifest,
    pub issues: Vec<Issue>,
    /// Workspace name/mission/coordination strategist captured at fork time —
    /// useful for the UI to show next to the draft for context.
    pub source: WorkspaceForkSource,
}

#[derive(Debug, Clone)]
pub struct WorkspaceForkSource {
    pub workspace_id: uuid::Uuid,
    pub workspace_name: String,
    pub workspace_slug: Option<String>,
    pub mission: Option<String>,
    pub composition_strategist_id: Option<uuid::Uuid>,
    pub origin: Option<String>,
    pub message_count: i64,
    pub agent_count: i64,
}

/// Errors that can arise from forking a workspace. These are converted to
/// HTTP status codes at the handler boundary.
#[derive(Debug)]
pub enum ForkError {
    NotFound,
    NotOwned,
    AlreadyAnApp(String), // existing App slug
    Db(sqlx::Error),
}

impl std::fmt::Display for ForkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "workspace not found"),
            Self::NotOwned => write!(f, "you don't own this workspace"),
            Self::AlreadyAnApp(s) => write!(f, "workspace is already an App ('{}')", s),
            Self::Db(e) => write!(f, "database error: {}", e),
        }
    }
}

impl std::error::Error for ForkError {}

impl From<sqlx::Error> for ForkError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e)
    }
}

// ─── Configuration knobs ─────────────────────────────────────────────────────
//
// Conservative defaults. Each one is a one-line edit if usage tells us we're
// being too cautious or too eager.

/// Agents hired with zero execution count flagged as "incidental — recommend
/// removing from auto_hire". A handful of those is fine in real workspaces
/// (the user hired and then changed direction), but they bloat first-session
/// budgets, so we recommend cleanup.
const FLAG_UNUSED_AGENTS: bool = true;

/// File paths matching these patterns are flagged as incidental.
const INCIDENTAL_FILE_PATTERNS: &[&str] =
    &["scratch", "tmp", "temp", "test", "debug", ".log", ".bak"];

/// Budget = average observed spend per session × 1.3 headroom, clamped.
const MIN_BUDGET: i32 = 50;
const MAX_BUDGET: i32 = 500;
const DEFAULT_BUDGET: i32 = 100; // when there's no session data to extrapolate from

// ─── The main entry point ────────────────────────────────────────────────────

pub async fn introspect_workspace_to_draft(
    db: &PgPool,
    workspace_id: uuid::Uuid,
    owner_id: &str,
) -> Result<WorkspaceForkDraft, ForkError> {
    // Step 1: workspace exists, caller owns it, and it isn't already an App
    // (we don't want users accidentally creating duplicate Apps from the same
    // workspace).
    let row = sqlx::query(
        r#"SELECT t.id, t.name, t.slug, t.owner_id, t.mission,
                  t.coordination_strategist_id, t.origin,
                  t.workspace_budget, t.workspace_spent,
                  t.description
           FROM teams t
           WHERE t.id = $1"#,
    )
    .bind(workspace_id)
    .fetch_optional(db)
    .await?
    .ok_or(ForkError::NotFound)?;

    let owner: String = row.try_get("owner_id").map_err(ForkError::Db)?;
    if owner != owner_id {
        return Err(ForkError::NotOwned);
    }
    let workspace_name: String = row.try_get("name").unwrap_or_default();
    let workspace_slug: Option<String> = row.try_get("slug").ok();
    let mission: Option<String> = row.try_get("mission").ok();
    let composition_strategist_id: Option<uuid::Uuid> =
        row.try_get("coordination_strategist_id").ok();
    let origin: Option<String> = row.try_get("origin").ok();
    let workspace_budget: i32 = row.try_get("workspace_budget").unwrap_or(0);
    let workspace_spent: i32 = row.try_get("workspace_spent").unwrap_or(0);
    let workspace_description: Option<String> = row.try_get("description").ok();

    // If this workspace was already spawned from an App, refuse — the user
    // is likely confused about what they're trying to do.
    if let Some(o) = origin.as_deref() {
        if !o.is_empty() && o != "personal_workspace" && o != "bestiary_workspace" {
            // Heuristic: an origin that's something other than the generic
            // personal/bestiary buckets likely points at an App slug already.
            // Could still be e.g. "rabble_swarm" — check the apps table.
            let app_row: Option<(String,)> =
                sqlx::query_as::<_, (String,)>("SELECT slug FROM apps WHERE slug = $1 LIMIT 1")
                    .bind(o)
                    .fetch_optional(db)
                    .await?;
            if let Some((slug,)) = app_row {
                return Err(ForkError::AlreadyAnApp(slug));
            }
        }
    }

    let mut issues: Vec<Issue> = Vec::new();

    // Step 2: fleet — agents hired in the workspace + per-workspace message
    // counts so we can flag the "hired but never used" ones.
    //
    // Note: episodes does not carry a workspace_id (it's an agent-scoped
    // event log, not a workspace one). The closest per-workspace signal is
    // workspace_messages where sender_type='agent' and sender_id matches
    // the agent's UUID-as-text. That gives us "how many times did this
    // agent speak in this workspace" — a reasonable proxy for "was this
    // agent actually used here" for fork-from-workspace heuristics.
    let fleet_rows = sqlx::query(
        r#"SELECT a.agent_name,
                  COUNT(wm.message_id) FILTER (WHERE wm.sender_type = 'agent') AS execution_count
           FROM workspace_agents wa
           JOIN agents a ON a.agent_id = wa.agent_id
           LEFT JOIN workspace_messages wm
             ON wm.workspace_id = wa.workspace_id
            AND wm.sender_id = a.agent_id::text
           WHERE wa.workspace_id = $1
           GROUP BY a.agent_name
           ORDER BY execution_count DESC, a.agent_name"#,
    )
    .bind(workspace_id)
    .fetch_all(db)
    .await?;

    let mut fleet: Vec<String> = Vec::new();
    let mut unused: Vec<String> = Vec::new();
    for row in &fleet_rows {
        let name: String = row.try_get("agent_name").unwrap_or_default();
        let count: i64 = row.try_get("execution_count").unwrap_or(0);
        if name.is_empty() {
            continue;
        }
        if count == 0 && FLAG_UNUSED_AGENTS {
            unused.push(name.clone());
        }
        fleet.push(name);
    }

    if !unused.is_empty() {
        let summary = if unused.len() == 1 {
            format!(
                "'{}' was hired but never executed in this workspace",
                unused[0]
            )
        } else {
            format!(
                "{} hired agents were never executed: {}",
                unused.len(),
                unused.join(", ")
            )
        };
        issues.push(
            Issue::suggest("workspace_template.auto_hire", summary).with_fix(Fix {
                label: format!("Remove unused agent(s) from auto_hire ({})", unused.len()),
                patch: json!([{
                    "op": "replace",
                    "path": "/workspace_template/auto_hire",
                    "value": fleet.iter()
                        .filter(|a| !unused.contains(a))
                        .cloned()
                        .collect::<Vec<_>>(),
                }]),
            }),
        );
    }

    // Step 3: message count — used for description + budget extrapolation.
    let message_count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*)::bigint FROM workspace_messages WHERE workspace_id = $1",
    )
    .bind(workspace_id)
    .fetch_one(db)
    .await
    .unwrap_or(0);

    let agent_count = fleet.len() as i64;

    // Step 4: initial_files — capture a sensible-ish subset of workspace files.
    //
    // We aren't reading the git tree here (that requires the workspace's git
    // repo path resolution). Instead we synthesize the standard stub set and
    // flag the synthesis as "review" — the user can paste real workspace
    // content into the manifest from the workspace UI if they want their App
    // to ship with non-stub initial files. This avoids reading potentially
    // sensitive workspace content into a draft that gets registered with the
    // wrong visibility.

    let slug = derive_slug_from_workspace(&workspace_name, workspace_slug.as_deref());
    let initial_files = json!([
        {
            "path": format!("{}/state.yaml", slug),
            "content": "# Canonical document for this App workspace.\n# Edit this — agents will read and write to it.\n"
        },
        {
            "path": ".app/manifest.yaml",
            "content": format!("app_slug: {}\n", slug)
        },
        {
            "path": "context/readme.md",
            "content": format!(
                "# {} workspace\n\nSpawned from the {} App (forked from a working workspace).\n",
                slug, slug
            )
        }
    ]);

    // Flag the stub-files synthesis so the user knows what they're shipping.
    issues.push(Issue::info(
        "workspace_template.initial_files",
        "drafted standard stub files (state.yaml, .app/manifest.yaml, context/readme.md). \
         If your workspace ships meaningful seed content, paste it into initial_files before publishing.",
    ));

    // Step 5: budget — observed spend × 1.3, clamped. If there's no data,
    // use the default. Doesn't take into account agent costs at first-session
    // scale (which the user can ask Xaman Ek about); good enough as a draft.
    let inferred_budget = if workspace_spent > 0 {
        let scaled = ((workspace_spent as f64) * 1.3) as i32;
        scaled.clamp(MIN_BUDGET, MAX_BUDGET)
    } else if workspace_budget > 0 {
        // No spend, but the workspace had an initial budget — copy it.
        workspace_budget.clamp(MIN_BUDGET, MAX_BUDGET)
    } else {
        DEFAULT_BUDGET
    };

    if workspace_spent == 0 && workspace_budget == 0 {
        issues.push(Issue::suggest(
            "workspace_template.initial_budget",
            format!(
                "no observed spend in this workspace; defaulted to {} credits — \
                 review against your expected first-session cost",
                DEFAULT_BUDGET
            ),
        ));
    } else if workspace_spent > 0 {
        issues.push(Issue::info(
            "workspace_template.initial_budget",
            format!(
                "inferred {}cr from observed workspace spend ({}cr × 1.3 headroom)",
                inferred_budget, workspace_spent
            ),
        ));
    }

    // Step 6: tagline / description — draft from workspace metadata.
    let tagline = mission.clone();
    let description = match (mission.as_deref(), workspace_description.as_deref()) {
        (Some(m), Some(d)) if !d.is_empty() => Some(format!("{}\n\n{}", m, d)),
        (Some(m), _) => Some(m.to_string()),
        (_, Some(d)) if !d.is_empty() => Some(d.to_string()),
        _ => None,
    };

    if description.is_none() {
        issues.push(Issue::suggest(
            "description",
            "no workspace mission or description to draft from — write one before publishing so xamanEK can surface your App to other users",
        ));
    }

    // Step 7: assemble the PartialManifest.
    let workspace_template = json!({
        "initial_budget": inferred_budget,
        "default_name_pattern": format!("{} — {{date}} session", slug),
        "auto_hire": fleet.iter().filter(|a| !unused.contains(a)).cloned().collect::<Vec<_>>(),
        "initial_files": initial_files,
    });

    let manifest = PartialManifest {
        slug: Some(slug.clone()),
        name: Some(workspace_name.clone()),
        tagline,
        description,
        workspace_template: Some(workspace_template),
        visibility: Some("private".to_string()),
        // composition_slug + schema not auto-derived; UI prompts the user
        composition_slug: None,
        schema_slug: None,
        schema_json: None,
        homepage_url: None,
        icon_url: None,
        metadata: Some(json!({
            "forked_from_workspace_id": workspace_id.to_string(),
            "forked_at": chrono::Utc::now().to_rfc3339(),
        })),
    };

    // Step 8: extra suggestions about composition identity.
    if composition_strategist_id.is_none() {
        issues.push(Issue::suggest(
            "composition_slug",
            "this workspace has no coordination strategist — Apps work best when the fleet is a defined composition. \
             Consider running through composition_design with xamanEK before publishing.",
        ));
    } else if mission.is_none() {
        issues.push(Issue::suggest(
            "tagline",
            "workspace has a coordination strategist but no mission statement — write one so users know what your App is for",
        ));
    }

    // Step 9: low-traffic suggestion (workspaces with very few messages are
    // probably too immature to publish, but we don't *block* — the user might
    // be intentionally publishing a starter template).
    if message_count < 5 {
        issues.push(Issue::suggest(
            "@workspace",
            format!(
                "this workspace has only {} message(s) — consider iterating in it more before publishing as an App",
                message_count
            ),
        ));
    }

    let source = WorkspaceForkSource {
        workspace_id,
        workspace_name,
        workspace_slug,
        mission,
        composition_strategist_id,
        origin,
        message_count,
        agent_count,
    };

    Ok(WorkspaceForkDraft {
        manifest,
        issues,
        source,
    })
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Derive a candidate slug from a workspace name or slug.
/// Conservative: prefers the workspace slug *if and only if* it already matches
/// App-slug rules verbatim (no modifications applied). This protects against
/// surprising behaviour where a malformed workspace slug gets minimally edited
/// into something the user didn't pick. Otherwise we slugify the name.
fn derive_slug_from_workspace(name: &str, workspace_slug: Option<&str>) -> String {
    // Use the workspace slug only if it would pass builder validation as-is.
    if let Some(s) = workspace_slug {
        if crate::apps::builder::validate_slug(s).is_ok() {
            return s.to_string();
        }
    }
    // Otherwise slugify the name; final validation runs in apps::builder.
    let slugified = slugify(name);
    if slugified.is_empty() {
        return "draft_app".to_string();
    }
    slugified
}

/// Conservative slugifier: lowercase ASCII letters/digits, underscores for
/// separators, leading non-letter chars stripped. Mirrors the rules in
/// apps::builder::validate_slug.
fn slugify(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_underscore = true; // strip leading non-letter chars
    for c in input.chars() {
        if c.is_ascii_alphanumeric() {
            let lc = c.to_ascii_lowercase();
            // Slugs must start with a letter; if we haven't emitted one yet,
            // skip leading digits.
            if out.is_empty() && lc.is_ascii_digit() {
                continue;
            }
            out.push(lc);
            prev_underscore = false;
        } else if !prev_underscore && !out.is_empty() {
            out.push('_');
            prev_underscore = true;
        }
    }
    // Trim trailing underscore
    while out.ends_with('_') {
        out.pop();
    }
    // Truncate to 64 chars
    if out.len() > 64 {
        out.truncate(64);
    }
    // Reject too-short
    if out.len() < 3 {
        return String::new();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Efrain Notes"), "efrain_notes");
        assert_eq!(slugify("kask-simops"), "kask_simops");
        assert_eq!(slugify("123 abc"), "abc"); // leading digits stripped
        assert_eq!(slugify("My App!"), "my_app");
        assert_eq!(slugify("   spaces   "), "spaces");
        assert_eq!(slugify("ab"), ""); // too short
    }

    #[test]
    fn derive_slug_prefers_workspace_slug() {
        assert_eq!(
            derive_slug_from_workspace("Efrain Notes", Some("efrain_notes")),
            "efrain_notes"
        );
        assert_eq!(
            derive_slug_from_workspace("Efrain Notes", None),
            "efrain_notes"
        );
        // Workspace slug invalid → fall back to name slugify
        assert_eq!(
            derive_slug_from_workspace("Efrain Notes", Some("123badslug")),
            "efrain_notes"
        );
    }
}
