// Renders the dashboard's Loop 1 tile against representative fleet shapes,
// outside a browser, so the "unused vs unproductive" distinction can be
// verified without a deploy. Extracts the real function from the template so
// this cannot drift from what ships.
//
// Usage: node scripts/check_loop1_render.js
globalThis.window = globalThis;
const fs = require("fs");
const h = fs.readFileSync("templates/dashboard.html", "utf8");

function grab(name) {
  const i = h.indexOf("function " + name + "(l) {");
  if (i < 0) throw new Error("not found: " + name);
  let depth = 0, j = h.indexOf("{", i);
  do {
    if (h[j] === "{") depth++;
    if (h[j] === "}") depth--;
    j++;
  } while (depth > 0);
  return h.slice(i, j);
}

const helpers = `
function escHtml(s){return (s||"").replace(/&/g,"&amp;").replace(/</g,"&lt;");}
function escAttr(s){return (s||"").replace(/"/g,"&quot;");}
function loopAgentLabel(a){return escHtml(a.display_alias||a.agent_name||"");}
const STOP="";
`;

const renderLoop1 = new Function(
  "window",
  helpers + grab("renderLoop1") + "; return renderLoop1;"
)(globalThis);

// Shapes taken from the real fleet: mostly idle, a few productive, and the
// genuine fault we care about.
const out = renderLoop1({
  agents: [
    { agent_name: "anomaly_triager", maturity: "unused", needs_attention: false,
      unconsolidated_episodes: 0, days_since_dreaming: 51, ontology_size: 0 },
    { agent_name: "ar_avatar_renderer", maturity: "unused", needs_attention: false,
      unconsolidated_episodes: 0, days_since_dreaming: 51, ontology_size: 0 },
    { agent_name: "football_institution_agent", maturity: "mature", needs_attention: false,
      unconsolidated_episodes: 0, days_since_dreaming: 4, ontology_size: 1071 },
    { agent_name: "broken_agent", maturity: "unproductive", needs_attention: true,
      unconsolidated_episodes: 0, days_since_dreaming: 1, ontology_size: 0 },
  ],
});

const strip = (s) =>
  s.replace(/\s+/g, " ").replace(/<[^>]+>/g, "|").replace(/\|+/g, " | ").trim();

console.log("SUMMARY:", out.summary);
console.log("DETAIL :", strip(out.detail).slice(0, 260));
console.log("COUNT  :", out.count, "(agents flagged for attention)");

if (out.summary.includes("unused") && out.count === 1) {
  console.log("\nOK: unused agents are reported as idle, not as faults;");
  console.log("    only the genuinely unproductive agent needs attention.");
} else {
  console.error("\nFAIL: unused/unproductive not distinguished correctly");
  process.exit(1);
}
