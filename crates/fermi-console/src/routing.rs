//! Driver → specialist routing for the Fermi orchestra.
//!
//! GPUI-free by construction so it can be tested with
//! `cargo test -p fermi-console --lib` in seconds. See the crate docs
//! for why anything worth testing must live outside the binary target.

// ═══════════════════════════════════════════════════════════════════
// Driver → specialist routing
// ═══════════════════════════════════════════════════════════════════
//
// Every driver of a forecast gets exactly one research agent. Picking
// the right one is the difference between "a football analyst pulled
// City's xG, Elo and injury list off API-Football" and "a generalist
// wrote three paragraphs of plausible prose". The routing below is the
// only thing standing between those two outcomes, so it is a free
// function with tests rather than an inline `if` ladder.
//
// Historically this lived inline in `process_macro_forecaster_result`
// and had three independent ways to collapse to `macro_forecaster`:
//
//   1. Availability was probed with `registry.get()`, i.e. the LOCAL
//      on-disk card directory. That directory is resolved relative to
//      CWD at startup and is absent in most installs — but agents
//      execute against the ABW server, which has the full roster. So a
//      perfectly runnable `football_analyst` was judged "missing" and
//      routing fell through. See `CockpitState::agent_is_routable`.
//   2. The fallback for an unavailable suggestion was `domain_agent` —
//      which, for a football question, is the very `football_analyst`
//      that just failed the check. The retry could not succeed, so it
//      always landed on the hardcoded `"macro_forecaster"` below it.
//   3. Fermi's own `suggested_agent` was honoured unconditionally, so a
//      generalist suggestion silently displaced the domain expert.
//
// Net effect: `macro_forecaster (5)` on a Premier League question.
//
// The second generation of this file fixed those three, and introduced a
// fourth: the keyword ladder was a FIRST-MATCH `if` chain over the driver
// name concatenated with its rationale. One incidental word anywhere in
// sixty words of prose decided the route, and the rung ORDER was the only
// tie-break. Observed 2026-08-22 on "Will Alexandria Ocasio-Cortez win the
// 2028 US Presidential Election?":
//
//   driver:      democratic_primary_viability
//   rationale:   "... Upside: movement energy, small-donor base ..."
//   routed to:   energy_advisor
//
// `energy` was checked above `sentiment` and `entity`, so the one word in
// the rationale that had nothing to do with the driver won outright. The
// same run sent three of five drivers to `macro_forecaster`, which sat at
// the bottom of the ladder with the broadest vocabulary in it (`economic`,
// `policy`, `crisis`, `trade`) AND was the hardcoded default, so it could
// not lose.
//
// The ladder is now a SCORED table instead. See `RUNGS` and `score_rungs`.
// Three properties do the work:
//
//   * the driver NAME outweighs its rationale. A name is a declaration of
//     what the driver is; a rationale is prose that may mention anything.
//   * prose counts as a SHARE, not a presence. `recession` in a rationale
//     that is four-fifths about public opinion is context, not the topic.
//   * displacing a resident specialist costs more than picking one when no
//     specialist exists, so prose alone can never take a driver away from
//     the domain expert.
//
// `energy_advisor` was also removed from the table outright: its card is a
// SimOps energy-balance member that answers JSON task payloads
// (`propose_stage_energy`), not a research agent. Handing it a forecast
// driver was a category error even when the keyword was genuinely about
// energy.

/// Agent ids Fermi may route to without proof of local installation.
///
/// These are the hand-authored orchestra members in `agents/curated/`.
/// They are always resolvable by the ABW server, so routing must not
/// depend on whether this machine happens to have their JSON cards on
/// disk. Third-party members are picked up dynamically from the server
/// roster — see [`CockpitState::agent_is_routable`].
pub const FERMI_ORCHESTRA: &[&str] = &[
    "macro_forecaster",
    "market_research",
    "sentiment_analyzer",
    "entity_investigator",
    "equity_analyst",
    "biotech_analyst",
    "nba_analyst",
    "football_analyst",
    "energy_advisor",
    "macro_data_agent",
    "football_institution_agent",
    "fixture_context_agent",
];

/// Agents with no domain of their own. A suggestion of one of these is
/// treated as "Fermi had no strong opinion" and does not get to
/// displace a domain specialist on an in-domain driver.
const GENERALIST_AGENTS: &[&str] = &["macro_forecaster", "market_research"];

/// Whether this agent has a domain of its own.
///
/// A generalist assignment is a legitimate outcome — sometimes nothing
/// better exists — but it is never a *specialist recommendation*, and the
/// console has to be able to tell the two apart before it presents them.
/// Presenting a stand-in with the same confidence as a resident expert is
/// the antipattern that made five identical-looking recommendations out of
/// one considered choice and four fallbacks.
pub fn is_generalist(agent: &str) -> bool {
    GENERALIST_AGENTS.contains(&agent)
}

/// The resident expert for a question domain, if one exists.
///
/// `None` means the domain has no specialist and the keyword ladder is
/// the primary signal. Note this is deliberately NOT `macro_forecaster`
/// for finance/politics/climate: those are the generalist's home turf,
/// and treating it as a "specialist" there would let it out-rank a
/// better keyword match.
pub fn domain_specialist(domain: &str) -> Option<&'static str> {
    match domain {
        "sports_nba" | "basketball" => Some("nba_analyst"),
        "sports_football" => Some("football_analyst"),
        "biotech" | "pharma" | "clinical" => Some("biotech_analyst"),
        "stocks" => Some("equity_analyst"),
        _ => None,
    }
}

/// Whole-word containment.
///
/// Plain `contains()` on short keywords is a live hazard in this file's
/// history: `"pre-industrial".contains("trial")` is true, which routed
/// a climate-warming driver to `biotech_analyst`; `"development"`
/// contains `"elo"`, which routed anything mentioning development to
/// `nba_analyst`. Match on non-alphanumeric boundaries instead.
///
/// Multi-word needles (`"home court"`) work unchanged — the boundary
/// test only looks at the characters either side of the match.
///
/// A trailing `s` or `es` is absorbed, so `"sanction"` matches
/// `"sanctions"` and `"playoff"` matches `"playoffs"`. Without that,
/// switching from `contains` to word matching would silently drop every
/// plural the old keyword lists relied on.
///
/// It does NOT do prefix matching: `"diplomat"` will not match
/// `"diplomatic"`. Spell those out in the keyword lists instead — the
/// prefix hack is what let `"trial"` match `"industrial"`.
pub fn contains_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let bytes = haystack.as_bytes();
    let is_boundary = |i: usize| i >= bytes.len() || !(bytes[i] as char).is_alphanumeric();
    let mut from = 0;
    while let Some(rel) = haystack[from..].find(needle) {
        let start = from + rel;
        let end = start + needle.len();
        let left_ok = start == 0 || !(bytes[start - 1] as char).is_alphanumeric();
        if left_ok {
            // Exact, or with a plural suffix.
            if is_boundary(end)
                || (bytes.get(end) == Some(&b's') && is_boundary(end + 1))
                || (bytes.get(end) == Some(&b'e')
                    && bytes.get(end + 1) == Some(&b's')
                    && is_boundary(end + 2))
            {
                return true;
            }
        }
        from = start + 1;
        if from >= haystack.len() {
            break;
        }
    }
    false
}

/// One agent's claim on a driver, expressed as vocabulary.
struct Rung {
    agent: &'static str,
    /// Whether this rung may take a driver AWAY from the resident domain
    /// specialist.
    ///
    /// A football analyst can tell you what fixture congestion does to xG.
    /// It cannot tell you how the Premier League's 115 FFP charges will be
    /// adjudicated. Only the three genuinely cross-domain rungs — legal /
    /// institutional, macroeconomic, public opinion — are allowed to
    /// displace an expert; everything else has to wait for a question in
    /// its own domain. Without this flag a driver named
    /// `broadcast_revenue_shock` would pull an EPL forecast over to
    /// `market_research` on the strength of one word in its name.
    cross_cutting: bool,
    /// Question domains in which this vocabulary is trustworthy on its own.
    ///
    /// Empty means domain-neutral. Outside its home domains a rung must
    /// match the driver NAME to score at all — prose is not enough. This is
    /// the guard that stops a clinical-trial vocabulary from claiming a
    /// climate driver, and it is checked before scoring so a foreign rung
    /// does not even dilute the prose denominator.
    home: &'static [&'static str],
    /// Whole-word needles. See [`contains_word`] for why these are spelled
    /// out rather than prefixed, and why bare `market`, `approval`,
    /// `policy` and `court` are deliberately absent: each of them matched
    /// something it had no business matching.
    needles: &'static [&'static str],
}

/// Every rung, scored together rather than tried in order.
///
/// Note what is NOT here. `energy_advisor` used to own
/// `energy | oil | renewable | solar | carbon | emission`, which is how
/// "movement energy" in a rationale about a Democratic primary routed a
/// political driver to a SimOps energy-balance agent. Its card answers
/// JSON task payloads for process design; it is not a research agent and
/// must never be auto-assigned to a driver. It stays in
/// [`FERMI_ORCHESTRA`] so an operator can still hire it deliberately.
/// Commodity and energy-price vocabulary moved to `macro_forecaster`,
/// which is where a WTI question belongs; physical climate vocabulary is
/// served by `weather_oracle` through its DECLARED domains.
const RUNGS: &[Rung] = &[
    // ── Legal, institutional, electoral ────────────────────────────
    Rung {
        agent: "entity_investigator",
        cross_cutting: true,
        home: &[],
        needles: &[
            "regulatory",
            "regulation",
            "legal",
            "lawsuit",
            "litigation",
            // NOT bare "court": it collides with "home_court_advantage",
            // which sent an NBA driver to entity_investigator.
            "court ruling",
            "court case",
            "tribunal",
            "hearing",
            "compliance",
            "antitrust",
            "investigation",
            "charge",
            "indictment",
            "ffp",
            "financial fair play",
            "ownership",
            "governance",
            "takeover",
            "sanction",
            // Electoral and institutional vocabulary. Before this existed,
            // every driver of a presidential-election forecast fell to the
            // generalist, because `politics` has no resident specialist and
            // nothing else in the table spoke about candidates at all.
            "candidate",
            "primary",
            "primaries",
            "nomination",
            "nominee",
            "incumbent",
            "challenger",
            "opponent",
            "caucus",
            "ballot",
            "electoral",
            "election",
            "political",
            "politician",
            "party",
            "coalition",
            "establishment",
            "endorsement",
            "fundraising",
            "donor",
            "leadership",
            "management",
            "succession",
            "regime",
            "government",
            "military",
            "cohesion",
        ],
    },
    // ── Macroeconomics, geopolitics, commodities ───────────────────
    Rung {
        agent: "macro_forecaster",
        cross_cutting: true,
        home: &[],
        needles: &[
            "macro",
            "macroeconomic",
            "economic",
            "economy",
            "inflation",
            "interest rate",
            "recession",
            "gdp",
            "unemployment",
            "currency",
            "fiscal",
            "monetary",
            "central bank",
            "treasury",
            "bond yield",
            "tariff",
            "trade war",
            "commodity",
            // Spelled out rather than a `geopolit` prefix — see
            // `contains_word`.
            "geopolitical",
            "geopolitics",
            "sanctions regime",
            "diplomatic",
            "diplomacy",
            "treaty",
            "alliance",
            "foreign policy",
            // Commodities. Inherited from the deleted energy_advisor rung:
            // a crude-oil price question is a macro question.
            "oil",
            "crude",
            "opec",
            "barrel",
            "energy price",
            "electricity price",
            "natural gas",
        ],
    },
    // ── Public opinion ─────────────────────────────────────────────
    Rung {
        agent: "sentiment_analyzer",
        cross_cutting: true,
        home: &[],
        needles: &[
            "sentiment",
            "public opinion",
            "opinion poll",
            "polling",
            "poll",
            // "approval RATING", not bare "approval": a driver named
            // `fda_approval_probability` is not a popularity contest.
            "approval rating",
            "favorability",
            "popularity",
            "perception",
            "buzz",
            "narrative",
            "media narrative",
            "press coverage",
            "social media",
            "fan sentiment",
            "public support",
            "protest",
            "unrest",
            "dissent",
            "backlash",
            "turnout",
        ],
    },
    // ── Domain-bound rungs. None of these may displace a specialist. ─
    Rung {
        agent: "football_analyst",
        cross_cutting: false,
        home: &["sports_football"],
        needles: &[
            "xg",
            "expected goals",
            "elo",
            "fixture",
            "squad",
            "transfer window",
            "matchday",
            "goal difference",
            "clean sheet",
            "relegation",
            "league table",
            "points deduction",
            "tactical",
            "formation",
            "pressing",
            "possession",
            "manager",
            "striker",
            "midfield",
            "defence",
            "defense",
        ],
    },
    Rung {
        agent: "nba_analyst",
        cross_cutting: false,
        home: &["sports_nba", "basketball"],
        needles: &[
            "nba",
            "basketball",
            "home court",
            "net rating",
            "netrtg",
            "playoff seed",
            "roster",
        ],
    },
    Rung {
        agent: "biotech_analyst",
        cross_cutting: false,
        home: &["biotech", "pharma", "clinical"],
        // "trial" is whole-word: `pre-industrial` must not read as a
        // clinical trial. `contains_word` treats `-` as a boundary, so
        // "clinical trial" and "trial readout" still match.
        needles: &[
            "clinical",
            "trial",
            "fda",
            "drug",
            "indication",
            "oncology",
            "endpoint",
            "readout",
            "efficacy",
        ],
    },
    Rung {
        agent: "equity_analyst",
        cross_cutting: false,
        home: &["stocks", "finance"],
        needles: &[
            "stock price",
            "share price",
            "eps",
            "p/e",
            "earnings",
            "shareholder",
            "valuation",
            "dividend",
            "buyback",
            "free cash flow",
            "market cap",
            "price target",
            "analyst estimate",
        ],
    },
    // ── Commercial. Domain-neutral, but never displaces a specialist. ─
    Rung {
        agent: "market_research",
        cross_cutting: false,
        home: &[],
        // NOT bare "market": it matched "prediction market", "stock
        // market" and "labour market", none of which is a TAM question.
        needles: &[
            "market share",
            "market size",
            "go-to-market",
            "competitor",
            "competition",
            "partnership",
            "revenue",
            "pricing",
            "demand",
            "adoption",
            "customer",
            "subscriber",
            "churn",
            "commercial",
            "sales",
        ],
    },
];

/// A name hit is worth this much. Deliberately larger than the entire
/// prose budget: a driver called `national_sentiment_shift` is about
/// sentiment no matter how much economics its rationale recites.
const NAME_HIT: f32 = 4.0;

/// The whole rationale is worth at most this much, split between the
/// agents that matched it in proportion to how much of the matched
/// vocabulary each one owns.
///
/// Sharing rather than counting is the point. `recession` appearing in a
/// rationale that is otherwise about public support is context; the same
/// word in a rationale that is *entirely* about inflation and rates is the
/// topic. A presence test cannot tell those apart, and routed both to
/// `macro_forecaster`.
const PROSE_BUDGET: f32 = 3.0;

/// Minimum score to be trusted when the question has no resident expert.
const MIN_KEYWORD: f32 = 1.5;

/// Minimum score to take a driver AWAY from a resident expert.
///
/// Above [`PROSE_BUDGET`] on purpose: prose alone, however dominant, can
/// never displace the domain specialist. A displacer has to match the
/// driver's NAME.
const MIN_DISPLACE: f32 = NAME_HIT;

/// Score every rung against one driver, best first.
///
/// Ties break on agent id so the choice is identical run to run — a
/// surprising assignment should be reproducible when someone goes looking
/// for it.
fn score_rungs(driver_name: &str, rationale: &str, domain: &str) -> Vec<(&'static str, f32)> {
    let name = driver_name.to_lowercase();
    let prose = rationale.to_lowercase();
    let domain = domain.trim().to_ascii_lowercase();

    let count = |hay: &str, needles: &[&str]| -> usize {
        needles.iter().filter(|n| contains_word(hay, n)).count()
    };

    // Pass one: hits, with foreign-domain rungs dropped before they can
    // dilute the prose denominator.
    let mut hits: Vec<(&'static str, usize, usize)> = Vec::new();
    for rung in RUNGS {
        // A rung outside its home domains has to be named by the driver.
        // Prose is not enough: this is where "movement energy" stopped
        // being an energy driver, and where "pre-industrial" stopped being
        // a clinical trial.
        let name_hits = count(&name, rung.needles);
        if !rung.home.is_empty() && !rung.home.contains(&domain.as_str()) && name_hits == 0 {
            continue;
        }
        let prose_hits = count(&prose, rung.needles);
        if name_hits == 0 && prose_hits == 0 {
            continue;
        }
        hits.push((rung.agent, name_hits, prose_hits));
    }

    let total_prose: usize = hits.iter().map(|(_, _, p)| p).sum();

    let mut scored: Vec<(&'static str, f32)> = hits
        .iter()
        .map(|(agent, name_hits, prose_hits)| {
            let share = if total_prose == 0 {
                0.0
            } else {
                *prose_hits as f32 / total_prose as f32
            };
            (*agent, NAME_HIT * *name_hits as f32 + PROSE_BUDGET * share)
        })
        .collect();

    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    scored
}

/// The scored candidates for one driver, best first.
///
/// Exposed so the console can answer "why this agent?" with the actual
/// numbers instead of asking someone to re-derive the table by hand. The
/// reported `energy_advisor` assignment took a code read to explain; it
/// should have taken a glance at a log line.
pub fn route_candidates(
    driver_name: &str,
    rationale: &str,
    domain: &str,
) -> Vec<(&'static str, f32)> {
    score_rungs(driver_name, rationale, domain)
}

/// Whether this agent's rung is allowed to displace a resident specialist.
fn is_cross_cutting(agent: &str) -> bool {
    RUNGS.iter().any(|r| r.agent == agent && r.cross_cutting)
}

/// Whether an agent has enough independent textual support to be taken
/// seriously as Fermi's suggestion.
///
/// Used only by the generalist guard: Fermi handing back `macro_forecaster`
/// is accepted when the driver text independently says something macro, and
/// rejected when it says nothing at all.
fn corroborated(scored: &[(&'static str, f32)], agent: &str) -> bool {
    scored.iter().any(|(a, s)| *a == agent && *s >= MIN_KEYWORD)
}

/// Why a driver ended up with the agent it did. Logged so a surprising
/// assignment is debuggable without re-deriving the ladder by hand.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteReason {
    /// An agent DECLARED competence in this domain, on its own card.
    ///
    /// Checked before the compile-time table so a new domain needs a card edit
    /// rather than a console release. This is the rung that was missing when
    /// two production weather forecasts fell through to the generalist and
    /// returned their own climatological base rate.
    DeclaredSpecialist,
    /// Fermi named this agent and it survived the domain guard.
    Fermi,
    /// Driver is outside the domain specialist's remit.
    CrossCutting,
    /// Resident expert for the question's domain.
    DomainSpecialist,
    /// Keyword match on the driver name + rationale.
    Keyword,
    /// Nothing matched; the generalist is the honest answer.
    Default,
}

impl RouteReason {
    pub fn as_str(self) -> &'static str {
        match self {
            RouteReason::DeclaredSpecialist => "declared domain specialist",
            RouteReason::Fermi => "fermi suggestion",
            RouteReason::CrossCutting => "cross-cutting concern",
            RouteReason::DomainSpecialist => "domain specialist",
            RouteReason::Keyword => "keyword match",
            RouteReason::Default => "no signal — generalist default",
        }
    }

    /// Machine-readable form, for provenance and tags.
    ///
    /// [`Self::as_str`] is prose for the log line and the UI — it contains
    /// spaces and an em-dash, so it cannot be a tag suffix. This is the
    /// stable identifier to persist and group by; changing one of these
    /// strings breaks historical comparability, so treat them as a wire
    /// format rather than a label.
    pub fn slug(self) -> &'static str {
        match self {
            RouteReason::DeclaredSpecialist => "declared_specialist",
            RouteReason::Fermi => "fermi",
            RouteReason::CrossCutting => "cross_cutting",
            RouteReason::DomainSpecialist => "domain_specialist",
            RouteReason::Keyword => "keyword",
            RouteReason::Default => "default",
        }
    }

    /// Whether this route reflects a positive signal about the agent.
    ///
    /// [`RouteReason::Default`] means nothing matched and the generalist was
    /// the honest fallback. Outcomes under that reason say almost nothing
    /// about the agent's competence and a lot about the router's coverage, so
    /// any credit model should weight them differently rather than pooling
    /// them with deliberate selections.
    pub fn is_deliberate(self) -> bool {
        !matches!(self, RouteReason::Default)
    }
}

/// Choose the research agent for one driver.
///
/// Candidates are tried in priority order and the first *routable* one
/// wins, so an unavailable preference degrades to the next-best expert
/// rather than straight to the generalist.
///
/// `is_routable` decides availability. It must not be a local-disk-only
/// check — see the module note above.
pub fn select_agent_for_driver(
    driver_name: &str,
    rationale: &str,
    domain: &str,
    suggested: Option<&str>,
    is_routable: &dyn Fn(&str) -> bool,
) -> (String, RouteReason) {
    select_agent_for_driver_declared(driver_name, rationale, domain, suggested, None, is_routable)
}

/// As [`select_agent_for_driver`], plus an agent that DECLARED this domain.
///
/// `declared` comes from the roster via [`declared_specialist`]. It is
/// consulted ahead of [`domain_specialist`] — a compile-time `match` over four
/// domains — so a new domain is served by editing a card rather than shipping a
/// console release, and a third-party agent is first-class the moment it
/// declares itself.
///
/// A declared specialist also displaces a generalist suggestion from Fermi, for
/// the same reason the hardcoded specialist already did: a generalist handed
/// back on a question with a resident expert is an absent opinion, not a
/// considered choice.
pub fn select_agent_for_driver_declared(
    driver_name: &str,
    rationale: &str,
    domain: &str,
    suggested: Option<&str>,
    declared: Option<&str>,
    is_routable: &dyn Fn(&str) -> bool,
) -> (String, RouteReason) {
    let declared = declared.map(str::trim).filter(|s| !s.is_empty());
    let specialist = domain_specialist(domain);
    let scored = score_rungs(driver_name, rationale, domain);

    let suggested = suggested.map(str::trim).filter(|s| !s.is_empty());

    let mut candidates: Vec<(&str, RouteReason)> = Vec::new();

    if let Some(s) = suggested {
        // Guard: Fermi handing back a generalist on a question that has
        // a resident expert is treated as an absent opinion, not as a
        // considered choice.
        //
        // The exception is narrow: the generalist stands only when the
        // driver text INDEPENDENTLY supports it. An earlier version
        // accepted the suggestion whenever any cross-cutting keyword
        // matched, which let a `macro_forecaster` suggestion pre-empt the
        // `entity_investigator` the analysis had actually selected — the
        // FFP driver routed to the generalist anyway.
        //
        // A DECLARED specialist counts as a resident expert here too, which is
        // the fix for weather: Fermi suggested the generalist, no hardcoded
        // specialist existed for `climate`, and every driver went generic.
        let expert_exists = specialist.is_some() || declared.is_some();
        let displaces_specialist =
            expert_exists && GENERALIST_AGENTS.contains(&s) && !corroborated(&scored, s);
        if !displaces_specialist {
            candidates.push((s, RouteReason::Fermi));
        }
    }

    // Declaration beats the scored table, and beats the compile-time one.
    if let Some(d) = declared {
        candidates.push((d, RouteReason::DeclaredSpecialist));
    }

    // A resident expert is displaced only by a cross-cutting rung that
    // cleared `MIN_DISPLACE` — which, by construction, requires the
    // driver's NAME to say so. Every qualifying rung is offered in score
    // order, so an unroutable first choice degrades to the next-best
    // displacer rather than snapping back to the specialist.
    if let Some(s) = specialist {
        for (agent, score) in &scored {
            if *agent != s && *score >= MIN_DISPLACE && is_cross_cutting(agent) {
                candidates.push((agent, RouteReason::CrossCutting));
            }
        }
        candidates.push((s, RouteReason::DomainSpecialist));
    } else {
        // No resident expert: the scored table IS the opinion. The
        // threshold is what distinguishes "the text says market share"
        // from "the text says nothing in particular"; below it the honest
        // answer is the generalist default, not the highest of several
        // meaningless scores.
        for (agent, score) in &scored {
            if *score >= MIN_KEYWORD {
                candidates.push((agent, RouteReason::Keyword));
            }
        }
    }

    // A generalist suggestion that lost the domain guard is still a
    // better answer than a blind default, so re-offer it here.
    if let Some(s) = suggested {
        candidates.push((s, RouteReason::Fermi));
    }
    candidates.push(("macro_forecaster", RouteReason::Default));

    candidates
        .into_iter()
        .find(|(agent, _)| is_routable(agent))
        .map(|(agent, reason)| (agent.to_string(), reason))
        .unwrap_or_else(|| ("macro_forecaster".to_string(), RouteReason::Default))
}

/// The best routable agent that DECLARES this domain.
///
/// The head of [`declared_specialists_ranked`], which documents the ordering.
/// Prefer the plural form when the runner-up matters — "one agent is carrying
/// four of five drivers" is only actionable if something knows whether a
/// second claimant exists.
///
/// There used to be a second, weaker version of this taking a roster without
/// the explicit-declaration flag. Both were live: the tests exercised this
/// one and the console called that one, so an agent merely TAGGED for search
/// could outrank one that had actually declared the domain — but only in
/// production. One ranking now, so the two cannot disagree again.
pub fn declared_specialist_ranked(
    domain: &str,
    roster: &[(String, Vec<String>, bool)],
    is_routable: &dyn Fn(&str) -> bool,
) -> Option<String> {
    declared_specialists_ranked(domain, roster, &[], is_routable)
        .into_iter()
        .next()
}

/// What the platform has MEASURED about an agent's own contribution, as
/// opposed to what the agent's card claims.
///
/// # This is deliberately NOT the agent's Brier score
///
/// The obvious number — "mean Brier of the forecasts this agent worked
/// on" — cannot rank agents, and the server says so in as many words
/// (`src/calibration.rs`, `score_scope: "team"`):
///
/// > `brier_mean` averages the Brier of forecasts this agent
/// > participated in, which is a property of the composition, not of the
/// > agent. When every member is cited on every forecast those team
/// > numbers are identical across members by construction and can never
/// > rank them.
///
/// A router fed team Brier would order co-cited agents identically
/// forever, at any sample size, and would look like it was working. The
/// platform already computes the identifiable quantity instead: an exact
/// Shapley decomposition of each resolved forecast's improvement over its
/// no-agent baseline (`src/attribution/`), which is per-agent by
/// construction and sums exactly to the team's total.
///
/// # Orientation
///
/// `mean_shapley` is positively oriented: higher is a larger contribution
/// toward the truth. It is signed — an agent that dragged a forecast away
/// from the outcome carries negative credit even when its team improved.
#[derive(Debug, Clone, PartialEq)]
pub struct Proven {
    pub agent: String,
    /// Mean Shapley credit across resolved forecasts. Higher is better.
    pub mean_shapley: f64,
    /// Resolved forecasts backing `mean_shapley`.
    pub n_forecasts: u32,
    /// Lower bound of the cluster bootstrap CI, or `None` when the server
    /// declined to compute one.
    ///
    /// `None` is meaningful and must not be read as zero: below
    /// `attribution::MIN_BOOTSTRAP_CLUSTERS` distinct clusters there is no
    /// replication to resample, so the data contain no information at all
    /// about between-cluster variability.
    pub ci_low: Option<f64>,
}

impl Proven {
    /// Whether this record is strong enough to outrank a declaration.
    ///
    /// The test is that the agent's credit is distinguishable from zero,
    /// not that some arbitrary number of forecasts have accumulated. A
    /// count threshold would be a guess at the question the bootstrap
    /// interval already answers properly, and it would pass an agent with
    /// fifty forecasts and no detectable effect while failing one with
    /// four and a decisive one.
    ///
    /// It also subsumes the sample-size floor for free: the server returns
    /// no interval below `attribution::MIN_BOOTSTRAP_CLUSTERS` clusters, so
    /// a thin record cannot pass this test however good its mean looks.
    /// That is the guard against the first agent to get lucky being
    /// promoted over every specialist.
    pub fn is_established(&self) -> bool {
        self.ci_low.is_some_and(|lo| lo > 0.0)
    }
}

/// Every routable agent that declares this domain, best first.
///
/// Ordering, in priority order:
///   1. a MEASURED record, when it is distinguishable from zero — higher
///      Shapley contribution first. What an agent has DONE outranks what
///      its card says. See [`Proven`] for why this is a contribution and
///      not a Brier score.
///   2. explicit `metadata.domains` before a `metadata.tags` fallback —
///      tags are written for search, not routing
///   3. fewer domains before more — narrower claim wins, so a generalist
///      tagging itself with twenty subjects cannot crowd out a specialist
///      claiming one
///   4. agent id, so the choice is identical run to run
///
/// With an empty `record` this reduces exactly to rules 2–4, which is the
/// ordering that shipped before measurement existed.
///
/// An agent with an explicitly EMPTY `domains: []` never appears here at
/// all, which is how a composition's members stay out of the router's way.
pub fn declared_specialists_ranked(
    domain: &str,
    roster: &[(String, Vec<String>, bool)],
    record: &[Proven],
    is_routable: &dyn Fn(&str) -> bool,
) -> Vec<String> {
    let d = domain.trim().to_ascii_lowercase().replace('-', "_");
    if d.is_empty() || d == "general" {
        return Vec::new();
    }
    let matches_domain = |t: &String| {
        let t = t.trim().to_ascii_lowercase().replace('-', "_");
        t == d || d.split('_').any(|p| p == t) || t.split('_').any(|p| p == d)
    };

    // An agent's measured contribution, but only once it is
    // distinguishable from zero.
    let proven = |id: &str| -> Option<f64> {
        record
            .iter()
            .find(|p| p.agent == id && p.is_established())
            .map(|p| p.mean_shapley)
    };

    let mut hits: Vec<(&String, Option<f64>, bool, usize)> = roster
        .iter()
        .filter(|(id, domains, _)| {
            !GENERALIST_AGENTS.contains(&id.as_str())
                && domains.iter().any(matches_domain)
                && is_routable(id)
        })
        .map(|(id, domains, explicit)| (id, proven(id), *explicit, domains.len()))
        .collect();

    hits.sort_by(|a, b| {
        // Measured before unmeasured, then by score. Descending, because
        // Shapley credit is positively oriented — the opposite direction
        // from the Brier score this used to be, and a silent sign error
        // here would rank the worst contributor first while looking
        // entirely plausible.
        match (a.1, b.1) {
            (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| b.2.cmp(&a.2)) // explicit first
        .then_with(|| a.3.cmp(&b.3)) // narrower claim first
        .then_with(|| a.0.cmp(b.0)) // deterministic
    });
    hits.into_iter().map(|(id, _, _, _)| id.clone()).collect()
}

pub fn detect_domain(question: &str) -> String {
    let q = question.to_lowercase();
    // Whole-word matching throughout. Bare `contains` here meant
    // "war" matched "warming", routing every climate question to
    // the politics branch (which is checked before climate).
    let w = |needle: &str| contains_word(&q, needle);

    // Sports — NBA / basketball (check BEFORE general sports)
    if w("nba")
        || w("lakers")
        || w("celtics")
        || w("knicks")
        || w("warriors")
        || w("nuggets")
        || w("bucks")
        || w("76ers")
        || w("basketball")
        || (w("playoff") && (w("game") || w("series")))
    {
        return "sports_nba".into();
    }

    // Sports — football / soccer
    if w("champions league")
        || w("premier league")
        || w("world cup")
        || w("euro")
        || w("europa league")
        || w("la liga")
        || w("bundesliga")
        || w("serie a")
        || w("ligue 1")
        || w("uefa")
        || w("fifa")
        || w("bayern")
        || w("barcelona")
        || w("real madrid")
        || w("manchester")
        || w("liverpool")
        || w("arsenal")
        || w("psg")
        || w("juventus")
        || w("inter milan")
        || w("soccer")
        || w("football") && !w("nfl")
    {
        return "sports_football".into();
    }

    // Stocks / equity — specific company financial analysis
    if w("stock price")
        || w("share price")
        || w("earnings per share")
        || w("eps ")
        || w("p/e ratio")
        || w("dcf")
        || w("intrinsic value")
        || w("market cap")
        || w("ipo")
        || w("quarterly earnings")
        || w("revenue beat")
        || w("analyst estimate")
        || w("price target")
        || w("stock split")
        || (w("valuation") && (w("company") || w("stock")))
    {
        return "stocks".into();
    }

    // Sports — NFL
    if w("nfl") || w("super bowl") || w("touchdown") || w("quarterback") {
        return "sports_nfl".into();
    }

    // Sports — general / other
    if w("olympics") || w("tennis") || w("f1") || w("formula 1") || w("eurovision") {
        return "sports_other".into();
    }

    // Biotech / pharma
    if w("fda")
        || w("clinical trial")
        || w("drug")
        || w("pharma")
        || w("biotech")
        || w("approval") && (w("drug") || w("therapy") || w("treatment"))
        || w("phase 1")
        || w("phase 2")
        || w("phase 3")
        || w("oncology")
        || w("crispr")
        || w("mrna")
    {
        return "biotech".into();
    }

    // Finance / stocks
    if w("stock")
        || w("share price")
        || w("revenue")
        || w("earnings")
        || w("valuation")
        || w("ipo")
        || w("nasdaq")
        || w("s&p")
        || w("dow")
        || w("market cap")
        || w("dividend")
        || w("quarterly")
    {
        return "finance".into();
    }

    // Politics / geopolitics
    if w("election")
        || w("vote")
        || w("president")
        || w("congress")
        || w("senate")
        || w("parliament")
        || w("referendum")
        || w("war")
        || w("conflict")
        || w("nato")
        || w("sanctions")
        || w("treaty")
    {
        return "politics".into();
    }

    // Technology
    if w(" ai ")
        || w("artificial intelligence")
        || w("software")
        || w("chip")
        || w("semiconductor")
        || w("quantum")
        || w("spacex")
        || w("satellite")
        || w("autonomous")
        || w("robotics")
    {
        return "technology".into();
    }

    // Climate / energy / weather
    //
    // Weather vocabulary was missing entirely, so "Will the highest
    // temperature in London be 32C on August 14?" classified as
    // `general`. That is not merely cosmetic: `domain` selects the
    // research-query template and gates the specialist lookup.
    //
    // Precipitation vocabulary was still missing after that fix: "Will it rain
    // in Amsterdam on Saturday?" classified as `general` and routed to the
    // generalist, because only the noun `rainfall` was listed and
    // `contains_word` deliberately does not do prefix matching.
    if w("rain")
        || w("rains")
        || w("raining")
        || w("snow")
        || w("snowing")
        || w("precipitation")
        || w("sleet")
        || w("hail")
        || w("blizzard")
        || w("monsoon")
        || w("climate")
        || w("carbon")
        || w("emission")
        || w("renewable")
        || w("solar")
        || w("wind power")
        || w("nuclear") && w("energy")
        || w("fusion")
        || w("warming")
        || w("temperature")
        || w("heatwave")
        || w("heat wave")
        || w("rainfall")
        || w("snowfall")
        || w("hurricane")
        || w("drought")
        || w("weather")
    {
        return "climate".into();
    }

    "general".into()
}
#[cfg(test)]
mod routing_tests {
    use super::*;

    // ── Declaration-driven domain routing ───────────────────────────────
    //
    // The regression these pin: `domain_specialist` is a `match` over four
    // domains, so a climate question found no specialist and every driver fell
    // to the generalist. Two production forecasts returned their own
    // climatological base rate — London 32C at 0.3% against a market of 13.5%,
    // Chicago 75F at 23.2% against 0.5%.

    /// Load the real on-disk cards, so these tests fail if a card regresses.
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

    #[test]
    fn a_weather_question_routes_to_the_weather_front_agent() {
        let roster = real_roster();
        let routable = |_: &str| true;

        for q in [
            "Will the highest temperature in Chicago be 75F or below on August 15?",
            "Will the highest temperature in London be 32C on August 14?",
            "Will it rain in Amsterdam on Saturday?",
            "Will snowfall in Denver exceed 6 inches?",
        ] {
            let domain = detect_domain(q);
            assert_eq!(domain, "climate", "domain for {q:?}");

            // The compile-time table still has nothing for climate; the point
            // is that routing no longer depends on it.
            assert_eq!(domain_specialist(&domain), None);

            let declared = declared_specialist_ranked(&domain, &roster, &routable);
            assert_eq!(
                declared.as_deref(),
                Some("weather_oracle"),
                "climate must route to the weather front agent for {q:?}"
            );

            // And it must survive a generalist suggestion from Fermi, which is
            // exactly what happened in production.
            let (agent, reason) = select_agent_for_driver_declared(
                "synoptic_pattern_august",
                "requires a specific synoptic setup",
                &domain,
                Some("macro_forecaster"),
                declared.as_deref(),
                &routable,
            );
            assert_eq!(
                agent, "weather_oracle",
                "generalist displaced the specialist for {q:?}"
            );
            assert_eq!(reason, RouteReason::DeclaredSpecialist);
        }
    }

    #[test]
    fn composition_members_declare_out_and_are_never_routed_to() {
        // A member with an explicit `domains: []` must not win the route. Left
        // to the tag fallback, `weather_calibrator` (tagged "weather") beat
        // `weather_oracle` on the narrower-claim tie-break — and would have
        // been handed a raw driver it is explicitly built not to research.
        let roster = real_roster();
        let ids: Vec<&str> = roster.iter().map(|(i, _, _)| i.as_str()).collect();
        for member in [
            "weather_calibrator",
            "weather_ensemble_forecaster",
            "weather_market_analyst",
        ] {
            assert!(
                !ids.contains(&member),
                "{member} declares domains and is therefore directly routable;                  composition members must declare `domains: []`"
            );
        }
    }

    #[test]
    fn an_explicit_declaration_outranks_a_tag_match() {
        let roster = vec![
            // Tag fallback: three tags, one of which happens to match.
            (
                "tag_matcher".to_string(),
                vec!["weather".to_string()],
                false,
            ),
            // Explicit, and deliberately WIDER, so it can only win on being
            // explicit rather than on the narrower-claim tie-break.
            (
                "declarer".to_string(),
                vec![
                    "weather".to_string(),
                    "climate".to_string(),
                    "temperature".to_string(),
                ],
                true,
            ),
        ];
        assert_eq!(
            declared_specialist_ranked("weather", &roster, &|_| true).as_deref(),
            Some("declarer"),
            "tags are written for search, not routing; an explicit declaration must win"
        );
    }

    #[test]
    fn a_narrower_claim_wins_among_equally_explicit_agents() {
        let roster = vec![
            (
                "jack_of_all".to_string(),
                (0..12)
                    .map(|i| format!("d{i}"))
                    .chain(["climate".to_string()])
                    .collect(),
                true,
            ),
            ("specialist".to_string(), vec!["climate".to_string()], true),
        ];
        assert_eq!(
            declared_specialist_ranked("climate", &roster, &|_| true).as_deref(),
            Some("specialist"),
            "an agent claiming everything must not crowd out one claiming this"
        );
    }

    #[test]
    fn general_and_unroutable_never_produce_a_declared_specialist() {
        let roster = real_roster();
        assert_eq!(
            declared_specialist_ranked("general", &roster, &|_| true),
            None
        );
        assert_eq!(declared_specialist_ranked("", &roster, &|_| true), None);
        // Nothing routable => no declared specialist, even with matches.
        assert_eq!(
            declared_specialist_ranked("climate", &roster, &|_| false),
            None
        );
    }

    #[test]
    fn precipitation_vocabulary_is_recognised() {
        // `contains_word` does no prefix matching, so listing only "rainfall"
        // left "Will it rain..." classified as `general`.
        for q in [
            "will it rain tomorrow",
            "how much snow will fall",
            "total precipitation in July",
            "will there be hail",
            "monsoon onset date",
        ] {
            assert_eq!(detect_domain(q), "climate", "{q:?} should be climate");
        }
        // And the old false-positive guard still holds.
        assert_ne!(
            detect_domain("will the training programme finish"),
            "climate"
        );
    }

    #[test]
    fn declared_specialist_does_not_change_non_weather_routing() {
        // Regression guard: adding a rung must not steal routes that already
        // worked through the hardcoded table.
        let roster = real_roster();
        let routable = |_: &str| true;
        let nba = detect_domain("Will the Lakers win the NBA championship?");
        assert_eq!(nba, "sports_nba");
        let (agent, _) = select_agent_for_driver_declared(
            "elo_rating",
            "team strength",
            &nba,
            None,
            declared_specialist_ranked(&nba, &roster, &routable).as_deref(),
            &routable,
        );
        assert_eq!(agent, "nba_analyst");
    }

    /// Stands in for a console whose local card directory resolved.
    fn all_available(_: &str) -> bool {
        true
    }

    /// Stands in for the far more common case: a packaged install with
    /// no `agents/curated/` on disk. The orchestra is still routable
    /// because the ABW server resolves it.
    fn orchestra_only(a: &str) -> bool {
        FERMI_ORCHESTRA.contains(&a)
    }

    fn route(driver: &str, rationale: &str, domain: &str, suggested: Option<&str>) -> String {
        select_agent_for_driver(driver, rationale, domain, suggested, &all_available).0
    }

    // ── The reported regression ─────────────────────────────
    //
    // "Will Manchester City win the 2026-27 English Premier League
    // (EPL) Championship?" decomposed into five drivers, and all five
    // were handed to macro_forecaster despite football_analyst being
    // hired into the workspace. Rationales below are the verbatim ones
    // Fermi produced in that run.

    const EPL_Q: &str =
        "Will Manchester City win the 2026-27 English Premier League (EPL) Championship?";

    const SQUAD: &str = "By 2026-27, Guardiola will be 55-56 years old with 10+ years at City. \
         Key players (De Bruyne, Walker, Stones) will be 35+. Squad refresh quality, managerial \
         succession if Guardiola departs, and ability to replace aging core are critical.";

    const LANDSCAPE: &str = "Arsenal, Liverpool, and Newcastle (post-Saudi investment) are \
         strengthening. Chelsea and Manchester United remain well-funded. Multi-club competition \
         for top talent intensifies.";

    const REGULATORY: &str = "Manchester City faces 115+ FFP charges from Premier League. \
         Hearing concluded late 2024, verdict expected 2025. Possible sanctions: points \
         deduction, transfer ban, or relegation.";

    const INJURY: &str = "2026-27 season follows 2026 World Cup. Compressed pre-season, player \
         fatigue, and injury risk elevated for clubs with many internationals. Historical \
         pattern: post-World Cup seasons show 8-12% increase in muscle injuries.";

    const TACTICAL: &str = "EPL tactical evolution: increasing prevalence of high-press, \
         counter-attacking systems designed to exploit City's possession style. By 2026-27, \
         league-wide tactical adaptation may erode City's systemic edge.";

    #[test]
    fn epl_question_is_detected_as_football() {
        assert_eq!(detect_domain(EPL_Q), "sports_football");
    }

    #[test]
    fn epl_drivers_route_to_the_football_specialist() {
        let d = "sports_football";
        assert_eq!(
            route("squad_quality_retention", SQUAD, d, None),
            "football_analyst"
        );
        assert_eq!(
            route("competitive_landscape", LANDSCAPE, d, None),
            "football_analyst"
        );
        assert_eq!(
            route("injury_fixture_congestion", INJURY, d, None),
            "football_analyst"
        );
        assert_eq!(
            route("tactical_meta_shift", TACTICAL, d, None),
            "football_analyst"
        );
    }

    #[test]
    fn ffp_driver_is_cross_cutting_and_leaves_the_specialist_behind() {
        // A football analyst has no read on Premier League disciplinary
        // proceedings. This is the one EPL driver that should NOT go to
        // football_analyst.
        let (agent, reason) = select_agent_for_driver(
            "regulatory_financial_risk",
            REGULATORY,
            "sports_football",
            None,
            &all_available,
        );
        assert_eq!(agent, "entity_investigator");
        assert_eq!(reason, RouteReason::CrossCutting);
    }

    // ── Root cause 1: local-registry availability check ───────────

    #[test]
    fn specialist_still_routes_without_a_local_card_directory() {
        // The old code probed `registry.get()`, which fails wholesale on
        // installs where `agents/curated/` wasn't found. Agents execute
        // server-side, so that must not change the routing decision.
        let (agent, _) = select_agent_for_driver(
            "tactical_meta_shift",
            TACTICAL,
            "sports_football",
            Some("football_analyst"),
            &orchestra_only,
        );
        assert_eq!(agent, "football_analyst");
    }

    // ── Root cause 2: the dead fallback chain ───────────────────

    #[test]
    fn unroutable_suggestion_falls_through_to_the_next_best_expert() {
        // Fermi names an agent nobody has. The old chain retried the
        // domain agent (identical to the failed candidate in the sports
        // case) and then hardcoded macro_forecaster. It should instead
        // walk down to the domain specialist.
        let only_known = |a: &str| a != "some_third_party_agent";
        let (agent, reason) = select_agent_for_driver(
            "injury_fixture_congestion",
            INJURY,
            "sports_football",
            Some("some_third_party_agent"),
            &only_known,
        );
        assert_eq!(agent, "football_analyst");
        assert_eq!(reason, RouteReason::DomainSpecialist);
    }

    #[test]
    fn third_party_suggestion_is_honoured_when_the_server_knows_it() {
        let with_third_party = |_: &str| true;
        let (agent, reason) = select_agent_for_driver(
            "pitch_conditions",
            "Venue and turf quality across the fixture list.",
            "sports_football",
            Some("fixture_context_agent"),
            &with_third_party,
        );
        assert_eq!(agent, "fixture_context_agent");
        assert_eq!(reason, RouteReason::Fermi);
    }

    // ── Root cause 3: generalist suggestions displacing the expert ──

    #[test]
    fn generalist_suggestion_does_not_displace_the_domain_specialist() {
        let (agent, reason) = select_agent_for_driver(
            "squad_quality_retention",
            SQUAD,
            "sports_football",
            Some("macro_forecaster"),
            &all_available,
        );
        assert_eq!(agent, "football_analyst");
        assert_eq!(reason, RouteReason::DomainSpecialist);
    }

    #[test]
    fn generalist_suggestion_survives_on_a_cross_cutting_driver() {
        // The guard must not be blind: when the driver really is macro,
        // Fermi naming the generalist is the right call.
        let (agent, reason) = select_agent_for_driver(
            "broadcast_revenue_shock",
            "UK inflation and interest rate path compress broadcast rights valuations.",
            "sports_football",
            Some("macro_forecaster"),
            &all_available,
        );
        assert_eq!(agent, "macro_forecaster");
        assert_eq!(reason, RouteReason::Fermi);
    }

    #[test]
    fn non_generalist_suggestion_always_wins_over_the_specialist() {
        // sentiment_analyzer is a considered choice, not a default, so
        // the guard leaves it alone.
        let (agent, reason) = select_agent_for_driver(
            "fanbase_pressure",
            "Supporter unrest after a poor start to the season.",
            "sports_football",
            Some("sentiment_analyzer"),
            &all_available,
        );
        assert_eq!(agent, "sentiment_analyzer");
        assert_eq!(reason, RouteReason::Fermi);
    }

    // ── Other domains keep working ───────────────────────────

    // ── Substring hazards ──────────────────────────────────

    // ── The reported regression: a US presidential election ─────────
    //
    // Observed 2026-08-22 on "Will Alexandria Ocasio-Cortez win the 2028 US
    // Presidential Election?". Five drivers, and the activity log recorded:
    //
    //   democratic_primary_viability  → energy_advisor
    //   national_sentiment_shift      → macro_forecaster
    //   republican_opponent_strength  → macro_forecaster
    //   aoc_political_capital_growth  → entity_investigator
    //   economic_conditions_2027_2028 → macro_forecaster
    //
    // Four of the five were wrong, and one of them was wrong in a way that
    // could not be recovered from: `energy_advisor` is a SimOps agent that
    // answers JSON task payloads about kWh per input unit. Rationales below
    // are the verbatim ones Fermi produced in that run.

    const PRIMARY: &str = "AOC's progressive brand faces structural headwinds in Democratic \
         primaries, which since 1972 have favored centrist candidates 11/14 times. However, \
         demographic shifts (younger, more diverse electorate), Sanders' 2016/2020 near-misses, \
         and potential lack of strong centrist heir in 2028 create upside. Downside: party \
         establishment resistance, fundraising disadvantage vs governors/senators. Upside: \
         movement energy, small-donor base, media fluency.";

    const SENTIMENT: &str = "Public support for Medicare-for-All, Green New Deal, and wealth \
         taxes has fluctuated 35-55% in polls 2018-2024. Economic conditions in 2027-28 \
         (recession, inequality trends, climate events) could shift this dramatically. P50 \
         slightly above 1.0 reflects modest leftward drift in Dem base; p95 at 1.6 allows for \
         major economic crisis creating appetite for structural change (cf. FDR 1932). P5 at \
         0.7 reflects backlash scenario (inflation fears, moderate restoration).";

    const OPPONENT: &str =
        "Strength of the eventual Republican nominee and the party's coalition heading into 2028.";
    const CAPITAL: &str =
        "Committee assignments, fundraising totals and national profile through 2027.";
    const ECONOMY: &str = "GDP growth, unemployment and inflation path into the election year.";

    fn aoc_drivers() -> [(&'static str, &'static str); 5] {
        [
            ("democratic_primary_viability", PRIMARY),
            ("national_sentiment_shift", SENTIMENT),
            ("republican_opponent_strength", OPPONENT),
            ("aoc_political_capital_growth", CAPITAL),
            ("economic_conditions_2027_2028", ECONOMY),
        ]
    }

    #[test]
    fn movement_energy_in_a_political_rationale_is_not_an_energy_driver() {
        // Two words of metaphor — "Upside: movement energy" — sixty words
        // into a rationale about Democratic primaries. The old ladder was a
        // first-match chain with `energy` above `sentiment` and `entity`, so
        // those two words decided the route outright.
        let (agent, reason) = select_agent_for_driver(
            "democratic_primary_viability",
            PRIMARY,
            "politics",
            None,
            &all_available,
        );
        assert_ne!(agent, "energy_advisor", "the metaphor won again");
        assert_eq!(agent, "entity_investigator");
        assert_eq!(reason, RouteReason::Keyword);
    }

    #[test]
    fn the_simops_energy_agent_is_never_auto_routed() {
        // The class of bug, not the instance. `energy_advisor`'s card is a
        // SimOps member that answers `propose_stage_energy` task payloads;
        // handing it any forecast driver is a category error regardless of
        // which keyword got there. It must not appear in the scored table
        // at all — it stays in FERMI_ORCHESTRA only so an operator can hire
        // it deliberately.
        assert!(
            !RUNGS.iter().any(|r| r.agent == "energy_advisor"),
            "a SimOps member agent is auto-routable again"
        );
        assert!(FERMI_ORCHESTRA.contains(&"energy_advisor"));
    }

    #[test]
    fn a_driver_named_for_sentiment_survives_economic_context() {
        // `national_sentiment_shift` says what it is in its own name. Its
        // rationale mentions recession, inflation, "economic crisis" and
        // "economic conditions" — four macro words against a driver that is
        // about public opinion. Presence-testing the concatenated text sent
        // it to macro_forecaster; sharing the prose budget does not.
        let (agent, _) = select_agent_for_driver(
            "national_sentiment_shift",
            SENTIMENT,
            "politics",
            None,
            &all_available,
        );
        assert_eq!(agent, "sentiment_analyzer");

        // And the macro reading is still recorded — as context, not topic.
        let scored = route_candidates("national_sentiment_shift", SENTIMENT, "politics");
        let macro_score = scored
            .iter()
            .find(|(a, _)| *a == "macro_forecaster")
            .map(|(_, s)| *s)
            .expect("macro context should still register");
        let top = scored[0].1;
        assert!(macro_score < top, "context outscored topic: {scored:?}");
    }

    #[test]
    fn a_presidential_election_does_not_collapse_onto_the_generalist() {
        // The shape of the complaint, asserted directly: three of five
        // drivers went to macro_forecaster, which was simultaneously the
        // broadest keyword rung and the hardcoded default, so it could not
        // lose. Only the one driver that is actually about the economy may
        // land there now.
        let assigned: Vec<String> = aoc_drivers()
            .iter()
            .map(|(n, r)| select_agent_for_driver(n, r, "politics", None, &orchestra_only).0)
            .collect();

        let macro_count = assigned.iter().filter(|a| *a == "macro_forecaster").count();
        assert_eq!(
            macro_count, 1,
            "generalist monoculture on a political decomposition: {assigned:?}"
        );
        assert_eq!(
            assigned[4], "macro_forecaster",
            "the one genuinely macro driver must still reach the macro agent: {assigned:?}"
        );
        assert!(
            !assigned.iter().any(|a| a == "energy_advisor"),
            "{assigned:?}"
        );
    }

    #[test]
    fn every_political_driver_is_a_deliberate_choice() {
        // A `Default` route means the router had nothing to say and the
        // generalist was the honest fallback. Before the electoral
        // vocabulary existed, that was the truthful description of most of
        // this forecast — it just was not what got logged.
        for (n, r) in aoc_drivers() {
            let (agent, reason) = select_agent_for_driver(n, r, "politics", None, &orchestra_only);
            assert!(
                reason.is_deliberate(),
                "{n} fell through to {agent} with no signal"
            );
        }
    }

    // ── The scoring rules themselves ────────────────────────────────

    #[test]
    fn prose_alone_cannot_take_a_driver_from_the_resident_expert() {
        // One legal word buried in a football rationale must not outrank
        // the football analyst. Displacing a resident expert requires the
        // driver's NAME to say so — which is exactly what the FFP driver
        // does, and what this one does not.
        let (agent, reason) = select_agent_for_driver(
            "squad_depth",
            "Rotation options across a congested fixture list; one player is subject to an \
             ongoing investigation.",
            "sports_football",
            None,
            &all_available,
        );
        assert_eq!(agent, "football_analyst");
        assert_eq!(reason, RouteReason::DomainSpecialist);
    }

    #[test]
    fn a_domain_bound_rung_cannot_claim_a_foreign_driver_on_prose() {
        // `equity_analyst` owns "valuation". A football driver whose
        // rationale happens to mention broadcast-rights valuations is not
        // an equity research assignment, and the rung is dropped before it
        // can even dilute the prose share of the agents that do belong.
        let scored = route_candidates(
            "broadcast_revenue_shock",
            "UK inflation and interest rate path compress broadcast rights valuations.",
            "sports_football",
        );
        assert!(
            !scored.iter().any(|(a, _)| *a == "equity_analyst"),
            "a foreign rung scored on prose: {scored:?}"
        );
    }

    #[test]
    fn a_name_hit_outranks_a_rationale_that_talks_about_something_else() {
        // The invariant the whole table rests on, stated once.
        let scored = route_candidates(
            "regulatory_review_outcome",
            "Demand, pricing and customer adoption are all strong; revenue is growing.",
            "general",
        );
        assert_eq!(scored[0].0, "entity_investigator", "{scored:?}");
        assert!(scored.iter().any(|(a, _)| *a == "market_research"));
    }

    #[test]
    fn the_scored_table_is_deterministic() {
        // A surprising assignment has to be reproducible when someone goes
        // looking for it, so ties break on agent id rather than on the
        // iteration order of the table.
        let a = route_candidates(
            "competitor_response",
            "Rival pricing and customer adoption.",
            "general",
        );
        let b = route_candidates(
            "competitor_response",
            "Rival pricing and customer adoption.",
            "general",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn contains_word_respects_boundaries() {
        assert!(!contains_word("pre-industrial warming", "trial"));
        assert!(contains_word("phase 3 clinical trial", "trial"));
        assert!(!contains_word("squad development plan", "elo"));
        assert!(contains_word("current elo rating", "elo"));
        assert!(contains_word("home court advantage", "home court"));
        assert!(!contains_word("telephone", "elo"));
        assert!(!contains_word("global warming trend", "war"));
        assert!(contains_word("the war in ukraine", "war"));
    }

    #[test]
    fn contains_word_absorbs_plurals() {
        // The old lists leaned on substring matching for plurals; word
        // matching must not silently drop them.
        assert!(contains_word("new sanctions imposed", "sanction"));
        assert!(contains_word("the playoffs begin", "playoff"));
        assert!(contains_word("carbon emissions", "emission"));
        assert!(contains_word("115 ffp charges", "charge"));
        assert!(contains_word("general elections", "election"));
    }

    // ── detect_domain ───────────────────────────────────────

    #[test]
    fn warming_is_not_war() {
        // `q.contains("war")` matched "warming", and the politics branch
        // is checked before climate — so climate questions were
        // classified as geopolitics.
        assert_eq!(
            detect_domain("Will global warming push 2027 above 1.5C?"),
            "climate"
        );
        assert_eq!(
            detect_domain("Will the war in Ukraine end in 2027?"),
            "politics"
        );
    }

    #[test]
    fn euro_tournaments_still_detected() {
        // The needle was "euro 20", which whole-word matching would
        // never hit; it is now plain "euro".
        assert_eq!(detect_domain("Who wins Euro 2028?"), "sports_football");
    }

    #[test]
    fn nba_playoffs_plural_still_detected() {
        assert_eq!(
            detect_domain("Will the Celtics reach the playoffs series?"),
            "sports_nba"
        );
    }

    #[test]
    fn climate_warming_driver_does_not_route_to_biotech() {
        // Observed in the wild: the picker recommended biotech_analyst
        // for 'climate_trend_warming' because the rationale said
        // "pre-industrial" and the ladder did a bare contains("trial").
        let (agent, _) = select_agent_for_driver(
            "climate_trend_warming",
            "London is ~1.2\u{b0}C warmer than pre-industrial levels; 30\u{b0}C+ days have \
             quadrupled since the 1961-1990 baseline.",
            "general",
            None,
            &all_available,
        );
        assert_ne!(agent, "biotech_analyst", "substring hazard resurfaced");
    }

    #[test]
    fn nba_question_routes_to_the_nba_specialist() {
        assert_eq!(
            route(
                "home_court_advantage",
                "Denver's altitude and crowd effect on net rating.",
                "sports_nba",
                Some("macro_forecaster"),
            ),
            "nba_analyst"
        );
    }

    #[test]
    fn biotech_trial_driver_routes_to_the_biotech_specialist() {
        assert_eq!(
            route(
                "phase3_readout",
                "Primary endpoint readout for the Phase 3 oncology trial.",
                "biotech",
                None,
            ),
            "biotech_analyst"
        );
    }

    #[test]
    fn domainless_question_uses_the_keyword_ladder() {
        let (agent, reason) = select_agent_for_driver(
            "competitor_response",
            "Rival pricing and customer adoption of the new product line.",
            "general",
            None,
            &all_available,
        );
        assert_eq!(agent, "market_research");
        assert_eq!(reason, RouteReason::Keyword);
    }

    #[test]
    fn no_signal_at_all_falls_back_to_the_generalist() {
        let (agent, reason) =
            select_agent_for_driver("misc_factor", "", "general", None, &all_available);
        assert_eq!(agent, "macro_forecaster");
        assert_eq!(reason, RouteReason::Default);
    }

    #[test]
    fn empty_suggestion_string_is_ignored() {
        let (agent, _) = select_agent_for_driver(
            "tactical_meta_shift",
            TACTICAL,
            "sports_football",
            Some("   "),
            &all_available,
        );
        assert_eq!(agent, "football_analyst");
    }

    #[test]
    fn a_football_question_never_routes_every_driver_to_one_generalist() {
        // The shape of the bug, asserted directly: the five EPL drivers
        // must not collapse onto a single generalist.
        let drivers = [
            ("squad_quality_retention", SQUAD),
            ("competitive_landscape", LANDSCAPE),
            ("regulatory_financial_risk", REGULATORY),
            ("injury_fixture_congestion", INJURY),
            ("tactical_meta_shift", TACTICAL),
        ];
        let assigned: Vec<String> = drivers
            .iter()
            .map(|(n, r)| {
                select_agent_for_driver(
                    n,
                    r,
                    "sports_football",
                    Some("macro_forecaster"),
                    &orchestra_only,
                )
                .0
            })
            .collect();

        assert!(
            !assigned.iter().any(|a| a == "macro_forecaster"),
            "generalist leaked into a football decomposition: {:?}",
            assigned
        );
        assert_eq!(
            assigned.iter().filter(|a| *a == "football_analyst").count(),
            4,
            "expected 4 football drivers, got {:?}",
            assigned
        );
    }

    // ── The predicate is a routing gate, not just an availability check ──

    /// An agent the assignment gate would refuse is skipped, not returned.
    ///
    /// `select_agent_for_driver_declared` takes its availability predicate from
    /// the caller, and the console passed `agent_is_routable` — "can anything
    /// execute this id". That is a weaker question than "may this agent be bound
    /// to a driver", which is what `negotiate::admit_assignment` answers, and the
    /// gap let auto-assignment bind an agent the MANUAL path would have refused
    /// outright.
    ///
    /// Observed in production: `energy_advisor` — an energy-balance SimOps
    /// specialist whose `accepts` are `stage_description_json`,
    /// `resource_description_json`, `process_yaml_json`, none of them a free-text
    /// port — auto-assigned to `democratic_primary_viability` on a US
    /// presidential election forecast, staged, and run.
    ///
    /// The console now passes `agent_is_assignable`. This pins the consequence:
    /// a refused candidate is passed over in favour of the next one, so the
    /// driver still gets an agent rather than none.
    #[test]
    fn a_candidate_the_gate_refuses_is_passed_over_for_the_next_one() {
        let refuses_energy_advisor = |a: &str| a != "energy_advisor";

        let (picked, _reason) = select_agent_for_driver_declared(
            "democratic_primary_viability",
            "AOC's progressive brand faces structural headwinds in Democratic primaries",
            "politics",
            Some("energy_advisor"),
            None,
            &refuses_energy_advisor,
        );

        assert_ne!(
            picked, "energy_advisor",
            "an agent that declares no free-text port must not be routed to a \
             driver, however confidently it was suggested"
        );
        assert!(
            !picked.is_empty(),
            "the driver must still get an agent — skipping is the point, \
             stranding the driver is not"
        );
    }

    /// With every candidate refused, the terminal fallback still answers.
    ///
    /// `select_agent_for_driver_declared` ends with `unwrap_or_else`, so
    /// `macro_forecaster` is returned even when the predicate rejects it. That
    /// is deliberate and worth pinning: a driver with a generalist is
    /// recoverable, a driver with no agent silently researches nothing.
    #[test]
    fn a_universally_refusing_predicate_still_yields_a_fallback() {
        let refuses_everything = |_: &str| false;
        let (picked, reason) = select_agent_for_driver_declared(
            "some_driver",
            "some rationale",
            "climate",
            None,
            Some("weather_oracle"),
            &refuses_everything,
        );
        assert_eq!(picked, "macro_forecaster");
        assert_eq!(reason, RouteReason::Default);
    }

    /// A permissive predicate is unchanged by the tightening.
    ///
    /// The guard against over-correcting: the weather specialist must still win
    /// a climate question, since `weather_oracle` declares `forecast-question`
    /// and passes the gate.
    #[test]
    fn tightening_the_predicate_does_not_disturb_an_admissible_specialist() {
        let all_ok = |_: &str| true;
        let (picked, _) = select_agent_for_driver_declared(
            "ensemble_spread",
            "GEFS ensemble spread at lead 1",
            "climate",
            None,
            Some("weather_oracle"),
            &all_ok,
        );
        assert_eq!(picked, "weather_oracle");
    }
}
