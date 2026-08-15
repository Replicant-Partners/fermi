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

/// Concerns that sit *outside* any domain specialist's remit.
///
/// A football analyst can tell you what fixture congestion does to xG.
/// It cannot tell you how the Premier League's 115 FFP charges will be
/// adjudicated. These keywords mark the drivers where the domain expert
/// should stand aside for a cross-cutting specialist — and are the only
/// reason a specialist-domain question ever routes elsewhere.
fn cross_cutting_agent(combined: &str) -> Option<&'static str> {
    let has = |needles: &[&str]| needles.iter().any(|n| contains_word(combined, n));

    if has(&[
        "regulatory",
        "regulation",
        "legal",
        "lawsuit",
        "litigation",
        // NOT bare "court": it collides with "home_court_advantage",
        // which sent an NBA driver to entity_investigator. Legal
        // adjudication is already covered by the neighbours here.
        "court ruling",
        "court case",
        "tribunal",
        "hearing",
        "compliance",
        "antitrust",
        "investigation",
        "charges",
        "ffp",
        "financial fair play",
        "ownership",
        "governance",
        "takeover",
    ]) {
        return Some("entity_investigator");
    }

    if has(&[
        "macroeconomic",
        "inflation",
        "interest rate",
        "recession",
        "gdp",
        "currency",
        "fiscal",
        "monetary",
        "central bank",
        // Spelled out rather than a `geopolit` prefix — see
        // `contains_word`.
        "geopolitical",
        "geopolitics",
        "sanctions regime",
    ]) {
        return Some("macro_forecaster");
    }

    if has(&[
        "public opinion",
        "fan sentiment",
        "media narrative",
        "social media",
        "press coverage",
    ]) {
        return Some("sentiment_analyzer");
    }

    None
}

/// Keyword ladder over the driver name + rationale.
///
/// Returns `None` when nothing matches, so callers can distinguish "the
/// text says market share" from "the text says nothing in particular".
/// The old inline version returned `macro_forecaster` for the latter,
/// which made a no-signal driver indistinguishable from a macro driver.
fn keyword_agent(combined: &str, domain: &str) -> Option<&'static str> {
    let has = |needles: &[&str]| needles.iter().any(|n| contains_word(combined, n));

    // Domain specialists first — a football driver that also mentions
    // "competition" is still a football driver.
    if has(&[
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
        "tactical",
        "formation",
        "pressing",
        "possession",
        "manager",
        "striker",
        "midfield",
        "defence",
        "defense",
    ]) && (domain == "sports_football" || has(&["football", "soccer", "league", "club"]))
    {
        return Some("football_analyst");
    }

    if has(&["nba", "basketball", "home court", "netrtg"]) {
        return Some("nba_analyst");
    }

    // "trial" is whole-word here: `pre-industrial` must not read as a
    // clinical trial. `contains_word` treats `-` as a boundary, so
    // "clinical trial" and "trial readout" still match.
    if has(&["clinical", "trial", "fda", "drug", "indication"]) {
        return Some("biotech_analyst");
    }

    if has(&[
        "stock",
        "equity",
        "eps",
        "p/e",
        "earnings",
        "share price",
        "shareholder",
        "valuation",
    ]) {
        return Some("equity_analyst");
    }

    if has(&[
        "energy",
        "oil",
        "renewable",
        "solar",
        "wind power",
        "carbon",
        "emission",
    ]) {
        return Some("energy_advisor");
    }

    if has(&[
        "sentiment",
        "opinion",
        "perception",
        "buzz",
        "narrative",
        "protest",
        "unrest",
        "dissent",
    ]) {
        return Some("sentiment_analyzer");
    }

    if has(&[
        "entity",
        "leadership",
        "management",
        "succession",
        "regime",
        "government",
        "military",
        "cohesion",
    ]) {
        return Some("entity_investigator");
    }

    if has(&[
        "market",
        "competitor",
        "partnership",
        "revenue",
        "pricing",
        "demand",
        "adoption",
        "customer",
        "commercial",
        "sales",
    ]) {
        return Some("market_research");
    }

    if has(&[
        "macro",
        "macroeconomic",
        "economic",
        "economy",
        "policy",
        "diplomatic",
        "diplomacy",
        "foreign",
        "international",
        "alliance",
        "trade",
        "crisis",
    ]) {
        return Some("macro_forecaster");
    }

    None
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
    let combined = format!("{} {}", driver_name, rationale).to_lowercase();

    let declared = declared.map(str::trim).filter(|s| !s.is_empty());
    let specialist = domain_specialist(domain);
    let cross = cross_cutting_agent(&combined);
    let keyword = keyword_agent(&combined, domain);

    let suggested = suggested.map(str::trim).filter(|s| !s.is_empty());

    let mut candidates: Vec<(&str, RouteReason)> = Vec::new();

    if let Some(s) = suggested {
        // Guard: Fermi handing back a generalist on a question that has
        // a resident expert is treated as an absent opinion, not as a
        // considered choice.
        //
        // The exception is narrow: the generalist stands only when the
        // cross-cutting analysis independently reached the SAME agent.
        // An earlier version accepted the suggestion whenever `cross`
        // was non-None, which let a `macro_forecaster` suggestion
        // pre-empt the `entity_investigator` that `cross` had actually
        // selected — the FFP driver routed to the generalist anyway.
        //
        // A DECLARED specialist counts as a resident expert here too, which is
        // the fix for weather: Fermi suggested the generalist, no hardcoded
        // specialist existed for `climate`, and every driver went generic.
        let expert_exists = specialist.is_some() || declared.is_some();
        let displaces_specialist =
            expert_exists && GENERALIST_AGENTS.contains(&s) && cross != Some(s);
        if !displaces_specialist {
            candidates.push((s, RouteReason::Fermi));
        }
    }

    // Declaration beats the compile-time table, and beats keyword guesswork.
    if let Some(d) = declared {
        candidates.push((d, RouteReason::DeclaredSpecialist));
    }
    if let Some(c) = cross {
        candidates.push((c, RouteReason::CrossCutting));
    }
    if let Some(s) = specialist {
        candidates.push((s, RouteReason::DomainSpecialist));
    }
    if let Some(k) = keyword {
        candidates.push((k, RouteReason::Keyword));
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

/// The routable agent that most specifically DECLARES this domain.
///
/// `roster` is `(agent_id, declared_domains)`, read from cards by
/// `AgentContract::from_card`. Ties break toward the agent claiming the FEWEST
/// domains, so a generalist tagging itself with twenty subjects cannot crowd
/// out a specialist claiming one; the agent id breaks remaining ties so routing
/// is deterministic across runs.
///
/// Prefer [`declared_specialist_ranked`], which additionally ranks an explicit
/// `metadata.domains` declaration above a tag-fallback match.
pub fn declared_specialist(
    domain: &str,
    roster: &[(String, Vec<String>)],
    is_routable: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let d = domain.trim().to_ascii_lowercase().replace('-', "_");
    if d.is_empty() || d == "general" {
        return None;
    }
    let matches_domain = |t: &String| {
        let t = t.trim().to_ascii_lowercase().replace('-', "_");
        t == d || d.split('_').any(|p| p == t) || t.split('_').any(|p| p == d)
    };

    let mut hits: Vec<(&String, usize)> = roster
        .iter()
        .filter(|(id, domains)| {
            !GENERALIST_AGENTS.contains(&id.as_str())
                && domains.iter().any(matches_domain)
                && is_routable(id)
        })
        .map(|(id, domains)| (id, domains.len()))
        .collect();
    hits.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)));
    hits.first().map(|(id, _)| (*id).clone())
}

/// As [`declared_specialist`], but ranking an EXPLICIT declaration above a tag
/// match.
///
/// `roster` is `(agent_id, domains, domains_are_explicit)`. Ordering:
///   1. explicit `metadata.domains` before a `metadata.tags` fallback — tags
///      are written for search, not routing
///   2. fewer domains before more — narrower claim wins
///   3. agent id, so the choice is deterministic run to run
///
/// An agent with an explicitly EMPTY `domains: []` never appears here at all,
/// which is how a composition's members stay out of the router's way.
pub fn declared_specialist_ranked(
    domain: &str,
    roster: &[(String, Vec<String>, bool)],
    is_routable: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let d = domain.trim().to_ascii_lowercase().replace('-', "_");
    if d.is_empty() || d == "general" {
        return None;
    }
    let matches_domain = |t: &String| {
        let t = t.trim().to_ascii_lowercase().replace('-', "_");
        t == d || d.split('_').any(|p| p == t) || t.split('_').any(|p| p == d)
    };

    let mut hits: Vec<(&String, bool, usize)> = roster
        .iter()
        .filter(|(id, domains, _)| {
            !GENERALIST_AGENTS.contains(&id.as_str())
                && domains.iter().any(matches_domain)
                && is_routable(id)
        })
        .map(|(id, domains, explicit)| (id, *explicit, domains.len()))
        .collect();
    hits.sort_by(|a, b| {
        b.1.cmp(&a.1) // explicit first
            .then_with(|| a.2.cmp(&b.2)) // narrower claim first
            .then_with(|| a.0.cmp(b.0)) // deterministic
    });
    hits.first().map(|(id, _, _)| (*id).clone())
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
}
