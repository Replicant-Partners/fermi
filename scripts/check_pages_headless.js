// Load the pages in a real browser and see whether they work.
//
// ## The gap this closes
//
// Three UI bugs in a row reached the user's screen and were found by
// screenshot, not by a test:
//
//   1. `tabs.js` pairs buttons to panels BY INDEX. Adding a button mid-list
//      with its panel at the end shifted five tabs — Contract showed
//      Economics, Intelligence showed Contract.
//   2. A new standalone page did not link `common.css` and `components.css`,
//      so nav.js and every primitive rendered unstyled.
//   3. A backtick inside an HTML comment inside a JS template literal ended
//      the string. `Uncaught SyntaxError: Unexpected identifier 'Tabs'`, blank
//      page.
//
// Each got a test afterwards. None of those tests would have caught the other
// two, and all three are static: `tests/agent_detail_tabs.rs` reads the markup,
// `tests/inline_js_syntax.rs` parses the scripts. The DOM-stub harness
// (`scripts/check_contract_builder.js`) executes JavaScript but has no layout,
// no stylesheets, and never loads a page.
//
// So: an actual browser, actual CSS, actual `getComputedStyle`. The three
// classes above become three assertions that do not depend on knowing which
// bug to look for —
//
//   * nothing throws and nothing logs an error;
//   * every stylesheet and script the page asks for arrives, and the page is
//     demonstrably styled (a computed background, not the browser default);
//   * clicking each tab shows the panel that tab NAMES, checked by attribute.
//
// ## Hermetic on purpose
//
// The pages under test are served by `app_shell`, which reads the template off
// disk with no interpolation, so a static server is not an approximation of
// production — it is the same bytes. The API is stubbed. That keeps this
// runnable on a clean checkout with no database, which matters because the
// real one is a remote Neon instance and a UI check that needs production to
// answer is a UI check nobody runs.
//
// Requires Google Chrome or Chromium on PATH. No npm dependencies: see
// `scripts/cdp.js` for why.
//
// Usage: node scripts/check_pages_headless.js [tool-shapes.json]

const http = require("http");
const fs = require("fs");
const path = require("path");
const { findChrome, launch, connect, openPage } = require("./cdp.js");

const ROOT = path.join(__dirname, "..");
const FAIL = [];
const ok = (c, what) => {
  if (!c) FAIL.push(what);
};

// ── fixtures ─────────────────────────────────────────────────────────────
//
// Real where it is cheap to be real. `/api/contracts/tools` is handed in by
// tests/pages_headless.rs from `tool_response_shapes::declared_shapes_json()`,
// the same function the live endpoint calls.
const shapesArg = process.argv[2];
const TOOL_PAYLOAD = shapesArg
  ? JSON.parse(fs.readFileSync(shapesArg, "utf8"))
  : { tools: [], response_shapes: [] };
const HAVE_SHAPES = (TOOL_PAYLOAD.response_shapes || []).length > 0;

const AGENT_ID = "genome_profiler";

const AGENT = {
  agent_name: AGENT_ID,
  display_name: "Genome Profiler",
  description: "Assembles a genome profile for a named taxon.",
  agent_type: "specialist",
  tier: "curated",
  status: "active",
  visibility: "public",
  owner_id: "00000000-0000-0000-0000-000000000001",
  owner_name: "ilabra",
  produces: ["genome_profile"],
  accepts: ["species_identity"],
  tags: ["biology", "genomics"],
  capabilities: {
    mcp_tools: [
      { name: "ncbi_genome_search", description: "Assembly lookup." },
      { name: "gbif_species_search", description: "Taxon resolution." },
    ],
    output_contract: null,
  },
  system_prompt: "You profile genomes.",
  model: "claude-sonnet-4",
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

const USER = {
  id: "00000000-0000-0000-0000-000000000001",
  username: "ilabra",
  email: "ilabra@example.invalid",
  role: "admin",
  is_admin: true,
};

const unfixtured = new Set();

// Paths that must answer 404 rather than an empty 200. `/avatar` reads
// `data.image.mime_type` off any 2xx, so a stub that returns `{}` makes the
// page throw for a reason that is the harness's fault and not the page's.
const NOT_FOUND = ["/avatar"];

function apiFixture(pathname) {
  const p = pathname;
  if (p === "/api/auth/me") return USER;
  if (p === "/api/secrets") return { secrets: [] };
  if (p === "/api/orchestras") return { orchestras: [] };
  if (p === "/api/agents") return { agents: [AGENT] };
  if (p === "/api/agents/mine") return { agents: [AGENT] };
  if (p === "/api/contracts/tools") return TOOL_PAYLOAD;
  if (p === "/api/contracts/types") return { types: [] };
  if (p === "/api/contracts/suggest") return { proposals: [] };
  if (p === "/api/contracts/compile")
    return { ok: false, errors: [], contract: null };
  // `genome_profiler` has no contract yet — it is the agent the whole feature
  // was built for and has not been migrated. That is also the state the
  // builder is most often opened in, so it is the one worth loading.
  if (p.startsWith("/api/contracts/decompile/"))
    return {
      agent_id: AGENT_ID,
      has_contract: false,
      sketch: null,
      tool_names: AGENT.capabilities.mcp_tools.map((t) => t.name),
      produces: AGENT.produces,
      findings: [],
    };
  if (p === "/api/agents/" + AGENT_ID) return AGENT;
  if (p === "/api/agents/" + AGENT_ID + "/orchestras") return { orchestras: [] };
  // Deliberately NOT fixtured: see `noAvatar` below. Most agents have no
  // cached avatar and the endpoint 404s, which is the path the page takes in
  // the common case and therefore the one worth loading.
  if (p === "/api/agents/" + AGENT_ID + "/ontology") return { ontology: null };
  if (p === "/api/agents/" + AGENT_ID + "/embeddings/stats")
    return { total: 0, by_kind: [] };
  if (p === "/api/agents/" + AGENT_ID + "/projections")
    return { points: [], dimensions: 3 };

  // Anything else answers 200 with an empty object and is reported. A 404
  // would make the page log an error that is the harness's fault, not the
  // page's, and an unreported stub would hide a real endpoint the page needs.
  unfixtured.add(p);
  return {};
}

const MIME = {
  ".html": "text/html; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".ico": "image/x-icon",
  ".woff2": "font/woff2",
};

function serve() {
  const server = http.createServer((req, res) => {
    const url = new URL(req.url, "http://127.0.0.1");
    const p = url.pathname;

    if (p.startsWith("/api/")) {
      if (NOT_FOUND.some((s) => p.endsWith(s))) {
        res.writeHead(404, { "content-type": "application/json" });
        return res.end('{"error":"not found"}');
      }
      const body = JSON.stringify(apiFixture(p));
      res.writeHead(200, {
        "content-type": "application/json",
        "content-length": Buffer.byteLength(body),
      });
      return res.end(body);
    }

    // `static/` is served with correct content types on purpose. A stylesheet
    // sent as text/plain is ignored by the browser and the page renders
    // unstyled — the same symptom as a missing <link>, which is one of the
    // bugs this file exists to catch. Getting it wrong here would fake it.
    let file = null;
    if (p.startsWith("/static/")) file = path.join(ROOT, p.slice(1));
    else if (p === "/contracts") file = path.join(ROOT, "templates/contract_builder.html");
    else if (p.startsWith("/agent/")) file = path.join(ROOT, "templates/agent_detail.html");
    else if (p === "/") file = path.join(ROOT, "templates/index.html");

    if (!file || !fs.existsSync(file) || fs.statSync(file).isDirectory()) {
      res.writeHead(404, { "content-type": "text/plain" });
      return res.end("not found: " + p);
    }
    const buf = fs.readFileSync(file);
    res.writeHead(200, {
      "content-type": MIME[path.extname(file)] || "application/octet-stream",
      "content-length": buf.length,
    });
    res.end(buf);
  });
  return new Promise((resolve) =>
    server.listen(0, "127.0.0.1", () =>
      resolve({ server, port: server.address().port }),
    ),
  );
}

// ── assertions shared by every page ──────────────────────────────────────

function checkClean(page, label) {
  ok(
    page.exceptions.length === 0,
    `${label}: ${page.exceptions.length} uncaught exception(s). A page that throws during ` +
      `load renders nothing after the throw, and the only symptom is a blank area:\n      ` +
      page.exceptions.join("\n      "),
  );
  ok(
    page.consoleErrors.length === 0,
    `${label}: ${page.consoleErrors.length} console error(s):\n      ` +
      page.consoleErrors.join("\n      "),
  );

  const badAssets = page.responses.filter(
    (r) =>
      r.status >= 400 &&
      (r.type === "Stylesheet" || r.type === "Script" || r.type === "Document"),
  );
  ok(
    badAssets.length === 0,
    `${label}: ${badAssets.length} stylesheet/script did not load. A standalone page that ` +
      `forgets common.css renders every primitive unstyled and throws nothing:\n      ` +
      badAssets.map((r) => r.status + " " + r.url).join("\n      "),
  );
  const netFail = page.failures.filter((f) => !/favicon/.test(f.url));
  ok(
    netFail.length === 0,
    `${label}: ${netFail.length} request(s) failed at the network layer:\n      ` +
      netFail.map((f) => f.error + " " + f.url).join("\n      "),
  );
}

// The stylesheets every page in this app needs, whatever else it links.
//
// `variables.css` declares the palette, `common.css` the page chrome and the
// nav, `components.css` every primitive. The bug was a new standalone page
// that linked only the first and the page-specific one: the palette resolved,
// the page had a background, and every button and the whole nav rendered as
// unstyled browser defaults. Counting stylesheets does not catch that — the
// count was still four. Naming them does.
const REQUIRED_CSS = ["variables.css", "common.css", "components.css"];

async function checkStyled(page, label) {
  const style = await page.eval(() => {
    const cs = getComputedStyle(document.body);
    const root = getComputedStyle(document.documentElement);
    const sheetHrefs = Array.from(document.styleSheets)
      .map((s) => {
        try {
          return s.href || "(inline)";
        } catch {
          return "(opaque)";
        }
      })
      .map((h) => h.split("/").pop());
    // A primitive from each of the two shared sheets, measured rather than
    // assumed. Loading a stylesheet is not the same as its rules applying: one
    // served with the wrong content type 200s, shows up in the <link>, and is
    // silently ignored.
    //
    // `.nav-header` is common.css and is injected by nav.js, so this doubles
    // as proof the nav ran at all.
    const probe = (sel, props) => {
      const el = document.querySelector(sel);
      if (!el) return null;
      const cs = getComputedStyle(el);
      const out = { sel };
      props.forEach((p) => (out[p] = cs.getPropertyValue(p)));
      return out;
    };
    return {
      sheetHrefs,
      // Whichever components.css primitive this page actually uses.
      prim:
        probe(".btn", ["border-top-width", "border-top-style"]) ||
        probe(".stat-card", ["border-top-width", "border-top-style"]) ||
        probe(".section-title", ["text-transform", "letter-spacing"]),
      nav: probe(".nav-header", ["display"]),
      sheets: document.styleSheets.length,
      vars: {
        bg0: root.getPropertyValue("--bg0").trim(),
        fg1: root.getPropertyValue("--fg1").trim(),
      },
      // A stylesheet that 200s but fails to parse still counts in
      // `styleSheets`, so read a value only the theme sets.
      bg: cs.backgroundColor,
      font: cs.fontFamily,
      themed: document.documentElement.className,
      bodyHeight: document.body.getBoundingClientRect().height,
      text: (document.body.innerText || "").trim().length,
    };
  });

  const missing = REQUIRED_CSS.filter((n) => !style.sheetHrefs.includes(n));
  ok(
    missing.length === 0,
    `${label}: does not load ${missing.join(", ")}. A standalone page that links only ` +
      `variables.css and its own sheet still has a background and a palette, so it looks ` +
      `nearly right — and every button and the entire nav render as browser defaults. ` +
      `Attached: ${style.sheetHrefs.join(", ")}`,
  );

  ok(
    style.prim !== null,
    `${label}: none of .btn / .stat-card / .section-title is on the page, so the ` +
      `components.css spot check ran on nothing and proved nothing`,
  );
  if (style.prim) {
    const p = style.prim;
    const applied =
      p.sel === ".section-title"
        ? p["text-transform"] === "uppercase"
        : p["border-top-width"] === "1px" && p["border-top-style"] === "solid";
    ok(
      applied,
      `${label}: ${p.sel} does not compute what components.css declares ` +
        `(${JSON.stringify(p)}). The sheet is linked but its rules are not applying — a wrong ` +
        `content type does exactly this, and the page looks almost right`,
    );
  }
  ok(
    style.nav !== null,
    `${label}: no .nav-header on the page — nav.js did not run, or common.css is absent`,
  );
  if (style.nav) {
    ok(
      style.nav.display === "flex",
      `${label}: .nav-header computes display:${style.nav.display}. common.css declares flex, ` +
        `so the nav is stacking as unstyled block elements`,
    );
  }
  ok(
    /theme-/.test(style.themed),
    `${label}: <html> carries no theme class (got "${style.themed}"). Every CSS variable ` +
      `is scoped to one, so without it the whole palette falls back to browser defaults`,
  );
  ok(
    style.bg !== "rgba(0, 0, 0, 0)" && style.bg !== "rgb(255, 255, 255)",
    `${label}: body background is "${style.bg}", the browser default. The variables did not ` +
      `resolve — this is what an unstyled page computes to`,
  );
  ok(
    style.vars.bg0 !== "" && style.vars.fg1 !== "",
    `${label}: the theme custom properties did not resolve (--bg0="${style.vars.bg0}", ` +
      `--fg1="${style.vars.fg1}"). Either variables.css is missing or the theme class on ` +
      `<html> does not match the selector they are declared under, and every colour in the ` +
      `page silently falls back`,
  );
  ok(
    style.bodyHeight > 200 && style.text > 200,
    `${label}: the page rendered ${Math.round(style.bodyHeight)}px tall with ${style.text} ` +
      `characters of text. That is a blank page`,
  );
  return style;
}

// ── /contracts ───────────────────────────────────────────────────────────

async function checkContractsPage(cdp, base) {
  const page = await openPage(cdp);
  await page.goto(base + "/contracts?agent=" + AGENT_ID);
  checkClean(page, "/contracts");
  await checkStyled(page, "/contracts");

  const built = await page.eval(() => {
    const shape = document.getElementById("cb-shape");
    // Whichever view the widget chose to open on — it opens onto the one that
    // owes something, so pinning view 1 here would be asserting the wrong
    // thing about a contract that already has gaps.
    const shown = [1, 2, 3]
      .map((n) => document.getElementById("cb-view-" + n))
      .filter((el) => el && el.getBoundingClientRect().height > 0);
    return {
      mounted: !!shape,
      visibleViews: shown.length,
      // Visible, not merely present. `display:none` and a zero-height box are
      // how a widget "renders" and still is not there.
      visibleHeight: shown.length ? shown[0].getBoundingClientRect().height : 0,
      views: document.querySelectorAll(".cb-viewbtn").length,
      status: (document.getElementById("cb-status") || {}).innerText || "",
    };
  });
  ok(built.mounted, "/contracts: the builder never mounted (#cb-shape is absent)");
  ok(
    built.visibleViews === 1,
    `/contracts: ${built.visibleViews} view(s) are visible at once, expected exactly one`,
  );
  ok(
    built.visibleHeight > 100,
    `/contracts: the open view is ${Math.round(built.visibleHeight)}px tall, which on screen ` +
      `is an empty page under a heading`,
  );
  ok(
    built.views >= 3,
    `/contracts: ${built.views} view button(s) rendered, expected the three views`,
  );
  // The status chip is where a throw inside the debounced compile hides: it
  // simply never leaves "Compiling…". That is how `cbSketch` reading an
  // element this page does not have went unnoticed.
  ok(
    !/Compiling/i.test(built.status),
    `/contracts: the status chip is stuck on "${built.status}". The compile never came back, ` +
      `which is what an exception inside the debounced handler looks like from outside`,
  );

  if (!HAVE_SHAPES) {
    console.log(
      "note: no tool response shapes were passed in, so the picker and " +
        "reverse-lookup checks did not run. Not a pass — run this through " +
        "`cargo test --test pages_headless`, which supplies the real table.",
    );
    await page.close();
    return;
  }

  // The affordance this page exists for, in a real layout. The DOM stub can
  // prove the markup contains a picker; only a browser can prove it is on
  // screen with a non-zero box.
  const picker = await page.eval(() => {
    // Drive it the way a person would: add a part, source it from a tool
    // whose response shape is declared, open it.
    cbAddBlock();
    const b = ContractBuilder.blocks[0];
    b.name = "identity";
    cbSetStatus(0, "sourced");
    cbSet(0, "tool", "gbif_species_search");
    cbToggle("identity");
    // The grounding cards live in view 2. Measuring a box inside a hidden view
    // returns zero for a reason that is not the bug.
    cbSetView(2);
    const grid = document.querySelector(".cb-shape-grid");
    const fields = grid ? grid.querySelectorAll(".cb-shape-f") : [];
    const r = grid ? grid.getBoundingClientRect() : null;
    return {
      present: !!grid,
      count: fields.length,
      height: r ? r.height : 0,
      firstVisible: fields.length
        ? fields[0].getBoundingClientRect().height > 0
        : false,
    };
  });
  ok(
    picker.present,
    "/contracts: no response-shape picker for a tool whose shape is declared. The shapes " +
      "arrive after mount, so this is the redraw regression, seen in a browser",
  );
  ok(
    picker.count > 5,
    `/contracts: the picker offered ${picker.count} field(s) for gbif_species_search`,
  );
  ok(
    picker.height > 0 && picker.firstVisible,
    `/contracts: the picker is in the DOM with height ${picker.height}px. Present and ` +
      `invisible is the same as absent, and only a browser can tell them apart`,
  );

  // The reverse lookup, styled. `.cb-src-hint` was added with its CSS in the
  // same commit; a rule that never landed would show here as an unstyled row.
  const hint = await page.eval(() => {
    cbAddField(0);
    const j = ContractBuilder.blocks[0].fields.length - 1;
    cbSetField(0, j, "name", "estimated_size_mb");
    cbRenderAll();
    // The field rows, and so the hints under them, are view 1.
    cbSetView(1);
    const h = document.querySelector(".cb-src-hint");
    const hit = document.querySelector(".cb-src-hit");
    if (!h || !hit) return { present: false };
    return {
      present: true,
      text: h.innerText,
      height: h.getBoundingClientRect().height,
      hitColor: getComputedStyle(hit).color,
      hitBorder: getComputedStyle(hit).borderTopWidth,
    };
  });
  // Compile, which is what happens on every keystroke through the debounce.
  //
  // Loading an agent with no contract takes an early return that never
  // compiles, so a page whose compile throws still looks fine on arrival. That
  // is how `cbSketch` reading `#agent-name` — an element only the wizard has —
  // survived: the throw was inside a debounced handler, so the only symptom
  // was a status chip that stayed on "Compiling…", and nothing reaches it
  // until the author edits something.
  const compiled = await page.eval(async () => {
    let err = null;
    let title = null;
    try {
      title = ContractBuilder.sketch.title || null;
      await ContractBuilder.compile();
    } catch (e) {
      err = String((e && e.message) || e);
    }
    const chip = document.getElementById("cb-status");
    return { err, title, status: chip ? chip.innerText.trim() : "" };
  });
  ok(
    compiled.err === null,
    `/contracts: compiling threw "${compiled.err}". The widget is mounted by the wizard and ` +
      `by this page; an element only one host has is null on the other, and every compile ` +
      `and every keystroke goes through here`,
  );
  ok(
    !/Compiling/i.test(compiled.status),
    `/contracts: the status chip is stuck on "${compiled.status}" after a compile. That is ` +
      `what a throw inside the debounced handler looks like from outside`,
  );
  // Not merely "did not throw". The title reaches the compiled contract, so a
  // page that quietly produced none would make the same agent compile to a
  // different document depending on where you opened it.
  ok(
    compiled.title === AGENT_ID.replace(/_/g, " "),
    `/contracts: the sketch title is ${JSON.stringify(compiled.title)}, expected ` +
      `"${AGENT_ID.replace(/_/g, " ")}". This page has no name input, so the title has to come ` +
      `from the agent it was opened for — dropping it silently is worse than throwing`,
  );

  ok(hint.present, "/contracts: the reverse-lookup hint did not render in the browser");
  if (hint.present) {
    ok(
      /ncbi_genome_search/.test(hint.text),
      `/contracts: the hint does not name ncbi_genome_search. It reads: ${hint.text}`,
    );
    ok(
      hint.height > 0,
      "/contracts: the hint has zero height, so it is invisible to the author it is for",
    );
    ok(
      hint.hitBorder !== "0px" && hint.hitColor !== "rgb(0, 0, 0)",
      `/contracts: the hit button is unstyled (colour ${hint.hitColor}, border ` +
        `${hint.hitBorder}) — .cb-src-hit did not reach the page`,
    );
  }

  await page.close();
}

// ── /agent/:id ───────────────────────────────────────────────────────────

async function checkAgentPage(cdp, base) {
  const page = await openPage(cdp);
  await page.goto(base + "/agent/" + AGENT_ID);
  checkClean(page, "/agent/:id");
  await checkStyled(page, "/agent/:id");

  // THE tab bug, tested by behaviour rather than by reading the markup.
  //
  // `tabs.js` pairs buttons to panels by index and ignores `data-tab`. A
  // button added mid-list with its panel appended at the end therefore shows
  // someone else's content, and every static check of the markup passes:
  // the button is there, the panel is there, both spellings are right.
  // The only way to see it is to click and look at what appears.
  const tabs = await page.eval(() => {
    const btns = Array.from(document.querySelectorAll("[data-tab]"));
    if (!btns.length) return { none: true };
    const out = [];
    for (const b of btns) {
      b.click();
      const want = b.getAttribute("data-tab");
      const shown = Array.from(document.querySelectorAll("[data-tab-panel]")).filter(
        (p) => p.getBoundingClientRect().height > 0,
      );
      out.push({
        want,
        label: (b.innerText || "").trim().slice(0, 24),
        got: shown.map((p) => p.getAttribute("data-tab-panel")),
      });
    }
    return { none: false, out };
  });

  if (tabs.none) {
    ok(false, "/agent/:id: no tab buttons found, so the pairing check ran on nothing");
  } else {
    const mismatched = tabs.out.filter(
      (t) => !(t.got.length === 1 && t.got[0] === t.want),
    );
    ok(
      mismatched.length === 0,
      `/agent/:id: ${mismatched.length} of ${tabs.out.length} tab(s) show the wrong panel. ` +
        `tabs.js pairs by INDEX and ignores data-tab, so inserting a button mid-list ` +
        `shifts everything after it:\n      ` +
        mismatched
          .map((t) => `"${t.label}" (data-tab=${t.want}) showed [${t.got.join(", ")}]`)
          .join("\n      "),
    );
  }

  // The Contract tab links out to the standalone builder rather than
  // embedding it; if that link rots the feature is unreachable from the one
  // page an owner actually opens.
  const link = await page.eval(
    (id) => document.body.innerHTML.includes("/contracts?agent="),
    AGENT_ID,
  );
  ok(link, "/agent/:id: nothing links to /contracts?agent=<id>, so the builder is unreachable");

  await page.close();
}

// ── run ──────────────────────────────────────────────────────────────────

async function main() {
  if (!findChrome()) {
    // Not a pass. The same distinction the contracts themselves make between
    // `unverified` and `valid`.
    console.log(
      "SKIPPED: no Chrome or Chromium found, so the pages were not loaded in a " +
        "browser. This is an absence of a check, not a passing one.",
    );
    process.exit(0);
  }

  const { server, port } = await serve();
  const base = "http://127.0.0.1:" + port;
  const chrome = await launch();
  const cdp = await connect(chrome.wsUrl);

  try {
    await checkContractsPage(cdp, base);
    await checkAgentPage(cdp, base);
  } finally {
    cdp.close();
    await chrome.close();
    server.close();
  }

  if (unfixtured.size) {
    console.log(
      "note: " +
        unfixtured.size +
        " API path(s) answered with an empty stub:\n  " +
        Array.from(unfixtured).sort().join("\n  "),
    );
  }

  if (FAIL.length) {
    console.error(`\n${FAIL.length} failure(s):`);
    FAIL.forEach((f) => console.error("  x " + f));
    process.exit(1);
  }
  console.log("pages in a real browser: all checks pass");
}

main().catch((e) => {
  if (e && e.message === "NO_CHROME") {
    console.log("SKIPPED: no Chrome or Chromium found.");
    process.exit(0);
  }
  console.error("harness threw: " + (e && e.stack ? e.stack : e));
  process.exit(1);
});
