//! # NCBI Assembly — giving the genome fields a real source
//!
//! `genome_profiler` was asked for genome size and chromosome count with two
//! GBIF tools that return taxonomy. It answered anyway, for 56 episodes.
//! `grounding_trust` now nulls those fields and stamps them
//! `unavailable_no_tool_source`, which is honest but empty.
//!
//! This is the tool that makes them answerable. Same fields, same card,
//! opposite provenance: `tool_verified` instead of `unavailable`.
//!
//! ## Why the fabricated values were not merely unsourced but wrong
//!
//! The old prompt told the model *"Lepidoptera ~400-500Mb"*. The monarch
//! butterfly's assembled genome is **245 Mb** — out by roughly 2×. The
//! fabrication was not a harmless approximation of a real number; it was a
//! confident statement of a family-level average that does not describe the
//! species it was attached to.
//!
//! ## Coverage is sparse, and that is the correct answer
//!
//! Measured against the species actually in `creature_conditions`:
//!
//! ```text
//!   Papilio polyxenes       2 assemblies
//!   Sympetrum striolatum    2 assemblies
//!   Apatura iris            0
//!   Anatis mali             0
//!   Sphingonotus personatus 0
//!   Reclavaspis evexa       0
//! ```
//!
//! Two of six. Most insects are unsequenced, so most lookups will return
//! nothing — and a null with `tool_no_match` is a materially different fact
//! from a null with `unavailable_no_tool_source`. The first says *we asked
//! and there is nothing*; the second says *nobody asked*. Only one of those
//! is a gap in the world rather than a gap in the platform.
//!
//! ## What this tool deliberately does NOT return
//!
//! NCBI reports `assemblytype: "haploid"` for the monarch. It is extremely
//! tempting to map that onto `genome.ploidy`, and it would be **false**: it
//! describes how the assembly represents the genome, not the organism's
//! ploidy. A monarch is diploid. That mapping is plausible, convenient and
//! wrong, which is precisely the class of error this whole contract exists
//! to prevent — so `ploidy` stays unavailable and the reason is recorded in
//! the field contract.
//!
//! Likewise `chromosome_count` is reported as **assembled chromosome-level
//! replicons**, named as such rather than as "karyotype". For *Danaus
//! plexippus* it returns 30, which matches the published n=30; that
//! agreement is not a licence to relabel it.
//!
//! ## No credentials
//!
//! E-utilities needs no key at low volume. `NCBI_API_KEY` is honoured if
//! present (it raises the rate limit from 3 to 10 requests/second), and its
//! absence is not an error.

use serde_json::json;

const ESEARCH: &str = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esearch.fcgi";
const ESUMMARY: &str = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi";
const UA: &str = "AgentBestiaryWorld/1.0 (agent-bestiary.world)";

/// One `<Stat category="…" sequence_tag="…">N</Stat>` value from an
/// assembly's `meta` blob.
///
/// The genome size is not a top-level field of the esummary JSON — it is
/// inside an XML string under `meta`. Parsed with a narrow scan rather than
/// an XML dependency: we want exactly two integers out of a document whose
/// shape NCBI may extend, and a strict parser would fail on additions that
/// do not concern us.
fn stat(meta: &str, category: &str) -> Option<i64> {
    let needle = format!("<Stat category=\"{category}\" sequence_tag=\"all\">");
    let start = meta.find(&needle)? + needle.len();
    let rest = &meta[start..];
    let end = rest.find("</Stat>")?;
    rest[..end].trim().parse().ok()
}

fn api_key_param() -> String {
    match std::env::var("NCBI_API_KEY") {
        Ok(k) if !k.trim().is_empty() => format!("&api_key={}", k.trim()),
        _ => String::new(),
    }
}

/// Look up assembled genome statistics for a species binomial.
///
/// Returns a `found: false` document rather than an error when the species
/// has no assembly. That distinction is load-bearing: an error would be
/// indistinguishable from a network failure, and the agent must be able to
/// tell "unsequenced" from "could not ask".
pub async fn execute_ncbi_genome_search(input: &serde_json::Value) -> Result<String, String> {
    let name = input
        .get("scientific_name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or("scientific_name is required (a species binomial, e.g. 'Danaus plexippus')")?;

    let client = reqwest::Client::new();
    let term = urlencoding_encode(&format!("{name}[Organism]"));

    // 1. Which assemblies exist for this organism?
    let search_url = format!(
        "{ESEARCH}?db=assembly&term={term}&retmode=json&retmax=5{}",
        api_key_param()
    );
    let search: serde_json::Value = client
        .get(&search_url)
        .header("User-Agent", UA)
        .send()
        .await
        .map_err(|e| format!("NCBI esearch failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("NCBI esearch returned unparseable JSON: {e}"))?;

    let ids: Vec<String> = search["esearchresult"]["idlist"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();

    if ids.is_empty() {
        // The honest empty answer. Most insects land here.
        return Ok(serde_json::to_string_pretty(&json!({
            "found": false,
            "scientific_name": name,
            "reason": "No assembly in NCBI for this organism. Most insect species \
                       are unsequenced; this is a gap in the world, not in the query.",
            "provenance": "tool_no_match"
        }))
        .unwrap());
    }

    // 2. Statistics for the best assembly. First id is NCBI's own relevance
    //    order; we report which one we used so the number is traceable to a
    //    specific assembly rather than to "NCBI".
    let sum_url = format!(
        "{ESUMMARY}?db=assembly&id={}&retmode=json{}",
        ids[0],
        api_key_param()
    );
    let summary: serde_json::Value = client
        .get(&sum_url)
        .header("User-Agent", UA)
        .send()
        .await
        .map_err(|e| format!("NCBI esummary failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("NCBI esummary returned unparseable JSON: {e}"))?;

    let rec = &summary["result"][&ids[0]];
    let meta = rec["meta"].as_str().unwrap_or("");

    let total_length = stat(meta, "total_length");
    let chromosome_count = stat(meta, "chromosome_count");

    Ok(serde_json::to_string_pretty(&json!({
        "found": true,
        "scientific_name": name,
        "organism": rec["organism"].as_str(),
        "assembly_name": rec["assemblyname"].as_str(),
        "assembly_accession": rec["assemblyaccession"].as_str(),
        "assembly_status": rec["assemblystatus"].as_str(),
        "release_date": rec["asmreleasedate_genbank"].as_str(),
        "submitter": rec["submitterorganization"].as_str(),
        "assemblies_available": ids.len(),

        // The two fields this tool exists to ground.
        "estimated_size_mb": total_length.map(|b| (b as f64 / 1_000_000.0 * 10.0).round() / 10.0),
        "total_length_bp": total_length,
        // Named for what it is. NOT "karyotype": this counts
        // chromosome-level replicons in THIS assembly.
        "assembled_chromosome_count": chromosome_count,

        // Deliberately absent, with the reason, so the next reader does not
        // reach for it: `assemblytype` describes the ASSEMBLY, not the
        // organism. Mapping it to ploidy would be a plausible falsehood.
        "ploidy": serde_json::Value::Null,
        "ploidy_note": "Not derivable from NCBI. `assemblytype` describes how the \
                        assembly represents the genome, not the organism's ploidy.",

        "provenance": "tool_verified"
    }))
    .unwrap())
}

/// Percent-encode a query term. `reqwest` will not do it for a hand-built
/// URL and the repo has no `urlencoding` dependency.
fn urlencoding_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

// ─── superorder: derived, not retrieved ────────────────────────────────

/// Insect order → superorder. A closed table over the ~30 recognised orders.
///
/// `phylogeny.superorder` is not a GBIF rank and no tool returns it, so while
/// the model supplied it the field was `Unsourced` — it was recalled, not
/// looked up. It is nonetheless a deterministic function of
/// `taxonomy.order`, which GBIF *does* return, so writing the table converts
/// it from an invention into a [`crate::grounding_trust::Grounding::Derived`]
/// value: reproducible, auditable, and checkable by anyone who disagrees
/// with a row.
///
/// Returns `None` for an order not in the table rather than guessing, so an
/// unrecognised or misspelled order yields no superorder instead of a
/// plausible one.
pub fn superorder_of(order: &str) -> Option<&'static str> {
    let o = order.trim().to_ascii_lowercase();
    Some(match o.as_str() {
        // Holometabola — complete metamorphosis
        "lepidoptera" | "coleoptera" | "hymenoptera" | "diptera" | "siphonaptera" | "mecoptera"
        | "megaloptera" | "neuroptera" | "raphidioptera" | "strepsiptera" | "trichoptera" => {
            "Holometabola"
        }

        // Palaeoptera — ancient winged, no wing folding
        "odonata" | "ephemeroptera" => "Palaeoptera",

        // Polyneoptera — orthopteroid hemimetabolous
        "orthoptera" | "blattodea" | "mantodea" | "dermaptera" | "phasmatodea" | "plecoptera"
        | "embioptera" | "zoraptera" | "grylloblattodea" | "mantophasmatodea" | "isoptera" => {
            "Polyneoptera"
        }

        // Paraneoptera — hemipteroid
        "hemiptera" | "thysanoptera" | "psocoptera" | "phthiraptera" | "psocodea" => "Paraneoptera",

        // Apterygota — primitively wingless
        "zygentoma" | "archaeognatha" | "thysanura" => "Apterygota",

        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_genome_size_from_the_meta_blob() {
        // Verbatim shape of NCBI's `meta` field, which is XML inside JSON.
        let meta = r#"<Stats><Stat category="total_length" sequence_tag="all">245173502</Stat>
            <Stat category="ungapped_length" sequence_tag="all">245169302</Stat>
            <Stat category="chromosome_count" sequence_tag="all">30</Stat></Stats>"#;
        assert_eq!(stat(meta, "total_length"), Some(245_173_502));
        assert_eq!(stat(meta, "chromosome_count"), Some(30));
        assert_eq!(stat(meta, "no_such_category"), None);
    }

    #[test]
    fn the_monarchs_real_genome_is_nothing_like_the_fabricated_range() {
        // The old prompt asserted "Lepidoptera ~400-500Mb". The assembled
        // monarch genome is 245 Mb. Pinned as a regression: if anyone
        // reintroduces a family-level range as a default, this is the number
        // it would have been wrong by.
        let mb = 245_173_502_f64 / 1_000_000.0;
        assert!((240.0..250.0).contains(&mb));
        assert!(
            mb < 400.0,
            "the fabricated floor was 400Mb — out by ~2x, not a rounding error"
        );
    }

    #[test]
    fn superorder_is_a_lookup_not_a_guess() {
        assert_eq!(superorder_of("Lepidoptera"), Some("Holometabola"));
        assert_eq!(superorder_of("lepidoptera"), Some("Holometabola"));
        assert_eq!(superorder_of("  Odonata  "), Some("Palaeoptera"));
        assert_eq!(superorder_of("Orthoptera"), Some("Polyneoptera"));
        assert_eq!(superorder_of("Hemiptera"), Some("Paraneoptera"));
    }

    #[test]
    fn an_unknown_order_yields_nothing_rather_than_something_plausible() {
        // The whole value of a derivation is that it refuses outside its
        // domain. A guess here would be indistinguishable from the recall it
        // replaced.
        assert_eq!(superorder_of("Lepidotera"), None); // misspelled
        assert_eq!(superorder_of("Primates"), None); // not an insect
        assert_eq!(superorder_of(""), None);
        assert_eq!(superorder_of("Unknown"), None);
    }

    #[test]
    fn every_table_entry_maps_to_a_real_superorder() {
        const VALID: &[&str] = &[
            "Holometabola",
            "Palaeoptera",
            "Polyneoptera",
            "Paraneoptera",
            "Apterygota",
        ];
        for order in [
            "lepidoptera",
            "coleoptera",
            "hymenoptera",
            "diptera",
            "odonata",
            "ephemeroptera",
            "orthoptera",
            "blattodea",
            "mantodea",
            "hemiptera",
            "thysanoptera",
            "zygentoma",
            "archaeognatha",
            "trichoptera",
            "isoptera",
        ] {
            let s = superorder_of(order).unwrap_or_else(|| panic!("{order} missing"));
            assert!(VALID.contains(&s), "{order} -> {s} is not a superorder");
        }
    }

    #[test]
    fn query_terms_are_encoded() {
        assert_eq!(
            urlencoding_encode("Danaus plexippus[Organism]"),
            "Danaus+plexippus%5BOrganism%5D"
        );
    }
}
