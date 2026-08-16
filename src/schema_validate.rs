//! # A minimal JSON Schema validator that refuses to guess
//!
//! ## Why not a crate
//!
//! The workspace has **no** schema-validation capability at all — that is the
//! reason `docs/papers/verification_for_agent_ecologies.md` can say every
//! declared contract in the corpus is cosmetic by construction. Something has
//! to close that.
//!
//! Adding `jsonschema` would work and was the obvious choice. Against it: the
//! repo currently carries 24 open advisories across its dependency tree, this
//! code sits on a request path, and the schemas we actually author use seven
//! keywords. Trading a full Draft 2020-12 implementation for a dependency is
//! a poor bargain when six hundred lines of it would never execute.
//!
//! ## The property that matters more than coverage
//!
//! **An unsupported keyword is not a pass.**
//!
//! A validator that silently ignores what it cannot interpret is worse than
//! no validator, because it returns `valid` for a document it never checked —
//! and "green" is indistinguishable from "inert" from the outside. That is
//! the failure this entire line of work exists to remove, so it would be
//! remarkable to reintroduce it here.
//!
//! [`Report`] therefore separates two things a naive validator conflates:
//!
//! ```text
//!   violations   the instance contradicts the schema
//!   unsupported  the schema said something this validator cannot evaluate
//! ```
//!
//! [`Report::is_valid`] requires **both** to be empty. So an unrecognised
//! keyword fails the document — but fails it with "I could not check this"
//! rather than "this is wrong", which are different facts and lead to
//! different fixes.
//!
//! ## Supported keywords
//!
//! `type` · `properties` · `required` · `additionalProperties: false` ·
//! `enum` · `const` · `items`
//!
//! Annotations (`$schema`, `$id`, `title`, `description`, and the
//! `_evidence`/`_draft` markers `scripts/port_migrate.py` emits) are ignored
//! deliberately rather than reported, because they make no assertion about
//! the instance.

use serde_json::Value;

/// Schema keys that assert nothing about the instance.
const ANNOTATIONS: &[&str] = &[
    "$schema",
    "$id",
    "$comment",
    "title",
    "description",
    "examples",
    "default",
    "deprecated",
    "readOnly",
    "writeOnly",
    // Emitted by scripts/port_migrate.py drafts.
    "_evidence",
    "_draft",
    "_generated_by",
    "_evidence_note",
];

/// Keywords this validator implements.
const SUPPORTED: &[&str] = &[
    "type",
    "properties",
    "required",
    "additionalProperties",
    "enum",
    "const",
    "items",
];

/// Guard against a pathological or hostile document exhausting the stack.
/// Our deepest real schema is 3 levels; 32 is generous and finite.
const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    /// Dotted path into the instance, e.g. `genome.estimated_size_mb`.
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    /// The instance contradicts the schema.
    pub violations: Vec<Violation>,
    /// The schema asked for something this validator cannot evaluate.
    /// Non-empty means the document is **unverified**, not that it is wrong.
    pub unsupported: Vec<String>,
}

impl Report {
    /// Valid means checked and conforming. An unsupported keyword fails this
    /// deliberately: see the module docs.
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty() && self.unsupported.is_empty()
    }

    /// Was the failure a contradiction, as opposed to an inability to check?
    pub fn is_contradiction(&self) -> bool {
        !self.violations.is_empty()
    }

    fn violate(&mut self, path: &str, message: impl Into<String>) {
        self.violations.push(Violation {
            path: if path.is_empty() {
                "<root>".into()
            } else {
                path.into()
            },
            message: message.into(),
        });
    }
}

/// Validate `instance` against `schema`.
pub fn validate(schema: &Value, instance: &Value) -> Report {
    let mut report = Report::default();
    walk(schema, instance, "", 0, &mut report);
    report
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(n) => number_type(n),
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// `integer` is a JSON Schema type distinct from `number`; serde_json does
/// not distinguish them, so decide from the value.
fn number_type(n: &serde_json::Number) -> &'static str {
    if n.is_i64() || n.is_u64() {
        "integer"
    } else {
        "number"
    }
}

fn type_matches(expected: &str, instance: &Value) -> bool {
    match expected {
        // Every integer is also a number. The converse is not true.
        "number" => matches!(instance, Value::Number(_)),
        other => type_name(instance) == other,
    }
}

fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        key.to_string()
    } else {
        format!("{path}.{key}")
    }
}

fn walk(schema: &Value, instance: &Value, path: &str, depth: usize, report: &mut Report) {
    if depth > MAX_DEPTH {
        report.violate(path, format!("nesting deeper than {MAX_DEPTH} levels"));
        return;
    }
    let Some(obj) = schema.as_object() else {
        // `true`/`false` schemas are legal JSON Schema and we do not
        // implement them. Say so rather than pass.
        report
            .unsupported
            .push(format!("{path}: schema is not an object"));
        return;
    };

    // Anything we do not implement makes the document unverified.
    for key in obj.keys() {
        if !SUPPORTED.contains(&key.as_str()) && !ANNOTATIONS.contains(&key.as_str()) {
            report
                .unsupported
                .push(format!("{}: unsupported keyword `{key}`", disp(path)));
        }
    }

    // ── type ───────────────────────────────────────────────────────
    if let Some(t) = obj.get("type") {
        let ok = match t {
            Value::String(s) => type_matches(s, instance),
            Value::Array(alts) => alts
                .iter()
                .filter_map(|a| a.as_str())
                .any(|s| type_matches(s, instance)),
            _ => {
                report.unsupported.push(format!(
                    "{}: `type` is neither string nor array",
                    disp(path)
                ));
                return;
            }
        };
        if !ok {
            let want = match t {
                Value::String(s) => s.clone(),
                Value::Array(a) => a
                    .iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join("|"),
                _ => unreachable!(),
            };
            report.violate(
                path,
                format!("expected type `{want}`, got `{}`", type_name(instance)),
            );
            // Further keywords assume the type; stop here to avoid a cascade
            // of confusing secondary errors.
            return;
        }
    }

    // ── const / enum ───────────────────────────────────────────────
    if let Some(c) = obj.get("const") {
        if instance != c {
            report.violate(path, format!("must equal {c}, got {instance}"));
        }
    }
    if let Some(Value::Array(allowed)) = obj.get("enum") {
        if !allowed.contains(instance) {
            let opts: Vec<String> = allowed.iter().map(|v| v.to_string()).collect();
            report.violate(
                path,
                format!("must be one of [{}], got {instance}", opts.join(", ")),
            );
        }
    }

    // ── objects ────────────────────────────────────────────────────
    if let Some(map) = instance.as_object() {
        if let Some(Value::Array(req)) = obj.get("required") {
            for r in req.iter().filter_map(|v| v.as_str()) {
                if !map.contains_key(r) {
                    report.violate(&join(path, r), "required property is missing");
                }
            }
        }
        let props = obj.get("properties").and_then(|p| p.as_object());
        if let Some(props) = props {
            for (k, sub) in props {
                if let Some(child) = map.get(k) {
                    walk(sub, child, &join(path, k), depth + 1, report);
                }
            }
        }
        match obj.get("additionalProperties") {
            Some(Value::Bool(false)) => {
                let known = props
                    .map(|p| p.keys().collect::<Vec<_>>())
                    .unwrap_or_default();
                for k in map.keys() {
                    if !known.iter().any(|s| *s == k) {
                        report.violate(
                            &join(path, k),
                            "property is not permitted (additionalProperties is false)",
                        );
                    }
                }
            }
            Some(Value::Bool(true)) | None => {}
            Some(_) => report.unsupported.push(format!(
                "{}: `additionalProperties` as a schema is not supported",
                disp(path)
            )),
        }
    }

    // ── arrays ─────────────────────────────────────────────────────
    if let (Some(items), Some(arr)) = (obj.get("items"), instance.as_array()) {
        for (i, el) in arr.iter().enumerate() {
            walk(
                items,
                el,
                &format!("{}[{i}]", disp(path)),
                depth + 1,
                report,
            );
        }
    }
}

fn disp(path: &str) -> &str {
    if path.is_empty() {
        "<root>"
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_conforming_document_passes() {
        let schema = json!({
            "type": "object",
            "required": ["a"],
            "properties": { "a": { "type": "string" }, "n": { "type": ["number", "null"] } }
        });
        let r = validate(&schema, &json!({ "a": "x", "n": 1.5 }));
        assert!(r.is_valid(), "{r:?}");
    }

    #[test]
    fn a_wrong_type_is_a_contradiction_with_a_path() {
        let schema = json!({ "type": "object",
            "properties": { "genome": { "type": "object",
                "properties": { "estimated_size_mb": { "type": ["number", "null"] } } } } });
        let r = validate(
            &schema,
            &json!({ "genome": { "estimated_size_mb": "420-480" } }),
        );
        assert!(r.is_contradiction());
        assert_eq!(r.violations[0].path, "genome.estimated_size_mb");
        assert!(r.violations[0].message.contains("expected type"));
    }

    /// The property this validator exists for.
    #[test]
    fn an_unsupported_keyword_is_not_a_pass() {
        let schema = json!({ "type": "string", "minLength": 5 });
        let r = validate(&schema, &json!("hi"));
        assert!(
            !r.is_valid(),
            "a schema this validator cannot fully evaluate must not report valid"
        );
        assert!(
            !r.is_contradiction(),
            "but it is not a contradiction either — the instance was never checked"
        );
        assert!(r.unsupported[0].contains("minLength"));
    }

    #[test]
    fn annotations_are_ignored_not_reported() {
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$id": "acme/thing", "title": "Thing", "description": "d",
            "type": "object", "properties": { "a": { "type": "string", "_evidence": "prompt_json" } }
        });
        let r = validate(&schema, &json!({ "a": "x" }));
        assert!(r.is_valid(), "{r:?}");
    }

    #[test]
    fn required_and_additional_properties_both_bite() {
        let schema = json!({
            "type": "object", "additionalProperties": false,
            "required": ["a"], "properties": { "a": { "type": "string" } }
        });
        let missing = validate(&schema, &json!({}));
        assert_eq!(missing.violations[0].path, "a");
        assert!(missing.violations[0].message.contains("required"));

        let extra = validate(&schema, &json!({ "a": "x", "sneaky": 1 }));
        assert_eq!(extra.violations[0].path, "sneaky");
        assert!(extra.violations[0].message.contains("not permitted"));
    }

    #[test]
    fn const_and_enum_are_enforced() {
        let s = json!({ "const": "unavailable_no_tool_source" });
        assert!(validate(&s, &json!("unavailable_no_tool_source")).is_valid());
        assert!(validate(&s, &json!("tool_verified")).is_contradiction());

        let e = json!({ "enum": ["tool_verified", "tool_no_match"] });
        assert!(validate(&e, &json!("tool_no_match")).is_valid());
        assert!(validate(&e, &json!("estimated")).is_contradiction());
    }

    #[test]
    fn integer_and_number_are_distinguished_but_compatible() {
        assert!(validate(&json!({ "type": "integer" }), &json!(30)).is_valid());
        assert!(validate(&json!({ "type": "integer" }), &json!(30.5)).is_contradiction());
        // Every integer is a number.
        assert!(validate(&json!({ "type": "number" }), &json!(30)).is_valid());
        assert!(validate(&json!({ "type": "number" }), &json!(245.2)).is_valid());
    }

    #[test]
    fn array_items_are_checked_with_indexed_paths() {
        let s = json!({ "type": "array", "items": { "type": "string" } });
        let r = validate(&s, &json!(["a", 2, "c"]));
        assert!(r.is_contradiction());
        assert_eq!(r.violations[0].path, "<root>[1]");
    }

    #[test]
    fn a_type_failure_does_not_cascade() {
        // Reporting "expected object, got string" AND every missing required
        // property of that object would bury the real error.
        let s = json!({ "type": "object", "required": ["a", "b", "c"],
                        "properties": { "a": {}, "b": {}, "c": {} } });
        let r = validate(&s, &json!("not an object"));
        assert_eq!(r.violations.len(), 1, "{:?}", r.violations);
    }

    #[test]
    fn deep_nesting_is_bounded_rather_than_fatal() {
        let mut inst = json!(1);
        for _ in 0..80 {
            inst = json!({ "a": inst });
        }
        let mut schema = json!({ "type": "integer" });
        for _ in 0..80 {
            schema = json!({ "type": "object", "properties": { "a": schema } });
        }
        let r = validate(&schema, &inst); // must not overflow the stack
        assert!(!r.is_valid());
    }

    /// The real card, the real document, end to end.
    #[test]
    fn the_pilot_agents_declared_schema_validates_its_own_output() {
        let card: Value = serde_json::from_str(
            &std::fs::read_to_string("agents/curated/genome_profiler/agent_card.json").unwrap(),
        )
        .unwrap();
        let schema = &card["capabilities"]["output_contract"]["schema"];
        assert!(schema.is_object(), "the pilot must declare a real schema");

        let good = json!({
            "taxonomy": { "kingdom": "Animalia", "phylum": "Arthropoda", "class": "Insecta",
                          "order": "Lepidoptera", "family": "Nymphalidae",
                          "genus": "Apatura", "species": "Apatura iris" },
            "taxonomy_provenance": "tool_verified",
            "genome": { "estimated_size_mb": 245.2, "chromosome_count": 30,
                        "assembly_name": "MEX_DaPlex", "assembly_accession": "GCA_018135715.1",
                        "notable_genes": null, "ploidy": null },
            "genome_provenance": "tool_verified",
            "phylogeny": { "sister_taxa": ["Apatura ilia"], "superorder": "Holometabola",
                           "divergence_mya": null, "defining_traits": null },
            "phylogeny_provenance": "platform_derived",
            "conservation": { "iucn_status": null, "population_trend": null,
                              "genetic_diversity_notes": null },
            "conservation_provenance": "unavailable_no_tool_source",
            "summary": "GBIF places Apatura iris in Nymphalidae."
        });
        let r = validate(schema, &good);
        assert!(r.is_valid(), "the pilot's own output must validate: {r:?}");
    }

    /// And the adversarial case: the fabrication the whole workstream began
    /// with must be rejected by the schema, not merely stripped by grounding.
    #[test]
    fn the_original_fabrication_fails_the_pilots_schema() {
        let card: Value = serde_json::from_str(
            &std::fs::read_to_string("agents/curated/genome_profiler/agent_card.json").unwrap(),
        )
        .unwrap();
        let schema = &card["capabilities"]["output_contract"]["schema"];

        // Verbatim shape of what shipped for 56 episodes: a genome size as a
        // range string, and a fabricated conservation status.
        let bad = json!({
            "taxonomy": {}, "taxonomy_provenance": "tool_verified",
            "genome": { "estimated_size_mb": "420-480" },
            "genome_provenance": "tool_verified",
            "phylogeny": {}, "phylogeny_provenance": "tool_verified",
            "conservation": { "iucn_status": "Not Evaluated (presumed Least Concern)" },
            "conservation_provenance": "unavailable_no_tool_source",
            "summary": "…"
        });
        let r = validate(schema, &bad);
        assert!(r.is_contradiction(), "the schema must reject this");
        let paths: Vec<&str> = r.violations.iter().map(|v| v.path.as_str()).collect();
        assert!(
            paths.contains(&"genome.estimated_size_mb"),
            "a range string is not a number: {paths:?}"
        );
        assert!(
            paths.contains(&"conservation.iucn_status"),
            "the schema pins unsourceable fields to null: {paths:?}"
        );
    }
}
