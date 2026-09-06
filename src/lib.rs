// See the note in `src/api_server.rs`: a gate decision or a write-accounting
// record placed after the `return` it is meant to describe is never taken, and
// the counter reads zero while the code around it works. rustc catches this
// precisely; nothing else does.
#![deny(unreachable_code)]

/// Fermi - Forecasting Programming Language (FPL)
///
/// This library implements the FPL language processing engine,
/// including lexer, parser, semantic analyzer, and execution engine.
pub mod ast;
pub mod attribution;
pub mod distributions;
pub mod evaluator;
pub mod executor;
pub mod lexer;
pub mod parser;
pub mod report;
pub mod semantic;
pub mod sensitivity;
pub mod symbol_table;
pub mod types;

// Agent Backend (Phase 2)
pub mod agent_backend;

// REST API (Phase 2)
pub mod api;

// Gas fees and workflow engine
pub mod gas;
pub mod glasses_shell;
pub mod workflows;

// App primitive — registry + manifest builder
// (handlers live in handlers::apps; builder substrate is here so the CLI,
// the xamanEK app_design session, the fork-from-workspace flow, and the
// auto-seed-from-filesystem path all share the same validation rules.)
pub mod apps;

// Shared slug validation for every publishable artifact (agents, apps,
// workspaces, compositions, …). Re-exports the same rules App slugs
// already use so the platform has a single mental model.
pub mod slug;

// Polymarket integration (prediction market data)
pub mod polymarket;

// Voice synthesis
pub mod voice;

// Outbound transactional email (Resend). No-ops when unconfigured.
pub mod email;

// Schema trust contract (v0.11.0) — the hand-declared manifest of every
// schema object the Rust code assumes exists, plus the boot-time probe
// that verifies it against the live DB.
//
// Declared here, not `#[path]`-included into `api_server.rs`, so that
// `cargo test` can see it. It previously lived in the binary only, which
// is why an unsatisfiable contract survived eight releases unnoticed.
// The verification ladder, as the paper names it. Three of the five modules are
// named for their mechanism rather than their rung, and each declares its own
// position relative to whatever existed when it was written — a chronology, not
// a ladder, and the two disagree on three of five. This is the map, with tests.
pub mod ladder;

pub mod schema_trust;

// Rollup trust contract — the sibling that asks whether a column is
// telling the truth, not merely whether it exists. `schema_trust` would
// have caught `agents.total_executions` disappearing; it could not catch
// the column being present, correctly typed, and permanently zero because
// nothing ever wrote it. Content drift needs its own contract.
pub mod rollup_trust;

// Grounding trust contract — the third sibling. `schema_trust` asks whether
// a column exists; `rollup_trust` asks whether it is true; this asks
// whether a value could have come from anywhere at all.
//
// `genome_profiler` was asked for genome size, karyotype, divergence date
// and IUCN status with two GBIF tools that return taxonomy only, and filled
// all of it confidently for 56 episodes. Presence and content checks both
// pass on that: the field is there and internally consistent. Only a
// contract that maps output fields to the tools that could supply them
// catches it.
pub mod grounding_trust;

// The one implementation of `ProvenanceOracle`, which carries the grounding
// verdict across the crate boundary into the memory layer.
//
// Semantic rules are extracted from episodes during dream cycles and then
// injected into other agents' prompts — they become things the platform tells
// its own agents are true. Without a floor, a rule extracted from prose is
// stored and retrieved identically to one extracted from tool output, and the
// citation makes it worse than a bare hallucination: `source_episode_cluster`
// genuinely points at episodes that genuinely said that. The laundering runs
// outward, into the whole ecology.
//
// The oracle lives in `fermi` because the field contracts do, and the memory
// crate declares a trait instead of copying the arithmetic — a second copy of
// a trust calculation is a second answer to the same question, and the one
// that disagrees is the one nearest the writer.
pub mod provenance_oracle;

// Verification corpus — the domain layer for community expert determination.
//
// Hodgson et al. 2023 measured phone mushroom-ID apps at 49% best case against
// expert-confirmed specimens. That figure disqualifies an app that tells people
// what is safe to eat, and it is the curriculum for one that teaches how
// unreliable photographic identification is.
//
// An expert determination is also the second independent determiner that
// `grounding_trust`'s cross-check exemption for `forage_identify.taxonomy` calls
// for — recorded there as a capability decision rather than a missing query. It
// was never a structural limit; it was an absence of people.
pub mod verification;

// Image attachments — what may travel with a request, and the rule that an
// undeliverable frame is an error rather than a silent omission.
//
// A camera agent has a failure mode the text-only platform does not: the
// picture goes missing and the answer still arrives. "What is this?" with no
// image attached still gets a species name, correctly labelled
// `model_inference` by a boundary that cannot tell an inference-from-a-photo
// from an inference-from-nothing. Worse than genome_profiler rather than equal
// to it: there the gap was permanent and nameable, here the same field is
// well-sourced or evidence-free depending on whether a blob survived the trip.
pub mod attachments;

// HUD contract — the display-layer sibling of `grounding_trust`, for agents
// whose output is read on glass in half a second rather than parsed.
//
// `grounding_trust` nulls what nothing could supply and stamps
// `<block>_provenance`, which is enough for a consumer that reads the tag. A
// heads-up display is not that consumer: a correctly-tagged guess rendered as
// identical text to a verified retrieval reproduces the `genome_profiler` harm
// through the presentation layer. So this module derives a visible treatment
// per line, conditions every lookup on the provenance of the subject it was
// keyed on, and computes `confidence_display` rather than accepting it.
pub mod hud_contract;

// What an agent quantified, recorded whether or not there is a forecast to
// bind it to.
//
// `forecast_agent_claims.workspace_id` is NOT NULL, which is right for a claim
// — a claim is an adjustment to a driver and is neutralisable at 1.0, which is
// what makes exact Shapley credit cheap. But it was the ONLY home for an
// agent's quantified output, so a standalone evaluation lost it: 14 quantified
// judgements measured, 14 discarded, 0 claims. Standalone evaluation is how
// agents are mostly exercised, so no agent could build a track record.
//
// An assertion is what the agent said; a claim is that assertion bound to a
// driver, 0..n per assertion. The load-bearing rule is that a multiplier can
// never be tool_verified — no database contains "the multiplier for this
// driver" — so a multiplier is not a checkable proposition and verification
// routes to its basis instead.
pub mod assertions;

// Write accounting — the rung beneath liveness. Liveness asks whether a sink
// has rows; this asks whether anybody ever tried to write one. Both are
// `Silent` from outside, and they have opposite remedies: a missing scheduler
// versus a statement the database refuses. An audit found 30 swallowed write
// sites across 15 feedback-loop sinks and no failure counter anywhere in the
// repository, which is how one INSERT hid two consecutive silent rejections.
//
// In memory on purpose: a failure ledger that is itself a fallible database
// write is most silent exactly when it is most needed.
pub mod write_accounting;

// Gate accounting — what every refusal point decided, and how often. No gate
// decision was persisted anywhere: the coherence gate returned a 422 and
// nothing, credit refusals returned before the ledger INSERT, the rate limiter
// was an in-memory map with no export. The platform had a record of every
// request it served and none of any it refused, which is how a gate that
// rejected 100% of agent-wide interventions for arithmetic reasons survived.
//
// The reading nobody checks is the inverse: a gate that has never refused is
// indistinguishable from a gate that is not wired.
pub mod gate_trust;

// Seam vocabularies — every closed token set a column accepts, indexed once and
// checked both ways against the live schema. Postgres holds one opinion in a
// CHECK constraint and Rust holds another in a string literal at the write
// site; each is independently correct, nothing compares them, and the drift is
// silent in both directions. `severity = "L1"` was one direction; migration
// 200 widening `anomaly_events.kind` for a producer nobody wrote was the other.
pub mod seam_vocabulary;

// The five feedback loops, declared as chains rather than measured stage by
// stage. Two of five turn; Loop 2 and Loop 4 have produced zero rows at every
// stage. Read per stage that is ten findings, read as chains it is two, and
// only the first link of each is actionable. The interpretation is delegated:
// `no_trigger` from this model, `writes_refused` from write_accounting,
// `gate_refuses_everything` from gate_trust.
pub mod loop_model;

// Native evaluators — the pluggable registry scores an agent's OUTPUT; this one
// scores the platform's own machinery. Separate on purpose: mixing "is this
// response harmful" with "is Loop 4 turning" gives two different questions one
// health verdict. And none of the pluggable scores mean anything if the loops
// they feed are not closing.
pub mod native_evaluators;

// Why is this panel empty? A routing table from each UI surface that can be
// blank to the contract that explains it, so no frontend authors its own empty
// state. Four of the nine defect classes in FEEDBACK_LOOPS.md are invisible at
// the surface by construction and render identically as "No data yet" — a
// severed read path, a loop that is closed but not turning, a gate that
// declined correctly 248 times, and a callee that has failed non-fatally since
// inception. Collapsing them is how a verification signal becomes a shrug.
//
// It owns no arithmetic: liveness answers unused-vs-broken, loop_model answers
// which link a chain stops at, gate_trust answers what was refused. The panels
// nothing can answer are listed with reasons and the list may only shrink.
pub mod panel_absence;

// One stamp, three densities. The server decides what a panel says on a desk, a
// phone and a waveguide, and every surface copies it — the split the glasses
// shell already documents: "It decides nothing. […] The glasses are I/O."
//
// It owns the density ladder and delegates every treatment decision to
// hud_contract, which keeps one vocabulary for the provenance question and
// gives that module its first production caller. Two rules are load-bearing and
// both are tests: an absence may never render as the unmarked trustworthy case,
// and dropping detail may never buy confidence.
pub mod panel_contract;

// Every verb the platform offers, and what governs it. The router knows every
// route; what it cannot say is which of them change something and which gate
// stands in front of that change — the question the gate audit had to answer by
// reading code, and whose answer nothing kept current.
//
// It declares the audit's §3 table as a live query: grounding is a control on
// the creature handlers and a metric on the two execute endpoints a third party
// actually calls, and on the surface a caller sees, a metric and an absent gate
// are the same thing. A write must name a gate that can refuse it or say why it
// needs none; the list of discarded verdicts is pinned and may only shrink.
pub mod command_registry;
// Did the agent fill the fields it was asked for? The one question on the
// artifact trace that no checkpoint stood behind.
pub mod completeness;

// Liveness trust contract — the fifth sibling, and the one that would have
// caught the other four. Every contract above examines data that EXISTS; none
// of them can see a table that is empty because nothing ever wrote to it.
//
// That blind spot produced five findings in an afternoon: a CHECK constraint
// declared by seventeen migrations and applied by none; a provenance oracle
// wired into one of three call sites; `forecast_agent_claims` coded, wired,
// exhaustively commented and holding zero rows; `anomaly_events` never fired;
// and `semantic_rules.application_count` declared in migration 010 and never
// incremented. Reading the code proves nothing — in every case the code looks
// right.
//
// Nobody writes this check because `count(*) = 0` is ambiguous. The
// disambiguator is the OPPORTUNITY count: zero claims beside fourteen
// multiplier-bearing episodes is broken; zero beside zero is merely unused.
pub mod liveness_trust;

// One definition of "this observation is a model projection, not a
// measurement". There were two, and they selected disjoint sets: the dynamics
// runner tags `extra.source_kind = "dynamics_projection"` (12,167 rows) and
// every consumer matched `extra.source = "simops_simulation"` (0 rows).
// Loop 5.A (projection accuracy) read the empty set and its liveness rung
// reported the mismatch as 12,167 missed opportunities at the trigger site,
// which is not where the break is.
pub mod projection_kind;

// What `anomaly_events` will accept. Loop 2's seed — the grounding anomaly that
// was supposed to break the loop's deadlock — wrote `severity = "L1"` against a
// CHECK of ('info','warning','critical'). Every insert was rejected, in a
// spawned task, with the error only logged, so the table stayed at zero and the
// handover said to watch for rows that could never arrive.
pub mod anomaly_vocabulary;

// The one way a grounding violation becomes a Loop 2 input. Nine files call
// `grounding_trust::enforce`; one raised an anomaly, and that one carries ~1%
// of the traffic from agents that have contracts. The creature paths run the
// control — they strip the fabricated field — and then say nothing to Loop 2.
pub mod grounding_anomaly;

// Grounding gate for the regulatory lens translator (Adaptogen Lab demo suite).
//
// Where `grounding_trust` asks "could this value have come from anywhere?",
// this module asks the next question for the lens translator specifically:
// "for the values that *should* come from the ruleset YAML, did the agent
// faithfully reproduce what the ruleset says?"
//
// Two provenance classes:
//   Ruleset-sourced (PROV_TOOL): rendered_text, status, allergen format,
//   ingredient status, verification_appendix. Must come from reading the
//   ruleset YAML via read_workspace_file.
//   Inferred (PROV_INFERRED): divergence_note, summary_divergence. The agent
//   reasons across rulesets; these are judgements, not retrievals.
//
// The gate produces violations in the same Report type as enforce(), so
// grounding_anomaly::spawn_raise sees a single merged picture. Merge with
// merge_reports() before raising.
pub mod lens_rendering;

// The commitment anchor for a projection — the row that proves a prediction
// pre-dated the measurement it is scored against. It lived in the api-server
// binary, which the library cannot reach, so the agent tool that writes
// projections had a `let _ = (…every argument…)` where the call belongs and
// returned a null commitment hash on both arms of the branch. 0 rows written
// against 61 projections on file.
pub mod projection_commit;

// Why a run produced no quantified claim. Same boundary as `projection_commit`
// above and moved for the same reason: the decision lives in the api-server's
// handler tree, which an integration test cannot reach, so it could not be
// registered in `tests/falsification_registry.rs` — and the rule that registry
// enforces is that a decision without a falsification does not get added.
pub mod claim_outcome;

// Does what a loop produces carry the signal its claim needs? `loop_model`
// reports a loop as turning when every stage has produced rows; that is
// compatible with the loop producing a number which cannot distinguish the
// things it is named after, and Loop 5.A is in exactly that state — the
// forecast's Brier, written once per contributing agent, identical every time.
// Turning is not closed, and nothing until now asked the difference.
pub mod outcome_trust;

// One shape for "where does this loop stand, and what can a person do about
// it". Assembly only — `loop_model`, `panel_absence` and `outcome_trust` keep
// their answers. It exists because those answers were reachable only through
// an admin diagnostic blob and through a 610-line handler giving a second
// answer to the first one's question, and because nothing anywhere declared
// the endpoint a person uses to work a human-gated stage.
// The shape every trust surface has — the door a person uses, and the caveat a
// green tick needs — declared once so loops, gates and evaluators inherit the
// rules rather than three copies of them. Answers are never shared; only these
// two, which are the same idea and the same mistakes in all three domains.
pub mod surface;

pub mod loop_api;

// The same pattern over gates. Second instance, which is what shows `surface`
// is a pattern rather than a rename: the door and the caveat are shared, and
// the model, measurement and interpretation stay with `gate_trust`.
//
// `GATE_DOORS` was empty, and that was a finding: nothing anywhere let a person
// act on a gate — no review of what it refused, no override, no record that a
// refusal was wrong. Until this list existed there was nowhere to notice it had
// never been decided. It is no longer empty; see `gate_review`.
pub mod gate_api;

// What crossed the boundary, as a digest.
//
// Computed on read rather than stored, and that is the interesting choice:
// `episodes.query` and `response_text` are both retained, so a hash of them is a
// pure function of data the platform already holds — and a computed digest
// cannot drift from the text it claims to describe, which a stored one can.
//
// It is also honest about what it cannot do. The seam check as originally framed
// — this episode's input hash against the parent's output hash — does NOT work
// here, because a delegated child receives a prompt built around the task rather
// than its parent's output verbatim. The place equality would hold is the
// envelope payload, and nothing hashes that yet.
pub mod artifact_hash;

// One artifact, and the checkpoints it passed — the instance-level counterpart
// to `surface`.
//
// `surface` is population-level: how many loops turn, how many gates
// discriminate. The UX team's verdict on it was right — it is legible only to
// someone who already holds the machine in their head. This inverts the primary
// object: one episode crossing one route, passing checkpoints where rungs fire.
// The two are the same structure from opposite ends.
//
// It holds no verdict of its own. Every judgement it renders belongs to a module
// that already owns it and has a falsification registered — including the reason
// an empty trace is empty, which comes from `declaration_ladder` because 3,571 of
// 3,576 episodes have nothing to show and that is the agents' missing
// declarations rather than a platform defect.
pub mod artifact_trace;

// Enqueuing a contracted field for verification — the writer
// `assertion_verifications` has never had.
//
// The table has existed since migration 205, is keyed to both the assertion and
// the episode, and carries the CHECK that makes a human verdict cost something.
// It has held 0 rows for its whole life, and the audit's conclusion was exactly
// right: it needs a writer, not a schema.
//
// The content comes from contracted FIELDS rather than from prose-extracted
// numbers, and that is forced rather than chosen: all 94 assertions in production
// are `Multiplier` or `Probability`, neither of which is verifiable, because you
// cannot verify a multiplier. A contracted field purports to be a retrieval, so
// it is checkable — and the contract already names the tool that could settle it,
// which is why the tool-versus-person routing costs nothing to wire.
pub mod verification_queue;

// The one place an agent's pulse becomes a row.
//
// The six checks above — reserve, enforce, stamp, stamp, decide, enqueue — were
// calls at call sites, and the call sites diverged three times running. The
// parity test written to catch the fourth divergence was a list of three files,
// and the list was wrong: twelve more writers persist an episode, seven from a
// genuine agent invocation. A scan is only as good as its list, so the remedy is
// not a longer list but a single entry point and a ratchet that bans the raw
// write. A new handler cannot forget the boundary because there is nothing else
// to call.
pub mod episode_boundary;

// Running the tool a field contract names.
//
// Sixteen tools are named across the contracts as the thing that could settle a
// field, and the trace printed those names beside rows with no way to run them.
// A name the platform can print and cannot offer is a description, not an
// affordance. This is the narrow door that turns one into the other: it runs the
// contract's tool and hands back what came out, and it decides nothing, because
// the contract does not say where in a response the value lives.
pub mod field_probe;

// What must an agent declare before this substrate can say anything about it?
//
// Every trust surface reports `unknown` more often than anything else, and the
// cause had never been separated from the other causes. Measured: of 206 agents
// that have produced an episode, **110 are `test_agent_*` rows declaring
// nothing**, and of the 96 real ones 93 declare ports, 2 a checkable schema and
// 7 a field contract. So `unknown` is overwhelmingly the SUBJECT declaring no
// structure to check against — not a stalled loop, not a cold counter, and not a
// contract the platform failed to write.
//
// That distinction is the module's whole reason to exist: `Unresolved` is a work
// item for us, `Undeclared` is a work item for the agent's author, and
// collapsing them made 89 undeclared agents look like 89 contracts the platform
// owed. It owes none of them.
pub mod declaration_ladder;

// The judgement half of the gate surface: was the refusal right?
//
// `gate_trust`'s readings come from approve/refuse counts, and no arrangement of
// counts distinguishes a correct refusal from an incorrect one. A gate that
// approves 90% and refuses the other 10% wrongly reads `discriminating`, which
// the surface renders as healthy. Correctness is a judgement about the subject,
// not a property of a count, so this is the only path by which "is this gate
// refusing the right things" becomes answerable at all.
//
// It records and does not override. `Overturned` changes no behaviour; it makes
// a wrong refusal visible to the person who can change the code.
pub mod gate_review;

// And over evaluators — third instance. `native_evaluators` already turns
// counters into sentences with remedies, and was reachable through exactly one
// admin-scoped diagnostics blob. `EVALUATOR_CAVEATS` is where
// `loop_stalled_in_code`'s known over-claim is recorded rather than silently
// fixed: narrowing it flips a live verdict platform-wide.
pub mod evaluator_api;

// Delivering a coordination finding into a member agent's memory — Loop 3's
// terminal half, which has produced 0 of 3,576 episodes.
//
// The mechanism was asking a language model to perform a side effect. The
// *content* of a coordination finding is a judgement and belongs to the model;
// the *delivery* is bookkeeping and belongs to the platform. Loop 3 asked the
// model to do both, so its closure was contingent on a tool call the model
// never made.
pub mod coordination_note;

// Port trust contract — whether the caller is sending what the agent said it
// takes. `negotiate::bind_input` in the console answered this correctly and
// was wired only into the desktop client, so every HTTP execute path went
// unchecked; worse, `stamp_invocation` recorded the CALLER's claim about the
// binding as if it were a finding. Canonical implementation lives here so
// the server can verify rather than transcribe.
pub mod port_trust;
// What a caller may rely on, in one token, derived from the verdicts the
// gates already produced. The prerequisite for promoting any of them to a
// Control: callers must already branch on this field before a refusal can
// appear in it.
pub mod reliance;
pub mod route_trust;

// Card-declared contracts — the typing and field-to-tool rules an agent
// must state about itself before it may be published.
//
// `grounding_trust` holds a Rust const table, which works for curated
// agents someone hand-wrote an entry for and cannot work for anyone else:
// a third party publishing over the API has no way to add a line to a
// compiled const. So the map lives in the card and Rust keeps the checker.
pub mod card_contract;

// The authoring surface for the above. 98 of 101 curated cards declare no
// typed contract, and the one card that satisfies `card_contract` in full
// expands six authored blocks into thirty-five artefacts — eight of them
// near-identical boilerplate. So the cost is the reason, not the rule. A
// sketch declares the three things that need a human (blocks, fields,
// where each value comes from and why) and this compiles the rest,
// emitting `schema.properties` and `grounding` from one traversal so the
// bijection between them is unrepresentable rather than merely checked.
pub mod contract_sketch;

// The third consumer of a schema verdict. The coordinator reads it per hop
// and `gate_trust` counts it in aggregate; neither accrues per agent, so
// "is this member getting better or worse" had no answer. Writes an
// `eval_signals` row per checked delegation — and deliberately none at all
// when nothing was checked, because there is no honest score for that.
pub mod schema_conformance;

// What each tool actually returns, so an author sourcing a block picks from
// fields that exist rather than typing a plausible key from memory — the same
// failure as the agent typing a plausible value, one level up. Declarations
// record whether the shape is constructed in this repo (verifiable) or a
// vendor passthrough (theirs, and it can change).
pub mod tool_response_shapes;

// A minimal JSON Schema validator. Seven keywords, no new dependency, and
// an unsupported keyword is NOT a pass — a validator that silently ignores
// what it cannot interpret returns `valid` for a document it never checked.
pub mod schema_validate;

// Agent economics — one definition of "how much has this agent run, and
// what did it cost", measured from `episodes` rather than from the
// denormalised counters on `agents` that no code path maintains.
pub mod agent_economics;

// Agent taxonomy (SPEC_30) — the derived ranks. Lives in the lib so both
// the API (classifying agents at creation) and the parity test can reach
// it; `scripts/taxonomy.py` remains the editorial tool for on-disk cards.
pub mod taxonomy;

// Pipeline planning (SPEC_31 P2) — validate a declared workflow_template's
// seams before executing it, so a pipeline that would break at stage 4 never
// spends stages 1–3.
pub mod pipeline;

// Episode construction — one constructor, reachable from both sides. The
// HTTP handlers in the api-server binary and the in-library delegation
// tools (agent_backend::tools_legacy) must build episodes through the same
// function; while it lived in the binary the lib had to keep a duplicate,
// and duplicates silently diverge on cost basis, provider attribution and
// failure provenance.
pub mod calibration;
pub mod episodes;
pub mod intentions;
pub mod plan_solicitation;

// Where BayesOps observations come from. The `Feed` trait and its contract
// types live in `crates/posterior`; the implementations live here because they
// read Postgres and that crate is transport-neutral. Before this existed the
// intake was a single `if feeds_from.source == "upstream_resolutions"` plus an
// undeclared side door that read `workspace_outputs.observations` for every
// parameter whether or not one was bound — which is why Loop 5.B could only
// ever learn from other Fermi forecast workspaces, and why a fit could draw on
// data nobody had pointed it at. See docs/specs/35_BAYESOPS_PLATFORM_LAYER.md.
pub mod feeds;

// A2A provider — pure mapping and task-building logic (no async, no AppState).
// Handler glue lives in api_server's handlers::a2a.
// Design: docs/DESIGN_a2a_provider.md
pub mod a2a_card;
pub mod a2a_task;

// Re-export main types
pub use ast::*;
pub use evaluator::{evaluate, EvalError, EvaluationContext};
pub use executor::{ExecutionError, ExecutionResults, Executor};
pub use lexer::{Lexer, LexerError, Token, TokenType};
pub use parser::{ParseError, Parser};
pub use report::generate_report;
pub use semantic::{SemanticAnalysis, SemanticAnalyzer, SemanticError};
pub use symbol_table::{Symbol, SymbolTable, SymbolTableBuilder, SymbolType};
pub use types::{BinaryOp, Type, TypeEnvironment, UnaryOp};

/// Convenience function to execute a program with default settings
pub fn execute_program(program: &Program) -> Result<ExecutionResults, ExecutionError> {
    let mut executor = Executor::new(10000);
    executor.execute(program)
}
