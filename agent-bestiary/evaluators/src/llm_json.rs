//! Tolerant extraction of a JSON object from an LLM completion.
//!
//! Every LLM-backed evaluator asks the model to "return ONLY valid JSON" and
//! then hands the raw completion straight to `serde_json::from_str`. Models
//! comply *most* of the time, but routinely also:
//!
//! - wrap the object in a ```json … ``` fence,
//! - prefix it with `Here is the evaluation:`,
//! - append a trailing sentence after the closing brace,
//! - emit smart quotes or a trailing comma.
//!
//! Any one of those turns a perfectly good score into a hard evaluator
//! failure. Because failures were recorded without their reason, a single
//! evaluator could break silently for months. This module makes the parse
//! forgiving of formatting without being forgiving of *content* — it locates
//! the outermost balanced `{…}` and parses that.

/// Extract the first balanced JSON object from `text`.
///
/// Brace matching is string-aware (and escape-aware), so a `}` inside a
/// quoted rationale does not terminate the object early.
pub fn extract_json_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = text.find('{')?;

    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for i in start..bytes.len() {
        let c = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Parse an LLM completion into `T`, tolerating surrounding prose and fences.
///
/// Falls back to parsing the whole string when no object can be isolated, so
/// the resulting error message still refers to the real payload.
pub fn parse_llm_json<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, serde_json::Error> {
    match extract_json_object(text) {
        Some(obj) => serde_json::from_str(obj),
        None => serde_json::from_str(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Scores {
        goal_completion: f64,
        rapport: f64,
        rationale: Option<String>,
    }

    fn parse(s: &str) -> Scores {
        parse_llm_json(s).expect("should parse")
    }

    #[test]
    fn parses_bare_object() {
        let s = parse(r#"{"goal_completion": 8, "rapport": 7, "rationale": "good"}"#);
        assert_eq!(s.goal_completion, 8.0);
    }

    #[test]
    fn parses_fenced_object() {
        let s =
            parse("```json\n{\"goal_completion\": 8, \"rapport\": 7, \"rationale\": null}\n```");
        assert_eq!(s.rapport, 7.0);
    }

    #[test]
    fn parses_object_with_leading_prose() {
        let s = parse(
            "Here is my evaluation:\n{\"goal_completion\": 6, \"rapport\": 5, \"rationale\": \"ok\"}",
        );
        assert_eq!(s.goal_completion, 6.0);
    }

    #[test]
    fn parses_object_with_trailing_prose() {
        let s = parse(
            "{\"goal_completion\": 6, \"rapport\": 5, \"rationale\": \"ok\"}\nLet me know if you need more.",
        );
        assert_eq!(s.rapport, 5.0);
    }

    /// A `}` inside a rationale must not truncate the object.
    #[test]
    fn brace_inside_string_does_not_terminate() {
        let s =
            parse(r#"{"goal_completion": 9, "rapport": 9, "rationale": "used {braces} inline"}"#);
        assert_eq!(s.rationale.as_deref(), Some("used {braces} inline"));
    }

    #[test]
    fn escaped_quote_inside_string_is_handled() {
        let s = parse(
            r#"{"goal_completion": 4, "rapport": 3, "rationale": "said \"hello\" then left"}"#,
        );
        assert_eq!(s.rationale.as_deref(), Some(r#"said "hello" then left"#));
    }

    #[test]
    fn nested_objects_are_balanced() {
        let obj = extract_json_object(r#"prefix {"a": {"b": {"c": 1}}, "d": 2} suffix"#).unwrap();
        assert_eq!(obj, r#"{"a": {"b": {"c": 1}}, "d": 2}"#);
    }

    #[test]
    fn returns_none_when_no_object_present() {
        assert_eq!(extract_json_object("I cannot evaluate this."), None);
    }

    #[test]
    fn unterminated_object_is_none() {
        assert_eq!(extract_json_object(r#"{"goal_completion": 8"#), None);
    }

    /// Genuinely malformed content must still error rather than silently pass.
    #[test]
    fn missing_required_field_still_errors() {
        let r: Result<Scores, _> = parse_llm_json(r#"{"rapport": 7}"#);
        assert!(r.is_err());
    }
}
