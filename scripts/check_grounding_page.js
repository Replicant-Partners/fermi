#!/usr/bin/env node
//
// # "How this output was grounded", rendered in a real browser
//
// The page exists to make one thing legible: which checkpoints stood on the
// route an output took, what each one CAN do, and what it DID. Those are two
// questions, and a page that answers them with one colour is a diagram that
// lies about what the platform prevents.
//
// Properties held here, none of which a unit test can see:
//
// 1. Every checkpoint the route declares is drawn, in order.
// 2. The four enforcement modes render as four different nodes.
// 3. `undetermined` is its own badge, never folded into approved or refused,
//    and on the grounding rung it carries a fix naming the missing contract.
// 4. A correct absence (`fires_before_artifact`) is not drawn as a finding.
// 5. An unrecovered route says so.
// 6. Delegation hops link to their own grounding page.
//
//     node scripts/check_grounding_page.js
//
// Exits non-zero with a written reason for each failure.

const http = require("http");
const fs = require("fs");
const path = require("path");
const { launch, connect, openPage } = require("./cdp.js");

const ROOT = path.resolve(__dirname, "..");
const FAIL = [];
const ok = (cond, msg) => { if (!cond) FAIL.push(msg); };

const EPISODE = "11111111-2222-3333-4444-555555555555";
const CHILD = "99999999-2222-3333-4444-555555555555";

const TRACE = {
  episode_id: EPISODE,
  agent: { id: "a", name: "weather_oracle" },
  input: { query: "Will it rain in Lyon tomorrow?" },
  checkpoint_route: { command: "workspace.message", label: "Ask agent in workspace",
    route: "POST /api/workspaces/:workspace_id/messages", recovered: false,
    source_kind: null, because: "no source_ref" },
  checkpoints: [
    { rung: "credit", clock: "invocation", enforcement: "control", why_not_control: null,
      refuses: "an action whose principal cannot pay for it", site: "x::y",
      decided_absent: { token: "fires_before_artifact", because: "decides before the artifact exists" } },
    { rung: "grounding", clock: "invocation", enforcement: "amend",
      why_not_control: "Cannot refuse: grounding is unknowable before the model writes.",
      refuses: "a field no tool could supply", site: "x::y",
      decided: { decision: "undetermined", reason: null, at: null, decision_id: 7 } },
    { rung: "output_schema", clock: "invocation", enforcement: "report",
      why_not_control: "Delivered labelled.", refuses: "a contradicting document", site: "x::y",
      decided: { decision: "refused", reason: "weather_oracle: invalid at /p", at: null, decision_id: 8 } },
    { rung: "input_binding", clock: "invocation", enforcement: "metric",
      why_not_control: "Advisory.", refuses: "nothing", site: "x::y",
      decided_absent: { token: "retention_counted", because: "counted in memory only" } },
  ],
  // The rows from the first production run, where the page read wrong.
  fields: [
    { name: "phylogeny.divergence_mya", value: 45, grade: "tool_verified", strength: 2,
      kind: "unsourced", observed: "stripped", whose: "the agent's", finding_tone: "red",
      settleable_by: null },
    { name: "phylogeny.superorder", value: "Holometabola", grade: "tool_verified", strength: 2,
      kind: "derived", observed: "filled", whose: "nobody's", finding_tone: "neutral",
      settleable_by: null },
    { name: "summary", value: "A stag beetle", grade: "unavailable_no_tool_source", strength: 0,
      kind: "narrative", observed: "filled", whose: "nobody's", finding_tone: "neutral",
      settleable_by: null },
    { name: "genome.chromosome_count", value: null, grade: "tool_no_match", strength: 0,
      kind: "sourced", observed: "tool_empty", whose: "the world's", finding_tone: "amber",
      settleable_by: "ncbi_genome_search" },
  ],
  substrate: { disposition: "retrofit" },
  owner: "agent_author",
};

const LINEAGE = {
  parent: null,
  children: [{ episode_id: CHILD, agent: "weather_calibrator" }],
  delivered: [{ workspace: "Lyon ops", workspace_id: "w" }],
};

const API = (p) => {
  if (p.endsWith("/trace")) return TRACE;
  if (p.endsWith("/lineage")) return LINEAGE;
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
    else if (p.startsWith("/grounding/")) file = path.join(ROOT, "templates/grounding.html");
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
  let chrome, cdp, seen = null;
  try {
    chrome = await launch();
    cdp = await connect(chrome.wsUrl);
    const page = await openPage(cdp);
    await page.goto(`http://127.0.0.1:${port}/grounding/${EPISODE}`);

    ok(page.exceptions.length === 0, "the page threw:\n      " + page.exceptions.join("\n      "));

    seen = await page.eval(() => {
      const txt = (el) => (el ? el.textContent.replace(/\s+/g, " ").trim() : "");
      return {
        nodes: Array.from(document.querySelectorAll(".path .node:not(.end)")).map((n) => ({
          cls: Array.from(n.classList).filter((c) => c !== "node").join(" "),
          gate: txt(n.querySelector(".gate")),
          badge: txt(n.querySelector(".badge")),
          badgeCls: n.querySelector(".badge") ? n.querySelector(".badge").className : "",
          fix: txt(n.querySelector(".fix")),
        })),
        route: txt(document.getElementById("route")),
        verdict: txt(document.querySelector(".verdict .said")),
        links: Array.from(document.querySelectorAll(".hops a")).map((a) => a.getAttribute("href")),
        rows: Object.fromEntries(Array.from(document.querySelectorAll("table tr")).slice(1).map((tr) => {
          const td = Array.from(tr.querySelectorAll("td")).map(txt);
          return [td[0], { source: td[2], state: td[3], next: td[4] }];
        })),
        body: txt(document.body).slice(0, 500),
      };
    });

    ok(seen.nodes.length === 4,
      `expected 4 checkpoints, drew ${seen.nodes.length}: a route that drops checkpoints looks safer than it is`);
    ok(new Set(seen.nodes.map((n) => n.cls)).size === 4,
      `the four enforcement modes are not drawn as four nodes: ${JSON.stringify(seen.nodes.map((n) => n.cls))}`);

    const g = seen.nodes.find((n) => n.gate === "grounding") || {};
    ok(/undetermined/.test(g.badgeCls) && !/approved|refused/.test(g.badgeCls),
      `undetermined was folded into a neighbour: ${g.badgeCls}`);
    ok(/output_contract/.test(g.fix),
      "an undetermined grounding rung does not say what would fix it");
    ok(/nothing about this output's content could be checked/i.test(seen.verdict),
      `an unchecked output is summarised as something else: "${seen.verdict}"`);

    const c = seen.nodes.find((n) => n.gate === "credit") || {};
    ok(!/finding/.test(c.badgeCls),
      "a gate that decides before the output exists is drawn as a missing record");

    ok(/not recovered/.test(seen.route), "an unrecovered route is not marked as such");

    const r = seen.rows;
    const d = r["phylogeny.divergence_mya"] || {};
    ok(!/tool_verified/.test(d.source) && /no tool can supply/.test(d.source),
      `a stripped unsourced field reads as tool-verified because its block was: "${d.source}"`);
    ok(/removed/.test(d.state), `a stripped field does not say it was removed: "${d.state}"`);
    const s = r["phylogeny.superorder"] || {};
    ok(/computed/.test(s.source) && !/citation/.test(s.next),
      `a derived field asks for a citation or hides that ABW computed it: ${JSON.stringify(s)}`);
    ok(!/citation/.test((r["summary"] || {}).next || ""), "prose asks a person for a citation");
    const cc = r["genome.chromosome_count"] || {};
    ok(/no data/.test(cc.state) && /ncbi_genome_search/.test(cc.next),
      `the world's gap is not said plainly: ${JSON.stringify(cc)}`);
    ok(!Object.values(r).some((x) => /nobody's|\u00b7/.test(x.state)),
      "a state cell still prints the bare attribution token");
    ok(seen.links.includes(`/grounding/${CHILD}`),
      "a delegation hop does not link to the child's own grounding page");
  } finally {
    try { if (cdp) cdp.close(); } catch (_) {}
    try { if (chrome) await chrome.close(); } catch (_) {}
    server.close();
  }

  if (FAIL.length) {
    console.error(`\n${FAIL.length} failure(s):\n`);
    FAIL.forEach((f) => console.error("  \u2716 " + f + "\n"));
    if (seen) console.error("rendered: " + seen.body + "\n");
    process.exit(1);
  }
  console.log("OK  every checkpoint drawn, four modes distinct, undetermined kept apart.");
}

main().catch((e) => {
  console.error("harness error: " + (e && e.stack ? e.stack : e));
  process.exit(2);
});
