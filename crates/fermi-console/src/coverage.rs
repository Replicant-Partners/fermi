//! Who is answering this forecast, and is that a choice or a shortage?
//!
//! GPUI-free by construction so it can be tested with
//! `cargo test -p fermi-console --lib` in seconds.
//!
//! ═══════════════════════════════════════════════════════════════════
//!
//! [`routing`] answers "which agent for this driver?" one driver at a
//! time. That is necessary and not sufficient. A decomposition is a TEAM,
//! and a team has properties no single assignment has: whether it is
//! staffed by experts or by stand-ins, whether one agent is answering
//! everything, and whether the roster has a hole where this question's
//! domain should be.
//!
//! None of that was visible. Every driver's assignment was announced with
//! the same sentence —
//!
//! ```text
//! Agent 'energy_advisor' assigned to 'democratic_primary_viability'
//! Agent 'macro_forecaster' assigned to 'national_sentiment_shift'
//! Agent 'macro_forecaster' assigned to 'republican_opponent_strength'
//! ```
//!
//! — so a considered choice and a fallback were typographically
//! identical, and "three of these five are the same generalist because no
//! agent in your roster claims politics" was a conclusion the operator had
//! to reach unaided, from five lines that each looked like a decision.
//!
//! That is the antipattern this module exists to prevent. A generalist
//! doing duty is a legitimate outcome — sometimes nothing better exists —
//! but it must be PRESENTED as what it is, and the shortage that produced
//! it must be surfaced as something the operator can act on. Hence
//! [`Standing`], which grades every assignment, and [`Gap`], which names
//! the shortages and carries a concrete [`Remedy`] for each.
//!
//! # Where this is going
//!
//! The grading here is structural: it reads declarations and routing
//! reasons, not outcomes. That is deliberate — it is the mechanism that
//! has to work before measurement is worth anything. Once resolved
//! forecasts can be attributed back to the agents that researched their
//! drivers, [`routing::Proven`] carries the score and
//! [`routing::declared_specialists_ranked`] already sorts on it, so a
//! measured specialist outranks a merely declared one without this module
//! changing shape. Composition effectiveness and tournament standing are
//! the same shape again: an agent, a domain, a sample size, a score.
//!
//! The ordering rule that matters, and the reason the floor in
//! [`routing::MIN_RESOLVED_TO_RANK_ON_RECORD`] exists: a record only
//! outranks a declaration once enough questions stand behind it.
//! Otherwise the first agent to get lucky is promoted over every
//! specialist, and the fitness signal becomes a way of laundering noise.

use crate::routing::{self, RouteReason};

/// How much confidence an assignment deserves.
///
/// The ordering is meaningful: `Resident` > `Adjacent` > `Stopgap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Standing {
    /// A generalist standing in. Nothing better was routable, and the
    /// evidence this produces should be read accordingly.
    Stopgap,
    /// A specialist from outside this question's domain, matched because
    /// the driver's own name asked for it — a legal analyst on an FFP
    /// driver, a macro forecaster on an inflation driver.
    Adjacent,
    /// An agent that declares this question's domain, or is its resident
    /// expert. The recommendation the console should be leading with.
    Resident,
}

impl Standing {
    /// Grade one assignment.
    ///
    /// The line is drawn at whether the agent matched vocabulary it OWNS.
    /// `macro_forecaster` selected for `economic_conditions_2027_2028` is
    /// doing exactly the work its card claims — grading that a stand-in
    /// would be a small lie, and a signal that lies in small ways stops
    /// being read. What makes an assignment a stopgap is the ABSENCE of a
    /// match: [`RouteReason::Default`] means nothing in the driver's name
    /// or rationale spoke to any agent, and the generalist is simply where
    /// the ladder ends.
    ///
    /// The generalist check survives only for [`RouteReason::Fermi`],
    /// which is the one rung that is not a match at all — it is a
    /// suggestion this module did not make.
    pub fn of(agent: &str, reason: RouteReason) -> Self {
        match reason {
            RouteReason::DeclaredSpecialist | RouteReason::DomainSpecialist => Standing::Resident,
            RouteReason::CrossCutting | RouteReason::Keyword => Standing::Adjacent,
            RouteReason::Fermi if !routing::is_generalist(agent) => Standing::Adjacent,
            RouteReason::Fermi | RouteReason::Default => Standing::Stopgap,
        }
    }

    /// One phrase for the UI, so the three cases cannot be confused.
    pub fn label(self) -> &'static str {
        match self {
            Standing::Resident => "resident specialist",
            Standing::Adjacent => "cross-domain specialist",
            Standing::Stopgap => "stand-in — nothing matched",
        }
    }

    /// Whether this assignment reflects expertise in the question's domain.
    pub fn is_specialist(self) -> bool {
        matches!(self, Standing::Resident | Standing::Adjacent)
    }
}

/// One driver's staffing, graded.
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub driver: String,
    pub agent: String,
    pub reason: RouteReason,
    pub standing: Standing,
}

/// Something about this team the operator should know before it spends.
#[derive(Debug, Clone, PartialEq)]
pub enum Gap {
    /// No routable agent declares this question's domain, so some
    /// drivers are being answered by stand-ins.
    ///
    /// `dormant` are agents that DO claim the domain but which the
    /// console currently refuses to assign. Naming them is the difference
    /// between "go and look" and "you already have one" — and when the
    /// list is empty, that is itself a finding: the catalogue has been
    /// searched and there is nothing in it, which is a far more useful
    /// thing to be told than "search the marketplace".
    NoResidentExpert {
        domain: String,
        /// Drivers answered by an agent that does not claim this domain.
        on_non_residents: usize,
        dormant: Vec<String>,
    },
    /// One agent that is NOT the resident expert is carrying most of the
    /// decomposition.
    ///
    /// Deliberately not raised when the concentrated agent IS the
    /// resident expert: a football analyst taking four of five drivers on
    /// a football question is the system working, not a shortage. The
    /// same count on a cross-cutting or generalist agent means the roster
    /// has a hole and one agent is papering over it.
    Concentrated {
        agent: String,
        carrying: usize,
        of: usize,
        standing: Standing,
    },
    /// A driver produced no routing signal at all — its name and
    /// rationale said nothing any agent claims.
    NoSignal { driver: String },
}

impl Gap {
    /// A sentence naming the problem.
    pub fn headline(&self) -> String {
        match self {
            Gap::NoResidentExpert {
                domain,
                on_non_residents,
                dormant,
            } => format!(
                "No agent in your roster claims '{}', so all {} driver{} are on agents \
                 from outside it.{}",
                domain,
                on_non_residents,
                if *on_non_residents == 1 { "" } else { "s" },
                if dormant.is_empty() {
                    String::new()
                } else {
                    format!(
                        " {} claim{} it but {} not assignable.",
                        dormant.join(", "),
                        if dormant.len() == 1 { "s" } else { "" },
                        if dormant.len() == 1 { "is" } else { "are" },
                    )
                }
            ),
            Gap::Concentrated {
                agent,
                carrying,
                of,
                standing,
            } => format!(
                "{} is answering {} of {} drivers as a {}.",
                agent,
                carrying,
                of,
                standing.label()
            ),
            Gap::NoSignal { driver } => {
                format!("'{}' gave the router nothing to match on.", driver)
            }
        }
    }

    /// What to do about it. Every gap has one, or it is an observation
    /// rather than a gap and does not belong here.
    pub fn remedy(&self) -> Remedy {
        match self {
            Gap::NoResidentExpert {
                domain, dormant, ..
            } if !dormant.is_empty() => Remedy::Admit {
                domain: domain.clone(),
                agents: dormant.clone(),
            },
            Gap::NoResidentExpert { domain, .. } => Remedy::Discover {
                domain: domain.clone(),
            },
            Gap::Concentrated { agent, .. } => Remedy::Diversify {
                agent: agent.clone(),
            },
            Gap::NoSignal { driver } => Remedy::ClarifyDriver {
                driver: driver.clone(),
            },
        }
    }
}

/// A concrete next action. Kept as data rather than prose so the console
/// can wire it to the affordance that performs it — the marketplace
/// search, the hire modal, the driver editor — instead of telling the
/// operator to go and find it.
#[derive(Debug, Clone, PartialEq)]
pub enum Remedy {
    /// The catalogue has been searched and contains nothing claiming this
    /// domain. A specialist has to come from outside it.
    ///
    /// Distinct from [`Remedy::Admit`] on purpose: "we looked and there is
    /// none" and "there is one you are not using" are different problems
    /// with different fixes, and collapsing them into one "search the
    /// marketplace" prompt is how a solvable shortage looks identical to
    /// an unsolvable one.
    Discover { domain: String },
    /// An agent claims this domain but is being refused. Find out why.
    Admit { domain: String, agents: Vec<String> },
    /// Hire a second specialist so one agent is not answering everything.
    Diversify { agent: String },
    /// Give the driver a name and rationale that say what it is.
    ClarifyDriver { driver: String },
}

impl Remedy {
    /// Imperative phrasing for a UI affordance.
    pub fn call_to_action(&self) -> String {
        match self {
            Remedy::Discover { domain } => format!(
                "Nothing in your catalogue claims '{}' — a specialist for it \
                 would have to be hired from the marketplace or authored",
                domain
            ),
            Remedy::Admit { domain, agents } => format!(
                "{} already claim{} '{}' — check why {} being refused \
                 (usually a missing fermi_contract on the card)",
                agents.join(", "),
                if agents.len() == 1 { "s" } else { "" },
                domain,
                if agents.len() == 1 {
                    "it is"
                } else {
                    "they are"
                },
            ),
            Remedy::Diversify { agent } => {
                format!("Hire a specialist to take some drivers off {}", agent)
            }
            Remedy::ClarifyDriver { driver } => {
                format!("Rename or expand the rationale for '{}'", driver)
            }
        }
    }
}

/// The staffing of one decomposition.
#[derive(Debug, Clone, PartialEq)]
pub struct Coverage {
    pub domain: String,
    /// The agent that declares this domain, if the roster has one.
    pub resident: Option<String>,
    pub assignments: Vec<Assignment>,
    pub gaps: Vec<Gap>,
}

impl Coverage {
    pub fn drivers(&self) -> usize {
        self.assignments.len()
    }

    pub fn specialists(&self) -> usize {
        self.assignments
            .iter()
            .filter(|a| a.standing.is_specialist())
            .count()
    }

    fn count(&self, standing: Standing) -> usize {
        self.assignments
            .iter()
            .filter(|a| a.standing == standing)
            .count()
    }

    /// Drivers answered by an agent that declares this question's domain.
    pub fn residents(&self) -> usize {
        self.count(Standing::Resident)
    }

    /// Drivers answered by a specialist from outside the domain.
    pub fn adjacents(&self) -> usize {
        self.count(Standing::Adjacent)
    }

    /// Drivers where nothing matched and the ladder simply ended.
    pub fn stopgaps(&self) -> usize {
        self.count(Standing::Stopgap)
    }

    /// Distinct agents, most-loaded first, ties broken by id.
    pub fn load(&self) -> Vec<(String, usize)> {
        let mut counts: Vec<(String, usize)> = Vec::new();
        for a in &self.assignments {
            match counts.iter_mut().find(|(id, _)| *id == a.agent) {
                Some((_, n)) => *n += 1,
                None => counts.push((a.agent.clone(), 1)),
            }
        }
        counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        counts
    }

    /// One line for the activity log.
    ///
    /// Reports the STAFFING, broken down by standing, because that is the
    /// thing five identically-phrased "Agent X assigned to Y" lines never
    /// added up to. "3 resident, 2 cross-domain" and "0 resident, 5
    /// cross-domain" are the same five lines and completely different
    /// forecasts.
    pub fn summary(&self) -> String {
        let load = self.load();
        let roll: Vec<String> = load
            .iter()
            .map(|(id, n)| {
                if *n > 1 {
                    format!("{} ({})", id, n)
                } else {
                    id.clone()
                }
            })
            .collect();

        let mut staffing: Vec<String> = Vec::new();
        if self.residents() > 0 {
            staffing.push(format!("{} resident", self.residents()));
        }
        if self.adjacents() > 0 {
            staffing.push(format!("{} cross-domain", self.adjacents()));
        }
        if self.stopgaps() > 0 {
            staffing.push(format!("{} unmatched", self.stopgaps()));
        }

        format!(
            "👥 {} driver{} → {} agent{}: {}. Staffing: {}.",
            self.drivers(),
            if self.drivers() == 1 { "" } else { "s" },
            load.len(),
            if load.len() == 1 { "" } else { "s" },
            roll.join(", "),
            staffing.join(", "),
        )
    }
}

/// Fraction of a decomposition one non-resident agent may carry before it
/// counts as a shortage rather than a coincidence.
const CONCENTRATION: f64 = 0.5;

/// Decompositions smaller than this are too small for concentration to
/// mean anything — two drivers on one agent is not a monoculture.
const MIN_DRIVERS_FOR_CONCENTRATION: usize = 3;

/// Grade a decomposition that has already been routed.
///
/// Takes the routing decisions rather than making them, because the
/// caller has already made them and re-deriving would risk grading
/// something other than what actually ran.
/// Agents that claim `domain` but which `is_routable` currently refuses.
///
/// This is the discovery step, and it runs against the catalogue the
/// console already holds rather than sending the operator away to perform
/// it by hand. An empty result is a real answer — "searched, found
/// nothing" — and is reported as such.
pub fn dormant_claimants(
    domain: &str,
    roster: &[(String, Vec<String>, bool)],
    record: &[routing::Proven],
    is_routable: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    routing::declared_specialists_ranked(domain, roster, record, &|a| !is_routable(a))
}

pub fn assess(
    domain: &str,
    resident: Option<&str>,
    routed: &[(String, String, RouteReason)],
    dormant: &[String],
) -> Coverage {
    let assignments: Vec<Assignment> = routed
        .iter()
        .map(|(driver, agent, reason)| Assignment {
            driver: driver.clone(),
            agent: agent.clone(),
            reason: *reason,
            standing: Standing::of(agent, *reason),
        })
        .collect();

    let mut coverage = Coverage {
        domain: domain.to_string(),
        resident: resident.map(str::to_string),
        assignments,
        gaps: Vec::new(),
    };

    let total = coverage.drivers();
    if total == 0 {
        return coverage;
    }

    // A domain with no resident expert is the shortage that produces
    // everything else, so it is reported first and reported once.
    // Keyed on residents, not on stopgaps. A decomposition where every
    // driver found a competent cross-domain agent still has no expert in
    // the question's own domain, and that is exactly the case the operator
    // could not see: five plausible assignments and nobody who actually
    // studies elections.
    let on_non_residents = coverage.adjacents() + coverage.stopgaps();
    if resident.is_none()
        && coverage.residents() == 0
        && on_non_residents > 0
        && domain != "general"
    {
        coverage.gaps.push(Gap::NoResidentExpert {
            domain: domain.to_string(),
            on_non_residents,
            dormant: dormant.to_vec(),
        });
    }

    for (agent, carrying) in coverage.load() {
        if total < MIN_DRIVERS_FOR_CONCENTRATION {
            break;
        }
        if (carrying as f64) <= (total as f64) * CONCENTRATION {
            break; // load() is sorted, so nothing below this qualifies
        }
        // The resident expert carrying its own domain is the system
        // working. Only a stand-in doing so is a shortage.
        let standing = coverage
            .assignments
            .iter()
            .filter(|a| a.agent == agent)
            .map(|a| a.standing)
            .max()
            .unwrap_or(Standing::Stopgap);
        if standing == Standing::Resident {
            continue;
        }
        coverage.gaps.push(Gap::Concentrated {
            agent,
            carrying,
            of: total,
            standing,
        });
    }

    for a in &coverage.assignments {
        if a.reason == RouteReason::Default {
            coverage.gaps.push(Gap::NoSignal {
                driver: a.driver.clone(),
            });
        }
    }

    coverage
}

/// One driver, as the router sees it.
#[derive(Debug, Clone)]
pub struct DriverBrief {
    pub name: String,
    pub rationale: String,
    /// What Fermi proposed, if anything.
    pub suggested: Option<String>,
}

/// Route a whole decomposition and grade it.
///
/// The convenience form of [`assess`], for callers that have not routed
/// yet — the picker's preview, and the tests that pin what a real
/// decomposition produces.
pub fn plan(
    domain: &str,
    drivers: &[DriverBrief],
    roster: &[(String, Vec<String>, bool)],
    record: &[routing::Proven],
    is_routable: &dyn Fn(&str) -> bool,
) -> Coverage {
    let resident = routing::declared_specialists_ranked(domain, roster, record, is_routable)
        .into_iter()
        .next()
        .or_else(|| routing::domain_specialist(domain).map(str::to_string));

    let routed: Vec<(String, String, RouteReason)> = drivers
        .iter()
        .map(|d| {
            let (agent, reason) = routing::select_agent_for_driver_declared(
                &d.name,
                &d.rationale,
                domain,
                d.suggested.as_deref(),
                resident.as_deref(),
                is_routable,
            );
            (d.name.clone(), agent, reason)
        })
        .collect();

    let dormant = dormant_claimants(domain, roster, record, is_routable);
    assess(domain, resident.as_deref(), &routed, &dormant)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brief(name: &str, rationale: &str) -> DriverBrief {
        DriverBrief {
            name: name.into(),
            rationale: rationale.into(),
            suggested: None,
        }
    }

    fn all_routable(_: &str) -> bool {
        true
    }

    // ── Grading one assignment ──────────────────────────────────────

    #[test]
    fn an_unmatched_driver_is_graded_as_a_stand_in() {
        // `Default` means nothing in the driver's name or rationale spoke
        // to any agent and the ladder simply ended. That is the assignment
        // that must never present as a decision.
        assert_eq!(
            Standing::of("macro_forecaster", RouteReason::Default),
            Standing::Stopgap
        );
        // As is Fermi naming a generalist, which is a suggestion this
        // module did not make and cannot corroborate from the rungs.
        assert_eq!(
            Standing::of("macro_forecaster", RouteReason::Fermi),
            Standing::Stopgap
        );
    }

    #[test]
    fn a_generalist_answering_its_own_subject_is_not_called_a_stand_in() {
        // `macro_forecaster` on `economic_conditions_2027_2028` is doing
        // exactly the work its card claims. Grading that a stand-in would
        // be a small lie, and a signal that lies in small ways stops being
        // read â the same failure as a calibration bar that draws
        // missing data as perfect.
        assert_eq!(
            Standing::of("macro_forecaster", RouteReason::Keyword),
            Standing::Adjacent
        );
        assert_eq!(
            Standing::of("macro_forecaster", RouteReason::CrossCutting),
            Standing::Adjacent
        );
    }

    #[test]
    fn a_generalist_is_never_the_resident_expert() {
        // The invariant that does survive: a generalist can answer a
        // driver competently, but it never becomes the agent that OWNS the
        // question's domain, and a forecast staffed entirely by them still
        // reports a missing specialist.
        let c = assess(
            "politics",
            None,
            &[(
                "economic_conditions".into(),
                "macro_forecaster".into(),
                RouteReason::Keyword,
            )],
            &[],
        );
        assert_eq!(c.residents(), 0);
        assert!(c
            .gaps
            .iter()
            .any(|g| matches!(g, Gap::NoResidentExpert { .. })));
    }

    #[test]
    fn a_declared_expert_outranks_a_cross_domain_one() {
        assert!(
            Standing::of("weather_oracle", RouteReason::DeclaredSpecialist)
                > Standing::of("entity_investigator", RouteReason::CrossCutting)
        );
        assert!(
            Standing::of("entity_investigator", RouteReason::CrossCutting)
                > Standing::of("macro_forecaster", RouteReason::Default)
        );
    }

    // ── Grading a team ──────────────────────────────────────────────

    /// The reported forecast: "Will Alexandria Ocasio-Cortez win the 2028
    /// US Presidential Election?". No agent in the curated roster claims
    /// `politics`, which is the fact the operator needed and never got.
    fn aoc_routed() -> Vec<(String, String, RouteReason)> {
        vec![
            (
                "democratic_primary_viability".into(),
                "entity_investigator".into(),
                RouteReason::Keyword,
            ),
            (
                "national_sentiment_shift".into(),
                "sentiment_analyzer".into(),
                RouteReason::Keyword,
            ),
            (
                "republican_opponent_strength".into(),
                "entity_investigator".into(),
                RouteReason::Keyword,
            ),
            (
                "aoc_political_capital_growth".into(),
                "entity_investigator".into(),
                RouteReason::Keyword,
            ),
            (
                "economic_conditions_2027_2028".into(),
                "macro_forecaster".into(),
                RouteReason::Keyword,
            ),
        ]
    }

    #[test]
    fn a_domain_with_no_resident_expert_says_so_and_says_what_to_do() {
        let c = assess("politics", None, &aoc_routed(), &[]);
        let gap = c
            .gaps
            .iter()
            .find(|g| matches!(g, Gap::NoResidentExpert { .. }))
            .expect("a politics question with no politics agent is a gap");
        assert_eq!(
            *gap,
            Gap::NoResidentExpert {
                domain: "politics".into(),
                on_non_residents: 5,
                dormant: Vec::new(),
            }
        );
        assert_eq!(
            gap.remedy(),
            Remedy::Discover {
                domain: "politics".into()
            }
        );
        assert!(gap.remedy().call_to_action().contains("marketplace"));
    }

    #[test]
    fn a_stand_in_carrying_the_decomposition_is_a_gap() {
        // Three of five on one cross-domain agent, because the roster has
        // no politics specialist to take them.
        let c = assess("politics", None, &aoc_routed(), &[]);
        assert_eq!(
            c.gaps
                .iter()
                .find(|g| matches!(g, Gap::Concentrated { .. })),
            Some(&Gap::Concentrated {
                agent: "entity_investigator".into(),
                carrying: 3,
                of: 5,
                standing: Standing::Adjacent,
            })
        );
    }

    #[test]
    fn the_resident_expert_carrying_its_own_domain_is_not_a_gap() {
        // Four of five EPL drivers on the football analyst is the system
        // working. Reporting it as concentration would train the operator
        // to ignore the warning that matters.
        let routed = vec![
            (
                "squad_quality_retention".into(),
                "football_analyst".into(),
                RouteReason::DomainSpecialist,
            ),
            (
                "competitive_landscape".into(),
                "football_analyst".into(),
                RouteReason::DomainSpecialist,
            ),
            (
                "injury_fixture_congestion".into(),
                "football_analyst".into(),
                RouteReason::DomainSpecialist,
            ),
            (
                "tactical_meta_shift".into(),
                "football_analyst".into(),
                RouteReason::DomainSpecialist,
            ),
            (
                "regulatory_financial_risk".into(),
                "entity_investigator".into(),
                RouteReason::CrossCutting,
            ),
        ];
        let c = assess("sports_football", Some("football_analyst"), &routed, &[]);
        assert!(
            c.gaps.is_empty(),
            "a fully-staffed team reported gaps: {:?}",
            c.gaps
        );
        assert_eq!(c.specialists(), 5);
        assert_eq!(c.stopgaps(), 0);
    }

    #[test]
    fn a_driver_nobody_could_route_is_named_individually() {
        let routed = vec![
            (
                "misc_factor".into(),
                "macro_forecaster".into(),
                RouteReason::Default,
            ),
            (
                "fda_decision".into(),
                "biotech_analyst".into(),
                RouteReason::DomainSpecialist,
            ),
        ];
        let c = assess("biotech", Some("biotech_analyst"), &routed, &[]);
        assert_eq!(
            c.gaps,
            vec![Gap::NoSignal {
                driver: "misc_factor".into()
            }]
        );
        assert_eq!(
            c.gaps[0].remedy(),
            Remedy::ClarifyDriver {
                driver: "misc_factor".into()
            }
        );
    }

    #[test]
    fn two_drivers_on_one_agent_is_not_a_monoculture() {
        let routed = vec![
            (
                "a".into(),
                "entity_investigator".into(),
                RouteReason::Keyword,
            ),
            (
                "b".into(),
                "entity_investigator".into(),
                RouteReason::Keyword,
            ),
        ];
        let c = assess("politics", None, &routed, &[]);
        assert!(!c.gaps.iter().any(|g| matches!(g, Gap::Concentrated { .. })));
    }

    #[test]
    fn the_summary_reports_the_staffing_not_just_the_headcount() {
        let c = assess("politics", None, &aoc_routed(), &[]);
        let s = c.summary();
        assert!(s.contains("5 drivers"), "{s}");
        assert!(s.contains("entity_investigator (3)"), "{s}");
        assert!(s.contains("Staffing: 5 cross-domain"), "{s}");
        assert!(!s.contains("resident"), "no resident expert exists: {s}");
    }

    // ── Discovery: a specialist is surfaced, not merely available ────

    #[test]
    fn a_declared_specialist_is_the_resident_even_when_no_table_knows_the_domain() {
        // `domain_specialist` is a compile-time match over four domains.
        // Discovery has to work for the other however-many, or every new
        // domain arrives as a shortage that never resolves.
        let roster = vec![(
            "weather_oracle".to_string(),
            vec!["climate".to_string(), "weather".to_string()],
            true,
        )];
        let c = plan(
            "climate",
            &[brief("synoptic_pattern", "a specific synoptic setup")],
            &roster,
            &[],
            &all_routable,
        );
        assert_eq!(c.resident.as_deref(), Some("weather_oracle"));
        assert_eq!(c.assignments[0].standing, Standing::Resident);
        assert!(c.gaps.is_empty(), "{:?}", c.gaps);
    }

    #[test]
    fn an_empty_roster_produces_a_discovery_prompt_not_a_silent_generalist() {
        let c = plan(
            "politics",
            &[brief("misc_factor", "")],
            &[],
            &[],
            &all_routable,
        );
        assert_eq!(c.resident, None);
        assert_eq!(c.assignments[0].standing, Standing::Stopgap);
        assert!(c
            .gaps
            .iter()
            .any(|g| matches!(g, Gap::NoResidentExpert { .. })));
    }

    // ── End to end, against the roster that actually ships ──────────

    /// Load the real on-disk cards, so these fail if a card regresses.
    /// Mirrors `routing::routing_tests::real_roster`.
    fn real_roster() -> Vec<(String, Vec<String>, bool)> {
        use crate::negotiate::AgentContract;
        let dir = ["agents/curated", "../../agents/curated"]
            .iter()
            .map(std::path::Path::new)
            .find(|p| p.exists())
            .expect("run from the workspace root or the crate root");
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).expect("read agents/curated") {
            let path = e.expect("entry").path().join("agent_card.json");
            if !path.exists() {
                continue;
            }
            let raw = std::fs::read_to_string(&path).expect("read card");
            let j: serde_json::Value = serde_json::from_str(&raw).expect("parse card");
            let id = j["agent_id"].as_str().unwrap_or_default().to_string();
            let c = AgentContract::from_card(&j);
            if !c.domains.is_empty() {
                out.push((id, c.domains, c.domains_explicit));
            }
        }
        assert!(out.len() > 20, "roster looks empty: {}", out.len());
        out
    }

    const PRIMARY: &str = "AOC's progressive brand faces structural headwinds in Democratic \
         primaries, which since 1972 have favored centrist candidates 11/14 times. However, \
         demographic shifts (younger, more diverse electorate), Sanders' 2016/2020 near-misses, \
         and potential lack of strong centrist heir in 2028 create upside. Downside: party \
         establishment resistance, fundraising disadvantage vs governors/senators. Upside: \
         movement energy, small-donor base, media fluency.";

    const SENTIMENT: &str = "Public support for Medicare-for-All, Green New Deal, and wealth \
         taxes has fluctuated 35-55% in polls 2018-2024. Economic conditions in 2027-28 \
         (recession, inequality trends, climate events) could shift this dramatically.";

    #[test]
    fn the_reported_forecast_reports_its_own_shortage() {
        // "Will Alexandria Ocasio-Cortez win the 2028 US Presidential
        // Election?" — five drivers, and what the operator saw was five
        // interchangeable "Agent X assigned to Y" lines. The fact that
        // mattered, and that nothing said, is that the curated roster
        // contains no agent claiming `politics` at all.
        let domain = routing::detect_domain(
            "Will Alexandria Ocasio-Cortez win the 2028 US Presidential Election?",
        );
        assert_eq!(domain, "politics");

        let c = plan(
            &domain,
            &[
                brief("democratic_primary_viability", PRIMARY),
                brief("national_sentiment_shift", SENTIMENT),
                brief(
                    "republican_opponent_strength",
                    "Strength of the eventual Republican nominee and the party's coalition.",
                ),
                brief(
                    "aoc_political_capital_growth",
                    "Committee assignments, fundraising totals and national profile.",
                ),
                brief(
                    "economic_conditions_2027_2028",
                    "GDP growth, unemployment and inflation path into the election year.",
                ),
            ],
            &real_roster(),
            &[],
            &all_routable,
        );

        // No politics specialist exists, and the console now says so.
        assert_eq!(c.resident, None, "roster gained a politics agent");
        assert!(
            c.gaps.iter().any(|g| matches!(
                g,
                Gap::NoResidentExpert { domain, .. } if domain == "politics"
            )),
            "the shortage that produced the whole mess went unreported: {:?}",
            c.gaps
        );

        // The SimOps energy agent is gone, and so is the generalist
        // monoculture: exactly one driver is macro, because exactly one
        // driver is about the economy.
        let agents: Vec<&str> = c.assignments.iter().map(|a| a.agent.as_str()).collect();
        assert!(!agents.contains(&"energy_advisor"), "{agents:?}");
        assert_eq!(
            agents.iter().filter(|a| **a == "macro_forecaster").count(),
            1,
            "{agents:?}"
        );

        // Four of five are cross-domain specialists rather than stand-ins,
        // and the summary leads with that number.
        // Every driver reached an agent whose vocabulary it matched â and
        // NONE of them is a resident expert, which is the sentence the
        // operator needed and the five assignment lines never said.
        assert_eq!(c.residents(), 0, "{:?}", c.assignments);
        assert_eq!(c.adjacents(), 5, "{:?}", c.assignments);
        assert!(
            c.summary().contains("Staffing: 5 cross-domain"),
            "{}",
            c.summary()
        );

        // Every gap is actionable. A gap without a next step is an
        // observation, and the operator already had five of those.
        for gap in &c.gaps {
            assert!(!gap.remedy().call_to_action().is_empty(), "{gap:?}");
        }
    }

    #[test]
    fn a_weather_forecast_reports_no_shortage_at_all() {
        // The control. `weather_oracle` declares `climate` explicitly, so
        // this question is fully staffed and must produce a clean bill —
        // otherwise the warnings are noise and get ignored.
        let domain =
            routing::detect_domain("Will the highest temperature in London be 32C on August 14?");
        assert_eq!(domain, "climate");

        let c = plan(
            &domain,
            &[
                brief(
                    "synoptic_pattern_august",
                    "Requires a specific synoptic setup.",
                ),
                brief(
                    "climate_trend_warming",
                    "London is ~1.2C warmer than pre-industrial levels.",
                ),
            ],
            &real_roster(),
            &[],
            &all_routable,
        );
        assert_eq!(c.resident.as_deref(), Some("weather_oracle"));
        assert_eq!(c.stopgaps(), 0);
        assert!(c.gaps.is_empty(), "{:?}", c.gaps);
    }

    #[test]
    fn a_claimant_the_console_is_refusing_is_named_rather_than_hidden() {
        // "There is no specialist" and "there is one and you are not using
        // it" are different problems. Collapsing both into "search the
        // marketplace" sends the operator shopping for something already
        // sitting in the catalogue, refused for a reason nobody surfaced.
        let roster = vec![(
            "psephologist".to_string(),
            vec!["politics".to_string()],
            true,
        )];
        let hired_nothing = |_: &str| false;

        let dormant = dormant_claimants("politics", &roster, &[], &hired_nothing);
        assert_eq!(dormant, vec!["psephologist".to_string()]);

        let c = assess(
            "politics",
            None,
            &[(
                "turnout".into(),
                "macro_forecaster".into(),
                RouteReason::Default,
            )],
            &dormant,
        );
        let gap = c
            .gaps
            .iter()
            .find(|g| matches!(g, Gap::NoResidentExpert { .. }))
            .expect("gap");
        assert!(
            gap.headline().contains("psephologist"),
            "{}",
            gap.headline()
        );
        assert_eq!(
            gap.remedy(),
            Remedy::Admit {
                domain: "politics".into(),
                agents: vec!["psephologist".to_string()],
            }
        );
        assert!(gap.remedy().call_to_action().contains("refused"));
    }

    #[test]
    fn an_exhausted_catalogue_says_it_searched_rather_than_telling_you_to() {
        // The politics case. The useful sentence is "there is nothing",
        // which can only be said by something that looked.
        let c = assess(
            "politics",
            None,
            &[(
                "turnout".into(),
                "macro_forecaster".into(),
                RouteReason::Default,
            )],
            &[],
        );
        let gap = c
            .gaps
            .iter()
            .find(|g| matches!(g, Gap::NoResidentExpert { .. }))
            .expect("gap");
        assert_eq!(
            gap.remedy(),
            Remedy::Discover {
                domain: "politics".into()
            }
        );
        assert!(gap
            .remedy()
            .call_to_action()
            .contains("Nothing in your catalogue"));
    }

    #[test]
    fn the_real_catalogue_genuinely_has_no_politics_specialist() {
        // The claim the console is about to make to the operator, checked
        // against the cards that ship. If someone adds one, this fails and
        // the message stops being true before it stops being printed.
        let roster = real_roster();
        assert!(
            dormant_claimants("politics", &roster, &[], &|_| false).is_empty(),
            "a politics claimant appeared; the 'nothing claims this' message is now false"
        );
        // And the control: climate does have one, hired or not.
        assert!(!dormant_claimants("climate", &roster, &[], &|_| false).is_empty());
    }

    // ── The fitness seam ────────────────────────────────────────────

    #[test]
    fn with_no_record_the_ranking_is_exactly_what_it_was() {
        // The seam must be inert until it is fed. If threading `Proven`
        // through changed today's answers, the change would be a
        // behavioural one wearing a forward-compatibility costume.
        let roster = vec![
            (
                "tag_matcher".to_string(),
                vec!["weather".to_string()],
                false,
            ),
            (
                "declarer".to_string(),
                vec!["weather".to_string(), "climate".to_string()],
                true,
            ),
        ];
        assert_eq!(
            routing::declared_specialists_ranked("weather", &roster, &[], &all_routable),
            vec!["declarer".to_string(), "tag_matcher".to_string()],
        );
        assert_eq!(
            routing::declared_specialist_ranked("weather", &roster, &all_routable).as_deref(),
            Some("declarer"),
        );
    }

    #[test]
    fn a_measured_agent_outranks_a_merely_declared_one() {
        // What an agent has DONE beats what its card says. This is the
        // rung tournaments and per-agent attribution land on.
        let roster = vec![
            (
                "declarer".to_string(),
                vec!["weather".to_string()],
                true, // explicit, narrow — wins on paper
            ),
            (
                "tag_matcher".to_string(),
                vec!["weather".to_string(), "climate".to_string()],
                false, // tag fallback, wider — loses on paper
            ),
        ];
        let record = vec![routing::Proven {
            agent: "tag_matcher".into(),
            mean_shapley: 0.031,
            n_forecasts: 12,
            ci_low: Some(0.004),
        }];
        assert_eq!(
            routing::declared_specialists_ranked("weather", &roster, &record, &all_routable),
            vec!["tag_matcher".to_string(), "declarer".to_string()],
        );
    }

    #[test]
    fn a_record_that_cannot_be_told_from_zero_does_not_promote() {
        // The guard against the first agent to get lucky. A mean of 0.5
        // looks decisive; an interval straddling zero says the data cannot
        // support that reading, and the ranking must believe the interval
        // rather than the mean.
        let roster = vec![
            ("declarer".to_string(), vec!["weather".to_string()], true),
            (
                "lucky".to_string(),
                vec!["weather".to_string(), "climate".to_string()],
                false,
            ),
        ];
        for ci_low in [None, Some(-0.02), Some(0.0)] {
            let record = vec![routing::Proven {
                agent: "lucky".into(),
                mean_shapley: 0.5,
                n_forecasts: 40,
                ci_low,
            }];
            assert_eq!(
                routing::declared_specialists_ranked("weather", &roster, &record, &all_routable),
                vec!["declarer".to_string(), "lucky".to_string()],
                "promoted on ci_low = {ci_low:?}"
            );
        }
    }

    #[test]
    fn a_missing_interval_is_not_read_as_zero() {
        // `ci_low: None` means the server declined to compute an interval:
        // below MIN_BOOTSTRAP_CLUSTERS there is no replication to
        // resample, so the data say nothing at all about variability.
        // Treating that absence as a bound of 0.0 would promote precisely
        // the thinnest records.
        let thin = routing::Proven {
            agent: "x".into(),
            mean_shapley: 0.9,
            n_forecasts: 2,
            ci_low: None,
        };
        assert!(!thin.is_established());
    }

    #[test]
    fn contribution_is_read_as_higher_is_better() {
        // Shapley credit is positively oriented, the opposite of the Brier
        // score this seam was first written around. A sign error here
        // would rank the worst contributor first and look entirely
        // plausible doing it.
        let roster = vec![
            ("a_weak".to_string(), vec!["weather".to_string()], true),
            ("b_strong".to_string(), vec!["weather".to_string()], true),
        ];
        let record = vec![
            routing::Proven {
                agent: "a_weak".into(),
                mean_shapley: 0.004,
                n_forecasts: 30,
                ci_low: Some(0.001),
            },
            routing::Proven {
                agent: "b_strong".into(),
                mean_shapley: 0.062,
                n_forecasts: 8,
                ci_low: Some(0.011),
            },
        ];
        assert_eq!(
            routing::declared_specialists_ranked("weather", &roster, &record, &all_routable),
            vec!["b_strong".to_string(), "a_weak".to_string()],
        );
    }

    #[test]
    fn a_negative_contributor_is_never_established() {
        // Shapley credit is signed: an agent can have dragged forecasts
        // AWAY from the outcome. Requiring the lower bound to be ABOVE
        // zero, rather than merely present, is what keeps such an agent
        // from being promoted over one with no record at all.
        let harmful = routing::Proven {
            agent: "harmful".into(),
            mean_shapley: -0.04,
            n_forecasts: 25,
            ci_low: Some(-0.07),
        };
        assert!(!harmful.is_established());
    }

    #[test]
    fn a_measured_generalist_still_never_becomes_the_resident_expert() {
        // Fitness reorders the agents that CLAIM the domain. It does not
        // admit one that doesn't — otherwise a generalist with a good
        // run gets promoted to specialist and the distinction this whole
        // module exists to preserve collapses.
        let roster = vec![(
            "macro_forecaster".to_string(),
            vec!["politics".to_string()],
            true,
        )];
        let record = vec![routing::Proven {
            agent: "macro_forecaster".into(),
            mean_shapley: 0.09,
            n_forecasts: 100,
            ci_low: Some(0.05),
        }];
        assert!(
            routing::declared_specialists_ranked("politics", &roster, &record, &all_routable)
                .is_empty()
        );
    }
}
