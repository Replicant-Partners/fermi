//! The trace reads the verification log as a log, and offers one settle UI.
//!
//! # The two defects this holds shut
//!
//! `assertion_verifications` is append-only, and migration 205 says what that
//! means in the table comment itself:
//!
//! > current state is the latest row per `assertion_id`, **derived rather than
//! > stored**, so a rejected-then-reverified assertion reads as exactly that
//! > instead of as "verified".
//!
//! `/api/episodes/:id/trace` serves `routed[]` as that whole log, ordered
//! `created_at DESC`. The client filtered it flat. Measured on episode
//! `386a6248-8663-417b-8b0d-82b277a4afb1` — the one run where the curation loop
//! closed end to end, so the reference example for every surface:
//!
//! | assertion | rows | latest |
//! |---|---|---|
//! | `assessment` | 3 | `human_endorsed` |
//! | `squad_value` | 2 | `human_endorsed` |
//!
//! Five rows, two claims, **both settled**. The page rendered five rows,
//! announced "2 of 5 still awaits a verdict", and offered a settle form on each
//! of the two `pending_human_check` rows underneath the endorsements that had
//! already settled them. Three of the five rows carried no `evidence.path` —
//! settlements are written without one — so they rendered as the literal word
//! `claim` followed by the reviewer's own UUID, naming nothing.
//!
//! The count could never fall, either, because settling a claim **adds** a row.
//! The same `held` figure drove the artifact's `held for review` reading and the
//! loops block, so a closed loop was indistinguishable from a stalled one.
//!
//! Second defect, same screen. Two settle UIs existed 120 lines apart, and one
//! hand-copied its verdicts:
//!
//! | | verdicts | source |
//! |---|---|---|
//! | the claim grid | `Cite it` · `Wrong` | hardcoded, 2 of 3 |
//! | "Routed for verification" | `Sourced` · `Endorse` · `Reject` | served |
//!
//! `Cite it` **is** `Sourced` is `human_sourced`. `Wrong` **is** `Reject` is
//! `rejected`. `human_endorsed` was reachable from one block and not the other,
//! for no reason other than the copy being short — which is exactly what
//! `settleForm`'s own comment warns about, written 120 lines from where it
//! happened:
//!
//! > `settleable_verdicts` is SERVED, never hardcoded here: copying it would be
//! > copying a declaration, and inventing a parallel list is how the two drift.
//!
//! # Why a source scan
//!
//! Both failures render. Neither errors. A flat read of an append-only log
//! produces a plausible screen with confident wrong numbers on it, and a second
//! button that posts the same verdict under a different word is indistinguishable
//! from a feature. There is nothing to catch at runtime, which is the
//! standing-clock problem from §4.1 of
//! `docs/papers/verification_for_agent_ecologies.md`.

use std::fs;

const TRACE: &str = "templates/trace.html";

fn trace() -> String {
    fs::read_to_string(TRACE).unwrap_or_else(|e| panic!("cannot read {TRACE}: {e}"))
}

/// Lines that are code rather than prose.
///
/// The file discusses both defects at length by name — it has to, since the
/// reason the fold exists is the reason it is written down — and a scan that
/// could not tell a sentence from a statement would force the explanation out
/// of the codebase to stay green.
fn code_lines(body: &str) -> impl Iterator<Item = &str> {
    body.lines()
        .map(str::trim_start)
        .filter(|l| !l.starts_with("//") && !l.starts_with("*") && !l.starts_with("/*"))
}

/// `routed[]` is a log. Current state is one entry per assertion, derived.
#[test]
fn the_log_is_never_filtered_flat_for_pending_rows() {
    let src = trace();
    let flat: Vec<&str> = code_lines(&src)
        .filter(|l| l.contains("routed") && l.contains("pending_"))
        .collect();

    assert!(
        flat.is_empty(),
        "the trace filters the verification log directly for pending rows:\n\n  {}\n\n\
         `routed[]` is the whole append-only history. A claim queued once and \
         settled twice keeps its `pending_` row forever, so this reports claims \
         as awaiting a verdict that a person has already settled — and the \
         figure cannot fall, because settling ADDS a row. Fold the log to one \
         entry per `assertion_id` first (the newest row is the state, the rest \
         are history) and filter that.",
        flat.join("\n  ")
    );
}

/// The fold has to actually be there, or the test above passes on a page that
/// simply stopped reporting.
#[test]
fn the_log_is_folded_to_one_entry_per_assertion() {
    let src = trace();
    for needle in [
        // the fold itself
        "CLAIMS[id] = { earlier: 0 }",
        // the path recovered from the queue row, since a settlement carries none
        "if (ev.path && !c.path) c.path = ev.path",
        // and the join the claim grid settles through
        "ASSERTION_BY_PATH[p] = id",
    ] {
        assert!(
            src.contains(needle),
            "the trace no longer folds the verification log — `{needle}` is gone. \
             Without the fold, `held` and the claim rows are counting log rows \
             rather than claims, and the numbers are confidently wrong rather \
             than missing."
        );
    }
}

/// Zero claims awaiting a verdict has two opposite causes.
///
/// Nothing was ever queued, or everything queued was settled. The loops block
/// showed neither, so the one run where curation closed read exactly like a run
/// where it never started — which is the reading that would have hidden the
/// whole point of the screen.
#[test]
fn a_settled_queue_is_distinguishable_from_an_empty_one() {
    let src = trace();
    assert!(
        code_lines(&src).any(|l| l.contains("const settled = Object.keys(CLAIMS)")),
        "the trace counts held claims and not settled ones, so an artifact whose \
         claims were all settled reports the same emptiness as one that was \
         never queued. Absent must look different from done."
    );
}

/// One act, one place, one list — and the list is served.
#[test]
fn there_is_exactly_one_settle_ui_and_it_reads_its_verdicts_from_the_platform() {
    let src = trace();

    // A verdict written as a literal into markup is a copied declaration.
    let hardcoded: Vec<&str> = code_lines(&src)
        .filter(|l| l.contains("data-verdict=\""))
        .filter(|l| !l.contains("${esc(v)}"))
        .collect();
    assert!(
        hardcoded.is_empty(),
        "a verdict is hardcoded into the trace's markup:\n\n  {}\n\n\
         The settleable verdicts are served by `/api/verification-queue`. A copy \
         here drifts from them, and the last copy was short by exactly one \
         verdict — `human_endorsed` was unreachable from the claim grid for no \
         reason but that.",
        hardcoded.join("\n  ")
    );

    // The button labels have one definition.
    assert_eq!(
        src.matches("const label = {").count(),
        1,
        "two label maps means two vocabularies for one act. `Cite it` and \
         `Sourced` were the same verdict under different words, on one screen."
    );

    // And there is one form producing them.
    assert_eq!(
        src.matches("function settleForm(").count(),
        1,
        "more than one settle form. Every settlement posts to the same endpoint \
         against the same `assertion_id`; a second form is a second rendering of \
         rows the page already shows."
    );
}

/// The question no checkpoint answers must keep reading as a hole.
///
/// The five questions each name the gate that answers them. Question three —
/// *did it actually do the work?* — names none, and that is the finding rather
/// than a gap in the page: `grounding` asks whether a tool **could** have
/// supplied a value, never whether the agent **did** produce one, so an empty
/// field inherits its block's grade and reads as sourced. On the reference
/// episode, `squad_value` grades against blocks marked `tool_verified` with two
/// of four values null.
///
/// The failure mode is somebody tidying it: assigning `grounding` to question
/// three because every other row has a gate and the blank looks unfinished.
/// That would delete the only place the platform admits the check does not
/// exist, and nothing would go red.
#[test]
fn the_question_with_no_gate_still_has_no_gate() {
    let src = trace();

    assert!(
        src.contains("g-none"),
        "the trace no longer renders a `no gate` token. One of the five questions \
         is answered by nothing in the system, and that absence is the finding \
         — it must be visible as a hole rather than as a blank cell."
    );

    // The empty gate list belongs to the work question and to no other.
    let q3 = src
        .find("Did it actually do the work?")
        .expect("the work question is gone; it is the only one computed from the values");
    let next = src[q3..]
        .find("Where did the numbers come from?")
        .map(|i| q3 + i)
        .expect("the provenance question must follow the work question");
    assert!(
        src[q3..next].contains("[]"),
        "`Did it actually do the work?` has been given a gate. Nothing in the \
         system checks whether a contracted field was filled in, so naming a \
         checkpoint here claims a check that does not run. If one is built, \
         delete this assertion in the same commit that builds it."
    );

    // Absent, declared-elsewhere and unrecorded are three different findings.
    for token in ["g-none", "g-off", "not recorded", "not on this route"] {
        assert!(
            src.contains(token),
            "`{token}` is gone. A question with no gate, a gate not declared on \
             this route, and a declared gate whose ledger row did not survive are \
             three different situations with three different remedies, and \
             collapsing any two of them blames the wrong party."
        );
    }
}

/// A gate that looked and could not act says so beside the question.
///
/// `records only` is the token that changes what an answer means, and it lived
/// two scrolls down in the ladder — so the five questions could report that a
/// checkpoint had read the contents while omitting that whatever it found could
/// not have stopped the response. On the reference episode `grounding` is
/// exactly that: it **refused**, and it refuses nothing.
#[test]
fn a_gate_says_whether_it_could_refuse() {
    let src = trace();
    for token in ["can refuse", "records only"] {
        assert!(
            src.contains(token),
            "`{token}` is gone from the five questions. A gate that inspects and \
             cannot refuse reads as protection it does not provide."
        );
    }
}

/// The document is rendered once, annotated, not twice.
///
/// It was two blocks. "The payload" held the whole answer with no checks and no
/// actions; "What it claimed" held the checks and the actions for 13 of 47
/// values, regrouped by strength so the document's own shape was gone. Each was
/// half a screen and the reader did the join by eye.
///
/// That was the third instance of one-object-two-renderings on this page, after
/// the two settle UIs and the flat verification log — which is why it is worth an
/// assertion rather than a note. The rule it violates is already written down in
/// `handlers/pages.rs`, about the bestiary: **a lens changes columns and sort,
/// not the page.**
#[test]
fn the_document_is_rendered_once_with_its_checks_on_it() {
    let src = trace();

    for gone in ["function payload(", "function fields("] {
        assert!(
            !src.contains(gone),
            "`{gone}` is back. The answer and its checks were two blocks and the \
             reader had to reconcile them; they are one annotated document now. \
             If a second view is genuinely wanted, make it a lens over the same \
             rows rather than a second rendering of the same object."
        );
    }

    for needed in ["function annotate(", "function flattenDoc(", "const LENSES"] {
        assert!(
            src.contains(needed),
            "`{needed}` is gone, so the merged view has been dismantled without \
             the two old blocks coming back — which leaves the page with neither."
        );
    }

    // A hole is only legible in the shape it is missing from.
    //
    // Asserted on the split form specifically, which is the stronger one. The
    // first version compared the whole remaining path, so a contract on
    // `phylogeny.superorder` was emitted only when `phylogeny` itself existed —
    // and a field whose PARENT the agent omitted disappeared entirely, which is
    // the most actionable row vanishing because more of it is missing.
    assert!(
        src.contains("!present.has(at.split(\".\")[0])"),
        "contracted fields the document does not contain are no longer injected \
         into the tree, or are injected only at the top level. Five on the \
         reference artifact — `fixtures`, `head_to_head`, `injuries`, \
         `match_statistics`, `summary` — are the most actionable rows on the page \
         and are invisible to a walk of the document alone."
    );
}

/// A value nothing examined must not wear a grade, and must not wear a label.
///
/// Two halves, and the second one reversed once it was on a screen.
///
/// `pips` floors a missing strength to `0`, which draws ▱▱ — the same glyphs as
/// `tool_no_match`. So an uncontracted value would claim to have been graded and
/// found worthless when nothing looked at it, and that is **125 of 135 rows** on
/// the reference document: almost the whole answer mislabelled.
///
/// The first attempt then labelled each of those rows `not under contract`, which
/// is a true sentence printed a hundred and twenty-five times — and unreadable
/// for exactly that reason. The rule it breaks is the page's own: explain once,
/// then show data. So an unexamined row now says **nothing**, and the count is
/// stated once in the lens strip, where it is also the control that shows them.
#[test]
fn a_value_under_no_contract_shows_no_grade_and_no_label() {
    let src = trace();
    assert!(
        src.contains("const gradePips"),
        "`gradePips` is gone. Calling `pips` directly on a value with no \
         contracted field renders ▱▱, which is a grade, and absent must look \
         different from bad."
    );
    // `code_lines`, because the comments explain the label and why it went —
    // they have to name it to do that, and a scan that could not tell prose from
    // markup would force the explanation out of the file.
    assert!(
        !code_lines(&src).any(|l| l.contains("not under contract")),
        "unexamined rows are labelled again. That is one true sentence repeated \
         125 times on the reference artifact, which is how the wall of noise came \
         back. The count belongs in the lens strip — once — where it is also the \
         control that shows them."
    );
    assert!(
        src.contains("\"unexamined\", \"nothing examined\""),
        "the `unexamined` lens is gone, so the population that is silent on the \
         rows is now silent everywhere — and silence about 125 of 135 values \
         reads as approval."
    );
}

/// Every row emits every cell, in one order: **value · condition · act**.
///
/// Two properties, and the second is the layout's whole premise.
///
/// *Every* cell, because a five-column grid given four children auto-places the
/// key into the 26px pips track and every uncontracted key then wraps one
/// character per line — `man`/`_ci`/`ty_`/`tot`/`al` down the page, which is what
/// shipped, because the pips cell was emitted as `""` rather than as an element.
///
/// In *that order*, because a reader scans the right-hand edge. One column used
/// to hold a state token on some rows and a control on others — `never asked`
/// beside `call_football_api ▸`, meaning different kinds of thing — and on the
/// absent rows, the ones with something to press, the control was a line further
/// down inside the prose. Learn one row, read a hundred; that only works if the
/// hundred agree.
///
/// Checked structurally rather than by counting, because the counting version
/// lives in a node harness and this file may not assume node exists.
#[test]
fn every_row_emits_every_cell_in_one_order() {
    let src = trace();
    let at = src
        .find("function arow(")
        .expect("`arow` is gone; the answer's rows are built somewhere unknown");
    let body: String = src[at..].chars().take(1400).collect();

    let mut seen = Vec::new();
    for cell in ["a-p", "a-k", "a-v", "a-c", "a-a"] {
        let needle = format!("class=\"{cell}");
        let pos = body.find(&needle).unwrap_or_else(|| {
            panic!(
                "`arow` no longer emits `{cell}`. A five-column grid given four \
                 children auto-places the key into the 26px pips track, and every \
                 key wraps one character per line. An empty cell must still be an \
                 element."
            )
        });
        seen.push((pos, cell));
    }
    let mut sorted = seen.clone();
    sorted.sort();
    assert_eq!(
        sorted.iter().map(|(_, c)| *c).collect::<Vec<_>>(),
        ["a-p", "a-k", "a-v", "a-c", "a-a"],
        "the row's cells are emitted in a different order. The grammar is pips, \
         field, value, condition, act — positionally fixed, because a column \
         whose meaning depends on the row cannot be scanned."
    );

    // The grid must have a track per cell, or the last one wraps under the row.
    let css = src
        .find(".arow{display:grid;")
        .map(|i| src[i..].chars().take(200).collect::<String>())
        .expect("`.arow` is no longer a grid");
    assert!(
        css.matches("px").count() >= 3 && css.contains("1fr"),
        "`.arow`'s track list no longer looks like five columns: {css}"
    );
}

/// A citation has to be followable, because that is what it is scored for.
///
/// `human_sourced` scores as high as a tool check for exactly one reason, stated
/// in migration 205: *someone else can follow the citation to the same source.*
/// It was rendered as plain text, so the surface that displayed the score did not
/// satisfy the score's own justification.
#[test]
fn a_cited_source_can_be_followed() {
    let src = trace();
    assert!(
        src.contains("follow the citation") && src.contains("rel=\"noopener noreferrer\""),
        "a citation is printed rather than linked. `human_sourced` is worth two \
         pips because a third party can check it; a surface that shows the score \
         and not the path to the source is asserting the one thing it withholds."
    );
}

/// The document's key order survives the API.
///
/// `serde_json` without `preserve_order` is a `BTreeMap`, so every document is
/// alphabetised the moment it is parsed. `football_analyst` writes
/// `league_context, squad_value, …, assessment` — context, then evidence, then
/// conclusion — and the trace rendered `advanced_metrics, assessment, …`. The raw
/// text and the parsed document were then in different orders and could not be
/// read against each other, which was the concrete reason reconciling the two
/// halves of that screen was impossible.
///
/// Order is information. Stripping it silently is the same class of loss as
/// stripping a value, and this platform's premise is that neither is acceptable.
///
/// The other half of the trade is guarded in `artifact_hash`:
/// `the_document_hash_ignores_key_order` requires identity to stay
/// order-independent, which `of_document` now implements rather than inheriting
/// from a default. **Display preserves order, identity ignores it** — and neither
/// property may be decided by which features a dependency happens to enable.
#[test]
fn the_document_keeps_the_order_the_agent_wrote_it_in() {
    let toml = fs::read_to_string("Cargo.toml").expect("Cargo.toml");
    let line = toml
        .lines()
        .find(|l| l.trim_start().starts_with("serde_json"))
        .expect("serde_json is a direct dependency and its line is gone");
    assert!(
        line.contains("preserve_order"),
        "`preserve_order` has been dropped from serde_json. Every JSON document \
         the platform parses is now alphabetised, the trace shows the agent's \
         answer in an order the agent did not write, and the raw text below it \
         cannot be read against it. Nothing fails; the screen just becomes \
         unreconcilable again."
    );
}

/// Every block the page assembles must exist.
///
/// A commit replaced a source range identified by its two comment anchors and
/// took `ladder()` with it, because `ladder()` happened to sit between them.
/// `render` then threw `ladder is not defined` and the trace rendered nothing but
/// its own error handler.
///
/// Nothing caught it. `node --check` is a **syntax** check and a call to an
/// undefined function is well-formed JavaScript; the harness that verified the
/// new code called the new function directly and never called `render`, so it
/// exercised the part and not the whole.
///
/// This is the cheap half of the remedy and needs no JavaScript engine: read the
/// expression `render` assembles the page from, and require every block it names
/// to be defined in the same file. Anchors describe the boundary of a range and
/// not its contents, and a range big enough to be worth scripting is big enough
/// to contain something nobody was thinking about.
#[test]
fn every_block_the_page_is_assembled_from_is_defined() {
    let src = trace();

    // The chain is the argument to `innerHTML =` inside `render`.
    let at = src
        .find("function render(d) {")
        .expect("`render` is gone, which is a larger problem than this test");
    let body = &src[at..];
    let chain_at = body
        .find("innerHTML =")
        .expect("`render` no longer assigns the page; nothing draws the trace");
    // Up to the statement's end.
    let chain: String = body[chain_at..].chars().take_while(|c| *c != ';').collect();

    // Every `name(d)` in the chain names a block builder.
    let mut called: Vec<String> = Vec::new();
    for part in chain.split('+') {
        let p = part.trim();
        if let Some(open) = p.find('(') {
            let name = &p[..open];
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                called.push(name.to_string());
            }
        }
    }
    assert!(
        called.len() >= 6,
        "the render chain parsed to only {called:?}, so this test is reading the \
         wrong thing and would pass whatever the page did"
    );

    // A block builder is a `function`, and it may also be a local `const` arrow —
    // `expert` wraps the folded views and is bound inside `render` itself. Both
    // are definitions; only an undefined name is fatal.
    let missing: Vec<&String> = called
        .iter()
        .filter(|n| {
            !src.contains(&format!("function {n}("))
                && !src.contains(&format!("const {n} = ("))
                && !src.contains(&format!("const {n} = "))
        })
        .collect();
    assert!(
        missing.is_empty(),
        "`render` assembles the page from blocks that do not exist: {missing:?}. \
         The page will throw and show nothing at all. This is exactly what \
         deleting a source range by its anchors did to `ladder`."
    );

    // The two wiring passes run after every render and are just as fatal.
    for f in ["wireSettle", "wireClamps", "render"] {
        assert!(
            src.contains(&format!("function {f}(")),
            "`{f}` is called after the page is drawn and is not defined. Every \
             control on the trace stops working, silently, with the page still \
             rendering."
        );
    }
}

/// A named remedy the platform can perform must be a control.
///
/// The rule the whole screen kept failing: **if the platform can name what would
/// close a gap, the name is the control.** `call_football_api` was printed beside
/// eleven rows and ran from none of them, which makes it a description.
///
/// Both halves matter. A tool that cannot be run from a read-only surface — five
/// of the sixteen the contracts name need a workspace, a memory store or
/// credentials of their own — must stay a label, because a button the endpoint
/// refuses is worse than no button: the refusal arrives after the click, by which
/// point the reader has concluded the platform could do it.
#[test]
fn a_runnable_tool_is_a_control_and_an_unrunnable_one_is_not() {
    let src = trace();

    assert!(
        src.contains("function toolControl("),
        "the tool a contract names is printed rather than offered. That is the \
         defect this screen was reported for: descriptions where affordances \
         belong."
    );
    assert!(
        src.contains("f.tool_runnable"),
        "`tool_runnable` is no longer consulted, so every named tool gets a \
         button — including the five that need a workspace or credentials and \
         will be refused by the endpoint after the click."
    );
    assert!(
        src.contains("data-probe="),
        "nothing on the page can run a tool. `/api/episodes/:id/probe` exists and \
         is unreachable, which is the same gap one layer down."
    );

    // The rows it matters most on are the empty ones, and that branch returned
    // before reaching the control the first time it was written.
    let at = src
        .find("if (f.produced === false)")
        .expect("the not-produced branch is gone");
    // Generous, because this branch grew when one absence became three, and the
    // control it must offer now sits after all three states are decided. A window
    // tight enough to feel precise is a window that fails on unrelated edits —
    // this one already did, at 1400 and again at 3200.
    let branch: String = src[at..].chars().take(5000).collect();
    assert!(
        branch.contains("actCell("),
        "a field the agent never produced offers no way to ask whether the tool \
         could have supplied it. That is the only question available about an \
         absence, and answering it splits one state into two with different \
         owners: the agent had the means and did not use them, or the \
         integration genuinely has nothing."
    );
}

/// Running a tool must not be presented as settling anything.
///
/// The probe retrieves; a person compares. The contract does not say where in a
/// response the value lives — `response_field` is prose as often as a path — so a
/// surface that rendered a successful call as a verdict would be making exactly
/// the claim the endpoint refuses to make, which is string-matching dressed as
/// verification.
#[test]
fn a_tool_run_is_not_rendered_as_a_verdict() {
    let src = trace();
    assert!(
        src.contains("This decides nothing"),
        "the probe's result no longer says it settles nothing. `ok` means the \
         tool answered, not that the claim is true, and a reader who reads a \
         green result as a verdict has been told the wrong thing by the surface \
         rather than by the endpoint."
    );
}

/// An absence the contract requires must not read as a fault.
///
/// `Grounding` has five variants and they answer the question every surface
/// asks — who, if anyone, can settle this. They were collapsed one line into
/// `graded_fields`:
///
/// ```ignore
/// settleable_by: match c.grounding {
///     Grounding::Sourced { tool, .. } => Some(tool),
///     _ => None,
/// },
/// ```
///
/// so `Unsourced`, `Inferred`, `Derived` and `Narrative` all arrived as `None`
/// and rendered as *needs a person* — including the three that are nobody's
/// work.
///
/// The visible cost: `squad_value` is `Unsourced`, meaning the contract says no
/// tool returns market values **so the field must be null**. Its two absent
/// totals were badged `not produced`, in the same yellow as
/// `advanced_metrics.xg`, which is `Sourced`, also null, and a real finding.
/// Compliance and failure wearing one badge, across 31 of the platform's 108
/// contracted fields.
#[test]
fn a_required_absence_is_not_rendered_as_a_failure() {
    let src = trace();
    assert!(
        src.contains("absence_expected"),
        "the trace no longer distinguishes an absence the contract REQUIRES from \
         one the agent owes. `unsourced` fields must be null, and reporting that \
         as `not produced` tells a reader to go and fix an agent that did exactly \
         what it was told."
    );
    assert!(
        src.contains("as declared"),
        "a required absence has lost its own reading. It is not a fault and it is \
         not a pass — it is a standing request for an integration that does not \
         exist."
    );
}

/// Unsettleable is three different things, and they imply different acts.
///
/// `inferred` is a judgement the agent was commissioned to make — an endorsement
/// is the TERMINAL verdict there, not a weak substitute for a citation, and
/// `assessment`'s own contract says "no database holds them". `derived` is
/// platform code applying a transform, reproducible by construction, whose
/// disagreements are our bug. `narrative` is prose.
///
/// Branching on a single `!settleable` boolean labelled a platform-computed
/// field "a judgement", which is the same collapse one level down — which is
/// why this is asserted rather than left to review.
#[test]
fn the_three_unsettleable_kinds_are_not_one_state() {
    let src = trace();
    for token in ["a judgement", "platform-computed", "prose"] {
        assert!(
            src.contains(token),
            "`{token}` is gone. `inferred`, `derived` and `narrative` are all \
             unsettleable and are not the same finding: one is the agent's \
             product, one is our arithmetic, one is prose. A reader told only \
             that nothing can settle a field cannot tell which of the three \
             they are looking at."
        );
    }
    // Three tokens, three entries. The legend is keyed by the token the row
    // prints, so one shared entry for all three would be the collapse arriving
    // in the explanation instead of in the row — which is where it arrived last
    // time, as a count of "judgements" that included our own arithmetic.
    let at = src
        .find("const CONDITION_WHY = {")
        .expect("the legend no longer explains states by their token");
    let table: String = src[at..].chars().take(4000).collect();
    for token in ["a judgement", "platform-computed", "prose"] {
        assert!(
            table.contains(&format!("\"{token}\":")),
            "the legend has no entry for `{token}`, so rows in that state are \
             explained by somebody else's sentence or by none at all."
        );
    }
}

/// The summary contextualises. It does not dominate.
///
/// The five questions were five stacked rows of prose above the thing the reader
/// came for — a question, a sentence, a line of gate tokens, five times, half a
/// screen. Everything in it was true and all of it was in the way. They are one
/// strip now: a label, one token that is the answer, and the checkpoint that
/// decides it.
///
/// Two things must NOT move behind a fold, and they are the reason this is a test
/// rather than a preference:
///
/// * **`no gate`.** Question three is the finding this page ratcheted — nothing
///   in the system checks whether a contracted field was filled in. A finding
///   behind a fold is a finding nobody reads.
/// * **Each fold's own headline.** A summary reading "the loops" hides a count;
///   a summary reading "1 claim awaiting a verdict" is the reason to open it.
///   Closing a fold may cost detail and must never cost a finding.
#[test]
fn the_summary_is_a_strip_and_the_expert_views_carry_their_headlines() {
    let src = trace();

    let at = src
        .find("function questions(d) {")
        .expect("the five questions are gone");
    let block: String = src[at..].chars().take(6000).collect();

    assert!(
        block.contains("q5strip") && block.contains("q5c-v"),
        "the five questions are not a strip. Stacked, they are half a screen of \
         prose above the document the reader opened the page for."
    );
    assert!(
        block.contains("gateNames("),
        "the strip no longer names the checkpoint that answers each question. A \
         question with no gate beside it is an opinion."
    );

    // The finding is above the fold, and the long answers are in it.
    let fold = block
        .find("<details class=\"expert\">")
        .expect("the long answers are not folded, so the summary still dominates");
    let nogate = block.find("q5-nogate").expect(
        "the `no gate` paragraph is gone; question three's absence is the \
                 finding this page ratcheted",
    );
    assert!(
        nogate < fold,
        "`no gate` moved inside the fold. It is the one answer on this page that \
         no checkpoint stands behind, and a finding nobody opens is a finding \
         nobody reads."
    );

    // The ladder and the loops are below the work and folded, and every summary
    // is computed rather than written.
    let at = src
        .find("function render(d) {")
        .expect("`render` is gone, which is a larger problem than this test");
    // Generous: `render` carries the head template, the fold summaries and the
    // assembly chain, and a window tight enough to feel precise is a window that
    // fails on a comment. It already did, at 3000, four characters short of the
    // line it was looking for.
    let body: String = src[at..].chars().take(6000).collect();
    for (name, what) in [("loopsFed(d)", "the loops"), ("ladder(d)", "the ladder")] {
        assert!(
            body.contains("expert(") && body.contains(name),
            "{what} is no longer part of the folded expert view. It is true, it is \
             not what a reader came for, and unfolded it pushes the document down \
             the page."
        );
    }
    // The chain is on the page and the loops are under it.
    //
    // A pulse nothing consumed fed nothing, and a pulse a teammate picked up is
    // the whole point of a composition — so where an artifact came from and where
    // it landed outranks a claim about learning over months. Both were in one
    // fold, loops first.
    let flow = body.find("collaboration(d)").expect(
        "the chain is no longer drawn. Who called this pulse and where its \
                 output landed is the half that says whether it was consumed at all",
    );
    let loops = body.find("loopsFed(d)").expect("the loops block is gone");
    assert!(
        flow < loops,
        "the loops are drawn above the chain again. What happened to the artifact \
         comes before what it might teach over months."
    );
    assert!(
        src.contains("function flowStrip("),
        "the chain is prose again. It is the one thing on this page that is \
         genuinely a picture — who called it, this pulse, who it called, where it \
         landed — and it was three stacked sentences two folds down."
    );

    for head in ["loopsHead", "ladderHead"] {
        let at = body
            .find(&format!("const {head} = "))
            .unwrap_or_else(|| panic!("`{head}` is gone, so a fold has no summary"));
        let line: String = body[at..].chars().take(400).collect();
        assert!(
            line.contains("${"),
            "`{head}` is a fixed string. A fold's summary has to carry the count \
             it is hiding, or closing it hides a finding rather than a detail."
        );
    }
}

/// One grade, three findings, three owners.
///
/// `tool_no_match` reads as "the tool answered and had nothing", and
/// `grounding_trust` is explicit that it is a **proxy** — *"Content present ~ tool
/// returned data"*. It is inferred from the field being empty, so it cannot tell
/// a tool that had nothing from a tool nobody called.
///
/// Measured on the two reference runs, which carry the same grade and opposite
/// findings:
///
/// | agent | grade | record | finding |
/// |---|---|---|---|
/// | `genome_profiler` | `tool_no_match` | called `ncbi_genome_search`, 210 bytes back | no sequenced genome exists. Correct behaviour. |
/// | `football_analyst` | `tool_no_match` | never called `fixtures/statistics`, where xG lives | it never asked. |
///
/// The contract file predicted this and could not test it: *"trusting an agent's
/// self-report about its own tool's capabilities is the identical error to
/// trusting its self-report about a genome size."* `tool_calls` is what makes it
/// testable, and collapsing the two back into one badge is how the screen came
/// to accuse the corrected canonical agent of failing.
#[test]
fn an_absence_says_whether_the_tool_was_ever_asked() {
    let src = trace();

    assert!(
        src.contains("function askedFor("),
        "the trace no longer asks whether the tool was consulted, so it is back to \
         trusting the grade — which is a proxy computed from the field being \
         empty, and therefore cannot distinguish a source with no data from an \
         agent that never looked."
    );

    for token in ["never asked", "tool unused"] {
        assert!(
            src.contains(token),
            "`{token}` is gone. An agent that declared a tool, used it, and never \
             asked it for a contracted field is the one genuinely accusatory case \
             here, and it reads identically to a beetle with no sequenced genome \
             without it."
        );
    }

    // The number, not a threshold. 210 bytes and 16,036 bytes are both "asked and
    // empty", and which of them is a discarded result is a judgement running the
    // tool settles — so the count is reported and both readings are named.
    assert!(
        src.contains("got.toLocaleString()"),
        "the size of what the tool returned is no longer shown. Without it, a \
         capability gap and a discarded 16KB response are the same row."
    );
}

/// The contradiction act files a finding; it does not change the agent.
///
/// This is the end of the trail the screen exists for: the grade said the tool
/// had nothing, the record showed it was never asked, a run showed the data is
/// there. Loop 2 is where that becomes a correction the agent retrieves.
///
/// It must stop at the queue. Intervening runs `InterventionEncoder`, the
/// `CoherenceGate`, second-reviewer consensus for agent-wide scope, and
/// `TwoWriteMemory`'s audit trail. **Once verification output is training
/// input, those four are the only things between a misclick and a rule the
/// agent will believe** — so a surface that wrote a correction directly, or an
/// endpoint that intervened on its own, would be the fastest way to teach an
/// agent something false.
#[test]
fn contradicting_an_agent_files_an_anomaly_and_stops_there() {
    let src = trace();
    let handler = fs::read_to_string("src/handlers/loops.rs").expect("loops.rs");

    assert!(
        src.contains("data-wrong="),
        "the trace can prove an agent under-sourced a field and cannot say so. \
         That is the act the whole screen was building toward."
    );

    // Evidence, both ends. The client sends what the tool returned and the
    // endpoint refuses the call without it — the same reasoning as migration
    // 205's citation CHECK, pointed at an agent instead of a claim.
    assert!(
        src.contains("evidence"),
        "the client no longer sends the tool's response as evidence, so a \
         contradiction arrives at the review queue indistinguishable from an \
         opinion."
    );
    assert!(
        handler.contains("`evidence` is required"),
        "the endpoint accepts a contradiction with no evidence. A one-click \
         `the agent is wrong` costs the agent a correction, so it has to cost \
         the reviewer a tool run."
    );

    // Files an anomaly, and nothing more.
    let at = handler
        .find("pub async fn contradict_field_handler")
        .expect("the contradiction handler is gone");
    let body: String = handler[at..].chars().take(6000).collect();
    assert!(
        body.contains("KIND_CONTRADICTED") && body.contains("create_anomaly_event"),
        "the contradiction no longer files an anomaly, so it no longer reaches \
         the HITL queue and Loop 2 has no input from this screen."
    );
    for forbidden in [
        "InterventionEncoder",
        "TwoWriteMemory",
        "bump_persona_version",
    ] {
        assert!(
            !body.contains(forbidden),
            "the contradiction handler reaches `{forbidden}` directly, skipping \
             the review queue, the coherence gate and the consensus rule. The \
             point of filing an anomaly is that a human and a gate stand between \
             this click and the agent's world model."
        );
    }
}

/// Each field proposes its own call, and the answer says whether it is in there.
///
/// Two faults, reported together and with one cause: the contract's
/// `response_field` was treated as an opaque string.
///
/// It is not. It has a grammar, consistent across every contract:
///
/// ```text
/// standings (rank, points, form, home/away splits)   endpoint + key names
/// fixtures/headtohead                                endpoint only
/// fixtures/statistics.expected_goals                 endpoint + one leaf
/// ```
///
/// The old test for "is this a path" rejected anything with a space or a
/// bracket — which is most of them — so the query loaded empty, a reader reached
/// for whichever replay chip was nearest, and `teams/statistics` got run to
/// answer a question about `fixtures`. **Two different fields, the same call, the
/// same 16KB response**, and nothing on the page saying so.
///
/// And then a 16,036-byte blob does not answer "is my field in there". The
/// contract lists the names; locating them turns the blob into a finding.
/// Reported as a name search, because that is what it is: *`expected_goals`
/// appears at this path in this response* is a fact, and *the tool can supply
/// this field* is the inference a person draws from it.
///
/// Three later corrections, each from the surface being used:
///
/// 1. The search ran on the **truncated** copy, so a large payload could report
///    NOT FOUND for a name in the part that did not travel. It now runs
///    server-side over the whole body.
/// 2. It searched keys only. API-Football returns fixture statistics as
///    `{type, value}` pairs, so xG arrives as a **value** — the one field this
///    screen was built to settle would have been reported absent.
/// 3. `match_statistics` and `advanced_metrics.xg` are both contracted to
///    `fixtures/statistics`, so they legitimately return the identical payload.
///    Correct, and it reads as a bug unless the page says so.
#[test]
fn a_probe_asks_this_fields_question_and_says_if_the_answer_is_there() {
    let src = trace();

    assert!(
        src.contains("function hintEndpoint(") && !src.contains("function parseHint("),
        "the client parses `response_field` itself again. There are now two \
         parsers of the same prose — the probe endpoint checks the query it \
         receives against ITS parse — so a disagreement has the platform \
         reporting the reader's own prefill as the wrong endpoint."
    );
    assert!(
        src.contains("function sharedParams("),
        "the run's subject is no longer recovered from the record. `league: 39` \
         and `season: 2024` are on every call the agent made and `team` varies, \
         so the intersection is what the run was ABOUT — the half the contract \
         cannot carry."
    );

    // The prefill must be the field's own call, never a replayed one.
    assert!(
        src.contains("endpoint: ep,"),
        "the loaded query no longer comes from this field's contract. A replay \
         chip is a call the agent happened to make; it answers its own question, \
         not this row's."
    );
    assert!(
        code_lines(&src).any(|l| l.contains("other calls this run made")),
        "the replay chips are unlabelled again. Unlabelled, they read as answers \
         to this row's question, and running the wrong one produces a confident \
         irrelevant response."
    );

    // And the response has to be read for us.
    for token in ["FOUND:", "NOT FOUND"] {
        assert!(
            src.contains(token),
            "`{token}` is gone, so a 16,000-byte response is handed over with no \
             statement about whether the field is in it — which is the complaint \
             this answers."
        );
    }

    // The search is the server's, over the untruncated body.
    let probe = std::fs::read_to_string("src/field_probe.rs").expect("src/field_probe.rs");
    assert!(
        probe.contains("pub fn search(") && probe.contains("search(&body, &target.keys)"),
        "the name search is no longer performed where the whole response exists. \
         Run on the truncated copy it reports NOT FOUND for names in the part \
         that did not travel: a false negative on a trust surface, produced by a \
         display limit."
    );
    assert!(
        probe.contains("site: \"value\""),
        "the search looks at keys only again. API-Football returns fixture \
         statistics as `{{type, value}}` pairs, so `expected_goals` is a VALUE — \
         and a key-only search reports the one field this screen exists to \
         settle as absent while the number sits in the payload."
    );
    assert!(
        src.contains("Byte-identical"),
        "two fields that share an endpoint get the identical payload and the \
         page says nothing again. `match_statistics` and `advanced_metrics.xg` \
         are both `fixtures/statistics`: correct, and indistinguishable from a \
         bug unless it is stated."
    );
    assert!(
        src.contains("Not this field's endpoint"),
        "a probe run against an endpoint the contract does not name is no longer \
         called out. A replay chip is one press away and returns a sound answer \
         to a different question."
    );

    // A bug in this file must not wear a network failure's clothes.
    assert!(
        src.contains("failed to render it"),
        "every throw inside `runProbe` reads as `Could not reach the platform` \
         again. That is how `h is not defined` — a live tool run, answered \
         correctly — spent a deploy looking like an outage."
    );
}

/// The scan must be able to fail.
#[test]
fn the_scan_can_actually_fail() {
    let flat =
        r#"    const held = (d.routed || []).filter(r => r.verdict.startsWith("pending_"));"#;
    assert!(
        code_lines(flat).any(|l| l.contains("routed") && l.contains("pending_")),
        "the scan does not recognise the defect it was written for"
    );

    // The file's own prose names it repeatedly and must not count.
    let prose = "// `routed[]` is the whole log, so filtering it for pending_ double-counts";
    assert!(
        !code_lines(prose).any(|l| l.contains("routed") && l.contains("pending_")),
        "the scan counts a comment as code, which would force the explanation out \
         of the file to keep this test green"
    );

    // The folded read must NOT trip it — otherwise the only way to pass is to
    // stop reporting, which is worse than the bug.
    let folded = r#"    const held = Object.keys(CLAIMS).filter(id => pending(CLAIMS[id]));"#;
    assert!(
        !code_lines(folded).any(|l| l.contains("routed") && l.contains("pending_")),
        "the scan rejects the correct implementation, so the cheapest way to go \
         green is to delete the count"
    );

    let literal = r#"      <button class="act" data-verdict="human_sourced">Cite it</button>"#;
    assert!(
        code_lines(literal).any(|l| l.contains("data-verdict=\"") && !l.contains("${esc(v)}")),
        "the scan does not see a hardcoded verdict"
    );
    let served = r#"      `<button class="act" data-verdict="${esc(v)}" title="${esc(v)}">`"#;
    assert!(
        !code_lines(served).any(|l| l.contains("data-verdict=\"") && !l.contains("${esc(v)}")),
        "the scan rejects the served form, which is the only correct one"
    );
}
