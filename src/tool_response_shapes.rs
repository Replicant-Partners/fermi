//! # What a tool actually returns
//!
//! ## The question this answers
//!
//! From review: *"wouldn't the tool determine which fields are available on a
//! sourced thing?"* Yes. It should, and until now it did not.
//!
//! `contract_sketch`'s suggestion path extracted candidate field names from a
//! tool's **description prose** — the nouns after the first colon — which is
//! why every one came back marked `unconfirmed`. That was the honest labelling
//! of a bad method. A tool description is the tool author's sentence about the
//! tool; the response keys are a different thing, and a contract's
//! `response_field` claims to name one of them.
//!
//! So the shape is declared here, once, per tool.
//!
//! ## Why a side table rather than a field on `BuiltinToolDef`
//!
//! Two reasons, one principled and one practical. This is *contract-authoring*
//! metadata, not tool-dispatch metadata: nothing in `ToolRegistry::execute`
//! needs it, and a struct that carries it invites the assumption that dispatch
//! validates against it. And practically, several `BuiltinToolDef` literals
//! spell out every field rather than using `..Default::default()`, so adding
//! one would touch a hundred definitions in a file two sessions are editing.
//!
//! ## The honesty field
//!
//! [`Evidence`] records where each declaration came from, and the distinction
//! is the same one the whole contract system is about:
//!
//! - [`Evidence::Constructed`] — the response is built by a `json!` literal in
//!   this repo. The declaration is verifiable by reading the named function,
//!   and a reviewer can check it in thirty seconds.
//! - [`Evidence::Vendor`] — a passthrough. The shape is the vendor's, taken
//!   from their documented API, and it can change without this repo noticing.
//!
//! A `Vendor` declaration is weaker and says so. What it is not is a guess:
//! anything nobody has checked is simply absent from this table, and an absent
//! tool falls back to the prose extraction with its `unconfirmed` marks.
//!
//! ## What this is not
//!
//! It is not a schema for the tool's response, and it does not validate one.
//! Nothing here runs at execution time. It exists so that an author choosing
//! where a block's values come from is choosing from a list of things that
//! exist, rather than typing a plausible key from memory — which is the same
//! failure, one level up, as the agent typing a plausible value.

/// One field of a tool's response.
pub struct ResponseField {
    /// Where it sits in the response, as the tool emits it. Dotted, with
    /// `[0]` for "the first element", matching how a card's `response_field`
    /// is written today.
    pub path: &'static str,
    /// A snake_case name for it in the agent's document. Suggested, not
    /// imposed: `assembled_chromosome_count` is the tool's name for it and
    /// `chromosome_count` may read better in a phylogenetic profile.
    pub field: &'static str,
    /// The type, in `contract_sketch`'s mini-language, so picking a field
    /// gives both the name and the type and the author guesses neither.
    ///
    /// Nullable by default throughout, and deliberately: a retrieval that
    /// found nothing must be able to say so, and the corpus convention is a
    /// type union rather than an absent key.
    pub ty: &'static str,
    /// What it is, where the name does not say. Empty where it does.
    pub note: &'static str,
}

/// Where a declaration came from.
pub enum Evidence {
    /// Built by a `json!` literal in this repo, at the named function.
    /// Verifiable by reading it.
    Constructed { at: &'static str },
    /// A vendor passthrough. The shape is theirs and can change without this
    /// repo noticing.
    Vendor { api: &'static str },
}

impl Evidence {
    pub fn kind(&self) -> &'static str {
        match self {
            Evidence::Constructed { .. } => "constructed",
            Evidence::Vendor { .. } => "vendor",
        }
    }
    pub fn where_from(&self) -> &'static str {
        match self {
            Evidence::Constructed { at } => at,
            Evidence::Vendor { api } => api,
        }
    }
}

/// One tool's declared response.
pub struct ToolResponse {
    pub tool: &'static str,
    pub evidence: Evidence,
    pub fields: &'static [ResponseField],
}

/// Every tool whose response shape someone has actually checked.
///
/// Absence is meaningful: a tool missing from this table has not been read,
/// and the builder falls back to extracting candidates from its description
/// and marking them unconfirmed. Adding a tool means reading its
/// implementation, not guessing from its name.
pub const TOOL_RESPONSES: &[ToolResponse] = &[
    // ── GBIF ────────────────────────────────────────────────────────
    ToolResponse {
        tool: "gbif_species_search",
        evidence: Evidence::Constructed {
            at: "tools_legacy::execute_gbif_species_search — the `species` \
                 projection, which selects a fixed set of keys from GBIF's \
                 response rather than passing it through",
        },
        fields: &[
            ResponseField {
                path: "species[0].key",
                field: "gbif_key",
                ty: "integer?",
                note: "The identity every downstream consumer should key on, \
                       rather than a name string two sources may spell \
                       differently.",
            },
            ResponseField {
                path: "species[0].scientificName",
                field: "scientific_name",
                ty: "string?",
                note: "Includes the authority.",
            },
            ResponseField {
                path: "species[0].canonicalName",
                field: "canonical_name",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "species[0].vernacularName",
                field: "common_name",
                ty: "string?",
                note: "English, via `gbif_preferred_vernacular`. Was null on \
                       every call for years because the search response has no \
                       `vernacularName` key and the tool read one anyway.",
            },
            ResponseField {
                path: "species[0].vernacularNamesEnglish",
                field: "common_names",
                ty: "string[]?",
                note: "Up to eight, deduplicated.",
            },
            ResponseField {
                path: "species[0].kingdom",
                field: "kingdom",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "species[0].phylum",
                field: "phylum",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "species[0].class",
                field: "class",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "species[0].order",
                field: "order",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "species[0].family",
                field: "family",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "species[0].genus",
                field: "genus",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "species[0].species",
                field: "species",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "species[0].rank",
                field: "rank",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "species[0].taxonomicStatus",
                field: "taxonomic_status",
                ty: "string?",
                note: "ACCEPTED, SYNONYM, DOUBTFUL. A synonym resolving to a \
                       different accepted taxon is a silent way for a card to \
                       be about the wrong animal.",
            },
            ResponseField {
                path: "media.results[]",
                field: "image_urls",
                ty: "string[]?",
                note: "Only on a lookup by `gbif_key`; a search by name never \
                       calls the media endpoint, so empty means not asked.",
            },
        ],
    },
    ToolResponse {
        tool: "gbif_taxonomy_tree",
        evidence: Evidence::Constructed {
            at: "tools_legacy::execute_gbif_taxonomy_tree",
        },
        fields: &[
            ResponseField {
                path: "species",
                field: "species",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "parents[]",
                field: "lineage",
                ty: "string[]?",
                note: "The rank ladder above the queried taxon.",
            },
            ResponseField {
                path: "family_siblings[]",
                field: "sister_taxa",
                ty: "string[]?",
                note: "Other genera in the family. Sisterhood here is \
                       taxonomic adjacency, not a phylogenetic claim.",
            },
            ResponseField {
                path: "order_families[]",
                field: "families_in_order",
                ty: "string[]?",
                note: "",
            },
        ],
    },
    // ── NCBI ────────────────────────────────────────────────────────
    ToolResponse {
        tool: "ncbi_genome_search",
        evidence: Evidence::Constructed {
            at: "agent_backend::ncbi_tools::execute_ncbi_genome_search",
        },
        fields: &[
            ResponseField {
                path: "found",
                field: "assembly_found",
                ty: "boolean?",
                note: "False is the common case for insects and is the honest \
                       answer, not a failure.",
            },
            ResponseField {
                path: "estimated_size_mb",
                field: "estimated_size_mb",
                ty: "number?",
                note: "THE field. A genome size written here from model memory \
                       rather than from this response is the fabrication that \
                       shipped for 56 episodes and started this whole line of \
                       work.",
            },
            ResponseField {
                path: "total_length_bp",
                field: "total_length_bp",
                ty: "integer?",
                note: "",
            },
            ResponseField {
                path: "assembled_chromosome_count",
                field: "chromosome_count",
                ty: "integer?",
                note: "The tool's name is `assembled_chromosome_count`; a \
                       profile may prefer the shorter one. Assembled, not \
                       karyotypic — they differ for many taxa.",
            },
            ResponseField {
                path: "karyotype",
                field: "karyotype",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "ploidy",
                field: "ploidy",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "ploidy_note",
                field: "ploidy_note",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "assembly_name",
                field: "assembly_name",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "assembly_accession",
                field: "assembly_accession",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "assembly_status",
                field: "assembly_status",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "release_date",
                field: "release_date",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "submitter",
                field: "submitter",
                ty: "string?",
                note: "",
            },
            ResponseField {
                path: "reason",
                field: "no_assembly_reason",
                ty: "string?",
                note: "Why nothing was found. Worth carrying: `no assembly \
                       exists` and `the query did not resolve` are different \
                       facts.",
            },
        ],
    },
    // ── Weather ─────────────────────────────────────────────────────
    ToolResponse {
        tool: "weather_ensemble_forecast",
        evidence: Evidence::Constructed {
            at: "agent_backend::weather_tools::ensemble_forecast",
        },
        fields: &[
            ResponseField {
                path: "bucket_probabilities",
                field: "raw_probability",
                ty: "number?",
                note: "RAW. The tool's own response carries a \
                       `calibration_required` block saying these must not be \
                       traded directly.",
            },
            ResponseField {
                path: "ensemble.n_members",
                field: "member_count",
                ty: "integer?",
                note: "",
            },
            ResponseField {
                path: "ensemble.models_returned",
                field: "models_returned",
                ty: "string[]?",
                note: "",
            },
            ResponseField {
                path: "ensemble.models_missing",
                field: "models_missing",
                ty: "string[]?",
                note: "Fewer than three returned means cross-model \
                       disagreement is UNMEASURABLE, not small.",
            },
            ResponseField {
                path: "ensemble.mean",
                field: "ensemble_mean",
                ty: "number?",
                note: "",
            },
            ResponseField {
                path: "ensemble.median",
                field: "ensemble_median",
                ty: "number?",
                note: "",
            },
            ResponseField {
                path: "ensemble.std_dev",
                field: "within_model_spread",
                ty: "number?",
                note: "Aleatoric: chaos at this lead time.",
            },
            ResponseField {
                path: "ensemble.p10",
                field: "p10",
                ty: "number?",
                note: "",
            },
            ResponseField {
                path: "ensemble.p90",
                field: "p90",
                ty: "number?",
                note: "",
            },
            ResponseField {
                path: "epistemic_disagreement.cross_model_median_range",
                field: "cross_model_spread",
                ty: "number?",
                note: "Epistemic: not knowing which model is right. No \
                       single-model ensemble can see it.",
            },
        ],
    },
    ToolResponse {
        tool: "weather_climatology",
        evidence: Evidence::Constructed {
            at: "agent_backend::weather_tools::climatology",
        },
        fields: &[
            ResponseField {
                path: "raw_base_rate",
                field: "climatology_base_rate",
                ty: "number?",
                note: "The reference a Brier Skill Score is computed against.",
            },
            ResponseField {
                path: "trend_adjusted_base_rate",
                field: "trend_adjusted_base_rate",
                ty: "number?",
                note: "A skill score against the wrong one of these flatters \
                       or punishes the forecast for a reason unrelated to the \
                       forecast.",
            },
            ResponseField {
                path: "trend_adjustment_pp",
                field: "warming_trend_per_decade",
                ty: "number?",
                note: "Percentage points.",
            },
            ResponseField {
                path: "n_years",
                field: "years_used",
                ty: "integer?",
                note: "",
            },
            ResponseField {
                path: "n_observations",
                field: "observation_count",
                ty: "integer?",
                note: "",
            },
        ],
    },
    ToolResponse {
        tool: "weather_station_observation",
        evidence: Evidence::Constructed {
            at: "agent_backend::weather_tools::station_observation",
        },
        fields: &[
            ResponseField {
                path: "available",
                field: "observation_available",
                ty: "boolean?",
                note: "NWS stations only. False means not asked or not \
                       covered, never that nothing happened.",
            },
            ResponseField {
                path: "running_extremes_in_window.max_f",
                field: "running_max",
                ty: "number?",
                note: "A HARD FLOOR on a daily-high question. A distribution \
                       placing weight below it is impossible, not merely \
                       poorly calibrated.",
            },
            ResponseField {
                path: "running_extremes_in_window.min_f",
                field: "running_min",
                ty: "number?",
                note: "",
            },
            ResponseField {
                path: "latest",
                field: "official_daily",
                ty: "number?",
                note: "The CLI figure once published.",
            },
            ResponseField {
                path: "precipitation_sum_mm",
                field: "precipitation_mm",
                ty: "number?",
                note: "",
            },
        ],
    },
    ToolResponse {
        tool: "polymarket_orderbook",
        evidence: Evidence::Constructed {
            at: "agent_backend::weather_tools::polymarket_orderbook",
        },
        fields: &[
            ResponseField {
                path: "best_bid",
                field: "best_bid",
                ty: "number?",
                note: "",
            },
            ResponseField {
                path: "best_ask",
                field: "best_ask",
                ty: "number?",
                note: "",
            },
            ResponseField {
                path: "spread",
                field: "spread",
                ty: "number?",
                note: "",
            },
            ResponseField {
                path: "midpoint",
                field: "midpoint",
                ty: "number?",
                note: "",
            },
            ResponseField {
                path: "implied_probability",
                field: "implied_probability",
                ty: "number?",
                note: "From the book, not from a last-traded price. \
                       Last-traded is history; the book is what you can \
                       transact against.",
            },
            ResponseField {
                path: "book_quality.issues",
                field: "book_quality_flags",
                ty: "string[]?",
                note: "A wide or one-sided book makes a nominal edge \
                       unrealisable.",
            },
            ResponseField {
                path: "tradeable",
                field: "tradeable",
                ty: "boolean?",
                note: "",
            },
            ResponseField {
                path: "notional_usd",
                field: "notional_usd",
                ty: "number?",
                note: "",
            },
        ],
    },
    // ── Financial Modeling Prep ─────────────────────────────────────
    //
    // Passthroughs: `execute_fmp_api` forwards the vendor's response
    // unchanged, so these shapes are FMP's and can change without this repo
    // noticing. Declared because they are documented and stable in practice,
    // and marked `Vendor` so a reviewer knows the difference.
    ToolResponse {
        tool: "fmp_company_profile",
        evidence: Evidence::Vendor {
            api: "Financial Modeling Prep /stable/profile",
        },
        fields: &[
            ResponseField { path: "[0].symbol", field: "symbol", ty: "string?", note: "" },
            ResponseField { path: "[0].companyName", field: "company_name", ty: "string?", note: "" },
            ResponseField { path: "[0].sector", field: "sector", ty: "string?", note: "" },
            ResponseField { path: "[0].industry", field: "industry", ty: "string?", note: "" },
            ResponseField { path: "[0].price", field: "price_usd", ty: "number?", note: "" },
            ResponseField { path: "[0].marketCap", field: "market_cap_usd", ty: "number?", note: "" },
            ResponseField { path: "[0].beta", field: "beta", ty: "number?", note: "" },
            ResponseField { path: "[0].ceo", field: "ceo", ty: "string?", note: "" },
            ResponseField { path: "[0].range", field: "week_52_range", ty: "string?", note: "" },
        ],
    },
    ToolResponse {
        tool: "fmp_ratios",
        evidence: Evidence::Vendor {
            api: "Financial Modeling Prep /stable/ratios",
        },
        fields: &[
            ResponseField { path: "[0].priceToEarningsRatio", field: "price_to_earnings", ty: "number?", note: "" },
            ResponseField { path: "[0].priceToBookRatio", field: "price_to_book", ty: "number?", note: "" },
            ResponseField { path: "[0].priceToSalesRatio", field: "price_to_sales", ty: "number?", note: "" },
            ResponseField { path: "[0].dividendYield", field: "dividend_yield", ty: "number?", note: "" },
            ResponseField { path: "[0].currentRatio", field: "current_ratio", ty: "number?", note: "" },
            ResponseField { path: "[0].debtToEquityRatio", field: "debt_to_equity", ty: "number?", note: "" },
            ResponseField { path: "[0].netProfitMargin", field: "net_profit_margin", ty: "number?", note: "" },
        ],
    },
    ToolResponse {
        tool: "fmp_key_metrics",
        evidence: Evidence::Vendor {
            api: "Financial Modeling Prep /stable/key-metrics",
        },
        fields: &[
            ResponseField { path: "[0].enterpriseValue", field: "enterprise_value_usd", ty: "number?", note: "" },
            ResponseField { path: "[0].returnOnEquity", field: "return_on_equity", ty: "number?", note: "" },
            ResponseField { path: "[0].returnOnInvestedCapital", field: "return_on_invested_capital", ty: "number?", note: "" },
            ResponseField { path: "[0].freeCashFlowYield", field: "free_cash_flow_yield", ty: "number?", note: "Absent for whole classes of filer, which is why a block sourced here is `partial`." },
            ResponseField { path: "[0].debtToEquity", field: "debt_to_equity", ty: "number?", note: "" },
            ResponseField { path: "[0].earningsYield", field: "earnings_yield", ty: "number?", note: "" },
        ],
    },
    ToolResponse {
        tool: "fmp_dcf",
        evidence: Evidence::Vendor {
            api: "Financial Modeling Prep /stable/discounted-cash-flow",
        },
        fields: &[
            ResponseField { path: "[0].dcf", field: "dcf_per_share_usd", ty: "number?", note: "" },
            ResponseField { path: "[0].stockPrice", field: "price_at_dcf_date_usd", ty: "number?", note: "The price on the same date, so the pair is comparable." },
            ResponseField { path: "[0].date", field: "dcf_date", ty: "string?", note: "" },
        ],
    },
    ToolResponse {
        tool: "fmp_analyst_estimates",
        evidence: Evidence::Vendor {
            api: "Financial Modeling Prep /stable/analyst-estimates",
        },
        fields: &[
            ResponseField { path: "[0].date", field: "estimate_date", ty: "string?", note: "" },
            ResponseField { path: "[0].revenueAvg", field: "revenue_avg_usd", ty: "number?", note: "" },
            ResponseField { path: "[0].epsAvg", field: "eps_avg", ty: "number?", note: "" },
            ResponseField { path: "[0].epsLow", field: "eps_low", ty: "number?", note: "" },
            ResponseField { path: "[0].epsHigh", field: "eps_high", ty: "number?", note: "" },
            ResponseField { path: "[0].numAnalystsEps", field: "analyst_count", ty: "integer?", note: "A consensus of two is a different object from a consensus of thirty." },
        ],
    },
];

/// The declared response for a tool, if anyone has read it.
pub fn response_for(tool: &str) -> Option<&'static ToolResponse> {
    TOOL_RESPONSES.iter().find(|t| t.tool == tool)
}

/// Which of a block's declared fields have a matching response field, and
/// which do not.
///
/// The second list is the interesting one, and it is the original bug stated
/// mechanically: `genome_profiler.genome` declares `notable_genes`, and
/// `ncbi_genome_search` returns no such thing. Before this table there was no
/// way to notice except by reading the tool.
///
/// Returns `None` when the tool has no declaration, which must not be read as
/// "everything is covered".
pub fn coverage<'a>(
    tool: &str,
    block_fields: &[&'a str],
) -> Option<(Vec<&'a str>, Vec<&'a str>)> {
    let decl = response_for(tool)?;
    let mut covered = Vec::new();
    let mut uncovered = Vec::new();
    for f in block_fields {
        // Match on the suggested name, or on the tail of the path — an author
        // who renamed `assembled_chromosome_count` to `chromosome_count` has
        // not lost the source, and flagging that would train people to ignore
        // this.
        let hit = decl.fields.iter().any(|r| {
            r.field == *f
                || r.path.rsplit('.').next().map(|p| p.trim_end_matches("[]")) == Some(*f)
        });
        if hit {
            covered.push(*f);
        } else {
            uncovered.push(*f);
        }
    }
    Some((covered, uncovered))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A declaration for a tool that does not dispatch describes nothing.
    #[test]
    fn every_declared_tool_actually_exists() {
        let real = crate::agent_backend::tools::platform_tool_names();
        for t in TOOL_RESPONSES {
            assert!(
                real.contains(&t.tool),
                "`{}` has a declared response shape and no dispatch arm",
                t.tool
            );
        }
    }

    /// Every declared type must parse, or the builder offers a field it cannot
    /// then compile.
    #[test]
    fn every_declared_type_is_a_valid_type_expression() {
        for t in TOOL_RESPONSES {
            for f in t.fields {
                crate::contract_sketch::TypeExpr::parse(f.ty).unwrap_or_else(|e| {
                    panic!("{}.{}: `{}` does not parse: {e}", t.tool, f.field, f.ty)
                });
            }
        }
    }

    /// Suggested names must be usable as document fields as-is.
    #[test]
    fn suggested_field_names_are_snake_case() {
        for t in TOOL_RESPONSES {
            for f in t.fields {
                assert!(!f.field.is_empty(), "{}: empty field name", t.tool);
                assert!(
                    f.field
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                    "{}.{} is not snake_case",
                    t.tool,
                    f.field
                );
                assert!(!f.path.is_empty(), "{}.{}: no path", t.tool, f.field);
            }
        }
    }

    /// No tool is declared twice, and every declaration says where it came
    /// from. `Vendor` is a weaker claim than `Constructed` and the difference
    /// has to survive into the UI.
    #[test]
    fn each_declaration_is_unique_and_attributed() {
        let mut seen = std::collections::HashSet::new();
        for t in TOOL_RESPONSES {
            assert!(seen.insert(t.tool), "`{}` declared twice", t.tool);
            assert!(
                !t.evidence.where_from().is_empty(),
                "`{}` does not say where its shape came from",
                t.tool
            );
            assert!(!t.fields.is_empty(), "`{}` declares no fields", t.tool);
        }
    }

    /// **The original bug, detected mechanically.**
    ///
    /// `genome_profiler.genome` declares `notable_genes`, and
    /// `ncbi_genome_search` returns no such field. That is precisely the shape
    /// that shipped fabricated genome data for 56 episodes: a field in a
    /// retrieved block with no possible source, indistinguishable from its
    /// neighbours.
    ///
    /// Before this table the only way to notice was to read the tool. Now the
    /// builder can say it while the author is looking at the block.
    #[test]
    fn the_coverage_check_finds_the_field_that_started_all_this() {
        let block = ["estimated_size_mb", "chromosome_count", "notable_genes", "ploidy"];
        let (covered, uncovered) =
            coverage("ncbi_genome_search", &block).expect("the tool is declared");

        assert!(
            covered.contains(&"estimated_size_mb"),
            "the tool really does return this one"
        );
        assert!(
            covered.contains(&"chromosome_count"),
            "matched on the tail of `assembled_chromosome_count` — renaming a \
             field does not lose its source, and flagging that would train \
             people to ignore this check"
        );
        assert_eq!(
            uncovered,
            vec!["notable_genes"],
            "the field with no source must be the one reported"
        );
    }

    /// An undeclared tool returns `None`, and that must not read as "covered".
    #[test]
    fn an_undeclared_tool_is_unknown_not_complete() {
        assert!(
            coverage("execute_agent", &["anything"]).is_none(),
            "absence of a declaration is absence of information"
        );
    }
}
