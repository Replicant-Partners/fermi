//! # Grounding trust contract — the fields that have no source
//!
//! Third sibling to [`crate::schema_trust`] and [`crate::rollup_trust`]:
//!
//! | Contract | Question |
//! | --- | --- |
//! | `schema_trust` | Is the column **present**? |
//! | `rollup_trust` | Is the column **telling the truth**? |
//! | `grounding_trust` | **Could this value have come from anywhere?** |
//!
//! ## The failure class
//!
//! `genome_profiler`'s system prompt asks for four blocks — `taxonomy`,
//! `genome`, `phylogeny`, `conservation` — and the agent has exactly two
//! tools, both GBIF, both returning taxonomy. Three of the four blocks have
//! no possible source. The model filled them anyway, confidently, and 56
//! episodes and 13 cached profiles shipped with values like
//! `estimated_size_mb: "200-400"` for a scale insect nobody has sequenced.
//!
//! Every existing check passed:
//!
//! | Guard | Why it missed this |
//! | --- | --- |
//! | `agent_contract` | Requires `accepts`/`produces` to be non-empty. They were. |
//! | `agent_card::tests` | Structural conformance: description, tags, tools-as-objects. All present. |
//! | `parse_agent_json` | Parses. The document parsed perfectly. |
//! | `cache_is_valid` (`agent_modules.rs`) | Requires non-empty `taxonomy`. Taxonomy was real — it is the one block with a tool. |
//! | Type checking | `String` is `String` whether it means "Not Evaluated" or an invention. |
//!
//! The prompt even carried a guard — *"Do not invent taxonomy — if GBIF
//! returns no match, say so"* — which protects the **only block that has a
//! tool**. A correct guard, aimed at the one place it was not needed.
//!
//! ## Why the narrative is checked too
//!
//! Nulling `genome.estimated_size_mb` is not sufficient. The `summary` field
//! is prose written by the same model in the same turn, and it restates the
//! numbers: the fixture at `tool_executor.rs` reads *"occupies Holometabola
//! with a ~480 Mb genome typical for Lepidoptera"*. `parse_evidence_text`
//! then lifts that summary out as the episode's `evidence`, so the sentence a
//! user reads would still carry the fabrication after every structured field
//! was cleared.
//!
//! So [`Grounding::Narrative`] fields are scanned for claims that exceed
//! what the sourced blocks can support. A prose channel that is not checked
//! is the channel the fabrication moves to.
//!
//! ## What this module is not
//!
//! It does not fetch anything. Making these fields *available* is a separate
//! integration task (NCBI, TimeTree, IUCN). This module's whole job is to
//! ensure that until such a tool exists, the gap reads as a gap.

use serde_json::Value;

// ─── provenance vocabulary ─────────────────────────────────────────────

/// A tool was called and returned content for this block.
///
/// Deliberately not named for a specific tool. The first draft used
/// `gbif_verified`, which stopped being true the moment a second agent
/// (`enemy_sensor`, sourcing from `scan_nearby_creatures`) was brought under
/// the contract. Which tool did it is recorded once, in
/// [`FIELD_CONTRACTS`], rather than smuggled into a status string.
pub const PROV_TOOL: &str = "tool_verified";
/// The block's tool was consulted and had nothing for this subject.
/// Materially different from "no tool exists".
pub const PROV_NO_MATCH: &str = "tool_no_match";
/// No tool in this agent's kit can supply this block.
pub const PROV_UNAVAILABLE: &str = "unavailable_no_tool_source";
/// The block is a judgement the agent was asked to make by reasoning over
/// sourced inputs. Legitimate output, explicitly labelled as reasoning
/// rather than retrieval.
pub const PROV_INFERRED: &str = "model_inference";
/// Computed by platform code from a sourced value, deterministically.
///
/// Distinct from [`PROV_INFERRED`] in the way that matters: a derivation is
/// reproducible and auditable — the same input yields the same output, and
/// the transform can be read. A model inference is neither. Collapsing them
/// would lose exactly the property that makes a derived value trustworthy.
pub const PROV_DERIVED: &str = "platform_derived";

/// Every value `<block>_provenance` is permitted to take.
///
/// A closed set, asserted by [`tests::provenance_values_are_closed`]. An
/// open one would let a future edit invent `"estimated"`, which is the
/// fabrication reappearing as a metadata value.
pub const PROVENANCE_VALUES: &[&str] = &[PROV_TOOL, PROV_NO_MATCH, PROV_UNAVAILABLE, PROV_INFERRED];

// ─── contract ──────────────────────────────────────────────────────────

/// Where a field's value can legitimately come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grounding {
    /// A named tool's response supplies this field.
    Sourced {
        tool: &'static str,
        response_field: &'static str,
    },
    /// No available tool returns data for this field. The model could only
    /// produce it from parametric knowledge, so it must be `null`.
    Unsourced,
    /// A judgement the agent is *asked* to make by reasoning over sourced
    /// inputs. Kept, and labelled `model_inference`.
    ///
    /// This variant exists because the second and third agents brought under
    /// the contract nearly broke it. `enemy_sensor` is asked to rate
    /// predation risk from taxonomy and proximity; `prey_locator` is asked
    /// for an intercept strategy. Those are inferences, and they are the
    /// entire product. Treating them like `genome.estimated_size_mb` would
    /// null the agents' only output and prove that the contract cannot tell
    /// an agent that fabricates from one that reasons — at which point it
    /// would rightly be switched off.
    ///
    /// The distinction is retrieval versus judgement. A genome size is a
    /// fact sitting in a database the agent did not query. A threat level is
    /// not in any database; producing it is the job.
    Inferred {
        /// What the judgement is reasoned from, for the report.
        from: &'static str,
    },
    /// Computed by platform code from a sourced field, deterministically.
    ///
    /// `phylogeny.superorder` is the motivating case: it is not a GBIF rank
    /// and no tool returns it, but it follows from `taxonomy.order` through a
    /// closed ~30-entry table (Lepidoptera -> Holometabola). While the model
    /// supplied it from memory it was `Unsourced`; once platform code applies
    /// the table it is reproducible, and that is a different kind of claim.
    Derived {
        /// The sourced field it is computed from.
        from: &'static str,
        /// The transform, named so a reader can check it.
        how: &'static str,
    },
    /// Free prose. Permitted, but must not assert anything the sourced
    /// blocks cannot support — see [`NARRATIVE_LEAKS`].
    Narrative,
}

/// One output field and what may legitimately fill it.
#[derive(Debug, Clone, Copy)]
pub struct FieldContract {
    pub agent_id: &'static str,
    /// Dotted path into the agent's output document, e.g.
    /// `genome.estimated_size_mb`. A leading segment names a top-level
    /// block, which is what gets a `_provenance` sibling.
    pub path: &'static str,
    pub grounding: Grounding,
    /// Why, in enough detail that the next person does not have to
    /// re-derive it from the tool list.
    pub why: &'static str,
}

/// How a leak needle is matched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeakRule {
    /// A distinctive word. Plain substring match is safe.
    Word(&'static str),
    /// A unit that only implies a claim when a number precedes it.
    ///
    /// This variant exists because the first version of this table used a
    /// plain `" gb"` needle, which matches **"GBIF"** — so an honest
    /// taxonomy-only summary citing its own source was reported as leaking
    /// a genome size. A check that fires on correct output is worse than no
    /// check: it gets switched off, and the switching-off looks like
    /// cleanup.
    Quantity(&'static str),
}

/// Patterns in a [`Grounding::Narrative`] field that assert something only
/// an unsourced block could support, paired with the block that would have
/// to be sourced for the claim to be legitimate.
///
/// Deliberately narrow, and matched against a lowercased haystack.
pub const NARRATIVE_LEAKS: &[(&str, LeakRule)] = &[
    ("genome", LeakRule::Quantity("mb")),
    ("genome", LeakRule::Quantity("gb")),
    ("genome", LeakRule::Quantity("kb")),
    ("genome", LeakRule::Word("megabase")),
    ("genome", LeakRule::Word("gigabase")),
    ("genome", LeakRule::Word("chromosom")),
    ("genome", LeakRule::Word("karyotype")),
    ("genome", LeakRule::Word("diploid")),
    ("genome", LeakRule::Word("haploid")),
    ("genome", LeakRule::Word("2n=")),
    ("genome", LeakRule::Word("2n =")),
    ("phylogeny", LeakRule::Quantity("mya")),
    ("phylogeny", LeakRule::Word("million years")),
    ("phylogeny", LeakRule::Word("diverged")),
    ("phylogeny", LeakRule::Word("divergence")),
    ("conservation", LeakRule::Word("iucn")),
    ("conservation", LeakRule::Word("least concern")),
    ("conservation", LeakRule::Word("endangered")),
    ("conservation", LeakRule::Word("vulnerable")),
    ("conservation", LeakRule::Word("red list")),
];

impl LeakRule {
    /// Does this rule fire against an already-lowercased haystack?
    pub fn matches(&self, haystack: &str) -> bool {
        match self {
            LeakRule::Word(w) => haystack.contains(w),
            LeakRule::Quantity(unit) => {
                let bytes = haystack.as_bytes();
                let mut from = 0usize;
                while let Some(rel) = haystack[from..].find(unit) {
                    let at = from + rel;
                    // Walk back over the separators a writer puts between a
                    // number and its unit: "480 Mb", "420-480Mb", "~90 mya".
                    let mut i = at;
                    while i > 0 && matches!(bytes[i - 1], b' ' | b'-' | b'~' | 0xE2) {
                        i -= 1;
                    }
                    if i > 0 && bytes[i - 1].is_ascii_digit() {
                        return true;
                    }
                    from = at + unit.len();
                }
                false
            }
        }
    }
}

/// Every field we have an opinion about.
///
/// Seeded with `genome_profiler` — the agent the class was found in. Extend
/// per agent as each is remediated; `port_census.py` reports which agents
/// have output fields and no entry here.
pub const FIELD_CONTRACTS: &[FieldContract] = &[
    // ── genome_profiler ────────────────────────────────────────────
    // Tools: gbif_species_search, gbif_taxonomy_tree. Both taxonomy.
    FieldContract {
        agent_id: "genome_profiler",
        path: "taxonomy",
        grounding: Grounding::Sourced {
            tool: "gbif_taxonomy_tree",
            response_field: "hierarchy (kingdom..species)",
        },
        why: "The one block with a real tool. GBIF returns the full rank \
              ladder with keys.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "phylogeny.sister_taxa",
        grounding: Grounding::Sourced {
            tool: "gbif_taxonomy_tree",
            response_field: "sibling taxa at each rank",
        },
        why: "The tool's own description promises sibling taxa for \
              phylogenetic context, so this is retrievable today at \
              genus/family rank. Notably the ONE phylogeny field that is \
              real — stripping it along with its neighbours would be a \
              check that overreaches, and an overreaching check gets \
              switched off.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "genome.estimated_size_mb",
        grounding: Grounding::Sourced {
            tool: "ncbi_genome_search",
            response_field: "estimated_size_mb (assembly total_length)",
        },
        why: "Was Unsourced; now answerable. NCBI Assembly reports \
              `total_length` and the tool names which assembly supplied it. \
              Coverage is ~2 of 6 for the species actually in \
              creature_conditions, so `tool_no_match` stays the common \
              outcome — a fact about the world, not a gap in the platform. \
              The value this replaces was not merely unsourced but wrong: the \
              prompt asserted Lepidoptera ~400-500Mb; the monarch is 245Mb.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "genome.chromosome_count",
        grounding: Grounding::Sourced {
            tool: "ncbi_genome_search",
            response_field: "assembled_chromosome_count",
        },
        why: "Was Unsourced on the belief that karyotype data has no API. \
              Partly wrong: NCBI reports `chromosome_count` in assembly \
              metadata. It counts assembled chromosome-level replicons, not a \
              cytological karyotype — for Danaus plexippus it returns 30, \
              matching published n=30, and that agreement is not a licence to \
              relabel it. Same coverage as genome size.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "genome.notable_genes",
        grounding: Grounding::Unsourced,
        why: "Species-level gene-family claims have no tool behind them.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "genome.ploidy",
        grounding: Grounding::Unsourced,
        why: "Stays unsourced even though NCBI is now wired up, and this is \
              the instructive one. NCBI returns `assemblytype: haploid` for \
              the monarch, which it is very tempting to map here — and it \
              would be FALSE: that field describes how the ASSEMBLY \
              represents the genome, not the organism. A monarch is diploid. \
              A plausible, convenient, wrong mapping is exactly the class \
              this contract exists to stop, so the tool returns ploidy as \
              null with that reason attached.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "phylogeny.superorder",
        grounding: Grounding::Derived {
            from: "taxonomy.order",
            how: "ncbi_tools::superorder_of — a closed table over the ~30 insect orders",
        },
        why: "The table got written, so this moved from Unsourced to Derived. \
              No tool returns a superorder and none needs to: it is a \
              deterministic function of an order GBIF does return. The \
              distinction from Inferred matters — a derivation is reproducible \
              and its table can be checked row by row, which is not true of \
              the recall it replaces. Unknown orders return None rather than \
              something plausible.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "phylogeny.divergence_mya",
        grounding: Grounding::Unsourced,
        why: "Needs a dated phylogeny (TimeTree). Coverage is decent at \
              order/family and sparse at species, so null stays the common \
              answer even once wired.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "phylogeny.defining_traits",
        grounding: Grounding::Unsourced,
        why: "Order-level trait narration from parametric knowledge.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "conservation.iucn_status",
        grounding: Grounding::Unsourced,
        why: "The most dangerous of the set. \"Not Evaluated\" is a REAL IUCN \
              value, so a fabricated \"Not Evaluated (presumed Least \
              Concern)\" is indistinguishable from a successful Red List \
              lookup. Once the IUCN tool exists this needs its own \
              provenance value — a queried NE is data; an invented one is \
              not.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "conservation.population_trend",
        grounding: Grounding::Unsourced,
        why: "An IUCN Red List field with no IUCN tool wired up. Reported \
              as \"stable\" for species that have never been assessed, which \
              reads as a measurement of a population nobody has counted.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "conservation.genetic_diversity_notes",
        grounding: Grounding::Unsourced,
        why: "No structured source at species level. Deprioritised \
              indefinitely absent a literature-mining step.",
    },
    // ── enemy_sensor ────────────────────────────────────────
    // Tools: scan_nearby_creatures (returns the actual nearby rows),
    // gbif_species_search. **Nothing here is Unsourced**, and that result
    // matters: a contract under which every agent looks guilty is a
    // contract nobody will keep. `enemy_sensor` reports creatures its tool
    // returned and rates a risk it was asked to judge. It is well-formed.
    FieldContract {
        agent_id: "enemy_sensor",
        path: "threats[].creature_id",
        grounding: Grounding::Sourced {
            tool: "scan_nearby_creatures",
            response_field: "nearby[].creature_id",
        },
        why: "The scan returns the creatures; the agent may only report ones \
              it was handed. An id not in the scan would be an invented \
              creature.",
    },
    FieldContract {
        agent_id: "enemy_sensor",
        path: "threats[].species",
        grounding: Grounding::Sourced {
            tool: "scan_nearby_creatures",
            response_field: "nearby[].scientific_name",
        },
        why: "Same row as the creature_id it accompanies — the scan carries \
              scientific_name, family and order for every hit.",
    },
    FieldContract {
        agent_id: "enemy_sensor",
        path: "threats[].relationship",
        grounding: Grounding::Inferred {
            from: "order/family from the scan, plus predator-prey reasoning",
        },
        why: "Judgement, not retrieval: no database holds 'Odonata prey on \
              Lepidoptera' keyed by these two creature ids. The card already \
              guards it — 'Do not invent predation relationships that do not \
              exist' — and that guard is aimed correctly, unlike the one on \
              genome_profiler.",
    },
    FieldContract {
        agent_id: "enemy_sensor",
        path: "threats[].risk",
        grounding: Grounding::Inferred {
            from: "taxonomy, size differential, habitat overlap, proximity",
        },
        why: "An enumerated judgement over sourced inputs. Producing it is \
              the entire point of the agent.",
    },
    FieldContract {
        agent_id: "enemy_sensor",
        path: "threat_level",
        grounding: Grounding::Inferred {
            from: "the aggregate of threats[].risk",
        },
        why: "Roll-up of the per-threat judgements; same status as its parts.",
    },
    FieldContract {
        agent_id: "enemy_sensor",
        path: "summary",
        grounding: Grounding::Narrative,
        why: "Prose over the assessment. Checked for the same reason \
              genome_profiler's is: parse_evidence_text lifts it out as the \
              episode's evidence, so it is the sentence a reader sees.",
    },
    // ── prey_locator ────────────────────────────────────────
    // Same scan tool as enemy_sensor, but this card asks for GEOMETRY, and
    // that is where it parts company. `scan_nearby_creatures` returns
    // `lat`/`lng` for the TARGET creature only; every nearby row carries
    // `h3_cell` and no coordinates and no distance
    // (`tools_legacy.rs:2800-2809`). So a waypoint latitude toward a prey
    // creature is a coordinate the agent was never given — in a flight plan.
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].creature_id",
        grounding: Grounding::Sourced {
            tool: "scan_nearby_creatures",
            response_field: "nearby[].creature_id",
        },
        why: "As enemy_sensor: prey must be a creature the scan returned.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].species",
        grounding: Grounding::Sourced {
            tool: "scan_nearby_creatures",
            response_field: "nearby[].scientific_name",
        },
        why: "Carried on the same scan row as the creature_id, so it is \
              retrieved rather than recalled — the agent cannot name a \
              species the scan did not hand it.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].order",
        grounding: Grounding::Sourced {
            tool: "scan_nearby_creatures",
            response_field: "nearby[].order",
        },
        why: "The scan resolves order and family from stored taxonomy.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].vulnerability",
        grounding: Grounding::Inferred {
            from: "size ratio, life stage, defences, habitat overlap",
        },
        why: "The tactical judgement the agent exists to make.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].reasoning",
        grounding: Grounding::Inferred {
            from: "the factors behind the vulnerability rating",
        },
        why: "Explanation of a judgement, and therefore judgement.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "prey_targets[].distance_cells",
        grounding: Grounding::Unsourced,
        why: "The scan returns `h3_cell` per neighbour and no distance of any \
              kind. A ring count between two H3 cells is exactly computable \
              and `h3o` is already a dependency — so this is the cheapest \
              Unsourced field in the corpus to retire: compute it in \
              `execute_scan_nearby_creatures` and it becomes Sourced. Until \
              someone does, the number is the model's guess at a quantity \
              the platform could state exactly.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.waypoints[].lat",
        grounding: Grounding::Unsourced,
        why: "Nearby creatures come back with `h3_cell` and no coordinates; \
              only the target creature has lat/lng. A waypoint latitude is \
              therefore invented, and it is invented into a FLIGHT PLAN, \
              where a plausible-looking wrong number is acted on rather than \
              read. H3 cell centres are exactly resolvable, so this is a \
              missing derivation rather than an impossible one.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.waypoints[].lng",
        grounding: Grounding::Unsourced,
        why: "Same as the latitude it is paired with — no coordinate for any \
              nearby creature reaches this agent.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.waypoints[].altitude_m",
        grounding: Grounding::Unsourced,
        why: "No altitude appears in any tool response, and unlike the \
              coordinates it is not derivable from an H3 cell either.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.estimated_distance_m",
        grounding: Grounding::Unsourced,
        why: "A metre distance to a creature whose position the agent was \
              never told. Derivable once cell centres are resolved; guessed \
              today.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.approach",
        grounding: Grounding::Inferred {
            from: "predator capability and prey escape behaviour",
        },
        why: "Tactical judgement, which is the requested product.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.intercept_strategy",
        grounding: Grounding::Inferred {
            from: "approach vector and prey behaviour",
        },
        why: "Tactical judgement over the approach vector — no tool \
              returns an intercept plan, and none could; it is the \
              reasoning the caller is paying for.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "flight_plan.difficulty",
        grounding: Grounding::Inferred {
            from: "the intercept problem as assessed",
        },
        why: "An enumerated judgement, not a measured quantity.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "hunting_summary",
        grounding: Grounding::Narrative,
        why: "Prose over the scan result; same leak channel as every other \
              summary field in this contract.",
    },
    FieldContract {
        agent_id: "prey_locator",
        path: "tactical_notes",
        grounding: Grounding::Narrative,
        why: "Free prose accompanying the flight plan, and therefore the \
              place a stripped coordinate would reappear as text.",
    },
    FieldContract {
        agent_id: "genome_profiler",
        path: "summary",
        grounding: Grounding::Narrative,
        why: "Prose over whatever was retrieved. Checked because \
              `parse_evidence_text` lifts it out as the episode's evidence, \
              making it the sentence a user actually reads — and therefore \
              the channel a stripped number moves into.",
    },
];

// ─── enforcement ───────────────────────────────────────────────────────

/// A field that was populated when it had no business being.
#[derive(Debug, Clone, PartialEq)]
pub struct Violation {
    /// Dotted path of the offending field.
    pub path: String,
    /// What the model put there. Retained rather than discarded so the
    /// caller can quarantine it for later comparison against a real source
    /// — the difference between "tag for reprocessing" and "delete".
    pub removed: Value,
    /// Machine-stable reason, for anomaly payloads.
    pub kind: ViolationKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationKind {
    /// An `Unsourced` field was non-null.
    UngroundedField,
    /// A `Narrative` field asserted something an unsourced block would have
    /// had to supply.
    NarrativeLeak,
}

/// What [`enforce`] did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub violations: Vec<Violation>,
    /// `(block, provenance)` pairs written onto the document.
    pub provenance: Vec<(String, &'static str)>,
}

impl Report {
    pub fn is_clean(&self) -> bool {
        self.violations.is_empty()
    }
}

/// Is this value an actual claim, as opposed to absent or a placeholder?
///
/// `"..."` counts as absent: it is the card's own schema-example filler, and
/// a model echoing it has declined to answer rather than fabricated one.
fn is_claim(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::String(s) => {
            let t = s.trim();
            !t.is_empty() && t != "..." && t != "null" && t != "N/A" && t != "-"
        }
        Value::Array(a) => a.iter().any(is_claim),
        Value::Object(o) => o.values().any(is_claim),
        _ => true,
    }
}

/// Collect every value a path selects.
///
/// A `[]` segment means "each element of this array", so
/// `threats[].species` selects the species of every reported threat. Needed
/// because the second agent under the contract keeps its interesting fields
/// inside arrays, and a contract that can only address top-level scalars
/// would silently decline to check them — passing by being unable to look.
fn select<'a>(doc: &'a Value, segs: &[&str]) -> Vec<&'a Value> {
    let Some((head, rest)) = segs.split_first() else {
        return vec![doc];
    };
    if *head == "[]" {
        return match doc.as_array() {
            Some(items) => items.iter().flat_map(|it| select(it, rest)).collect(),
            None => vec![],
        };
    }
    match doc.get(head) {
        Some(v) => select(v, rest),
        None => vec![],
    }
}

/// Split a dotted path, turning `foo[]` into `foo` + `[]`.
fn segments(path: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for seg in path.split('.') {
        if let Some(name) = seg.strip_suffix("[]") {
            out.push(name);
            out.push("[]");
        } else {
            out.push(seg);
        }
    }
    out
}

fn get_path<'a>(doc: &'a Value, path: &str) -> Option<&'a Value> {
    select(doc, &segments(path)).into_iter().next()
}

/// Does any value this path selects constitute a claim?
fn path_has_claim(doc: &Value, path: &str) -> bool {
    select(doc, &segments(path)).into_iter().any(is_claim)
}

/// Replace the value at `path` with `null`, returning what was there.
///
/// Null rather than removed: an absent key is indistinguishable from a
/// serialisation bug or a truncated response, and the UI needs to be able to
/// say "we do not have this" rather than render a blank that reads as a
/// loading error.
fn null_path(doc: &mut Value, path: &str) -> Option<Value> {
    let removed = null_all(doc, &segments(path));
    match removed.len() {
        0 => None,
        1 => removed.into_iter().next(),
        // Several array elements were cleared. Report them together so the
        // caller can quarantine the lot; a violation per element would spam
        // the anomaly log with one row per threat in a list.
        _ => Some(Value::Array(removed)),
    }
}

/// Null every value the path selects, returning the non-null ones removed.
fn null_all(doc: &mut Value, segs: &[&str]) -> Vec<Value> {
    let Some((head, rest)) = segs.split_first() else {
        if doc.is_null() {
            return vec![];
        }
        return vec![std::mem::replace(doc, Value::Null)];
    };
    if *head == "[]" {
        return match doc.as_array_mut() {
            Some(items) => items.iter_mut().flat_map(|it| null_all(it, rest)).collect(),
            None => vec![],
        };
    }
    match doc.get_mut(head) {
        Some(v) => null_all(v, rest),
        None => vec![],
    }
}

/// Top-level block a dotted path belongs to.
fn block_of(path: &str) -> &str {
    let head = path.split('.').next().unwrap_or(path);
    // Strip the array marker: `threats[].species` belongs to block
    // `threats`, which is where its `_provenance` sibling goes. Without
    // this the block was named `threats[]`, nothing matched, and every
    // array-bearing agent got no provenance at all -- silently, because an
    // absent key is indistinguishable from an agent nobody has contracted.
    head.strip_suffix("[]").unwrap_or(head)
}

/// Contracts for one agent.
pub fn contracts_for(agent_id: &str) -> impl Iterator<Item = &'static FieldContract> + '_ {
    FIELD_CONTRACTS
        .iter()
        .filter(move |c| c.agent_id == agent_id)
}

/// Strip ungrounded values from `doc`, stamp provenance, and report what
/// was wrong.
///
/// A no-op for agents with no declared contract: silence must not read as a
/// verdict. `port_census.py` is what reports the absence of a contract;
/// this function must not invent one.
pub fn enforce(agent_id: &str, doc: &mut Value) -> Report {
    let contracts: Vec<&FieldContract> = contracts_for(agent_id).collect();
    if contracts.is_empty() || !doc.is_object() {
        return Report::default();
    }

    let mut report = Report::default();

    // 1. Ungrounded structured fields.
    for c in &contracts {
        if c.grounding != Grounding::Unsourced {
            continue;
        }
        let populated = path_has_claim(doc, c.path);
        if populated {
            if let Some(removed) = null_path(doc, c.path) {
                report.violations.push(Violation {
                    path: c.path.to_string(),
                    removed,
                    kind: ViolationKind::UngroundedField,
                });
            }
        }
    }

    // 2. Which blocks ended up with any real, sourced content?
    let mut sourced_block_has_content: Vec<(&str, bool)> = Vec::new();
    for c in &contracts {
        if let Grounding::Sourced { .. } = c.grounding {
            let b = block_of(c.path);
            let has = path_has_claim(doc, c.path);
            match sourced_block_has_content.iter_mut().find(|(n, _)| *n == b) {
                Some(entry) => entry.1 |= has,
                None => sourced_block_has_content.push((b, has)),
            }
        }
    }

    // Blocks whose content is judgement rather than retrieval.
    let inferred_blocks: Vec<&str> = contracts
        .iter()
        .filter(|c| matches!(c.grounding, Grounding::Inferred { .. }))
        .map(|c| block_of(c.path))
        .collect();

    // Blocks computed by platform code from a sourced value. Ranked above
    // `inferred` when both are present in a block, because a reproducible
    // derivation is the stronger claim and the label should say so.
    let derived_blocks: Vec<&str> = contracts
        .iter()
        .filter(|c| matches!(c.grounding, Grounding::Derived { .. }))
        .map(|c| block_of(c.path))
        .collect();
    let block_is_sourced = |b: &str| {
        sourced_block_has_content
            .iter()
            .find(|(n, _)| *n == b)
            .map(|(_, has)| *has)
    };

    // 3. Narrative leaks — claims the sourced blocks cannot support.
    for c in &contracts {
        if c.grounding != Grounding::Narrative {
            continue;
        }
        let Some(text) = get_path(doc, c.path).and_then(|v| v.as_str()) else {
            continue;
        };
        let haystack = text.to_ascii_lowercase();
        let mut leaked = false;
        for (block, rule) in NARRATIVE_LEAKS {
            // Only a leak if that block is not actually sourced here.
            if block_is_sourced(block) == Some(true) {
                continue;
            }
            if rule.matches(&haystack) {
                leaked = true;
                break;
            }
        }
        if leaked {
            // Nulled, not merely flagged. A validator cannot rewrite prose
            // into honesty, and leaving the sentence in place would move the
            // fabrication into the one string a user actually reads. The
            // text is retained on the violation so the claim can be checked
            // against a real source later rather than lost.
            //
            // Nulling is also what makes enforcement idempotent: a cached
            // profile re-read through this function must not raise a fresh
            // anomaly every time.
            let removed = null_path(doc, c.path).unwrap_or(Value::Null);
            report.violations.push(Violation {
                path: c.path.to_string(),
                removed,
                kind: ViolationKind::NarrativeLeak,
            });
        }
    }

    // 4. Stamp `<block>_provenance`. Every block a contract mentions gets
    //    one, so a consumer never has to infer availability from emptiness.
    let mut blocks: Vec<&str> = Vec::new();
    for c in &contracts {
        let b = block_of(c.path);
        if c.grounding != Grounding::Narrative && !blocks.contains(&b) {
            blocks.push(b);
        }
    }
    // A block that is only ever a narrative gets no provenance key: the
    // summary is not a data block and labelling it as one would imply a
    // retrieval claim it never makes.
    for b in blocks {
        let verdict = match block_is_sourced(b) {
            // Has sourced fields, and at least one came back populated.
            Some(true) => PROV_TOOL,
            // Has sourced fields and none came back: the tool was asked and
            // had nothing. Distinct from "no tool exists".
            Some(false) => PROV_NO_MATCH,
            // No sourced field. A reproducible derivation outranks a model
            // judgement; anything else genuinely has no source.
            None if derived_blocks.contains(&b) => PROV_DERIVED,
            None if inferred_blocks.contains(&b) => PROV_INFERRED,
            None => PROV_UNAVAILABLE,
        };
        if let Some(obj) = doc.as_object_mut() {
            obj.insert(format!("{b}_provenance"), Value::String(verdict.into()));
        }
        report.provenance.push((b.to_string(), verdict));
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A document shaped exactly like the ones in production today.
    fn fabricated() -> Value {
        json!({
            "taxonomy": {
                "kingdom": "Animalia", "phylum": "Arthropoda", "class": "Insecta",
                "order": "Lepidoptera", "family": "Nymphalidae",
                "genus": "Apatura", "species": "Apatura iris"
            },
            "genome": {
                "estimated_size_mb": "420-480",
                "chromosome_count": "n=31 (diploid 2n=62)",
                "notable_genes": ["opsin duplications"],
                "ploidy": "diploid"
            },
            "phylogeny": {
                "superorder": "Holometabola",
                "sister_taxa": ["Apatura ilia"],
                "divergence_mya": "~90-100 million years ago",
                "defining_traits": ["iridescent scales"]
            },
            "conservation": {
                "iucn_status": "Not Evaluated (presumed Least Concern)",
                "population_trend": "stable",
                "genetic_diversity_notes": "no known concerns"
            },
            "summary": "Apatura iris sits in Nymphalidae with a ~450 Mb genome, \
                        having diverged ~90 MYA."
        })
    }

    #[test]
    fn provenance_values_are_closed() {
        for (_, rule) in NARRATIVE_LEAKS {
            let needle = match rule {
                LeakRule::Word(w) => *w,
                LeakRule::Quantity(u) => *u,
            };
            assert!(!needle.is_empty(), "an empty needle matches everything");
            assert_eq!(
                needle,
                needle.to_ascii_lowercase(),
                "needles are matched against a lowercased haystack, so an \
                 uppercase needle can never fire: {needle}"
            );
        }
        for (block, _) in NARRATIVE_LEAKS {
            assert!(
                FIELD_CONTRACTS.iter().any(|c| block_of(c.path) == *block),
                "leak rule names block `{block}`, which no field contract \
                 mentions — the rule can never be adjudicated"
            );
        }
    }

    #[test]
    fn every_contract_explains_itself() {
        for c in FIELD_CONTRACTS {
            assert!(
                c.why.len() > 40,
                "{}.{} needs a real justification, not `{}` — an \
                 unexplained entry is how the contract rots",
                c.agent_id,
                c.path,
                c.why
            );
        }
    }

    #[test]
    fn strips_every_ungrounded_field_and_keeps_what_it_removed() {
        let mut doc = fabricated();
        let report = enforce("genome_profiler", &mut doc);

        // Still unsourceable: no tool supplies these, so they go.
        for path in [
            "genome.ploidy",
            "genome.notable_genes",
            "phylogeny.divergence_mya",
            "phylogeny.defining_traits",
            "conservation.iucn_status",
            "conservation.population_trend",
            "conservation.genetic_diversity_notes",
        ] {
            assert!(
                get_path(&doc, path).unwrap().is_null(),
                "{path} still has no source and must be stripped"
            );
            assert!(
                report.violations.iter().any(|v| v.path == path),
                "{path} was stripped without being reported"
            );
        }

        let iucn = report
            .violations
            .iter()
            .find(|v| v.path == "conservation.iucn_status")
            .unwrap();
        assert_eq!(
            iucn.removed,
            json!("Not Evaluated (presumed Least Concern)"),
            "the fabricated value must be retained for reprocessing"
        );
    }

    /// The fields the NCBI integration gave back.
    ///
    /// Before `ncbi_genome_search` existed, `enforce` nulled genome size and
    /// chromosome count and stamped the block `unavailable_no_tool_source`.
    /// The contract entries changed and nothing else did: same enforcement
    /// code, same card, opposite outcome. That is the property worth having —
    /// grounding is a statement about the tools available, so wiring up a tool
    /// is the whole fix.
    #[test]
    fn newly_sourced_fields_come_back() {
        let mut doc = fabricated();
        let report = enforce("genome_profiler", &mut doc);

        for path in ["genome.estimated_size_mb", "genome.chromosome_count"] {
            assert!(
                !get_path(&doc, path).unwrap().is_null(),
                "{path} is sourced from ncbi_genome_search now and must survive"
            );
            assert!(
                !report.violations.iter().any(|v| v.path == path),
                "{path} must no longer be reported as a violation"
            );
        }

        assert_eq!(
            doc.get("genome_provenance").and_then(|v| v.as_str()),
            Some(PROV_TOOL),
            "the genome block has a real tool behind it now"
        );
    }

    /// A derivation is not a retrieval and not a guess.
    #[test]
    fn a_derived_field_survives_and_says_it_was_computed() {
        let mut doc = json!({
            "taxonomy": { "order": "Lepidoptera" },
            "phylogeny": { "superorder": "Holometabola", "sister_taxa": [] },
            "summary": "A nymphalid."
        });
        let report = enforce("genome_profiler", &mut doc);
        assert_eq!(
            get_path(&doc, "phylogeny.superorder").unwrap(),
            &json!("Holometabola"),
            "superorder is derivable from taxonomy.order by a closed table"
        );
        assert!(!report
            .violations
            .iter()
            .any(|v| v.path == "phylogeny.superorder"));
    }

    #[test]
    fn does_not_touch_the_one_phylogeny_field_that_is_real() {
        let mut doc = fabricated();
        enforce("genome_profiler", &mut doc);
        assert_eq!(
            get_path(&doc, "phylogeny.sister_taxa").unwrap(),
            &json!(["Apatura ilia"]),
            "gbif_taxonomy_tree returns sibling taxa; stripping it would be \
             a check that overreaches, and those get switched off"
        );
    }

    #[test]
    fn taxonomy_survives_untouched() {
        let mut doc = fabricated();
        let before = doc.get("taxonomy").cloned().unwrap();
        enforce("genome_profiler", &mut doc);
        assert_eq!(doc.get("taxonomy"), Some(&before));
        assert_eq!(
            doc.get("taxonomy_provenance").and_then(|v| v.as_str()),
            Some(PROV_TOOL)
        );
    }

    #[test]
    fn the_narrative_is_not_a_loophole() {
        // Partial sourcing means partial leak detection, which is the correct
        // behaviour and worth pinning: now that genome size HAS a tool, prose
        // may cite it. Conservation still does not, so prose may not.
        let mut doc = fabricated();
        doc["summary"] = json!("Apatura iris is Not Evaluated by the IUCN and of least concern.");
        let report = enforce("genome_profiler", &mut doc);
        let leak = report
            .violations
            .iter()
            .find(|v| v.path == "summary" && v.kind == ViolationKind::NarrativeLeak);
        assert!(
            leak.is_some(),
            "the summary asserts a conservation status no tool can supply"
        );
        assert!(doc.get("summary").unwrap().is_null());
    }

    #[test]
    fn prose_may_cite_a_field_once_it_has_a_tool() {
        // The same sentence that was a leak before the NCBI integration is
        // legitimate after it. A leak rule keyed to a word rather than to
        // whether the block is sourced would have got this wrong forever.
        let mut doc = fabricated();
        doc["summary"] = json!("Apatura iris has a genome of roughly 450 Mb.");
        let report = enforce("genome_profiler", &mut doc);
        assert!(
            !report.violations.iter().any(|v| v.path == "summary"),
            "genome is sourced now, so citing a size is not a fabrication: {:?}",
            report.violations
        );
    }

    #[test]
    fn an_honest_narrative_is_left_alone() {
        let mut doc = fabricated();
        doc["summary"] = json!(
            "Apatura iris is a nymphalid butterfly; GBIF places it in \
             Apatura alongside Apatura ilia."
        );
        let report = enforce("genome_profiler", &mut doc);
        assert!(
            !report.violations.iter().any(|v| v.path == "summary"),
            "a summary that claims only what taxonomy supports must pass, or \
             the check cries wolf and gets ignored: {:?}",
            report.violations
        );
    }

    #[test]
    fn unsourced_blocks_are_labelled_as_unavailable_not_as_empty() {
        let mut doc = fabricated();
        let report = enforce("genome_profiler", &mut doc);

        // `conservation` still has no tool — IUCN needs a token nobody has
        // supplied yet — so it must say why it is empty rather than just be
        // empty.
        assert_eq!(
            doc.get("conservation_provenance").and_then(|v| v.as_str()),
            Some(PROV_UNAVAILABLE),
            "a null field with no provenance is indistinguishable from a \
             loading error"
        );
        assert!(report
            .provenance
            .iter()
            .all(|(_, v)| PROVENANCE_VALUES.contains(v)));
    }

    #[test]
    fn a_gbif_miss_is_distinguishable_from_a_missing_tool() {
        let mut doc = fabricated();
        doc["taxonomy"] = json!({});
        doc["phylogeny"]["sister_taxa"] = Value::Null;
        let _ = enforce("genome_profiler", &mut doc);
        assert_eq!(
            doc.get("taxonomy_provenance").and_then(|v| v.as_str()),
            Some(PROV_NO_MATCH),
            "GBIF was asked and had nothing — materially different from \
             'no tool exists', and prompt 2's IUCN 'Not Evaluated' case is \
             the same distinction"
        );
        assert_eq!(
            doc.get("conservation_provenance").and_then(|v| v.as_str()),
            Some(PROV_UNAVAILABLE),
            "conservation has no tool at all, which is not the same as GBIF \
             having no match"
        );
    }

    #[test]
    fn enforcement_is_idempotent() {
        let mut doc = fabricated();
        let first = enforce("genome_profiler", &mut doc);
        let second = enforce("genome_profiler", &mut doc);
        assert!(!first.is_clean());
        assert!(
            second.is_clean(),
            "a second pass must find nothing; otherwise the validator would \
             log an anomaly every time a cached profile is re-read: {:?}",
            second.violations
        );
    }

    #[test]
    fn placeholders_are_an_absence_not_a_fabrication() {
        let mut doc = json!({
            "taxonomy": {"species": "Apatura iris"},
            "genome": {"estimated_size_mb": "...", "ploidy": ""},
            "summary": "A nymphalid."
        });
        let report = enforce("genome_profiler", &mut doc);
        assert!(
            report.violations.is_empty(),
            "the card's own example used `...` as filler; a model echoing it \
             has declined to answer, not invented one: {:?}",
            report.violations
        );
    }

    // ── the second and third agents ─────────────────────────────

    #[test]
    fn a_well_grounded_agent_passes_completely() {
        // `enemy_sensor` reports creatures its scan returned and rates a
        // risk it was asked to judge. Nothing it produces is a retrieval
        // claim it cannot back.
        //
        // This is the single most important test in the module. A contract
        // under which every agent looks guilty is a contract nobody keeps,
        // and "the checker found a problem everywhere" is indistinguishable
        // from "the checker is broken".
        let mut doc = json!({
            "threat_level": "medium",
            "threats": [{
                "creature_id": "6f1a...",
                "species": "Aeshna cyanea",
                "relationship": "Odonata are aerial predators of Lepidoptera",
                "risk": "medium"
            }],
            "summary": "One dragonfly within the scan radius poses a moderate risk."
        });
        let report = enforce("enemy_sensor", &mut doc);
        assert!(
            report.is_clean(),
            "enemy_sensor is well-formed; flagging it would prove the \
             contract cannot tell an agent that fabricates from one that \
             reasons: {:?}",
            report.violations
        );
        assert_eq!(
            doc.get("threats_provenance").and_then(|v| v.as_str()),
            Some(PROV_TOOL),
            "the threats block is retrieved-plus-judged, and its creature \
             rows came from the scan"
        );
    }

    #[test]
    fn a_judgement_is_labelled_as_judgement_not_stripped() {
        let mut doc = json!({
            "threat_level": "high",
            "threats": [],
            "summary": "No neighbours in range."
        });
        enforce("enemy_sensor", &mut doc);
        assert_eq!(
            doc.get("threat_level").and_then(|v| v.as_str()),
            Some("high"),
            "an Inferred field is the agent's product and must survive"
        );
        assert_eq!(
            doc.get("threat_level_provenance").and_then(|v| v.as_str()),
            Some(PROV_INFERRED),
            "but it must be labelled as reasoning rather than measurement"
        );
    }

    #[test]
    fn invented_coordinates_are_stripped_from_a_flight_plan() {
        // `scan_nearby_creatures` returns lat/lng for the TARGET only; every
        // nearby row carries h3_cell and no coordinates. So these waypoints
        // are numbers the agent was never given, in a document meant to be
        // acted on.
        let mut doc = json!({
            "prey_targets": [{
                "creature_id": "abc",
                "species": "Pieris rapae",
                "order": "Lepidoptera",
                "vulnerability": "high",
                "reasoning": "slow flier, no defences",
                "distance_cells": 3
            }],
            "flight_plan": {
                "approach": "downwind from the treeline",
                "waypoints": [
                    {"lat": 51.5072, "lng": -0.1276, "altitude_m": 12, "instruction": "climb"},
                    {"lat": 51.5081, "lng": -0.1290, "altitude_m": 8, "instruction": "intercept"}
                ],
                "intercept_strategy": "stoop from above",
                "estimated_distance_m": 140,
                "difficulty": "moderate"
            },
            "hunting_summary": "One viable target to the north-west.",
            "tactical_notes": "Approach low."
        });
        let report = enforce("prey_locator", &mut doc);

        for path in [
            "flight_plan.waypoints[].lat",
            "flight_plan.waypoints[].lng",
            "flight_plan.waypoints[].altitude_m",
            "flight_plan.estimated_distance_m",
            "prey_targets[].distance_cells",
        ] {
            assert!(
                report.violations.iter().any(|v| v.path == path),
                "{path} was invented and not reported"
            );
        }

        // Both array elements cleared, not just the first.
        let wps = doc["flight_plan"]["waypoints"].as_array().unwrap();
        assert_eq!(wps.len(), 2);
        for wp in wps {
            assert!(wp["lat"].is_null(), "every waypoint must be cleared");
            assert!(wp["lng"].is_null());
            assert_eq!(
                wp["instruction"].as_str(),
                Some("climb")
                    .filter(|_| wp["instruction"] == "climb")
                    .or(Some("intercept")),
                "the instruction is prose the agent may write; only the \
                 fabricated geometry goes"
            );
        }

        // And the judgement survives.
        assert_eq!(
            doc["flight_plan"]["approach"].as_str(),
            Some("downwind from the treeline")
        );
        assert_eq!(
            doc["prey_targets"][0]["species"].as_str(),
            Some("Pieris rapae"),
            "the scan supplied this; stripping it would be over-reach"
        );
    }

    #[test]
    fn array_paths_report_every_element_they_cleared() {
        let mut doc = json!({
            "prey_targets": [
                {"creature_id": "a", "distance_cells": 1},
                {"creature_id": "b", "distance_cells": 4}
            ]
        });
        let report = enforce("prey_locator", &mut doc);
        let v = report
            .violations
            .iter()
            .find(|v| v.path == "prey_targets[].distance_cells")
            .expect("array path must be checked, not skipped");
        assert_eq!(
            v.removed,
            json!([1, 4]),
            "both guesses retained for reprocessing, as one violation rather \
             than one row per array element"
        );
    }

    #[test]
    fn an_agent_with_no_contract_is_left_entirely_alone() {
        let mut doc = fabricated();
        let before = doc.clone();
        let report = enforce("weather_oracle", &mut doc);
        assert_eq!(doc, before, "silence is not a verdict");
        assert!(report.is_clean());
        assert!(report.provenance.is_empty());
    }
}
