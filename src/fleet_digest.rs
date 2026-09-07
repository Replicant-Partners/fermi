//! **The fleet's shape, in a fixed number of lines.**
//!
//! The prompt tier of the meta-agent pattern in
//! `docs/architecture/META_AGENT_FLEET_AWARENESS.md`. A meta agent gets the map
//! here and the territory from tools; this module is the map.
//!
//! # What it replaces
//!
//! 102 agents pasted into `xaman_ek`'s system prompt, one line each, kept in
//! sync by a test asserting every `agent_id` appears as `**agent_id**`. Every
//! agent added made the prompt longer, the digest lossier, and the test redder.
//!
//! The line that fits carries a description and drops the facts, and that is
//! how the navigator came to tell a user `biotech_analyst` has no
//! `model_ladder` (it has three rungs), is tier-agnostic (free resolves to
//! `openrouter/free`), and defaults to Haiku (it defaults to a Sonnet). Its
//! prompt entry had no model information and neither did the only tool it could
//! call, so the question was unanswerable from every source it had.
//!
//! # The invariant
//!
//! **This digest names categories and counts, never individual agents.** Adding
//! a hundred agents moves the counts and leaves the row count almost unchanged.
//!
//! That is the property that erodes, because the natural edit is *"and here are
//! the members"* — which silently restores the O(n) prompt this exists to
//! remove. [`tests::the_digest_names_no_individual_agent`] defends it, and it is
//! the single most important test in this file.

use std::collections::BTreeMap;

/// How many rows any one axis may contribute to the map.
///
/// # Why a cap and not a threshold
///
/// The first version had none, and the corpus showed why it needed one: the
/// cohort axis produced **47** rows, most of them two-agent groups, for a
/// 1,614-character map. Two agents out of 102 is the smallest group that
/// exists; it is real, and it is not shape.
///
/// A minimum group size would be the same magic number wearing different
/// clothes. The honest bound is the tier itself: **this is a prompt, not a
/// report.** A prompt has a budget, so the map is capped by construction rather
/// than by hoping the fleet's structure stays small. Everything past the cap is
/// a tool's answer — which is where per-member detail was always going to live.
///
/// This is also what makes the O(structure) claim true rather than hopeful.
/// Cloning agents does not add labels, so an uncapped map looked bounded under
/// that test; a fleet that grows by adding genuinely new asks would have grown
/// it without limit. `the_map_is_bounded_even_when_the_vocabulary_grows` is the
/// test that could see the difference.
pub const MAP_ROWS: usize = 12;

/// One row of the map: a category and how many agents are in it.
///
/// Deliberately not `(name, Vec<agent>)`. The members are a tool's answer, and
/// a struct that *can* hold them is a struct someone will fill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bucket {
    pub name: String,
    pub agents: usize,
}

/// What a meta agent needs to know about its fleet without knowing its fleet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digest {
    /// **The staleness anchor.** The number of agents this digest was built
    /// from.
    ///
    /// Load-bearing rather than decorative: a meta agent that compares this
    /// against a live count can tell its map is out of date and say so, instead
    /// of answering from it. A compressed map without this is the authored
    /// prompt again with fewer lines.
    pub agents: usize,
    /// `agent_type` census. The reliable spine: 13 values covering every card,
    /// and the vocabulary is curated rather than per-agent.
    pub types: Vec<Bucket>,
    /// Type-namespace census (`abw`, `fermi`, `scro`). Grows with products
    /// rather than agents. Thin today — only the typed cohort participates —
    /// and slow-growing, which is the property that matters.
    pub namespaces: Vec<Bucket>,
    /// Shared asks that genuinely narrow the fleet, from
    /// `port_trust::Substitutes::Cohort`. `Universal` labels are excluded
    /// because they exclude nothing, and `Bespoke` ones because a map of
    /// one-agent categories is the enumeration again.
    pub cohorts: Vec<Bucket>,
}

/// One agent, as the digest needs it. Nothing per-agent survives into the
/// output; this is the input shape only.
#[derive(Debug, Clone)]
pub struct CardFacts {
    pub agent_id: String,
    pub agent_type: String,
    /// `accepts` labels, for the cohort axis.
    pub accepts: Vec<String>,
    /// `produces` and `accepts` together — namespaces appear on both sides.
    pub port_labels: Vec<String>,
}

/// Build the map.
///
/// # Why `skills` is not an axis
///
/// It was the obvious candidate and it fails the only test that matters: 366
/// distinct skills across 102 cards, the modal skill shared by one agent. Its
/// cardinality tracks **membership**, not structure, so it would reproduce the
/// enumeration under a different heading. Anything with that shape belongs
/// behind a tool.
pub fn digest(cards: &[CardFacts]) -> Digest {
    let mut types: BTreeMap<&str, usize> = BTreeMap::new();
    let mut namespaces: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_label: BTreeMap<&str, usize> = BTreeMap::new();

    for c in cards {
        *types.entry(c.agent_type.as_str()).or_insert(0) += 1;

        // An agent counts once per namespace it touches, not once per label.
        let mut seen: Vec<&str> = Vec::new();
        for label in &c.port_labels {
            if let Some((ns, _)) = label.split_once('/') {
                if !seen.contains(&ns) {
                    seen.push(ns);
                    *namespaces.entry(ns).or_insert(0) += 1;
                }
            }
        }

        for label in &c.accepts {
            *by_label.entry(label.as_str()).or_insert(0) += 1;
        }
    }

    let corpus = cards.len();
    let cohorts = by_label
        .into_iter()
        .filter(|(_, n)| {
            matches!(
                crate::port_trust::substitutes(*n, corpus),
                crate::port_trust::Substitutes::Cohort(_)
            )
        })
        .map(|(name, agents)| Bucket {
            name: name.to_string(),
            agents,
        })
        .collect();

    let rank = |m: BTreeMap<&str, usize>| {
        let mut v: Vec<Bucket> = m
            .into_iter()
            .map(|(name, agents)| Bucket {
                name: name.to_string(),
                agents,
            })
            .collect();
        // Biggest first, then alphabetical, so the rendering is stable and the
        // reader meets the large categories first.
        v.sort_by(|a, b| b.agents.cmp(&a.agents).then(a.name.cmp(&b.name)));
        v.truncate(MAP_ROWS);
        v
    };

    Digest {
        agents: corpus,
        types: rank(types),
        namespaces: rank(namespaces),
        cohorts: {
            let mut c: Vec<Bucket> = cohorts;
            c.sort_by(|a, b| b.agents.cmp(&a.agents).then(a.name.cmp(&b.name)));
            c.truncate(MAP_ROWS);
            c
        },
    }
}

/// What the meta agent must be told it does **not** know.
///
/// Ships with the map or the map is dangerous. Compressing the fleet out of the
/// prompt raises confabulation risk — the `biotech_analyst` answer happened
/// because the source was absent and answering was still expected, and a
/// thinner prompt is a larger version of that gap.
///
/// A constant rather than prose in a card, so the sentence that closes the gap
/// cannot be edited away from the digest that opens it.
pub const WHAT_YOU_DO_NOT_KNOW: &str =
    "You know the fleet's SHAPE, not per-agent facts. Model, model_ladder, tier, \
     ports and tools come from `describe_agent`. Who answers a given ask comes \
     from `who_answers`. What has actually fed what comes from the run record. \
     Do not state any of them from memory: if a fact is not in the map above, \
     call the tool or say you need to look.";

impl Digest {
    /// Render for a system prompt.
    ///
    /// Counts and categories, one line per axis rather than per member, with
    /// the staleness anchor first and [`WHAT_YOU_DO_NOT_KNOW`] last. A reader
    /// meets the size of the fleet, its shape, and then the boundary of its own
    /// knowledge, in that order.
    pub fn render(&self) -> String {
        let row = |b: &[Bucket]| {
            b.iter()
                .map(|x| format!("{} {}", x.name, x.agents))
                .collect::<Vec<_>>()
                .join(" · ")
        };
        let mut out = format!(
            "FLEET MAP — {} agents, {} types, {} namespaces, {} shared asks.\n",
            self.agents,
            self.types.len(),
            self.namespaces.len(),
            self.cohorts.len()
        );
        out.push_str(&format!("  by type:      {}\n", row(&self.types)));
        out.push_str(&format!("  by namespace: {}\n", row(&self.namespaces)));
        out.push_str(&format!("  who answers:  {}\n", row(&self.cohorts)));
        out.push_str(&format!("\n{WHAT_YOU_DO_NOT_KNOW}\n"));
        out
    }

    /// Is this map still describing the fleet?
    ///
    /// The whole reason [`Digest::agents`] exists. `false` means the meta agent
    /// should look rather than answer.
    pub fn describes(&self, live_agents: usize) -> bool {
        self.agents == live_agents
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, ty: &str, accepts: &[&str], produces: &[&str]) -> CardFacts {
        let accepts: Vec<String> = accepts.iter().map(|s| s.to_string()).collect();
        let mut port_labels = accepts.clone();
        port_labels.extend(produces.iter().map(|s| s.to_string()));
        CardFacts {
            agent_id: id.to_string(),
            agent_type: ty.to_string(),
            accepts,
            port_labels,
        }
    }

    /// A fleet with the shape of the real one, small enough to reason about.
    fn fleet() -> Vec<CardFacts> {
        let mut v = vec![
            card("weather_oracle", "research", &["market-question"], &["fermi/weather_market_call"]),
            card("equity_analyst", "research", &["fermi/forecast-question/1", "ticker"], &["fermi/equity_evidence"]),
            card("macro_data_agent", "research", &["fermi/forecast-question/1"], &["fermi/socioeconomic_evidence"]),
            card("cohere_and_coordinate", "coordination", &["workspace-state"], &[]),
            card("coherence_consultant", "coherence", &["workspace-state"], &[]),
            card("genome_profiler", "research", &["abw/genome-query/1"], &["rabble/phylogenetic_profile"]),
            card("supply_chain_oracle", "commerce", &["scro/bom-query/1"], &["scro/bom_response"]),
        ];
        // Pad with `query`-accepting agents so `query` reads Universal, as it
        // does in the corpus. Without this the fixture cannot exercise the
        // exclusion that keeps the map useful.
        for i in 0..13 {
            v.push(card(&format!("padding_{i}"), "creative", &["query"], &[]));
        }
        v
    }

    /// **The invariant the whole module exists to hold.**
    ///
    /// The digest describes categories. The moment it names a member it is the
    /// 102-line prompt again under a new heading, and the failure is gradual:
    /// one helpful `Vec<String>` of members, then the prompt grows with the
    /// fleet once more and nothing says so.
    ///
    /// Asserted over the RENDERED output, because that is what reaches a model.
    /// A struct field nobody prints is not the risk.
    #[test]
    fn the_digest_names_no_individual_agent() {
        let cards = fleet();
        let rendered = digest(&cards).render();
        for c in &cards {
            assert!(
                !rendered.contains(&c.agent_id),
                "the digest names `{}`. It must describe categories and counts \
                 only — members are a tool's answer. Naming one agent is how \
                 this becomes an enumeration again, one helpful edit at a time.",
                c.agent_id
            );
        }
    }

    /// **Doubling the fleet must not double the map.**
    ///
    /// The scaling claim, measured rather than asserted in prose. Growth is
    /// allowed — a genuinely more varied fleet has more structure — but it must
    /// track structure, not membership.
    #[test]
    fn the_map_grows_with_structure_not_membership() {
        let small = fleet();
        let mut large = fleet();
        // Twice the agents, same shape: more members in existing categories.
        for i in 0..small.len() {
            let mut extra = small[i].clone();
            extra.agent_id = format!("clone_{i}");
            large.push(extra);
        }

        let a = digest(&small);
        let b = digest(&large);

        assert_eq!(b.agents, a.agents * 2, "the fixture did not double");
        assert_eq!(
            b.types.len(),
            a.types.len(),
            "the type axis grew when only membership changed"
        );
        assert_eq!(
            b.namespaces.len(),
            a.namespaces.len(),
            "the namespace axis grew when only membership changed"
        );

        let (short, long) = (a.render().lines().count(), b.render().lines().count());
        assert_eq!(
            short, long,
            "the rendered map is {long} lines for twice the fleet and {short} \
             for the original. Line count must track the number of CATEGORIES, \
             which did not change here."
        );
    }

    /// A universal label is not a category worth mapping.
    ///
    /// `query` is accepted by a quarter of the real corpus. Putting it in the
    /// map tells a navigator that two dozen agents are interchangeable for any
    /// ask, which is true and useless, and it crowds out the labels that do
    /// narrow the fleet.
    #[test]
    fn the_map_omits_the_calling_convention_and_keeps_the_cohorts() {
        let d = digest(&fleet());
        let names: Vec<&str> = d.cohorts.iter().map(|c| c.name.as_str()).collect();

        assert!(
            !names.contains(&"query"),
            "`query` is in the map. It is the platform's calling convention, \
             not a specialisation: {names:?}"
        );
        assert!(
            names.contains(&"workspace-state"),
            "`workspace-state` is missing. Two agents share it and both are \
             coordination-shaped, which is exactly the structure the map is \
             for: {names:?}"
        );
        assert!(
            !names.contains(&"abw/genome-query/1"),
            "a single-agent label is in the map. A category of one is the \
             enumeration wearing a heading: {names:?}"
        );
    }

    /// The map has to be able to say it is out of date.
    ///
    /// Without this the digest is the authored prompt again with fewer lines:
    /// confidently describing a fleet that has moved, with nothing to notice.
    #[test]
    fn a_stale_map_can_say_so() {
        let d = digest(&fleet());
        assert!(d.describes(d.agents));
        assert!(
            !d.describes(d.agents + 1),
            "a digest built from {} agents reports that it still describes a \
             fleet of {}. Staleness has to be detectable by the agent holding \
             the map, or a thinner prompt is strictly more dangerous than the \
             long one.",
            d.agents,
            d.agents + 1
        );
    }

    /// The boundary of knowledge travels with the map.
    #[test]
    fn the_render_says_what_the_agent_does_not_know() {
        let rendered = digest(&fleet()).render();
        assert!(
            rendered.contains(WHAT_YOU_DO_NOT_KNOW),
            "the rendered map does not carry `WHAT_YOU_DO_NOT_KNOW`. \
             Compressing the fleet out of the prompt raises the chance of an \
             invented answer; the sentence naming where to look is what makes \
             the compression safe rather than merely cheaper."
        );
        for tool in ["describe_agent", "who_answers"] {
            assert!(
                rendered.contains(tool),
                "the map does not name `{tool}`. If the platform can name what \
                 would close a gap, the name is the control — a boundary with \
                 no route past it is just a disclaimer."
            );
        }
    }

    /// Every axis is ordered biggest-first and stable.
    #[test]
    fn the_axes_are_ordered_so_a_reader_meets_the_large_categories_first() {
        let d = digest(&fleet());
        for axis in [&d.types, &d.namespaces, &d.cohorts] {
            let counts: Vec<usize> = axis.iter().map(|b| b.agents).collect();
            let mut sorted = counts.clone();
            sorted.sort_unstable_by(|a, b| b.cmp(a));
            assert_eq!(counts, sorted, "an axis is not ordered biggest-first");
        }
        // Stable across calls: a prompt that reshuffles invalidates its own cache.
        assert_eq!(digest(&fleet()), digest(&fleet()));
    }

    /// **The test that could see what cloning could not.**
    ///
    /// `the_map_grows_with_structure_not_membership` doubles the fleet by
    /// duplicating agents, so it adds no new labels and an uncapped map passes
    /// it comfortably. That is exactly how the cohort axis reached 47 rows on
    /// the real corpus while a scaling test stayed green.
    ///
    /// This one grows the fleet the way a fleet actually grows: new agents
    /// bringing new asks. Every one of them is a legitimate two-agent cohort,
    /// so nothing here is a defect to be filtered out — there are simply more
    /// of them than a prompt can carry, which is what `MAP_ROWS` is for.
    #[test]
    fn the_map_is_bounded_even_when_the_vocabulary_grows() {
        let mut cards = fleet();
        // 200 new agents, each pair sharing a brand-new ask.
        for i in 0..100 {
            let ask = format!("novel-ask-{i}");
            for half in 0..2 {
                cards.push(card(
                    &format!("grown_{i}_{half}"),
                    "research",
                    &[ask.as_str()],
                    &[],
                ));
            }
        }

        let d = digest(&cards);
        assert!(
            d.cohorts.len() > 5,
            "the fixture did not produce cohorts; it cannot test the bound"
        );
        for (axis, rows) in [
            ("types", d.types.len()),
            ("namespaces", d.namespaces.len()),
            ("cohorts", d.cohorts.len()),
        ] {
            assert!(
                rows <= MAP_ROWS,
                "the `{axis}` axis contributed {rows} rows to the map, over the \
                 {MAP_ROWS}-row budget. A map that grows with the fleet's \
                 vocabulary is the enumerated prompt again, arriving one new \
                 shared ask at a time."
            );
        }

        // And the rendered size is bounded, which is the thing that actually
        // costs tokens on every invocation.
        let rendered = d.render();
        assert!(
            rendered.len() < 2_000,
            "the map rendered to {} characters for a fleet of {}. The prompt \
             tier has to be a fixed cost or the whole pattern collapses back \
             into the O(n) prompt it replaced.",
            rendered.len(),
            d.agents
        );

        // The staleness anchor still tracks the real size, which is the one
        // number that MUST scale.
        assert_eq!(d.agents, cards.len());
    }
}
