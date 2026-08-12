//! Canonical `dyad_id` construction and parsing.
//!
//! A dyad is the durable (agent, human) pair that the companion loop
//! learns over. Its identifier is deliberately a **three-segment string**
//! so that the human can always be recovered from the id alone, without a
//! join:
//!
//! ```text
//!   <origin>:<agent_uuid>:<human_id>
//! ```
//!
//! Two origins exist:
//!
//! - [`ORIGIN_DYAD`] — a real human↔agent conversation. These are the ones
//!   that should drive companion adaptation.
//! - [`ORIGIN_EVAL`] — synthetic history produced by the eval pipeline
//!   (`run_eval_cases`). Kept distinguishable so a relationship built out
//!   of regression fixtures is never mistaken for a real one.
//!
//! Before this module existed each call site formatted its own string and
//! `auto_form_dyads_handler` re-parsed it with an inline `splitn(3, ':')`.
//! Keeping construction and parsing adjacent here is what guarantees the
//! two stay in sync.

use uuid::Uuid;

/// Origin segment for a real human↔agent conversation.
pub const ORIGIN_DYAD: &str = "dyad";

/// Origin segment for eval-pipeline synthetic history.
pub const ORIGIN_EVAL: &str = "eval";

/// Build the dyad id for a real conversation between `agent_id` and `human_id`.
pub fn dyad_id(agent_id: Uuid, human_id: &str) -> String {
    format!("{}:{}:{}", ORIGIN_DYAD, agent_id, human_id)
}

/// Build the dyad id for an eval-pipeline execution.
pub fn eval_dyad_id(agent_id: Uuid, human_id: &str) -> String {
    format!("{}:{}:{}", ORIGIN_EVAL, agent_id, human_id)
}

/// Recover the human id from a dyad id.
///
/// Returns `None` for malformed ids rather than guessing, so callers can
/// skip rows instead of inventing a `human_id` from the whole string (which
/// is what the original inline parse did on the two-segment format).
pub fn human_id_from_dyad(dyad_id: &str) -> Option<&str> {
    let mut parts = dyad_id.splitn(3, ':');
    let origin = parts.next()?;
    let agent = parts.next()?;
    let human = parts.next()?;
    if origin.is_empty() || agent.is_empty() || human.is_empty() {
        return None;
    }
    Some(human)
}

/// Recover the agent uuid from a dyad id, when it parses.
pub fn agent_id_from_dyad(dyad_id: &str) -> Option<Uuid> {
    let mut parts = dyad_id.splitn(3, ':');
    let _origin = parts.next()?;
    let agent = parts.next()?;
    Uuid::parse_str(agent).ok()
}

/// True when this dyad came from the eval pipeline rather than a human.
pub fn is_eval_dyad(dyad_id: &str) -> bool {
    dyad_id.starts_with(ORIGIN_EVAL)
        && dyad_id
            .as_bytes()
            .get(ORIGIN_EVAL.len())
            .is_some_and(|b| *b == b':')
}

/// True when this dyad represents a real human conversation.
pub fn is_real_dyad(dyad_id: &str) -> bool {
    dyad_id.starts_with(ORIGIN_DYAD)
        && dyad_id
            .as_bytes()
            .get(ORIGIN_DYAD.len())
            .is_some_and(|b| *b == b':')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent() -> Uuid {
        Uuid::parse_str("a20c239d-c35b-4e18-b45c-a2e2ae1c4372").unwrap()
    }

    #[test]
    fn round_trips_real_dyad() {
        let id = dyad_id(agent(), "user-42");
        assert_eq!(id, "dyad:a20c239d-c35b-4e18-b45c-a2e2ae1c4372:user-42");
        assert_eq!(human_id_from_dyad(&id), Some("user-42"));
        assert_eq!(agent_id_from_dyad(&id), Some(agent()));
        assert!(is_real_dyad(&id));
        assert!(!is_eval_dyad(&id));
    }

    #[test]
    fn round_trips_eval_dyad() {
        let id = eval_dyad_id(agent(), "user-42");
        assert_eq!(id, "eval:a20c239d-c35b-4e18-b45c-a2e2ae1c4372:user-42");
        assert_eq!(human_id_from_dyad(&id), Some("user-42"));
        assert!(is_eval_dyad(&id));
        assert!(!is_real_dyad(&id));
    }

    #[test]
    fn human_id_survives_colons_in_the_human_segment() {
        let id = dyad_id(agent(), "oauth:google:1234");
        assert_eq!(human_id_from_dyad(&id), Some("oauth:google:1234"));
    }

    #[test]
    fn rejects_malformed_ids() {
        assert_eq!(human_id_from_dyad("not-a-dyad"), None);
        assert_eq!(human_id_from_dyad("dyad:only-two"), None);
        assert_eq!(human_id_from_dyad("dyad::"), None);
        assert_eq!(human_id_from_dyad(""), None);
    }

    #[test]
    fn prefix_checks_do_not_match_lookalikes() {
        assert!(!is_eval_dyad("evaluation:x:y"));
        assert!(!is_real_dyad("dyadic:x:y"));
    }
}
