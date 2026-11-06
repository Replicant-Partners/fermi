//! Who may put an agent into a workspace (SPEC 36 R0.1.5).
//!
//! One rule, used by `hire_agent_handler` (src/handlers/workspace/messages.rs)
//! and by App spawn auto-hire (src/apps/provision.rs). Before this, spawn
//! inserted any agent by name with `status IN ('active','published','draft')`
//! and no ownership check [V: src/handlers/apps.rs:654-666 @ a3cea516], while
//! the hire handler checked visibility but not draft/archived status
//! [V: src/handlers/workspace/messages.rs:1415-1428 @ a3cea516].

/// Why an agent may not be placed in a workspace by this caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HireRefusal {
    Archived,
    /// A draft is usable only by its owner.
    DraftNotOwned,
    /// Not public and not a curated/system (platform) agent.
    NotVisible,
}

impl HireRefusal {
    pub fn reason(self) -> &'static str {
        match self {
            HireRefusal::Archived => "archived",
            HireRefusal::DraftNotOwned => "draft_not_owned",
            HireRefusal::NotVisible => "not_visible_to_caller",
        }
    }
}

/// The facts the rule needs about an agent.
pub struct AgentFacts<'a> {
    pub owner_id: Option<&'a str>,
    pub visibility: Option<&'a str>,
    pub tier: &'a str,
    pub status: &'a str,
}

/// `Ok(())` iff `caller_id` may place this agent in a workspace it controls.
pub fn may_place(agent: &AgentFacts<'_>, caller_id: &str) -> Result<(), HireRefusal> {
    let owned = agent.owner_id == Some(caller_id);
    if agent.status == "archived" {
        return Err(HireRefusal::Archived);
    }
    if agent.status == "draft" && !owned {
        return Err(HireRefusal::DraftNotOwned);
    }
    let platform = agent.tier == "curated" || agent.tier == "system";
    let public = agent.visibility == Some("public");
    if owned || public || platform {
        Ok(())
    } else {
        Err(HireRefusal::NotVisible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a<'x>(
        owner: Option<&'x str>,
        vis: &'x str,
        tier: &'x str,
        status: &'x str,
    ) -> AgentFacts<'x> {
        AgentFacts {
            owner_id: owner,
            visibility: Some(vis),
            tier,
            status,
        }
    }

    #[test]
    fn rule_table() {
        let me = "me";
        assert_eq!(
            may_place(&a(Some("me"), "private", "community", "draft"), me),
            Ok(())
        );
        assert_eq!(
            may_place(&a(Some("you"), "private", "community", "draft"), me),
            Err(HireRefusal::DraftNotOwned)
        );
        assert_eq!(
            may_place(&a(Some("you"), "public", "community", "draft"), me),
            Err(HireRefusal::DraftNotOwned)
        );
        assert_eq!(
            may_place(&a(Some("you"), "public", "community", "published"), me),
            Ok(())
        );
        assert_eq!(
            may_place(&a(None, "private", "curated", "published"), me),
            Ok(())
        );
        assert_eq!(
            may_place(&a(None, "private", "system", "draft"), me),
            Err(HireRefusal::DraftNotOwned)
        );
        assert_eq!(
            may_place(&a(Some("you"), "private", "community", "published"), me),
            Err(HireRefusal::NotVisible)
        );
        assert_eq!(
            may_place(&a(Some("me"), "public", "community", "archived"), me),
            Err(HireRefusal::Archived)
        );
    }
}
