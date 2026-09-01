// Render the configuration shelf, over the ladder states that matter.
//
// The shelf is where an agent is created and managed, so its one rankable panel
// — the declaration ladder — has to hold three properties that are easy to break
// and invisible to a syntax check:
//
//   1. Exactly ONE rung is recommended. A recommendation pointing at four things
//      is a list, and the whole reason this group leads is that the platform can
//      rank it.
//   2. A declared rung says what it UNLOCKS; an absent one says what reads
//      `unknown` WITHOUT it. Swapping those makes the panel a checklist.
//   3. A ladder that failed to load says so, rather than rendering as an agent
//      that has declared nothing. Absent must look different from empty.
//
// Plus the drag handle exists and the width is clamped, because a shelf that can
// be dragged to 12px is a shelf you can lose.
//
// Usage: node scripts/check_specimen_shelf.js

const fs = require("fs");
const path = require("path");

const HTML = fs.readFileSync(
  path.join(__dirname, "..", "templates", "specimen.html"), "utf8");

const blocks = [...HTML.matchAll(/<script(?:\s[^>]*)?>([\s\S]*?)<\/script>/g)]
  .map((m) => m[1])
  .filter((s) => s.includes("function drawer("));
if (blocks.length !== 1) {
  throw new Error(`expected one shelf-bearing script block, found ${blocks.length}`);
}

// ── a DOM, only as much as the shelf touches ─────────────────────────────
const NODES = {};
function el(id) {
  return NODES[id] || (NODES[id] = {
    id, html: "", className: "",
    dataset: {}, style: { setProperty(k, v) { this[k] = v; } },
    classList: { add() {}, remove() {}, toggle() {} },
    set innerHTML(v) { this.html = String(v); },
    get innerHTML() { return this.html; },
    addEventListener() {}, removeEventListener() {},
    setPointerCapture() {}, releasePointerCapture() {},
    querySelector: () => null, querySelectorAll: () => [],
    closest: () => null,
  });
}
const STORE = {};
globalThis.localStorage = {
  getItem: (k) => (k in STORE ? STORE[k] : null),
  setItem: (k, v) => { STORE[k] = String(v); },
};
globalThis.document = {
  getElementById: el,
  documentElement: el("html"),
  body: el("body"),
  addEventListener() {},
  querySelectorAll: () => [],
  querySelector: () => null,
  createElement: () => el("tmp"),
};
globalThis.window = { innerWidth: 1600, innerHeight: 900, Nav: null };
globalThis.location = { pathname: "/specimen/football_analyst", href: "" };
globalThis.fetch = () => Promise.reject(new Error("no net"));

const mod = { exports: {} };
new Function("module", blocks[0] +
  "\n;module.exports = { drawer, declarationPanel, setShelfWidth, wireGrip," +
  " set D(v) { D = v; }, get D() { return D; } };")(mod);
const S = mod.exports;

const FAIL = [];
const ok = (c, what) => { if (!c) FAIL.push(what); };

const RUNGS = [
  { rung: "ports", declares: "What it accepts and produces.", owner: "agents.accepts",
    unlocks: "port_trust::bind_input at every boundary.",
    without_it: "The input-binding gate returns undetermined for every call.",
    present: true },
  { rung: "output_type", declares: "The name of the type it produces.",
    owner: "agents.output_contract.produces_schema",
    unlocks: "declared_type, so a consumer knows what it was handed.",
    without_it: "A delegated consumer receives an untyped blob.", present: true },
  { rung: "output_schema", declares: "A checkable shape.", owner: "output_contract.schema",
    unlocks: "Structural validation at the seam.",
    without_it: "Nothing can say the document is the shape it claims.", present: false },
  { rung: "field_contract", declares: "Which tool could settle each field.",
    owner: "output_contract.grounding",
    unlocks: "Grounding, the assertion queue and the trace.",
    without_it: "Nothing can say whether this agent fabricated a value.",
    present: false },
];
const PROFILE = {
  agent_name: "football_analyst", label: "football_analyst", status: "active",
  visibility: "public", tier: "research", min_tier: "free", fork_count: 0,
  forked_from: null,
  substrate: { provider: "anthropic", model: "claude-opus-4", executor: "llm",
               temperature: 0.7, persona_version: 3 },
};

// ── 1. exactly one recommendation, and it is the first absent rung ───────
S.D = { profile: PROFILE,
        declaration: { rungs: RUNGS, declared: 2, total: 4, next: "output_schema" } };
let h = S.declarationPanel();
ok((h.match(/do this next/g) || []).length === 1,
  `${(h.match(/do this next/g) || []).length} rungs are recommended; a recommendation ` +
  `pointing at more than one thing is a list`);
ok(/class="rung on"/.test(h) && /class="rung off"/.test(h),
  "declared and absent rungs are not distinguished");
ok(h.includes("2 of 4"), "the count is missing, so there is no sense of progress");

// ── 2. unlocks for the declared, without-it for the absent ───────────────
const ports = h.slice(h.indexOf("ports"), h.indexOf("output_type"));
ok(ports.includes("Unlocks:") && !ports.includes("Without it:"),
  "a declared rung is being told what it lacks");
const missing = h.slice(h.indexOf("output_schema"));
ok(missing.includes("Without it:"),
  "an absent rung does not say what reads unknown without it — which is the " +
  "difference between a workbench panel and a checklist");
ok(missing.includes("declared in"),
  "an absent rung does not say where the declaration goes, so the reader cannot act");
ok(!/undefined|NaN/.test(h), "the ladder printed a placeholder: " + h.slice(0, 200));

// ── 3. nothing left to declare, and a ladder that did not load ──────────
S.D = { profile: PROFILE,
        declaration: { rungs: RUNGS.map((r) => ({ ...r, present: true })),
                       declared: 4, total: 4, next: null } };
h = S.declarationPanel();
ok(!/do this next/.test(h), "a fully declared agent is still being told to do something");
ok(/nothing left to declare/.test(h), "a fully declared agent gets no acknowledgement");

S.D = { profile: PROFILE, declaration: {} };
h = S.declarationPanel();
ok(/did not load/.test(h),
  "a ladder that failed to load renders as an agent that declared nothing — " +
  "absent must look different from empty");

// ── 4. the whole shelf, and the three groups ────────────────────────────
S.D = { profile: PROFILE,
        declaration: { rungs: RUNGS, declared: 2, total: 4, next: "output_schema" } };
S.drawer();
const shelf = el("drawer").html;
for (const group of ["Declaration", "Intelligence", "Manage"]) {
  ok(shelf.includes(`<h3>${group}</h3>`), `the ${group} group is missing from the shelf`);
}
ok(shelf.includes('id="shelf-grip"'), "there is no drag handle");

// ── 4b. the recommended rung has the control that closes it ─────────────
//
// `ContractBuilder` is already shared by the create wizard and /contracts. A
// rung the shelf recommends and cannot act on is a description, which is the
// defect the trace's act column was rebuilt to end — so the field_contract rung
// carries the editor, and the editor is the existing widget rather than a
// fourth copy of it.
ok(/data-open-contract/.test(shelf),
  "the field_contract rung recommends itself and offers no way to build it");
ok(/id="shelf-contract"/.test(shelf), "there is nowhere for the editor to mount");
ok(/contract-builder\.js/.test(HTML),
  "the shelf does not load the shared contract editor, so it would have to grow " +
  "its own — and a second copy of a 1,700-line editor is the drift this repo " +
  "keeps finding");
// Only that rung. A button on every rung is a button that means nothing.
ok((shelf.match(/data-open-contract/g) || []).length === 1,
  "more than one rung carries the contract editor");
ok(shelf.includes("claude-opus-4") && shelf.includes("anthropic"),
  "Intelligence does not show what the agent actually runs on");
ok(!/undefined|NaN/.test(shelf),
  "the shelf printed a placeholder: " +
  (shelf.match(/.{0,60}(undefined|NaN).{0,60}/) || [""])[0]);

// ── 5. the width is clamped and remembered ──────────────────────────────
ok(S.setShelfWidth(10) === 380, "the shelf can be dragged to nothing and lost");
ok(S.setShelfWidth(99999) === Math.round(1600 * 0.96),
  "the shelf can be dragged past the viewport");
ok(S.setShelfWidth(700) === 700, "a reasonable width is being clamped");
ok(el("html").style["--shelf-w"] === "700px",
  `the width is not applied to the document: ${el("html").style["--shelf-w"]}`);

if (FAIL.length) {
  console.error(`\n${FAIL.length} failure(s):`);
  FAIL.forEach((f) => console.error("  ✗ " + f));
  process.exit(1);
}
console.log("specimen shelf: all checks pass");
