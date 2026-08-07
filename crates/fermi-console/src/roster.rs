//! Orchestra roster verification.
//!
//! # Why this exists
//!
//! The console asks the server for `/api/agents?orchestra=fermi`. That is
//! a *query parameter*, and an unrecognised query parameter is silently
//! ignored — so a server predating the filter answers with the entire
//! catalogue, and the console renders it as
//! **"104 fermi orchestra agents"**: every vertical's agents (AR beacons,
//! adaptogen curation, observability triage) presented as members of the
//! Fermi forecasting orchestra.
//!
//! That failure mode is worse than an error, because the result *looks*
//! authoritative. It is the same class of bug as the one that made
//! membership itself untrustworthy: a predicate that quietly fails open.
//!
//! So membership is confirmed against `/api/orchestras/{name}/members`,
//! an endpoint whose **path** encodes the constraint. It cannot return a
//! non-member, and if the server doesn't have it we get a 404 we can see
//! rather than a wrong answer we can't.
//!
//! Lives in the lib target because the binary's `#[cfg(test)]` modules
//! are unrunnable (see the crate docs on rustc's stack overflow when
//! expanding the GPUI element tree under `--test`).

use serde_json::Value as JsonValue;
use std::collections::HashSet;

/// Extract the set of member identifiers from a
/// `GET /api/orchestras/{name}/members` response.
///
/// Accepts either `{ "members": [...] }` or a bare array, and indexes by
/// both `agent_name` and `agent_id` because callers hold whichever the
/// agent-list endpoint happened to return.
pub fn member_ids(roster: &JsonValue) -> HashSet<String> {
    roster
        .get("members")
        .and_then(|m| m.as_array())
        .cloned()
        .or_else(|| roster.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .flat_map(|m| {
            ["agent_name", "agent_id"]
                .iter()
                .filter_map(|k| m.get(*k).and_then(|v| v.as_str()).map(str::to_string))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Outcome of verifying an agent list against a roster.
#[derive(Debug, Clone, PartialEq)]
pub struct RosterFilter {
    /// Cards confirmed to be orchestra members.
    pub cards: Vec<JsonValue>,
    /// How many the server returned before verification.
    pub received: usize,
    /// True when the server returned non-members, i.e. it ignored the
    /// `?orchestra=` filter. Callers should log this: it means the server
    /// predates the filter and the client is compensating.
    pub server_ignored_filter: bool,
}

/// Keep only cards the roster confirms as members.
pub fn retain_members(cards: Vec<JsonValue>, roster: &JsonValue) -> RosterFilter {
    let members = member_ids(roster);
    let received = cards.len();
    let cards: Vec<JsonValue> = cards
        .into_iter()
        .filter(|c| {
            let id = c.get("agent_id").and_then(|v| v.as_str()).unwrap_or("");
            let name = c.get("agent_name").and_then(|v| v.as_str()).unwrap_or("");
            (!id.is_empty() && members.contains(id)) || (!name.is_empty() && members.contains(name))
        })
        .collect();
    RosterFilter {
        server_ignored_filter: cards.len() != received,
        received,
        cards,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn card(name: &str) -> JsonValue {
        json!({ "agent_id": format!("id-{name}"), "agent_name": name })
    }

    fn roster(names: &[&str]) -> JsonValue {
        json!({
            "orchestra": "fermi",
            "members": names.iter().map(|n| json!({
                "agent_id": format!("id-{n}"), "agent_name": n
            })).collect::<Vec<_>>()
        })
    }

    /// The reported bug, reproduced: server ignores `?orchestra=`, returns
    /// agents from every vertical, and the console must not present them
    /// as Fermi members.
    #[test]
    fn filters_out_agents_the_server_returned_despite_the_filter() {
        let cards = vec![
            card("sentiment_analyzer"),
            card("market_research"),
            card("ar_beacon"),
            card("adaptogen_curator"),
            card("anomaly_triager"),
        ];
        let r = retain_members(cards, &roster(&["sentiment_analyzer", "market_research"]));

        assert_eq!(r.received, 5);
        assert_eq!(r.cards.len(), 2);
        assert!(
            r.server_ignored_filter,
            "must flag that the server failed to apply the filter"
        );
        let names: Vec<&str> = r
            .cards
            .iter()
            .map(|c| c["agent_name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["sentiment_analyzer", "market_research"]);
    }

    /// A server that DID apply the filter must not be flagged, and nothing
    /// should be dropped.
    #[test]
    fn passes_through_untouched_when_the_server_filtered_correctly() {
        let cards = vec![card("sentiment_analyzer"), card("market_research")];
        let r = retain_members(cards, &roster(&["sentiment_analyzer", "market_research"]));
        assert_eq!(r.cards.len(), 2);
        assert!(!r.server_ignored_filter);
    }

    /// An empty roster means no members — never "show everything".
    #[test]
    fn empty_roster_yields_no_members() {
        let cards = vec![card("ar_beacon"), card("adaptogen_curator")];
        let r = retain_members(cards, &roster(&[]));
        assert!(r.cards.is_empty());
        assert!(r.server_ignored_filter);
    }

    /// Matching by agent_id alone must work — the agents endpoint and the
    /// roster don't always agree on which identifier they expose.
    #[test]
    fn matches_on_agent_id_when_names_are_absent() {
        let cards = vec![json!({ "agent_id": "id-efra_forensic" })];
        let r = retain_members(cards, &roster(&["efra_forensic"]));
        assert_eq!(r.cards.len(), 1);
    }

    /// A bare-array roster response is accepted too.
    #[test]
    fn accepts_a_bare_array_roster() {
        let bare = json!([{ "agent_name": "efra_forensic" }]);
        let r = retain_members(vec![card("efra_forensic"), card("ar_beacon")], &bare);
        assert_eq!(r.cards.len(), 1);
    }

    /// Cards with neither identifier must not slip through on an
    /// empty-string match against an empty-string roster entry.
    #[test]
    fn cards_without_identifiers_are_dropped() {
        let roster = json!({ "members": [{ "note": "no ids here" }] });
        let r = retain_members(vec![json!({ "description": "anonymous" })], &roster);
        assert!(r.cards.is_empty());
    }
}
