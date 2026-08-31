// Render the trace client end to end, across every lens, and press every probe.
//
// `node --check` is a syntax check and nothing more. It has passed on this file's
// subject while `ladder()` was deleted (blank page in production) and while
// `h is not defined` made a working tool run report itself as a network outage:
// both are runtime failures inside a `try`, and both were visible only by
// running the thing.
//
// So this extracts the inline script out of `templates/trace.html`, stubs enough
// DOM to let `boot()` complete, and then:
//
//   1. renders once per lens, asserting the output is non-empty and that no
//      handler threw;
//   2. opens the probe form on every runnable field;
//   3. runs the probe against every shape of response the endpoint can return —
//      found as a key, found as a value, missing, unparseable, refused, wrong
//      endpoint, and a byte-identical repeat — asserting the verdict says the
//      right thing and never reports a bug in this file as a network failure.
//
// Usage: node scripts/check_trace_probe_render.js

const fs = require("fs");
const path = require("path");

const HTML = fs.readFileSync(
  path.join(__dirname, "..", "templates", "trace.html"),
  "utf8",
);

// The last <script> block is the page's own. Others are Nav and friends.
const blocks = [...HTML.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g)]
  .map((m) => m[1])
  .filter((s) => s.includes("function boot"));
if (blocks.length !== 1) {
  throw new Error(`expected exactly one bootable script block, found ${blocks.length}`);
}
const SCRIPT = blocks[0];

// ── the DOM, only as much as the page touches ─────────────────────────────
// Where markup was inserted, and into what.
//
// The stub used to swallow this: `insertAdjacentHTML` appended to a string and
// nobody asked which element it appended to. So when the probe form began being
// inserted after the button — which lives in a 132px grid cell, and `.probe` is
// `grid-column: 2 / -1`, which does nothing for a node that is not a grid child
// — the form, the replay chips, the query box and a 16KB response rendered one
// word wide down the right-hand edge, and every check here passed.
const INSERTS = [];
function makeEl(id) {
  const el = {
    id,
    html: "",
    text: "",
    className: "",
    dataset: {},
    children: [],
    set innerHTML(v) { this.html = String(v); },
    get innerHTML() { return this.html; },
    set textContent(v) { this.text = String(v); },
    get textContent() { return this.text; },
    addEventListener() {},
    appendChild(c) { this.children.push(c); return c; },
    insertAdjacentHTML(pos, v) {
      INSERTS.push({ into: this, pos, html: String(v) });
      this.html += String(v);
    },
    closest() { return null; },
    querySelector() { return null; },
    querySelectorAll() { return []; },
  };
  return el;
}
const CONTENT = makeEl("content");
globalThis.document = {
  getElementById: (id) => (id === "content" ? CONTENT : makeEl(id)),
  createElement: (tag) => makeEl(tag),
  querySelector: () => null,
  querySelectorAll: () => [],
  addEventListener() {},
};
globalThis.location = { pathname: "/trace/386a6248-8663-417b-8b0d-82b277a4afb1" };
globalThis.window = {};
globalThis.navigator = { clipboard: { writeText: () => Promise.resolve() } };

// ── the fixture ──────────────────────────────────────────────────────────
//
// `football_analyst` on the reference episode: xG contracted to
// `fixtures/statistics.expected_goals`, graded `tool_no_match`, and the record
// shows the agent called seven other endpoints and never that one.
const F = (o) => Object.assign(
  {
    grade: "unavailable_no_tool_source",
    strength: 0,
    value: null,
    settleable_by: null,
    produced: false,
    not_checkable: null,
    kind: "sourced",
    absence_expected: false,
    settleable: true,
    tool_runnable: false,
    response_hint: null,
    probe_endpoint: null,
  },
  o,
);
const FIELDS = [
  F({ name: "league_context", settleable_by: "call_football_api", tool_runnable: true,
      response_hint: "standings (rank, points, form, home/away splits)",
      probe_endpoint: "standings", grade: "tool_verified", strength: 2,
      value: { rank: 1 }, produced: true }),
  F({ name: "fixtures", settleable_by: "call_football_api", tool_runnable: true,
      response_hint: "fixtures (date, competition, venue, status)",
      probe_endpoint: "fixtures", grade: "tool_no_match" }),
  F({ name: "head_to_head", settleable_by: "call_football_api", tool_runnable: true,
      response_hint: "fixtures/headtohead", probe_endpoint: "fixtures/headtohead",
      grade: "tool_no_match" }),
  F({ name: "injuries", settleable_by: "call_football_api", tool_runnable: true,
      response_hint: "injuries (player, type, reason)", probe_endpoint: "injuries",
      grade: "tool_no_match" }),
  F({ name: "match_statistics", settleable_by: "call_football_api", tool_runnable: true,
      response_hint: "fixtures/statistics (shots, possession, passes, cards, saves)",
      probe_endpoint: "fixtures/statistics", grade: "tool_no_match" }),
  F({ name: "advanced_metrics.xg", settleable_by: "call_football_api", tool_runnable: true,
      response_hint: "fixtures/statistics.expected_goals",
      probe_endpoint: "fixtures/statistics", grade: "tool_no_match" }),
  // Named by a contract and NOT runnable from a read-only surface.
  F({ name: "summary", settleable_by: "scan_nearby_creatures", tool_runnable: false,
      response_hint: "sibling taxa at each rank", probe_endpoint: null }),
  // `unsourced`: absent is the contract being obeyed, not a fault.
  F({ name: "ratings.elo_current", kind: "unsourced", absence_expected: true,
      settleable: false, not_checkable: "no tool exists that could settle it" }),
  // `inferred`: nothing settles it, so an endorsement is terminal.
  F({ name: "assessment", kind: "inferred", settleable: false, produced: true,
      value: "Liverpool look strong", grade: "model_asserted" }),
];
const CALL = (endpoint, out, params) => ({
  tool: "call_football_api",
  input: { endpoint, params: params || { league: 39, season: 2024 } },
  output_chars: out,
  replayable: true,
  iteration: 1,
});
const TRACE = {
  agent: { id: "2a38f0a0-0000-0000-0000-000000000001", name: "football_analyst" },
  episode_id: "386a6248-8663-417b-8b0d-82b277a4afb1",
  at: "2026-08-19T13:48:35.013729+00:00",
  model: "claude-opus-4",
  owner: { name: "ilabra" },
  reading: "fault",
  floor: "tool_no_match",
  floor_strength: 0,
  fields: FIELDS,
  // Under `response`, which is where `answer()` reads it. It was at the top
  // level, so every lens rendered "This episode retained no answer" and the row
  // grammar this file claims to check was never drawn once.
  response: {
    document: {
      league_context: { rank: 1 },
      advanced_metrics: { xg: null, xgd: null },
      ratings: { elo_current: null },
      assessment: "Liverpool look strong",
      summary: null,
    },
    text: '{"league_context":{"rank":1},"assessment":"Liverpool look strong"}',
    chars: 9888,
  },
  tool_calls: [
    CALL("standings", 16034),
    CALL("teams/statistics", 6482, { team: 40, league: 39, season: 2024 }),
    CALL("teams/statistics", 6411, { team: 50, league: 39, season: 2024 }),
    CALL("players", 16034),
    CALL("players", 16031),
    CALL("injuries", 16036),
    CALL("players/topscorers", 16028),
  ],
  // The verification log, append-only and newest first — the shape `boot` folds.
  // Without these the `you can act` lens is empty and the settle form, the claim
  // state and the worklist's "waiting" branch are never drawn once.
  routed: [
    { assertion_id: "c1", verdict: "pending_human_check", actor: null,
      actor_kind: "system", at: "2026-08-19T13:49:00Z",
      evidence: { path: "league_context", claimed: { rank: 1 },
                  settleable_by: "call_football_api" } },
    { assertion_id: "c2", verdict: "human_endorsed", actor: "ilabra",
      actor_kind: "human", citation: null, at: "2026-08-19T14:02:00Z",
      evidence: { path: "assessment" } },
    { assertion_id: "c2", verdict: "pending_human_check", actor: null,
      actor_kind: "system", at: "2026-08-19T13:49:00Z",
      evidence: { path: "assessment", claimed: "Liverpool look strong" } },
  ],
  belt: [],
  belt_route: [],
  caveats: [],
  contract: { declared: [], undeclared: [] },
  hashes: {},
  input: {},
  silence: null,
  substrate: { disposition: "legible" },
  corpus_eligible: false,
  loops: [],
  truncated: false,
  response_text: "{}",
};

// ── the probe endpoint, in every shape it can answer ──────────────────────
const STATS_BODY = JSON.stringify({
  response: [{
    team: { id: 50 },
    statistics: [
      { type: "Shots on Goal", value: 7 },
      { type: "expected_goals", value: "1.23" },
    ],
  }],
});
function probeReply(kind) {
  const base = {
    tool: "call_football_api", ok: true, response: STATS_BODY, truncated: false,
    chars: STATS_BODY.length, hint: "fixtures/statistics.expected_goals",
    endpoint_expected: "fixtures/statistics", endpoint_called: "fixtures/statistics",
    endpoint_matches: true, searched: ["expected_goals"], not_searched: [],
    parsed: true, digest: "sha256:aaaa", decides: "nothing.",
    found: [{ key: "expected_goals", at: "$.response[0].statistics[1].type",
              site: "value", sample: '{"type":"expected_goals","value":"1.23"}' }],
    found_total: 1,
    missing: [],
  };
  switch (kind) {
    case "found_value": return base;
    // A capped list: 12 places shown of 30 that exist.
    case "found_key": return Object.assign({}, base, {
      found: Array.from({ length: 12 }, (_, i) =>
        ({ key: "rank", at: `$.response[${i}].rank`, site: "key", sample: String(i) })),
      found_total: 30,
      searched: ["rank", "points", "form"], not_searched: ["home/away splits"],
      digest: "sha256:bbbb",
    });
    case "missing": return Object.assign({}, base, {
      found: [], found_total: 0, missing: ["expected_goals"], digest: "sha256:cccc",
    });
    case "no_keys": return Object.assign({}, base, {
      searched: [], found: [], found_total: 0, missing: [], digest: "sha256:dddd",
    });
    case "unparseable": return Object.assign({}, base, {
      response: "<html>rate limited</html>", chars: 25, parsed: false,
      found: [], missing: ["expected_goals"], digest: "sha256:eeee",
    });
    case "wrong_endpoint": return Object.assign({}, base, {
      endpoint_called: "teams/statistics", endpoint_matches: false,
      found: [], missing: ["expected_goals"], digest: "sha256:ffff",
    });
    case "no_endpoint": return Object.assign({}, base, {
      endpoint_called: null, endpoint_expected: null, endpoint_matches: null,
      digest: "sha256:0001",
    });
    case "refused": return Object.assign({}, base, {
      ok: false, response: "MISSING PARAMETER: fixture", parsed: false,
      searched: ["expected_goals"], found: [], missing: [], digest: "sha256:0002",
    });
    case "truncated": return Object.assign({}, base, {
      truncated: true, chars: 160312, digest: "sha256:0003",
    });
    // Byte-identical to `found_value`: two fields, one endpoint, one payload.
    case "repeat": return base;
    default: throw new Error("no such probe shape: " + kind);
  }
}
let NEXT_PROBE = "found_value";
globalThis.fetch = (url, opts) => {
  const reply = (body, status = 200) => Promise.resolve({
    ok: status < 400, status,
    text: () => Promise.resolve(typeof body === "string" ? body : JSON.stringify(body)),
    json: () => Promise.resolve(body),
  });
  if (url.endsWith("/trace")) return reply(TRACE);
  if (url.includes("verification-queue")) {
    return reply({ settleable_verdicts: ["human_sourced", "human_endorsed", "rejected"] });
  }
  if (url.endsWith("/lineage")) return reply({ callers: [], callees: [] });
  if (url.endsWith("/probe")) return reply(probeReply(NEXT_PROBE));
  if (url.endsWith("/contradict")) return reply({ anomaly_id: "x" });
  return reply({}, 404);
};

// ── run it ───────────────────────────────────────────────────────────────
const FAIL = [];
const ok = (cond, what) => { if (!cond) FAIL.push(what); };

const mod = { exports: {} };
// The page's script, plus a hook returning the internals this file drives.
new Function(
  "module",
  SCRIPT + "\n;module.exports = { render: () => render(LAST_TRACE), boot," +
    " probeForm, probeVerdict, runProbe, askedFor, hintEndpoint," +
    " get LENSES() { return LENSES; }, setLens: v => { LENS = v; }," +
    " get LAST_TRACE() { return LAST_TRACE; }, get FIELD_BY_PATH() { return FIELD_BY_PATH; }," +
    " get TOOL_CALLS() { return TOOL_CALLS; } };",
)(mod);
const P = mod.exports;

(async () => {
  await P.boot();
  ok(CONTENT.html.length > 500, "boot produced no page");
  ok(!/Could not read the trace/.test(CONTENT.html), "boot fell into its catch: " + CONTENT.html.slice(0, 400));

  // 1. every lens draws something, or says it is empty on purpose.
  for (const [id, label] of P.LENSES) {
    P.setLens(id);
    CONTENT.html = "";
    P.render();
    ok(CONTENT.html.length > 200, `lens ${id} (${label}) drew nothing`);
    const bad = CONTENT.html.match(/.{0,60}(undefined|NaN|\[object Object\]).{0,60}/);
    ok(!bad, `lens ${id} rendered a placeholder value: ${bad && bad[0]}`);
  }
  P.setLens("checked");

  // 1b. the row grammar: value · condition · act, in that order, on every row of
  //     every lens. This is the property the whole layout rests on — a reader
  //     learns one row and reads a hundred — and it is invisible to any check
  //     that looks at one representative row.
  for (const [id] of P.LENSES) {
    P.setLens(id);
    CONTENT.html = "";
    P.render();
    const rows = CONTENT.html.split('<div class="arow').slice(1);
    ok(rows.length > 0 || id === "empty" || id === "unsourced",
      `lens ${id} drew no rows at all`);
    rows.forEach((row, i) => {
      const cells = [...row.matchAll(/<span class="(a-p|a-k[^"]*|a-v[^"]*|a-c[^"]*|a-a)"/g)]
        .map((m) => m[1].split(" ")[0].replace(/^(a-k|a-v).*/, "$1"));
      const order = cells.filter((c) => ["a-p", "a-k", "a-v", "a-c", "a-a"].includes(c));
      ok(order.join(",") === "a-p,a-k,a-v,a-c,a-a",
        `lens ${id} row ${i}: cells are ${order.join(",")} — the grammar is ` +
        `pips, name, value, condition, act, always, or the columns cannot be scanned`);
    });
  }
  P.setLens("checked");

  // 1c. the legend speaks the rows' vocabulary. A legend entry naming a state no
  //     row is in, or a row in a state the legend never explains, is the drift
  //     that put a paragraph about "returned nothing" over rows reading
  //     "never asked".
  CONTENT.html = "";
  P.render();
  const rowTokens = [...CONTENT.html.matchAll(/<span class="a-c [^"]*">([^<]*)<\/span>/g)]
    .map((m) => m[1].trim()).filter(Boolean);
  const legendTokens = [...CONTENT.html.matchAll(/<span class="lg-t">([^<]*)<\/span>/g)]
    .map((m) => m[1].trim());
  ok(legendTokens.length > 0, "the legend explained nothing at all");
  for (const t of legendTokens) {
    ok(rowTokens.includes(t),
      `the legend explains "${t}" and no row is in that state`);
  }
  // Every explainable state on screen is explained exactly once.
  ok(new Set(legendTokens).size === legendTokens.length,
    "a state is explained twice in one legend");

  // 1d. explain once: no sentence from the legend is repeated on a row.
  const perRowProse = CONTENT.html.match(/class="why"/g) || [];
  ok(perRowProse.length === 0,
    `${perRowProse.length} row(s) carry their own paragraph. The reason belongs ` +
    `to the state and the state's reason is in the legend — this is the wall ` +
    `that shipped twice`);

  // 2. the probe form opens on every field, runnable or not.
  for (const f of TRACE.fields) {
    const html = P.probeForm(f.name, P.FIELD_BY_PATH[f.name] || f);
    ok(/class="probe"/.test(html), `no probe form for ${f.name}`);
    ok(/probe-verdict/.test(html), `${f.name}'s form has no verdict slot`);
    ok(!/undefined/.test(html), `${f.name}'s form printed undefined`);
    // A field whose contract names no endpoint must not be handed one.
    const fd = P.FIELD_BY_PATH[f.name] || f;
    ok(P.hintEndpoint(fd) === (fd.probe_endpoint || ""),
      `${f.name}: hintEndpoint disagrees with the served probe_endpoint`);
    if (!fd.probe_endpoint) {
      ok(!/"endpoint"/.test(html), `${f.name} has no endpoint and was prefilled one`);
    }
  }

  // 3. `askedFor` agrees with the record: `injuries` was asked, xG never was.
  ok(P.askedFor(P.FIELD_BY_PATH["injuries"]) === "asked", "injuries reads as unasked");
  ok(P.askedFor(P.FIELD_BY_PATH["advanced_metrics.xg"]) === "unasked",
    "xG reads as asked, and the record shows `fixtures/statistics` was never called");
  ok(P.askedFor(P.FIELD_BY_PATH["summary"]) === "unused",
    "a tool with no calls at all should read `unused`");

  // 4. the verdict, per response shape. Text, not classes: the words are what a
  //    reader acts on, and a wrong word here is the whole failure mode.
  // Whitespace-collapsed, because that is what a browser shows: a phrase broken
  // across a template literal's newlines renders as one line, and asserting on
  // the source would fail on indentation rather than on wording.
  const V = (kind, dotted = "advanced_metrics.xg") =>
    P.probeVerdict(probeReply(kind), dotted, true).replace(/\s+/g, " ");
  const cases = [
    ["found_value", /FOUND/, /the name is a value here/],
    ["found_key", /in 30 places, first 12 shown/, /Not searched: home\/away splits/],
    ["missing", /NOT FOUND/, /source does not carry it/],
    ["no_keys", /nothing to look for/, null],
    ["unparseable", /not JSON/, /unknown, not absent/],
    ["wrong_endpoint", /not this field's endpoint/, /teams\/statistics/],
    ["refused", /The tool refused/, null],
    ["truncated", /FOUND/, /160,312/],
  ];
  for (const [kind, must, also] of cases) {
    const t = V(kind);
    ok(must.test(t), `${kind}: verdict does not say ${must} — got: ${t.slice(0, 200)}`);
    if (also) ok(also.test(t), `${kind}: verdict is missing ${also}`);
    ok(!/undefined|NaN/.test(t), `${kind}: verdict printed a placeholder — ${t.slice(0, 200)}`);
  }
  ok(!/endpoint/.test(V("no_endpoint")),
    "a tool with no endpoint had one claimed about it");
  // An answer to the wrong question must not be offered as a citation.
  ok(!/settle it above and cite this call/.test(V("wrong_endpoint")),
    "a run against an endpoint the contract does not name invites settling the " +
    "claim with it, which is how a sound answer to a different question becomes " +
    "a citation");

  // 5. the same payload for two different fields must SAY so.
  const fresh = Object.assign(probeReply("found_value"), { digest: "sha256:fresh" });
  const first = P.probeVerdict(fresh, "match_statistics", true);
  ok(!/identical/.test(first), "the first run cannot be identical to anything");
  const second = P.probeVerdict(
    Object.assign(probeReply("repeat"), { digest: "sha256:fresh" }),
    "advanced_metrics.xg", true);
  ok(/Byte-identical/.test(second) && /match_statistics/.test(second),
    "the second field got the same payload and the page did not say so: " + second.slice(0, 300));

  // 5b. The form must open as a child of the ROW.
  //
  // `.probe` is `grid-column: 2 / -1`. That applies to a grid child and to
  // nothing else, so a form inserted after the button — inside the 132px act
  // cell — renders one word wide down the edge of an empty row. Checked by where
  // the markup went, not by what it says, because what it says was always right.
  {
    const actCell = makeEl("span");
    actCell.className = "a-a";
    const row = makeEl("div");
    row.className = "arow";
    // No form yet: this is the first press, the one that opens it.
    row.querySelector = () => null;
    const openBtn = makeEl("button");
    openBtn.dataset = { probe: "advanced_metrics.xg" };
    openBtn.closest = (sel) => (sel === ".arow" ? row : null);
    INSERTS.length = 0;
    await P.runProbe(openBtn);
    ok(INSERTS.length === 1, `opening the form inserted ${INSERTS.length} times`);
    const ins = INSERTS[0] || {};
    ok(ins.into === row,
      "the probe form is not inserted into the row. `grid-column: 2 / -1` only " +
      "applies to a grid child — anywhere else the form renders inside whatever " +
      "cell it landed in, which is 132px wide.");
    ok(/class="probe"/.test(ins.html || ""), "what was inserted is not the probe form");
    ok(ins.into !== actCell, "the form opened inside the act cell");
  }

  // 6. `runProbe` end to end, which is where `h is not defined` lived: a
  //    ReferenceError inside the try reported itself as an outage.
  const out = makeEl("div"), verdict = makeEl("div"), input = makeEl("input");
  input.value = JSON.stringify({ endpoint: "fixtures/statistics", params: { fixture: 1 } });
  const form = makeEl("div");
  form.dataset = { probeForm: "advanced_metrics.xg", settleable: "1" };
  form.querySelector = (sel) => ({
    ".probe-out": out, ".probe-verdict": verdict, ".probe-in": input, ".wrong": null,
  }[sel] ?? null);
  const btn = makeEl("button");
  btn.dataset = { probe: "advanced_metrics.xg" };
  btn.closest = () => ({ querySelector: () => form });
  for (const kind of ["found_value", "missing", "unparseable", "refused", "truncated", "wrong_endpoint"]) {
    NEXT_PROBE = kind;
    out.text = ""; verdict.html = "";
    await P.runProbe(btn);
    ok(!/Could not reach the platform/.test(out.text),
      `${kind}: a completed request reported itself as a network failure — ${out.text.slice(0, 200)}`);
    ok(!/failed to render it/.test(out.text),
      `${kind}: runProbe threw — ${out.text.slice(0, 300)}`);
    ok(verdict.html.length > 20, `${kind}: no verdict was written`);
    ok(out.text.length > 0, `${kind}: no response body was written`);
  }
  // And a real network failure still reads as one.
  const savedFetch = globalThis.fetch;
  globalThis.fetch = () => Promise.reject(new Error("offline"));
  out.text = "";
  await P.runProbe(btn);
  ok(/Could not reach the platform: offline/.test(out.text),
    "a dead network no longer reads as a dead network: " + out.text);
  globalThis.fetch = savedFetch;

  if (FAIL.length) {
    console.error(`\n${FAIL.length} failure(s):`);
    FAIL.forEach((f) => console.error("  ✗ " + f));
    process.exit(1);
  }
  console.log("trace probe render: all checks pass");
})();
