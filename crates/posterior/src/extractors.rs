//! Extractors — turn upstream resolutions into scalar observations.
//!
//! An [`Extractor`] takes a JSON resolution payload (the `outcome` field of a
//! workspace resolution, per `WORKSPACE_RESOLUTION.md`), a workspace context
//! (the entity this workspace represents, e.g. team ARG), and a config blob
//! (extractor-specific), and produces either `Some(f64)` — a scalar observation
//! for `fit_marginal` — or `None` (this resolution doesn't apply to this driver).
//!
//! Extractors are domain-neutral primitives: they don't know about football,
//! cultivation, or any specific domain. They only know how to look up fields
//! in JSON and apply a few common shape-mappings (binary winner match, scalar
//! field, difference of two fields).
//!
//! ## Registry pattern
//!
//! A live [`ExtractorRegistry`] is held by the server (`AppState`); extractors
//! are registered at server boot. The trait is open — new extractors can be
//! added by code change. Discoverability for agents is provided by a future
//! MCP tool that walks the registry.
//!
//! See `docs/specs/23_BAYESOPS_WORLD_CUP_DEMO.md` §3.4 for the design.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

/// Errors an extractor can return. All non-fatal — the caller logs and skips
/// the observation rather than aborting the fit.
#[derive(Debug, Error)]
pub enum ExtractorError {
    #[error("missing required field '{field}' in resolution outcome")]
    MissingField { field: String },

    #[error("field '{field}' has unexpected type (got {got}, want {want})")]
    BadType {
        field: String,
        got: String,
        want: String,
    },

    #[error("missing required config key '{key}'")]
    MissingConfig { key: String },

    #[error("extractor '{name}' not registered")]
    Unknown { name: String },

    #[error("workspace_context has no entity_id (required for this extractor)")]
    NoEntity,

    #[error("internal extractor error: {0}")]
    Internal(String),
}

/// What a workspace knows about itself, passed into every extractor so it can
/// resolve `${workspace.entity_id}`-style config substitutions.
///
/// The fields here are intentionally minimal — extractors should not poke at
/// arbitrary workspace state; the workspace owner is responsible for surfacing
/// only what's relevant for evidence mapping.
#[derive(Debug, Clone, Default)]
pub struct WorkspaceContext {
    /// The entity this workspace represents — e.g. team identifier, batch SKU,
    /// asset ticker. None if the workspace doesn't represent a single entity.
    pub entity_id: Option<String>,

    /// Free-form additional metadata. Use sparingly; prefer surfacing data
    /// through `entity_id` or extractor config first.
    pub metadata: HashMap<String, JsonValue>,
}

impl WorkspaceContext {
    pub fn with_entity(entity_id: impl Into<String>) -> Self {
        Self {
            entity_id: Some(entity_id.into()),
            metadata: HashMap::new(),
        }
    }
}

/// The extractor primitive. Implementors are stateless and `Send + Sync` so
/// the registry can hand them out as `Arc<dyn Extractor>` to concurrent
/// refit hooks.
pub trait Extractor: Send + Sync {
    /// Stable identifier (e.g. "binary_winner_id_match"). Used as the registry
    /// key and as the value of `feeds_from.extractor` in FPL annotations.
    fn name(&self) -> &str;

    /// Optional human-readable description, surfaced via the discovery MCP tool.
    fn description(&self) -> &str {
        ""
    }

    /// Optional JSON Schema describing the config shape, for editor support.
    /// Default: no schema (extractor accepts an unspecified config blob).
    fn config_schema(&self) -> Option<JsonValue> {
        None
    }

    /// Extract a scalar observation from a single resolution outcome.
    ///
    /// - `Ok(Some(f64))` — observation extracted; fold into `observations` vec.
    /// - `Ok(None)` — this resolution doesn't apply to this driver (silently skip).
    /// - `Err(_)` — config or data malformed; caller logs and skips.
    fn extract(
        &self,
        outcome: &JsonValue,
        context: &WorkspaceContext,
        config: &JsonValue,
    ) -> Result<Option<f64>, ExtractorError>;
}

/// Registry of named extractors. Cheap to clone (Arcs internally), built once
/// at server boot, immutable from then on.
#[derive(Clone, Default)]
pub struct ExtractorRegistry {
    extractors: HashMap<String, Arc<dyn Extractor>>,
}

impl ExtractorRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an extractor. Panics if a name collision occurs — at boot time
    /// this is a programming error worth surfacing loudly.
    pub fn register(&mut self, extractor: Arc<dyn Extractor>) {
        let name = extractor.name().to_string();
        if self.extractors.contains_key(&name) {
            panic!("ExtractorRegistry: duplicate extractor name '{}'", name);
        }
        self.extractors.insert(name, extractor);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Extractor>> {
        self.extractors.get(name).cloned()
    }

    /// List every registered extractor, for discoverability.
    pub fn list(&self) -> Vec<ExtractorDescription> {
        let mut out: Vec<_> = self
            .extractors
            .values()
            .map(|e| ExtractorDescription {
                name: e.name().to_string(),
                description: e.description().to_string(),
                config_schema: e.config_schema(),
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Build the default registry with all built-in extractors. Servers
    /// typically use this directly.
    pub fn with_builtins() -> Self {
        let mut r = Self::new();
        r.register(Arc::new(BinaryWinnerIdMatch));
        r.register(Arc::new(BinaryFieldValue));
        r.register(Arc::new(ScalarFieldValue));
        r.register(Arc::new(ScalarDifference));
        r
    }
}

/// Discovery shape for MCP / HTTP introspection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractorDescription {
    pub name: String,
    pub description: String,
    pub config_schema: Option<JsonValue>,
}

// ═════════════════════════════════════════════════════════════════════════════
// BUILT-IN EXTRACTORS
// ═════════════════════════════════════════════════════════════════════════════

/// Walk a JSON pointer of the form `"a.b.c"` (dot-separated). Treats array
/// indices as `"a.0.b"`. Returns `None` if any segment is missing.
fn lookup_path<'a>(root: &'a JsonValue, path: &str) -> Option<&'a JsonValue> {
    if path.is_empty() {
        return Some(root);
    }
    let mut cur = root;
    for seg in path.split('.') {
        cur = match cur {
            JsonValue::Object(obj) => obj.get(seg)?,
            JsonValue::Array(arr) => {
                let idx: usize = seg.parse().ok()?;
                arr.get(idx)?
            }
            _ => return None,
        };
    }
    Some(cur)
}

/// Resolve `${workspace.entity_id}`-style substitutions in a config value.
/// Currently supports `${workspace.entity_id}` only; extend the list when more
/// substitutions become useful.
fn resolve_substitutions(value: &str, ctx: &WorkspaceContext) -> Result<String, ExtractorError> {
    if !value.contains("${") {
        return Ok(value.to_string());
    }
    let mut out = value.to_string();
    if out.contains("${workspace.entity_id}") {
        let entity = ctx
            .entity_id
            .as_deref()
            .ok_or(ExtractorError::NoEntity)?;
        out = out.replace("${workspace.entity_id}", entity);
    }
    Ok(out)
}

fn config_get<'a>(config: &'a JsonValue, key: &str) -> Result<&'a JsonValue, ExtractorError> {
    config.get(key).ok_or_else(|| ExtractorError::MissingConfig {
        key: key.to_string(),
    })
}

fn config_get_str(config: &JsonValue, key: &str) -> Result<String, ExtractorError> {
    config_get(config, key)?
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| ExtractorError::BadType {
            field: key.to_string(),
            got: "non-string".to_string(),
            want: "string".to_string(),
        })
}

// ── binary_winner_id_match ───────────────────────────────────────────────────

/// `1.0` if the outcome's `winner_field` matches this workspace's entity,
/// else `0.0`. Used by WC team-prior workspaces' `won` drivers.
///
/// Config:
/// ```json
/// {
///   "winner_field": "winner_team_id",
///   "match_value":  "${workspace.entity_id}"
/// }
/// ```
pub struct BinaryWinnerIdMatch;

impl Extractor for BinaryWinnerIdMatch {
    fn name(&self) -> &str {
        "binary_winner_id_match"
    }

    fn description(&self) -> &str {
        "Emit 1.0 if the resolution's winner field matches this workspace's entity, else 0.0. Standard extractor for binary win-style drivers fed by H2H or match-type upstreams."
    }

    fn config_schema(&self) -> Option<JsonValue> {
        Some(serde_json::json!({
            "type": "object",
            "required": ["winner_field", "match_value"],
            "properties": {
                "winner_field": { "type": "string", "description": "Path inside outcome to the winner identifier (dot-separated)." },
                "match_value": { "type": "string", "description": "Value the winner_field must equal. Supports ${workspace.entity_id}." }
            }
        }))
    }

    fn extract(
        &self,
        outcome: &JsonValue,
        context: &WorkspaceContext,
        config: &JsonValue,
    ) -> Result<Option<f64>, ExtractorError> {
        let field = config_get_str(config, "winner_field")?;
        let raw_target = config_get_str(config, "match_value")?;
        let target = resolve_substitutions(&raw_target, context)?;

        let v = lookup_path(outcome, &field).ok_or(ExtractorError::MissingField {
            field: field.clone(),
        })?;
        let s = v.as_str().ok_or(ExtractorError::BadType {
            field: field.clone(),
            got: "non-string".to_string(),
            want: "string".to_string(),
        })?;
        Ok(Some(if s == target { 1.0 } else { 0.0 }))
    }
}

// ── binary_field_value ───────────────────────────────────────────────────────

/// `1.0` if `outcome.path == value`, else `0.0`. Generic binary indicator.
///
/// Config:
/// ```json
/// { "path": "advanced", "value": true }
/// ```
///
/// `value` may be a string, boolean, or number. Comparison is exact via
/// `JsonValue::==`.
pub struct BinaryFieldValue;

impl Extractor for BinaryFieldValue {
    fn name(&self) -> &str {
        "binary_field_value"
    }

    fn description(&self) -> &str {
        "Emit 1.0 if outcome.path matches the configured value exactly, else 0.0. Use for binary flags that aren't winner-identifier matches (e.g. advanced=true, accepted=false)."
    }

    fn config_schema(&self) -> Option<JsonValue> {
        Some(serde_json::json!({
            "type": "object",
            "required": ["path", "value"],
            "properties": {
                "path": { "type": "string", "description": "Dot-separated path into the outcome." },
                "value": { "description": "Expected value (string/boolean/number); exact match." }
            }
        }))
    }

    fn extract(
        &self,
        outcome: &JsonValue,
        _context: &WorkspaceContext,
        config: &JsonValue,
    ) -> Result<Option<f64>, ExtractorError> {
        let path = config_get_str(config, "path")?;
        let expected = config_get(config, "value")?;
        let actual = lookup_path(outcome, &path).ok_or(ExtractorError::MissingField {
            field: path.clone(),
        })?;
        Ok(Some(if actual == expected { 1.0 } else { 0.0 }))
    }
}

// ── scalar_field_value ───────────────────────────────────────────────────────

/// Return `outcome.path` as f64. Returns `Err(BadType)` if the field exists
/// but isn't a number.
///
/// Config:
/// ```json
/// { "path": "outcome.goals_scored.${workspace.entity_id}" }
/// ```
///
/// Path substitutions are resolved before lookup.
pub struct ScalarFieldValue;

impl Extractor for ScalarFieldValue {
    fn name(&self) -> &str {
        "scalar_field_value"
    }

    fn description(&self) -> &str {
        "Extract a numeric field from the outcome as a scalar observation. Path supports ${workspace.entity_id} substitution for per-entity drill-in."
    }

    fn config_schema(&self) -> Option<JsonValue> {
        Some(serde_json::json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string", "description": "Dot-separated path; supports ${workspace.entity_id}." }
            }
        }))
    }

    fn extract(
        &self,
        outcome: &JsonValue,
        context: &WorkspaceContext,
        config: &JsonValue,
    ) -> Result<Option<f64>, ExtractorError> {
        let raw_path = config_get_str(config, "path")?;
        let path = resolve_substitutions(&raw_path, context)?;
        let v = lookup_path(outcome, &path).ok_or(ExtractorError::MissingField {
            field: path.clone(),
        })?;
        let f = v.as_f64().ok_or(ExtractorError::BadType {
            field: path.clone(),
            got: format!("{:?}", v),
            want: "number".to_string(),
        })?;
        Ok(Some(f))
    }
}

// ── scalar_difference ────────────────────────────────────────────────────────

/// Return `outcome[field_a] - outcome[field_b]` as a scalar. Either path
/// can use `${workspace.entity_id}` substitution, letting the same extractor
/// produce "goals_for - goals_against from this team's perspective" by
/// flipping perspective per workspace.
///
/// Config:
/// ```json
/// {
///   "field_a": "outcome.goals.${workspace.entity_id}",
///   "field_b": "outcome.goals.opponent"
/// }
/// ```
pub struct ScalarDifference;

impl Extractor for ScalarDifference {
    fn name(&self) -> &str {
        "scalar_difference"
    }

    fn description(&self) -> &str {
        "Extract two numeric fields and return their difference (a - b) as a scalar. Both paths support ${workspace.entity_id} substitution for entity-perspective metrics like goal differential."
    }

    fn config_schema(&self) -> Option<JsonValue> {
        Some(serde_json::json!({
            "type": "object",
            "required": ["field_a", "field_b"],
            "properties": {
                "field_a": { "type": "string", "description": "Minuend path; supports ${workspace.entity_id}." },
                "field_b": { "type": "string", "description": "Subtrahend path; supports ${workspace.entity_id}." }
            }
        }))
    }

    fn extract(
        &self,
        outcome: &JsonValue,
        context: &WorkspaceContext,
        config: &JsonValue,
    ) -> Result<Option<f64>, ExtractorError> {
        let raw_a = config_get_str(config, "field_a")?;
        let raw_b = config_get_str(config, "field_b")?;
        let path_a = resolve_substitutions(&raw_a, context)?;
        let path_b = resolve_substitutions(&raw_b, context)?;

        let a = lookup_path(outcome, &path_a)
            .ok_or(ExtractorError::MissingField {
                field: path_a.clone(),
            })?
            .as_f64()
            .ok_or(ExtractorError::BadType {
                field: path_a.clone(),
                got: "non-numeric".to_string(),
                want: "number".to_string(),
            })?;
        let b = lookup_path(outcome, &path_b)
            .ok_or(ExtractorError::MissingField {
                field: path_b.clone(),
            })?
            .as_f64()
            .ok_or(ExtractorError::BadType {
                field: path_b.clone(),
                got: "non-numeric".to_string(),
                want: "number".to_string(),
            })?;

        Ok(Some(a - b))
    }
}

// ═════════════════════════════════════════════════════════════════════════════
// TESTS
// ═════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn registry_with_builtins_has_four() {
        let r = ExtractorRegistry::with_builtins();
        let list = r.list();
        assert_eq!(list.len(), 4);
        let names: Vec<_> = list.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"binary_winner_id_match"));
        assert!(names.contains(&"binary_field_value"));
        assert!(names.contains(&"scalar_field_value"));
        assert!(names.contains(&"scalar_difference"));
    }

    #[test]
    fn lookup_path_walks_nested() {
        let v = json!({ "a": { "b": { "c": 42 } } });
        assert_eq!(lookup_path(&v, "a.b.c"), Some(&json!(42)));
        assert_eq!(lookup_path(&v, "a.b"), Some(&json!({ "c": 42 })));
        assert_eq!(lookup_path(&v, ""), Some(&v));
        assert_eq!(lookup_path(&v, "missing"), None);
        assert_eq!(lookup_path(&v, "a.missing.c"), None);
    }

    #[test]
    fn lookup_path_handles_arrays() {
        let v = json!({ "items": [{ "x": 1 }, { "x": 2 }] });
        assert_eq!(lookup_path(&v, "items.0.x"), Some(&json!(1)));
        assert_eq!(lookup_path(&v, "items.1.x"), Some(&json!(2)));
        assert_eq!(lookup_path(&v, "items.5"), None);
    }

    #[test]
    fn substitution_resolves_entity_id() {
        let ctx = WorkspaceContext::with_entity("ARG");
        assert_eq!(
            resolve_substitutions("${workspace.entity_id}", &ctx).unwrap(),
            "ARG"
        );
        assert_eq!(
            resolve_substitutions("outcome.goals.${workspace.entity_id}", &ctx).unwrap(),
            "outcome.goals.ARG"
        );
        assert_eq!(resolve_substitutions("static", &ctx).unwrap(), "static");
    }

    #[test]
    fn substitution_errors_without_entity_id() {
        let ctx = WorkspaceContext::default();
        assert!(matches!(
            resolve_substitutions("${workspace.entity_id}", &ctx),
            Err(ExtractorError::NoEntity)
        ));
    }

    // ── binary_winner_id_match ─────────────────────────────────────────────

    #[test]
    fn binary_winner_match_emits_one_for_match() {
        let e = BinaryWinnerIdMatch;
        let outcome = json!({ "winner_team_id": "ARG", "home_goals": 2 });
        let ctx = WorkspaceContext::with_entity("ARG");
        let cfg = json!({ "winner_field": "winner_team_id", "match_value": "${workspace.entity_id}" });
        assert_eq!(e.extract(&outcome, &ctx, &cfg).unwrap(), Some(1.0));
    }

    #[test]
    fn binary_winner_match_emits_zero_for_mismatch() {
        let e = BinaryWinnerIdMatch;
        let outcome = json!({ "winner_team_id": "MEX" });
        let ctx = WorkspaceContext::with_entity("ARG");
        let cfg = json!({ "winner_field": "winner_team_id", "match_value": "${workspace.entity_id}" });
        assert_eq!(e.extract(&outcome, &ctx, &cfg).unwrap(), Some(0.0));
    }

    #[test]
    fn binary_winner_match_errors_on_missing_field() {
        let e = BinaryWinnerIdMatch;
        let outcome = json!({ "score": "2-1" });
        let ctx = WorkspaceContext::with_entity("ARG");
        let cfg = json!({ "winner_field": "winner_team_id", "match_value": "${workspace.entity_id}" });
        assert!(matches!(
            e.extract(&outcome, &ctx, &cfg),
            Err(ExtractorError::MissingField { .. })
        ));
    }

    // ── binary_field_value ─────────────────────────────────────────────────

    #[test]
    fn binary_field_matches_boolean() {
        let e = BinaryFieldValue;
        let outcome = json!({ "advanced": true });
        let ctx = WorkspaceContext::default();
        let cfg = json!({ "path": "advanced", "value": true });
        assert_eq!(e.extract(&outcome, &ctx, &cfg).unwrap(), Some(1.0));

        let outcome = json!({ "advanced": false });
        assert_eq!(e.extract(&outcome, &ctx, &cfg).unwrap(), Some(0.0));
    }

    #[test]
    fn binary_field_matches_string() {
        let e = BinaryFieldValue;
        let outcome = json!({ "round_reached": "final" });
        let ctx = WorkspaceContext::default();
        let cfg = json!({ "path": "round_reached", "value": "final" });
        assert_eq!(e.extract(&outcome, &ctx, &cfg).unwrap(), Some(1.0));

        let cfg = json!({ "path": "round_reached", "value": "quarter" });
        assert_eq!(e.extract(&outcome, &ctx, &cfg).unwrap(), Some(0.0));
    }

    // ── scalar_field_value ─────────────────────────────────────────────────

    #[test]
    fn scalar_field_extracts_number() {
        let e = ScalarFieldValue;
        let outcome = json!({ "goals": { "ARG": 2, "MEX": 1 } });
        let ctx = WorkspaceContext::with_entity("ARG");
        let cfg = json!({ "path": "goals.${workspace.entity_id}" });
        assert_eq!(e.extract(&outcome, &ctx, &cfg).unwrap(), Some(2.0));
    }

    #[test]
    fn scalar_field_errors_on_non_numeric() {
        let e = ScalarFieldValue;
        let outcome = json!({ "result": "win" });
        let ctx = WorkspaceContext::default();
        let cfg = json!({ "path": "result" });
        assert!(matches!(
            e.extract(&outcome, &ctx, &cfg),
            Err(ExtractorError::BadType { .. })
        ));
    }

    // ── scalar_difference ──────────────────────────────────────────────────

    #[test]
    fn scalar_difference_computes_diff() {
        let e = ScalarDifference;
        let outcome = json!({ "goals": { "ARG": 3, "opponent": 1 } });
        let ctx = WorkspaceContext::with_entity("ARG");
        let cfg = json!({
            "field_a": "goals.${workspace.entity_id}",
            "field_b": "goals.opponent"
        });
        assert_eq!(e.extract(&outcome, &ctx, &cfg).unwrap(), Some(2.0));
    }

    #[test]
    fn scalar_difference_handles_negative() {
        let e = ScalarDifference;
        let outcome = json!({ "goals": { "ARG": 1, "opponent": 4 } });
        let ctx = WorkspaceContext::with_entity("ARG");
        let cfg = json!({
            "field_a": "goals.${workspace.entity_id}",
            "field_b": "goals.opponent"
        });
        assert_eq!(e.extract(&outcome, &ctx, &cfg).unwrap(), Some(-3.0));
    }

    // ── registry ───────────────────────────────────────────────────────────

    #[test]
    fn registry_lookup_returns_extractor() {
        let r = ExtractorRegistry::with_builtins();
        let e = r.get("binary_winner_id_match").expect("registered");
        assert_eq!(e.name(), "binary_winner_id_match");
    }

    #[test]
    fn registry_lookup_returns_none_for_unknown() {
        let r = ExtractorRegistry::with_builtins();
        assert!(r.get("nonexistent").is_none());
    }

    #[test]
    #[should_panic(expected = "duplicate extractor name")]
    fn registry_panics_on_duplicate_registration() {
        let mut r = ExtractorRegistry::new();
        r.register(Arc::new(BinaryWinnerIdMatch));
        r.register(Arc::new(BinaryWinnerIdMatch));
    }
}
