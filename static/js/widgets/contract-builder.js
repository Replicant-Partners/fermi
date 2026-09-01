// Contract builder — the typed-output-contract editor.
//
// Shared by the create wizard (step 4) and the standalone page at /contracts.
// Extracted from templates/agent_create.html when the builder gained a second
// home: two copies of a thousand-line editor is the drift this repo keeps
// finding, and one of the copies would have been the one nobody updated.
//
// The compile is a SERVER round-trip on purpose. A browser-side compiler would
// be a second implementation of the publish gate and the two would drift, at
// which point this shows a green tick for a contract publish refuses. See
// src/handlers/contracts.rs.
//
//   ContractBuilder.mount({ container, agentId })
//
// `agentId` is optional. When given, the widget loads that agent's current
// contract via /api/contracts/decompile/:id and offers to save back.

const ContractBuilder = (() => {
  // Escaping, owned by the widget rather than borrowed from the host page.
  //
  // It was `window.esc`, which works in a browser only because
  // `window === globalThis`. That is an implicit coupling to the host, and it
  // broke the moment the widget was exercised outside one — which is how this
  // was found. Interpolated into `value="..."` attributes, so quotes are
  // escaped too: a single `"` in a `why` would otherwise close the attribute
  // and swallow the rest of the block.
  const esc = (str) => {
    if (!str) return "";
    const d = document.createElement("div");
    d.textContent = str;
    return d.innerHTML.replace(/"/g, "&quot;");
  };

  const MARKUP = `          <div class="form-section">
        <div class="form-title">Typed Output Contract</div>

        <div class="info-box">
          A contract answers three questions on three different clocks:
          <strong>what you return</strong>, <strong>where each value comes
          from</strong>, and <strong>who uses it</strong>. They are three
          views because stacking them makes each one read like
          form-filling. The document on the right is the artefact; the
          views are lenses onto it.
          <div style="margin-top: 6px; color: var(--fg3)">
            Optional now &mdash; required to publish. An untyped agent runs
            fine, but nothing can compose with it and its output is never
            validated.
          </div>
        </div>

        <div class="cb-toolbar">
          <button class="btn" onclick="cbLoadExample()">
            Load worked example
          </button>
          <button class="btn" onclick="cbClear()">Clear</button>
          <span class="cb-spacer"></span>
          <span class="cb-status" id="cb-status">No contract yet</span>
        </div>

        <!-- Three lenses. Not wizard steps: you move between them freely,
             because authoring a contract is not linear — naming a consumer
             in view 3 routinely sends you back to rename a type in view 1. -->
        <div class="cb-viewnav" id="cb-viewnav"></div>

        <div class="cb-split">
          <div class="cb-col">
            <!-- ═══ View 1: what do you return ═══ -->
            <div class="cb-view" id="cb-view-1">
              <div class="cb-lead">
                Name the artefact and list its parts. Nothing here says
                where a value comes from &mdash; that is the next view, and
                keeping them apart is the point.
              </div>

              <div class="row">
                <div class="form-group">
                  <label for="cb-type">Type name</label>
                  <input
                    type="text"
                    id="cb-type"
                    placeholder="myapp/risk_assessment"
                    oninput="cbTouch(); cbRenderNav(); cbRenderConsumers()"
                  />
                  <div class="hint">
                    <code>namespace/type</code>, so two teams can both have
                    a <code>summary</code> without colliding.
                  </div>
                </div>
                <div class="form-group">
                  <label for="cb-domain">Domain</label>
                  <input
                    type="text"
                    id="cb-domain"
                    placeholder="equity-research"
                    oninput="cbTouch()"
                  />
                </div>
              </div>

              <div class="cb-sub">
                Parts
                <span class="cb-hintlet">
                  one coherent piece each &mdash; profile, assessment, summary
                </span>
              </div>
              <div id="cb-shape"></div>
              <button
                class="btn"
                style="margin-top: 8px"
                onclick="cbAddBlock()"
              >
                + Add a part
              </button>

              <div class="cb-borrow" id="cb-borrow"></div>
            </div>

            <!-- ═══ View 2: where does each value come from ═══ -->
            <div class="cb-view" id="cb-view-2" style="display: none">
              <div class="cb-lead">
                The only view with real weight: this is where the contract
                stops being a schema and starts being a claim. Every value
                comes from exactly one of four places.
              </div>

              <div class="form-group">
                <label for="cb-tools">Declared tools</label>
                <input
                  type="text"
                  id="cb-tools"
                  placeholder="fmp_company_profile, fmp_ratios"
                  oninput="cbToolsChanged()"
                />
                <div class="hint">
                  Comma-separated, saved onto the card. A part marked
                  <code>sourced</code> must name one of these &mdash; a
                  field sourced from a tool the agent cannot call is the
                  exact defect this contract exists to catch.
                </div>
              </div>
              <datalist id="cb-tool-list"></datalist>

              <div class="cb-palette-wrap">
                <div class="cb-sub">
                  Add a part from
                  <span class="cb-hintlet">
                    starting from a tool you have is the fastest honest route
                  </span>
                </div>
                <div class="cb-palette" id="cb-palette"></div>
              </div>

              <div class="cb-sub">
                Where each part comes from
                <span class="cb-hintlet">click to expand</span>
              </div>
              <div id="cb-blocks"></div>

              <div class="collapsible-header" onclick="toggleCollapsible(this)">
                <span class="arrow">&#9654;</span> Advanced: ontology
                vocabulary
              </div>
              <div class="collapsible-body">
                <div class="hint" style="margin-bottom: 6px">
                  Paste an ontology and a field type of
                  <code>@sentiment</code> takes its type, its closed value
                  set and its definition from the entity &mdash; selecting
                  vocabulary instead of reinventing it. An unknown id is an
                  error, never a silent fallback to <code>string</code>.
                </div>
                <textarea
                  id="cb-ontology"
                  rows="4"
                  placeholder='{"entities": []}'
                  oninput="cbTouch()"
                ></textarea>
              </div>
            </div>

            <!-- ═══ View 3: who uses it ═══ -->
            <div class="cb-view" id="cb-view-3" style="display: none">
              <div class="cb-lead">
                The view whose absence turns a contract into decoration.
                &ldquo;It compiles&rdquo; is not the finish line &mdash;
                something has to be able to consume it, and something has
                to be able to score it.
              </div>

              <div class="cb-sub">Who could consume this</div>
              <div id="cb-consumers"></div>

              <div class="row" style="margin-top: 14px">
                <div class="form-group">
                  <label for="cb-synthesis">
                    How a coordinator combines members
                  </label>
                  <select id="cb-synthesis" onchange="cbTouch()">
                    <option value="">(not declared)</option>
                    <option value="aggregation">
                      aggregation &mdash; merge every member's document
                    </option>
                    <option value="pipeline">
                      pipeline &mdash; each feeds the next
                    </option>
                    <option value="selection">
                      selection &mdash; pick the best one
                    </option>
                    <option value="max_risk">
                      max_risk &mdash; the worst finding wins
                    </option>
                    <option value="cep_weighted">
                      cep_weighted &mdash; weight by calibration
                    </option>
                  </select>
                </div>
                <div class="form-group">
                  <label for="cb-cal-signal">
                    How correctness is eventually measured
                  </label>
                  <select
                    id="cb-cal-signal"
                    onchange="cbTouch(); cbRenderNav()"
                  >
                    <option value="">(not declared)</option>
                    <option value="brier_forecast">
                      brier_forecast &mdash; a forecast resolves
                    </option>
                    <option value="sosa_observation">
                      sosa_observation &mdash; a sensor reading arrives
                    </option>
                    <option value="hitl_review">
                      hitl_review &mdash; a human judges it
                    </option>
                    <option value="user_rating">
                      user_rating &mdash; the caller rates it
                    </option>
                  </select>
                  <div class="hint">
                    Without this the document is composable and
                    unfalsifiable &mdash; a strange pair to ship.
                  </div>
                </div>
              </div>

              <div class="cb-sub" style="margin-top: 14px">
                Paste this into your system prompt
                <span class="cb-hintlet">
                  or the schema is checked against prose for ever
                </span>
              </div>
              <div id="cb-promptsnippet"></div>
            </div>
          </div>

          <!-- The artefact, persistent across all three views. -->
          <div class="cb-col">
            <div class="cb-sticky">
              <div class="cb-sub">
                The document your agent returns
                <span class="cb-hintlet">live</span>
              </div>
              <div id="cb-doc"></div>
              <div id="cb-output"></div>
            </div>
          </div>
        </div>`;

  // ═══ Contract builder ═══════════════════════════════════════
  //
  // Three views over ONE artefact, not three forms.
  //
  // A contract answers three questions on three different clocks:
  //
  //   1. what do you return          the shape        cheap to change
  //   2. where does each value       the claim        the real work
  //      come from
  //   3. who uses it                 the composition  why it exists
  //
  // The first cut of this step put all three on one screen, and the
  // feedback was that it read as a form built for a DB analyst. That was
  // right, and the reason is that the three questions have different
  // audiences and different costs. Tangling shape with grounding is what
  // makes people give up: you cannot decide "is this retrieved or
  // reasoned" while still deciding whether the block exists.
  //
  // The views are not wizard steps. Movement between them is expected to
  // be non-linear — naming a consumer in view 3 routinely sends you back
  // to rename a type in view 1 so it composes with something that already
  // exists.
  //
  // The compile is a SERVER round-trip. A browser-side compiler would be a
  // second implementation of the publish gate and the two would drift, at
  // which point this shows a green tick for a contract publish refuses.
  // See src/handlers/contracts.rs.

  let cbBlocks = [];
  let cbCompiled = null;
  let cbTimer = null;
  let cbAvailableTools = [];
  let cbShapes = {};   // tool -> declared response shape
  let cbProposals = [];
  let cbTypes = [];
  let cbOpen = new Set();
  let cbView = 1;
  let cbOutTab = "schema";

  const CB_STATUSES = ["sourced", "inferred", "narrative", "unavailable"];

  // ─── view switching ───────────────────────────────────────────

  function cbSetView(n) {
    cbView = n;
    [1, 2, 3].forEach((i) => {
      const el = document.getElementById("cb-view-" + i);
      if (el) el.style.display = i === n ? "block" : "none";
    });
    cbRenderAll();
  }

  // The nav carries per-view state, because the useful question is not
  // "which view am I on" but "which view still owes something". A badge
  // that says `2 without a why` is the whole to-do list.
  function cbRenderNav() {
    const named = cbBlocks.filter((b) => b.name.trim());
    const noWhy = named.filter((b) => (b.why || "").trim().length < 40);
    const typed = document.getElementById("cb-type").value.trim();
    const signal = (document.getElementById("cb-cal-signal") || {}).value;

    const views = [
      {
        n: 1,
        k: "what you return",
        t: "The artefact",
        badge: !typed
          ? { cls: "todo", text: "needs a type name" }
          : named.length
            ? { cls: "ok", text: `${named.length} part(s)` }
            : { cls: "todo", text: "no parts yet" },
      },
      {
        n: 2,
        k: "where values come from",
        t: "The claim",
        badge: !named.length
          ? { cls: "", text: "—" }
          : noWhy.length
            ? { cls: "todo", text: `${noWhy.length} without a why` }
            : { cls: "ok", text: "all answered" },
      },
      {
        n: 3,
        k: "who uses it",
        t: "The composition",
        badge: signal
          ? { cls: "ok", text: "scoreable" }
          : { cls: "todo", text: "unfalsifiable" },
      },
    ];

    document.getElementById("cb-viewnav").innerHTML = views
      .map(
        (v) => `
      <button class="cb-viewbtn ${cbView === v.n ? "active" : ""}"
              onclick="cbSetView(${v.n})">
        <span class="n">${v.n}. ${esc(v.k)}</span>
        ${esc(v.t)}
        <span class="badge ${v.badge.cls}">${esc(v.badge.text)}</span>
      </button>`,
      )
      .join("");
  }

  // ─── block model ──────────────────────────────────────────────

  function cbNewBlock(name) {
    return {
      name: name || "",
      status: "inferred",
      tool: "",
      response_field: "",
      coverage: "complete",
      from: "",
      would_need: "",
      why: "",
      shape: "fields",
      value: "string",
      fields: [{ name: "", type: "string?" }],
    };
  }

  function cbAddBlock() {
    cbBlocks.push(cbNewBlock());
    cbRenderAll();
    cbTouch();
  }
  function cbDelBlock(i) {
    cbBlocks.splice(i, 1);
    cbRenderAll();
    cbTouch();
  }
  function cbAddField(i) {
    cbBlocks[i].fields.push({ name: "", type: "string?" });
    cbRenderAll();
    cbTouch();
  }
  function cbDelField(i, j) {
    cbBlocks[i].fields.splice(j, 1);
    cbRenderAll();
    cbTouch();
  }

  // Value writes do NOT re-render, so typing keeps focus. Only structural
  // changes (add/remove/status/shape) re-render.
  function cbSet(i, key, val) {
    cbBlocks[i][key] = val;
    if (key === "why") cbUpdateWhyCount(i);
    cbTouch();
  }
  function cbSetField(i, j, key, val) {
    cbBlocks[i].fields[j][key] = val;
    cbTouch();
  }
  function cbSetStatus(i, val) {
    cbBlocks[i].status = val;
    cbRenderAll();
    cbTouch();
  }
  function cbSetShape(i, val) {
    cbBlocks[i].shape = val;
    cbRenderAll();
    cbTouch();
  }
  function cbRenameBlock(i, val) {
    cbBlocks[i].name = val;
    cbTouch();
  }

  function cbUpdateWhyCount(i) {
    const el = document.getElementById(`cb-whycount-${i}`);
    if (el) {
      const n = (cbBlocks[i].why || "").trim().length;
      el.textContent = `${n}/40`;
      el.classList.toggle("short", n < 40);
    }
    cbRenderNav();
  }

  // ─── VIEW 1: the artefact ─────────────────────────────────────
  //
  // Shape only. A part's name, whether it holds fields or one value, and
  // the fields themselves. Deliberately silent about grounding.

  function cbRenderShape() {
    const wrap = document.getElementById("cb-shape");
    if (!wrap) return;
    if (!cbBlocks.length) {
      wrap.innerHTML = `<div class="hint">
        No parts yet. A part is one coherent piece of the document. Add one
        here, or start from a tool in view 2 &mdash; which is often faster,
        because a tool you already have implies the part.</div>`;
      return;
    }
    wrap.innerHTML = cbBlocks
      .map(
        (b, i) => `
      <div class="cb-shape-row" data-status="${b.status}">
        <div style="flex:1">
          <div style="display:flex;gap:6px;align-items:center">
            <input class="nm" value="${esc(b.name)}" placeholder="part_name"
                   oninput="cbRenameBlock(${i}, this.value)" />
            <select onchange="cbSetShape(${i}, this.value)">
              <option value="fields" ${b.shape === "fields" ? "selected" : ""}>fields</option>
              <option value="value" ${b.shape === "value" ? "selected" : ""}>one value</option>
            </select>
            <span class="cb-pill ${b.status}">${b.status}</span>
            <button class="cb-del" onclick="cbDelBlock(${i})">Remove</button>
          </div>
          ${
            b.shape === "value"
              ? `<div class="cb-shape-fields">
                   <input value="${esc(b.value)}" style="font-family:monospace;font-size:0.75rem"
                          oninput="cbSet(${i},'value',this.value)" />
                 </div>`
              : `<div class="cb-shape-fields">
                   ${b.fields
                     .map(
                       (f, j) => `
                     <div class="cb-field-row">
                       <input class="cb-fname" value="${esc(f.name)}" placeholder="field_name"
                              oninput="cbSetField(${i},${j},'name',this.value)" />
                       <input class="cb-ftype" value="${esc(f.type)}" placeholder="string?"
                              oninput="cbSetField(${i},${j},'type',this.value)" />
                       <button class="cb-del" onclick="cbDelField(${i},${j})">&times;</button>
                     </div>
                     ${cbSourceHint(i, j)}`,
                     )
                     .join("")}
                   <button class="btn" style="font-size:0.7rem;padding:3px 8px"
                           onclick="cbAddField(${i})">+ field</button>
                 </div>`
          }
        </div>
      </div>`,
      )
      .join("");
  }

  // Borrowing a neighbour's part NAMES belongs in view 1, because "what do
  // documents like mine look like" is a shape question. Grounding is never
  // borrowed: where your values come from is not transferable, so borrowed
  // parts arrive unanswered and view 2 will say so.
  function cbRenderBorrow() {
    const el = document.getElementById("cb-borrow");
    if (!el) return;
    if (!cbTypes.length) {
      el.innerHTML = `<div class="cb-sub">Borrow a shape</div>
        <div class="hint">No typed agents reachable yet &mdash; either none
        are published or the server is not up. Yours would be the first,
        which is fine; it just means nothing can match on it yet.</div>`;
      return;
    }
    el.innerHTML = `
      <div class="cb-sub">
        Borrow a shape
        <span class="cb-hintlet">
          ${cbTypes.length} type(s) declared here &middot; names only, never grounding
        </span>
      </div>
      ${cbTypes
        .slice(0, 8)
        .map(
          (t) => `
        <div class="cb-type-row">
          <span class="t" onclick="cbBorrow('${esc(t.type)}')">${esc(t.type)}</span>
          <span class="b">${esc((t.blocks || []).join(" · ")) || "—"}</span>
          <span class="c">${esc(t.producer)}</span>
        </div>`,
        )
        .join("")}`;
  }

  function cbBorrow(type) {
    const t = cbTypes.find((x) => x.type === type);
    if (!t) return;
    const have = new Set(cbBlocks.map((b) => b.name));
    (t.blocks || []).forEach((name) => {
      if (have.has(name)) return;
      const b = cbNewBlock(name);
      b.status = name === "summary" ? "narrative" : "inferred";
      if (b.status === "narrative") {
        b.shape = "value";
        b.value = "string";
        b.fields = [];
      }
      cbBlocks.push(b);
    });
    cbRenderAll();
    cbTouch();
  }

  // ─── VIEW 2: the claim ────────────────────────────────────────

  async function cbLoadProposals() {
    const tools = cbToolNames();
    if (!tools.length) {
      cbProposals = [];
      cbRenderPalette();
      return;
    }
    try {
      const res = await fetch("/api/contracts/suggest", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ tool_names: tools }),
      });
      cbProposals = res.ok ? (await res.json()).blocks || [] : [];
    } catch {
      cbProposals = [];
    }
    cbRenderPalette();
  }

  function cbRenderPalette() {
    const wrap = document.getElementById("cb-palette");
    if (!wrap) return;
    const used = new Set(cbBlocks.map((b) => (b.tool || "").trim()));
    const chips = [];

    cbProposals.forEach((p, i) => {
      const tool = p.source.tool;
      chips.push(`
        <button class="cb-chip tool ${used.has(tool) ? "used" : ""}"
                onclick="cbAddFromProposal(${i})"
                title="${esc(p.source.response_field)}">
          <span class="cb-chip-k">retrieved · ${used.has(tool) ? "added" : "tool"}</span>
          ${esc(tool)}
        </button>`);
    });

    if (!cbProposals.length) {
      chips.push(`
        <div class="hint" style="max-width:420px">
          No tools declared yet. Add some above and they appear here as
          retrieved parts &mdash; the only kind whose values a tool can
          actually vouch for.
        </div>`);
    }

    chips.push(`
      <button class="cb-chip judgement" onclick="cbAddJudgement()">
        <span class="cb-chip-k">reasoned · no tool can give you this</span>
        assessment
      </button>`);
    chips.push(`
      <button class="cb-chip prose" onclick="cbAddProse()">
        <span class="cb-chip-k">prose · for a human</span>
        summary
      </button>`);
    chips.push(`
      <button class="cb-chip gap" onclick="cbAddGap()">
        <span class="cb-chip-k">refused · nothing can supply it</span>
        declare a gap
      </button>`);

    wrap.innerHTML = chips.join("");
  }

  function cbAddFromProposal(i) {
    const p = cbProposals[i];
    const b = cbNewBlock(p.name);
    b.status = "sourced";
    b.tool = p.source.tool;
    b.response_field = p.source.response_field;
    b.coverage = p.source.coverage || "complete";
    b.fields = (p.candidate_fields || []).map((f) => ({
      name: f.name,
      type: f.type,
    }));
    if (!b.fields.length) b.fields = [{ name: "", type: "string?" }];
    cbBlocks.push(b);
    cbOpen.add(b.name);
    cbRenderAll();
    cbTouch();
  }

  function cbAddJudgement() {
    const b = cbNewBlock("assessment");
    b.status = "inferred";
    b.from = cbBlocks
      .filter((x) => x.status === "sourced" && x.name)
      .map((x) => x.name)
      .join(", ");
    b.fields = [
      { name: "direction", type: "enum:up|flat|down" },
      { name: "confidence", type: "number?" },
    ];
    cbBlocks.push(b);
    cbOpen.add(b.name);
    cbRenderAll();
    cbTouch();
  }

  function cbAddProse() {
    const b = cbNewBlock("summary");
    b.status = "narrative";
    b.shape = "value";
    b.value = "string";
    b.fields = [];
    cbBlocks.push(b);
    cbOpen.add(b.name);
    cbRenderAll();
    cbTouch();
  }

  function cbAddGap() {
    const b = cbNewBlock("");
    b.status = "unavailable";
    cbBlocks.push(b);
    cbRenderAll();
    cbTouch();
  }

  // What each status implies for the derived `_provenance` stamp. Mirrors
  // Source::provenance_schema in src/contract_sketch.rs, shown so the
  // author sees the consequence of the choice while making it.
  // ── the tool decides which fields are available ──────────────────
  //
  // Asked in review: "wouldn't the tool determine which fields are available
  // on a sourced thing?" It does now. For a tool whose response has been read,
  // this offers its real keys with their real types, so an author picks rather
  // than types — and cannot invent a key that does not exist, which is the
  // same failure as the agent inventing a value.
  function cbFieldPicker(i, b) {
    const sh = cbShapes[(b.tool || "").trim()];
    if (!sh) {
      return b.status === "sourced" && (b.tool || "").trim()
        ? `<div class="cb-hintlet" style="display:block;margin:6px 0">
             No declared response shape for <code>${esc(b.tool)}</code> — nobody
             has read it. Field names here are unchecked, and the
             <code>response_field</code> claim cannot be verified against the
             tool's actual output.
           </div>`
        : "";
    }
    const have = new Set(b.fields.map((f) => (f.name || "").trim()));
    const vendor = sh.evidence === "vendor";
    return `
      <div class="cb-shape">
        <div class="cb-shape-head">
          ${sh.fields.length} field(s) <code>${esc(sh.tool)}</code> returns
          <span class="cb-ev ${vendor ? "vendor" : "built"}">${sh.evidence}</span>
          <span class="cb-hintlet">${esc(sh.evidence_from)}</span>
        </div>
        <div class="cb-shape-grid">
          ${sh.fields
            .map(
              (f, j) => `
            <button class="cb-shape-f ${have.has(f.field) ? "on" : ""}"
                    onclick="cbToggleShapeField(${i}, ${j})"
                    title="${esc(f.path)}${f.note ? " — " + esc(f.note) : ""}">
              ${have.has(f.field) ? "&#10003; " : "+ "}${esc(f.field)}
              <span class="t">${esc(f.type)}</span>
            </button>`,
            )
            .join("")}
        </div>
        ${
          vendor
            ? `<div class="cb-hintlet" style="display:block;margin-top:4px">
                 A passthrough: this shape is the vendor's and can change
                 without this repo noticing.
               </div>`
            : ""
        }
      </div>`;
  }

  function cbToggleShapeField(i, j) {
    const b = cbBlocks[i];
    const sh = cbShapes[(b.tool || "").trim()];
    if (!sh) return;
    const f = sh.fields[j];
    const at = b.fields.findIndex((x) => (x.name || "").trim() === f.field);
    if (at >= 0) {
      b.fields.splice(at, 1);
    } else {
      // Drop the blank starter row rather than leaving it above the fields
      // the author just picked.
      const blank = b.fields.findIndex((x) => !(x.name || "").trim());
      if (blank >= 0) b.fields.splice(blank, 1);
      b.fields.push({ name: f.field, type: f.type });
    }
    // Keep `response_field` honest: it should name the paths actually used,
    // not every path the tool returns.
    const used = new Set(b.fields.map((x) => (x.name || "").trim()));
    const paths = sh.fields.filter((x) => used.has(x.field)).map((x) => x.path);
    if (paths.length) b.response_field = paths.join(", ");
    cbRenderAll();
    cbTouch();
  }

  // Which of a block's fields the tool cannot supply. The original bug, shown
  // next to the block rather than discovered by reading the tool.
  function cbUncovered(b) {
    const sh = cbShapes[(b.tool || "").trim()];
    if (!sh || b.status !== "sourced") return [];
    const known = new Set();
    sh.fields.forEach((f) => {
      known.add(f.field);
      known.add(f.path.split(".").pop().replace(/\[\]$/, ""));
    });
    return b.fields
      .map((f) => (f.name || "").trim())
      .filter((n) => n && !known.has(n));
  }

  // ── reverse lookup: I know the field, who returns it? ────────────
  //
  // The picker answers "I have this tool, what does it give me". This answers
  // the question authors actually arrive with: they know the field they want
  // in the document and not which tool has it. Without it the only way to
  // find out is to name a tool and look, once per tool.
  //
  // Two tools can return `symbol`, and this does NOT pick between them. Every
  // hit carries its tool AND its full path, so the author chooses. A system
  // that guessed here would be making a grounding claim on the author's
  // behalf, which is the one thing this whole feature exists to prevent.
  function cbFieldSources(name) {
    const n = (name || "").trim().toLowerCase();
    // One or two characters match nearly everything; that is noise, not a
    // lead, and it would put a hint under every half-typed field.
    if (n.length < 3) return [];
    const hits = [];
    Object.keys(cbShapes).forEach((tool) => {
      const sh = cbShapes[tool];
      (sh.fields || []).forEach((f) => {
        const leaf = f.path.split(".").pop().replace(/\[\]$/, "").toLowerCase();
        const key = (f.field || "").toLowerCase();
        const exact = key === n || leaf === n;
        if (exact || key.includes(n) || leaf.includes(n))
          hits.push({
            tool: sh.tool,
            path: f.path,
            type: f.type,
            field: f.field,
            exact: exact,
          });
      });
    });
    // Exact before substring, then by tool name, so the obvious answer is
    // first and the ordering does not move as unrelated tools get declared.
    hits.sort(
      (a, b) => Number(b.exact) - Number(a.exact) || a.tool.localeCompare(b.tool),
    );
    return hits;
  }

  // The candidates for ONE field row. Shared by the hint and by the adopt, so
  // the button and the action cannot disagree — the button passes an index
  // into this list rather than a tool name, which also keeps tool and path out
  // of an `onclick` attribute where escaping would have to be trusted.
  function cbSourceCandidates(i, j) {
    const b = cbBlocks[i];
    if (!b) return [];
    const f = (b.fields || [])[j];
    if (!f) return [];
    const name = (f.name || "").trim();
    if (!name) return [];
    const cur = (b.tool || "").trim();
    const sh = cbShapes[cur];
    // Already answered by this block's own tool. The picker above said so;
    // repeating it underneath would be noise on the common case.
    if (
      sh &&
      (sh.fields || []).some(
        (x) =>
          x.field === name ||
          x.path.split(".").pop().replace(/\[\]$/, "") === name,
      )
    )
      return [];
    return cbFieldSources(name)
      .filter((h) => h.tool !== cur)
      .slice(0, 4);
  }

  function cbSourceHint(i, j) {
    const hits = cbSourceCandidates(i, j);
    if (!hits.length) return "";
    const cur = (cbBlocks[i].tool || "").trim();
    // Said plainly when the block is already sourced: this is the uncovered
    // case, the field the named tool cannot supply.
    const lead = cur ? "not returned by " + cur + " \u00b7 but is by" : "returned by";
    return `<div class="cb-src-hint"><span class="k">${esc(lead)}</span>${hits
      .map(
        (h, k) => `<button class="cb-src-hit" onclick="cbAdoptSource(${i},${j},${k})"
                title="${esc(h.path)} \u2014 ${esc(h.type)}${h.exact ? "" : " (name match, not exact)"}">${esc(
                  h.tool,
                )}<span class="p">${esc(h.path)}</span></button>`,
      )
      .join("")}</div>`;
  }

  function cbAdoptSource(i, j, k) {
    const b = cbBlocks[i];
    const hit = cbSourceCandidates(i, j)[k];
    if (!b || !hit) return;
    const prev = (b.tool || "").trim();
    b.status = "sourced";
    b.tool = hit.tool;
    if (hit.type) b.fields[j].type = hit.type;
    if (prev && prev !== hit.tool) {
      // The old paths named the old tool. Carrying them over would leave a
      // `response_field` that claims paths this tool never returns, which is
      // exactly the unverifiable claim the picker exists to stop.
      b.response_field = hit.path;
    } else {
      const paths = new Set(
        (b.response_field || "")
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean),
      );
      paths.add(hit.path);
      b.response_field = Array.from(paths).join(", ");
    }
    // `why` is deliberately untouched. It is the one field the compiler
    // refuses to write, and a UI that filled it in would break that rule from
    // the other side: the author would ship a justification nobody wrote.
    cbRenderAll();
    cbTouch();
  }

  function cbDerivedHint(b) {
    if (b.status === "narrative")
      return "No provenance stamp. A retrieval verdict about a paragraph is a category error.";
    if (b.status === "inferred")
      return 'Stamp: <code>const: "model_inference"</code> &mdash; can never claim to be retrieved.';
    if (b.status === "unavailable")
      return 'Stamp: <code>const: "unavailable_no_tool_source"</code> &mdash; the value must be null.';
    const base = "tool_verified, tool_no_match";
    if (b.coverage === "partial")
      return `Stamp: <code>enum [${base}, unavailable_no_tool_source]</code>`;
    if (b.coverage === "deferred")
      return `Stamp: <code>enum [${base}, pending_tool_check]</code>`;
    if (b.coverage === "partial_deferred")
      return `Stamp: <code>enum [${base}, unavailable_no_tool_source, pending_tool_check]</code>`;
    return `Stamp: <code>enum [${base}]</code>`;
  }

  function cbToggle(name) {
    if (cbOpen.has(name)) cbOpen.delete(name);
    else cbOpen.add(name);
    cbRenderAll();
  }

  // View 2's cards carry grounding ONLY. Shape moved to view 1, which is
  // what stops this card being the eight-field form it used to be.
  function cbRenderGrounding() {
    const wrap = document.getElementById("cb-blocks");
    if (!wrap) return;
    if (!cbBlocks.length) {
      wrap.innerHTML = `<div class="hint" style="padding:12px 0">
        Nothing to ground yet. Add a part from the palette above, or in
        view 1.</div>`;
      return;
    }
    wrap.innerHTML = cbBlocks
      .map((b, i) => {
        const whyN = (b.why || "").trim().length;
        const isOpen = cbOpen.has(b.name) || !b.name.trim();

        if (!isOpen) {
          return `
          <div class="cb-block" data-status="${b.status}">
            <div class="cb-sum" onclick="cbToggle('${esc(b.name)}')">
              <span class="nm">${esc(b.name)}</span>
              <span class="cb-pill ${b.status}">${b.status}</span>
              ${
                b.status === "sourced"
                  ? `<span class="cb-hintlet">${esc(b.tool || "no tool named")}</span>`
                  : ""
              }
              <span class="cb-spacer"></span>
              ${
                whyN >= 40
                  ? '<span class="cb-ok">&#10003; why</span>'
                  : '<span class="cb-todo">why needed</span>'
              }
            </div>
          </div>`;
        }

        const src =
          b.status === "sourced"
            ? `<div class="row">
                 <div class="form-group">
                   <label>Tool</label>
                   <input list="cb-tool-list" value="${esc(b.tool)}"
                          oninput="cbSet(${i},'tool',this.value)"
                          placeholder="fmp_ratios" />
                 </div>
                 <div class="form-group">
                   <label>Response field</label>
                   <input value="${esc(b.response_field)}"
                          oninput="cbSet(${i},'response_field',this.value)"
                          placeholder="priceToEarningsRatio, priceToBookRatio" />
                 </div>
               </div>
               ${cbFieldPicker(i, b)}
               ${(() => {
                 const u = cbUncovered(b);
                 return u.length
                   ? `<div class="cb-uncovered">
                        <strong>${u.length} field(s) this tool does not return:</strong>
                        ${u.map((n) => `<code>${esc(n)}</code>`).join(" ")}
                        <div>A field in a retrieved block with no possible
                        source is the shape that shipped fabricated genome data
                        for 56 episodes. Move it to a reasoned block, declare it
                        unavailable, or widen coverage and say why.</div>
                      </div>`
                   : "";
               })()}
               <div class="form-group">
                 <label>Coverage</label>
                 <select onchange="cbSet(${i},'coverage',this.value); cbRenderAll()">
                   <option value="complete" ${b.coverage === "complete" ? "selected" : ""}>complete — answers for every field, or honestly reports no match</option>
                   <option value="partial" ${b.coverage === "partial" ? "selected" : ""}>partial — some fields have no source at all</option>
                   <option value="deferred" ${b.coverage === "deferred" ? "selected" : ""}>deferred — the check may not have run yet</option>
                   <option value="partial_deferred" ${b.coverage === "partial_deferred" ? "selected" : ""}>partial + deferred — both, and usually a sign the part wants splitting</option>
                 </select>
               </div>`
            : b.status === "inferred"
              ? `<div class="form-group">
                   <label>Reasoned from</label>
                   <input value="${esc(b.from)}"
                          oninput="cbSet(${i},'from',this.value)"
                          placeholder="the retrieved profile and valuation parts, against sector base rates" />
                   <div class="hint">An inference over nothing is a guess; naming the basis is what separates them.</div>
                 </div>`
              : b.status === "unavailable"
                ? `<div class="form-group">
                     <label>Would need (optional)</label>
                     <input value="${esc(b.would_need)}"
                            oninput="cbSet(${i},'would_need',this.value)"
                            placeholder="an NCBI Assembly lookup" />
                     <div class="hint">Turns a null into a to-do.</div>
                   </div>`
                : "";

        return `
        <div class="cb-block" data-status="${b.status}">
          <div class="cb-sum" onclick="cbToggle('${esc(b.name)}')" style="margin-bottom:8px">
            <span class="nm">${esc(b.name) || "(unnamed part)"}</span>
            <span class="cb-hintlet">▾ collapse</span>
            <span class="cb-spacer"></span>
            ${
              whyN >= 40
                ? '<span class="cb-ok">&#10003; why</span>'
                : '<span class="cb-todo">why needed</span>'
            }
          </div>
          <div class="form-group">
            <label>Where does it come from</label>
            <select onchange="cbSetStatus(${i}, this.value)">
              ${CB_STATUSES.map(
                (s) =>
                  `<option value="${s}" ${b.status === s ? "selected" : ""}>${s}</option>`,
              ).join("")}
            </select>
          </div>
          ${src}
          <div class="form-group" style="margin-bottom:4px">
            <label>Why this status</label>
            <textarea rows="2" placeholder="Say why this part has the status it has — the next author cannot tell a considered 'unavailable' from a lazy one."
                      oninput="cbSet(${i},'why',this.value)">${esc(b.why)}</textarea>
            <div class="cb-why-count ${whyN < 40 ? "short" : ""}" id="cb-whycount-${i}">${whyN}/40</div>
          </div>
          <div class="cb-derived">${cbDerivedHint(b)}</div>
        </div>`;
      })
      .join("");
  }

  // ─── VIEW 3: the composition ──────────────────────────────────

  async function cbLoadTypes() {
    try {
      const res = await fetch("/api/contracts/types");
      if (!res.ok) return;
      cbTypes = (await res.json()).types || [];
    } catch {
      cbTypes = [];
    }
    cbRenderAll();
  }

  function cbRenderConsumers() {
    const el = document.getElementById("cb-consumers");
    if (!el) return;
    const mine = document.getElementById("cb-type").value.trim();

    if (!mine) {
      el.innerHTML = `<div class="hint">Name a type in view 1 first.</div>`;
      return;
    }
    const match = cbTypes.find((t) => t.type === mine);
    const consumers = match ? match.consumers || [] : [];

    let body = "";
    if (consumers.length) {
      body = `<div class="cb-consumer">
        <span class="who">${esc(consumers.join(", "))}</span>
        declare <code>${esc(mine)}</code> in <code>accepts</code>, so they
        match on identity rather than on a string that looks familiar.
      </div>`;
    } else {
      body = `<div class="cb-consumer">
        Nothing declares <code>${esc(mine)}</code> in <code>accepts</code>
        yet. Not a problem while you are the only producer &mdash; but it is
        the thing that makes the type worth having, so it is worth deciding
        now rather than discovering later.
      </div>`;
    }

    // The near-duplicate warning. Two types differing by one field name
    // compose with nothing, and this is the moment it is cheap to fix.
    const near = cbTypes.filter(
      (t) => t.type !== mine && cbSimilar(t.type, mine),
    );
    if (near.length) {
      body += `<div class="cb-consumer" style="border-color:var(--orange)">
        Close to an existing type:
        ${near.map((t) => `<code>${esc(t.type)}</code> (${esc(t.producer)})`).join(", ")}.
        If you mean the same document, produce THAT type instead. Two types
        that differ by one name compose with nothing.
      </div>`;
    }
    el.innerHTML = body;
  }

  // Deliberately crude: same trailing segment, different namespace, or one
  // is a substring of the other. A cleverer matcher would be confidently
  // wrong more often, and the author is reading these either way.
  function cbSimilar(a, b) {
    const seg = (s) => (s.split("/").pop() || "").replace(/[_-]/g, "");
    const x = seg(a),
      y = seg(b);
    if (!x || !y) return false;
    return x === y || x.includes(y) || y.includes(x);
  }

  function cbRenderPromptSnippet() {
    const el = document.getElementById("cb-promptsnippet");
    if (!el) return;
    if (!cbCompiled) {
      el.innerHTML = `<div class="hint">Nothing compiled yet. Once the
        contract compiles, the exact document to paste appears here.</div>`;
      return;
    }
    const doc = JSON.stringify(cbSampleDoc(cbCompiled.output_contract.schema), null, 2);
    const rules = `End every response with one JSON document in a \`\`\`json fence, conforming exactly to type ${cbCompiled.output_contract.produces_schema}:

${doc}

Rules, each enforced by the platform:
1. Every key is required, including the nulls. A field you could not fill is null, not absent — "I looked and found nothing" and "I did not answer" are different facts.
2. No extra keys. The document is closed at every level.
3. Never write a *_provenance value outside its declared set.
4. Only fill a sourced block from that block's own tool. If you did not call it, the block is null and its stamp says tool_no_match.
5. Reasoned blocks are stamped model_inference always. Do not compute derived numbers inside a retrieved block.`;

    el.innerHTML = `
      <button class="btn cb-copybtn" onclick="cbCopySnippet()">Copy</button>
      <div class="cb-pre" id="cb-snippet-text">${esc(rules)}</div>`;
  }

  function cbCopySnippet() {
    const t = document.getElementById("cb-snippet-text");
    if (!t) return;
    navigator.clipboard
      ?.writeText(t.textContent)
      .then(() => cbSetStatusChip("Snippet copied", "ok"))
      .catch(() => {});
  }

  // ─── the document, persistent across all three views ──────────

  function cbRenderDoc() {
    const el = document.getElementById("cb-doc");
    if (!el) return;
    const named = cbBlocks.filter((b) => b.name.trim());
    if (!named.length) {
      el.innerHTML = `<div class="cb-doc"><span class="nul">// nothing composed yet</span></div>`;
      return;
    }

    const lines = ["{"];
    named.forEach((b, i) => {
      const last = i === named.length - 1;
      const name = esc(b.name.trim());
      if (b.shape === "value" || b.status === "narrative") {
        const cls = b.status === "narrative" ? "prose" : "k";
        lines.push(
          `  <span class="k">"${name}"</span>: <span class="${cls}">"…"</span>${last ? "" : ","}`,
        );
      } else {
        const fields = b.fields.filter((f) => f.name.trim());
        const inner = fields.length
          ? fields
              .map(
                (f) =>
                  `    <span class="k">"${esc(f.name.trim())}"</span>: <span class="nul">${esc(cbTypeHint(f.type))}</span>`,
              )
              .join(",\n")
          : `    <span class="nul">// no fields yet</span>`;
        lines.push(`  <span class="k">"${name}"</span>: {`);
        lines.push(inner);
        lines.push("  },");
      }

      if (b.status !== "narrative") {
        const cls =
          b.status === "sourced"
            ? "stamp-sourced"
            : b.status === "inferred"
              ? "stamp-inferred"
              : "stamp-unavailable";
        const v =
          b.status === "sourced"
            ? "tool_verified | tool_no_match" +
              (b.coverage === "partial"
                ? " | unavailable_no_tool_source"
                : b.coverage === "deferred"
                  ? " | pending_tool_check"
                  : b.coverage === "partial_deferred"
                    ? " | unavailable_no_tool_source | pending_tool_check"
                    : "")
            : b.status === "inferred"
              ? "model_inference"
              : "unavailable_no_tool_source";
        lines.push(
          `  <span class="k">"${name}_provenance"</span>: <span class="${cls}">${esc(v)}</span>${last ? "" : ","}`,
        );
      }
    });
    lines.push("}");

    el.innerHTML =
      `<div class="cb-doc">${lines.join("\n")}</div>
       <div class="cb-doc-legend">
         <span class="stamp-sourced">■</span> retrieved ·
         <span class="stamp-inferred">■</span> reasoned ·
         <span class="prose">■</span> prose ·
         <span class="stamp-unavailable">■</span> refused.
         The <code>_provenance</code> lines are written by the platform, not
         by your agent — which is why a judgement can never present itself
         as a lookup.
       </div>`;
  }

  function cbTypeHint(t) {
    const s = (t || "").trim();
    if (s.startsWith("enum:")) return '"' + s.slice(5).split("|")[0] + '"';
    if (s.startsWith("const:")) return '"' + s.slice(6) + '"';
    if (s.endsWith("[]") || s.endsWith("[]?")) return "[ … ]";
    if (s.endsWith("?")) return "null";
    if (s.startsWith("number") || s.startsWith("integer")) return "0";
    if (s.startsWith("boolean")) return "false";
    return '"…"';
  }

  // ─── orchestration ────────────────────────────────────────────

  function cbRenderAll() {
    cbRenderNav();
    cbRenderShape();
    cbRenderBorrow();
    cbRenderPalette();
    cbRenderGrounding();
    cbRenderConsumers();
    cbRenderPromptSnippet();
    cbRenderDoc();
  }

  // ─── compile ──────────────────────────────────────────────────

  // The one element the widget reads and does not own.
  //
  // The wizard has an `agent-name` input, and the title should track it as the
  // author types — so that case reads the DOM live rather than caching. The
  // standalone page has no such field, because it edits an agent that already
  // exists, and reading it unguarded threw `Cannot read properties of null`
  // inside `cbSketch`.
  //
  // That is called by every compile and, through `cbTouch`, by every
  // keystroke, so nothing on /contracts could compile. Nothing said so either:
  // the throw happened inside a debounced handler, so the status chip just
  // never left "Compiling…". Found by loading the page in a browser; the
  // DOM-stub harness cannot see it, because a stub that answers every
  // `getElementById` with an element is exactly what makes the renders work,
  // and it papers over this at the same time.
  //
  // Falling back to "" would have been the smaller-looking fix and the worse
  // one: `title` reaches the compiled contract, so the same agent would
  // compile to a different document depending on which page you opened it in.
  let cbTitleFor = "";
  function cbAgentTitle() {
    const el = document.getElementById("agent-name");
    return el ? el.value.trim() : cbTitleFor.trim();
  }

  function cbSketch() {
    const s = {
      domain: document.getElementById("cb-domain").value.trim(),
      produces_schema: document.getElementById("cb-type").value.trim(),
      blocks: cbBlocks.map((b) => {
        const source = { status: b.status };
        if (b.status === "sourced") {
          source.tool = b.tool.trim();
          source.response_field = b.response_field.trim();
          source.coverage = b.coverage;
        } else if (b.status === "inferred") {
          source.from = b.from.trim();
        } else if (b.status === "unavailable" && b.would_need.trim()) {
          source.would_need = b.would_need.trim();
        }
        const out = { name: b.name.trim(), source, why: b.why.trim() };
        if (b.shape === "value") {
          out.value = b.value.trim();
        } else {
          out.fields = {};
          b.fields.forEach((f) => {
            if (f.name.trim()) out.fields[f.name.trim()] = f.type.trim();
          });
        }
        return out;
      }),
    };
    const title = cbAgentTitle();
    if (title) s.title = title.replace(/_/g, " ");

    // View 3's answers. Passed through by the compiler, which has no
    // opinion about calibration — inventing one would be worse than
    // carrying the author's.
    const syn = document.getElementById("cb-synthesis");
    if (syn && syn.value) s.synthesis = syn.value;
    const sig = document.getElementById("cb-cal-signal");
    if (sig && sig.value) {
      s.calibration = {
        signal: sig.value,
        comparison:
          sig.value === "brier_forecast" ? "brier_score" : "hitl_review",
      };
    }
    return s;
  }

  function cbToolNames() {
    const el = document.getElementById("cb-tools");
    if (!el) return [];
    return el.value
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean);
  }

  function cbTouch() {
    cbRenderDoc();
    cbRenderNav();
    clearTimeout(cbTimer);
    cbTimer = setTimeout(cbCompile, 400);
  }

  let cbToolTimer = null;
  function cbToolsChanged() {
    cbTouch();
    clearTimeout(cbToolTimer);
    cbToolTimer = setTimeout(cbLoadProposals, 350);
  }

  function cbSetStatusChip(text, cls) {
    const el = document.getElementById("cb-status");
    if (!el) return;
    el.textContent = text;
    el.className = "cb-status" + (cls ? " " + cls : "");
  }

  async function cbCompile() {
    const out = document.getElementById("cb-output");
    if (!out) return;
    const sketch = cbSketch();

    if (!cbBlocks.length && !sketch.domain && !sketch.produces_schema) {
      cbCompiled = null;
      out.innerHTML = "";
      cbSetStatusChip("No contract yet", "");
      return;
    }

    cbSetStatusChip("Compiling…", "busy");

    let ontology = null;
    const ontEl = document.getElementById("cb-ontology");
    const ontRaw = ontEl ? ontEl.value.trim() : "";
    if (ontRaw) {
      try {
        ontology = JSON.parse(ontRaw);
      } catch (e) {
        cbCompiled = null;
        cbSetStatusChip("Ontology is not valid JSON", "bad");
        out.innerHTML = `<div class="cb-finding"><span class="cb-check">ontology</span>${esc(e.message)}</div>`;
        return;
      }
    }

    let data;
    try {
      const res = await fetch("/api/contracts/compile", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          sketch,
          tool_names: cbToolNames(),
          ontology,
        }),
      });
      if (!res.ok) {
        cbCompiled = null;
        cbSetStatusChip("Compiler error", "bad");
        out.innerHTML = `<div class="cb-finding">${esc(await res.text())}</div>`;
        return;
      }
      data = await res.json();
    } catch (e) {
      cbCompiled = null;
      cbSetStatusChip("Network error", "bad");
      out.innerHTML = `<div class="cb-finding">${esc(e.message)}</div>`;
      return;
    }

    if (!data.compiles) {
      cbCompiled = null;
      const n = (data.findings || []).length;
      cbSetStatusChip(`${n} to fix`, "bad");
      out.innerHTML =
        `<div class="hint" style="margin:10px 0 6px">Not compiled. Nothing
         partial is emitted — a contract that is almost complete reads
         exactly like one that is.</div>` +
        (data.findings || [])
          .map(
            (f) =>
              `<div class="cb-finding"><span class="cb-check">${esc(f.check)}</span>${esc(f.fix)}</div>`,
          )
          .join("");
      cbRenderPromptSnippet();
      return;
    }

    cbCompiled = data;
    cbSetStatusChip("Compiles — would publish", "ok");
    cbRenderOutput();
    cbRenderPromptSnippet();
  }

  function cbRenderOutput() {
    if (!cbCompiled) return;
    const out = document.getElementById("cb-output");
    if (!out) return;
    const oc = cbCompiled.output_contract;
    const gen = cbCompiled.generated_properties || [];
    const body =
      cbOutTab === "schema"
        ? JSON.stringify(oc.schema, null, 2)
        : JSON.stringify(oc.grounding, null, 2);

    out.innerHTML = `
      <div class="cb-result">
        <h4>&#10003; Compiles — the publish gate accepts this</h4>
        <div class="cb-gen">
          Type <code>${esc(oc.produces_schema)}</code> ·
          ${Object.keys(oc.schema.properties).length} properties ·
          <code>produces</code> set to <code>${esc((cbCompiled.produces || []).join(", "))}</code>
        </div>
        ${
          gen.length
            ? `<div class="cb-gen">Written for you: ${gen.map((g) => `<code>${esc(g)}</code>`).join(" ")}
               — plus their grounding entries, the narrowed enums and the required list.</div>`
            : ""
        }
        <div class="cb-tabs">
          <button class="${cbOutTab === "schema" ? "active" : ""}" onclick="cbTab('schema')">JSON Schema</button>
          <button class="${cbOutTab === "grounding" ? "active" : ""}" onclick="cbTab('grounding')">Grounding</button>
        </div>
        <div class="cb-pre">${esc(body)}</div>
      </div>`;
  }

  function cbTab(t) {
    cbOutTab = t;
    cbRenderOutput();
  }

  // A skeleton document from the compiled schema. The json-schema-builder
  // "sample data" idea, which here is what turns a declared contract into
  // something an author can test a prompt against.
  function cbSampleDoc(schema) {
    function val(s) {
      if (!s || typeof s !== "object") return null;
      if (s.const !== undefined) return s.const;
      if (Array.isArray(s.enum)) return s.enum[0];
      const t = Array.isArray(s.type) ? s.type[0] : s.type;
      const nullable = Array.isArray(s.type) && s.type.includes("null");
      if (t === "object") {
        const o = {};
        Object.entries(s.properties || {}).forEach(([k, v]) => {
          o[k] = val(v);
        });
        return o;
      }
      if (t === "array") return nullable ? null : [];
      if (nullable) return null;
      if (t === "integer" || t === "number") return 0;
      if (t === "boolean") return false;
      return "…";
    }
    return val(schema);
  }

  // ─── load / clear ─────────────────────────────────────────────

  function cbClear() {
    cbBlocks = [];
    cbCompiled = null;
    cbProposals = [];
    cbOpen = new Set();
    ["cb-domain", "cb-type", "cb-tools", "cb-ontology"].forEach((id) => {
      const el = document.getElementById(id);
      if (el) el.value = "";
    });
    ["cb-synthesis", "cb-cal-signal"].forEach((id) => {
      const el = document.getElementById(id);
      if (el) el.value = "";
    });
    const out = document.getElementById("cb-output");
    if (out) out.innerHTML = "";
    cbRenderAll();
    cbTouch();
  }

  // A sketch's `fields` is an object because that is pleasant to write;
  // the editor needs an ordered array because rows have positions.
  function cbBlocksFromSketch(sketch) {
    return (sketch.blocks || []).map((b) => {
      const s = b.source || {};
      const blk = cbNewBlock(b.name);
      blk.status = s.status || "inferred";
      blk.tool = s.tool || "";
      blk.response_field = s.response_field || "";
      blk.coverage = s.coverage || "complete";
      blk.from = s.from || "";
      blk.would_need = s.would_need || "";
      blk.why = b.why || "";
      if (b.value) {
        blk.shape = "value";
        blk.value = b.value;
        blk.fields = [];
      } else {
        blk.shape = "fields";
        blk.fields = Object.entries(b.fields || {}).map(([name, t]) => ({
          name,
          type: typeof t === "string" ? t : t.type,
        }));
      }
      return blk;
    });
  }

  // Fetched, not inlined, so tests/contract_sketch_corpus.rs compiles the
  // same bytes this button loads. A demo that ships broken shows the
  // newcomer a wall of findings about the example.
  async function cbLoadExample() {
    let ex;
    try {
      const res = await fetch(
        "/static/contract-examples/equity_evidence.sketch.json",
      );
      if (!res.ok) throw new Error("HTTP " + res.status);
      ex = await res.json();
    } catch (e) {
      cbSetStatusChip("Could not load example", "bad");
      return;
    }
    document.getElementById("cb-domain").value = ex.sketch.domain || "";
    document.getElementById("cb-type").value =
      ex.sketch.produces_schema || "";
    document.getElementById("cb-tools").value = (ex.tool_names || []).join(
      ", ",
    );
    const syn = document.getElementById("cb-synthesis");
    if (syn && ex.sketch.synthesis) syn.value = ex.sketch.synthesis;
    const sig = document.getElementById("cb-cal-signal");
    if (sig && ex.sketch.calibration && ex.sketch.calibration.signal)
      sig.value = ex.sketch.calibration.signal;
    cbBlocks = cbBlocksFromSketch(ex.sketch);
    cbOpen = new Set();
    cbLoadProposals();
    cbRenderAll();
    cbCompile();
  }

  async function cbLoadToolNames() {
    try {
      const res = await fetch("/api/contracts/tools");
      if (!res.ok) return;
      const data = await res.json();
      cbAvailableTools = data.tools || [];
      // The declared response shapes. This is what makes the field picker a
      // choice among fields that exist rather than a text box.
      cbShapes = {};
      (data.response_shapes || []).forEach((sh) => {
        cbShapes[sh.tool] = sh;
      });
      const dl = document.getElementById("cb-tool-list");
      if (dl)
        dl.innerHTML = cbAvailableTools
          .map((t) => `<option value="${esc(t)}"></option>`)
          .join("");
      // Redraw. This fetch resolves after mount's initial render, so without
      // it the shapes sat in `cbShapes` and were never drawn: the picker
      // simply never appeared, and the reverse-lookup hints never fired. The
      // data was right and invisible, which is the worst kind of wrong.
      cbRenderAll();
    } catch {}
  }


  // ── mount ────────────────────────────────────────────────────────
  //
  // The widget writes its own markup, so a host page is one div. That is also
  // what lets the standalone page exist without duplicating 240 lines of it.
  function mount(opts) {
    const el =
      typeof opts.container === "string"
        ? document.getElementById(opts.container)
        : opts.container;
    if (!el) throw new Error("ContractBuilder.mount: container not found");
    el.innerHTML = MARKUP;
    cbLoadToolNames();
    cbLoadTypes();
    cbSetView(1);
    cbRenderAll();
    if (opts.agentId) loadAgent(opts.agentId);
    return api;
  }

  // ── loading an existing agent's contract ─────────────────────────
  async function loadAgent(agentId) {
    cbSetStatusChip("Loading " + agentId + "…", "busy");
    // Set before the fetch, so a contract compiled after a failed load still
    // carries the right title rather than an empty one.
    cbTitleFor = agentId;
    let data;
    try {
      const res = await fetch(
        "/api/contracts/decompile/" + encodeURIComponent(agentId),
      );
      if (!res.ok) {
        cbSetStatusChip("Could not load " + agentId, "bad");
        return null;
      }
      data = await res.json();
    } catch (e) {
      cbSetStatusChip("Network error", "bad");
      return null;
    }

    document.getElementById("cb-tools").value = (data.tool_names || []).join(
      ", ",
    );

    if (!data.sketch) {
      // No contract, or one too rich to read back. Either way the tools are
      // the useful starting point.
      cbBlocks = [];
      cbSetView(data.has_contract ? 2 : 1);
      cbRenderAll();
      cbLoadProposals();
      cbSetStatusChip(
        data.has_contract ? "Contract not readable here" : "No contract yet",
        data.has_contract ? "bad" : "",
      );
      return data;
    }

    const sk = data.sketch;
    document.getElementById("cb-domain").value = sk.domain || "";
    document.getElementById("cb-type").value = sk.produces_schema || "";
    const syn = document.getElementById("cb-synthesis");
    if (syn) syn.value = sk.synthesis || "";
    const sig = document.getElementById("cb-cal-signal");
    if (sig) sig.value = (sk.calibration && sk.calibration.signal) || "";
    cbBlocks = cbBlocksFromSketch(sk);
    cbOpen = new Set();
    // Open straight onto the view that owes something. A contract loaded with
    // four missing `why`s should not present itself as a naming exercise.
    const owes = cbBlocks.some((b) => (b.why || "").trim().length < 40);
    cbSetView(owes ? 2 : 3);
    cbRenderAll();
    cbLoadProposals();
    cbCompile();
    return data;
  }

  // ── saving back ──────────────────────────────────────────────────
  //
  // Only a compiled contract can be saved. Saving a partial one would put a
  // card into a state the publish gate refuses, which is worse than not
  // saving: the agent keeps running and nobody sees the refusal until
  // publish.
  async function saveTo(agentId) {
    if (!cbCompiled) {
      cbSetStatusChip("Nothing to save — it does not compile yet", "bad");
      return false;
    }
    cbSetStatusChip("Saving…", "busy");
    try {
      const res = await fetch("/api/agents/" + encodeURIComponent(agentId), {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          output_contract: cbCompiled.output_contract,
          produces: cbCompiled.produces,
        }),
      });
      if (!res.ok) {
        cbSetStatusChip("Save refused: " + (await res.text()), "bad");
        return false;
      }
      cbSetStatusChip("Saved to " + agentId, "ok");
      return true;
    } catch (e) {
      cbSetStatusChip("Network error", "bad");
      return false;
    }
  }

  // ── an unavailable block, turned into a tool brief ───────────────
  async function requestTool(agentId, blockName) {
    const b = cbBlocks.find((x) => x.name === blockName);
    if (!b) return null;
    const res = await fetch("/api/contracts/tool-request", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        agent_id: agentId || "(unnamed agent)",
        block: b.name,
        would_need: b.would_need,
        fields: b.fields.map((f) => f.name).filter(Boolean),
      }),
    });
    return res.ok ? await res.json() : null;
  }

  // The markup uses inline handlers, so these have to be reachable from the
  // global scope. Exported from INSIDE the closure because that is where they
  // live; the widget's state stays private either way.
  //
  // Inline handlers rather than delegated listeners is the existing style in
  // this codebase's templates, and matching it was cheaper than converting a
  // thousand lines to something the surrounding pages do not do.
  // `globalThis`, not `window`. They are the same object in a browser, and
  // naming the one that is always defined means the widget can be exercised
  // outside one — which is how the `esc` coupling above got found.
  Object.assign(globalThis, {
    cbSetView, cbAddBlock, cbDelBlock, cbAddField, cbDelField,
    cbSet, cbSetField, cbSetStatus, cbSetShape, cbRenameBlock,
    cbToggle, cbAddFromProposal, cbAddJudgement, cbAddProse, cbAddGap,
    cbBorrow, cbTab, cbToolsChanged, cbTouch, cbLoadExample, cbClear,
    cbToggleShapeField, cbAdoptSource,
    cbCopySnippet, cbRenderAll, cbRenderNav, cbRenderConsumers,
  });

  // The markup's ontology section uses the wizard's collapsible helper. Define
  // it only if the host page has not: on /contracts there is no wizard.
  if (typeof globalThis.toggleCollapsible !== "function") {
    globalThis.toggleCollapsible = (hdr) => {
      const body = hdr.nextElementSibling;
      if (!body) return;
      const open = body.classList.toggle("open");
      const arrow = hdr.querySelector(".arrow");
      if (arrow) arrow.innerHTML = open ? "&#9660;" : "&#9654;";
    };
  }

  const api = {
    mount,
    /// Inject declared response shapes without a server.
    ///
    /// The widget is exercised headlessly (see the DOM-stub harness used while
    /// building it), and the tool-driven field picker is the part most worth
    /// exercising: it is what stops an author naming a response key that does
    /// not exist. A seam on the api object rather than a bare global, so it is
    /// part of the widget's surface rather than a hook someone finds later and
    /// has to guess about.
    setShapes(arr) {
      cbShapes = {};
      (arr || []).forEach((x) => {
        cbShapes[x.tool] = x;
      });
    },
    loadAgent,
    saveTo,
    requestTool,
    compile: () => cbCompile(),
    get compiled() {
      return cbCompiled;
    },
    get sketch() {
      return cbSketch();
    },
    get blocks() {
      return cbBlocks;
    },
    toolNames: () => cbToolNames(),
    setView: (n) => cbSetView(n),
  };
  return api;
})();
