//! What crossed the boundary, as a digest.
//!
//! # Why these are computed on read and not stored
//!
//! The UX request asked for `input.hash` and `output.hash` on the artifact trace,
//! and the obvious implementation is three columns on `episodes` plus a migration.
//! That is not what this does, and the reason is not cost.
//!
//! `episodes.query` and `episodes.response_text` are both **retained** — the
//! latter since migration 199, deliberately, because *"retention is a
//! precondition for every later form of verification and a digest is not a
//! record."* So a hash of them is a **pure function of data the platform already
//! holds**, and computing it on read has a property a stored column cannot have:
//! **it cannot drift from the text it claims to describe.** A stored digest whose
//! subject was edited, re-encoded, or backfilled is a confident lie about
//! provenance, and this codebase has already been bitten by a stored value that
//! disagreed with its source (`agents.total_executions`, which is what
//! `rollup_trust` exists for).
//!
//! What the computed form cannot do is answer a **cross-episode** query — *find
//! the episode whose output hash equals this one's input hash* — because that
//! needs an index. When a seam check across episodes is actually wanted, the
//! columns are the right answer and this module is what fills them. Until then a
//! migration would be storage in advance of a use, and the use is further away
//! than it looks (see below).
//!
//! # What these hashes can and cannot detect
//!
//! Stated plainly, because the request's framing —
//! *"`input.hash` vs previous `output.hash` … a mismatch is a substituted
//! artifact, mechanically detectable"* — is **not true of this platform yet**, and
//! shipping the field while implying it were would be the over-read the whole
//! surface exists to refuse.
//!
//! **Can:**
//!
//! * tell whether two episodes were given the identical input, and whether they
//!   produced the identical output. That is the drift and determinism question,
//!   and it is answerable today;
//! * tell whether grounding changed the document, by comparing
//!   [`Hashes::output`] against [`Hashes::output_grounded`]. The difference
//!   between the raw and the enforced document **is** what grounding did, which is
//!   why both are hashed rather than one;
//! * give a reviewer a stable handle for *this exact text*, so a verdict recorded
//!   against it can be re-checked rather than merely trusted.
//!
//! **Cannot:**
//!
//! * verify a seam by equality with a parent's output. A delegated child does not
//!   receive its parent's output verbatim — it receives a **prompt built around**
//!   the task, so the hashes will differ for entirely correct runs. The place that
//!   equality *would* hold is the envelope payload in
//!   `agent_backend::envelope::build`, which is passed through unchanged, and
//!   nothing hashes it yet. That is the honest next step for a seam check, and it
//!   is a different piece of work from this one.
//!
//! # Canonicalisation is asserted, not assumed
//!
//! Hashing a JSON document is meaningless unless serialisation is canonical: with
//! insertion-ordered maps, `{"a":1,"b":2}` and `{"b":2,"a":1}` are the same
//! document and different bytes.
//!
//! This used to lean on `serde_json`'s default `Map` being a `BTreeMap`, with the
//! note that it was "a feature flag away from being false" and that
//! `the_document_hash_ignores_key_order` existed to catch the day the flag was
//! turned on. It was turned on — deliberately, because a document's key order is
//! part of the retained bytes and the trace was alphabetising every answer it
//! displayed — and the test fired on the same commit that did it.
//!
//! So [`of_document`] sorts keys itself now. The two requirements are not in
//! conflict once they are named separately: **display preserves order, identity
//! ignores it.** The test stays, and it now guards an implementation rather than
//! an absence.

use sha2::{Digest, Sha256};

/// The algorithm, named in the payload so a consumer never has to guess.
pub const ALGORITHM: &str = "sha256";

/// Digest of a text artifact, prefixed with its algorithm.
///
/// Prefixed for `AgentCard.declared_prompt_sha256`'s reason and one more: a bare
/// hex string is indistinguishable from a different algorithm's bare hex string,
/// and the day this moves to something else every stored value becomes ambiguous.
pub fn of_text(s: &str) -> String {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    format!("{ALGORITHM}:{:x}", h.finalize())
}

/// Digest of a JSON document, over its canonical serialisation.
///
/// Canonical means **keys sorted, at every depth**, and that is now done here
/// rather than inherited from `serde_json`'s default `Map`.
///
/// The module docs predicted this exactly: the property was "a feature flag away
/// from being false", and `the_document_hash_ignores_key_order` existed to catch
/// the day someone turned the flag on. Someone did — deliberately, because a
/// document's key order is part of the retained bytes and the trace was
/// alphabetising every answer it displayed — and the test fired on the same
/// commit.
///
/// Both properties are wanted and they are not in conflict once they are
/// separated: **display preserves order, identity ignores it.** A hash is a claim
/// about which document this is, and `{"a":1,"b":2}` and `{"b":2,"a":1}` are the
/// same document — so an order-sensitive digest would report drift between two
/// renderings of one answer. Implemented rather than assumed, so no dependency's
/// feature flags can decide it again.
pub fn of_document(v: &serde_json::Value) -> String {
    of_text(&canonical(v).to_string())
}

/// The same document with every object's keys sorted, recursively.
///
/// Arrays are left alone: their order is content, not presentation. Sorting them
/// would make `[1,2]` and `[2,1]` the same artifact, which for a ranked list or
/// a sequence of steps is false.
fn canonical(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::Value;
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), canonical(&m[k])))
                    .collect(),
            )
        }
        Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
        other => other.clone(),
    }
}

/// The digests of one episode's artifacts.
///
/// Every field is an `Option` and none of them defaults. A missing hash means
/// *the platform did not retain the thing to hash* — for episodes before
/// migration 199 that is most of them — and a `null` is the only honest rendering
/// of that. Hashing the empty string instead would produce a real-looking digest
/// for an artifact nobody kept, which is worse than an absence.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct Hashes {
    pub algorithm: &'static str,
    /// Over the query as it was given to the agent.
    pub input: Option<String>,
    /// Over the response **verbatim**, before grounding touched it.
    pub output: Option<String>,
    /// Over the enforced document.
    ///
    /// `None` when the response carried no document, or when the agent has no
    /// field contract so nothing was enforced. Distinct from equalling `output`:
    /// equal means grounding ran and changed nothing, absent means it could not
    /// run at all, and those are different facts.
    pub output_grounded: Option<String>,
    /// Did enforcement change the document's bytes at all?
    ///
    /// # This is not a proxy for "a claim was stripped", and it was named as one
    ///
    /// The first version of this field was `grounding_changed_the_document`, and a
    /// live cross-check against the contract's own violation count immediately
    /// disagreed with it on 21 episodes: `weather_oracle` and `enemy_sensor`
    /// responses where enforcement changed the document and the contract had
    /// recorded **zero** violations.
    ///
    /// The reason is that `enforce` does two different things. It nulls a field the
    /// contract refuses — which is a finding — and it **stamps `<block>_provenance`
    /// siblings onto the document**, which is bookkeeping and happens on every
    /// contracted response whether or not anything was wrong. `Report.provenance`
    /// says so in its own doc: *"`(block, provenance)` pairs written onto the
    /// document."*
    ///
    /// So a digest comparison cannot distinguish the two, and a field named as
    /// though it could would have a reader concluding that fabrication was
    /// stripped from 21 responses where nothing was. **For "was a claim removed",
    /// read the violation count** — the contract knows, and it is the one
    /// implementation of that question.
    ///
    /// `None` when the comparison cannot be made. Derived rather than stored so it
    /// cannot disagree with the two hashes above.
    pub enforcement_changed_the_bytes: Option<bool>,
}

/// Assemble the digests for one episode.
///
/// `enforced` must be the document **after** `grounding_trust::enforce`, and
/// `response` the text before it. Passing the same thing twice would report that
/// grounding changed nothing, which is exactly the false reassurance this struct
/// is shaped to avoid — so the caller's two arguments come from two different
/// values by construction, and the trace handler keeps them apart for this reason.
pub fn of_episode(
    query: Option<&str>,
    response: Option<&str>,
    enforced: Option<&serde_json::Value>,
) -> Hashes {
    let output = response.map(of_text);
    let output_grounded = enforced.map(of_document);
    // Compared on the *document* both times, or the answer would be a fact about
    // whitespace: `output` is over the raw text, which includes any prose the
    // model wrapped the document in, so it can never equal a hash of the parsed
    // document even when grounding changed nothing.
    let changed = match (response, enforced) {
        (Some(text), Some(enf)) => crate::agent_backend::envelope::extract_json(text)
            .map(|before| of_document(&before) != of_document(enf)),
        _ => None,
    };
    Hashes {
        algorithm: ALGORITHM,
        input: query.map(of_text),
        output,
        output_grounded,
        enforcement_changed_the_bytes: changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Key order must not change the digest.
    ///
    /// # This is a dependency guard, not a check on the code below it
    ///
    /// Worth stating precisely, because it was learned by trying to break it. The
    /// property is owned by **`serde_json::Map` being a `BTreeMap`**, not by
    /// [`of_document`]: with a sorted map there is no edit to this module that
    /// makes the digest insertion-order-sensitive, because both fixtures parse to
    /// the same map before anything here sees them. An attempted sabotage that
    /// iterated the map in reverse came back green for exactly that reason.
    ///
    /// So this test cannot fail because of a mistake here. It can fail because a
    /// dependency enables `preserve_order` — which any crate in the tree can do,
    /// without this one knowing — and then every document digest silently becomes
    /// a function of insertion order, two identical documents get different
    /// digests, and any determinism check built on them reports drift that is not
    /// there.
    ///
    /// That is a smaller claim than "this logic is verified" and it is the true
    /// one. The half of [`of_document`] that *is* falsifiable — that it depends on
    /// content at all — is `a_changed_value_changes_the_digest`.
    #[test]
    fn the_document_hash_ignores_key_order() {
        let a: serde_json::Value = serde_json::from_str(r#"{"a":1,"b":{"x":1,"y":2}}"#).unwrap();
        let b: serde_json::Value = serde_json::from_str(r#"{"b":{"y":2,"x":1},"a":1}"#).unwrap();
        assert_eq!(a, b, "these are the same document");
        assert_eq!(
            of_document(&a),
            of_document(&b),
            "the same document hashed differently depending on key order. \
             `preserve_order` has been enabled somewhere in the tree, and every \
             document digest is now a function of insertion order."
        );
    }

    /// A different document must hash differently.
    ///
    /// The other half: a hash that ignored key order by ignoring content would
    /// satisfy the test above completely.
    #[test]
    fn a_changed_value_changes_the_digest() {
        let a = json!({"genome": {"estimated_size_mb": 2400}});
        let b = json!({"genome": {"estimated_size_mb": 2401}});
        assert_ne!(of_document(&a), of_document(&b));
        assert!(of_document(&a).starts_with("sha256:"));
    }

    /// An absent artifact has no hash, and not the hash of nothing.
    ///
    /// 3,576 episodes exist and `response_text` was discarded before migration
    /// 199. Hashing the empty string for those would produce a real-looking digest
    /// for an artifact nobody kept — and `e3b0c442…`, the SHA-256 of the empty
    /// string, is a value a reader would have no way to recognise as an absence.
    #[test]
    fn nothing_retained_means_no_digest_rather_than_a_digest_of_nothing() {
        let h = of_episode(None, None, None);
        assert_eq!(h.input, None);
        assert_eq!(h.output, None);
        assert_eq!(h.output_grounded, None);
        assert_eq!(h.enforcement_changed_the_bytes, None);
        // And the empty string is not treated as absent — it is a retained empty
        // artifact, which is a different fact again.
        assert!(of_episode(Some(""), None, None).input.is_some());
    }

    /// Whether enforcement changed the bytes is compared document-to-document.
    ///
    /// The subtle one. `output` is hashed over the **raw text**, which includes
    /// any prose the model wrapped its document in — 64 of 94 retained responses
    /// from contracted agents are wrapped that way. So comparing `output` against
    /// `output_grounded` directly would report that grounding changed the document
    /// on every wrapped response, whether or not it touched a field. The
    /// comparison therefore re-extracts and compares document to document.
    #[test]
    fn a_wrapped_response_is_not_reported_as_modified_by_grounding() {
        let doc = json!({"taxonomy": {"order": "Orthoptera"}});
        let wrapped = format!("Here you go:\n\n```json\n{doc}\n```\n");

        let unchanged = of_episode(Some("q"), Some(&wrapped), Some(&doc));
        assert_eq!(
            unchanged.enforcement_changed_the_bytes,
            Some(false),
            "grounding touched nothing, and the prose wrapper must not be \
             reported as a modification"
        );

        let nulled = json!({"taxonomy": {"order": null}});
        let changed = of_episode(Some("q"), Some(&wrapped), Some(&nulled));
        assert_eq!(
            changed.enforcement_changed_the_bytes,
            Some(true),
            "grounding nulled a field and the comparison did not notice"
        );
    }

    /// `output_grounded` absent and `output_grounded` equal are different facts.
    #[test]
    fn grounding_that_could_not_run_is_not_grounding_that_changed_nothing() {
        // No document in the response: nothing to enforce.
        let prose_only = of_episode(Some("q"), Some("No forecast today."), None);
        assert_eq!(prose_only.output_grounded, None);
        assert_eq!(
            prose_only.enforcement_changed_the_bytes, None,
            "with nothing enforced the question is unanswerable, and `false` \
             would read as `grounding ran and approved this`"
        );
    }
}
