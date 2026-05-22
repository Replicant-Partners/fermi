//! Shared slug-validation rules for every publishable artifact on the
//! platform.
//!
//! ## Why one place
//!
//! Several artifact types are referenced in URLs by a human-readable
//! string (agent_id, app slug, workspace slug, composition_slug, …).
//! When any of those strings contain a `/`, the artifact becomes
//! unreachable through its URL path: axum's tree router splits on `/`
//! and the agent `efra-ai/05-valuation` becomes a 404 at
//! `/api/agents/efra-ai/05-valuation/...` because the router routes
//! `/api/agents/:agent_id/...` with `agent_id = "efra-ai"` and then
//! fails to match `05-valuation/...` against the remaining tree.
//!
//! The same goes for spaces, `..` traversal, `?`/`#` URL meta-chars,
//! and assorted other punctuation. Rather than letting every entry
//! point handle this defensively, we centralise the rule here and
//! every creation path that mints a publishable slug calls
//! [`validate`].
//!
//! ## The rule
//!
//! A slug must:
//!   - be 3..=64 ASCII characters,
//!   - start with a lowercase letter `a..=z`,
//!   - contain only `a..=z`, `0..=9`, and `_`,
//!   - not be a reserved platform tag (see `apps::builder::is_reserved`).
//!
//! This matches the validator that already protects App slugs
//! (`crates/abw-apps-core::validate_slug`) and that every curated
//! agent already conforms to (`agents/curated/` is all lowercase
//! snake_case). Extending the same rule to every other creation
//! surface gives the platform a single mental model.
//!
//! ## Why not hyphens / dots
//!
//! Hyphens and dots are URL-safe and would be a reasonable addition,
//! but every artifact shipped today uses `_` exclusively. Allowing
//! more characters now would create three migration headaches later
//! (case folding, normalisation, equivalence classes). The
//! conservative ASCII-snake-case rule means slugs are also valid
//! lowercase identifiers in virtually every downstream context —
//! shell variables, JSON keys, file paths, sub-paths in S3, etc.

use crate::apps::builder;

/// Validate a slug for use as a publishable artifact identifier.
///
/// Returns `Ok(())` if the slug is acceptable on every URL-routed
/// surface (agents, apps, workspaces, compositions). Otherwise
/// returns a human-readable explanation suitable for surfacing
/// verbatim to the caller.
///
/// See module-level docs for the rule. This function is a thin
/// alias for [`crate::apps::builder::validate_slug`] so every artifact
/// type shares one definition.
pub fn validate(slug: &str) -> Result<(), String> {
    builder::validate_slug(slug)
}

/// Convenience wrapper that returns an `(HTTP status, error body)` pair
/// suitable for `?`-propagation out of axum handlers.
///
/// Usage:
/// ```ignore
/// use crate::slug;
/// slug::validate_http("name", &req.agent_name)?;
/// ```
pub fn validate_http(
    field: &str,
    slug: &str,
) -> Result<(), (axum::http::StatusCode, String)> {
    validate(slug).map_err(|msg| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            format!("Invalid {}: {}", field, msg),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_typical_snake_case() {
        assert!(validate("efra_ai").is_ok());
        assert!(validate("supply_chain_oracle").is_ok());
        assert!(validate("kask_simops").is_ok());
        assert!(validate("simops_narrator_local").is_ok());
        assert!(validate("a12_b34").is_ok());
    }

    /// Regression for the Mario reproduction in workspace 5fee2101 —
    /// the literal name that caused the @-mention parser bug.
    #[test]
    fn rejects_slash_separator() {
        let err = validate("efra-ai/05-valuation").unwrap_err();
        assert!(
            err.contains("only lowercase letters, digits, and underscores")
                || err.contains("/"),
            "error message must explain the rule; got {err}"
        );
    }

    #[test]
    fn rejects_hyphen() {
        // Hyphens conflict with platform-wide snake_case. If you want
        // a name like "efra-ai", spell it `efra_ai`.
        assert!(validate("efra-ai").is_err());
    }

    #[test]
    fn rejects_dot_segments() {
        // `..` is path traversal; `.` is meaningful in URL routing too.
        assert!(validate("foo.bar").is_err());
        assert!(validate("..").is_err());
    }

    #[test]
    fn rejects_whitespace_and_specials() {
        assert!(validate("foo bar").is_err());
        assert!(validate("foo?bar").is_err());
        assert!(validate("foo#bar").is_err());
        assert!(validate("foo%20bar").is_err());
    }

    #[test]
    fn rejects_leading_digit_or_underscore() {
        assert!(validate("1foo").is_err());
        assert!(validate("_foo").is_err());
    }

    #[test]
    fn rejects_too_short_or_too_long() {
        assert!(validate("ab").is_err());
        let too_long: String = std::iter::repeat('a').take(65).collect();
        assert!(validate(&too_long).is_err());
    }

    #[test]
    fn rejects_uppercase() {
        assert!(validate("EfraAI").is_err());
        assert!(validate("efra_AI").is_err());
    }
}
