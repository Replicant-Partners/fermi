//! The `agent` block the console writes is the one this parser reads.
//!
//! # The defect this exists to prevent
//!
//! `generate_fpl_text` in `crates/fermi-console/src/cockpit.rs` rebuilds the cached
//! FPL from the AST on every save. It walked question → drivers → evidence → model
//! → simulate and emitted no `agent` statements at all. So an assignment made by
//! `assign_agent_to_driver` lived in the AST, was dropped on emit, and the save
//! reported success. The only visible trace was a driver reading "⚠ No agents" the
//! next time the forecast was opened — while the schedule panel still listed the
//! agent, because schedules are persisted server-side, outside the FPL.
//!
//! `cached_fpl_is_richer_than_ast` looked like a guard against this and was not: it
//! declines to regenerate when the *text* already contains `agent `, which protects
//! a forecast that arrived with one and cannot protect an assignment just made —
//! the AST has it, the text does not, so the guard sees nothing worth saving.
//!
//! # Why the test lives here and not next to the emitter
//!
//! It cannot live next to the emitter. `cargo test -p fermi-console --bin
//! fermi-console` segfaults rustc expanding GPUI's element chains under `--test`,
//! which is why that crate keeps its testable logic in a GPUI-free lib target and
//! why `cockpit.rs` — a bin-only module — has no tests of its own.
//!
//! So this pins the *format contract* from the other side: the exact text the
//! emitter is written to produce, asserted to survive a parse with every field
//! intact. If someone changes this parser's `agent` grammar, this test fails and
//! names the emitter as the thing to update. That the emitter actually emits this
//! shape is verified by review, not by CI, and that remains a known gap.
//!
//! The same trick as `the_probability_line_the_card_declares_is_the_one_this_module
//! _parses`: when the producer is untestable, pin the wire format both sides agree
//! on.

use fermi::ast::{ExecutorType, Schedule, Statement, TimeUnit};
use fermi::{Lexer, Parser};

fn parse(src: &str) -> fermi::ast::Program {
    let tokens = Lexer::new(src)
        .tokenize()
        .unwrap_or_else(|e| panic!("tokenize failed:\n{src}\n\n{e:?}"));
    Parser::new(tokens)
        .parse()
        .unwrap_or_else(|e| panic!("parse failed:\n{src}\n\n{e:?}"))
}

/// Byte-for-byte the shape `generate_fpl_text` writes, for a manual assignment.
///
/// `schedule: once` is the load-bearing line. Every assignment made through the
/// picker or through the chat `assign_agent` tool is `Schedule::Once`, so if this
/// one line does not round-trip, the emitter is unusable for the only case that
/// actually occurs.
const EMITTED: &str = r#"question "Will KORD hit 74-75F on Aug 22?"

driver ensemble_spread continuous {
    distribution: normal(2.0, 0.5)
    unit: "degrees_f"
    applies_to: quantity
}

agent weather_oracle_ensemble_spread {
    type: "research"
    query: "What is the GEFS ensemble spread for KORD 2m temperature on Aug 22?"
    executor: "llm"
    schedule: once
    driver_refs: ["ensemble_spread"]
    confidence_threshold: 0.7
}

model: ensemble_spread

simulate 10000 iterations
"#;

#[test]
fn the_agent_block_the_emitter_writes_parses_back_with_every_field() {
    let program = parse(EMITTED);
    let agents = program.agents();

    assert_eq!(
        agents.len(),
        1,
        "the emitted `agent` block did not survive the parse at all — \
         this is the original defect, reproduced"
    );
    let a = agents[0];

    assert_eq!(a.name, "weather_oracle_ensemble_spread");
    assert_eq!(a.agent_type.as_deref(), Some("research"));
    assert_eq!(
        a.query,
        "What is the GEFS ensemble spread for KORD 2m temperature on Aug 22?"
    );
    assert_eq!(a.executor, Some(ExecutorType::LLM));
    assert_eq!(
        a.schedule,
        Some(Schedule::Once),
        "`schedule: once` did not round-trip; the emitter writes it on every \
         manual assignment"
    );
    assert_eq!(
        a.driver_refs,
        vec!["ensemble_spread".to_string()],
        "driver_refs is the whole point of the statement — an agent bound to \
         nothing researches nothing"
    );
    assert_eq!(a.confidence_threshold, Some(0.7));
}

/// The other statements are still there.
///
/// The regeneration guard added alongside the emitter compares a per-kind census
/// before and after a reparse and refuses to overwrite `cached_fpl` when a kind
/// would be lost. That guard is only as good as this being true for a document the
/// emitter itself produced: if emitting an agent broke the surrounding program, the
/// guard would fire on every save and the cached text would freeze.
#[test]
fn emitting_an_agent_does_not_disturb_the_rest_of_the_program() {
    let program = parse(EMITTED);

    let count = |f: &dyn Fn(&Statement) -> bool| program.statements.iter().filter(|s| f(s)).count();

    assert_eq!(count(&|s| matches!(s, Statement::Question(_))), 1);
    assert_eq!(count(&|s| matches!(s, Statement::Driver(_))), 1);
    assert_eq!(count(&|s| matches!(s, Statement::Agent(_))), 1);
    assert_eq!(count(&|s| matches!(s, Statement::Model(_))), 1);
}

/// Every `Schedule` variant has a surface syntax, so the emitter never has to lie.
///
/// It had two holes. `Once` was reachable only as the value of an ABSENT field, and
/// `Cron` could not be written at all — which left the emitter choosing between
/// dropping a cadence and writing something unparseable. Both are the same defect
/// as the missing `agent` block: a value the AST can hold and the text cannot say.
#[test]
fn every_schedule_variant_can_be_written_and_read_back() {
    let cases: [(&str, Schedule); 4] = [
        ("once", Schedule::Once),
        (
            "every 6 hours",
            Schedule::Every {
                interval: 6,
                unit: TimeUnit::Hour,
            },
        ),
        (
            "every 1 week",
            Schedule::Every {
                interval: 1,
                unit: TimeUnit::Week,
            },
        ),
        ("cron \"0 */6 * * *\"", Schedule::Cron("0 */6 * * *".into())),
    ];

    for (text, expected) in cases {
        let src = format!(
            "question \"q\"\n\nagent a {{\n    query: \"x\"\n    schedule: {text}\n    \
             driver_refs: [\"d\"]\n}}\n"
        );
        let program = parse(&src);
        assert_eq!(
            program.agents()[0].schedule,
            Some(expected),
            "`schedule: {text}` did not round-trip"
        );
        assert_eq!(
            program.agents()[0].driver_refs,
            vec!["d".to_string()],
            "`schedule: {text}` consumed the wrong number of tokens — the field \
             after it was swallowed"
        );
    }
}

/// An absent `schedule:` still means once.
///
/// 118 agent blocks in the corpus omit the field. Giving `once` a spelling must not
/// change what leaving it out means.
#[test]
fn an_absent_schedule_still_means_once() {
    let src = "question \"q\"\n\nagent a {\n    query: \"x\"\n    driver_refs: [\"d\"]\n}\n";
    assert_eq!(parse(src).agents()[0].schedule, None);
}

/// A cadence the parser does not know is an error, not a silent `once`.
///
/// `schedule: daily` used to return `Once` WITHOUT consuming `daily`, so the field
/// loop read `daily` as the next field name and failed complaining about a missing
/// colon — a diagnostic that points at the wrong token and names the wrong problem.
/// Accepting it quietly would be worse: a forecast that says it re-runs daily and
/// never runs again.
#[test]
fn an_unknown_cadence_is_rejected_rather_than_silently_meaning_once() {
    let src = "question \"q\"\n\nagent a {\n    query: \"x\"\n    schedule: daily\n}\n";
    let tokens = Lexer::new(src).tokenize().expect("tokenize");
    let err = Parser::new(tokens)
        .parse()
        .expect_err("`schedule: daily` must not parse");

    let msg = format!("{err:?}");
    assert!(
        msg.contains("daily"),
        "the error must name the token the author actually wrote, got: {msg}"
    );
}
