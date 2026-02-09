//! Agent state machine — valid lifecycle transitions.

use super::types::AgentLifecycleStatus;

/// Valid state transitions:
/// - Draft -> Published (via publish pipeline)
/// - Draft -> Archived (discard)
/// - Published -> Archived
/// - Archived -> Draft (restore)
pub fn validate_transition(
    from: &AgentLifecycleStatus,
    to: &AgentLifecycleStatus,
) -> Result<(), String> {
    use AgentLifecycleStatus::*;
    match (from, to) {
        (Draft, Published) => Ok(()),
        (Draft, Archived) => Ok(()),
        (Published, Archived) => Ok(()),
        (Archived, Draft) => Ok(()),
        _ => Err(format!(
            "Invalid transition: {} -> {}",
            from.as_str(),
            to.as_str()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use AgentLifecycleStatus::*;

    #[test]
    fn test_valid_transitions() {
        assert!(validate_transition(&Draft, &Published).is_ok());
        assert!(validate_transition(&Draft, &Archived).is_ok());
        assert!(validate_transition(&Published, &Archived).is_ok());
        assert!(validate_transition(&Archived, &Draft).is_ok());
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(validate_transition(&Published, &Draft).is_err());
        assert!(validate_transition(&Archived, &Published).is_err());
        assert!(validate_transition(&Published, &Published).is_err());
        assert!(validate_transition(&Draft, &Draft).is_err());
    }
}
