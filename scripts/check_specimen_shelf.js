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
  " agentVerdict, promptCheck, closeDrawer, PRODUCERS, CONSUMERS," +
  " set REG(v) { REG = v; }, get REG() { return REG; }," +
  " set DIRTY(v) { CONTRACT_DIRTY = v; }, get DIRTY() { return CONTRACT_DIRTY; }," +
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
ok(/The contract as stored/.test(c) && /15 declared field\(s\)|4 declared field\(s\)/.test(c),
  "the compile block does not name its own subject, so its headline competes " +
  "with the agent's");
ok(!/\bcompiles?\b/i.test(c.replace(/does not compile/g, "")),
  "the compile block is still passing a verdict. `compile` has exactly one " +
  "subject now — the agent — and it is said once, at the top of the shelf");
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
ok(/2<\/b> that nothing can settle/.test(c),
  "two unsettleable fields are not reported by the stored-contract block");
ok(/ghost_tool/.test(c), "the error does not name the tool nobody can dispatch");
// Even then, pending must not be swept into the failure.
ok(/standing request/.test(c),
  "pending lost its meaning as soon as an unrelated error appeared");

// No contract at all is not the same as a contract that resolves to nothing.
S.D = { profile: PROFILE, declaration: { rungs: RUNGS, declared: 2, total: 4,
                                         next: "output_schema" } };
c = S.declarationPanel();
ok(!/The contract as stored/.test(c),
  "an agent with no declared fields is reporting on a stored contract, which " +
  "asserts something about a contract that does not exist");

// ── 2c. ONE verdict, and it is about the agent ───────────────────────────
//
// Three headlines used to disagree on one screen, each about a different
// subject and all three correct:
//
//   prompt panel      "2 errors"       the prompt against the contract
//   compile block     "Compiles."      the STORED contract's fields
//   contract builder  "Not compiled"   the DRAFT in the editor
//
// A reader asking "does this agent work?" got three answers to three questions
// they had not asked. So the verb has one subject, said once and first; the
// panels below report facts and name their own subjects.
const TOOLED = { ...PROFILE, prompt_check: {
  gets_tools: true, trigger: null, sourced_fields: 6,
  contradicts_contract: false, produces_schema: "rabble/phylogenetic_profile",
  names_its_type: true } };

S.D = { profile: TOOLED, declaration: GP };
let v = S.agentVerdict();
ok(/This agent compiles\./.test(v),
  "an agent whose prompt and contract agree, with no unsettleable field, does " +
  "not read as compiling");
ok(/7 field\(s\) are pending/.test(v),
  "the verdict swallowed the pending count. Green means zero ERRORS, and the " +
  "seven standing requests still have to be visible in the headline that " +
  "declares the agent healthy");

// The contradiction the platform can author itself: a contract naming tools
// for six fields, and a prompt that removed the loop which could call them.
// This is genome_profiler's state for its first 68 pulses.
const BYPASSED = { ...PROFILE, prompt_check: {
  gets_tools: false, trigger: "ONLY", sourced_fields: 6,
  contradicts_contract: true, produces_schema: "rabble/phylogenetic_profile",
  names_its_type: false } };
S.D = { profile: BYPASSED, declaration: GP };
v = S.agentVerdict();
ok(/does not compile — 2 error\(s\)/.test(v),
  "the two prompt errors do not reach the agent's verdict, so the shelf can " +
  "say `Compiles.` about an agent that cannot call any of its tools");
ok(/what it says/.test(v),
  "the verdict does not say WHERE the fault is, so it is a grade rather than " +
  "a direction");

// Faults from both halves add up into one number, rather than two panels each
// reporting their own.
S.D = { profile: BYPASSED, declaration: { ...GP,
  counts: { resolved: 1, error: 2, pending: 7 } } };
v = S.agentVerdict();
ok(/3 error\(s\)/.test(v),
  "prompt faults and contract faults are not summed, so the headline is about " +
  "a panel rather than about the agent");
ok(/its contract/.test(v) && /what it says/.test(v),
  "the verdict names only one of the two places that are wrong");

// Absent is not bad: an agent with no prompt check and no contract cannot
// contradict anything, and must not be reported as broken.
S.D = { profile: PROFILE, declaration: { rungs: RUNGS, declared: 2, total: 4 } };
v = S.agentVerdict();
ok(/This agent compiles\./.test(v) && !/error/.test(v),
  "an unconfigured agent is being called broken. Absent must look different " +
  "from bad.");

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
for (const part of ["What it says", "trusted about", "Brain", "Personality",
                    "Bank account", "Identity and reach"]) {
  ok(shelf.includes(part), `the shelf has no "${part}" part`);
}
for (const g of ["prompt", "intelligence", "personality", "manage"]) {
  ok(shelf.includes(`id="af-${g}"`), `the ${g} group has no mount point`);
}
// Prose budget. The shelf grew a paragraph per group and they were the first
// thing on screen every time it opened.
const paras = (shelf.match(/class="note"/g) || []).length;
ok(paras <= 1, `${paras} paragraphs above the controls; the shelf is a workbench`);
ok(shelf.includes('id="shelf-grip"'), "there is no drag handle");

// ── 4b. the prompt against the contract ─────────────────────────────────
//
// `prompt_demands_structured_output` substring-matches the system prompt, and a
// match makes ToolAwareExecutor skip the tool loop. So a prompt saying "output
// valid JSON only" plus a contract with Sourced fields is a contradiction the
// platform authored and never displayed. Three typed agents are in that state.
//
// The prompt panel comes FIRST, because it is the first thing an author needs
// and it was three panels down inside Brain.
ok(shelf.indexOf("What it says") < shelf.indexOf("trusted about"),
  "the prompt is not the first part of the shelf");

const CHK = (o) => {
  S.D = { profile: { ...PROFILE, prompt_check: o },
          declaration: { rungs: RUNGS, declared: 2, total: 4, next: "output_schema" },
          record: { runs: 1, dream_budget: 10, dream_used: 1 } };
  S.drawer();
  return el("drawer").html;
};

// The contradiction: an error, and it names the phrase and the count.
let h2 = CHK({ gets_tools: false, trigger: "output valid JSON only",
               sourced_fields: 6, contradicts_contract: true,
               produces_schema: "rabble/phylogenetic_profile", names_its_type: true });
ok(/c-error/.test(h2), "a prompt that removes the tool loop its contract needs is not an error");
ok(/output valid JSON only/.test(h2),
  "the trigger phrase is not named, so the author is told a verdict and not a cause");
ok(/6 contracted field/.test(h2), "the number of unfillable fields is not stated");

// No tool loop and no sourced fields: a reading, not a fault.
h2 = CHK({ gets_tools: false, trigger: "Return JSON:", sourced_fields: 0,
           contradicts_contract: false, produces_schema: null, names_its_type: null });
ok(!/c-error/.test(h2),
  "an agent with no sourced fields is faulted for having no tool loop, which is " +
  "correct behaviour for one that reasons over what it is given");
ok(/c-pending/.test(h2), "no tool loop is not reported at all");

// Tools available, type named: two resolved and nothing red.
h2 = CHK({ gets_tools: true, trigger: null, sourced_fields: 6,
           contradicts_contract: false, produces_schema: "fermi/x", names_its_type: true });
ok(!/c-error/.test(h2), "a healthy prompt is being faulted");
ok((h2.match(/c-resolved/g) || []).length >= 2, "neither fact reads as resolved");

// Typed and unnamed: an error, because nothing tells the agent what to emit.
h2 = CHK({ gets_tools: true, trigger: null, sourced_fields: 0,
           contradicts_contract: false, produces_schema: "fermi/x", names_its_type: false });
ok(/c-error/.test(h2) && /never mentions it/.test(h2),
  "a prompt that never names the type its contract declares is not flagged");

// Absent is not bad: no contract means nothing to contradict.
h2 = CHK({ gets_tools: true, trigger: null, sourced_fields: 0,
           contradicts_contract: false, produces_schema: null, names_its_type: null });
ok(!/c-error/.test(h2) && !/names its type/.test(h2),
  "an agent with no contract is being judged against one");
// A served check with no phrase list must not render a dangling sentence.
ok(!/phrases are \./.test(h2),
  "the row promises a list of phrases the platform did not serve");

// ── 4b-ii. the sentence an author actually reads ────────────────────────
//
// The healthy row read: "Nothing in the prompt trips
// `prompt_demands_structured_output`, so the tool loop runs." Two faults in one
// line. It named a Rust function nobody outside this repo can look up, and
// "trips" reads as the PROMPT doing something — an author reported it as
// "sounds like the prompt can invoke the tools". The direction is the reverse:
// the platform reads the prompt and decides from it whether the agent gets
// tools at all. A row about the mechanism that silently removes an agent's
// tools is the last one that may be read backwards.
h2 = CHK({ gets_tools: true, trigger: null, sourced_fields: 6,
           contradicts_contract: false, produces_schema: null, names_its_type: null,
           patterns: ["ONLY", "raw JSON", "Return JSON:"] });
ok(!/prompt_demands_structured_output/.test(h2),
  "the row still names a Rust symbol as though the reader could look it up");
ok(!/\btrips\b/.test(h2),
  "`trips` is still the verb, and it reads as the prompt invoking something " +
  "rather than the platform reading it");
ok(/platform scans/.test(h2),
  "the row does not say who reads the prompt, so the direction is left to be " +
  "inferred and it was inferred backwards");
// The phrases come from the served list, which comes from the one Rust
// definition. A copy in the template would state a rule the executor does not
// follow, and the author would trust it.
ok(/<code>Return JSON:<\/code>/.test(h2),
  "the phrases that switch tools off are not named, so the author is told a " +
  "rule exists and not what it is");

// The two off states say what the phrase DID, not that it matched a predicate.
h2 = CHK({ gets_tools: false, trigger: "Return JSON:", sourced_fields: 0,
           contradicts_contract: false, produces_schema: null, names_its_type: null });
ok(/switche[sd] (?:this agent's )?tools off|switched off/.test(h2),
  "the off state does not say that the agent's tools are gone");

// And no check served at all must render nothing rather than guessing.
S.D = { profile: PROFILE, declaration: { rungs: RUNGS, declared: 2, total: 4 },
        record: { runs: 1, dream_budget: 10, dream_used: 1 } };
S.drawer();
ok(!/pchk-row/.test(el("drawer").html),
  "the panel invents a verdict when the platform served no check");

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
// Said ONCE for the group, not once per rung — and now SHOWN as one thing
// rather than asserted over three rows that happen to be adjacent.
//
// This used to require three `.rung.grouped` rows plus the sentence "these
// three are one save" exactly once. The three rows were the defect the
// sentence existed to apologise for: three headings, three paragraphs of
// `unlocks`/`without_it`, and a single button on the last of them, which reads
// as three pieces of work no matter what the prose says. They are one rung now
// — one mark, one heading, one button — with the three parts as segments of
// one card, so the claim is structural instead of prose.
const oneSave = (shelf.match(/one save/g) || []).length;
ok(oneSave === 1,
  `"one save" appears ${oneSave} times; the reason belongs to the group and is ` +
  `said once, or it is per-row documentation again`);
ok((shelf.match(/class="rung [^"]*grouped/g) || []).length === 1,
  "the contract is not one rung, so the three parts that close together are " +
  "still presented as separate work");
ok((shelf.match(/class="cseg/g) || []).length === 3,
  "the contract card does not show its three parts, so `2 of 3 written` is a " +
  "number with nothing behind it");
ok(/class="cseg on/.test(shelf) && /class="cseg"/.test(shelf),
  "a written part and an unwritten one render identically, which is the " +
  "distinction the whole card exists to draw");
// `ports` is the one rung the contract editor does NOT close — and only half of
// it, because the compiler derives `produces` from `produces_schema`.
const portsRow = shelf.slice(shelf.indexOf("ports"), shelf.indexOf("output_type"));
ok(!/closed by the/.test(portsRow),
  "the ports rung claims the contract editor closes it; `accepts` is nobody else's");
// The ports rung now has an editor for `accepts`.
ok(shelf.includes('id="af-ports"'),
  "the ports rung has no mount point for the accepts editor");

// ── 4c-ii. ports, drawn as the connectors they are ─────────────────────
//
// A port is a stud: another agent's `produces` clicks into this agent's
// `accepts` wherever the label matches, and that join is the entire basis of
// composition here. It was rendered as two rows of a definition list plus
// three lines of prose — the same information and none of the idea, and with
// the one fact that makes a port worth declaring missing entirely: WHO is on
// the other side.
const PORTED = { ...PROFILE,
  accepts: ["fermi/forecast-question/1", "team"],
  produces: ["fermi/football_evidence"] };
const withPorts = () => {
  S.D = { profile: PORTED,
          declaration: { rungs: RUNGS, declared: 2, total: 4, next: "output_schema" } };
  return S.declarationPanel();
};

S.PRODUCERS.set("fermi/forecast-question/1",
  [{ name: "fermi", label: "fermi" }, { name: "macro_forecaster", label: "macro_forecaster" }]);
S.CONSUMERS.set("fermi/football_evidence", [{ name: "fermi", label: "fermi" }]);
S.REG = true;
let ports2 = withPorts();

ok(/class="seam-side" data-dir="in"/.test(ports2)
   && /class="seam-side" data-dir="out"/.test(ports2),
  "the two faces are not drawn as two faces, so which labels are inputs and " +
  "which are outputs is left to be read off a definition list");
ok(/class="stud joined"/.test(ports2) && /class="stud orphan"/.test(ports2),
  "a port with somebody on the other side renders the same as one facing a " +
  "wall — which is not a smaller version of connected, it is a different fact");
// The count, and then the names. "2 agents" you cannot open is a statistic.
ok(/<b>2<\/b> produce this/.test(ports2),
  "a port does not say how many agents can plug into it");
ok(/href="\/specimen\/macro_forecaster"/.test(ports2),
  "the counterparts are counted and not named, so the seam is a number rather " +
  "than a composition somebody can go and build");
ok(/<b>1<\/b> accept this/.test(ports2),
  "the produces face does not say who accepts what this agent emits");
ok(/nothing on the other side/.test(ports2),
  "a label that matches nothing anywhere is not reported as such, and that is " +
  "the one port state an author can act on immediately");
// A schema id is a checkable type; a bare noun is an author's word for one,
// and `team` is not something a validator can resolve on both sides.
ok(/class="stud-k schema">schema</.test(ports2)
   && /class="stud-k">label</.test(ports2),
  "schema ids and bare author labels render identically, so `team` reads as " +
  "the same kind of promise as `fermi/forecast-question/1`");

// Unknown is not zero. Third time this rule appears on this page: if the
// register did not load, every port would read "nothing on the other side" —
// a false claim about the whole fleet, written by an empty Map.
S.REG = false;
const noReg = withPorts();
ok(!/nothing on the other side/.test(noReg),
  "a register that failed to load renders every port as unconnectable, which " +
  "is a claim about the fleet made by an empty map. Absent must look " +
  "different from bad");
ok(/counterparts unknown/.test(noReg),
  "the shelf does not say that the counterpart read failed, so the reader " +
  "cannot tell a quiet port from a quiet page");
ok(!/class="stud orphan"/.test(noReg),
  "ports are marked as facing a wall on the strength of a fetch that failed");
S.REG = true;

// ── 4d. the editor can be saved from the surface that mounts it ──────────
//
// `ContractBuilder` writes no save button of its own — its HOST provides one.
// /contracts has `Save to agent`; the create wizard saves at the end of its
// flow; the shelf mounted the editor and provided nothing. So every contract
// edited from the surface an owner actually configures agents from was
// discarded on close, silently, while the editor's own status chip read
// `Draft is ready to save`. A mounted editor with no save is worse than no
// editor, because it invites the work it then throws away.
const SRC = blocks[0];
ok(/data-cb-save/.test(SRC),
  "the shelf mounts the contract editor and offers no way to save it, so every " +
  "edit made here is discarded on close");
ok(/ContractBuilder\.saveTo\(/.test(SRC),
  "the save control does not call the editor's own save path, so it is either " +
  "a second implementation of the PUT or a button that does nothing");
ok(/mountContractBar\(/.test(SRC) &&
   SRC.indexOf("ContractBuilder.mount(") < SRC.indexOf("mountContractBar(host"),
  "the save bar is not attached when the editor is mounted");

// Closing the shelf destroys the editor. With unsaved work that is the loss of
// an afternoon, and the scrim is one stray click wide.
let asked = 0;
globalThis.confirm = () => { asked += 1; return false; };
S.DIRTY = true;
el("drawer").classList.remove = () => { FAIL.push("the shelf closed over unsaved contract work"); };
S.closeDrawer();
ok(asked === 1,
  "closing the shelf with unsaved contract work does not ask, so the only " +
  "unrecoverable action on this surface is also the easiest one to trigger");
el("drawer").classList.remove = () => {};
S.DIRTY = false;
asked = 0;
S.closeDrawer();
ok(asked === 0,
  "closing a shelf with nothing to lose still interrupts, which is how a " +
  "warning stops being read");
// The competition block shows where creators declare their participation.
ok(shelf.includes('id="af-competition"'),
  "the shelf has no competition mount point — creators cannot declare domains, price, or support tier");
// Platform-computed competition stats are displayed (not editable).
ok(/comp-stats/.test(shelf),
  "the competition block is missing the platform-computed stats section (fidelity, selection rate)");
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
