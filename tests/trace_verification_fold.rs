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

/// Every row emits every cell.
///
/// The grid is four columns and a row that renders three gets its key
/// auto-placed into the 26px pips track. Every uncontracted key then wrapped one
/// character per line — `man`/`_ci`/`ty_`/`tot`/`al` down the page — which is
/// what shipped, because the pips cell was emitted as `""` rather than as an
/// empty element.
///
/// Checked structurally rather than by counting, because the counting version
/// lives in a node harness and this file may not assume node exists.
#[test]
fn every_row_emits_every_cell() {
    let src = trace();
    let at = src
        .find("function arow(")
        .expect("`arow` is gone; the answer's rows are built somewhere unknown");
    let body: String = src[at..].chars().take(1200).collect();
    for cell in [
        "class=\"a-p\"",
        "class=\"a-k",
        "class=\"a-v ",
        "class=\"a-d\"",
    ] {
        assert!(
            body.contains(cell),
            "`arow` no longer emits `{cell}`. A four-column grid given three \
             children auto-places the key into the 26px pips track, and every \
             key wraps one character per line. An empty cell must still be an \
             element."
        );
    }
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

    let missing: Vec<&String> = called
        .iter()
        .filter(|n| !src.contains(&format!("function {n}(")))
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
    let branch: String = src[at..].chars().take(1400).collect();
    assert!(
        branch.contains("toolControl("),
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
    assert!(
        code_lines(&src).any(|l| l.contains("kind === \"inferred\"")),
        "the legend counts judgements by `!settleable`, which also catches \
         `derived` and `narrative` — so the count reintroduces, in prose, the \
         collapse the rows just stopped making."
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
