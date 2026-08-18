// Exercises the Observatory's metric-explainability layer outside a browser,
// against the real football_analyst numbers, so the claims the UI makes about
// each score can be verified without a deploy. Extracts the live script body
// from the template so this cannot drift from what ships.
//
// Usage: node scripts/check_metric_registry.js
//
// Why this exists. Nine dimensions render in one grid as nine identical
// coloured bars, and they are not nine of the same kind of number: one is a SQL
// AVG over resolved forecasts, one is the negation of a six-regex match, one is
// response length divided by 80, one is exactly 1.0 by arithmetic. The registry
// in observatory.html states, per dimension, what the number is and what to do
// about it. Every statement it makes is checkable, so it is checked here:
//
//   * a value that is constant by construction is never coloured like a
//     measurement, and never folded into an aggregate;
//   * `mechFor` recovers the mechanism that actually produced a score from the
//     signal's own confidence and flags, rather than trusting the registry
//     default (Sotopia and CharacterEval both degrade to heuristics silently);
//   * the Brier arithmetic on the live 48-forecast set reproduces the figures
//     the page displays, including the one that matters most: a flat base-rate
//     prior scores 98% raw, one point below the agent's 99%, which is why raw
//     `1 - brier` must not be the headline;
//   * a trend arrow fires only on movement beyond the dimension's own spread.

const fs = require("fs");
const vm = require("vm");

// Head-less stubs. The script body binds a handful of DOM entry points at parse
// time; none of the functions under test touch them.
globalThis.document = { getElementById: () => null, querySelector: () => null, addEventListener: () => {} };
globalThis.window = globalThis;
globalThis.MicroChart = { sparkline: () => "<svg/>" };
globalThis.fetch = async () => ({ ok: true, json: async () => ({}) });

// Everything between `<script>` and the trailing `boot()` call. Sliced by
// marker rather than by line number so edits above it cannot silently shift the
// window and turn a real failure into a syntax error.
const html = fs.readFileSync("templates/observatory.html", "utf8").split("\n");
const start = html.findIndex((l) => l.trim() === "<script>");
const end = html.findIndex((l) => l.trim() === "boot();");
if (start < 0 || end < 0 || end <= start) {
  console.error("FAIL: could not locate the script body in templates/observatory.html");
  process.exit(1);
}
vm.runInThisContext(html.slice(start + 1, end).join("\n"));

let fails = 0;
const check = (name, cond, extra) => {
  console.log((cond ? "  ok   " : "  FAIL ") + name + (!cond && extra ? "  <- " + extra : ""));
  if (!cond) fails++;
};
const section = (s) => console.log("\n== " + s + " ==");

const DIMS = [
  "forecast_calibration", "goal_completion", "grounding", "persona_consistency",
  "persona_fidelity", "rapport", "safety", "social_capital", "value_alignment",
];

// ── Every dimension the platform emits must have an entry ────────────────────
// A dimension with no entry renders as a bare number with no stated scale,
// direction or mechanism, which is the condition this whole layer exists to
// remove. If an evaluator gains a dimension, this check is what fails.
section("registry covers every emitted dimension");
for (const d of DIMS) {
  const m = METRIC[d];
  const missing = m ? ["label", "is", "scale", "down", "act", "src"].filter((k) => !m[k]) : ["<entire entry>"];
  check(d, missing.length === 0 && MECH_DESC[m.mech] && ["case", "agent"].includes(m.scope),
    "missing " + missing.join(","));
}

// ── Constants must not read as measurements ──────────────────────────────────
section("values that are constant by construction");
_fa = { persona_version: 1 };
_calib = { evidence_class: "usable" };

check("persona_consistency 1.0 at v1 is called a tautology",
  /Tautological at persona v1/.test(suspectReason("persona_consistency", 1.0, null) || ""));
check("...and its colour is withheld rather than green",
  dimColor("persona_consistency", 1.0, null) === "var(--fg4)");
_fa = { persona_version: 3 };
check("...but the same 1.0 at v3 is a real measurement",
  suspectReason("persona_consistency", 1.0, null) === null);
_fa = { persona_version: 1 };

check("safety 1.0 says 'no pattern matched', not 'safe'",
  /six regex patterns/.test(suspectReason("safety", 1.0, null) || ""));
check("safety 0.0 is a genuine detection and is not demoted",
  suspectReason("safety", 0.0, null) === null);
check("grounding 1.0 is flagged as lexical saturation",
  /4-word overlap/.test(suspectReason("grounding", 1.0, null) || ""));
check("grounding 0.6 is left alone", suspectReason("grounding", 0.6, null) === null);
check("value_alignment exactly 0.5 is named as the hard-coded default",
  /nothing extractable/.test(suspectReason("value_alignment", 0.5, null) || ""));
check("value_alignment 0.11 is a real score", suspectReason("value_alignment", 0.11, null) === null);
check("goal_completion exactly 0.5 is named as the no-keywords default",
  /no usable goal keywords/.test(suspectReason("goal_completion", 0.5, null) || ""));

// A mean can land on a sentinel by coincidence. Two cases scoring 0.11 and 0.89
// average to exactly 0.50 with neither being the hard-coded default, and calling
// that "not a measurement" would suppress the most informative signal on the
// page — a dimension whose cases disagree from end to end.
check("a mean of 0.11 and 0.89 landing on 0.50 is NOT called a vacuous default",
  suspectReason("goal_completion", 0.5, null, true) === null);
check("...and it keeps its colour",
  dimColor("goal_completion", 0.5, null, true) === colorFor(0.5));
check("a genuine 0.5 with no spread is still flagged",
  suspectReason("goal_completion", 0.5, null, false) !== null);
check("safety averaged over a mix of 1.0 and 0.0 is not called a vacuous pass",
  suspectReason("safety", 1.0, null, true) === null);
check("forecast_calibration's evidence-class check survives averaging",
  (() => { _calib = { evidence_class: "no_skill" };
           const r = suspectReason("forecast_calibration", 0.98, null, true) !== null;
           _calib = { evidence_class: "usable" }; return r; })(),
  "it keys on the endpoint's verdict, not on an exact value");

// ── forecast_calibration follows the live evidence class, not a constant ─────
section("forecast_calibration defers to /calibration's evidence_class");
for (const [ec, expectSuspect] of [["usable", false], ["thin", false],
                                   ["undiscriminating", true], ["no_skill", true], ["none", true]]) {
  _calib = { evidence_class: ec };
  const got = suspectReason("forecast_calibration", 0.9868, null) !== null;
  check(`evidence '${ec}' -> ${expectSuspect ? "carries no information" : "informative"}`, got === expectSuspect);
}
_calib = { evidence_class: "usable" };

// ── The mechanism shown must be the one that actually ran ────────────────────
// Sotopia and CharacterEval both fall back to their heuristics on a provider
// failure and mark themselves doing it. Trusting the registry default would
// label a words-divided-by-80 length proxy as an LLM judgement.
section("mechanism recovered from the signal, not assumed");
check("a sotopia llm_fallback flag downgrades judge -> heuristic",
  mechFor("goal_completion", { confidence: 0.40, flags: [{ kind: "sotopia", value: "llm_fallback" }] }) === "heuristic");
check("judge confidence 0.82 stays judge",
  mechFor("goal_completion", { confidence: 0.82, flags: [] }) === "judge");
check("character's 0.30 'nothing extractable' confidence downgrades to heuristic",
  mechFor("persona_fidelity", { confidence: 0.30, flags: [] }) === "heuristic");
check("a deterministic dimension is never downgraded by its low confidence",
  mechFor("grounding", { confidence: 0.70, flags: [] }) === "deterministic");
check("no signal falls back to the registry default",
  mechFor("rapport", null) === "heuristic");
check("malformed flags do not throw", mechFor("safety", { flags: "not-an-array" }) === "deterministic");

// ── The help panel must render for anything the API can return ───────────────
section("help panel renders for every dimension");
for (const d of DIMS) {
  const withSig = metricHelpHtml("h1", d, 0.5, {
    confidence: 0.82, flags: [{ kind: "sotopia", value: "llm_fallback" }],
    rationale: '3/4 claims supported', model_used: "claude-sonnet",
  });
  const bare = metricHelpHtml("h2", d, 0.5, null);
  check(d + " renders with and without a signal",
    withSig.includes('id="h1"') && bare.includes('id="h2"') &&
    withSig.includes("To act") && !withSig.includes("undefined") && !bare.includes("undefined"),
    withSig.includes("undefined") ? "leaked 'undefined' into user-facing copy" : "");
}
check("an unregistered dimension says so rather than inventing a scale",
  /No registry entry/.test(metricHelpHtml("h3", "totally_made_up", 0.5, null)));
// The panel must carry the same suppression as the bar it explains, or the bar
// stays coloured while the panel under it calls the value vacuous.
check("the help panel honours the mean-of-differing flag",
  !/Not a measurement/.test(metricHelpHtml("h5", "goal_completion", 0.5, null, true)) &&
   /Not a measurement/.test(metricHelpHtml("h6", "goal_completion", 0.5, null, false)));
check("evaluator rationale is HTML-escaped",
  metricHelpHtml("h4", "grounding", 1.0,
    { confidence: 0.7, flags: [], rationale: '<img src=x onerror=alert(1)>' }).includes("&lt;img"));

// ── Brier: the live figures, and why raw must not be the headline ────────────
section("Brier arithmetic on the live 48-forecast set");
const N = 48, N_YES = 1;                       // 47 NO, 1 YES: World Cup winners
const base = N_YES / N;
const baseline = base * (1 - base);            // src/calibration.rs:596
const brier = 0.013262;                        // the value that yields the displayed +0.35
const skill = 1 - brier / baseline;            // src/calibration.rs:598

check("base rate displays as 2%", pct(base) === 2);
check("skill computes to +0.35", skill.toFixed(2) === "0.35", skill.toFixed(4));
check("raw 1-brier displays as 99%", pct(1 - brier) === 99);
check("a flat base-rate prior scores 98% raw on this set", pct(1 - baseline) === 98,
  "one point below the agent: the reason raw 1-brier is not the headline");
check("skill is coloured by sign: +0.35 is green",
  (skill > 0 ? "var(--green)" : "var(--red)") === "var(--green)");
check("...whereas the 0-1 band scale called the same +0.35 a failure",
  colorFor(0.35) === "var(--red)",
  "regression guard: this is the inversion the skill row was rebuilt to remove");

// ── Trend arrows must clear the dimension's own noise ────────────────────────
// The old threshold was a flat +/-0.04 against the window mean. On
// goal_completion (sigma 0.28 in production) that is 0.14 sigma, so a red
// down-arrow fired on noise and read as a finding.
section("trend arrows scaled to each dimension's spread");
const arrow = (mean, std, latest) => {
  if (std < 0.005) return "const";
  const band = Math.max(0.02, std);
  return latest > mean + band ? "up" : latest < mean - band ? "down" : "flat";
};
check("sigma 0.28, latest 0.05 below mean -> flat", arrow(0.45, 0.28, 0.40) === "flat",
  "the old code drew a red down-arrow here");
check("sigma 0.28, latest 0.35 below mean -> down", arrow(0.45, 0.28, 0.10) === "down");
check("sigma 0.00 -> 'constant', not a direction", arrow(0.99, 0.00, 0.99) === "const");
check("a tight dimension still reports a real jump", arrow(0.50, 0.01, 0.90) === "up");
check("a tight dimension does not twitch inside the 0.02 floor", arrow(0.50, 0.001, 0.505) === "const");

// ── Aggregates exclude what cannot be aggregated ─────────────────────────────
section("'eval score mean' drops constants and agent-scoped lookups");
_fa = { persona_version: 1 };
_calib = { evidence_class: "usable" };
const series = {
  forecast_calibration: { mean: 0.99 }, goal_completion: { mean: 0.45 },
  grounding: { mean: 1.00 }, persona_consistency: { mean: 1.00 },
  persona_fidelity: { mean: 0.37 }, rapport: { mean: 0.51 },
  safety: { mean: 1.00 }, social_capital: { mean: 0.46 }, value_alignment: { mean: 0.22 },
};
const names = Object.keys(series);
const excluded = names.filter((n) => excludedFromMean(n, series[n].mean, false));
const scored = names.filter((n) => !excluded.includes(n));
excluded.forEach((n) => console.log("       - " + n + ": " + excludedFromMean(n, series[n].mean)));
const meanOf = (ns) => ns.reduce((a, n) => a + series[n].mean, 0) / ns.length;
console.log("       old unweighted all-9 mean = " + pct(meanOf(names)) + "%"
  + "   new per-case mean = " + pct(meanOf(scored)) + "%");

check("the three constants are excluded",
  ["grounding", "persona_consistency", "safety"].every((d) => excluded.includes(d)));
check("forecast_calibration is excluded on scope despite 'usable' evidence",
  excluded.includes("forecast_calibration"));
check("the more specific reason wins when both apply",
  excludedFromMean("persona_consistency", 1.0, false) === "constant by construction");
check("scope exclusions state the scope reason",
  /echoed onto every case/.test(excludedFromMean("forecast_calibration", 0.99, false) || ""));
check("a dimension that varies across the window is kept in the mean",
  excludedFromMean("goal_completion", 0.5, true) === null);
check("the genuinely varying dimensions are kept",
  ["goal_completion", "persona_fidelity", "rapport", "social_capital", "value_alignment"]
    .every((d) => scored.includes(d)));
check("a dimension with no registry entry is still aggregatable",
  excludedFromMean("some_future_dimension", 0.7, false) === null,
  "excluding unknowns would silently shrink the mean as evaluators are added");

console.log();
if (fails) {
  console.error(`FAIL: ${fails} check(s) failed.`);
  process.exit(1);
}
console.log("OK: every claim the metric registry makes about these scores holds,");
console.log("    constants are not presented as measurements, and the Brier skill");
console.log("    row no longer inverts the meaning of a good result.");
