//! Manifest builder + validator for the App primitive.
//!
//! Three callers share this substrate:
//!   1. The CLI (`abw-cli`) — runs `build_manifest()` locally before POSTing
//!   2. The xamanEK `app_design` session-mode flow — runs it against the
//!      session's accumulated `__UPDATE__` blocks before calling
//!      `POST /api/apps` on the user's behalf
//!   3. The "Save workspace as App" fork flow — runs it against an
//!      introspected workspace state to produce a draft manifest
//!
//! The HTTP `POST /api/apps` handler also calls into the validators here so
//! the rules are not duplicated.
//!
//! Design notes:
//!   - Pure functions where possible. Defaults are computed from inputs only.
//!   - Issues are collected, not thrown. Callers decide whether warnings block.
//!   - The output is always a `BuildResult` — either a finalized manifest plus
//!     any non-blocking issues, or just a list of blocking issues with no
//!     manifest. This lets the CLI render structured errors and the xamanEK
//!     session drive the next conversational turn from the same shape.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ─── Reserved origin tags ────────────────────────────────────────────────────
//
// These cannot be used as App slugs because existing workspaces already use
// them as origin values. Kept in code, not the DB, so we can extend without
// a migration. Mirrors `src/handlers/apps.rs::RESERVED_SLUGS`.

pub const RESERVED_SLUGS: &[&str] = &[
    "bestiary_workspace",
    "rabble_swarm",
    "personal_workspace",
    "fermi_forecast",
    "silat_workspace",
];

pub fn is_reserved(slug: &str) -> bool {
    RESERVED_SLUGS.contains(&slug)
}

// ─── Issue shape ─────────────────────────────────────────────────────────────

/// Severity of a build-time issue.
///
/// `Error` blocks manifest finalization. `Warning` and `Info` do not.
/// `Suggestion` is a positive recommendation (e.g. "consider adding a tagline"
/// or "this agent looks incidental — remove from auto_hire?").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Suggestion,
}

/// A structured build-time issue. Callers render these consistently — the CLI
/// shows them as lines under each input field; the xamanEK session uses them
/// to choose the next conversational turn; the workspace-fork UI shows them
/// in a side panel with auto-fixer buttons where applicable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Issue {
    pub severity: Severity,
    /// Dotted path identifying the field this issue refers to (e.g. "slug",
    /// "workspace_template.initial_budget", "workspace_template.auto_hire[2]").
    pub field: String,
    /// Human-readable message. Should be specific and actionable.
    pub message: String,
    /// Optional auto-fix suggestion the caller can apply with one action.
    /// `None` means "human judgement required."
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

/// A machine-applicable fix the caller can offer as a one-click action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fix {
    /// Human label for the fix ("Set initial_budget to 100", "Remove scratch.txt").
    pub label: String,
    /// JSON Patch (RFC 6902) operations to apply to the partial manifest.
    /// Kept as a generic `Value` so callers don't need to depend on a patch crate.
    pub patch: Value,
}

impl Issue {
    pub fn error<F: Into<String>, M: Into<String>>(field: F, message: M) -> Self {
        Self { severity: Severity::Error, field: field.into(), message: message.into(), fix: None }
    }
    pub fn warn<F: Into<String>, M: Into<String>>(field: F, message: M) -> Self {
        Self { severity: Severity::Warning, field: field.into(), message: message.into(), fix: None }
    }
    pub fn info<F: Into<String>, M: Into<String>>(field: F, message: M) -> Self {
        Self { severity: Severity::Info, field: field.into(), message: message.into(), fix: None }
    }
    pub fn suggest<F: Into<String>, M: Into<String>>(field: F, message: M) -> Self {
        Self { severity: Severity::Suggestion, field: field.into(), message: message.into(), fix: None }
    }
    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }
}

// ─── Partial manifest input ──────────────────────────────────────────────────

/// A partial manifest as it might arrive from any of the three callers.
///
/// Mirrors `CreateAppRequest` in `handlers::apps` but every field is optional —
/// the builder is responsible for filling defaults and surfacing missing
/// required fields as `Error` issues.
///
/// Extra fields the callers want to round-trip (icon_url, schema_slug,
/// composition_slug, etc.) are accepted via `extra`. The builder copies them
/// through verbatim after validating known shapes.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PartialManifest {
    pub slug: Option<String>,
    pub name: Option<String>,
    pub tagline: Option<String>,
    pub description: Option<String>,
    pub homepage_url: Option<String>,
    pub icon_url: Option<String>,
    pub composition_slug: Option<String>,
    pub schema_slug: Option<String>,
    pub schema_json: Option<Value>,
    pub workspace_template: Option<Value>,
    pub visibility: Option<String>,
    pub metadata: Option<Value>,
}

impl PartialManifest {
    /// Build a `PartialManifest` from a serde_json::Value (the form used by
    /// the auto-seed path and the CLI's local `manifest.json`).
    pub fn from_value(v: &Value) -> Self {
        Self {
            slug: v.get("slug").and_then(|x| x.as_str()).map(String::from),
            name: v.get("name").and_then(|x| x.as_str()).map(String::from),
            tagline: v.get("tagline").and_then(|x| x.as_str()).map(String::from),
            description: v.get("description").and_then(|x| x.as_str()).map(String::from),
            homepage_url: v.get("homepage_url").and_then(|x| x.as_str()).map(String::from),
            icon_url: v.get("icon_url").and_then(|x| x.as_str()).map(String::from),
            composition_slug: v.get("composition_slug").and_then(|x| x.as_str()).map(String::from),
            schema_slug: v.get("schema_slug").and_then(|x| x.as_str()).map(String::from),
            schema_json: v.get("schema_json").cloned(),
            workspace_template: v.get("workspace_template").cloned(),
            visibility: v.get("visibility").and_then(|x| x.as_str()).map(String::from),
            metadata: v.get("metadata").cloned(),
        }
    }

    /// Serialize back to JSON. Drops `None` fields.
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(json!({}))
    }
}

// ─── Builder output ──────────────────────────────────────────────────────────

/// Result of `build_manifest()`. Either a finalized manifest (possibly with
/// non-blocking issues) or a list of blocking issues only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildResult {
    /// `Some(...)` if there are no `Error`-severity issues. `None` otherwise.
    pub manifest: Option<Value>,
    /// All issues collected during build. The CLI renders these directly;
    /// the xamanEK session uses them to choose what to ask next.
    pub issues: Vec<Issue>,
}

impl BuildResult {
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    /// Convenience: return only errors. Useful for the CLI's exit-code path.
    pub fn errors(&self) -> Vec<&Issue> {
        self.issues.iter().filter(|i| i.severity == Severity::Error).collect()
    }

    /// Convenience: return only warnings + suggestions. Useful for the
    /// `abw app validate` "show me what to improve" path.
    pub fn non_blocking(&self) -> Vec<&Issue> {
        self.issues.iter().filter(|i| i.severity != Severity::Error).collect()
    }
}

// ─── Pure validators ─────────────────────────────────────────────────────────

/// Validate a slug against the platform rules.
/// Mirrors the validation in `handlers::apps::create_app_handler`.
/// Returns `Ok(())` if valid, `Err(message)` otherwise.
pub fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() {
        return Err("slug is required".into());
    }
    if slug.len() < 3 || slug.len() > 64 {
        return Err(format!("slug must be 3-64 chars, got {}", slug.len()));
    }
    if !slug.chars().next().map(|c| c.is_ascii_lowercase()).unwrap_or(false) {
        return Err("slug must start with a lowercase letter".into());
    }
    if !slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
        return Err("slug must contain only lowercase letters, digits, and underscores".into());
    }
    if is_reserved(slug) {
        return Err(format!("'{}' is a reserved platform origin tag and cannot be used as an App slug", slug));
    }
    Ok(())
}

const VALID_VISIBILITIES: &[&str] = &["private", "unlisted", "public"];

pub fn validate_visibility(v: &str) -> Result<(), String> {
    if VALID_VISIBILITIES.contains(&v) {
        Ok(())
    } else {
        Err(format!("visibility must be one of {:?}, got '{}'", VALID_VISIBILITIES, v))
    }
}

// ─── Defaults ────────────────────────────────────────────────────────────────

/// Generate a default workspace_template if the caller didn't supply one.
/// The slug is woven into the manifest.yaml stub so the App is self-identifying
/// at runtime.
pub fn default_workspace_template(slug: &str, schema_slug: Option<&str>) -> Value {
    let manifest_yaml = match schema_slug {
        Some(s) => format!("app_slug: {}\nschema: {}\n", slug, s),
        None => format!("app_slug: {}\n", slug),
    };
    json!({
        "initial_budget": 100,
        "default_name_pattern": format!("{} — {{date}} session", slug),
        "auto_hire": [],
        "initial_files": [
            {
                "path": format!("{}/state.yaml", slug),
                "content": "# Canonical document for this App workspace.\n# Edit this — agents will read and write to it.\n"
            },
            {
                "path": ".app/manifest.yaml",
                "content": manifest_yaml
            },
            {
                "path": "context/readme.md",
                "content": format!("# {} workspace\n\nSpawned from the {} App. See `.app/manifest.yaml` for the App identity and `{}/state.yaml` for the canonical document.\n", slug, slug, slug)
            }
        ]
    })
}

/// Generate a default display name from a slug.
/// Turns `efrain_genealogy` into `Efrain Genealogy`.
pub fn default_name_from_slug(slug: &str) -> String {
    slug.split('_')
        .filter(|s| !s.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ─── Workspace_template structural validators ────────────────────────────────

fn validate_workspace_template(template: &Value, slug: &str, issues: &mut Vec<Issue>) {
    if !template.is_object() {
        issues.push(Issue::error("workspace_template", "must be a JSON object"));
        return;
    }

    // initial_budget
    match template.get("initial_budget") {
        Some(v) if v.is_i64() || v.is_u64() => {
            let n = v.as_i64().unwrap_or(0);
            if n < 0 {
                issues.push(Issue::error("workspace_template.initial_budget", "must be non-negative"));
            } else if n == 0 {
                issues.push(Issue::warn(
                    "workspace_template.initial_budget",
                    "initial_budget is 0 — users will be unable to invoke agents without topping up first",
                ));
            } else if n < 20 {
                issues.push(Issue::warn(
                    "workspace_template.initial_budget",
                    format!("initial_budget is {} — typically too low for a useful first session", n),
                ));
            } else if n > 1000 {
                issues.push(Issue::warn(
                    "workspace_template.initial_budget",
                    format!("initial_budget is {} — unusually large; verify this is intentional", n),
                ));
            }
        }
        Some(_) => issues.push(Issue::error("workspace_template.initial_budget", "must be an integer")),
        None => issues.push(Issue::suggest(
            "workspace_template.initial_budget",
            "consider setting an initial_budget (typically 50-300 credits) so users can complete a first session without topping up",
        )),
    }

    // auto_hire
    if let Some(auto_hire) = template.get("auto_hire") {
        if !auto_hire.is_array() {
            issues.push(Issue::error("workspace_template.auto_hire", "must be an array of agent_id strings"));
        } else {
            let arr = auto_hire.as_array().unwrap();
            if arr.is_empty() {
                issues.push(Issue::suggest(
                    "workspace_template.auto_hire",
                    "no agents are auto-hired — users will see an empty workspace; consider adding at least one primary agent",
                ));
            }
            for (i, item) in arr.iter().enumerate() {
                if !item.is_string() {
                    issues.push(Issue::error(
                        format!("workspace_template.auto_hire[{}]", i),
                        "each entry must be an agent_id string",
                    ));
                }
            }
        }
    }

    // initial_files
    if let Some(files) = template.get("initial_files") {
        if !files.is_array() {
            issues.push(Issue::error("workspace_template.initial_files", "must be an array of file objects"));
        } else {
            let arr = files.as_array().unwrap();
            let mut has_manifest = false;
            for (i, file) in arr.iter().enumerate() {
                if !file.is_object() {
                    issues.push(Issue::error(
                        format!("workspace_template.initial_files[{}]", i),
                        "must be an object with 'path' and 'content' keys",
                    ));
                    continue;
                }
                let path = file.get("path").and_then(|p| p.as_str());
                match path {
                    None => issues.push(Issue::error(
                        format!("workspace_template.initial_files[{}].path", i),
                        "missing or non-string 'path'",
                    )),
                    Some(".app/manifest.yaml") => has_manifest = true,
                    Some(_) => {}
                }
                if !file.get("content").is_some_and(|c| c.is_string()) {
                    issues.push(Issue::error(
                        format!("workspace_template.initial_files[{}].content", i),
                        "missing or non-string 'content'",
                    ));
                }
            }
            if !has_manifest {
                issues.push(
                    Issue::warn(
                        "workspace_template.initial_files",
                        "no `.app/manifest.yaml` file in initial_files — workspaces will not be self-identifying as App instances",
                    )
                    .with_fix(Fix {
                        label: "Add a default .app/manifest.yaml stub".into(),
                        patch: json!([{
                            "op": "add",
                            "path": "/workspace_template/initial_files/-",
                            "value": {
                                "path": ".app/manifest.yaml",
                                "content": format!("app_slug: {}\n", slug)
                            }
                        }]),
                    }),
                );
            }
        }
    } else {
        issues.push(Issue::suggest(
            "workspace_template.initial_files",
            "no initial_files declared — consider adding a state.yaml stub and a .app/manifest.yaml so workspaces start with the right structure",
        ));
    }
}

// ─── The main entry point ────────────────────────────────────────────────────

/// Build a finalized App manifest from a partial input.
///
/// Workflow:
///   1. Validate required fields (slug). Missing slug is fatal — without it,
///      we can't generate defaults that reference the slug.
///   2. Apply defaults for absent fields (name from slug, workspace_template).
///   3. Validate the now-complete manifest. Collect issues.
///   4. If no errors, emit a finalized manifest. If errors, emit `None`.
///
/// This function is pure — it does not touch the database. Callers wishing
/// to check uniqueness or agent-existence should do so as a separate step
/// after `build_manifest()` returns Ok.
pub fn build_manifest(input: PartialManifest) -> BuildResult {
    let mut issues = Vec::new();

    // Step 1: slug is non-negotiable.
    let slug = match input.slug.as_deref() {
        Some(s) => match validate_slug(s) {
            Ok(()) => s.to_string(),
            Err(msg) => {
                issues.push(Issue::error("slug", msg));
                return BuildResult { manifest: None, issues };
            }
        },
        None => {
            issues.push(Issue::error("slug", "slug is required"));
            return BuildResult { manifest: None, issues };
        }
    };

    // Step 2: defaults.
    let name = input.name.clone().unwrap_or_else(|| {
        issues.push(Issue::info(
            "name",
            format!("name defaulted to '{}' (derived from slug)", default_name_from_slug(&slug)),
        ));
        default_name_from_slug(&slug)
    });

    let workspace_template = input
        .workspace_template
        .clone()
        .unwrap_or_else(|| {
            issues.push(Issue::info(
                "workspace_template",
                "workspace_template defaulted (initial_budget=100, manifest.yaml stub, state.yaml stub)",
            ));
            default_workspace_template(&slug, input.schema_slug.as_deref())
        });

    let visibility = input.visibility.clone().unwrap_or_else(|| {
        issues.push(Issue::info("visibility", "visibility defaulted to 'private'"));
        "private".into()
    });
    if let Err(msg) = validate_visibility(&visibility) {
        issues.push(Issue::error("visibility", msg));
    }

    // Step 3: structural validation of the assembled template.
    validate_workspace_template(&workspace_template, &slug, &mut issues);

    // Step 4: soft checks on optional but recommended fields.
    if input.tagline.is_none() {
        issues.push(Issue::suggest(
            "tagline",
            "consider adding a one-line tagline — xamanEK uses it to surface your App in conversation",
        ));
    }
    if input.description.is_none() {
        issues.push(Issue::suggest(
            "description",
            "consider adding a description — xamanEK uses it to explain your App to other users",
        ));
    }
    if input.composition_slug.is_none() && input.schema_json.is_none() && input.schema_slug.is_none() {
        issues.push(Issue::suggest(
            "schema_json",
            "consider declaring either a schema_json (inline) or a schema_slug (reference) so the canonical document is introspectable",
        ));
    }

    // Step 5: bail if blocking errors.
    let has_errors = issues.iter().any(|i| i.severity == Severity::Error);
    if has_errors {
        return BuildResult { manifest: None, issues };
    }

    // Step 6: assemble the finalized manifest.
    let mut manifest = json!({
        "slug": slug,
        "name": name,
        "workspace_template": workspace_template,
        "visibility": visibility,
    });
    let obj = manifest.as_object_mut().unwrap();
    if let Some(v) = input.tagline { obj.insert("tagline".into(), json!(v)); }
    if let Some(v) = input.description { obj.insert("description".into(), json!(v)); }
    if let Some(v) = input.homepage_url { obj.insert("homepage_url".into(), json!(v)); }
    if let Some(v) = input.icon_url { obj.insert("icon_url".into(), json!(v)); }
    if let Some(v) = input.composition_slug { obj.insert("composition_slug".into(), json!(v)); }
    if let Some(v) = input.schema_slug { obj.insert("schema_slug".into(), json!(v)); }
    if let Some(v) = input.schema_json { obj.insert("schema_json".into(), v); }
    if let Some(v) = input.metadata { obj.insert("metadata".into(), v); }

    BuildResult { manifest: Some(manifest), issues }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_rules() {
        assert!(validate_slug("efrain").is_ok());
        assert!(validate_slug("kask_simops").is_ok());
        assert!(validate_slug("a12_b34").is_ok());

        assert!(validate_slug("").is_err());                  // empty
        assert!(validate_slug("ab").is_err());                // too short
        assert!(validate_slug(&"a".repeat(65)).is_err());     // too long
        assert!(validate_slug("Efrain").is_err());            // uppercase
        assert!(validate_slug("1efrain").is_err());           // leading digit
        assert!(validate_slug("efrain-2").is_err());          // hyphen
        assert!(validate_slug("efrain ai").is_err());         // space
        assert!(validate_slug("rabble_swarm").is_err());      // reserved
    }

    #[test]
    fn visibility_rules() {
        assert!(validate_visibility("private").is_ok());
        assert!(validate_visibility("unlisted").is_ok());
        assert!(validate_visibility("public").is_ok());
        assert!(validate_visibility("secret").is_err());
        assert!(validate_visibility("").is_err());
    }

    #[test]
    fn name_default() {
        assert_eq!(default_name_from_slug("efrain"), "Efrain");
        assert_eq!(default_name_from_slug("kask_simops"), "Kask Simops");
        assert_eq!(default_name_from_slug("a_b_c"), "A B C");
    }

    #[test]
    fn minimal_input_yields_manifest() {
        let input = PartialManifest {
            slug: Some("efrain".into()),
            ..Default::default()
        };
        let result = build_manifest(input);
        assert!(!result.has_errors(), "minimal input should succeed: {:#?}", result.errors());
        let manifest = result.manifest.expect("expected manifest");
        assert_eq!(manifest["slug"], "efrain");
        assert_eq!(manifest["name"], "Efrain");
        assert_eq!(manifest["visibility"], "private");
        assert!(manifest["workspace_template"]["initial_budget"].is_i64());
    }

    #[test]
    fn missing_slug_blocks() {
        let result = build_manifest(PartialManifest::default());
        assert!(result.has_errors());
        assert!(result.manifest.is_none());
        assert_eq!(result.errors()[0].field, "slug");
    }

    #[test]
    fn invalid_slug_blocks() {
        let input = PartialManifest {
            slug: Some("Efrain".into()),
            ..Default::default()
        };
        let result = build_manifest(input);
        assert!(result.has_errors());
        assert!(result.manifest.is_none());
    }

    #[test]
    fn reserved_slug_blocks() {
        let input = PartialManifest {
            slug: Some("rabble_swarm".into()),
            ..Default::default()
        };
        let result = build_manifest(input);
        assert!(result.has_errors());
    }

    #[test]
    fn full_input_passes_clean() {
        let input = PartialManifest {
            slug: Some("efrain".into()),
            name: Some("Efrain".into()),
            tagline: Some("AI-augmented research notes".into()),
            description: Some("A research-notes App built by Mario on the ABW substrate.".into()),
            homepage_url: Some("https://efrain.ai".into()),
            composition_slug: Some("efrain_fleet".into()),
            schema_json: Some(json!({"type": "object"})),
            workspace_template: Some(json!({
                "initial_budget": 150,
                "auto_hire": ["companion_builder_coach"],
                "initial_files": [
                    { "path": "efrain/state.yaml", "content": "notes: []\n" },
                    { "path": ".app/manifest.yaml", "content": "app_slug: efrain\n" }
                ]
            })),
            visibility: Some("unlisted".into()),
            ..Default::default()
        };
        let result = build_manifest(input);
        assert!(!result.has_errors(), "full input should pass: {:#?}", result.errors());
        let m = result.manifest.expect("manifest");
        assert_eq!(m["slug"], "efrain");
        assert_eq!(m["tagline"], "AI-augmented research notes");
        assert_eq!(m["workspace_template"]["initial_budget"], 150);
    }

    #[test]
    fn low_budget_warns_but_does_not_block() {
        let input = PartialManifest {
            slug: Some("efrain".into()),
            workspace_template: Some(json!({
                "initial_budget": 5,
                "auto_hire": [],
                "initial_files": [{"path": ".app/manifest.yaml", "content": "app_slug: efrain\n"}]
            })),
            ..Default::default()
        };
        let result = build_manifest(input);
        assert!(!result.has_errors());
        assert!(result.non_blocking().iter().any(|i| {
            i.field == "workspace_template.initial_budget" && i.severity == Severity::Warning
        }));
    }

    #[test]
    fn missing_manifest_yaml_warns_with_fix() {
        let input = PartialManifest {
            slug: Some("efrain".into()),
            workspace_template: Some(json!({
                "initial_budget": 100,
                "auto_hire": ["companion_builder_coach"],
                "initial_files": [{"path": "efrain/state.yaml", "content": "notes: []\n"}]
            })),
            ..Default::default()
        };
        let result = build_manifest(input);
        let warning = result.non_blocking().into_iter().find(|i| {
            i.field == "workspace_template.initial_files" && i.severity == Severity::Warning
        });
        assert!(warning.is_some(), "expected a warning about missing manifest.yaml");
        assert!(warning.unwrap().fix.is_some(), "expected the warning to carry an auto-fix");
    }

    #[test]
    fn kask_simops_manifest_round_trips() {
        // The existing apps/kask_simops.json must build cleanly.
        let kask = json!({
            "slug": "kask_simops",
            "name": "SimOps",
            "tagline": "Design, simulate, and compare process pipelines.",
            "description": "Agent-led process modelling.",
            "homepage_url": "https://kask.bio/projects/simops",
            "composition_slug": "simops_fleet",
            "schema_slug": "kask-simops/2",
            "visibility": "public",
            "workspace_template": {
                "initial_budget": 295,
                "auto_hire": ["simops_advisor", "simops_cascade", "sidestream_miner"],
                "initial_files": [
                    {"path": "simops/process.yaml", "content": "name: New Process\n"},
                    {"path": ".app/manifest.yaml", "content": "app_slug: kask_simops\n"}
                ]
            }
        });
        let input = PartialManifest::from_value(&kask);
        let result = build_manifest(input);
        assert!(!result.has_errors(), "kask_simops should validate clean: {:#?}", result.errors());
    }
}
