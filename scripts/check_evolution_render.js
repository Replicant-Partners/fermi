// Exercises the evolution renderers extracted from the shipped templates, so
// the badge can be checked without a browser or a deploy. Extracting from the
// template (rather than copying the functions) means this cannot drift from
// what users actually see.
//
// Usage: node scripts/check_evolution_render.js
const fs = require("fs");

function grabFn(src, name) {
  let i = src.indexOf("function " + name + "(");
  if (i < 0) throw new Error(`not found: ${name}`);
  // Keep the `async` keyword if the declaration has one, or the extracted body
  // fails to parse on its first `await`.
  if (src.slice(Math.max(0, i - 6), i) === "async ") i -= 6;
  let depth = 0,
    j = src.indexOf("{", i);
  do {
    if (src[j] === "{") depth++;
    if (src[j] === "}") depth--;
    j++;
  } while (depth > 0);
  return src.slice(i, j);
}

const strip = (s) =>
  s.replace(/<[^>]+>/g, "|").replace(/\s+/g, " ").replace(/\|+/g, " | ").trim();

let failures = 0;
const check = (label, cond, detail) => {
  console.log(`${cond ? "PASS" : "FAIL"}  ${label}`);
  if (!cond) {
    failures++;
    if (detail) console.log("      " + detail);
  }
};

// ── ecology.html ───────────────────────────────────────────────────────────
const eco = fs.readFileSync("templates/ecology.html", "utf8");
const helpers = `function esc(s){return String(s==null?"":s).replace(/&/g,"&amp;").replace(/</g,"&lt;");}`;
const fcast = new Function(helpers + grabFn(eco, "fcast") + "; return fcast;")();
const rankChip = new Function(
  helpers + grabFn(eco, "rankChip") + "; return rankChip;",
)();

console.log("── ecology: rank chip ─────────────────────────────────────");
check(
  "unranked specimen gets NO chip (not a zero)",
  rankChip({ evolution: { ranked: false, level: 0 } }) === "",
);
check("missing evolution block is tolerated", rankChip({}) === "");
const chip4 = rankChip({
  evolution: { ranked: true, level: 4, rank: "specialist" },
});
check("ranked specimen gets a chip with its level", chip4.includes(">4<"), chip4);
check("chip is titled with the rank name", chip4.includes('title="specialist"'));

console.log("");
console.log("── ecology: forecasting credential ────────────────────────");
check(
  "no record renders nothing at all",
  fcast({ forecasting: { n_resolved_forecasts: 0 } }) === "",
);

// The real World Cup shape: Brier looks superb, skill is what matters.
const wc = fcast({
  forecasting: {
    n_resolved_forecasts: 48,
    brier_mean: 0.0132,
    brier_baseline: 0.0204,
    brier_skill_score: 0.351,
    outcome_base_rate: 0.0208,
  },
});
console.log("      " + strip(wc));
check("skill is shown", wc.includes("0.351"));
check("baseline appears next to the raw Brier", wc.includes("0.0204"));
check("positive skill marked good", wc.includes("mono good"));
check("no misleading-Brier caveat when skill is positive", !wc.includes("one-sided"));

// Same flattering Brier, zero skill — the trap.
const skew = fcast({
  forecasting: {
    n_resolved_forecasts: 48,
    brier_mean: 0.0204,
    brier_baseline: 0.0204,
    brier_skill_score: 0.0,
    outcome_base_rate: 0.0208,
  },
});
check("zero skill marked bad", skew.includes("mono bad"));
check(
  "flattering Brier with no skill carries an explicit caveat",
  skew.includes("one-sided"),
);

// ── agent_detail.html: loadEvolution, through a stubbed fetch ───────────────
console.log("");
console.log("── agent detail badge ─────────────────────────────────────");

const detail = fs.readFileSync("templates/agent_detail.html", "utf8");
const loadEvoSrc = grabFn(detail, "loadEvolution");

async function renderDetail(payload) {
  const el = { innerHTML: "", textContent: "" };
  const escapeHtml = (s) =>
    String(s == null ? "" : s).replace(/&/g, "&amp;").replace(/</g, "&lt;");
  const document = {
    getElementById: (id) => (id === "evolution-body" ? el : null),
  };
  const fetch = async () => ({ ok: true, json: async () => payload });
  const fn = new Function(
    "document",
    "fetch",
    "escapeHtml",
    loadEvoSrc + "; return loadEvolution;",
  )(document, fetch, escapeHtml);
  await fn("some-agent");
  return el.innerHTML || el.textContent;
}

(async () => {
  const untried = await renderDetail({ ranked: false, status: "pending_usage_data" });
  check(
    "untried agent reads as pending, never as a rank",
    untried.includes("Unranked") && untried.includes("Untried, not failing"),
    strip(untried),
  );

  // Owner view of a real specialist, mirroring football_institution_agent.
  const owner = await renderDetail({
    ranked: true,
    level: 4,
    rank: "specialist",
    peak_level: 4,
    peak_rank: "specialist",
    regressed: false,
    next_step: "Get rules through verification.",
    dimensions: [
      { dimension: "memory", tier: 2, evidence: "1071 durable items, but no verified rules yet." },
      { dimension: "judgment", tier: 3, evidence: "Skill +0.35 over 48 forecasts." },
      { dimension: "conduct", tier: 1, evidence: "55 episodes observed with no anomaly raised." },
      { dimension: "craft", tier: 3, evidence: "Mean eval score 95% across 4 dimensions." },
    ],
    forecasting: {
      n_resolved_forecasts: 48,
      brier_mean: 0.0132,
      brier_baseline: 0.0204,
      brier_skill_score: 0.351,
    },
  });
  console.log("      " + strip(owner).slice(0, 210));
  check("rank name and level render", owner.includes("specialist") && owner.includes(">4<"));
  check("all four dimensions render", (owner.match(/evo-dim-name/g) || []).length === 4);
  check("pips reflect tiers (2+3+1+3 = 9 filled)", (owner.match(/evo-pip on/g) || []).length === 9);
  check("next step is surfaced", owner.includes("Get rules through verification"));
  check("public forecasting record is shown", owner.includes("0.351"));

  // Regression is owner-only and must be labelled as such.
  const regressed = await renderDetail({
    ranked: true,
    level: 2,
    rank: "fledgling",
    peak_level: 4,
    peak_rank: "specialist",
    regressed: true,
    next_step: "Restore judgment.",
    dimensions: [{ dimension: "memory", tier: 1, evidence: "thin." }],
    forecasting: {},
  });
  check("regression is shown to the owner", regressed.includes("Regressed"));
  check(
    "regression is explicitly marked private",
    regressed.includes("Visible to you only"),
    strip(regressed),
  );

  // A public viewer gets no peak and no regression field at all.
  const publicView = await renderDetail({
    ranked: true,
    level: 2,
    rank: "fledgling",
    next_step: "Keep going.",
    dimensions: [{ dimension: "memory", tier: 1, evidence: "thin." }],
    forecasting: {},
    visibility: "public",
  });
  check(
    "public view leaks neither regression nor peak",
    !publicView.includes("Regressed") && !publicView.includes("Peak"),
    strip(publicView),
  );

  console.log("");
  if (failures) {
    console.error(`${failures} check(s) failed`);
    process.exit(1);
  }
  console.log("all evolution render checks passed");
})();
