//! Why *this* agent was chosen, recorded by the server rather than claimed by
//! the caller.
//!
//! # The defect this exists to close
//!
//! `route:{reason}` is the tag three views are built on — `route_outcomes`,
//! `domain_agent_ranking` and `declaration_quality_outcomes`. All three were
//! empty, and measured against production on 2026-08-29 the tag appeared on
//! **0 of 3,581 episodes**, while its siblings `ibind:` and `qsrc:` appeared on
//! 90 and 18.
//!
//! The cause was not a missing writer. [`crate::stamp_invocation`] writes the
//! tag faithfully — but only from a caller-supplied `route_reason`, and the only
//! producer of that field is `crates/fermi-console`, which the Dockerfile strips
//! out of the workspace because it depends on gpui. **The loop was wired against
//! the desktop console and production traffic comes through a different door.**
//!
//! That is the exact failure `FEEDBACK_LOOPS.md` §2 warns about: every hop has
//! an executing call site, and no production request supplies the input.
//! *Wiring* and *observed turning* are two claims, and this is what it looks
//! like when only the first holds.
//!
//! # It is the `input_binding` fix again
//!
//! The platform already learned this, one gate over. From `handlers::execution`:
//!
//! > check the asking against what the agent advertises — server-side, from the
//! > resolved card, rather than believing the caller's account of it.
//! > `bind_input` shipped in v0.16.0 and was wired only into the desktop
//! > console, so this path never ran it; the episode carried the client's
//! > assertion instead.
//!
//! The scoreboard is the argument: the server-computed tag has 90 rows, the
//! caller-supplied ones have 18 and 0. A caller-supplied route reason is the
//! caller's *claim* about why it chose an agent. The server knows how the agent
//! was actually reached and the caller may never have seen the card.
//!
//! # What this deliberately does not do
//!
//! It does not invent a router. On the HTTP path there is no routing decision to
//! describe — the request named the agent — and the honest tag says exactly
//! that. [`RouteSelection::CallerNamed`] is not a weaker `DomainSpecialist`; it
//! is a different fact, and conflating the two is what the tag exists to
//! prevent:
//!
//! > an agent that underperformed as the generalist fallback is
//! > indistinguishable in outcome data from one deliberately selected as the
//! > resident domain expert and found wanting.
//!
//! Pooling those populations teaches a credit model to distrust whichever agents
//! the router reaches for by default. One honest value that separates
//! *router-selected* from *human-selected* is worth more than six guessed ones.

use agent_bestiary_memory::Episode;

/// How the server knows this agent was reached.
///
/// Disjoint by construction from `fermi_console::routing::RouteReason`
/// (`declared_specialist`, `fermi`, `cross_cutting`, `domain_specialist`,
/// `keyword`, `default`), whose slugs are a wire format that historical
/// comparisons depend on. Nothing here reuses or shadows one of those: a router
/// reason and the absence of a router are different categories, and a shared
/// slug would make them indistinguishable in exactly the aggregate the tag was
/// added to disambiguate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteSelection {
    /// The request named the agent — by id in the path, or by `@mention` in a
    /// workspace. No router was consulted.
    ///
    /// **Deliberate.** A person picking an agent by name is the strongest
    /// selection signal on the platform, and outcomes under it say something
    /// about the agent rather than about a fallback.
    CallerNamed,
}

impl RouteSelection {
    /// The stable identifier persisted in the tag. Treat as a wire format.
    pub fn slug(self) -> &'static str {
        match self {
            RouteSelection::CallerNamed => "caller_named",
        }
    }

    /// Prose, for a log line or a surface. Never a tag suffix — it has spaces.
    pub fn as_str(self) -> &'static str {
        match self {
            RouteSelection::CallerNamed => "named by the caller; no router involved",
        }
    }

    /// Whether the selection carries a positive signal about the agent.
    ///
    /// The inverse is what `route:fallback` marks: nothing matched and the
    /// generalist was the honest answer, so an outcome under it says almost
    /// nothing. A named agent is the opposite of that.
    pub fn deliberate(self) -> bool {
        match self {
            RouteSelection::CallerNamed => true,
        }
    }
}

/// Stamp the server's own account of how this agent was reached.
///
/// Call **unconditionally** at the execute boundary. The whole defect was that
/// [`crate::stamp_invocation`] sits behind `if let Some(invocation)`, so a
/// request that sends no invocation block is recorded with no provenance at all
/// — and most production requests send none.
///
/// Returns `false` and writes nothing when a `route:` reason is already present,
/// so a caller that genuinely knows better — the console, which really did route
/// — keeps its richer answer. The server fills silence; it does not overwrite
/// testimony.
pub fn stamp(episode: &mut Episode, selection: RouteSelection) -> bool {
    stamp_tags(&mut episode.tags, selection)
}

/// The whole of [`stamp`], over the only field it touches.
///
/// Split out because taking an entire `Episode` to append two strings is
/// over-broad, and because a test that has to build a full episode to assert a
/// tag ends up asserting the constructor instead.
pub fn stamp_tags(tags: &mut Vec<String>, selection: RouteSelection) -> bool {
    // `route:fallback` and `route:overrode_fermi` are modifiers rather than
    // reasons, so their presence must not suppress the reason itself.
    const MODIFIERS: [&str; 2] = ["route:fallback", "route:overrode_fermi"];
    let already = tags
        .iter()
        .any(|t| t.starts_with("route:") && !MODIFIERS.contains(&t.as_str()));
    if already {
        return false;
    }
    tags.push(format!("route:{}", selection.slug()));
    if !selection.deliberate() {
        tags.push("route:fallback".to_string());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ep() -> Vec<String> {
        Vec::new()
    }

    #[test]
    fn stamps_a_queryable_reason() {
        let mut e = ep();
        assert!(stamp_tags(&mut e, RouteSelection::CallerNamed));
        assert!(e.contains(&"route:caller_named".to_string()));
    }

    /// A deliberate selection must not also be filed as a fallback: the two
    /// populations are the thing the tag exists to keep apart.
    #[test]
    fn a_named_agent_is_not_a_fallback() {
        let mut e = ep();
        stamp_tags(&mut e, RouteSelection::CallerNamed);
        assert!(!e.iter().any(|t| t == "route:fallback"));
    }

    /// The console really did route, and its answer is richer than ours.
    #[test]
    fn a_real_router_reason_is_not_overwritten() {
        let mut e = ep();
        e.push("route:domain_specialist".to_string());
        assert!(!stamp_tags(&mut e, RouteSelection::CallerNamed));
        assert_eq!(
            e.iter().filter(|t| t.starts_with("route:")).count(),
            1,
            "two route reasons on one episode makes the aggregate uncountable"
        );
    }

    /// A modifier is not a reason. An episode carrying only `route:fallback`
    /// still has no reason, and suppressing the stamp would leave it with a
    /// qualifier and nothing to qualify.
    #[test]
    fn a_modifier_does_not_count_as_a_reason() {
        let mut e = ep();
        e.push("route:fallback".to_string());
        assert!(stamp_tags(&mut e, RouteSelection::CallerNamed));
    }

    /// Our slugs must stay disjoint from the console's, or the two categories
    /// merge in the only aggregate that matters.
    #[test]
    fn slugs_do_not_collide_with_the_console_vocabulary() {
        const CONSOLE: [&str; 6] = [
            "declared_specialist",
            "fermi",
            "cross_cutting",
            "domain_specialist",
            "keyword",
            "default",
        ];
        for s in [RouteSelection::CallerNamed] {
            assert!(
                !CONSOLE.contains(&s.slug()),
                "`{}` shadows a console route reason",
                s.slug()
            );
        }
    }
}
