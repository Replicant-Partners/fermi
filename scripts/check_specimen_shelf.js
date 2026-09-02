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
ok(/class="rung on\b/.test(h) && /class="rung off\b/.test(h),
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

// ── 2b. the three compile states, and pending is not failure ────────────
//
// The case this exists for: genome_profiler declares fifteen fields, seven
// saying "no tool exists for this yet". It is the best-declared agent on the
// platform and every surface reported it as not-green, which made the only route
// to a healthy agent deleting the ambition the contract records.
const GP = {
  rungs: RUNGS, declared: 4, total: 4, next: null,
  counts: { resolved: 6, pending: 7, derived: 1, narrative: 1 },
  compiles: true,
  fields: [
    { path: "taxonomy", state: "resolved", tool: "gbif_taxonomy_tree" },
    { path: "genome.notable_genes", state: "pending", tool: null },
    { path: "phylogeny.superorder", state: "derived", tool: null },
    { path: "summary", state: "narrative", tool: null },
  ],
};
S.D = { profile: PROFILE, declaration: GP };
let c = S.declarationPanel();
ok(/Compiles\./.test(c),
  "an agent with seven pending fields and no errors does not read as compiling");
ok(/Green means zero <b>errors<\/b>/.test(c),
  "the page does not say green means zero errors, so pending reads as failure");
ok(/c-pending/.test(c), "pending fields carry no state class");
// Read from the stylesheet, because that is where the colour is. The previous
// version of this line tested the rendered markup for a CSS rule and therefore
// tested nothing — caught by mutating the rule and watching the check pass.
const pendingRule = (/\.c-pending\s*\{([^}]*)\}/.exec(HTML) || [])[1] || "";
ok(pendingRule && !/--red|#fb4934/.test(pendingRule),
  `pending is coloured as a fault (${pendingRule.trim()}). It is a declared gap ` +
  `with no source yet — colouring it like an error is what made the only route ` +
  `to a healthy agent deleting the agent's ambition`);
const errorRule = (/\.c-error\s*\{([^}]*)\}/.exec(HTML) || [])[1] || "";
ok(/--red|#fb4934/.test(errorRule),
  "error is not coloured as a fault, so the one state that IS somebody's fault " +
  "reads like the ones that are not");
for (const k of ["resolved", "pending", "derived", "narrative"]) {
  ok(new RegExp(`<b>\\d+</b> ${k}`).test(c), `the ${k} count is missing from the tally`);
}
// Explain once: one legend row per state present, never one per field.
ok((c.match(/class="cmp-leg"/g) || []).length === 4,
  `${(c.match(/class="cmp-leg"/g) || []).length} legend rows for 4 states present`);

// An error is the only state that stops a compile, and it names the tool.
S.D = { profile: PROFILE, declaration: { ...GP, compiles: false,
  counts: { resolved: 1, error: 2, pending: 7 },
  fields: [{ path: "a", state: "error", tool: "ghost_tool" },
           { path: "b", state: "error", tool: "ghost_tool" },
           { path: "c", state: "pending", tool: null }] } };
c = S.declarationPanel();
ok(/does not compile/.test(c), "two unsettleable fields still read as compiling");
ok(/ghost_tool/.test(c), "the error does not name the tool nobody can dispatch");
// Even then, pending must not be swept into the failure.
ok(/standing request/.test(c),
  "pending lost its meaning as soon as an unrelated error appeared");

// No contract at all is not the same as a contract that resolves to nothing.
S.D = { profile: PROFILE, declaration: { rungs: RUNGS, declared: 2, total: 4,
                                         next: "output_schema" } };
c = S.declarationPanel();
ok(!/Compiles\./.test(c),
  "an agent with no declared fields claims to compile, which asserts something " +
  "about a contract that does not exist");

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
// The parts of a living thing. The shelf is an anatomy, not a settings screen:
// a brain it thinks with, a personality it reads as, a bank account it spends
// from, and what it can be trusted about.
for (const part of ["trusted about", "Brain", "Personality", "Bank account",
                    "Identity and reach"]) {
  ok(shelf.includes(part), `the shelf has no "${part}" part`);
}
for (const g of ["intelligence", "personality", "manage"]) {
  ok(shelf.includes(`id="af-${g}"`), `the ${g} group has no mount point`);
}
// Prose budget. The shelf grew a paragraph per group and they were the first
// thing on screen every time it opened.
const paras = (shelf.match(/class="note"/g) || []).length;
ok(paras <= 1, `${paras} paragraphs above the controls; the shelf is a workbench`);
ok(shelf.includes('id="shelf-grip"'), "there is no drag handle");

// ── 4c. an unloaded bank account is not a broke one ──────────────────────
//
// `(undefined ?? 0) - (undefined ?? 0)` is 0, which is `<= 0`, which told an
// agent whose record had not loaded that it was out of dream credits and its
// learning had stopped. Absent must look different from bad, and this one was
// written by the arithmetic rather than by a decision.
ok(!/Out of dream credits/.test(shelf),
  "an agent whose record has not loaded is being told it is out of dream credits");
ok(/dream credits left/.test(shelf), "the bank does not report dream credits at all");

S.D = { profile: PROFILE,
        declaration: { rungs: RUNGS, declared: 2, total: 4, next: "output_schema" },
        record: { runs: 218, cost_usd: 74.2184, cost_per_run: 0.34045,
                  dream_budget: 10, dream_used: 10 } };
S.drawer();
ok(/Out of dream credits/.test(el("drawer").html),
  "an agent that has actually spent its budget is not warned");
ok(el("drawer").html.includes("$74.22") && el("drawer").html.includes("218"),
  "the bank does not show what the agent has spent or how many pulses it took");

S.D = { profile: PROFILE,
        declaration: { rungs: RUNGS, declared: 2, total: 4, next: "output_schema" },
        record: { runs: 5, dream_budget: 10, dream_used: 2 } };
S.drawer();
ok(!/Out of dream credits/.test(el("drawer").html),
  "an agent with credits remaining is warned anyway");

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
// One control, and it says which rungs it closes.
//
// ContractBuilder writes produces_schema, schema AND grounding in one save, so
// three rungs close together. The shelf's only button sat on `field_contract`,
// which implied the other two needed a separate editor that does not exist.
ok((shelf.match(/data-open-contract/g) || []).length === 1,
  "more than one rung carries the contract editor");
// Said ONCE for the group, not once per rung. Three identical sentences down
// three consecutive rows is the wall this project keeps rebuilding.
const saidOnce = (shelf.match(/these three are\s+one save/g) || []).length;
ok(saidOnce === 1,
  `"these three are one save" appears ${saidOnce} times; the reason belongs to the ` +
  `group and the rows are bracketed to show it`);
ok((shelf.match(/class="rung [^"]*grouped/g) || []).length === 3,
  "the three contract rungs are not bracketed together, so nothing shows which " +
  "ones the single sentence is about");
// `ports` is the one rung the contract editor does NOT close — and only half of
// it, because the compiler derives `produces` from `produces_schema`.
const portsRow = shelf.slice(shelf.indexOf("ports"), shelf.indexOf("output_type"));
ok(!/closed by the/.test(portsRow),
  "the ports rung claims the contract editor closes it; `accepts` is nobody else's");
// The two groups mount the shared field editor rather than printing a <dl>.
// A read-only summary where an editor belongs is how the old page grew eight
// tabs, and a second hand-written form is how they drifted.
for (const g of ["intelligence", "manage"]) {
  ok(shelf.includes(`id="af-${g}"`),
    `the ${g} group has no mount point, so it is a read-only summary again`);
}
ok(/agent-fields\.js/.test(HTML),
  "the shelf does not load the shared field editor");
ok(!/<dt>Provider<\/dt>/.test(shelf),
  "Intelligence is back to a definition list");
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
