// Exercise the contract builder's tool-driven field affordances headlessly.
//
// Three UI bugs in this widget in a row were found only by a screenshot: tabs
// paired to panels by index, a page missing its stylesheets, and a backtick
// that ended a template literal. Two of those are now pinned by tests. The
// third class — code that runs, renders nothing, and looks fine in the source
// — is what this file is for.
//
// The specific regression it pins: the declared response shapes are fetched
// AFTER mount() has already rendered. They landed in the widget's state and
// nothing redrew, so the field picker never appeared and the reverse-lookup
// hints never fired. The data was correct and invisible, which no syntax check
// can see.
//
// What is asserted:
//
//   * the picker appears on its own once the tools fetch resolves, with no
//     manual re-render — the regression above;
//   * the reverse lookup finds `estimated_size_mb` in `ncbi_genome_search`
//     from a field NAMED that in a block sourced from somewhere else. That is
//     the genome_profiler shape: a retrieved block carrying a number its tool
//     does not return;
//   * a name two tools return offers BOTH, each with its tool and its full
//     path, and picks neither. Guessing there would be the platform making a
//     grounding claim for the author;
//   * adopting a source writes status, tool, type and response_field, and
//     leaves `why` alone. `why` is the one field the compiler refuses to
//     write; a UI that filled it in would break that rule from the other side.
//
// The shapes are not a fixture invented here. They are the real
// TOOL_RESPONSES table, serialised by tests/contract_builder_headless.rs
// exactly as /api/contracts/tools serialises it, and passed in as a file. A
// hand-written fixture would let this test keep passing about a tool whose
// response had changed.
//
// Usage: node scripts/check_contract_builder.js <shapes.json>
//        (tests/contract_builder_headless.rs does this for you)

const fs = require("fs");
const path = require("path");

const FAIL = [];
const ok = (c, what) => {
  if (!c) FAIL.push(what);
};

// ── the payload /api/contracts/tools would return ────────────────────────
const payloadPath = process.argv[2];
if (!payloadPath) {
  console.error("usage: node scripts/check_contract_builder.js <shapes.json>");
  process.exit(2);
}
const PAYLOAD = JSON.parse(fs.readFileSync(payloadPath, "utf8"));
const SHAPES = PAYLOAD.response_shapes || [];

const shapeFor = (tool) => SHAPES.find((s) => s.tool === tool);
// If the table stops declaring these, the assertions below would pass
// vacuously by finding nothing to check.
ok(!!shapeFor("gbif_species_search"), "gbif_species_search has no declared response shape, so the picker assertions below check nothing");
ok(!!shapeFor("ncbi_genome_search"), "ncbi_genome_search has no declared response shape, so the reverse-lookup assertions below check nothing");

// ── DOM stub ─────────────────────────────────────────────────────────────
//
// Every id resolves to an element, because the widget's renders bail on a
// missing one and a stub that returned null would make this file assert
// nothing while reporting success.
function El(id) {
  this.id = id;
  this._html = "";
  this._text = "";
  this.value = "";
  this.disabled = false;
  this.style = {};
  this.classList = {
    toggle: () => false,
    add: () => {},
    remove: () => {},
    contains: () => false,
  };
}
// `textContent` in, `innerHTML` out, escaped. Not a nicety: the widget's
// `esc()` IS this round trip —
//
//   const d = document.createElement("div"); d.textContent = str;
//   return d.innerHTML.replace(/"/g, "&quot;");
//
// so a stub without it returns the empty string for every escaped value and
// the whole page renders with blank names. That is not a hypothetical: it is
// what this harness did on its first run, and every assertion failed for a
// reason that had nothing to do with the widget.
Object.defineProperty(El.prototype, "innerHTML", {
  get() {
    return this._html;
  },
  set(v) {
    this._html = String(v);
    this._text = "";
  },
});
Object.defineProperty(El.prototype, "textContent", {
  get() {
    return this._text;
  },
  set(v) {
    this._text = String(v);
    this._html = String(v)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  },
});
El.prototype.querySelector = () => null;
El.prototype.appendChild = () => {};
El.prototype.removeChild = () => {};
El.prototype.setAttribute = () => {};
El.prototype.focus = () => {};
El.prototype.select = () => {};

const REG = {};
const byId = (id) => REG[id] || (REG[id] = new El(id));
globalThis.document = {
  getElementById: byId,
  createElement: () => new El("created"),
  body: new El("body"),
};
globalThis.navigator = { clipboard: { writeText: async () => {} } };
globalThis.location = { search: "" };

// ── fetch stub, with the tools call held open on purpose ─────────────────
//
// Holding it lets the test observe the widget's state BEFORE the shapes
// arrive, which is the only way to prove the redraw afterwards is what makes
// the picker appear.
let releaseTools;
const toolsGate = new Promise((r) => {
  releaseTools = r;
});
const jsonResp = (obj) => ({
  ok: true,
  status: 200,
  json: async () => obj,
  text: async () => JSON.stringify(obj),
});
const deadResp = {
  ok: false,
  status: 503,
  json: async () => ({}),
  text: async () => "no server in this harness",
};
globalThis.fetch = async (url) => {
  const u = String(url);
  if (u.includes("/api/contracts/tools")) {
    await toolsGate;
    return jsonResp(PAYLOAD);
  }
  return deadResp;
};

const unhandled = [];
process.on("unhandledRejection", (e) => unhandled.push(String(e)));

// ── load ─────────────────────────────────────────────────────────────────
const src = fs.readFileSync(
  path.join(__dirname, "..", "static", "js", "widgets", "contract-builder.js"),
  "utf8",
);
const CB = new Function(src + "\nreturn ContractBuilder;")();
ok(!!CB, "the widget did not evaluate to a ContractBuilder");

// View 1 renders into `cb-shape`; view 2's grounding cards render into
// `cb-blocks`. Read the wrong one and every assertion below fails against an
// empty string, which looks exactly like the bug being tested for.
const shapeHtml = () => byId("cb-shape").innerHTML;
const groundHtml = () => byId("cb-blocks").innerHTML;
const dump = (label, html) => {
  if (process.env.CB_DEBUG) console.log("\n===== " + label + " =====\n" + html);
};
const tick = () => new Promise((r) => setTimeout(r, 0));

async function main() {
  CB.mount({ container: "host" });

  // One block, sourced from a tool whose shape is declared. Rendered before
  // the tools fetch resolves.
  globalThis.cbAddBlock();
  const b = CB.blocks[0];
  b.name = "identity";
  b.why = "AUTHOR WROTE THIS";
  globalThis.cbSetStatus(0, "sourced");
  globalThis.cbSet(0, "tool", "gbif_species_search");
  // A named block renders collapsed until opened, and the picker lives inside
  // the expanded card. Asserting against a collapsed summary would report the
  // picker missing for a reason that is not the bug.
  globalThis.cbToggle("identity");

  ok(
    !groundHtml().includes("cb-shape-grid"),
    "the picker rendered before the shapes were fetched, so the redraw assertion below proves nothing",
  );

  releaseTools();
  await tick();
  await tick();

  // ── 1. the picker appears on its own ───────────────────────────────────
  const g = groundHtml();
  dump("cb-blocks after shapes", g);
  ok(
    g.includes("cb-shape-grid"),
    "the response-shape picker did not appear after the tools fetch resolved. " +
      "cbLoadToolNames stores the shapes and must redraw; without that they are " +
      "loaded and invisible, and the author is back to typing field names from memory",
  );
  ok(
    g.includes("scientific_name") && g.includes("taxonomic_status"),
    "the picker rendered without gbif_species_search's real keys",
  );
  ok(
    !g.includes("No declared response shape"),
    "a tool with a declared shape is being reported as unread",
  );

  // ── 2. the reverse lookup finds the genome_profiler shape ──────────────
  //
  // A retrieved block sourced from GBIF, carrying a genome size. GBIF does
  // not return one. This is the arrangement that shipped fabricated sizes for
  // 56 episodes, so the hint has to name the tool that does.
  globalThis.cbAddField(0);
  const j = CB.blocks[0].fields.length - 1;
  globalThis.cbSetField(0, j, "name", "estimated_size_mb");
  globalThis.cbRenderAll();

  const s = shapeHtml();
  dump("cb-shape with estimated_size_mb", s);
  ok(
    s.includes("cb-src-hint"),
    "no reverse-lookup hint for `estimated_size_mb`, a field this block's tool cannot supply",
  );
  ok(
    s.includes("ncbi_genome_search"),
    "the reverse lookup did not name ncbi_genome_search for `estimated_size_mb` — " +
      "the one tool that actually returns it",
  );
  ok(
    /not returned by gbif_species_search/.test(s),
    "the hint does not say that the block's own tool cannot supply this field, " +
      "which is the part that makes it a warning rather than a suggestion",
  );

  // A field the block's own tool DOES return gets no hint; the picker above
  // already answered it and repeating it would be noise on the common case.
  globalThis.cbAddField(0);
  const jk = CB.blocks[0].fields.length - 1;
  globalThis.cbSetField(0, jk, "name", "scientific_name");
  globalThis.cbRenderAll();
  const hints = (shapeHtml().match(/cb-src-hint/g) || []).length;
  ok(
    hints === 1,
    `${hints} hints rendered; expected exactly one. A field the block's own tool returns must not be hinted`,
  );
  globalThis.cbDelField(0, jk);

  // ── 3. a clash offers both and picks neither ───────────────────────────
  //
  // `species` is returned by gbif_taxonomy_tree at `species` and by
  // gbif_species_search at `species[0].species`. Two tools, one name. The
  // author chooses; showing the path on every hit is what lets them.
  const clash = SHAPES.filter((sh) =>
    (sh.fields || []).some((f) => f.field === "species"),
  ).map((sh) => sh.tool);
  if (clash.length >= 2) {
    globalThis.cbAddBlock();
    const n = CB.blocks.length - 1;
    CB.blocks[n].name = "taxon";
    globalThis.cbSetField(n, 0, "name", "species");
    globalThis.cbRenderAll();
    const html = shapeHtml();
    clash.forEach((t) =>
      ok(
        html.includes(t),
        `the reverse lookup dropped ${t}, which also returns \`species\`. ` +
          `Narrowing a clash silently is the platform choosing a source for the author`,
      ),
    );
    // Both hits carry a path, so the two are distinguishable.
    const paths = (html.match(/class="p"/g) || []).length;
    ok(
      paths >= clash.length,
      `${paths} path label(s) for ${clash.length} candidate tools — without the path, ` +
        `two hits that differ only in where the value sits read as duplicates`,
    );
    globalThis.cbDelBlock(n);
  } else {
    console.log(
      "note: no field name is returned by two declared tools, so the clash " +
        "assertions did not run. Not a pass — re-check when more shapes are declared.",
    );
  }

  // ── 4. adopting writes provenance and never writes `why` ───────────────
  const before = CB.blocks[0].why;
  const beforeType = CB.blocks[0].fields[j].type;
  globalThis.cbAdoptSource(0, j, 0);
  const a = CB.blocks[0];
  ok(a.status === "sourced", "adopting a source left the block unsourced");
  ok(
    a.tool === "ncbi_genome_search",
    `adopting set tool to \`${a.tool}\`, not the tool the hit named`,
  );
  ok(
    a.fields[j].type === "number?",
    `the field kept type \`${a.fields[j].type}\` instead of taking the tool's declared \`number?\`. ` +
      `A type the author guessed is a second place for the shape to be wrong`,
  );
  ok(
    a.response_field === "estimated_size_mb",
    `response_field is \`${a.response_field}\`. Switching tools must replace the paths, ` +
      `not append to them: paths from the old tool are a claim this tool cannot honour`,
  );
  ok(
    a.why === before && a.why === "AUTHOR WROTE THIS",
    "adopting a source rewrote `why`. That is the one field the compiler refuses to " +
      "generate, so a UI that writes it ships a justification nobody wrote",
  );
  ok(beforeType !== "number?", "the type assertion above was already true before adopting");

  // Once adopted, the hint is gone: the block's own tool now answers it.
  globalThis.cbRenderAll();
  ok(
    !/not returned by ncbi_genome_search/.test(shapeHtml()),
    "the hint still warns about a field the block's tool now returns",
  );

  ok(
    unhandled.length === 0,
    `${unhandled.length} unhandled rejection(s) while rendering: ${unhandled.join("; ")}`,
  );
}

main().then(
  () => {
    if (FAIL.length) {
      console.error(`\n${FAIL.length} failure(s):`);
      FAIL.forEach((f) => console.error("  x " + f));
      process.exit(1);
    }
    console.log("contract builder: all checks pass");
  },
  (e) => {
    console.error("harness threw: " + (e && e.stack ? e.stack : e));
    process.exit(1);
  },
);
