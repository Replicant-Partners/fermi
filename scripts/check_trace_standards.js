#!/usr/bin/env node
//
// # The artifact trace's two standards, rendered in a real browser
//
// A user opened one `football_analyst` trace and reported it as *"a bunch of
// rejection"* about an artifact that was **deliverable**. They were right about
// the page and wrong about the artifact, which means the page was wrong.
//
// The strip rendered four different kinds of finding in one red:
//
// | what the page said | what it actually was |
// |---|---|
// | header badge `VIOLATIONS` | a **fault** — the model asserted what it could not know |
// | `1 of 8 owed`, tone hardcoded `bad` | a **shortfall** — commissioned work not delivered |
// | `11 ◌` in the fault colour | mostly **capability gaps** and **compliance** |
// | `records only` | a **ceiling**, not a finding at all |
//
// So the one event that should alarm a reader was the *smallest number on the
// page*, drowned by two larger numbers that both mean "the agent did not do
// everything it hoped to".
//
// The properties below are what the fix has to preserve, and **none of them is
// visible to a unit test**: `field_state` proves in Rust that a shortfall and a
// fault are different findings with different tones, and that guarantee is worth
// nothing if the page then paints them the same. What a reader is shown is a
// fact about the DOM.
//
// Run by `tests/trace_standards.rs`, or by hand:
//
//     node scripts/check_trace_standards.js
//
// Exits non-zero with a written reason for each failure.

const http = require("http");
const fs = require("fs");
const path = require("path");
const { launch, connect, openPage } = require("./cdp.js");

const ROOT = path.join(__dirname, "..");
const FAIL = [];
let DIAG = "(not captured)";
const ok = (cond, what) => {
  if (!cond) FAIL.push(what);
};

// ── The fixture: the pulse that produced this work ───────────────────────
//
// One field in each of the five run states, so every tone is exercised on one
// page. Deliberately built so the shortfall and capability-gap counts are
// LARGER than the fault count — that asymmetry is the whole defect, and a
// fixture with one of each would not reproduce it.
const FIELDS = [
  // The fault. One field, and it must be the only red thing on the page.
  st("ratings.elo_current", "stripped", "fault", "red", 2, "the agent's", 1834),
  // The shortfall. Amber, never red.
  st("match_statistics.xg", "owed", "shortfall", "amber", 1, "the agent's", null,
     { settleable_by: "call_football_api", tool_runnable: true, evidence_bytes: 16036 }),
  st("match_statistics.shots", "owed", "shortfall", "amber", 1, "the agent's", null,
     { settleable_by: "call_football_api", tool_runnable: true, evidence_bytes: 16036 }),
  // Capability gaps. Neutral: the tool was asked and the world had nothing.
  st("injuries.current", "tool_empty", "capability_gap", "neutral", 0, "the world's", null,
     { settleable_by: "call_football_api", tool_runnable: true, evidence_bytes: 16036 }),
  st("injuries.returning", "tool_empty", "capability_gap", "neutral", 0, "the world's", null,
     { settleable_by: "call_football_api", tool_runnable: true, evidence_bytes: 16036 }),
  st("injuries.doubtful", "tool_empty", "capability_gap", "neutral", 0, "the world's", null,
     { settleable_by: "call_football_api", tool_runnable: true, evidence_bytes: 16036 }),
  // Compliance. The contract requires these to be empty.
  st("squad_value.arsenal_total", "absent_by_contract", "compliance", "neutral", 0, "nobody's", null,
     { kind: "unsourced", absence_expected: true }),
  st("squad_value.chelsea_total", "absent_by_contract", "compliance", "neutral", 0, "nobody's", null,
     { kind: "unsourced", absence_expected: true }),
  // Delivered.
  st("league_context.season", "filled", "delivered", "neutral", 0, "nobody's", "2024-25"),
];

function st(name, observed, finding, tone, rank, whose, value, extra) {
  return Object.assign(
    {
      name,
      // The contract clock. Deliberately NOT correlated with `observed`: a
      // `pending` field can be absent by contract and a `resolved` one can be
      // owed, and a fixture that paired them would hide exactly the case the
      // two-clock split exists for.
      declared: observed === "absent_by_contract" ? "pending" : "resolved",
      value: value === undefined ? null : value,
      grade: observed === "filled" ? "tool_verified" : "unavailable_no_tool_source",
      strength: observed === "filled" ? 2 : 0,
      settleable_by: null,
      produced: value !== null && value !== undefined,
      not_checkable: value === null ? "the field is null, so no assertion can be built" : null,
      kind: "sourced",
      absence_expected: false,
      settleable: true,
      tool_runnable: false,
      response_hint: null,
      probe_endpoint: null,
      observed,
      whose,
      finding,
      finding_tone: tone,
      finding_rank: rank,
    },
    extra || {},
  );
}

// The legend, served. Sentences long enough to be real, because the page
// asserts nothing about their length and a reader does.
const FIELD_STATES = [
  gl("filled", "a value came back", "nobody's", "delivered", "neutral"),
  gl("absent_by_contract", "empty, and the contract requires that. This is the contract working rather than a gap",
     "nobody's", "compliance", "neutral"),
  gl("tool_empty", "the named tool was asked and had nothing. A capability gap in the world, and nobody's fault",
     "the world's", "capability_gap", "neutral"),
  gl("owed", "empty and owed - a named tool was never called, or commissioned work did not come back",
     "the agent's", "shortfall", "amber"),
  gl("stripped", "the model wrote a value the contract forbids and the platform removed it before the response left",
     "the agent's", "fault", "red"),
];

function gl(token, why, whose, finding, finding_tone) {
  return {
    token,
    why,
    whose,
    finding,
    finding_tone,
    finding_why: `why ${finding} carries the ${finding_tone} tone, said once`,
  };
}

// The contract clock's legend. No tone: `pending` is a standing request for an
// integration and the opposite of a defect, and `error` is the only contract
// state that is one.
const DECLARED_STATES = [
  dg("resolved", "a tool is named and the platform can run it", false),
  dg("error", "the contract names a tool the platform cannot dispatch, so nothing can ever settle this field", true),
  dg("pending", "no tool exists for this yet. The field must be null and a value here is the violation - a standing request for an integration, not a defect", false),
  dg("derived", "computed by the platform from other fields, so it is reproducible by construction", false),
  dg("inferred", "a judgement the agent is commissioned to make. An endorsement is the terminal verdict, not a weak citation", false),
  dg("narrative", "prose. There is no proposition to settle", false),
];

function dg(token, why, is_defect) {
  return { token, why, is_defect };
}

const TRACE = {
  episode_id: "11111111-2222-3333-4444-555555555555",
  parent_episode_id: null,
  agent: { id: "a1", name: "football_analyst" },
  model: { model_used: "m", provider_used: "p", persona_version_at_write: 1 },
  at: "2026-09-01T10:00:00Z",
  input: { query: "how will arsenal do" },
  hashes: { algorithm: "sha256", input: "sha256:a", output: "sha256:b",
            output_grounded: "sha256:c", enforcement_changed_the_bytes: true },
  substrate: { disposition: "legible", legibility: { legibility: "full", present: [], missing: [] },
               declared: ["ports", "field_contract"], because: "declared" },
  checkpoint_route: { assumed: "agent.execute", recoverable: false, because: "no route column" },
  // `grounding` is `amend` on the execute route: it cannot refuse the run,
  // and the ungrounded value does not reach the caller. `records only` was
  // true of the code and false of the consequence.
  checkpoints: [
    { rung: "credit", clock: "invocation", enforcement: "control", why_not_control: null,
      refuses: "an action whose principal cannot pay", site: "gas::charge_gas",
      decided_absent: { token: "fires_before_artifact", because: "decides whether to run at all" } },
    { rung: "attachment", clock: "invocation", enforcement: "control", why_not_control: null,
      refuses: "a request whose attachments are absent", site: "attachments",
      decided: { decision: "approved", reason: null, at: "2026-09-01T10:00:00Z", decision_id: 1 } },
    { rung: "grounding", clock: "invocation", enforcement: "amend",
      why_not_control: "a field's grounding is unknowable until the model has written it, so there is no moment at which refusing the run is available",
      refuses: "a claim the contract cannot support", site: "grounding_trust::enforce",
      decided: { decision: "refused", reason: "1 ungrounded field", at: "2026-09-01T10:00:00Z", decision_id: 2 },
      recomputed: { fields: 9, violations: 1 } },
  ],
  fields: FIELDS,
  field_states: FIELD_STATES,
  declared_states: DECLARED_STATES,
  floor: "unavailable_no_tool_source",
  floor_strength: 0,
  // Two owed, three the world could not supply, two excused. The shortfall and
  // the capability gap both outnumber the single fault.
  completeness: {
    asked_for: 8,
    filled: 1,
    owed: [
      { path: "match_statistics.xg", why: "tool_never_called", tool: "call_football_api" },
      { path: "match_statistics.shots", why: "tool_never_called", tool: "call_football_api" },
    ],
    no_data: ["injuries.current", "injuries.returning", "injuries.doubtful"],
    excused: 2,
  },
  routed: [],
  // The tool WAS called and returned little. `completeness.no_data` says so,
  // and a fixture whose run record contradicted its own assessment would be
  // asserting against a state the platform cannot produce.
  //
  // Worth knowing: the row's `state` column is still derived on the client from
  // this list, so with it empty the row read `tool unused` beside a served
  // `tool_empty` chip — the client copy contradicting the server on the same
  // row. That is the residual duplication §4.2 leaves open, and it showed up
  // here first.
  tool_calls: [
    { tool: "call_football_api", output_chars: 16036, replayable: true,
      input: { endpoint: "injuries" } },
  ],
  reading: "fault",
  token: "violations",
  silence: { silence: "unresolved" },
  owner: "platform",
  caveats: [],
  response: {
    text: '{"ratings":{"elo_current":null},"league_context":{"season":"2024-25"}}',
    chars: 68,
    document: {
      ratings: { elo_current: null },
      match_statistics: { xg: null, shots: null },
      injuries: { current: null, returning: null, doubtful: null },
      squad_value: { arsenal_total: null, chelsea_total: null },
      league_context: { season: "2024-25" },
    },
  },
};

const API = (p) => {
  if (p.includes("/trace")) return TRACE;
  if (p === "/api/auth/me") return { user: null };
  if (p === "/api/notifications") return { notifications: [] };
  return {};
};

const MIME = { ".html": "text/html; charset=utf-8", ".css": "text/css",
               ".js": "text/javascript", ".svg": "image/svg+xml", ".png": "image/png",
               ".ico": "image/x-icon", ".woff2": "font/woff2" };

function serve() {
  const server = http.createServer((req, res) => {
    const p = new URL(req.url, "http://127.0.0.1").pathname;
    if (p.startsWith("/api/")) {
      const body = JSON.stringify(API(p));
      res.writeHead(200, { "content-type": "application/json",
                           "content-length": Buffer.byteLength(body) });
      return res.end(body);
    }
    let file = null;
    if (p.startsWith("/static/")) file = path.join(ROOT, p.slice(1));
    else if (p.startsWith("/trace/")) file = path.join(ROOT, "templates/trace.html");
    if (!file || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      res.writeHead(404, { "content-type": "text/plain" });
      return res.end("not found: " + p);
    }
    const buf = fs.readFileSync(file);
    res.writeHead(200, { "content-type": MIME[path.extname(file)] || "application/octet-stream",
                         "content-length": buf.length });
    res.end(buf);
  });
  return new Promise((r) =>
    server.listen(0, "127.0.0.1", () => r({ server, port: server.address().port })));
}

async function main() {
  const { server, port } = await serve();
  let chrome, cdp;
  try {
    chrome = await launch();
    cdp = await connect(chrome.wsUrl);
    const page = await openPage(cdp);
    await page.goto(`http://127.0.0.1:${port}/trace/${TRACE.episode_id}`);

    ok(page.exceptions.length === 0,
      "the page threw during load, so everything after the throw rendered nothing:\n      " +
        page.exceptions.join("\n      "));
    ok(page.consoleErrors.length === 0,
      "console error(s):\n      " + page.consoleErrors.join("\n      "));

    const seen = await page.eval(() => {
      const txt = (el) => (el ? el.textContent.replace(/\s+/g, " ").trim() : null);
      const rows = Array.from(document.querySelectorAll(".std")).map((s) => ({
        name: txt(s.querySelector(".std-n")),
        cells: Array.from(s.querySelectorAll(".q5c")).map((c) => ({
          label: txt(c.querySelector(".q5c-l")),
          value: txt(c.querySelector(".q5c-v")),
          tone: (Array.from(c.classList).find((k) => k.startsWith("t-")) || "").slice(2),
        })),
      }));
      const dist = document.querySelector(".dist");
      return {
        rows,
        caption: txt(document.querySelector(".std-cap")),
        // The header badge, and the row it declares itself a summary of.
        badge: (() => {
          const b = document.querySelector(".head .badge");
          if (!b) return null;
          const of = b.querySelector(".badge-of");
          return { text: txt(b), of: txt(of), title: b.getAttribute("title") || "" };
        })(),
        dist: dist
          ? { value: txt(dist.querySelector(".dist-v")),
              classes: Array.from(dist.classList) }
          : null,
        // The field-state legend, and whether any ancestor of it is folded.
        // The run-state legend only. The contract clock has its own band and
        // carries no tone, so folding the two into one list would make the
        // "exactly one red" assertion below meaningless.
        legend: Array.from(document.querySelectorAll(".obs-leg"))
          .filter((b) => /this run/i.test(txt(b.querySelector(".leg-k"))))
          .flatMap((b) => Array.from(b.querySelectorAll(".lgi")))
          .filter((l) => l.querySelector(".obs"))
          .map((l) => ({
            token: txt(l.querySelector(".obs")),
            tone: (Array.from(l.querySelector(".obs").classList)
              .find((k) => k.startsWith("f-")) || "").slice(2),
            text: txt(l),
          })),
        // And the contract clock's band, asserted separately.
        declaredLegend: Array.from(document.querySelectorAll(".obs-leg"))
          .filter((b) => /the contract/i.test(txt(b.querySelector(".leg-k"))))
          .flatMap((b) => Array.from(b.querySelectorAll(".lgi")))
          .map((l) => txt(l.querySelector(".dec"))),
        legendFolded: !!(document.querySelector(".obs-leg") &&
          document.querySelector(".obs-leg").closest("details")),
        // Every run-state chip on a field row, with the colour it wears and
        // the state word the row's own `state` column prints beside it.
        chips: Array.from(document.querySelectorAll(".arow"))
          .filter((r) => r.querySelector(".obs"))
          .map((r) => ({
            token: txt(r.querySelector(".obs")),
            tone: (Array.from(r.querySelector(".obs").classList)
              .find((k) => k.startsWith("f-")) || "").slice(2),
            field: txt(r.querySelector(".a-k")),
            // The contract clock, and whether it is marked as the one defect.
            declared: txt(r.querySelector(".dec")),
            declaredDefect: !!(r.querySelector(".dec")
              && r.querySelector(".dec").classList.contains("d-defect")),
            // Every word in the state cell, so an invented one is visible.
            words: Array.from(r.querySelectorAll(".a-c > span")).map(txt),
            fact: txt(r.querySelector(".fact")),
          })),
        // Rendered text only. `document.body.textContent` includes the inline
        // <script>, so a word appearing in a source COMMENT would satisfy a
        // check about what a reader sees — which is a check that cannot fail.
        bodyText: Array.from(document.querySelectorAll("#content, #content *"))
          .filter((el) => el.tagName !== "SCRIPT")
          .map((el) => Array.from(el.childNodes)
            .filter((n) => n.nodeType === 3)
            .map((n) => n.textContent).join(" "))
          .join(" ").replace(/\s+/g, " "),
        // Only read when something failed, and only to say what the page did
        // instead. An empty strip and a page that never rendered look identical
        // from every assertion above.
        diagnostic: (document.getElementById("content") || {}).innerHTML
          ? document.getElementById("content").innerHTML.slice(0, 500)
          : "#content is empty or absent",
        // Anything at all wearing the fault colour among the strip cells.
        redCells: Array.from(document.querySelectorAll(".q5c.t-bad"))
          .map((c) => txt(c.querySelector(".q5c-l"))),
        // The full answers, separately. `why_not_control` renders in two places
        // — the gate chip's caveat and the enforcement question's own answer —
        // so a page-wide search for it passes when either survives, and a check
        // that cannot distinguish them cannot fail for the case that matters.
        answers: Array.from(document.querySelectorAll(".q5-a")).map(txt),
      };
    });

    DIAG = seen.diagnostic;

    // 1. Two standards, named, three cells each. Legality and ambition are
    //    different questions and the reader has both.
    ok(seen.rows.length === 2,
      `${seen.rows.length} standard row(s) rendered, expected 2. The split IS the fix: ` +
        `five questions under one colour ramp is what made a designer-grade ` +
        `disappointment read as a caller-grade defect.`);
    ok(seen.rows[0] && /fit to send/i.test(seen.rows[0].name),
      `row A is named "${seen.rows[0] && seen.rows[0].name}", not "Fit to send". ` +
        `The caller's standard has to be named as such or the reader cannot tell ` +
        `which standard they are reading.`);
    ok(seen.rows[1] && /as designed/i.test(seen.rows[1].name),
      `row B is named "${seen.rows[1] && seen.rows[1].name}", not "As designed"`);
    seen.rows.forEach((r, i) =>
      ok(r.cells.length === 3, `row ${i === 0 ? "A" : "B"} has ${r.cells.length} cells, expected 3`));

    // 2. **THE property.** Exactly one thing on this strip is red, and it is
    //    the fault. Two owed fields and three capability gaps outnumber it in
    //    the fixture, exactly as they did on the real pulse.
    ok(seen.redCells.length === 1,
      `${seen.redCells.length} cells wear the fault colour: ${JSON.stringify(seen.redCells)}. ` +
        `Exactly one question here can be a fault — did it assert something it ` +
        `could not know. A shortfall is amber and a capability gap is neutral; ` +
        `painting them red is what made 2 omissions and 3 world-gaps drown the ` +
        `1 fabrication.`);
    ok(seen.redCells.length === 1 && /unsourced|assert|clean/i.test(seen.redCells[0]),
      `the red cell is "${seen.redCells[0]}", which is not the claims question. ` +
        `Red belongs to the one question whose bad answer means the model ` +
        `asserted what it could not have known.`);

    // 3. The shortfall row is amber and present. Amber, not green: it IS a
    //    finding, just not a fault.
    const didWork = seen.rows[1] && seen.rows[1].cells[0];
    ok(didWork && didWork.tone === "warn",
      `"did the work" rendered tone "${didWork && didWork.tone}", expected "warn". ` +
        `It hardcoded \`e.empty === 0 ? "ok" : "bad"\` — separating owed, no_data ` +
        `and excused carefully in the prose one line above, then painting a ` +
        `shortfall with the fault colour anyway.`);
    ok(didWork && /2 of 8 owed/.test(didWork.value),
      `"did the work" reads "${didWork && didWork.value}", expected the owed count`);

    // 4. The distribution carries NO tone. It is a description; a description
    //    cannot attribute anything, and `weakSourced > 0 ? "bad"` was a verdict
    //    smuggled into one.
    ok(seen.dist, "the distribution cell is gone; where the numbers came from is still worth saying");
    ok(seen.dist && !seen.dist.classes.some((c) => /^t-(ok|warn|bad)$/.test(c)),
      `the distribution cell wears ${JSON.stringify(seen.dist && seen.dist.classes)}. It is a ` +
        `description of where the numbers came from and must not render a verdict: ` +
        `strength 0 is CORRECT for a field the contract says nothing can source.`);

    // 5. The caption is derived from the finding counts and carries the frame
    //    the specimen page already had.
    // The fixture's grounding rung is `amend` and one field was stripped, which
    // is the real football_analyst case. So the artifact IS deliverable: the
    // value was nulled and the answer was delivered whole. `Decision::Refused`
    // is the ledger's word for "this gate acted", and reading it as "the
    // platform declined" is the confusion this whole change is about.
    ok(seen.caption && /deliverable/i.test(seen.caption),
      `the row B caption does not say the artifact is deliverable: "${seen.caption}". ` +
        `One field was stripped and the answer was delivered whole — that is what ` +
        `\`amend\` means. A stripped field is a fault the agent must answer for AND ` +
        `an artifact the caller can use, and the page has to be able to say both.`);
    ok(seen.caption && /stripped/i.test(seen.caption),
      `the caption does not say the forbidden value was stripped: "${seen.caption}". ` +
        `Without it, "deliverable" reads as though nothing was wrong.`);
    ok(seen.caption && /2 value\(s\) the agent owed/.test(seen.caption),
      `the caption does not name the owed count from the served findings: "${seen.caption}"`);
    ok(seen.caption && /ambition/i.test(seen.caption),
      `the caption omits the pruning sentence. "Pruning the contract to reach green ` +
        `would delete the ambition it exists to record" is the frame; without it a ` +
        `reader's remedy for an amber row is to delete the contract entry.`);

    // 5b. **The badge names the row it summarises.** (Decision 3b.)
    //
    // `artifact_trace::reading()` returns `Fault` on `violations > 0` and on
    // nothing else, so the badge has always been reporting the single
    // fabrication-shaped event on the page. Unlabelled it read as a verdict on
    // the artifact entire: it said `1`, question 3 said `1`, they were
    // different `1`s, and eleven omissions sat beside them in the same colour
    // family.
    //
    // Kept rather than dropped, and labelled \u2014 a header with no verdict is its
    // own kind of unreadable, and the label is what makes keeping it honest.
    ok(seen.badge, "the header badge is gone. It was to be kept and labelled, not dropped.");
    ok(seen.badge && /fit to send/i.test(seen.badge.of || ""),
      `the badge does not name the standard it summarises (found ` +
        `${JSON.stringify(seen.badge && seen.badge.of)}). It reports one row of ` +
        `"fit to send" and is silent about "as designed"; a reader who cannot see ` +
        `that reads it as a verdict on the artifact entire.`);
    ok(seen.badge && /says nothing about/i.test(seen.badge.title || ""),
      "the badge carries no explanation of what it does NOT cover. That silence " +
        "is the whole defect: the one number that should alarm a reader was the " +
        "smallest on the page and nothing said which question it answered.");

    // 6. `records only` is gone, and `amend` says what it does.
    ok(!/records only/i.test(seen.bodyText),
      "`records only` is still on the page. It collapsed four declared enforcement " +
        "modes into two, so `amend` — the mode that actually removes the bad part — " +
        "read as impotent, and `report` was indistinguishable from `metric`.");
    ok(/removes the bad part/i.test(seen.bodyText),
      "the `amend` mode does not say it removes the bad part. `command_registry` " +
        "declares four modes and the page received all four; rendering consequence " +
        "rather than bookkeeping is the point.");
    // Asserted on the enforcement question's OWN answer, not on the page.
    //
    // The sentence also reaches the reader through the gate chip's caveat, so a
    // page-wide search stays green when it is deleted from the answer that
    // needs it — which is exactly what happened the first time this was
    // mutated. The question "could anything have acted on what it found" is
    // where the reason belongs, because the reason is the answer.
    ok(seen.answers.some((t) => /unknowable until the model has written it/i.test(t || "")),
      "`why_not_control` does not reach the enforcement question's answer. It is " +
        "MANDATORY on every non-control application — " +
        "`a_gate_that_cannot_refuse_explains_itself` panics without it, because " +
        "\"a gate demoted to a metric is a decision somebody made, and the reason " +
        "is what tells a later reader whether it was deliberate or drift\" — and " +
        "the page received the sentence and threw it away, then printed a word " +
        "that sounded like the platform shrugging.\n      answers seen: " +
        JSON.stringify(seen.answers.map((t) => (t || "").slice(0, 60))));

    // 7. The run-state legend is served, present, and NOT folded. A token whose
    //    gloss is folded has no gloss.
    ok(seen.legend.length >= 4,
      `the run-state legend has ${seen.legend.length} entries; the shown rows print ` +
        `five distinct states`);
    ok(!seen.legendFolded,
      "the run-state legend is inside a <details>. That honours `explain once` in " +
        "the letter and breaks it in effect: the token is unfolded, so the sentence " +
        "must be too.");
    const legRed = seen.legend.filter((l) => l.tone === "red").map((l) => l.token);
    ok(legRed.length === 1 && /stripped/.test(legRed[0]),
      `the legend paints ${JSON.stringify(legRed)} red. Only \`stripped\` earns it.`);
    ok(seen.legend[0] && seen.legend[0].tone === "red",
      `the legend leads with "${seen.legend[0] && seen.legend[0].token}". Worst first: ` +
        `a fault below four states that are nobody's fault is a fault a reader ` +
        `meets last, which is the ordering that buried it.`);

    // 8. Every field row prints its SERVED run state, and none of them is a
    //    word this page invented.
    ok(seen.chips.length > 0,
      "no field row prints a run state. `field_state::Observed` is served per " +
        "field and nothing rendered it, which was the whole task.");
    const known = new Set(FIELD_STATES.map((g) => g.token.replace(/_/g, " ")));
    const unknown = seen.chips.filter((c) => !known.has(c.token));
    ok(unknown.length === 0,
      `these rows print a state the platform did not serve: ${JSON.stringify(unknown)}. ` +
        `A word invented on the client is the fourth inline copy of the five-way ` +
        `match, and the copy that cannot see the run record.`);
    const chipRed = seen.chips.filter((c) => c.tone === "red");
    ok(chipRed.length === 1,
      `${chipRed.length} field rows wear the fault colour: ${JSON.stringify(chipRed)}. ` +
        `One field was stripped; the rest are omissions, compliance and gaps in the ` +
        `world. "Omissions not hallucinations" is the distinction.`);

    // 9. `unsourced` must not be a run state again. It is a DECLARED kind on
    //    the specimen page — a standing request, not a defect — and this page
    //    used it to mean a violation.
    ok(!seen.chips.some((c) => c.token === "unsourced"),
      "a field row prints `unsourced` as a run state. That is the original " +
        "collision: a declared kind on one panel and a violation on the next, one " +
        "word, two opposite meanings, two panels a reader sees together.");

    // 10. **Every word on every row comes from a served vocabulary.**
    //
    // This replaces a check that compared the served state against the row's
    // own condition word. There is no longer a second producer to disagree
    // with — `annotate` prints what the platform serves and derives nothing —
    // so that comparison can no longer fail, and a check that cannot fail
    // reports coverage it does not have.
    //
    // What can still go wrong is someone spelling a state here again. The old
    // page had nine words of its own — `platform-computed`, `prose`,
    // `a judgement`, `no tool exists`, `asked, empty`, `no data there`,
    // `never asked`, `tool unused`, `agent wrote nothing` — every one of them a
    // state the platform already owned, under a different name. That is what
    // this fails on.
    const VOCAB = new Set(
      FIELD_STATES.map((g) => g.token.replace(/_/g, " "))
        .concat(DECLARED_STATES.map((g) => g.token.replace(/_/g, " ")))
        // The verification log's own words. A different question from either
        // clock — has anybody judged this claim — and this page's to know,
        // because `CLAIMS` is a fold over an append-only log that changes when
        // a reader settles something.
        .concat(["awaiting a verdict", "nothing queued it", "settled",
                 "endorsed", "cited", "rejected", "tool-checked", "derived"]),
    );
    const invented = [];
    seen.chips.forEach((c) =>
      (c.words || []).forEach((w) => {
        if (w && !VOCAB.has(w)) invented.push({ field: c.field, word: w });
      }));
    ok(invented.length === 0,
      `these rows print a word from no served vocabulary: ${JSON.stringify(invented)}. ` +
        `The page had nine such words and every one was a state the platform ` +
        `already owned under a different name \u2014 which is how one word came to ` +
        `mean a declared kind on one panel and a violation on the next.`);

    // 11. Both clocks are on the row, and they are different vocabularies.
    ok(seen.chips.every((c) => c.declared),
      `${seen.chips.filter((c) => !c.declared).length} row(s) print no contract ` +
        `state. Two clocks: what the contract says this field can be trusted ` +
        `about, and what happened to it on this pulse. Dropping the first is how ` +
        `the trace came to spell its own words for it.`);
    const dTok = new Set(seen.chips.map((c) => c.declared));
    const oTok = new Set(seen.chips.map((c) => c.token));
    const shared = [...dTok].filter((t) => oTok.has(t));
    ok(shared.length === 0,
      `these words appear as BOTH a contract state and a run state: ` +
        `${JSON.stringify(shared)}. The two vocabularies are disjoint by ` +
        `construction \u2014 \`the_two_vocabularies_share_no_token\` is a build ` +
        `failure \u2014 and a reader who learns a word must learn one thing.`);

    // 12. The contract clock carries no tone. `pending` is a standing request
    //     for an integration and the opposite of a defect; `error` is the only
    //     contract state that is one.
    const wrongly = seen.chips.filter((c) => c.declaredDefect && c.declared !== "error");
    ok(wrongly.length === 0,
      `these rows mark a contract state as a defect that is not one: ` +
        `${JSON.stringify(wrongly.map((c) => c.field + " \u2192 " + c.declared))}. ` +
        `Only \`error\` \u2014 a contract naming a tool the platform cannot dispatch \u2014 ` +
        `is a defect here. Colouring \`pending\` is how an agent's ambition came ` +
        `to be reported as its failure.`);

    // 13. The byte count is served and rendered. A fact, not a verdict: 210
    //     bytes for a beetle with no sequenced genome and 16,036 for an
    //     injuries call are both "asked, and still empty", and the number is
    //     the whole difference between a capability gap and a discarded result.
    const withFact = seen.chips.filter((c) => c.fact);
    ok(withFact.length > 0,
      "no row carries the byte count. It is served as `evidence_bytes`, " +
        "computed over the WHOLE run record rather than the `.take(40)` list the " +
        "page receives \u2014 which reported the forty-first call as never having " +
        "happened.");
    ok(withFact.some((c) => /16,036/.test(c.fact)),
      `the served byte count is not rendered verbatim: ` +
        `${JSON.stringify(withFact.map((c) => c.fact))}`);

  } finally {
    try { if (cdp) cdp.close(); } catch (_) {}
    try { if (chrome) await chrome.close(); } catch (_) {}
    server.close();
  }

  if (FAIL.length) {
    console.error(`\n${FAIL.length} failure(s):\n`);
    FAIL.forEach((f) => console.error("  \u2716 " + f + "\n"));
    console.error("what the page rendered instead, first 500 chars:\n");
    console.error("  " + DIAG + "\n");
    process.exit(1);
  }
  console.log("OK  two standards render, and exactly one thing on the page is red.");
}

main().catch((e) => {
  console.error("harness error: " + (e && e.stack ? e.stack : e));
  process.exit(2);
});
