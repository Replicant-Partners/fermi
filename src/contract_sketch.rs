//! # Compiling a typed output contract from a sketch
//!
//! ## The measurement that motivates this
//!
//! 98 of the 101 curated cards declare no typed output contract, and
//! `agent_contract::TYPED_TIER_EXEMPT` grandfathers 86 of them. That is not
//! 86 authors who disagreed with the contract; it is a contract whose
//! authoring cost nobody paid twice.
//!
//! Count the work on the one card that does satisfy
//! [`crate::card_contract::validate`] end to end. `hud_field_scout` declares
//! **six** evidence blocks. Those six expand to:
//!
//! ```text
//!   14 schema properties      (6 blocks + 6 provenance siblings + summary + an audit marker)
//!   14 grounding entries      (bijection with the above — enforced)
//!    6 narrowed provenance enums
//!    1 required list of 13 names
//! ```
//!
//! Six decisions, thirty-five artefacts. And of the fourteen grounding
//! entries, **eight** are platform-stamp boilerplate whose prose is
//! near-identical block to block — each needing 40+ characters of `why`
//! (`card_contract::MIN_WHY`) to clear the gate. An author who writes that
//! once writes it correctly; an author who writes it six times copies the
//! nearest neighbour, which is the failure mode
//! `card_contract::grounding_explained` exists to catch and cannot.
//!
//! ## What is actually a decision, and what is derivable
//!
//! Three things require a human (or an agent that can be held to account):
//!
//! 1. What blocks does the document have?
//! 2. What fields, of what type, does each block hold?
//! 3. Where does each block's value come from, and **why**?
//!
//! Everything else follows mechanically: the `_provenance` sibling, its
//! narrowed enum, its grounding entry, the `required` list,
//! `additionalProperties: false`, `$id`, the nullable unions, and the
//! rewrite of `produces` to reference the declared type. This module
//! authors the first three and computes the rest.
//!
//! ## The property that makes it worth having
//!
//! [`Sketch::compile`] emits `schema.properties` and `grounding` **from one
//! traversal of one block list**. The bijection between them — the
//! `grounding_declared` check, which is the one an author fails most because
//! it is the one that scales with the number of fields — therefore cannot be
//! violated by construction. It is not checked and reported; it is
//! unrepresentable, in the same way `football_analyst`'s narrowed provenance
//! enums make a dishonest stamp unrepresentable rather than discouraged.
//!
//! The compiler then runs `card_contract::validate` over its own output and
//! refuses to return anything that would not publish. So:
//!
//! ```text
//!   compile() returned Ok  ⟹  the Admission gate passes
//! ```
//!
//! `contract_compiles_to_something_the_gate_accepts` holds that line.
//!
//! ## Where this differs from `scripts/port_migrate.py`
//!
//! That tool is deliberately a *proposer*: it emits `NEEDS_AUTHOR`, which is
//! not a valid `grounding.status`, precisely so a draft cannot be pasted into
//! a card and published. Its input is a card that contains no evidence for
//! the type it is being asked to invent, so anything it emitted confidently
//! would be a fabrication with good manners.
//!
//! This module's input is different: a sketch is *authored*. The statuses,
//! the `why`s and the tool bindings come from a person. So it is allowed to
//! produce a publishable contract — it is expanding a declaration, not
//! guessing one.
//!
//! ### The one line the compiler will not cross
//!
//! **A generated `why` may only describe what the platform does. It may
//! never describe where the agent's data comes from.**
//!
//! The provenance-sibling entries are generated with prose, because their
//! subject is `grounding_trust::enforce` — platform behaviour this module
//! knows for certain. Every entry describing an agent's own value requires
//! an authored `why`, and a missing one is an error rather than a default.
//! Blur that and this becomes the fabrication engine `port_migrate.py`
//! refuses to be.
//!
//! ## Ontology binding
//!
//! Field vocabulary can come from an agent's ontology rather than an
//! author's memory: `"@sentiment"` resolves against an [`Ontology`], taking
//! the type, the closed value set and the definition from the entity. See
//! [`Ontology::field`] — including why it puts a numeric range in the
//! *description* rather than emitting `minimum`/`maximum`.

use crate::card_contract::{self, Finding};
use crate::grounding_trust::{
    PROV_INFERRED, PROV_NO_MATCH, PROV_PENDING_TOOL, PROV_TOOL, PROV_UNAVAILABLE,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

/// Suffix of the sibling stamp `grounding_trust::enforce` writes per block.
/// An author may not use it as a block name: the compiler owns that namespace.
pub const PROVENANCE_SUFFIX: &str = "_provenance";

/// The JSON Schema dialect the corpus declares.
const DIALECT: &str = "https://json-schema.org/draft/2020-12/schema";

fn f(check: &'static str, message: impl Into<String>) -> Finding {
    Finding {
        check,
        message: message.into(),
    }
}

// ─── the type mini-language ────────────────────────────────────────────

/// A leaf type, written as a short string rather than a JSON Schema object.
///
/// ```text
///   string            {"type": "string"}
///   integer?          {"type": ["integer", "null"]}
///   string[]          {"type": "array", "items": {"type": "string"}}
///   number[]?         {"type": ["array", "null"], "items": {"type": "number"}}
///   enum:up|down|flat {"enum": ["up", "down", "flat"]}
///   enum:a|b?         {"enum": ["a", "b", null]}
///   const:platform    {"const": "platform"}
/// ```
///
/// The grammar is `<base>` `[]`? `?`? — array before nullable, because
/// "nullable array of strings" and "array of nullable strings" are different
/// types and only one order can mean one of them.
///
/// **Every form emits only keywords `crate::schema_validate` implements.**
/// That is a hard constraint, not a coincidence: a schema carrying one
/// keyword the validator cannot evaluate makes the whole document
/// `unverified_unsupported_schema` at the delegation hop — which is *not a
/// pass*, and is strictly worse than having declared less. `minimum`,
/// `pattern` and `format` are therefore unavailable here on purpose.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeExpr {
    pub base: Base,
    pub array: bool,
    pub nullable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Base {
    String,
    Integer,
    Number,
    Boolean,
    /// A nested object whose fields this sketch does not enumerate. Allowed,
    /// but it types nothing below the top of the block — prefer `fields`.
    Object,
    /// A closed set of string values.
    Enum(Vec<String>),
    /// Exactly one value.
    Const(String),
    /// Pinned to `null`: nothing can supply this field, ever.
    ///
    /// Not the same as nullable. `string?` says "a value or nothing"; `null`
    /// says "nothing, and a value here would be a contract violation". It is
    /// the field-level form of the `unavailable` grounding status, and the
    /// corpus already used it — `hud_field_scout.edibility.verdict` and five
    /// fields across `genome_profiler` — before the compiler could express it.
    /// The decompiler found that gap.
    ///
    /// `envelope::build` names the consequence: grounding runs BEFORE
    /// validation precisely so a field pinned this way is cleaned before it is
    /// checked, rather than the agent being blamed for a null the platform was
    /// about to write.
    Null,
}

impl TypeExpr {
    /// Parse a type expression. The error is the message an author reads.
    pub fn parse(src: &str) -> Result<Self, String> {
        let mut s = src.trim();
        if s.is_empty() {
            return Err("empty type expression".into());
        }
        let nullable = s.ends_with('?');
        if nullable {
            s = s[..s.len() - 1].trim_end();
        }
        let array = s.ends_with("[]");
        if array {
            s = s[..s.len() - 2].trim_end();
        }
        if s.ends_with('?') {
            return Err(format!(
                "`{src}`: write the nullable marker last, as `{}[]?`. `[]?` is a \
                 nullable array; `?[]` would be an array of nullables, and letting \
                 both orders mean the same thing would make one of the two types \
                 unwritable.",
                s.trim_end_matches('?')
            ));
        }

        if s == "null" && (nullable || array) {
            return Err(format!(
                "`{src}`: `null` is already the absence of a value, so `?` and \
                 `[]` add nothing to it. Write plain `null` for a field pinned \
                 to null, or `string?` for one that may or may not have a value \
                 — they are different claims and only the first says nothing \
                 could ever supply it."
            ));
        }

        let base = match s {
            "string" => Base::String,
            "integer" => Base::Integer,
            "number" => Base::Number,
            "boolean" => Base::Boolean,
            "object" => Base::Object,
            "null" => Base::Null,
            other if other.starts_with("enum:") => {
                let vals: Vec<String> = other[5..]
                    .split('|')
                    .map(|v| v.trim().to_string())
                    .filter(|v| !v.is_empty())
                    .collect();
                if vals.len() < 2 {
                    return Err(format!(
                        "`{src}`: an `enum:` needs at least two values separated by \
                         `|`. A one-value enum is a `const:`, and saying so makes the \
                         intent legible."
                    ));
                }
                Base::Enum(vals)
            }
            other if other.starts_with("const:") => {
                let v = other[6..].trim();
                if v.is_empty() {
                    return Err(format!("`{src}`: `const:` needs a value."));
                }
                Base::Const(v.to_string())
            }
            other => {
                return Err(format!(
                    "`{src}`: unknown type `{other}`. Available: string, integer, \
                     number, boolean, object, `enum:a|b|c`, `const:v`, or `@entity` \
                     to take the type from the agent's ontology. Suffix `[]` for an \
                     array and `?` for nullable, in that order.\n\
                     Deliberately absent: `minimum`, `pattern`, `format`. \
                     src/schema_validate.rs cannot evaluate them, and a schema it \
                     cannot evaluate reports `unverified_unsupported_schema` at the \
                     delegation hop — which is not a pass. State the constraint in a \
                     `description` where a reader gets it and the validator is not \
                     asked to lie about it."
                ))
            }
        };
        Ok(TypeExpr {
            base,
            array,
            nullable,
        })
    }

    /// JSON Schema for this leaf, with an optional description.
    pub fn to_schema(&self, description: Option<&str>) -> Value {
        let mut out = Map::new();

        // The item schema, before array/nullable wrapping.
        let scalar_type = |b: &Base| -> Option<&'static str> {
            match b {
                Base::String => Some("string"),
                Base::Integer => Some("integer"),
                Base::Number => Some("number"),
                Base::Boolean => Some("boolean"),
                Base::Object => Some("object"),
                Base::Null => Some("null"),
                Base::Enum(_) | Base::Const(_) => None,
            }
        };

        match (&self.base, self.array) {
            // enum / const, not an array: the keyword carries the type.
            (Base::Enum(vals), false) => {
                let mut items: Vec<Value> = vals.iter().map(|v| json!(v)).collect();
                if self.nullable {
                    // `null` joins the enum rather than a type union: `enum`
                    // already fully determines the admissible set, and adding
                    // `type` alongside it would be a second, redundant
                    // assertion that could disagree with the first.
                    items.push(Value::Null);
                }
                out.insert("enum".into(), Value::Array(items));
            }
            (Base::Const(v), false) => {
                out.insert("const".into(), json!(v));
            }
            // arrays
            (base, true) => {
                let ty = if self.nullable {
                    json!(["array", "null"])
                } else {
                    json!("array")
                };
                out.insert("type".into(), ty);
                let mut items = Map::new();
                match base {
                    Base::Enum(vals) => {
                        items.insert(
                            "enum".into(),
                            Value::Array(vals.iter().map(|v| json!(v)).collect()),
                        );
                    }
                    Base::Const(v) => {
                        items.insert("const".into(), json!(v));
                    }
                    other => {
                        items.insert("type".into(), json!(scalar_type(other).unwrap_or("string")));
                    }
                }
                out.insert("items".into(), Value::Object(items));
            }
            // plain scalars
            (base, false) => {
                let name = scalar_type(base).unwrap_or("string");
                let ty = if self.nullable {
                    json!([name, "null"])
                } else {
                    json!(name)
                };
                out.insert("type".into(), ty);
            }
        }

        if let Some(d) = description.filter(|d| !d.trim().is_empty()) {
            out.insert("description".into(), json!(d));
        }
        Value::Object(out)
    }
}

// ─── the sketch ────────────────────────────────────────────────────────

/// A field of a block: a type expression, optionally with prose.
///
/// Accepts the shorthand `"number?"` and the long form
/// `{"type": "number?", "description": "..."}`, because most fields need no
/// prose and the ones that do need it badly.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FieldSpec {
    Short(String),
    Long {
        #[serde(rename = "type")]
        ty: String,
        #[serde(default)]
        description: Option<String>,
    },
}

impl FieldSpec {
    fn ty(&self) -> &str {
        match self {
            FieldSpec::Short(s) => s,
            FieldSpec::Long { ty, .. } => ty,
        }
    }
    fn description(&self) -> Option<&str> {
        match self {
            FieldSpec::Short(_) => None,
            FieldSpec::Long { description, .. } => description.as_deref(),
        }
    }
}

/// How completely a tool covers the block it sources.
///
/// This is the whole reason the provenance enums in the corpus differ from
/// each other — `genome_profiler.taxonomy` admits two verdicts and
/// `genome_profiler.genome` admits three. Asking the author the question
/// once, here, is what lets the compiler narrow the enum correctly instead
/// of emitting the widest set and calling it safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    /// The tool answers for every field, or honestly reports no match.
    #[default]
    Complete,
    /// The tool covers part of the block; the rest has no source at all, so
    /// `unavailable_no_tool_source` is a reachable verdict.
    Partial,
    /// The check exists but may not have run when the document was built, so
    /// `pending_tool_check` is reachable. Distinct from `Partial`: "not yet
    /// asked" and "asked, nothing exists" need different fixes.
    Deferred,
    /// Both at once: part of the block has no source at all, AND the check on
    /// the part that does may not have run.
    ///
    /// Added for `football_analyst.advanced_metrics`, whose hand-written
    /// contract already declared all four verdicts. `xg` comes from
    /// `fixtures/statistics` and is often simply not asked for; `ppda` and
    /// `progressive_passes` are Opta event-data metrics that API-Football does
    /// not carry at all and never will.
    ///
    /// Without this the migration had to pick one, and both choices destroy a
    /// distinction the platform makes elsewhere. Dropping `pending_tool_check`
    /// collapses "never asked" into `tool_no_match` ("asked and had nothing")
    /// — which is the exact pair the trace view is built to separate, and the
    /// pair `Deferred`'s own doc comment says "need different fixes". Dropping
    /// `unavailable_no_tool_source` leaves the contract unable to say that
    /// `ppda` is unobtainable, so a null there would read as a failed lookup
    /// rather than an honest gap.
    ///
    /// Rare on purpose. A block needing this is usually a block that wants
    /// splitting; `advanced_metrics` cannot be split because it is a live
    /// document shape with consumers.
    PartialDeferred,
}

impl Coverage {
    /// Every authorable token, in the order an author should consider them.
    ///
    /// Exists so the things that must enumerate coverage — `xaman_ek`'s
    /// guidance prompt and the test that checks it — read the list rather than
    /// keeping a second copy. A hand-maintained literal is how a fourth
    /// variant gets added and the assistant keeps describing three.
    pub const TOKENS: &'static [&'static str] =
        &["complete", "partial", "deferred", "partial_deferred"];
}

/// Where a block's value comes from. Mirrors `card_contract::GROUNDING_STATUSES`
/// and is the sole input to the provenance enum.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Source {
    /// A declared tool returns it. The tool name is cross-checked against
    /// what the agent actually declares — the check with teeth.
    Sourced {
        tool: String,
        response_field: String,
        #[serde(default)]
        coverage: Coverage,
    },
    /// The model reasons it out from something named.
    Inferred { from: String },
    /// Prose. Gets no provenance sibling, per `grounding_trust.rs`: a block
    /// that is only ever a narrative must not carry a stamp, because a stamp
    /// on prose is a retrieval claim about a sentence.
    Narrative,
    /// Nothing available can supply it, so it must be null.
    Unavailable {
        /// What would have to be wired up for this to become `sourced`.
        /// Optional, and worth writing: it turns a null into a to-do.
        #[serde(default)]
        would_need: Option<String>,
    },
}

impl Source {
    fn status(&self) -> &'static str {
        match self {
            Source::Sourced { .. } => "sourced",
            Source::Inferred { .. } => "inferred",
            Source::Narrative => "narrative",
            Source::Unavailable { .. } => "unavailable",
        }
    }

    /// The narrowed schema for this block's `_provenance` sibling, or `None`
    /// when the block gets no sibling at all.
    fn provenance_schema(&self) -> Option<Value> {
        let verdicts: Vec<&str> = match self {
            Source::Sourced { coverage, .. } => match coverage {
                Coverage::Complete => vec![PROV_TOOL, PROV_NO_MATCH],
                Coverage::Partial => vec![PROV_TOOL, PROV_NO_MATCH, PROV_UNAVAILABLE],
                Coverage::Deferred => vec![PROV_TOOL, PROV_NO_MATCH, PROV_PENDING_TOOL],
                Coverage::PartialDeferred => vec![
                    PROV_TOOL,
                    PROV_NO_MATCH,
                    PROV_UNAVAILABLE,
                    PROV_PENDING_TOOL,
                ],
            },
            Source::Inferred { .. } => return Some(json!({ "const": PROV_INFERRED })),
            Source::Unavailable { .. } => return Some(json!({ "const": PROV_UNAVAILABLE })),
            Source::Narrative => return None,
        };
        Some(json!({ "enum": verdicts }))
    }

    /// Prose naming the verdicts, for the generated sibling's `why`.
    fn verdict_prose(&self, block: &str) -> String {
        match self {
            Source::Sourced { tool, coverage, .. } => {
                let base = format!(
                    "`{PROV_TOOL}` when `{tool}` returned a value, `{PROV_NO_MATCH}` \
                     when it was asked and had nothing"
                );
                match coverage {
                    Coverage::Complete => base,
                    Coverage::Partial => format!(
                        "{base}, and `{PROV_UNAVAILABLE}` for the parts of `{block}` \
                         that `{tool}` does not cover"
                    ),
                    Coverage::Deferred => {
                        format!("{base}, and `{PROV_PENDING_TOOL}` when the check had not run yet")
                    }
                    Coverage::PartialDeferred => format!(
                        "{base}, `{PROV_UNAVAILABLE}` for the parts of `{block}` that \
                         `{tool}` does not cover, and `{PROV_PENDING_TOOL}` when the \
                         check on the parts it does cover had not run yet"
                    ),
                }
            }
            Source::Inferred { .. } => format!(
                "constant `{PROV_INFERRED}`, because every field under `{block}` is a \
                 judgement rather than a retrieval"
            ),
            Source::Unavailable { .. } => format!(
                "constant `{PROV_UNAVAILABLE}`. Constant rather than variable on \
                 purpose: if a source is wired up later this becomes an enum, and the \
                 change shows in the schema diff rather than only in behaviour"
            ),
            Source::Narrative => String::new(),
        }
    }

    /// The card grounding entry for the block itself, from the author's `why`.
    fn grounding_entry(&self, why: &str) -> Value {
        let mut m = Map::new();
        m.insert("status".into(), json!(self.status()));
        match self {
            Source::Sourced {
                tool,
                response_field,
                ..
            } => {
                m.insert("tool".into(), json!(tool));
                m.insert("response_field".into(), json!(response_field));
            }
            Source::Inferred { from } => {
                m.insert("from".into(), json!(from));
            }
            Source::Unavailable { would_need } => {
                if let Some(w) = would_need.as_deref().filter(|w| !w.trim().is_empty()) {
                    m.insert("would_need".into(), json!(w));
                }
            }
            Source::Narrative => {}
        }
        m.insert("why".into(), json!(why));
        Value::Object(m)
    }
}

/// One evidence block: the unit an author actually thinks in.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Where the value comes from. Decides the provenance sibling.
    pub source: Source,
    /// Why it has that status. 40+ characters (`card_contract::MIN_WHY`), and
    /// never generated — see the module docs.
    pub why: String,
    /// The block's fields, when it is an object. Mutually exclusive with
    /// [`Block::value`].
    #[serde(default)]
    pub fields: BTreeMap<String, FieldSpec>,
    /// The block's type, when it is a single value rather than an object.
    #[serde(default)]
    pub value: Option<String>,
    /// Whether the document must carry it. Defaults true; set false for a
    /// field only the platform sometimes adds, like `hud_field_scout`'s
    /// `_hud_review`.
    #[serde(default = "yes")]
    pub required: bool,
}

fn yes() -> bool {
    true
}

/// The authored form of a typed output contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sketch {
    /// Human-readable domain, e.g. `equity-research`.
    pub domain: String,
    /// Namespaced type name. Becomes `$id` and the single entry in `produces`.
    pub produces_schema: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// How a coordinator combines members' documents. Passed through.
    #[serde(default)]
    pub synthesis: Option<String>,
    /// How correctness is measured over time. Passed through verbatim: this
    /// module has no opinion about calibration and inventing one would be
    /// worse than carrying the author's.
    #[serde(default)]
    pub calibration: Option<Value>,
    pub blocks: Vec<Block>,
}

/// What the compiler produces: the card fragments, ready to paste.
#[derive(Debug, Clone)]
pub struct Compiled {
    /// Goes at `capabilities.output_contract`.
    pub output_contract: Value,
    /// The declared type, as one entry.
    ///
    /// **Not the card's `produces` on its own** — use
    /// [`Compiled::merge_produces`]. This used to be written straight over the
    /// card and that was a defect: `agents.produces` is also the port label
    /// set, so a recompile deleted labels other agents match on. Six of them
    /// on `football_analyst`, measured.
    pub produces: Vec<String>,
    /// Properties the compiler added that the author did not write. Returned
    /// so the expansion is inspectable rather than magic.
    pub generated_properties: Vec<String>,
}

impl Compiled {
    /// The card's `produces` after a compile: the declared type first, then
    /// everything the card already had.
    ///
    /// # The decision this settles
    ///
    /// `agents.produces` carries two things that were never separated: **the
    /// type this agent emits**, and **the labels it can be matched on**. The
    /// compiler replaced the whole column with the type, which deleted the
    /// labels; leaving the column alone means a typed agent never advertises
    /// its type. Recorded as a decision in
    /// `docs/plans/AGENT_COMPILE_AND_TOOL_REGISTRY.md` §6.8, which asked for
    /// "a rule for which labels are the contract's to remove".
    ///
    /// # The rule: none of them
    ///
    /// **A compile ADDS the declared type at the front and removes nothing.**
    ///
    /// The first version of this was cleverer and wrong. It treated any label
    /// containing `/` as a type reference the compiler owned, on the grounds
    /// that `card_contract` *enforces* a namespaced `produces_schema` while
    /// port nouns are conventionally bare — measured over the fleet as 314
    /// labels, 14 namespaced and every one its own card's declared type, 300
    /// bare, no exceptions.
    ///
    /// The test written to pin that measurement failed on the first run
    /// against a card committed while it was being written:
    /// `simops_companion` declares `kask_simops/action_block` AND
    /// `kask_simops/prose_response`. Two namespaced output types, both real —
    /// it answers with an action block or with prose. The clever rule would
    /// have silently deleted the second.
    ///
    /// So: additive. The cost is that a stale type name lingers after a
    /// `produces_schema` rename, which is a deliberate act whose leftover an
    /// author can delete by hand. The alternative cost was deleting a
    /// declared output type nobody was asked about. For a column that is also
    /// a match surface, "never loses a label" is the property worth having,
    /// and it makes the merge trivially idempotent.
    ///
    /// Whether `produces` should carry both meanings at all is still §6.8's
    /// question. This makes the compiler stop damaging it while that is
    /// decided.
    pub fn merge_produces(&self, existing: &[String]) -> Vec<String> {
        merge_produces(&self.produces, existing)
    }
}

/// [`Compiled::merge_produces`] as a free function, for callers holding the
/// compiler's JSON rather than a `Compiled` — the `/api/contracts/compile`
/// handler, which post-processes what `execute_build_tool` returned.
///
/// One implementation, three callers (the binary, the corpus test, the HTTP
/// endpoint). A second spelling of this rule is a second answer to "which
/// labels are the contract's to remove", which is the question §6.8 was open
/// on — and the whole reason it needed deciding once.
pub fn merge_produces(compiled: &[String], existing: &[String]) -> Vec<String> {
    let mut out: Vec<String> = compiled.to_vec();
    for label in existing {
        // Everything the card had, in the order it had it. Deduplicated only
        // so a card already naming its type does not name it twice.
        if !out.iter().any(|k| k == label) {
            out.push(label.clone());
        }
    }
    out
}

impl Sketch {
    pub fn from_json(v: &Value) -> Result<Self, Vec<Finding>> {
        serde_json::from_value(v.clone()).map_err(|e| {
            vec![f(
                "sketch_shape",
                format!(
                    "Could not read the sketch: {e}. Expected `domain`, \
                     `produces_schema` and `blocks`, where each block has `name`, \
                     `source` and `why`, plus either `fields` or `value`."
                ),
            )]
        })
    }

    /// Compile to a publishable `output_contract`.
    ///
    /// `tool_names` is the agent's declared `capabilities.mcp_tools`. It is
    /// required rather than optional because the load-bearing check —
    /// a `sourced` field naming a tool the agent cannot call — is a
    /// cross-reference, and a compiler that skipped it would emit contracts
    /// that fail at publish having looked fine at author time.
    ///
    /// On success the result is guaranteed to satisfy
    /// [`card_contract::validate`]. On failure every finding is returned, so
    /// an author fixes the sketch in one pass.
    pub fn compile(&self, tool_names: &[String]) -> Result<Compiled, Vec<Finding>> {
        let mut errs: Vec<Finding> = Vec::new();

        if self.blocks.is_empty() {
            errs.push(f(
                "sketch_shape",
                "The sketch declares no blocks, so it would compile to an empty \
                 schema — which is what `output_contract_typed` refuses. Name at \
                 least one block, and a `narrative` one for prose.",
            ));
        }

        let mut properties = Map::new();
        let mut grounding = Map::new();
        let mut required: Vec<Value> = Vec::new();
        let mut generated: Vec<String> = Vec::new();
        let mut seen: BTreeMap<&str, ()> = BTreeMap::new();

        for b in &self.blocks {
            let name = b.name.trim();

            // ── the block's own name ──────────────────────────────────
            if name.is_empty() {
                errs.push(f("sketch_block_name", "A block has an empty `name`."));
                continue;
            }
            if name.ends_with(PROVENANCE_SUFFIX) {
                errs.push(f(
                    "sketch_block_name",
                    format!(
                        "Block `{name}` ends in `{PROVENANCE_SUFFIX}`, which the \
                         compiler owns: it writes one sibling stamp per block and a \
                         hand-written twin would either collide or contradict it. \
                         Name the block for what it holds and let the stamp be \
                         derived."
                    ),
                ));
                continue;
            }
            if seen.insert(name, ()).is_some() {
                errs.push(f(
                    "sketch_block_name",
                    format!("Block `{name}` is declared twice."),
                ));
                continue;
            }

            // ── the block's shape ─────────────────────────────────────
            let has_fields = !b.fields.is_empty();
            let has_value = b.value.as_deref().is_some_and(|v| !v.trim().is_empty());
            let block_schema = match (has_fields, has_value) {
                (true, true) => {
                    errs.push(f(
                        "sketch_shape",
                        format!(
                            "Block `{name}` declares both `fields` and `value`. A block \
                             is either an object of fields or a single typed value; \
                             both at once has no JSON Schema."
                        ),
                    ));
                    continue;
                }
                (false, false) => {
                    errs.push(f(
                        "sketch_shape",
                        format!(
                            "Block `{name}` declares neither `fields` nor `value`, so \
                             nothing below it is typed. Give it `fields: {{ ... }}`, or \
                             `value: \"string\"` if it really is one scalar."
                        ),
                    ));
                    continue;
                }
                (false, true) => match TypeExpr::parse(b.value.as_deref().unwrap()) {
                    Ok(t) => t.to_schema(b.description.as_deref()),
                    Err(e) => {
                        errs.push(f("sketch_type_expr", format!("Block `{name}`: {e}")));
                        continue;
                    }
                },
                (true, false) => {
                    let mut props = Map::new();
                    let mut bad = false;
                    for (fname, spec) in &b.fields {
                        match TypeExpr::parse(spec.ty()) {
                            Ok(t) => {
                                props.insert(fname.clone(), t.to_schema(spec.description()));
                            }
                            Err(e) => {
                                errs.push(f(
                                    "sketch_type_expr",
                                    format!("Block `{name}`, field `{fname}`: {e}"),
                                ));
                                bad = true;
                            }
                        }
                    }
                    if bad {
                        continue;
                    }
                    let mut m = Map::new();
                    m.insert("type".into(), json!("object"));
                    // Closed on purpose, matching every typed card in the
                    // corpus: an open object lets a model add a field nobody
                    // classified, which is the shape `grounding` exists to
                    // stop, one level down where it is invisible.
                    m.insert("additionalProperties".into(), json!(false));
                    m.insert("properties".into(), Value::Object(props));
                    if let Some(d) = b.description.as_deref().filter(|d| !d.trim().is_empty()) {
                        m.insert("description".into(), json!(d));
                    }
                    Value::Object(m)
                }
            };

            // ── the author's why ─────────────────────────────────────
            if b.why.trim().len() < card_contract::MIN_WHY {
                errs.push(f(
                    "sketch_why",
                    format!(
                        "Block `{name}` has no usable `why` ({}+ characters needed). \
                         This is the one field the compiler will not write for you: \
                         its subject is where *your agent's* data comes from, and a \
                         generated justification for that is the fabrication this \
                         whole contract exists to catch.",
                        card_contract::MIN_WHY
                    ),
                ));
            }

            properties.insert(name.to_string(), block_schema);
            grounding.insert(name.to_string(), b.source.grounding_entry(b.why.trim()));
            if b.required {
                required.push(json!(name));
            }

            // ── the derived sibling ──────────────────────────────────
            //
            // An underscore-prefixed name is a platform annotation, not an
            // agent output — `hud_field_scout._hud_review` is the audit trail
            // `hud_contract::enforce` writes when it has had to correct a
            // response. A provenance stamp on one would be a retrieval verdict
            // about the platform's own note, which is the same category error
            // as stamping prose. It still needs a grounding entry, because the
            // bijection covers every top-level field; it does not need a stamp.
            //
            // Found by the decompiler: recompiling that card produced a
            // `_hud_review_provenance` the hand-written schema does not have.
            let platform_annotation = name.starts_with('_');
            if let Some(prov_schema) = b
                .source
                .provenance_schema()
                .filter(|_| !platform_annotation)
            {
                let sib = format!("{name}{PROVENANCE_SUFFIX}");
                properties.insert(sib.clone(), prov_schema);
                grounding.insert(sib.clone(), stamp_grounding_entry(name, &b.source));
                if b.required {
                    required.push(json!(sib.clone()));
                }
                generated.push(sib);
            }
        }

        if !errs.is_empty() {
            return Err(errs);
        }

        // ── assemble ─────────────────────────────────────────────────
        let mut schema = Map::new();
        schema.insert("$schema".into(), json!(DIALECT));
        schema.insert("$id".into(), json!(self.produces_schema.trim()));
        if let Some(t) = self.title.as_deref() {
            schema.insert("title".into(), json!(t));
        }
        if let Some(d) = self.description.as_deref() {
            schema.insert("description".into(), json!(d));
        }
        schema.insert("type".into(), json!("object"));
        schema.insert("additionalProperties".into(), json!(false));
        schema.insert("required".into(), Value::Array(required));
        schema.insert("properties".into(), Value::Object(properties));

        let mut oc = Map::new();
        oc.insert("domain".into(), json!(self.domain.trim()));
        oc.insert("produces_schema".into(), json!(self.produces_schema.trim()));
        if let Some(s) = self.synthesis.as_deref() {
            oc.insert("synthesis".into(), json!(s));
        }
        if let Some(c) = self.calibration.clone() {
            oc.insert("calibration".into(), c);
        }
        oc.insert("schema".into(), Value::Object(schema));
        oc.insert("grounding".into(), Value::Object(grounding));

        let output_contract = Value::Object(oc);
        let produces = vec![self.produces_schema.trim().to_string()];

        // ── the guarantee ────────────────────────────────────────────
        //
        // Check our own output against the gate we are compiling toward. A
        // compiler that emits an unpublishable contract has moved the
        // authoring cost rather than removed it, and the author would find
        // out at publish with no idea which part of the sketch to blame.
        let findings = card_contract::validate(Some(&output_contract), &produces, tool_names);
        if !findings.is_empty() {
            return Err(findings);
        }

        Ok(Compiled {
            output_contract,
            produces,
            generated_properties: generated,
        })
    }
}

/// The grounding entry for a derived `_provenance` sibling.
///
/// Generated, `why` included, and the module docs draw the line this sits on:
/// the subject is `grounding_trust::enforce` — platform behaviour the
/// compiler knows for certain — not the agent's data. Compare the eight
/// hand-written near-duplicates in `hud_field_scout`'s card, which say the
/// same thing eight times and are the reason this function exists.
fn stamp_grounding_entry(block: &str, source: &Source) -> Value {
    json!({
        // `inferred` rather than `derived` because the authoring vocabulary
        // has no `derived` token while the runtime's Grounding enum does; see
        // card_contract::PLATFORM_ASSIGNED_ONLY for why the gap is deliberate.
        // `inferred` understates a reproducible value, which is the safe
        // direction — the unsafe one would overstate a guess.
        "status": "inferred",
        "from": "src/grounding_trust.rs enforcement over this response",
        "why": format!(
            "Platform-written provenance stamp over the `{block}` block: {}. Written \
             by the platform, not by the model and not by a tool, so it is declared \
             `inferred`: card_contract::GROUNDING_STATUSES has no `derived` token \
             while the runtime's Grounding enum does, and understating a reproducible \
             value is the safe direction. Generated by contract_sketch, whose subject \
             here is platform behaviour rather than this agent's data.",
            source.verdict_prose(block)
        ),
    })
}

// ─── ontology binding ──────────────────────────────────────────────────

/// An agent's ontology, read as a field vocabulary.
///
/// The point is not automation, it is *selection over invention*. An author
/// naming `@sentiment` is choosing a concept the agent already reasons in,
/// with its closed value set and its definition attached; an author typing
/// `"enum:positive|negative"` from memory is minting a second, slightly
/// different vocabulary that nothing reconciles with the first.
///
/// Matches the shape in `ontologies/samples/*.json`: `entities[]`, each with
/// `id`, `name`, and `properties` holding `definition`, and optionally
/// `scale` or `categories`.
pub struct Ontology {
    entities: BTreeMap<String, Value>,
}

impl Ontology {
    pub fn from_json(v: &Value) -> Result<Self, String> {
        let arr = v
            .get("entities")
            .and_then(|e| e.as_array())
            .ok_or("ontology has no `entities` array")?;
        let mut entities = BTreeMap::new();
        for e in arr {
            if let Some(id) = e.get("id").and_then(|i| i.as_str()) {
                entities.insert(id.to_string(), e.clone());
            }
        }
        Ok(Ontology { entities })
    }

    /// Resolve `@id` to a type expression and a description.
    ///
    /// Three shapes are recognised, in order:
    ///
    /// - `properties.scale` of strings, or `properties.categories` → an
    ///   `enum`. The ontology already closed the set; re-closing it by hand
    ///   is how the two drift.
    /// - `properties.scale` of two numbers → `number`, **with the range in
    ///   the description**. Not `minimum`/`maximum`: `schema_validate`
    ///   implements neither, so emitting them would flip every document at
    ///   the delegation hop to `unverified_unsupported_schema` — declaring
    ///   more and thereby verifying less. The bound belongs where a reader
    ///   sees it until the validator can enforce it.
    /// - anything else → `string`, described.
    ///
    /// Returns `None` for an unknown id: a silent fallback to `string` would
    /// let a typo become a type.
    pub fn field(&self, id: &str) -> Option<(TypeExpr, Option<String>)> {
        let e = self.entities.get(id.trim_start_matches('@'))?;
        let props = e.get("properties");
        let definition = props
            .and_then(|p| p.get("definition"))
            .and_then(|d| d.as_str())
            .map(str::to_string);

        let as_strings = |key: &str| -> Option<Vec<String>> {
            let a = props?.get(key)?.as_array()?;
            let v: Vec<String> = a
                .iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect();
            (v.len() == a.len() && v.len() >= 2).then_some(v)
        };

        if let Some(vals) = as_strings("scale").or_else(|| as_strings("categories")) {
            return Some((
                TypeExpr {
                    base: Base::Enum(vals),
                    array: false,
                    nullable: false,
                },
                definition,
            ));
        }

        // A numeric scale, e.g. `"scale": [0.0, 1.0]`.
        if let Some(a) = props
            .and_then(|p| p.get("scale"))
            .and_then(|s| s.as_array())
        {
            let nums: Vec<f64> = a.iter().filter_map(|x| x.as_f64()).collect();
            if nums.len() == 2 && nums.len() == a.len() {
                let range = format!("Ontology scale {} to {}.", nums[0], nums[1]);
                let desc = match definition {
                    Some(d) => format!("{d} {range}"),
                    None => range,
                };
                return Some((
                    TypeExpr {
                        base: Base::Number,
                        array: false,
                        nullable: false,
                    },
                    Some(desc),
                ));
            }
        }

        Some((
            TypeExpr {
                base: Base::String,
                array: false,
                nullable: false,
            },
            definition,
        ))
    }

    /// Expand every `@id` in a sketch's field types against this ontology.
    ///
    /// Rewrites in place so the compiled schema carries real types and the
    /// sketch on disk stays short. Suffixes survive: `@sentiment?` resolves
    /// the base and keeps the nullable marker.
    pub fn expand(&self, sketch: &mut Sketch) -> Vec<Finding> {
        let mut errs = Vec::new();
        for b in &mut sketch.blocks {
            for (fname, spec) in b.fields.iter_mut() {
                let raw = spec.ty().to_string();
                if !raw.trim_start().starts_with('@') {
                    continue;
                }
                let trimmed = raw.trim();
                let (id, suffix) = split_suffix(trimmed);
                match self.field(id) {
                    Some((t, desc)) => {
                        let base_src = render_base(&t.base);
                        let ty = format!("{base_src}{suffix}");
                        let description = match spec.description() {
                            Some(d) if !d.trim().is_empty() => Some(d.to_string()),
                            _ => desc,
                        };
                        *spec = FieldSpec::Long { ty, description };
                    }
                    None => errs.push(f(
                        "sketch_ontology_ref",
                        format!(
                            "Block `{}`, field `{fname}`: `{trimmed}` names no entity in \
                             the ontology. Known ids: {}. Not defaulted to `string` on \
                             purpose — a silent fallback would let a typo become a type.",
                            b.name,
                            if self.entities.is_empty() {
                                "(none)".to_string()
                            } else {
                                self.entities.keys().cloned().collect::<Vec<_>>().join(", ")
                            }
                        ),
                    )),
                }
            }
        }
        errs
    }
}

/// Split `@id[]?` into (`id`, `[]?`).
fn split_suffix(s: &str) -> (&str, &str) {
    let cut = s.find(['[', '?']).unwrap_or(s.len());
    (&s[..cut], &s[cut..])
}

fn render_base(b: &Base) -> String {
    match b {
        Base::String => "string".into(),
        Base::Integer => "integer".into(),
        Base::Number => "number".into(),
        Base::Boolean => "boolean".into(),
        Base::Object => "object".into(),
        Base::Null => "null".into(),
        Base::Enum(v) => format!("enum:{}", v.join("|")),
        Base::Const(v) => format!("const:{v}"),
    }
}

// ─── decompiling an existing contract ──────────────────────────────────
//
// The compiler had no inverse, and that made it useful only for greenfield
// authoring. Three of the corpus's typed cards were hand-written before it
// existed, and one of them — `genome_profiler`, the agent whose fabricated
// genome sizes are the reason any of this exists — declares a schema and **no
// grounding map at all**, so it still fails the publish gate today.
//
// Without an inverse the only way to fix that card was to hand-write a sketch
// that reproduced a 250-line schema exactly, which nobody was going to do. So:
// read the contract back into a sketch, and let the author fix the part that
// is missing rather than retype the part that is not.
//
// ## What it recovers, and what it cannot
//
// Recovered: block names, field names, field types, the shape (object vs
// single value), the domain, the type name, synthesis and calibration. Those
// are all mechanically present in the schema.
//
// Recovered when a grounding map exists: status, tool, response_field, from,
// and the author's `why`.
//
// **Inferred from the provenance stamp when it does not**: a stamp admitting
// `tool_verified` means the block was sourced, `const model_inference` means
// inferred, and no stamp at all means narrative. That is a real signal — the
// stamp was narrowed by whoever wrote the schema — and it is why decompiling
// `genome_profiler` produces four correctly-classified blocks rather than four
// unknowns.
//
// **Never recovered: `why`.** If the contract has no grounding map, every
// block comes back with an empty `why` and the result deliberately does not
// compile. That is the correct outcome: the whys are the information the card
// never had, and inventing them here would be the fabrication this module's
// docs promise not to commit. The compiler's findings then name exactly what
// is missing, per block.

/// Render a JSON Schema leaf back into a [`TypeExpr`] source string.
///
/// The inverse of [`TypeExpr::to_schema`] over the forms that method emits.
/// Anything outside them returns `None` rather than a guess: a type expression
/// that does not round-trip would silently change the schema on recompile,
/// which is worse than telling the author this field needs a look.
pub fn render_type_expr(schema: &Value) -> Option<String> {
    let obj = schema.as_object()?;

    // enum — possibly with `null` folded in, which is how `to_schema` spells a
    // nullable enum.
    if let Some(vals) = obj.get("enum").and_then(|v| v.as_array()) {
        let nullable = vals.iter().any(|v| v.is_null());
        let names: Vec<String> = vals
            .iter()
            .filter(|v| !v.is_null())
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        if names.len() + usize::from(nullable) != vals.len() || names.len() < 2 {
            return None;
        }
        return Some(format!(
            "enum:{}{}",
            names.join("|"),
            if nullable { "?" } else { "" }
        ));
    }

    if let Some(c) = obj.get("const").and_then(|v| v.as_str()) {
        return Some(format!("const:{c}"));
    }

    let ty = obj.get("type")?;
    // A field pinned to null, before the nullable-union handling below strips
    // "null" as a modifier. `{"type": "null"}` is a type, not a modifier.
    if ty.as_str() == Some("null") {
        return Some("null".to_string());
    }
    let (base, nullable) = match ty {
        Value::String(s) => (s.clone(), false),
        Value::Array(a) => {
            let nullable = a.iter().any(|v| v.as_str() == Some("null"));
            let concrete: Vec<&str> = a
                .iter()
                .filter_map(|v| v.as_str())
                .filter(|s| *s != "null")
                .collect();
            if concrete.len() != 1 {
                return None;
            }
            (concrete[0].to_string(), nullable)
        }
        _ => return None,
    };

    if base == "array" {
        // The item type, which `to_schema` always writes.
        let items = obj.get("items")?;
        let inner = render_type_expr(items)?;
        // An array of nullables is not something `to_schema` can emit, so an
        // inner `?` means this schema was not produced by this compiler.
        if inner.ends_with('?') {
            return None;
        }
        return Some(format!("{inner}[]{}", if nullable { "?" } else { "" }));
    }

    if !["string", "integer", "number", "boolean", "object"].contains(&base.as_str()) {
        return None;
    }
    Some(format!("{base}{}", if nullable { "?" } else { "" }))
}

/// Read an existing `output_contract` back into an editable [`Sketch`].
///
/// The returned sketch is not guaranteed to compile — and for a card with no
/// grounding map it is guaranteed not to. That is the point: the findings are
/// the to-do list.
pub fn sketch_from_contract(oc: &Value) -> Result<Sketch, Vec<Finding>> {
    let schema = oc.get("schema").filter(|s| s.is_object()).ok_or_else(|| {
        vec![f(
            "decompile_no_schema",
            "This contract has no inline `schema`, so there is nothing to read              back. `produces_schema` is a name; a name cannot be decompiled              into a document shape.",
        )]
    })?;
    let props = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .ok_or_else(|| {
            vec![f(
                "decompile_no_schema",
                "The declared schema has no `properties`.",
            )]
        })?;

    let grounding = oc.get("grounding").and_then(|g| g.as_object());
    let required: Vec<&str> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();

    let mut blocks = Vec::new();
    let mut errs = Vec::new();

    for (name, sub) in props {
        // The compiler owns the sibling stamps; they are re-derived, never
        // authored, so reading them back as blocks would duplicate them on
        // recompile.
        if name.ends_with(PROVENANCE_SUFFIX) {
            continue;
        }

        let stamp = props.get(&format!("{name}{PROVENANCE_SUFFIX}"));
        let g = grounding.and_then(|m| m.get(name));

        let source = match g {
            Some(entry) => {
                let mut src = source_from_grounding(entry);
                // `coverage` is a sketch-level concept: it exists to decide
                // how wide the stamp's enum is, and the card records the
                // RESULT rather than the input. So it has to be recovered
                // from the stamp, not from the grounding entry.
                //
                // Missing this silently NARROWED enums on recompile — a
                // `partial` block came back `complete` and lost
                // `unavailable_no_tool_source`, which is the verdict that says
                // "the tool answered and this field has no source". The
                // corpus round-trip test caught it on `macro_data_agent`.
                if let Source::Sourced {
                    ref mut coverage, ..
                } = src
                {
                    if let Source::Sourced { coverage: c, .. } = source_from_stamp(stamp) {
                        *coverage = c;
                    }
                }
                src
            }
            // No grounding map. Classify from the stamp the schema author
            // narrowed, which is real evidence about the block's kind even
            // though it says nothing about which tool supplied it.
            None => source_from_stamp(stamp),
        };

        let why = g
            .and_then(|e| e.get("why"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut block = Block {
            name: name.clone(),
            description: sub
                .get("description")
                .and_then(|d| d.as_str())
                .map(str::to_string),
            source,
            why,
            fields: BTreeMap::new(),
            value: None,
            required: required.contains(&name.as_str()),
        };

        match sub.get("properties").and_then(|p| p.as_object()) {
            Some(fields) => {
                for (fname, fschema) in fields {
                    match render_type_expr(fschema) {
                        Some(ty) => {
                            let desc = fschema
                                .get("description")
                                .and_then(|d| d.as_str())
                                .map(str::to_string);
                            block.fields.insert(
                                fname.clone(),
                                match desc {
                                    Some(d) => FieldSpec::Long {
                                        ty,
                                        description: Some(d),
                                    },
                                    None => FieldSpec::Short(ty),
                                },
                            );
                        }
                        None => errs.push(f(
                            "decompile_unreadable_type",
                            format!(
                                "`{name}.{fname}` uses a schema shape this                                  compiler cannot express, so it cannot be                                  round-tripped without changing it: {fschema}.                                  Retype it by hand and check the diff."
                            ),
                        )),
                    }
                }
            }
            None => match render_type_expr(sub) {
                Some(ty) => block.value = Some(ty),
                None => errs.push(f(
                    "decompile_unreadable_type",
                    format!(
                        "Block `{name}` is a bare value whose schema this                          compiler cannot express: {sub}"
                    ),
                )),
            },
        }

        blocks.push(block);
    }

    if !errs.is_empty() {
        return Err(errs);
    }

    // Restore the author's block order where the schema recorded one.
    // `serde_json::Map` is a BTreeMap so `properties` came back alphabetical,
    // but `required` is an array and preserves the order the compiler wrote.
    if !required.is_empty() {
        blocks.sort_by_key(|b| {
            required
                .iter()
                .position(|r| *r == b.name)
                .unwrap_or(usize::MAX)
        });
    }

    Ok(Sketch {
        domain: oc
            .get("domain")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        produces_schema: oc
            .get("produces_schema")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        title: schema
            .get("title")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        description: schema
            .get("description")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        synthesis: oc
            .get("synthesis")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        calibration: oc.get("calibration").cloned(),
        blocks,
    })
}

/// Rebuild a [`Source`] from an existing grounding entry.
fn source_from_grounding(entry: &Value) -> Source {
    let get = |k: &str| {
        entry
            .get(k)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };
    match entry.get("status").and_then(|v| v.as_str()) {
        Some("sourced") => Source::Sourced {
            tool: get("tool"),
            response_field: get("response_field"),
            // A placeholder. The card does not record coverage, so the
            // caller overwrites this from the provenance stamp — see
            // `sketch_from_contract`. Left as the narrowest value so that a
            // caller which forgets to overwrite produces a visible schema
            // diff rather than a silently widened enum.
            coverage: Coverage::Complete,
        },
        Some("inferred") => Source::Inferred { from: get("from") },
        Some("unavailable") => Source::Unavailable {
            would_need: entry
                .get("would_need")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        },
        // `narrative`, and anything unrecognised. An unknown status must not
        // become `sourced`, which is the only status that makes a retrieval
        // claim.
        _ => Source::Narrative,
    }
}

/// Classify a block from its provenance stamp, for a contract with no
/// grounding map.
///
/// The stamp is evidence: whoever wrote the schema narrowed it deliberately.
/// It cannot say *which* tool supplied a sourced block, so `tool` comes back
/// empty and the compiler will refuse until the author names one — correctly,
/// since that is the check with teeth.
fn source_from_stamp(stamp: Option<&Value>) -> Source {
    let Some(stamp) = stamp else {
        return Source::Narrative;
    };

    if let Some(c) = stamp.get("const").and_then(|v| v.as_str()) {
        return match c {
            PROV_INFERRED => Source::Inferred {
                from: String::new(),
            },
            PROV_UNAVAILABLE => Source::Unavailable { would_need: None },
            // `platform_derived` has no authoring token; `inferred`
            // understates a reproducible value, which is the safe direction.
            _ => Source::Inferred {
                from: String::new(),
            },
        };
    }

    if let Some(vals) = stamp.get("enum").and_then(|v| v.as_array()) {
        let has = |t: &str| vals.iter().any(|v| v.as_str() == Some(t));
        if has(PROV_TOOL) {
            return Source::Sourced {
                tool: String::new(),
                response_field: String::new(),
                // Both before either. Testing `unavailable` first and
                // returning `Partial` is what silently narrowed
                // `macro_data_agent` on recompile, and a four-verdict stamp
                // read back as three would do it again — this time dropping
                // `pending_tool_check` from a live `football_analyst`.
                coverage: match (has(PROV_UNAVAILABLE), has(PROV_PENDING_TOOL)) {
                    (true, true) => Coverage::PartialDeferred,
                    (true, false) => Coverage::Partial,
                    (false, true) => Coverage::Deferred,
                    (false, false) => Coverage::Complete,
                },
            };
        }
        if has(PROV_UNAVAILABLE) {
            return Source::Unavailable { would_need: None };
        }
    }

    Source::Narrative
}

// ─── MCP tool ──────────────────────────────────────────────────────────

/// MCP tool body for `build_output_contract`.
///
/// Sits next to the compiler for the same reason
/// `card_contract::execute_validate_tool` sits next to the rules: an agent
/// working from a *description* of the expansion in its system prompt would
/// drift from the expansion, and confidently produce contracts that do not
/// compile. Calling [`Sketch::compile`] means the advice and the compiler are
/// the same code.
///
/// This is the division of labour the ontology agent makes possible. A model
/// is good at the part that needs judgement — which blocks, which fields,
/// which vocabulary, and the `why` — and is exactly the wrong thing to trust
/// with a bijection over fourteen keys. So it writes a sketch, and Rust
/// writes the contract. Note that the model *cannot* fabricate a `sourced`
/// claim through this path: `compile` cross-checks every tool name against
/// `tool_names` and refuses.
pub fn execute_build_tool(input: &Value) -> Result<String, String> {
    let sketch_val = input
        .get("sketch")
        .filter(|v| v.is_object())
        .ok_or("`sketch` is required and must be an object")?;

    let tool_names: Vec<String> = input
        .get("tool_names")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    let mut sketch = match Sketch::from_json(sketch_val) {
        Ok(s) => s,
        Err(findings) => return Ok(render_findings(&findings)),
    };

    // Optional ontology, expanded before compilation so `@refs` become types.
    if let Some(ont) = input.get("ontology").filter(|v| v.is_object()) {
        match Ontology::from_json(ont) {
            Ok(o) => {
                let errs = o.expand(&mut sketch);
                if !errs.is_empty() {
                    return Ok(render_findings(&errs));
                }
            }
            Err(e) => {
                return Ok(render_findings(&[f(
                    "sketch_ontology_ref",
                    format!("Could not read `ontology`: {e}"),
                )]))
            }
        }
    }

    match sketch.compile(&tool_names) {
        Ok(c) => serde_json::to_string_pretty(&json!({
            "compiles": true,
            "would_publish": true,
            "output_contract": c.output_contract,
            "produces": c.produces,
            "generated_properties": c.generated_properties,
            "note": "Paste `output_contract` at `capabilities.output_contract` and \
                     replace `produces` wholesale. This has been checked against \
                     card_contract::validate — the Admission gate accepts it. Keep the \
                     sketch beside the card and assert the two agree in a test, so the \
                     card cannot drift away from the declaration that produced it.",
            "guide": "docs/guides/AGENT_CONTRACT_AUTHORING.md",
        }))
        .map_err(|e| e.to_string()),
        Err(findings) => Ok(render_findings(&findings)),
    }
}

fn render_findings(findings: &[Finding]) -> String {
    serde_json::to_string_pretty(&json!({
        "compiles": false,
        "would_publish": false,
        "findings": findings
            .iter()
            .map(|x| json!({ "check": x.check, "fix": x.message }))
            .collect::<Vec<_>>(),
        "note": "Nothing was emitted. The compiler does not return a partial contract: \
                 a contract that is almost complete reads exactly like one that is, and \
                 the gap would be found at publish with no clue which part of the \
                 sketch caused it.",
        "guide": "docs/guides/AGENT_CONTRACT_AUTHORING.md",
    }))
    .unwrap_or_else(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tools() -> Vec<String> {
        vec!["fmp_ratios".to_string(), "fmp_company_profile".to_string()]
    }

    fn minimal() -> Sketch {
        Sketch {
            domain: "testing".into(),
            produces_schema: "test/doc".into(),
            title: Some("Doc".into()),
            description: None,
            synthesis: None,
            calibration: None,
            blocks: vec![
                Block {
                    name: "ratios".into(),
                    description: None,
                    source: Source::Sourced {
                        tool: "fmp_ratios".into(),
                        response_field: "peRatio, debtEquityRatio".into(),
                        coverage: Coverage::Complete,
                    },
                    why: "FMP returns these ratios pre-computed for the queried symbol, \
                          so the block is a real retrieval."
                        .into(),
                    fields: [("pe".to_string(), FieldSpec::Short("number?".into()))]
                        .into_iter()
                        .collect(),
                    value: None,
                    required: true,
                },
                Block {
                    name: "summary".into(),
                    description: None,
                    source: Source::Narrative,
                    why: "Prose over what was retrieved; must not assert anything the \
                          sourced blocks cannot support."
                        .into(),
                    fields: BTreeMap::new(),
                    value: Some("string".into()),
                    required: true,
                },
            ],
        }
    }

    // ── the type mini-language ────────────────────────────────────────

    #[test]
    fn a_plain_scalar_is_a_plain_type() {
        let t = TypeExpr::parse("string").unwrap();
        assert_eq!(t.to_schema(None), json!({ "type": "string" }));
    }

    #[test]
    fn nullable_is_a_union_not_an_omission() {
        // The corpus convention: a field that may be absent is typed
        // ["integer","null"] and stays REQUIRED, so the model must say
        // "nothing" explicitly rather than quietly dropping the key.
        let t = TypeExpr::parse("integer?").unwrap();
        assert_eq!(t.to_schema(None), json!({ "type": ["integer", "null"] }));
    }

    #[test]
    fn a_nullable_array_is_the_array_that_is_nullable() {
        let t = TypeExpr::parse("string[]?").unwrap();
        assert_eq!(
            t.to_schema(None),
            json!({ "type": ["array", "null"], "items": { "type": "string" } })
        );
    }

    #[test]
    fn the_suffix_order_is_fixed_so_both_types_stay_writable() {
        // `?[]` is refused rather than treated as a synonym for `[]?`. If
        // both spellings meant "nullable array", "array of nullables" would
        // have no spelling at all.
        let e = TypeExpr::parse("string?[]").unwrap_err();
        assert!(e.contains("nullable marker last"), "{e}");
    }

    #[test]
    fn a_nullable_enum_admits_null_into_the_set() {
        let t = TypeExpr::parse("enum:up|down?").unwrap();
        assert_eq!(t.to_schema(None), json!({ "enum": ["up", "down", null] }));
    }

    #[test]
    fn a_one_value_enum_is_refused_in_favour_of_const() {
        let e = TypeExpr::parse("enum:only").unwrap_err();
        assert!(e.contains("at least two values"), "{e}");
    }

    #[test]
    fn an_unsupported_keyword_is_not_offered_at_all() {
        // The trap this closes: `{"minimum": 0}` looks like a tightening and
        // is a loosening, because schema_validate cannot evaluate it and
        // reports the whole document `unverified_unsupported_schema` — which
        // is not a pass. So there is no way to write it.
        let e = TypeExpr::parse("number(min=0)").unwrap_err();
        assert!(e.contains("unverified_unsupported_schema"), "{e}");
    }

    #[test]
    fn every_emitted_keyword_is_one_the_validator_implements() {
        // The invariant behind the previous test, checked over the whole
        // mini-language rather than asserted in prose.
        const SUPPORTED: &[&str] = &[
            "type",
            "properties",
            "required",
            "additionalProperties",
            "enum",
            "const",
            "items",
            "description",
            "$schema",
            "$id",
            "title",
        ];
        fn walk(v: &Value, path: &str) {
            if let Value::Object(m) = v {
                for (k, sub) in m {
                    assert!(
                        SUPPORTED.contains(&k.as_str()),
                        "{path}.{k} is a keyword schema_validate does not implement, \
                         which would make every document unverified"
                    );
                    if k == "properties" {
                        if let Value::Object(props) = sub {
                            for (name, s) in props {
                                walk(s, &format!("{path}.{name}"));
                            }
                        }
                    } else if k == "items" {
                        walk(sub, &format!("{path}[]"));
                    }
                }
            }
        }
        let c = minimal().compile(&tools()).unwrap();
        walk(c.output_contract.get("schema").unwrap(), "schema");
    }

    // ── the expansion ─────────────────────────────────────────────────

    #[test]
    fn a_sourced_block_gets_a_narrowed_provenance_sibling() {
        let c = minimal().compile(&tools()).unwrap();
        let props = c.output_contract.pointer("/schema/properties").unwrap();
        assert_eq!(
            props.get("ratios_provenance").unwrap(),
            &json!({ "enum": ["tool_verified", "tool_no_match"] }),
            "a tool with complete coverage cannot honestly say \
             unavailable_no_tool_source"
        );
    }

    #[test]
    fn partial_coverage_widens_the_enum_and_deferred_widens_it_differently() {
        let mut s = minimal();
        if let Source::Sourced { coverage, .. } = &mut s.blocks[0].source {
            *coverage = Coverage::Partial;
        }
        let c = s.compile(&tools()).unwrap();
        assert_eq!(
            c.output_contract
                .pointer("/schema/properties/ratios_provenance")
                .unwrap(),
            &json!({ "enum": ["tool_verified", "tool_no_match", "unavailable_no_tool_source"] })
        );

        let mut s = minimal();
        if let Source::Sourced { coverage, .. } = &mut s.blocks[0].source {
            *coverage = Coverage::Deferred;
        }
        let c = s.compile(&tools()).unwrap();
        assert_eq!(
            c.output_contract
                .pointer("/schema/properties/ratios_provenance")
                .unwrap(),
            &json!({ "enum": ["tool_verified", "tool_no_match", "pending_tool_check"] }),
            "`not asked yet` and `nothing exists` are different facts and must not \
             collapse into one enum"
        );
    }

    #[test]
    fn a_narrative_block_gets_no_stamp() {
        // grounding_trust is explicit: a block that is only ever prose must
        // not carry a provenance key, because a retrieval verdict about a
        // sentence is a category error.
        let c = minimal().compile(&tools()).unwrap();
        let props = c
            .output_contract
            .pointer("/schema/properties")
            .unwrap()
            .as_object()
            .unwrap();
        assert!(props.contains_key("summary"));
        assert!(!props.contains_key("summary_provenance"));
    }

    #[test]
    fn nullable_blocks_stay_required_so_absence_must_be_stated() {
        let c = minimal().compile(&tools()).unwrap();
        let req = c.output_contract.pointer("/schema/required").unwrap();
        assert_eq!(
            req,
            &json!(["ratios", "ratios_provenance", "summary"]),
            "required follows block order, so the diff of a schema reads in the \
             order the author thinks in"
        );
    }

    #[test]
    fn the_generated_properties_are_reported_rather_than_slipped_in() {
        let c = minimal().compile(&tools()).unwrap();
        assert_eq!(c.generated_properties, vec!["ratios_provenance"]);
    }

    #[test]
    fn produces_is_rewritten_to_the_declared_type() {
        let c = minimal().compile(&tools()).unwrap();
        assert_eq!(c.produces, vec!["test/doc"]);
    }

    // ── the guarantee ─────────────────────────────────────────────────

    #[test]
    fn contract_compiles_to_something_the_gate_accepts() {
        // The load-bearing property. If this ever fails, the compiler has
        // started moving the authoring cost instead of removing it.
        let c = minimal().compile(&tools()).unwrap();
        let findings = card_contract::validate(Some(&c.output_contract), &c.produces, &tools());
        assert!(findings.is_empty(), "{findings:#?}");
    }

    #[test]
    fn the_grounding_map_is_a_bijection_by_construction() {
        let c = minimal().compile(&tools()).unwrap();
        let mut props: Vec<&String> = c
            .output_contract
            .pointer("/schema/properties")
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .collect();
        let mut ground: Vec<&String> = c
            .output_contract
            .get("grounding")
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .collect();
        props.sort();
        ground.sort();
        assert_eq!(props, ground);
    }

    #[test]
    fn a_sourced_claim_against_a_tool_the_agent_lacks_is_refused() {
        // The check with teeth, reached through the compiler: an author
        // cannot get a plausible-looking contract out of this by naming a
        // tool that does not exist.
        let mut s = minimal();
        if let Source::Sourced { tool, .. } = &mut s.blocks[0].source {
            *tool = "fmp_imaginary".into();
        }
        let errs = s.compile(&tools()).unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.check == "grounding_sourced_names_tool"),
            "{errs:#?}"
        );
    }

    #[test]
    fn a_short_why_is_refused_and_never_filled_in() {
        let mut s = minimal();
        s.blocks[0].why = "because".into();
        let errs = s.compile(&tools()).unwrap_err();
        let e = errs.iter().find(|e| e.check == "sketch_why").unwrap();
        assert!(
            e.message.contains("will not write for you"),
            "the refusal must say why it is a refusal and not a default: {}",
            e.message
        );
    }

    #[test]
    fn the_provenance_namespace_belongs_to_the_compiler() {
        let mut s = minimal();
        s.blocks[0].name = "ratios_provenance".into();
        let errs = s.compile(&tools()).unwrap_err();
        assert!(
            errs.iter().any(|e| e.check == "sketch_block_name"),
            "{errs:#?}"
        );
    }

    #[test]
    fn a_block_with_no_shape_is_refused() {
        let mut s = minimal();
        s.blocks[0].fields.clear();
        s.blocks[0].value = None;
        let errs = s.compile(&tools()).unwrap_err();
        assert!(errs.iter().any(|e| e.check == "sketch_shape"), "{errs:#?}");
    }

    #[test]
    fn nested_objects_are_closed_too() {
        // An open nested object is the same defect one level down, where
        // `grounding`'s top-level bijection cannot see it.
        let c = minimal().compile(&tools()).unwrap();
        assert_eq!(
            c.output_contract
                .pointer("/schema/properties/ratios/additionalProperties")
                .unwrap(),
            &json!(false)
        );
    }

    #[test]
    fn every_finding_is_returned_not_just_the_first() {
        let mut s = minimal();
        s.blocks[0].why = "no".into();
        s.blocks[1].why = "no".into();
        let errs = s.compile(&tools()).unwrap_err();
        assert!(errs.len() >= 2, "{errs:#?}");
    }

    #[test]
    fn a_failed_compile_emits_no_partial_contract() {
        let mut s = minimal();
        s.blocks[0].why = "no".into();
        let out = execute_build_tool(&json!({
            "sketch": serde_json::to_value(&s).unwrap(),
            "tool_names": tools(),
        }))
        .unwrap();
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["compiles"], json!(false));
        assert!(
            v.get("output_contract").is_none(),
            "a partial contract reads exactly like a complete one"
        );
    }

    // ── ontology binding ──────────────────────────────────────────────

    #[test]
    fn an_ontology_entity_supplies_the_closed_set_and_the_definition() {
        let ont = Ontology::from_json(&json!({
            "entities": [{
                "id": "sentiment",
                "properties": {
                    "definition": "Emotional tone expressed in text",
                    "scale": ["negative", "neutral", "positive"]
                }
            }]
        }))
        .unwrap();
        let (t, desc) = ont.field("@sentiment").unwrap();
        assert_eq!(
            t.base,
            Base::Enum(vec!["negative".into(), "neutral".into(), "positive".into()])
        );
        assert_eq!(desc.as_deref(), Some("Emotional tone expressed in text"));
    }

    #[test]
    fn a_numeric_scale_lands_in_the_description_not_in_minimum() {
        // Emitting `minimum` would declare more and verify less: the whole
        // document would go `unverified_unsupported_schema` at the hop.
        let ont = Ontology::from_json(&json!({
            "entities": [{
                "id": "intensity",
                "properties": { "definition": "Strength of sentiment.", "scale": [0.0, 1.0] }
            }]
        }))
        .unwrap();
        let (t, desc) = ont.field("@intensity").unwrap();
        assert_eq!(t.base, Base::Number);
        let d = desc.unwrap();
        assert!(d.contains("0 to 1"), "{d}");
    }

    #[test]
    fn an_unknown_entity_is_an_error_not_a_string() {
        let ont = Ontology::from_json(&json!({
            "entities": [{ "id": "sentiment", "properties": {} }]
        }))
        .unwrap();
        let mut s = minimal();
        s.blocks[0]
            .fields
            .insert("mood".into(), FieldSpec::Short("@typo".into()));
        let errs = ont.expand(&mut s);
        assert!(
            errs.iter().any(|e| e.check == "sketch_ontology_ref"),
            "{errs:#?}"
        );
    }

    #[test]
    fn an_ontology_ref_keeps_its_suffixes() {
        let ont = Ontology::from_json(&json!({
            "entities": [{
                "id": "emotion",
                "properties": { "categories": ["joy", "anger", "fear"] }
            }]
        }))
        .unwrap();
        let mut s = minimal();
        s.blocks[0]
            .fields
            .insert("emotions".into(), FieldSpec::Short("@emotion[]?".into()));
        assert!(ont.expand(&mut s).is_empty());
        let c = s.compile(&tools()).unwrap();
        assert_eq!(
            c.output_contract
                .pointer("/schema/properties/ratios/properties/emotions")
                .unwrap(),
            &json!({
                "type": ["array", "null"],
                "items": { "enum": ["joy", "anger", "fear"] }
            })
        );
    }

    /// Against a real ontology on disk, so the binding is not a story told
    /// with a fixture. `ontologies/samples/sentiment_analyzer_ontology.json`
    /// is one of the two ontologies the repo actually carries.
    #[test]
    fn the_real_sentiment_ontology_resolves_to_types() {
        let path = "ontologies/samples/sentiment_analyzer_ontology.json";
        let raw = match std::fs::read_to_string(path) {
            Ok(r) => r,
            // Cargo runs unit tests from the crate root, but do not fail a
            // build over a sample file's location: an absent sample is not
            // evidence about the code.
            Err(_) => return,
        };
        let ont = Ontology::from_json(&serde_json::from_str(&raw).unwrap()).unwrap();

        // A five-point scale becomes a closed set, taken from the ontology
        // rather than retyped from memory into a second, subtly different one.
        let (t, desc) = ont.field("@sentiment").expect("sentiment entity");
        assert_eq!(
            t.base,
            Base::Enum(vec![
                "very_negative".into(),
                "negative".into(),
                "neutral".into(),
                "positive".into(),
                "very_positive".into()
            ])
        );
        assert!(desc.unwrap().to_lowercase().contains("tone"));

        // Eight emotion categories, likewise.
        let (t, _) = ont.field("@emotion").expect("emotion entity");
        match t.base {
            Base::Enum(ref v) => assert_eq!(v.len(), 8),
            ref other => panic!("expected an enum, got {other:?}"),
        }

        // `"scale": [0.0, 1.0]` is a numeric range: a number, with the bound
        // in the description because the validator cannot enforce `minimum`.
        let (t, desc) = ont.field("@intensity").expect("intensity entity");
        assert_eq!(t.base, Base::Number);
        assert!(desc.unwrap().contains("0 to 1"));
    }

    // ── decompiling ───────────────────────────────────────────────────

    /// **The property that makes the decompiler trustworthy.** For every card
    /// with a complete contract, decompile → compile reproduces the contract
    /// byte for byte.
    ///
    /// Without this the inverse is a convenience that quietly rewrites
    /// schemas: an author opens a card to fix one `why`, saves, and three
    /// unrelated fields have changed type. Run over the real corpus rather
    /// than a fixture, so a card authored tomorrow is covered too.
    #[test]
    fn decompiling_then_compiling_reproduces_the_contract() {
        let mut checked = 0;
        for dir in std::fs::read_dir("agents/curated").expect("curated dir") {
            let dir = dir.expect("entry").path();
            let card_path = dir.join("agent_card.json");
            if !card_path.exists() {
                continue;
            }
            let card: Value =
                serde_json::from_str(&std::fs::read_to_string(&card_path).unwrap()).unwrap();
            let Some(oc) = card.pointer("/capabilities/output_contract") else {
                continue;
            };
            // Only cards that already satisfy the gate can round-trip: one
            // with no grounding map decompiles to blocks with no `why`, which
            // is the intended behaviour and tested separately below.
            let produces: Vec<String> = card
                .get("produces")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            let tools: Vec<String> = card
                .pointer("/capabilities/mcp_tools")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if !card_contract::validate(Some(oc), &produces, &tools).is_empty() {
                continue;
            }
            // Only contracts this compiler PRODUCED are required to round-trip
            // exactly, and the marker for that is a sketch beside the card.
            // Hand-written contracts can and do use shapes the mini-language
            // cannot express; `hud_field_scout` is the worked example and the
            // three specific gaps are documented in
            // `a_hand_written_contract_may_exceed_what_the_compiler_can_express`.
            // Asserting exactness over those would either fail for ever or
            // pressure someone into widening the mini-language to match one
            // card, which is the wrong direction.
            if !dir.join("output_contract.sketch.json").exists() {
                continue;
            }

            let name = dir.file_name().unwrap().to_string_lossy().to_string();
            let sketch = sketch_from_contract(oc)
                .unwrap_or_else(|e| panic!("{name}: does not decompile:\n{e:#?}"));
            let recompiled = sketch
                .compile(&tools)
                .unwrap_or_else(|e| panic!("{name}: decompiled sketch does not compile:\n{e:#?}"));

            assert_eq!(
                &recompiled.output_contract, oc,
                "{name}: decompile -> compile changed the contract. An author \
                 opening this card to fix one field would silently rewrite \
                 others."
            );
            checked += 1;
        }
        assert!(
            checked >= 5,
            "only round-tripped {checked} card(s) — the corpus walk is \
             probably broken, which would make this test vacuously pass"
        );
    }

    /// The §6.8 rule: a compile advertises the type and keeps the labels.
    ///
    /// `Compiled.produces` used to be written straight over the card's, and
    /// `agents.produces` is not the type — it is also the port label set the
    /// seam census matches on. Recompiling `football_analyst` deleted six
    /// labels, one of which (`evidence`) is named by eight other cards.
    #[test]
    fn compiling_advertises_the_type_and_keeps_the_authors_port_labels() {
        let c = Compiled {
            output_contract: json!({}),
            produces: vec!["fermi/football_evidence".into()],
            generated_properties: vec![],
        };

        let card = [
            "fermi/football_evidence",
            "evidence",
            "win-probability",
            "elo-analysis",
            "match-prediction",
            "form-analysis",
            "league-analysis",
        ]
        .map(String::from);

        assert_eq!(
            c.merge_produces(&card),
            card.to_vec(),
            "the real `football_analyst` card must survive a compile unchanged. \
             This is the case that was measured as losing six labels."
        );

        // The type is added when the card never had it, and goes first so the
        // agent's primary identity is unambiguous. `condition_forecaster`:
        // three labels, none of them its declared type.
        let cf = Compiled {
            output_contract: json!({}),
            produces: vec!["kask_wild/condition_forecast".into()],
            generated_properties: vec![],
        };
        assert_eq!(
            cf.merge_produces(
                &[
                    "condition_forecast",
                    "species_probability",
                    "brier_forecast"
                ]
                .map(String::from)
            ),
            vec![
                "kask_wild/condition_forecast",
                "condition_forecast",
                "species_probability",
                "brier_forecast"
            ],
            "a card whose labels do not include its type must GAIN the type and \
             keep all three, not be replaced by it"
        );

        // A SECOND namespaced type survives. `simops_companion` declares
        // `kask_simops/action_block` and `kask_simops/prose_response` -- it
        // answers with an action block or with prose, and both are real. An
        // earlier version of this rule treated every namespaced label as the
        // compiler's to remove and would have deleted the second one.
        let sc = Compiled {
            output_contract: json!({}),
            produces: vec!["kask_simops/action_block".into()],
            generated_properties: vec![],
        };
        assert_eq!(
            sc.merge_produces(
                &["kask_simops/action_block", "kask_simops/prose_response"].map(String::from)
            ),
            vec!["kask_simops/action_block", "kask_simops/prose_response"],
            "a second declared output type must survive a compile"
        );

        // Idempotent, or a second compile churns the card and the corpus test
        // that compares them oscillates.
        let once = c.merge_produces(&card);
        assert_eq!(
            c.merge_produces(&once),
            once,
            "merge_produces is not idempotent"
        );

        // A card with nothing to preserve gets exactly the type.
        assert_eq!(c.merge_produces(&[]), vec!["fermi/football_evidence"]);
    }

    /// A compile must never shrink a card's `produces`.
    ///
    /// The property, over the whole corpus, in the direction that matters:
    /// every label a card declares today is still there after a recompile.
    /// `agents.produces` is a match surface -- `panel_absence` counts 289
    /// distinct labels across the fleet -- so a compiler that drops one
    /// unbinds a belt nobody was asked about. Six labels on
    /// `football_analyst`, measured, which is what opened section 6.8.
    ///
    /// This replaced a test asserting that no card declares a namespaced
    /// label other than its own type. That premise was measured true over 314
    /// labels and was false within the day: `simops_companion` declares two
    /// namespaced types on purpose. The rule became additive instead, and
    /// this is the assertion that survives being wrong about the corpus.
    #[test]
    fn a_recompile_never_drops_a_produces_label() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("agents/curated");
        let mut checked = 0usize;
        let mut labels = 0usize;

        for e in std::fs::read_dir(&root)
            .expect("read agents/curated")
            .flatten()
        {
            let p = e.path().join("agent_card.json");
            let Ok(raw) = std::fs::read_to_string(&p) else {
                continue;
            };
            let Ok(card) = serde_json::from_str::<Value>(&raw) else {
                continue;
            };
            let Some(declared) = card
                .pointer("/capabilities/output_contract/produces_schema")
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            let produces: Vec<String> = card
                .get("produces")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            let c = Compiled {
                output_contract: json!({}),
                produces: vec![declared.to_string()],
                generated_properties: vec![],
            };
            let merged = c.merge_produces(&produces);
            let name = e.file_name().to_string_lossy().into_owned();

            for l in &produces {
                labels += 1;
                assert!(
                    merged.contains(l),
                    "{name}: recompiling would drop the `produces` label `{l}`. That \
                     column is a match surface, so dropping one unbinds a belt silently."
                );
            }
            assert!(
                merged.contains(&declared.to_string()),
                "{name}: the declared type `{declared}` is missing from the merged \
                 `produces`, so a typed agent would not advertise its own type."
            );
            assert_eq!(
                c.merge_produces(&merged),
                merged,
                "{name}: merge_produces is not idempotent, so two compiles churn the card"
            );
            checked += 1;
        }

        assert!(
            checked >= 10 && labels >= 10,
            "only checked {checked} typed card(s) / {labels} label(s); the corpus walk \
             is broken and this test would pass by finding nothing"
        );
    }

    /// A four-verdict stamp must survive a round trip.
    ///
    /// `Coverage::Partial` and `Coverage::Deferred` each drop one verdict the
    /// other keeps, so before `PartialDeferred` existed a block declaring all
    /// four could only be read back as three. The `if has(UNAVAILABLE)` branch
    /// tested first, so it always came back `Partial` and `pending_tool_check`
    /// vanished on recompile — the same silent narrowing already caught once
    /// on `macro_data_agent`, and this time it would have hit a live
    /// `football_analyst` where "never asked" is the common case.
    #[test]
    fn a_block_that_is_both_partial_and_deferred_survives_a_round_trip() {
        let all_four = json!({
            "enum": [PROV_TOOL, PROV_NO_MATCH, PROV_UNAVAILABLE, PROV_PENDING_TOOL]
        });

        let src = source_from_stamp(Some(&all_four));
        let Source::Sourced { coverage, .. } = src else {
            panic!("a stamp naming `{PROV_TOOL}` must read back as sourced, got {src:?}");
        };
        assert_eq!(
            coverage,
            Coverage::PartialDeferred,
            "a stamp admitting both `{PROV_UNAVAILABLE}` and `{PROV_PENDING_TOOL}` \
             read back as {coverage:?}, which emits only three verdicts. The \
             missing one disappears on the next recompile and nothing says so."
        );

        // And back out again, identically. This is the direction that actually
        // rewrites the card.
        let emitted = Source::Sourced {
            tool: "call_football_api".into(),
            response_field: "fixtures/statistics.expected_goals".into(),
            coverage: Coverage::PartialDeferred,
        }
        .provenance_schema()
        .expect("a sourced block has a stamp");
        assert_eq!(
            emitted, all_four,
            "compiling `partial_deferred` did not reproduce the four-verdict stamp"
        );
    }

    /// Every coverage token the guidance prompt is required to explain must
    /// actually parse. `Coverage::TOKENS` is read by
    /// `tests/xaman_ek_contract_guidance.rs`; a token in that list that the
    /// compiler rejects would have the assistant teaching authors a setting
    /// that fails to compile.
    #[test]
    fn every_advertised_coverage_token_parses() {
        for t in Coverage::TOKENS {
            let parsed: Coverage = serde_json::from_value(json!(t)).unwrap_or_else(|e| {
                panic!("`coverage: {t}` is advertised but does not parse: {e}")
            });
            assert!(
                Source::Sourced {
                    tool: "t".into(),
                    response_field: "f".into(),
                    coverage: parsed,
                }
                .provenance_schema()
                .is_some(),
                "`coverage: {t}` parsed but emits no provenance stamp"
            );
        }
    }

    /// `genome_profiler` is the reason this whole line of work exists, and it
    /// is now the worked example: the round trip is a **fixpoint**.
    ///
    /// This test used to assert the opposite. It read
    /// `oc.get("grounding").is_none()` and checked that the card refused to
    /// compile, naming four missing `why`s — and it carried an instruction to
    /// rewrite rather than delete it if a grounding map ever appeared. One has.
    ///
    /// ## What was wrong, and why no surface could see it
    ///
    /// The field contract lived in **two homes**. `FIELD_CONTRACTS` in
    /// `grounding_trust.rs` held fifteen entries for this agent; the card held
    /// a schema and no `grounding` block at all. Both were real and each was
    /// authoritative for a different reader:
    ///
    /// * `declaration_ladder::has_field_contract` checks **both** paths, so the
    ///   shelf's ladder showed the `field_contract` rung declared;
    /// * `ContractBuilder` decompiles the **card**, found no grounding, and
    ///   produced five blocks with an empty `why` — which by design does not
    ///   compile, so the agent could not be saved from its own editor.
    ///
    /// The shelf read one home and the editor edited the other, and neither was
    /// lying. Every symptom followed from that.
    ///
    /// The Rust table stays. It carries per-**field** granularity and the SQL
    /// cross-checks, which the card's per-**block** vocabulary cannot express
    /// (`DESIGN_a2a_contracting.md` §7.6). The two now agree instead of
    /// substituting for one another, which is what
    /// [`no_declared_field_was_pruned_from_genome_profiler`] holds true.
    #[test]
    fn genome_profiler_round_trips_through_its_own_editor() {
        let path = "agents/curated/genome_profiler/agent_card.json";
        let Ok(raw) = std::fs::read_to_string(path) else {
            return;
        };
        let card: Value = serde_json::from_str(&raw).unwrap();
        let oc = card
            .pointer("/capabilities/output_contract")
            .expect("genome_profiler declares a contract");
        assert!(
            oc.get("grounding").is_some(),
            "the card's grounding map is what unblocks the editor. If it has \
             been removed, the shelf can render this agent and cannot save it."
        );

        let sketch = sketch_from_contract(oc).expect("decompiles");

        // (a) the shape came back, in the author's order
        let names: Vec<&str> = sketch.blocks.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["taxonomy", "genome", "phylogeny", "conservation", "summary"],
            "block order should follow `required`, not the alphabet"
        );

        // (b) every block now carries a recovered `why`. This is the one that
        // was empty, and emptiness here is what blocked the save.
        for b in &sketch.blocks {
            assert!(
                b.why.trim().len() >= card_contract::MIN_WHY,
                "block `{}` decompiled with no usable `why`, so the draft in \
                 the editor cannot recompile and the agent cannot be saved",
                b.name
            );
        }

        // The genome block's stamp admits `unavailable_no_tool_source`, so its
        // coverage is partial — precisely the honest reading of the original
        // bug: the tool answered and the field had no source. And unlike
        // before, the grounding map can now say WHICH tool.
        let genome = sketch.blocks.iter().find(|b| b.name == "genome").unwrap();
        match &genome.source {
            Source::Sourced { coverage, tool, .. } => {
                assert_eq!(*coverage, Coverage::Partial);
                assert_eq!(
                    tool, "ncbi_genome_search",
                    "a stamp alone cannot say which tool supplied a block; the \
                     grounding map is the only place that fact can live"
                );
            }
            other => panic!("expected sourced, got {other:?}"),
        }

        // (c) and the round trip is a FIXPOINT. Decompile, recompile, and the
        // contract is byte-identical — which is the property the shelf's save
        // button depends on and the property this card did not have.
        let tools: Vec<String> = card
            .pointer("/capabilities/mcp_tools")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let recompiled = sketch
            .compile(&tools)
            .unwrap_or_else(|errs| panic!("the card no longer recompiles:\n{errs:#?}"));
        assert_eq!(
            &recompiled.output_contract, oc,
            "decompile -> recompile must be a fixpoint. It is not, so opening \
             the contract editor and pressing save would silently change the \
             contract."
        );
    }

    /// The migration must not have bought its green tick by deleting ambition.
    ///
    /// Seven of `genome_profiler`'s fifteen fields are `Unsourced`, and that is
    /// the deliberate content of the contract rather than a backlog: each one
    /// is a standing request for an integration, and pruning them would make
    /// the card claim the agent was never trying to report them.
    ///
    /// So: every path the Rust table declares must still resolve against the
    /// card's schema. `FIELD_CONTRACTS` is the per-field home and this is the
    /// join between the two homes, asserted rather than assumed.
    #[test]
    fn no_declared_field_was_pruned_from_genome_profiler() {
        let path = "agents/curated/genome_profiler/agent_card.json";
        let Ok(raw) = std::fs::read_to_string(path) else {
            return;
        };
        let card: Value = serde_json::from_str(&raw).unwrap();
        let props = card
            .pointer("/capabilities/output_contract/schema/properties")
            .expect("a typed card has schema properties");

        let mut declared = 0usize;
        let mut unsourced = 0usize;
        let mut missing: Vec<&str> = Vec::new();
        for fc in crate::grounding_trust::contracts_for("genome_profiler") {
            declared += 1;
            if fc.grounding == crate::grounding_trust::Grounding::Unsourced {
                unsourced += 1;
            }
            // `a.b` in the Rust table is `/a/properties/b` in the schema.
            let ptr = format!("/{}", fc.path.replace('.', "/properties/"));
            if props.pointer(&ptr).is_none() {
                missing.push(fc.path);
            }
        }

        assert!(
            missing.is_empty(),
            "the card's schema no longer carries {missing:?}. A field the Rust \
             table declares and the schema does not is unenforceable: `enforce` \
             will null a path the document has no place for."
        );
        assert_eq!(
            declared, 15,
            "the two homes disagree on how many fields exist"
        );
        assert_eq!(
            declared, 15,
            "the two homes disagree on how many fields exist"
        );
        assert_eq!(
            unsourced, 7,
            "seven fields have no source and that is the contract's content. If \
             this number FELL, check that a tool was really wired up rather \
             than a field quietly deleted to reach a green tick."
        );
    }

    /// **The card's grounding map must not be mistaken for a replacement.**
    ///
    /// This is the hazard the migration arms, and it is this codebase's
    /// characteristic bug: *a writer that replaces a composite it only partly
    /// owns.* The card's vocabulary is per-**block**; `FIELD_CONTRACTS` is
    /// per-**field**. `genome_profiler` needs both, because four of its blocks
    /// are mixed — `genome` is `sourced` as a block while `genome.ploidy` and
    /// `genome.notable_genes` have no source at all.
    ///
    /// Measured against the in-flight `enforce_from_output_contract`, which
    /// prefers the card's map and falls back to `FIELD_CONTRACTS`: routed down
    /// the block path, this document keeps `ploidy: "diploid"`,
    /// `notable_genes: [...]`, `divergence_mya: 45.0` and
    /// `defining_traits: "scaled wings"` — four recalled values in retrieved
    /// blocks, which is the original defect exactly. It also nulls the whole
    /// `conservation` block to `null`, which the card's own schema forbids
    /// (`conservation` is a required object), and skips `NARRATIVE_LEAKS`
    /// entirely so the summary keeps its megabases and its Red List status.
    ///
    /// So this test pins what the field path does. If somebody deletes
    /// `genome_profiler` from `FIELD_CONTRACTS` on the reasonable-sounding
    /// grounds that "the card declares it now", seven protections disappear
    /// silently and this goes red instead.
    #[test]
    fn the_field_level_contract_still_nulls_every_unsourced_value() {
        let mut doc = json!({
            "taxonomy": { "order": "Lepidoptera", "species": "Danaus plexippus" },
            "genome": {
                "estimated_size_mb": 245, "chromosome_count": 30,
                "notable_genes": ["cyp6b"], "ploidy": "diploid",
                "assembly_name": "Dplex_v4", "assembly_accession": "GCF_009731565.1"
            },
            "phylogeny": {
                "sister_taxa": ["Danaus gilippus"], "superorder": null,
                "divergence_mya": 45.0, "defining_traits": "scaled wings"
            },
            "conservation": {
                "iucn_status": "Not Evaluated", "population_trend": "stable",
                "genetic_diversity_notes": "high"
            },
            "summary": "A monarch, placed in Nymphalidae beside Danaus gilippus."
        });
        crate::grounding_trust::enforce("genome_profiler", &mut doc);

        for path in [
            "/genome/notable_genes",
            "/genome/ploidy",
            "/phylogeny/divergence_mya",
            "/phylogeny/defining_traits",
            "/conservation/iucn_status",
            "/conservation/population_trend",
            "/conservation/genetic_diversity_notes",
        ] {
            assert_eq!(
                doc.pointer(path),
                Some(&Value::Null),
                "`{path}` survived enforcement. Every one of these is a value \
                 no tool can supply, and a plausible value in a retrieved block \
                 is indistinguishable from a measured one."
            );
        }

        // The four that ARE sourced must survive, or the contract is just a
        // filter and nobody will keep it.
        assert_eq!(doc.pointer("/genome/estimated_size_mb"), Some(&json!(245)));
        assert_eq!(doc.pointer("/genome/chromosome_count"), Some(&json!(30)));
        assert_eq!(
            doc.pointer("/phylogeny/sister_taxa"),
            Some(&json!(["Danaus gilippus"]))
        );
        // And `conservation` stays an OBJECT of nulls. The schema requires an
        // object; a bare `null` here would fail the agent's own validation.
        assert!(
            doc.pointer("/conservation").is_some_and(Value::is_object),
            "`conservation` must stay an object of nulls, not become null"
        );
    }

    /// The one value the migration DID change, and why that is a correction.
    ///
    /// `genome_profiler`'s `phylogeny_provenance` declared
    /// `[tool_verified, tool_no_match, platform_derived]`. The third is
    /// unreachable: `grounding_trust::enforce` only stamps `platform_derived`
    /// when `block_is_sourced` returns `None` — that is, when the block has no
    /// `Sourced` field at all — and `phylogeny.sister_taxa` is `Sourced`. So
    /// the card was declaring a verdict its own runtime could never write,
    /// which is the `gbif_verified` drift in a new place.
    ///
    /// `no_card_declares_a_provenance_value_the_runtime_cannot_emit` does not
    /// catch this, and correctly so: it checks membership in the global
    /// `PROVENANCE_VALUES` set, and `platform_derived` is a real value — just
    /// not a reachable one *for this block*. Reachability is per-block and
    /// needs the runtime run against it, which is what this does.
    ///
    /// Partial coverage replaces it with `unavailable_no_tool_source`, which
    /// the runtime does emit here (the pre-contract path) and which is the
    /// honest reading of `divergence_mya` and `defining_traits`.
    #[test]
    fn phylogeny_can_never_be_stamped_platform_derived() {
        // Both branches of `block_is_sourced`: the tool returned siblings, and
        // the tool was asked and returned none.
        for sisters in [json!(["Danaus gilippus"]), json!([])] {
            let mut doc = json!({
                "taxonomy": { "order": "Lepidoptera" },
                "genome": {},
                "phylogeny": { "sister_taxa": sisters, "superorder": null },
                "conservation": {},
                "summary": "GBIF places it in Nymphalidae."
            });
            crate::grounding_trust::enforce("genome_profiler", &mut doc);
            let stamp = doc
                .get("phylogeny_provenance")
                .and_then(|v| v.as_str())
                .expect("a block with a sourced field is always stamped");
            assert_ne!(
                stamp, "platform_derived",
                "if the runtime can now emit this here, the schema should \
                 declare it again and `Coverage` needs a variant that can"
            );
        }

        // And the schema admits exactly what the runtime can write.
        let path = "agents/curated/genome_profiler/agent_card.json";
        let Ok(raw) = std::fs::read_to_string(path) else {
            return;
        };
        let card: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            card.pointer(
                "/capabilities/output_contract/schema/properties/phylogeny_provenance/enum"
            ),
            Some(&json!([PROV_TOOL, PROV_NO_MATCH, PROV_UNAVAILABLE])),
        );
    }

    /// A hand-written contract may exceed what the compiler can express, and
    /// the decompiler must say so rather than quietly approximating.
    ///
    /// `hud_field_scout` is the case. Three distinct gaps, all found by
    /// attempting the round-trip and all worth having written down:
    ///
    /// 1. **`platform_derived` stamps.** Its schema declares
    ///    `const: "platform_derived"` while its grounding entries say
    ///    `inferred`, because `card_contract::GROUNDING_STATUSES` has no
    ///    `derived` token and the runtime's `Grounding` enum does. The card
    ///    documents that gap in its own `why` text. The compiler therefore
    ///    emits `model_inference` where the card says `platform_derived`.
    /// 2. **Nested arrays of objects.** `card.lines` is an array whose items
    ///    have their own `properties` and `required`. `object[]` in the
    ///    mini-language emits `items: {"type": "object"}` and drops the inner
    ///    shape.
    /// 3. ~~A stamp on a platform annotation~~ — fixed: `_`-prefixed blocks
    ///    now get no sibling.
    ///
    /// The honest response to 1 and 2 is not to widen the mini-language to fit
    /// one card. It is to let the decompiler recover the *shape* — which is
    /// what saves an author from retyping 250 lines — and to be explicit that
    /// a recompile would change these fields, so nobody saves over them by
    /// accident.
    #[test]
    fn a_hand_written_contract_may_exceed_what_the_compiler_can_express() {
        let path = "agents/curated/hud_field_scout/agent_card.json";
        let Ok(raw) = std::fs::read_to_string(path) else {
            return;
        };
        let card: Value = serde_json::from_str(&raw).unwrap();
        let oc = card.pointer("/capabilities/output_contract").unwrap();

        // It decompiles: the shape comes back, which is the useful part.
        let sketch = sketch_from_contract(oc).expect("the shape is recoverable");
        assert!(
            sketch.blocks.len() >= 6,
            "the blocks should be recovered even though the schema is richer \
             than the mini-language"
        );

        // And the round-trip is NOT exact, for the two reasons above. Asserted
        // so that if someone widens the mini-language the test tells them this
        // note is now stale rather than leaving it to rot.
        let tools: Vec<String> = card
            .pointer("/capabilities/mcp_tools")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let recompiled = sketch.compile(&tools).expect("it still compiles");
        assert_ne!(
            recompiled.output_contract.pointer("/schema/properties"),
            oc.pointer("/schema/properties"),
            "the round-trip is now exact for hud_field_scout. If the \
             mini-language gained `platform_derived` and nested array items, \
             delete this test and let the corpus round-trip test cover the \
             card instead."
        );

        // The specific divergences, named. A future reader should not have to
        // rediscover which fields these are.
        let now = recompiled
            .output_contract
            .pointer("/schema/properties")
            .unwrap();
        assert_eq!(
            now.pointer("/capture_provenance/const")
                .and_then(|v| v.as_str()),
            Some(PROV_INFERRED),
            "gap 1: the compiler cannot emit `platform_derived`"
        );
        assert!(
            now.pointer("/card/properties/lines/items/properties")
                .is_none(),
            "gap 2: nested array item shape is dropped"
        );
        // Gap 3 is fixed, and this is what fixed looks like.
        assert!(
            now.get("_hud_review_provenance").is_none(),
            "a platform annotation must not acquire a provenance stamp"
        );
    }

    /// No `why` is ever invented while decompiling, including for blocks whose
    /// kind the stamp identifies confidently. The stamp says what kind of
    /// value it is; only an author can say why.
    #[test]
    fn decompiling_never_invents_a_why() {
        let oc = json!({
            "domain": "d",
            "produces_schema": "x/y",
            "schema": {
                "type": "object",
                "properties": {
                    "a": { "type": "object", "properties": { "n": { "type": "number" } } },
                    "a_provenance": { "enum": ["tool_verified", "tool_no_match"] },
                    "summary": { "type": "string" }
                },
                "required": ["a", "a_provenance", "summary"]
            }
        });
        let sketch = sketch_from_contract(&oc).unwrap();
        assert!(
            sketch.blocks.iter().all(|b| b.why.is_empty()),
            "a contract with no grounding map has no whys to recover, and \
             generating one would be the fabrication this module refuses"
        );
    }

    #[test]
    fn a_type_expression_survives_the_round_trip() {
        for src in [
            "string",
            "integer?",
            "number[]",
            "string[]?",
            "boolean",
            "enum:up|down|flat",
            "enum:a|b?",
            "const:model_inference",
            "object?",
        ] {
            let schema = TypeExpr::parse(src).unwrap().to_schema(None);
            assert_eq!(
                render_type_expr(&schema).as_deref(),
                Some(src),
                "`{src}` did not survive to_schema -> render_type_expr"
            );
        }
    }

    /// A schema shape the compiler cannot express must be REFUSED rather than
    /// approximated. Approximating it would change the schema on the next save
    /// while looking like a no-op edit.
    #[test]
    fn an_inexpressible_type_is_refused_not_approximated() {
        // `minimum` is not the problem here — an unrepresentable *type* is.
        assert_eq!(
            render_type_expr(&json!({ "type": ["string", "integer"] })),
            None
        );
        assert_eq!(render_type_expr(&json!({ "enum": ["only"] })), None);
        assert_eq!(render_type_expr(&json!({})), None);
    }

    #[test]
    fn the_sketch_round_trips_through_json() {
        let v = serde_json::to_value(minimal()).unwrap();
        let back = Sketch::from_json(&v).unwrap();
        let a = back.compile(&tools()).unwrap();
        let b = minimal().compile(&tools()).unwrap();
        assert_eq!(a.output_contract, b.output_contract);
    }
}
