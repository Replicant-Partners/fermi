//! Does what the console sends survive the server's read?
//!
//! # The seam
//!
//! A claim is a number bound to one driver of one forecast, and both halves
//! travel from here to the server inside the `invocation` object on the execute
//! request. The console writes them from `InvocationProvenance`'s serde field
//! names; the server reads them with
//! [`fermi::claim_outcome::binding_from_invocation`].
//!
//! Before this test there were **four independent spellings** of those two keys
//! — two serde attributes in `negotiate.rs`, and string literals typed
//! separately in `handlers/execution.rs` and `handlers/execution_stream.rs` —
//! and nothing compared them.
//!
//! # Why it matters more than a usual rename risk
//!
//! `forecast_agent_claims` has held zero rows since migration 187 created it.
//! So a broken envelope produces **exactly the observation the platform already
//! has**: an empty table. There is no state to compare against, no alarm to
//! fall silent, and nothing anywhere that would distinguish "the key was
//! renamed" from "nobody has run a forecast-bound agent yet".
//!
//! That is the shape of every defect in `docs/AUDIT_loops_and_gates.md`, and the
//! reason this is a build failure rather than a code review note.
//!
//! # What it does not show
//!
//! That the console *populates* the halves — that depends on a forecast being
//! saved, and an unsaved draft correctly yields no claim. Only that when it
//! does populate them, the server recovers them.

use fermi::claim_outcome::binding_from_invocation;
use fermi_console::negotiate::{
    AgentContract, ComposedQuery, InputBinding, InvocationProvenance, QuerySource,
};

/// A fully-bound run, as the composer builds it.
fn bound_provenance() -> InvocationProvenance {
    let composed = ComposedQuery {
        text: "What is the outlook for GDP growth?".into(),
        source: QuerySource::UserAuthored,
        recomposed_from: None,
    };
    InvocationProvenance::new(
        &composed,
        &InputBinding::Undeclared,
        None::<&AgentContract>,
        Some("gdp_growth"),
    )
    .for_forecast(Some("fc-abc123"))
}

/// The console's wire form, read by the server's parser.
#[test]
fn both_halves_of_the_claim_binding_survive_the_wire() {
    let json = bound_provenance().to_json();

    // Sanity: the console really did put something on the wire. Without this
    // the assertions below would pass over an empty object if `to_json` ever
    // started returning `Null`.
    assert!(
        json.is_object(),
        "the provenance did not serialise to an object: {json}"
    );

    let recovered = binding_from_invocation(Some(&json));
    assert_eq!(
        recovered.forecast_id.as_deref(),
        Some("fc-abc123"),
        "the server cannot recover `forecast_id` from what the console sends. \
         The key was renamed on one side of the seam.\n  wire: {json}"
    );
    assert_eq!(
        recovered.driver.as_deref(),
        Some("gdp_growth"),
        "the server cannot recover `driver` from what the console sends. A \
         forecast id with no driver is unattributable: `classify_claim` returns \
         `Unbound` and writes nothing.\n  wire: {json}"
    );
}

/// An unbound run must recover as unbound, not as a partial binding.
///
/// `InvocationProvenance` marks both halves `skip_serializing_if =
/// "Option::is_none"`, so `None` is an **absent key** rather than a null. The
/// server's reader has to treat those the same way, and the two facts are
/// stated in different crates.
#[test]
fn an_unbound_run_recovers_as_unbound() {
    let composed = ComposedQuery {
        text: "q".into(),
        source: QuerySource::UserAuthored,
        recomposed_from: None,
    };
    // A query composed against a draft that has never been saved: no forecast
    // id to give, and no claim to write. Correct, and it must not read as half
    // a binding.
    let json =
        InvocationProvenance::new(&composed, &InputBinding::Undeclared, None, None).to_json();

    let recovered = binding_from_invocation(Some(&json));
    assert!(
        recovered.forecast_id.is_none() && recovered.driver.is_none(),
        "an unbound provenance recovered as bound, so the gate at \
         `execution.rs` would spawn the claim hook for a run with nothing to \
         attach to.\n  wire: {json}"
    );

    // And the keys really are absent rather than null — the property the
    // server's reader is written against.
    let obj = json.as_object().expect("object");
    assert!(
        !obj.contains_key(fermi::claim_outcome::KEY_FORECAST_ID),
        "`skip_serializing_if` no longer applies to `forecast_id`; the server \
         reads an absent key and a null the same way today, but this is the \
         assumption that makes that safe"
    );
}
