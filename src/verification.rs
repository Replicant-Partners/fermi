//! # A verification corpus — turning a liability into a curriculum
//!
//! ## Why this exists
//!
//! Hodgson et al. 2023 (PMID 36794335) measured three phone apps against 78
//! expert-confirmed specimens: best accuracy 49%, iNaturalist 35%, death caps
//! misidentified by two of the three. For an app that tells people what is safe
//! to eat, that number is disqualifying.
//!
//! For an app that teaches people how unreliable photographic identification is,
//! **it is the curriculum.**
//!
//! That reframing is not a marketing move; it changes what the software is for,
//! and it closes three gaps this codebase had written down as open:
//!
//! | Gap, as previously recorded | What an expert determination provides |
//! | --- | --- |
//! | "No accuracy figure exists for this system" | Ground truth to measure against |
//! | "A curated, citable lookalike source is what would make this advisory" | Confusion pairs, accumulated from real misses |
//! | `forage_identify.taxonomy` cross-check: *"the check that would mean something is agreement between two independent determiners on the same frame — a capability decision, not a missing JOIN"* | Exactly that second determiner |
//!
//! The third is the one worth pausing on. That exemption was written as a
//! permanent structural limit: no platform record knows what a person
//! photographed. An expert determination **is** such a record. The limit was
//! never structural; it was an absence of people.
//!
//! ## What this module is and is not
//!
//! It is the domain layer: what a submission is, what a determination is, which
//! determinations can settle a record, and how to measure a model against the
//! ones that are settled. Pure, no I/O, no database.
//!
//! It is **not** the queue, the endpoints, the moderation model, the notification
//! path, or the persistence. Those are named in
//! `docs/specs/VERIFICATION_CORPUS.md` and deliberately not built here.
//!
//! ## The rule that matters most
//!
//! > **Agreement is not corroboration.**
//!
//! Five people saying "chanterelle" without citing anything is five uncited
//! opinions, and it resolves to [`PROV_HUMAN_ENDORSED`] — the same strength as a
//! model's guess. Only a determination that names what it was checked against
//! reaches [`PROV_HUMAN_SOURCED`], because the ladder measures reproducibility
//! and nothing else. `grounding_trust` puts it plainly: a one-click "verified"
//! button is how a queue becomes a laundering UI.
//!
//! This is the same doctrine `vote_strategist` applies to agents — that voters
//! sharing a model may be exhibiting correlation rather than corroboration — and
//! it applies harder to people, who read each other's answers.

use chrono::{DateTime, Utc};

use crate::grounding_trust::{
    PROV_HUMAN_ENDORSED, PROV_HUMAN_SOURCED, PROV_INFERRED, PROV_PENDING_HUMAN, PROV_REJECTED,
};

// ─── determinations ────────────────────────────────────────────────────

/// Who made a determination, and on what basis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Determiner {
    /// A model reading a photograph. Never ground truth: it is the thing being
    /// measured, and scoring it against itself would be circular.
    Model { model_id: String },
    /// A community member. May be highly skilled; the platform has no way to
    /// know, so it does not pretend to.
    Community { user_id: String },
    /// Someone with a recorded standing to determine — a mycological society, a
    /// herbarium, a named specialist.
    ///
    /// `credential` is free text on purpose. A structured credential registry is
    /// a governance problem, and inventing a schema for it here would encode a
    /// hierarchy nobody agreed to.
    Expert { user_id: String, credential: String },
}

/// One person's or model's answer about one specimen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Determination {
    pub determiner: Determiner,
    /// Scientific name, at whatever rank the determiner could actually support.
    /// `None` means they looked and declined — which is a real answer and is
    /// counted as one.
    pub taxon: Option<String>,
    /// The rank the name reaches: `species`, `genus`, `family`.
    pub rank: Option<String>,
    /// What this was checked against: a key and edition, a herbarium accession, a
    /// sequencing result, a spore-print observation.
    ///
    /// **The citation is what earns strength 2.** Without it, an expert's answer
    /// is an expert's opinion, which is worth having and is not reproducible.
    pub citation: Option<String>,
    /// What the determiner looked at, in their words. The most useful field in
    /// the corpus for teaching: it is the reasoning a learner can compare
    /// against.
    pub notes: Option<String>,
    pub at: DateTime<Utc>,
}

impl Determination {
    /// Provenance this determination alone would justify.
    pub fn provenance(&self) -> &'static str {
        match (&self.determiner, self.citation.as_deref()) {
            // A citation someone else can follow to the same answer.
            (Determiner::Expert { .. }, Some(c)) if !c.trim().is_empty() => PROV_HUMAN_SOURCED,
            // Standing without a citation is an opinion. Deliberately the same
            // rung as a model's judgement: deference to a title is what the
            // grounding ladder exists to remove.
            (Determiner::Expert { .. }, _) => PROV_HUMAN_ENDORSED,
            // A community citation is still a citation — the ladder measures
            // reproducibility, not standing. Someone who names the key they used
            // has done the checkable thing whether or not anyone credentialed
            // them.
            (Determiner::Community { .. }, Some(c)) if !c.trim().is_empty() => PROV_HUMAN_SOURCED,
            (Determiner::Community { .. }, _) => PROV_HUMAN_ENDORSED,
            (Determiner::Model { .. }, _) => PROV_INFERRED,
        }
    }

    /// Is this a determination the corpus may treat as ground truth?
    ///
    /// Models never are. Everything else is, at the strength its citation earns.
    pub fn can_settle(&self) -> bool {
        !matches!(self.determiner, Determiner::Model { .. })
    }
}

// ─── how two names compare ─────────────────────────────────────────────

/// How closely a model's answer matched the settled one.
///
/// Not a boolean, because in this domain the near misses differ enormously in
/// consequence. Getting the genus right and the species wrong is harmless for
/// *Cantharellus* and potentially fatal for *Amanita*, so collapsing both into
/// "incorrect" would discard the distinction that matters and inflate a headline
/// accuracy figure with agreements that would not have kept anyone safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchLevel {
    /// Same binomial.
    Exact,
    /// Same genus, different species.
    Genus,
    /// Neither.
    Wrong,
    /// The model declined, or the record is unsettled. Not a failure, and not a
    /// success — excluded from accuracy rather than counted as either.
    Undetermined,
}

/// Compare two scientific names.
///
/// Deliberately crude: case-insensitive, authority suffixes ignored, first two
/// tokens compared. It does **not** resolve synonyms, so *Agaricus chantarellus*
/// and *Cantharellus cibarius* read as `Wrong` when they are the same fungus.
/// That is a known undercount and is recorded rather than papered over — fixing
/// it means routing both names through GBIF's accepted-name view first, which is
/// a lookup this pure function deliberately cannot do.
pub fn compare_names(model: Option<&str>, settled: Option<&str>) -> MatchLevel {
    let (Some(a), Some(b)) = (model, settled) else {
        return MatchLevel::Undetermined;
    };
    let norm = |s: &str| -> Vec<String> {
        s.split_whitespace()
            .take(2)
            .map(|t| {
                t.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .filter(|t| !t.is_empty())
            .collect()
    };
    let (a, b) = (norm(a), norm(b));
    if a.is_empty() || b.is_empty() {
        return MatchLevel::Undetermined;
    }
    if a == b {
        return MatchLevel::Exact;
    }
    if a[0] == b[0] {
        return MatchLevel::Genus;
    }
    MatchLevel::Wrong
}

// ─── a record ──────────────────────────────────────────────────────────

/// One submitted specimen and everything said about it.
#[derive(Debug, Clone)]
pub struct VerificationRecord {
    pub record_id: String,
    /// Where the image lives. Not fetched here.
    pub image_ref: String,
    /// Free-text locality as the submitter gave it. Not a coordinate: a precise
    /// location for a rare or over-collected species is a conservation risk, and
    /// this module should not be the reason a patch gets stripped.
    pub locality: Option<String>,
    pub determinations: Vec<Determination>,
    pub submitted_at: DateTime<Utc>,
}

/// What a record has settled on, if anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub taxon: Option<String>,
    /// Strongest provenance any settling determination earned.
    pub provenance: &'static str,
    /// How many non-model determinations agreed with `taxon` at species level.
    pub concurring: usize,
    /// How many disagreed. Non-zero is not a problem to hide: a contested
    /// specimen is the most instructive item in the corpus.
    pub dissenting: usize,
}

impl VerificationRecord {
    /// The record's current state, as a provenance verdict.
    ///
    /// `pending_human_check` while only a model has spoken — the honest label for
    /// "queued, nobody has looked yet", and distinct from `unavailable` because a
    /// person *can* answer this.
    pub fn state(&self) -> &'static str {
        match self.resolve() {
            Some(r) => r.provenance,
            None => PROV_PENDING_HUMAN,
        }
    }

    /// The model's determination, if one was recorded.
    pub fn model_determination(&self) -> Option<&Determination> {
        self.determinations
            .iter()
            .find(|d| matches!(d.determiner, Determiner::Model { .. }))
    }

    /// Settle the record, if anyone qualified has spoken.
    ///
    /// The majority taxon among settling determinations wins, and the resolution
    /// carries the **strongest** provenance among those that agreed with it — not
    /// the strongest in the record. A cited expert who dissents does not lend
    /// their citation to the answer they argued against.
    ///
    /// Ties resolve to the determination with the strongest provenance, then to
    /// the earliest. Deterministic, and the tie is reported through
    /// `dissenting` rather than hidden.
    pub fn resolve(&self) -> Option<Resolution> {
        let settling: Vec<&Determination> = self
            .determinations
            .iter()
            .filter(|d| d.can_settle())
            .collect();
        if settling.is_empty() {
            return None;
        }

        // Tally by normalised binomial.
        let mut tally: Vec<(Option<String>, usize, &'static str, DateTime<Utc>)> = Vec::new();
        for d in &settling {
            let key = d.taxon.as_deref().map(|t| {
                t.split_whitespace()
                    .take(2)
                    .map(|w| w.to_lowercase())
                    .collect::<Vec<_>>()
                    .join(" ")
            });
            match tally.iter_mut().find(|(k, _, _, _)| *k == key) {
                Some(entry) => {
                    entry.1 += 1;
                    if strength_of(d.provenance()) > strength_of(entry.2) {
                        entry.2 = d.provenance();
                    }
                    if d.at < entry.3 {
                        entry.3 = d.at;
                    }
                }
                None => tally.push((key, 1, d.provenance(), d.at)),
            }
        }

        tally.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then(strength_of(b.2).cmp(&strength_of(a.2)))
                .then(a.3.cmp(&b.3))
        });
        let (key, concurring, provenance, _) = tally[0].clone();

        // Report the winner using an original spelling rather than the
        // lowercased tally key, so the corpus keeps the determiner's casing.
        let taxon = settling
            .iter()
            .find(|d| {
                d.taxon.as_deref().map(|t| {
                    t.split_whitespace()
                        .take(2)
                        .map(|w| w.to_lowercase())
                        .collect::<Vec<_>>()
                        .join(" ")
                }) == key
            })
            .and_then(|d| d.taxon.clone());

        Some(Resolution {
            taxon,
            provenance,
            concurring,
            dissenting: settling.len() - concurring,
        })
    }
}

fn strength_of(p: &str) -> u8 {
    crate::grounding_trust::strength(p)
}

// ─── measuring the model ───────────────────────────────────────────────

/// An accuracy figure that cannot be quoted without its interval.
///
/// The whole reason this type exists rather than a bare `f64`: Hodgson's best
/// result is **49% with a 95% CI of [0-100]**. The point estimate alone is
/// nearly uninformative, and it is exactly the number a summary would repeat.
/// [`AccuracyEstimate::headline`] always emits n and the bounds.
#[derive(Debug, Clone, PartialEq)]
pub struct AccuracyEstimate {
    /// Records that could be scored: settled, with a model determination that
    /// did not decline.
    pub n: usize,
    pub successes: usize,
    pub point: f64,
    /// Wilson score interval, 95%. Wilson rather than normal-approximation
    /// because at small n and extreme proportions the normal interval runs
    /// outside [0,1] and reports impossible bounds.
    pub low: f64,
    pub high: f64,
}

/// Below this many scored records, a point estimate is not reported as a figure.
///
/// Not a statistical threshold — there is no n at which an estimate becomes
/// true. It is an editorial one: under 30, the interval is wide enough that
/// quoting a percentage invites a reader to believe the percentage rather than
/// the interval, and this project has already found one AI-generated page and one
/// peer-reviewed abstract doing precisely that.
pub const MIN_N_FOR_HEADLINE: usize = 30;

impl AccuracyEstimate {
    /// Wilson score interval at 95%.
    pub fn wilson(successes: usize, n: usize) -> Self {
        if n == 0 {
            return Self {
                n: 0,
                successes: 0,
                point: f64::NAN,
                low: 0.0,
                high: 1.0,
            };
        }
        let z = 1.959_963_984_540_054_f64; // 97.5th percentile of the standard normal
        let nf = n as f64;
        let p = successes as f64 / nf;
        let z2 = z * z;
        let denom = 1.0 + z2 / nf;
        let centre = (p + z2 / (2.0 * nf)) / denom;
        let margin = (z / denom) * (p * (1.0 - p) / nf + z2 / (4.0 * nf * nf)).sqrt();
        Self {
            n,
            successes,
            point: p,
            low: (centre - margin).max(0.0),
            high: (centre + margin).min(1.0),
        }
    }

    /// Is there enough here to state a percentage at all?
    pub fn is_reportable(&self) -> bool {
        self.n >= MIN_N_FOR_HEADLINE
    }

    /// The only sanctioned way to render this. Always carries n and the bounds.
    pub fn headline(&self) -> String {
        if self.n == 0 {
            return "no scored records yet — no accuracy figure exists".to_string();
        }
        if !self.is_reportable() {
            return format!(
                "insufficient evidence: {}/{} correct, 95% CI [{:.0}%–{:.0}%]. \
                 Under {MIN_N_FOR_HEADLINE} scored records this is not reported as a \
                 percentage, because the interval is wide enough that the point \
                 estimate would be believed instead of it.",
                self.successes,
                self.n,
                self.low * 100.0,
                self.high * 100.0
            );
        }
        format!(
            "{:.0}% (95% CI {:.0}%–{:.0}%, n={})",
            self.point * 100.0,
            self.low * 100.0,
            self.high * 100.0,
            self.n
        )
    }
}

/// Accumulated submissions.
#[derive(Debug, Clone, Default)]
pub struct Corpus {
    pub records: Vec<VerificationRecord>,
}

/// What a corpus can say about a model's photographic identification.
#[derive(Debug, Clone)]
pub struct CorpusReport {
    pub total_records: usize,
    /// Awaiting a human. The queue depth, and the thing that limits everything
    /// else.
    pub pending: usize,
    /// Settled by a determination carrying a citation.
    pub cited: usize,
    /// Settled only by uncited opinion.
    pub endorsed_only: usize,
    /// Settled records where determiners disagreed. The most instructive
    /// subset, and the seed of a confusion-pair table.
    pub contested: usize,
    /// Exact-binomial agreement with the settled answer.
    pub exact: AccuracyEstimate,
    /// Agreement at genus level or better. Reported separately because the
    /// consequence of a genus-level near miss is species-dependent and can be
    /// fatal.
    pub genus_or_better: AccuracyEstimate,
    /// Pairs (model said, expert said) where they differed. Raw material for the
    /// lookalike source three agents currently return null for.
    pub confusions: Vec<(String, String)>,
}

impl Corpus {
    pub fn report(&self) -> CorpusReport {
        let mut pending = 0;
        let mut cited = 0;
        let mut endorsed_only = 0;
        let mut contested = 0;
        let mut scored = 0;
        let mut exact = 0;
        let mut genus_or_better = 0;
        let mut confusions: Vec<(String, String)> = Vec::new();

        for r in &self.records {
            let Some(res) = r.resolve() else {
                pending += 1;
                continue;
            };
            if res.provenance == PROV_HUMAN_SOURCED {
                cited += 1;
            } else if res.provenance == PROV_HUMAN_ENDORSED {
                endorsed_only += 1;
            }
            if res.dissenting > 0 {
                contested += 1;
            }

            let Some(model) = r.model_determination() else {
                continue;
            };
            match compare_names(model.taxon.as_deref(), res.taxon.as_deref()) {
                MatchLevel::Undetermined => {}
                MatchLevel::Exact => {
                    scored += 1;
                    exact += 1;
                    genus_or_better += 1;
                }
                MatchLevel::Genus => {
                    scored += 1;
                    genus_or_better += 1;
                    if let (Some(m), Some(s)) = (&model.taxon, &res.taxon) {
                        confusions.push((m.clone(), s.clone()));
                    }
                }
                MatchLevel::Wrong => {
                    scored += 1;
                    if let (Some(m), Some(s)) = (&model.taxon, &res.taxon) {
                        confusions.push((m.clone(), s.clone()));
                    }
                }
            }
        }

        CorpusReport {
            total_records: self.records.len(),
            pending,
            cited,
            endorsed_only,
            contested,
            exact: AccuracyEstimate::wilson(exact, scored),
            genus_or_better: AccuracyEstimate::wilson(genus_or_better, scored),
            confusions,
        }
    }
}

/// A record that was checked and found wrong keeps [`PROV_REJECTED`] rather than
/// being deleted.
///
/// Re-exported so callers do not reach for a bespoke string. `grounding_trust`'s
/// reasoning applies unchanged: a rejection rate is the first quality signal on
/// this platform that is not self-reported, and deleting the misses would make
/// the corpus flatter to look at.
pub const REJECTED: &str = PROV_REJECTED;

#[cfg(test)]
mod tests {
    use super::*;

    fn t(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000 + secs, 0).expect("timestamp")
    }

    fn model(taxon: Option<&str>) -> Determination {
        Determination {
            determiner: Determiner::Model {
                model_id: "claude-sonnet-4-5".into(),
            },
            taxon: taxon.map(str::to_string),
            rank: Some("species".into()),
            citation: None,
            notes: None,
            at: t(0),
        }
    }

    fn expert(taxon: &str, citation: Option<&str>, secs: i64) -> Determination {
        Determination {
            determiner: Determiner::Expert {
                user_id: "u1".into(),
                credential: "Royal Botanic Gardens Victoria".into(),
            },
            taxon: Some(taxon.to_string()),
            rank: Some("species".into()),
            citation: citation.map(str::to_string),
            notes: None,
            at: t(secs),
        }
    }

    fn community(taxon: &str, citation: Option<&str>, user: &str, secs: i64) -> Determination {
        Determination {
            determiner: Determiner::Community {
                user_id: user.into(),
            },
            taxon: Some(taxon.to_string()),
            rank: Some("species".into()),
            citation: citation.map(str::to_string),
            notes: None,
            at: t(secs),
        }
    }

    fn record(id: &str, dets: Vec<Determination>) -> VerificationRecord {
        VerificationRecord {
            record_id: id.into(),
            image_ref: "ws://photo.jpg".into(),
            locality: Some("Sherbrooke Forest".into()),
            determinations: dets,
            submitted_at: t(0),
        }
    }

    // ─── the rule that matters most ────────────────────────────────────

    /// **Agreement is not corroboration.**
    ///
    /// Five uncited agreements are five opinions. Only a citation reaches
    /// strength 2, because the ladder measures reproducibility and nothing else.
    #[test]
    fn consensus_without_citations_does_not_manufacture_certainty() {
        let r = record(
            "r1",
            vec![
                model(Some("Cantharellus cibarius")),
                community("Cantharellus cibarius", None, "a", 1),
                community("Cantharellus cibarius", None, "b", 2),
                community("Cantharellus cibarius", None, "c", 3),
                community("Cantharellus cibarius", None, "d", 4),
                community("Cantharellus cibarius", None, "e", 5),
            ],
        );
        let res = r.resolve().expect("settled");
        assert_eq!(res.concurring, 5);
        assert_eq!(
            res.provenance, PROV_HUMAN_ENDORSED,
            "five uncited agreements were promoted to a sourced verdict"
        );
        assert!(strength_of(res.provenance) < 2);
    }

    /// A credential without a citation is an opinion. Deference to a title is
    /// what the grounding ladder exists to remove.
    #[test]
    fn an_uncited_expert_is_an_endorsement_not_a_source() {
        let r = record("r2", vec![expert("Amanita phalloides", None, 1)]);
        assert_eq!(r.resolve().unwrap().provenance, PROV_HUMAN_ENDORSED);
    }

    #[test]
    fn a_cited_determination_reaches_human_sourced() {
        let r = record(
            "r3",
            vec![expert(
                "Amanita phalloides",
                Some("Fungi of Southern Australia, key 4b; spore print white"),
                1,
            )],
        );
        let res = r.resolve().unwrap();
        assert_eq!(res.provenance, PROV_HUMAN_SOURCED);
        assert_eq!(strength_of(res.provenance), 2);
    }

    /// The ladder measures reproducibility, not standing. Someone who names the
    /// key they used has done the checkable thing whether or not anyone
    /// credentialed them.
    #[test]
    fn a_cited_community_member_outranks_an_uncited_expert() {
        let cited = community("Cantharellus cibarius", Some("Phillips, plate 212"), "a", 1);
        let uncited = expert("Cantharellus cibarius", None, 2);
        assert_eq!(cited.provenance(), PROV_HUMAN_SOURCED);
        assert_eq!(uncited.provenance(), PROV_HUMAN_ENDORSED);
        assert!(strength_of(cited.provenance()) > strength_of(uncited.provenance()));
    }

    /// A model can never settle its own score.
    #[test]
    fn a_model_cannot_be_ground_truth() {
        let r = record("r4", vec![model(Some("Cantharellus cibarius"))]);
        assert!(r.resolve().is_none(), "the model settled the record");
        assert_eq!(r.state(), PROV_PENDING_HUMAN);
        assert!(!model(Some("x")).can_settle());
    }

    /// Queued is not unanswerable. A person *can* answer this, which is why the
    /// state is `pending_human_check` and not `unavailable_no_tool_source`.
    #[test]
    fn an_unsettled_record_is_pending_not_unavailable() {
        let r = record("r5", vec![model(Some("x"))]);
        assert_eq!(r.state(), PROV_PENDING_HUMAN);
        assert_ne!(
            r.state(),
            crate::grounding_trust::PROV_UNAVAILABLE,
            "a queued specimen was reported as unanswerable"
        );
    }

    /// A dissenting expert does not lend their citation to the answer they
    /// argued against.
    #[test]
    fn a_dissenters_citation_does_not_strengthen_the_majority() {
        let r = record(
            "r6",
            vec![
                community("Cantharellus cibarius", None, "a", 1),
                community("Cantharellus cibarius", None, "b", 2),
                expert(
                    "Hygrophoropsis aurantiaca",
                    Some("key 7a, forking gills"),
                    3,
                ),
            ],
        );
        let res = r.resolve().unwrap();
        assert_eq!(res.concurring, 2);
        assert_eq!(res.dissenting, 1);
        assert_eq!(
            res.provenance, PROV_HUMAN_ENDORSED,
            "the majority borrowed the dissenter's citation"
        );
    }

    // ─── comparison ────────────────────────────────────────────────────

    #[test]
    fn names_compare_at_the_level_that_matters() {
        assert_eq!(
            compare_names(
                Some("Cantharellus cibarius"),
                Some("Cantharellus cibarius Fr.")
            ),
            MatchLevel::Exact,
            "an authority suffix broke an exact match"
        );
        assert_eq!(
            compare_names(Some("Amanita phalloides"), Some("Amanita muscaria")),
            MatchLevel::Genus
        );
        assert_eq!(
            compare_names(Some("Cantharellus cibarius"), Some("Amanita phalloides")),
            MatchLevel::Wrong
        );
        assert_eq!(
            compare_names(None, Some("Amanita phalloides")),
            MatchLevel::Undetermined
        );
        assert_eq!(compare_names(Some("x"), None), MatchLevel::Undetermined);
    }

    /// A declined determination is neither right nor wrong, and must not be
    /// counted as either. Scoring "I don't know" as a miss would push the model
    /// toward guessing, which is the opposite of what this corpus is for.
    #[test]
    fn declining_is_excluded_from_accuracy_rather_than_counted_wrong() {
        let c = Corpus {
            records: vec![record(
                "r7",
                vec![model(None), expert("Amanita phalloides", Some("key 4b"), 1)],
            )],
        };
        let rep = c.report();
        assert_eq!(rep.exact.n, 0, "a declined determination was scored");
        assert_eq!(rep.cited, 1);
    }

    // ─── the number, and its interval ──────────────────────────────────

    /// Wilson, checked against the paper's own n and proportion.
    ///
    /// 49% of 78 gives roughly [38%, 60%] — noticeably narrower than the
    /// published [0-100], because Hodgson accounts for three independent raters
    /// per specimen and this does not. Recorded so nobody reads our tighter
    /// interval as an improvement on their statistics.
    #[test]
    fn the_interval_is_wilson_and_stays_inside_zero_to_one() {
        let e = AccuracyEstimate::wilson(38, 78);
        assert!((e.point - 0.487).abs() < 0.01, "point was {}", e.point);
        assert!(e.low > 0.37 && e.low < 0.40, "low was {}", e.low);
        assert!(e.high > 0.58 && e.high < 0.61, "high was {}", e.high);

        // The reason Wilson: a normal approximation at the extremes reports
        // bounds outside the possible range.
        let all = AccuracyEstimate::wilson(40, 40);
        assert!(all.high <= 1.0 && all.low > 0.8, "{all:?}");
        let none = AccuracyEstimate::wilson(0, 40);
        assert!(none.low >= 0.0 && none.high < 0.2, "{none:?}");
    }

    /// **The point estimate is never quotable alone.**
    ///
    /// Hodgson's best figure is 49% with a CI of [0-100]. `headline` always
    /// carries n and the bounds, so a summary cannot repeat the percentage
    /// without them.
    #[test]
    fn a_headline_always_carries_its_interval_and_n() {
        let e = AccuracyEstimate::wilson(20, 40);
        let h = e.headline();
        assert!(h.contains("CI"), "no interval in `{h}`");
        assert!(h.contains("n=40"), "no n in `{h}`");
        assert!(h.contains('%'), "no percentage in `{h}`");
    }

    /// Below the threshold, no percentage is stated at all.
    #[test]
    fn a_thin_corpus_refuses_to_state_a_percentage() {
        let e = AccuracyEstimate::wilson(2, 3);
        assert!(!e.is_reportable());
        let h = e.headline();
        assert!(h.contains("insufficient evidence"), "`{h}`");
        assert!(
            h.contains("2/3"),
            "the raw counts should still be visible: `{h}`"
        );
        assert!(!h.starts_with("67%"), "a percentage led the string: `{h}`");
    }

    #[test]
    fn an_empty_corpus_says_no_figure_exists() {
        let rep = Corpus::default().report();
        assert_eq!(rep.total_records, 0);
        assert_eq!(rep.exact.n, 0);
        assert!(rep.exact.headline().contains("no accuracy figure exists"));
    }

    // ─── what the corpus accumulates ───────────────────────────────────

    /// Misses become the confusion pairs that three agents currently return
    /// `null` for. This is the corpus paying for itself.
    #[test]
    fn misses_accumulate_as_confusion_pairs() {
        let c = Corpus {
            records: vec![
                record(
                    "a",
                    vec![
                        model(Some("Cantharellus cibarius")),
                        expert("Hygrophoropsis aurantiaca", Some("key 7a"), 1),
                    ],
                ),
                record(
                    "b",
                    vec![
                        model(Some("Agaricus campestris")),
                        expert("Amanita phalloides", Some("key 4b; white spore print"), 1),
                    ],
                ),
            ],
        };
        let rep = c.report();
        assert_eq!(rep.exact.successes, 0);
        assert_eq!(rep.exact.n, 2);
        assert_eq!(rep.confusions.len(), 2);
        assert!(
            rep.confusions
                .iter()
                .any(|(m, s)| m.contains("Agaricus") && s.contains("phalloides")),
            "the death-cap confusion was not recorded: {:?}",
            rep.confusions
        );
    }

    #[test]
    fn the_report_separates_cited_from_merely_endorsed() {
        let c = Corpus {
            records: vec![
                record(
                    "a",
                    vec![model(Some("x")), expert("Boletus edulis", Some("key 2"), 1)],
                ),
                record(
                    "b",
                    vec![model(Some("y")), expert("Boletus edulis", None, 1)],
                ),
                record("c", vec![model(Some("z"))]),
            ],
        };
        let rep = c.report();
        assert_eq!(rep.cited, 1);
        assert_eq!(rep.endorsed_only, 1);
        assert_eq!(rep.pending, 1);
        assert_eq!(rep.total_records, 3);
    }

    /// Genus-level near misses are counted separately, because their consequence
    /// is species-dependent: harmless in *Cantharellus*, potentially fatal in
    /// *Amanita*. Folding them into "correct" would inflate the headline with
    /// agreements that would not have kept anyone safe.
    #[test]
    fn genus_matches_are_counted_apart_from_exact_ones() {
        let c = Corpus {
            records: vec![record(
                "a",
                vec![
                    model(Some("Amanita muscaria")),
                    expert("Amanita phalloides", Some("key 4b"), 1),
                ],
            )],
        };
        let rep = c.report();
        assert_eq!(
            rep.exact.successes, 0,
            "a genus-only match counted as exact"
        );
        assert_eq!(rep.genus_or_better.successes, 1);
        assert_eq!(rep.confusions.len(), 1);
    }

    /// Contested records are surfaced, not smoothed. A specimen experts disagree
    /// about is the most instructive item in the corpus.
    #[test]
    fn contested_records_are_reported() {
        let c = Corpus {
            records: vec![record(
                "a",
                vec![
                    model(Some("Cantharellus cibarius")),
                    expert("Cantharellus cibarius", Some("key 7b"), 1),
                    community("Hygrophoropsis aurantiaca", None, "b", 2),
                ],
            )],
        };
        assert_eq!(c.report().contested, 1);
    }
}
