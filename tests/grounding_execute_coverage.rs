//! Every execute boundary must run the grounding contract.
//!
//! # The failure this exists to prevent
//!
//! `grounding_trust` declares field contracts for nine agents. Enforcement was
//! wired into six bespoke creature handlers and into the agent-to-agent
//! delegation hop (`agent_backend/envelope.rs`), which between them covered
//! four of those nine. The other five — `football_analyst`, `weather_oracle`,
//! `hud_field_scout`, `harvest_advisor`, `forage_scout` — had a written
//! contract and no enforcement call site on the generic path. Their output was
//! checked when another agent called them and unchecked when a person did.
//!
//! That is the worst version of this defect rather than a mild one. The
//! contract exists, so a reader looking for one finds it; the coverage metric
//! counts the agent as contracted; and the fabrication travels the human route
//! untouched into `episodes`, where the consolidation worker turns it into a
//! semantic rule and `kg_context` appends it to a prompt as a premise.
//!
//! # What moved, and what is now scanned
//!
//! Enforcement is no longer at the call sites. It happens once, in
//! `episode_boundary::Pulse::grade`, and the rest of the boundary — the stamp,
//! the write, the raise, the queue — happens in `episode_boundary::close`. The
//! reason is the finding that ended the previous shape of this file: the list
//! of execute paths was three files and the real number of episode writers was
//! fifteen, seven of them from a genuine agent invocation. A scan is only as
//! good as the list it scans, and the remedy for a list that is wrong is not a
//! longer list.
//!
//! So the property is stated twice, and the two halves together are stronger
//! than the per-handler scan they replace:
//!
//! 1. The boundary enforces, and consumes what enforcement returned
//!    ([`EXECUTE_BOUNDARIES`]).
//! 2. Every handler that persists an agent's output reaches the boundary, and
//!    hands it the grading rather than computing one and dropping it
//!    ([`BOUNDARY_CALLERS`]).
//!
//! Half 2 is the half that can still go wrong quietly. `pulse.grade(..)`
//! followed by a write that never receives the `Graded` compiles, runs, strips
//! nothing and stamps nothing — the same defect as before, one refactor further
//! from view.
//!
//! # Why a source scan
//!
//! Same reasoning as `provenance_floor_coverage`: the type-level fix would be
//! to make enforcement impossible to skip, but every candidate signature ends
//! in an `Option` that a caller may pass `None` to, which is the same hole with
//! more ceremony. So the enforcement is a scan, with an exemption list that has
//! to name each exception and say why.
//!
//! # What it cannot do
//!
//! It proves the call is present, not that it ran, and not that it was correct.
//! Reading the code proves nothing — that is what `liveness_trust` is for. This
//! only closes the specific regression where a third execute path is added and
//! quietly skips the check, which is how the first two came to skip it.

use std::fs;

/// Where enforcement lives, and what each site must do with the verdict.
/// `(path, must_also_consume, why)`.
///
/// The second field is the point. Calling `enforce` and dropping the `Report`
/// leaves the document checked and the verdict thrown away: nothing records
/// that the check ran, nothing downstream can tell a checked document from an
/// unchecked one, and the scan would still be green.
///
/// This is not hypothetical. The sibling scan, `provenance_floor_coverage`,
/// tested `contains(".with_provenance_oracle(")` — presence of a call, nothing
/// about its argument — so passing `None` satisfied it completely while
/// producing exactly the ungraded rules it existed to prevent. Verified by
/// sabotage, then fixed. Asserting a proxy that is cheaper to satisfy than the
/// property is the defect class this whole line of work is about, and a scan is
/// not exempt from it.
///
/// Two entries where there were three, because `execution.rs` and
/// `execution_stream.rs` no longer enforce anything themselves. That is the
/// consolidation and not a shrinking of the property: what they used to do
/// individually is asserted over `episode_boundary.rs` here and over their own
/// reachability of it in [`BOUNDARY_CALLERS`].
const EXECUTE_BOUNDARIES: &[(&str, &str, &str)] = &[
    (
        "src/episode_boundary.rs",
        // The boundary consumes the report by stamping it onto the episode it
        // is about to write, which is what lets anything downstream tell a
        // checked document from an unchecked one.
        "stamp_grounding",
        "the one place a pulse becomes a row. Every episode-persisting path \
         goes through it, so this is the single file whose failure to enforce \
         would be a platform-wide loss rather than one route's",
    ),
    (
        "src/agent_backend/envelope.rs",
        // The delegation hop consumes the report by building the enforced
        // payload into the envelope rather than by stamping an episode.
        "report",
        "the agent-to-agent delegation hop, which writes no episode and so \
         cannot use the boundary",
    ),
];

/// The call that must be present. Matching on the function name rather than a
/// full expression so reformatting does not break the test — the thing being
/// asserted is that enforcement is reached, not how it is spelled.
///
/// Deliberately stops before the parenthesis, so it covers
/// `enforce_from_output_contract` too. `envelope.rs` migrated to that entry
/// point while this consolidation was landing; a needle ending in `(` would
/// have reported the hop as unenforced.
const ENFORCE_CALL: &str = "grounding_trust::enforce";

/// Handlers that take a live agent invocation and persist it. Each must reach
/// the boundary, because reaching it is now the whole of how enforcement,
/// grading, the gate's ledger row, the stamps, the raise and the queue happen.
/// `(path, why)`.
///
/// Named individually rather than globbed. The number of files that persist an
/// episode is fifteen and most of them are not live request paths; a glob would
/// either fail on the twelve that are correct or need exemptions that hide the
/// rule. These three are the ones a person or a console can reach directly, and
/// they are the three that diverged: `/execute` ran all six checks, the
/// streaming sibling ran four, and the workspace path — where multi-agent work
/// actually happens — ran none. Nine of ten contracted agents had never graded
/// a field and `route:` was stamped on 0 of 3,581 episodes, and both numbers
/// were dominated by the path that ran none of it.
const BOUNDARY_CALLERS: &[(&str, &str)] = &[
    (
        "src/handlers/execution.rs",
        "POST /api/agents/:id/execute — the generic execute endpoint",
    ),
    (
        "src/handlers/execution_stream.rs",
        "the SSE streaming variant; if only one of the pair checks, the \
         unchecked one becomes the one callers use",
    ),
    (
        "src/handlers/workspace/messages.rs",
        "the @-mention path, which is where multi-agent work actually happens \
         and which ran none of the six checks",
    ),
];

/// The two ways into the boundary's write. A handler reaching neither has not
/// reached enforcement at all.
const CLOSE_CALLS: &[&str] = &[
    "episode_boundary::close(",
    "episode_boundary::persist(",
    "episode_boundary::persist_opened(",
];

/// Lines that are not comments. The filter that stops a mention counting as a
/// call — this repository's comments name modules they do not invoke constantly.
fn code_lines(src: &str) -> impl Iterator<Item = &str> {
    src.lines().filter(|l| {
        let t = l.trim_start();
        !t.starts_with("//") && !t.starts_with("///") && !t.starts_with("*")
    })
}

fn calls(src: &str, needle: &str) -> bool {
    code_lines(src).any(|l| l.contains(needle))
}

/// The `n` code lines following each occurrence of `needle`, joined.
///
/// Enough to read one call's arguments and no more. Both properties this file
/// asserts about a call site are about what is *passed* — the grading, and the
/// pool — and a whole-file `contains` cannot tell an argument from a mention of
/// the same token four hundred lines away. `execution.rs` writes `db: Some(..)`
/// on its `ToolContext` as well as on its `Write`, so a file-wide scan for the
/// pool would pass a handler that had changed the one that matters to `None`.
fn after(src: &str, needle: &str, n: usize) -> String {
    let lines: Vec<&str> = code_lines(src).collect();
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        if l.contains(needle) {
            for w in lines.iter().skip(i).take(n) {
                out.push_str(w);
                out.push('\n');
            }
        }
    }
    out
}

#[test]
fn every_execute_boundary_enforces_the_grounding_contract() {
    let mut missing = Vec::new();

    for (path, must_consume, why) in EXECUTE_BOUNDARIES {
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                missing.push(format!(
                    "{path}: could not be read ({e}). If this file moved, move \
                     the entry — do not delete it."
                ));
                continue;
            }
        };

        // A mention inside a comment is not a call. This repo's comments name
        // modules that are not invoked there constantly, and treating those as
        // coverage is the exact mistake the scan exists to catch.
        if !calls(&src, ENFORCE_CALL) {
            missing.push(format!("{path}\n         why it matters: {why}"));
        } else if !calls(&src, must_consume) {
            missing.push(format!(
                "{path}\n         calls `{ENFORCE_CALL}` but never uses the result \
                 (expected `{must_consume}`) — the document is checked and the \
                 verdict discarded\n         why it matters: {why}"
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "\n\n  {} execute boundary/boundaries do not call `{ENFORCE_CALL}`:\n\n         {}\n\n  \
         An agent's output reaching `episodes` unchecked is not contained there: \
         the consolidation worker distils it into a semantic rule and `kg_context` \
         appends that rule to a later prompt as a premise. A contract enforced on \
         one route and not another is a convention.\n",
        missing.len(),
        missing.join("\n\n         ")
    );
}

/// Every live path reaches the boundary, and gives it the grading.
///
/// The second half is the one that can rot. A handler that calls
/// `pulse.grade(..)` for its own use — the enforced document, to validate a
/// schema against — and then writes the episode without passing the `Graded` in
/// has enforced, graded, counted a gate verdict, and stamped none of it. The
/// row lands untagged, `grounding:enforced` never appears, and no surface can
/// tell that document from one nobody checked. That is the original defect with
/// a `Graded` value sitting one line above it.
#[test]
fn every_live_path_reaches_the_boundary_with_its_grading() {
    let mut missing = Vec::new();

    for (path, why) in BOUNDARY_CALLERS {
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                missing.push(format!(
                    "{path}: could not be read ({e}). If this file moved, move \
                     the entry — do not delete it."
                ));
                continue;
            }
        };

        let Some(close) = CLOSE_CALLS.iter().find(|c| calls(&src, c)) else {
            missing.push(format!(
                "{path} reaches none of {CLOSE_CALLS:?}, so nothing on this path \
                 enforces the field contract, stamps a grade, raises an anomaly \
                 or queues a claim\n         why it matters: {why}"
            ));
            continue;
        };

        // `persist` and `persist_opened` grade internally; only a handler that
        // graded for itself can drop the result.
        if calls(&src, ".grade(") && !after(&src, close, 4).contains("graded") {
            missing.push(format!(
                "{path} grades the document and does not pass the grading to \
                 `{close}`, so the episode is written with no grade on it and \
                 the enforcement it just performed is unrecorded\n         why it \
                 matters: {why}"
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "\n\n  {} live path(s) do not reach the grounding boundary:\n\n         {}\n\n  \
         `/execute` ran all six checks, the stream ran four, the workspace path ran \
         none — and the consequences were all visible and none attributable: nine of \
         ten contracted agents had never graded a field, `route:` was stamped on 0 of \
         3,581 episodes, and the verification queue held no rows for want of a writer. \
         The boundary is one function so that a new handler cannot forget it. Not \
         calling it is the only remaining way to forget.\n",
        missing.len(),
        missing.join("\n\n         ")
    );
}

/// The two general execute paths must both queue contracted claims.
///
/// # Why this is a second scan and not a third boundary
///
/// [`EXECUTE_BOUNDARIES`] includes `agent_backend/envelope.rs`, and the
/// delegation hop **cannot** enqueue: `assertion_verifications.episode_id` is a
/// real foreign key and the hop writes no episode. So the property is narrower
/// than enforcement — it belongs to the paths that persist an episode, and
/// stating it over the wider list would either fail on a file that is correct
/// or need an exemption that hides the real rule.
///
/// # Where it now lives
///
/// The `enqueue` call is in `episode_boundary::close`, so no handler contains
/// it any more. What each handler still decides is whether the boundary *can*
/// queue: `Write.db` is an `Option<&PgPool>`, and `None` means the fields are
/// graded, logged as unqueued, and never written. Nothing fails. That option is
/// the whole of the remaining hole, and it is what this scan now reads —
/// `db: Some(..)` inside the `Write` each handler constructs.
///
/// Which makes this strictly stronger than the scan it replaces. The old one
/// proved the text `verification_queue::enqueue` appeared somewhere in the
/// file; a handler could have called it under a condition that never held. This
/// one reads the argument that decides whether the write happens.
///
/// # Why it is enforced at all
///
/// `execution_stream.rs` said it in its own comment: *"Deliberately mirrors
/// `execution.rs` rather than sharing a helper: the two handlers already
/// duplicate their episode, credit and royalty logic, and the thing worth
/// preventing is not the duplication but the two paths silently DIVERGING. Keep
/// them edited in pairs."* That instruction was a comment, and comments do not
/// fail builds. It has already been ignored twice on this exact pair: grounding
/// was wired into `execution.rs` and not the stream, and claims were retained on
/// `execution.rs` since mig-187 and never on the stream — which was the whole of
/// the remaining loss after migration 213, because the console prefers the
/// stream. The comment has since been replaced by the shared helper it argued
/// against, on the grounds that three hand-kept copies diverged three times; the
/// history is the reason this scan does not simply trust that.
///
/// The consequence here is the same shape: whichever endpoint does not queue
/// becomes the one whose claims are never checked, and the queue looks healthy
/// because the other endpoint is filling it.
const QUEUE_BOUNDARIES: &[(&str, &str)] = &[
    ("src/handlers/execution.rs", "POST /api/agents/:id/execute"),
    (
        "src/handlers/execution_stream.rs",
        "the SSE streaming variant, which the Fermi Console prefers",
    ),
    (
        "src/handlers/workspace/messages.rs",
        "the @-mention path — measured on one weather pulse: 12 graded claims, \
         0 queued, so `a human could settle this` was an offer with no object",
    ),
];

/// The enqueue call, and the input it must have.
///
/// Both, because either alone is satisfiable while the feature is dead:
/// `graded_fields` with nothing consuming it grades and discards, and `enqueue`
/// over an empty vector is a call site that can never write a row. That is the
/// `provenance_floor_coverage` lesson — a scan asserting presence of a call and
/// nothing about its argument passed completely while producing the exact defect
/// it existed to prevent.
const ENQUEUE_CALL: &str = "verification_queue::enqueue";
const GRADED_CALL: &str = "grounding_trust::graded_fields";

/// The struct whose fields decide what the boundary is able to do, and the one
/// field of it a handler can get wrong without anything failing.
const WRITE_STRUCT: &str = "episode_boundary::Write {";
const POOL_SUPPLIED: &str = "db: Some(";

#[test]
fn the_boundary_queues_contracted_claims_for_verification() {
    let src = fs::read_to_string("src/episode_boundary.rs")
        .expect("src/episode_boundary.rs: the boundary cannot be read");
    assert!(
        calls(&src, ENQUEUE_CALL),
        "the boundary does not call `{ENQUEUE_CALL}`. Every episode-persisting \
         path goes through it, so nothing anywhere queues a contracted claim and \
         `assertion_verifications` returns to the 0 rows it held from migration \
         205 until a writer existed — for the whole platform at once, which is \
         the cost of having one boundary."
    );
    assert!(
        calls(&src, GRADED_CALL),
        "the boundary calls `{ENQUEUE_CALL}` and never `{GRADED_CALL}`, so the \
         enqueue has no contracted fields to work from and can never write a row"
    );
}

#[test]
fn both_execute_paths_queue_contracted_claims_for_verification() {
    let mut missing = Vec::new();

    for (path, why) in QUEUE_BOUNDARIES {
        let src = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                missing.push(format!(
                    "{path}: could not be read ({e}). If this file moved, move \
                     the entry — do not delete it."
                ));
                continue;
            }
        };
        let write = after(&src, WRITE_STRUCT, 10);
        if write.is_empty() {
            missing.push(format!(
                "{path} constructs no `{WRITE_STRUCT}`\n         why it matters: \
                 {why} — a path that does not reach the boundary queues nothing"
            ));
        } else if !write.contains(POOL_SUPPLIED) {
            missing.push(format!(
                "{path} reaches the boundary with no pool (`db: None`), so its \
                 contracted fields are graded, logged as unqueued and never \
                 written\n         why it matters: {why} — whichever endpoint \
                 does not queue becomes the one whose claims are never checked"
            ));
        }
    }

    assert!(
        missing.is_empty(),
        "\n\n  {} execute path(s) do not queue contracted claims:\n\n         {}\n\n\
         `assertion_verifications` held 0 rows from migration 205 until this \
         writer existed. A path that grades its fields and does not queue them \
         puts it back, silently, for that path only — and `db: None` is how that \
         now happens: it compiles, it logs a warning nobody reads, and the \
         verification queue looks healthy because the other endpoints fill it.\n",
        missing.len(),
        missing.join("\n\n         ")
    );
}

/// The queue scan can tell present from absent, a comment from a call, and a
/// supplied pool from a withheld one.
///
/// Its own falsifier, beside the enforcement one, because it is a separate
/// detector over a separate list. The third property is new and is the one that
/// matters most: the scan no longer looks for a call, it looks for an argument,
/// and `db: None` is a legal value that must read as absent.
#[test]
fn the_queue_scan_can_actually_fail() {
    let sentinel = "episode_boundary::WriteThatDoesNotExist {";
    for (path, _) in QUEUE_BOUNDARIES {
        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };
        assert!(
            !src.contains(sentinel),
            "{path} contains the sentinel, so the scan cannot distinguish \
             present from absent"
        );
    }

    let commented = "// fermi::episode_boundary::Write { db: Some(&state.db), ..";
    assert!(
        after(commented, WRITE_STRUCT, 4).is_empty(),
        "a commented-out `Write` was counted as coverage — the scan would pass a \
         path that had deleted its call to the boundary"
    );

    // The withheld pool must read as absent. This is the case the scan exists
    // for: it compiles, it is one word, and it silently turns the queue off for
    // that path only.
    let withheld = "\
    fermi::episode_boundary::Write {
        store: &state.memory_store,
        db: None,
        agent_slug: &agent_id,
    }
";
    assert!(
        !after(withheld, WRITE_STRUCT, 10).contains(POOL_SUPPLIED),
        "`db: None` was read as a supplied pool, so the scan cannot see the \
         one-word edit that stops a path queueing anything"
    );

    // And the positive direction, so the filter is not simply rejecting
    // everything: a real call site with a real pool must be seen.
    let real = "\
    fermi::episode_boundary::Write {
        store: &state.memory_store,
        db: Some(&state.db),
        agent_slug: &agent_id,
    }
";
    assert!(
        after(real, WRITE_STRUCT, 10).contains(POOL_SUPPLIED),
        "the scan rejects a genuine call site, so it is vacuous"
    );

    // The window must not be wide enough to borrow a neighbour's pool.
    // `execution.rs` supplies `db: Some(state.db.clone())` to its `ToolContext`
    // three hundred lines above its `Write`, and a scan that reached that far
    // would pass a handler whose boundary call said `None`.
    let neighbour = "\
    let ctx = ToolContext { db: Some(state.db.clone()) };
    fermi::episode_boundary::Write {
        db: None,
    }
";
    assert!(
        !after(neighbour, WRITE_STRUCT, 10).contains(POOL_SUPPLIED),
        "the window reads backwards or too widely, so an unrelated `db: Some` \
         elsewhere in the handler counts as the boundary's pool"
    );
}

/// The boundary-reachability scan can go red, and can tell a dropped grading
/// from a delivered one.
#[test]
fn the_boundary_caller_scan_can_actually_fail() {
    let sentinel = "episode_boundary::close_that_does_not_exist(";
    for (path, _) in BOUNDARY_CALLERS {
        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };
        assert!(
            !src.contains(sentinel),
            "{path} contains the sentinel, so the scan cannot distinguish \
             present from absent"
        );
    }

    // A handler that grades for its own use and writes without the grading.
    // Compiles, runs, strips the fields, records none of it.
    let dropped = "\
    let graded = pulse.grade(&agent_id, output.raw_response.as_deref());
    let status = fermi::schema_validate::validate(schema, graded.enforced.as_ref());
    fermi::episode_boundary::close(
        pulse,
        &Default::default(),
        fermi::episode_boundary::Write { store: &state.memory_store },
    )
";
    assert!(
        calls(dropped, ".grade("),
        "the scan cannot see a handler grading for itself"
    );
    assert!(
        !after(dropped, "episode_boundary::close(", 4).contains("graded"),
        "a write that receives no grading was counted as one that does, so an \
         episode stamped with nothing would pass"
    );

    // And the correct arrangement must clear it.
    let delivered = "\
    let graded = pulse.grade(&agent_id, output.raw_response.as_deref());
    fermi::episode_boundary::close(
        pulse,
        &graded,
        fermi::episode_boundary::Write { store: &state.memory_store },
    )
";
    assert!(
        after(delivered, "episode_boundary::close(", 4).contains("graded"),
        "the scan rejects a correct call site, so it fires on working code and \
         will be deleted"
    );
}

/// **A compiled card contract is enforced on the execute path, and stamping it
/// is what makes the document satisfy its own schema.**
///
/// # The seam that was open
///
/// `Pulse::grade` took an `output_contract` argument and discarded it:
/// `let _ = output_contract;`, under a comment saying
/// `enforce_from_output_contract` "is in flight on another working tree and is
/// not on `main`". It had been on `main` for some time, and `envelope::build`
/// — the delegation hop — had been calling it all along.
///
/// So this file's own opening paragraph applied again, one layer up. It
/// describes five agents whose "output was checked when another agent called
/// them and unchecked when a person did", and fixes that for the agents with a
/// **`FIELD_CONTRACTS`** entry. Ten more were in exactly that state via the
/// **card** path, including `simops_companion` (113 pulses) and
/// `supply_chain_oracle` (83) — and all ten recorded `Gate::Grounding` as
/// `undetermined`, meaning *no contract*, while carrying one the platform
/// already knew how to apply.
///
/// # Why the schema is the assertion, not the provenance stamp
///
/// Asserting "a stamp appeared" would pass on a stamp of any value, and the
/// interesting failure is a stamp the card's own schema rejects. These
/// contracts declare their `_provenance` siblings **required**, with an `enum`
/// drawn from the platform's own vocabulary:
///
/// ```text
/// required:           [items, items_provenance, risks, risks_provenance, summary, summary_provenance]
/// items_provenance:   enum [tool_verified, tool_no_match, unavailable_no_tool_source]
/// ```
///
/// Only the stamper can legitimately write those, and the platform's verdict
/// overwrites whatever the model emitted. So while the seam was open, the
/// document was **necessarily invalid against its own declared schema** — three
/// required properties absent on every pulse — and the execute path reported
/// exactly that, to nobody in particular.
///
/// That makes this the rare coverage change with no trade: enforcement engages
/// AND schema validity improves, because the missing keys were the platform's
/// to supply.
#[test]
fn a_compiled_card_contract_is_enforced_and_satisfies_its_own_schema() {
    let card: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/agents/curated/supply_chain_oracle/agent_card.json"
        ))
        .expect("supply_chain_oracle card"),
    )
    .expect("the card must parse");

    let output_contract = card
        .pointer("/capabilities/output_contract")
        .expect("this agent's coverage comes from its compiled card contract");
    let schema = output_contract
        .get("schema")
        .expect("the contract must declare a schema for this test to mean anything");

    // Chosen because it is a real, busy, card-only agent with no `unavailable`
    // block: the only thing enforcement can do here is stamp, so a failure is
    // unambiguously about the seam and not about a strip.
    assert!(
        output_contract
            .pointer("/grounding/items")
            .and_then(|g| g.get("status"))
            .and_then(|s| s.as_str())
            == Some("sourced"),
        "the fixture assumes `items` is a sourced block; the card changed and \
         this test is now asserting something else"
    );

    // What the model returns: the three declared blocks, and none of the
    // `_provenance` keys its schema requires. That absence is the point.
    let raw = serde_json::json!({
        "items": { "name": "ashwagandha root", "unit": "kg", "currency": "USD",
                   "unit_cost": 12.5, "risk_flags": ["seasonal"] },
        "risks": { "item": "ashwagandha root", "flag": "seasonal",
                   "severity": "medium", "description": "monsoon-dependent harvest" },
        "summary": { "currency": "USD", "total_bom_cost": 12.5,
                     "oracle_note": "single supplier region" }
    })
    .to_string();

    // The model's own document does not satisfy the contract it declares.
    let before = fermi::schema_validate::validate(
        schema,
        &serde_json::from_str::<serde_json::Value>(&raw).unwrap(),
    );
    assert!(
        !before.is_valid(),
        "the fixture is supposed to be missing the required `_provenance` keys, \
         so that the stamper is what supplies them. If this document is already \
         valid the test proves nothing."
    );

    let pulse = fermi::episode_boundary::Pulse::after_the_fact(
        uuid::Uuid::nil(),
        "a card-contract coverage fixture",
    );
    let graded = pulse.grade("supply_chain_oracle", Some(output_contract), Some(&raw));

    let enforced = graded
        .enforced
        .as_ref()
        .expect("a parseable document must survive grading");

    // The stamps the schema requires, with the values its enums allow.
    assert_eq!(
        enforced.get("items_provenance").and_then(|v| v.as_str()),
        Some(fermi::grounding_trust::PROV_TOOL),
        "a `sourced` block with content must be stamped `tool_verified`; the \
         compiled path did not run, so `output_contract` is being discarded \
         again"
    );
    assert_eq!(
        enforced.get("risks_provenance").and_then(|v| v.as_str()),
        Some(fermi::grounding_trust::PROV_INFERRED),
        "an `inferred` block must be stamped `model_inference`"
    );

    // And the whole document now satisfies the schema it declares.
    let after = fermi::schema_validate::validate(schema, enforced);
    assert!(
        after.is_valid(),
        "the enforced document still fails its own declared schema: {:?}\n\
         Enforcement and schema validation must not disagree — the execute path \
         validates the ENFORCED document, so a stamp the schema rejects turns \
         every pulse of this agent invalid.",
        after
            .violations
            .iter()
            .map(|v| format!("{}: {}", v.path, v.message))
            .collect::<Vec<_>>()
    );

    // And the coverage claim itself: the gate now has an opinion.
    //
    // This is what `undetermined` meant on all ten of these agents — not "the
    // gate looked and found nothing wrong" but "no contract, nothing to
    // check". `has_contract` in `Pulse::grade` read only `FIELD_CONTRACTS`,
    // so a card-only agent recorded no-contract on every pulse while its
    // contract sat in the card being enforced at the delegation hop.
    //
    // Draining is safe here: every other test in this file is a source scan
    // and none of them reaches a gate.
    let queued = fermi::gate_trust::drain();
    let grounding: Vec<_> = queued.iter().filter(|d| d.gate == "grounding").collect();
    assert_eq!(
        grounding.len(),
        1,
        "one graded document is one grounding decision; got {:?}",
        queued.iter().map(|d| d.gate).collect::<Vec<_>>()
    );
    assert_eq!(
        grounding[0].decision, "approved",
        "the gate recorded `{}` for an agent whose card carries a grounding \
         map. `undetermined` here means NO CONTRACT, which is what all ten \
         card-only agents reported while the seam was open — so `has_contract` \
         is still reading only FIELD_CONTRACTS.",
        grounding[0].decision
    );
    assert_eq!(
        grounding[0].subject.as_deref(),
        Some("supply_chain_oracle"),
        "the decision must name the agent it was about, or the row cannot be \
         reviewed"
    );

    // Nothing was stripped: no block here is `unavailable`.
    assert!(
        graded.report.violations.is_empty(),
        "enforcement removed something from an agent whose contract declares no \
         `unavailable` block: {:?}",
        graded.report.refusal_reason()
    );
}

#[test]
fn the_scan_can_actually_fail() {
    let sentinel = "grounding_trust::enforce_a_call_that_does_not_exist";
    let mut found_none = true;

    for (path, _, _) in EXECUTE_BOUNDARIES {
        let Ok(src) = fs::read_to_string(path) else {
            continue;
        };
        if src.contains(sentinel) {
            found_none = false;
        }
    }

    assert!(
        found_none,
        "the sentinel was found in a source file, so the scan cannot \
         distinguish present from absent"
    );

    // And confirm the comment filter bites: a commented-out call must not
    // count as coverage.
    assert!(
        !calls("// grounding_trust::enforce(agent, doc);", ENFORCE_CALL),
        "a commented-out call was counted as coverage — the scan would pass a \
         file that had deleted its enforcement"
    );
    assert!(
        calls(
            "    let r = grounding_trust::enforce(agent, doc);",
            ENFORCE_CALL
        ),
        "the filter rejects a genuine call site, so the scan is vacuous"
    );
    // The needle must still see the general entry point. `envelope.rs` moved to
    // it mid-consolidation, and a needle ending in `(` reported the delegation
    // hop as unenforced while it was enforcing correctly.
    assert!(
        calls(
            "    let r = grounding_trust::enforce_from_output_contract(a, oc, doc);",
            ENFORCE_CALL
        ),
        "the needle no longer matches `enforce_from_output_contract`, so an \
         agent whose contract is compiled into its card is scanned as \
         uncontracted"
    );
}
