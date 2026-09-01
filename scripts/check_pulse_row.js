// Render the shared pulse row over every state it has to distinguish.
//
// The widget exists because the stream and a specimen's Record tab had drifted
// into two renderings of one object, and the surface an agent's owner opens was
// the stripped one. A shared renderer only helps if it keeps the distinctions
// the good one made, so those are asserted here rather than reviewed:
//
//   * four addresser kinds, four marks, and `unattributed` is a GAP in the
//     record rather than an actor — it must not look benign;
//   * grounding has three states and `ungraded` is not a pass;
//   * a pulse with no episode id cannot be opened, and says so by not offering
//     the affordance rather than by failing after the click.
//
// Usage: node scripts/check_pulse_row.js

const fs = require("fs");
const path = require("path");

globalThis.window = {};
new Function(fs.readFileSync(
  path.join(__dirname, "..", "static", "js", "widgets", "pulse.js"), "utf8"))();
const Pulse = globalThis.window.Pulse;

const FAIL = [];
const ok = (c, what) => { if (!c) FAIL.push(what); };

ok(!!Pulse, "the widget did not define window.Pulse");

const P = (o) => Object.assign({
  episode_id: "386a6248-8663-417b-8b0d-82b277a4afb1",
  at: new Date(Date.now() - 3600e3).toISOString(),
  from: { kind: "human", name: "ilabra" },
  to: { kind: "agent", name: "football_analyst" },
  query: "Compare squad quality of Man city to arsenal",
  status: "success",
  cost_usd: 0.46422,
  grounding: "clean",
  recorded: true,
  error: null,
}, o);

// ── the four addressers ──────────────────────────────────────────────────
const KINDS = [
  ["human", "◍", "g-human"],
  ["agent", "▣", "g-agent"],
  ["system", "⚙", "g-system"],
  ["unattributed", "?", "g-unattributed"],
];
for (const [kind, glyph, cls] of KINDS) {
  const h = Pulse.row(P({ from: { kind, name: kind === "unattributed" ? null : "x" } }));
  ok(h.includes(glyph), `${kind} lost its glyph — a person and an agent read alike without it`);
  ok(h.includes(cls), `${kind} lost its class, so it cannot be told apart by colour or shape`);
  ok(!/undefined|NaN/.test(h), `${kind} row printed a placeholder: ${h.slice(0, 160)}`);
}
// The gap says what it is rather than inventing an actor.
ok(Pulse.row(P({ from: { kind: "unattributed", name: null } })).includes("not recorded"),
  "an unattributed pulse no longer says the record is missing");

// ── grounding: three states, and `ungraded` is not a pass ────────────────
ok(Pulse.row(P({ grounding: "clean" })).includes("m-clean"), "clean lost its mark");
ok(Pulse.row(P({ grounding: "violations" })).includes("m-viol"), "violations lost its mark");
const ung = Pulse.row(P({ grounding: "ungraded" }));
ok(ung.includes("m-ungraded"), "ungraded lost its mark");
ok(!ung.includes("m-clean"), "ungraded is being rendered as a pass, which it is not");
ok(/Not a pass/i.test(ung), "ungraded no longer says it is not a pass");
// Anything unrecognised must land on ungraded, never on clean.
ok(Pulse.row(P({ grounding: "wat" })).includes("m-ungraded"),
  "an unrecognised grounding state does not fall through to ungraded");

// ── recorded, failure, cost, and the unopenable pulse ────────────────────
ok(Pulse.row(P({ recorded: true })).includes("m-rec"), "recorded lost its mark");
ok(!Pulse.row(P({ recorded: false })).includes("m-rec"), "not-recorded is marked as recorded");
ok(Pulse.row(P({ status: "failed" })).includes("failed"), "a failed pulse does not say so");
ok(Pulse.row(P({ cost_usd: null })).includes("—"), "an absent cost renders as a number");
ok(Pulse.row(P({ cost_usd: 0.5 })).includes("$0.5000"), "cost lost its fixed precision");
const dead = Pulse.row(P({ episode_id: null }));
ok(dead.includes("ex-dead"), "a pulse with no episode id still offers to open");
ok(!dead.includes("data-href"), "a pulse with no episode id carries a destination anyway");

// ── the list, and escaping ──────────────────────────────────────────────
ok(Pulse.rows([P({}), P({})]).split('class="ex').length === 3, "rows() dropped a row");
ok(Pulse.rows([]) === "", "an empty list is not empty markup");
ok(Pulse.rows(null) === "", "a missing list throws instead of rendering nothing");
const xss = Pulse.row(P({ query: '<img src=x onerror=alert(1)>' }));
ok(!xss.includes("<img"), "a query is interpolated as markup");

// ── the clock ───────────────────────────────────────────────────────────
ok(/m ago$/.test(Pulse.when(new Date(Date.now() - 120e3).toISOString())),
  "a pulse from two minutes ago does not read as minutes");
ok(/h ago$/.test(Pulse.when(new Date(Date.now() - 7200e3).toISOString())),
  "a pulse from two hours ago does not read as hours");
ok(Pulse.when(new Date(Date.now() - 5 * 86400e3).toISOString()).includes("-"),
  "a pulse from last week does not fall back to a date");
ok(Pulse.when(null) === "", "a missing timestamp renders something");

if (FAIL.length) {
  console.error(`\n${FAIL.length} failure(s):`);
  FAIL.forEach((f) => console.error("  ✗ " + f));
  process.exit(1);
}
console.log("pulse row: all checks pass");
