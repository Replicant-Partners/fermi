// Agent configuration fields — one renderer, one save path, many groups.
//
// # Why a field spec rather than two hand-written panels
//
// The shelf needs an Intelligence panel and a Manage panel. Written by hand
// those are two forms with two save paths that drift, and the create wizard
// already has a third copy of the same fields. So the FIELDS table below is the
// data and this file is the only renderer, which is the rule this repo keeps
// arriving at: one object, one rendering.
//
// The same table serves creation. `mount({ agentId: null })` collects values and
// hands them back instead of saving, so a creation flow is this widget in a
// different mode rather than a fourth copy of the fields.
//
// # Only what the endpoint accepts
//
// Every key here is a field `PUT /api/agents/:agent_id` actually takes, checked
// against `AgentUpdate` rather than assumed. Two obvious candidates are
// deliberately absent:
//
//   status, visibility  — the PUT REFUSES them. `reject_lifecycle_fields`
//                         returns "lifecycle transitions run through the publish
//                         pipeline so publish checks, fees and audit logging are
//                         applied. Use POST .../publish, /archive or /restore."
//
// So they are rendered as lifecycle ACTIONS pointing at those endpoints, never
// as fields. A control the endpoint refuses is worse than no control, because
// the refusal arrives after the click — the same rule that governs which tools
// the trace offers as buttons.
//
// # Save is a diff
//
// Only changed fields are sent. A PUT echoing every value back would make the
// `agent_card.updated` event claim the author changed twenty things when they
// changed one, and `collect_changed_fields` on the server exists precisely so
// that event can say what moved.
window.AgentFields = (function () {
  const esc = (s) =>
    String(s ?? "").replace(/[&<>"']/g, (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

  // ── Markdown, because that is what a system prompt already is ──────────
  //
  // Authors write headings, underlines, fenced JSON blocks and bullet lists
  // into the prompt box, and every surface rendered it back as one wall of
  // monospace. Nothing was wrong with the text; the reader had to be the
  // parser. `football_analyst`'s prompt has a `STRUCTURED OUTPUT — REQUIRED`
  // heading over an `====` rule, a fenced type name and eight bullets of
  // provenance vocabulary, and none of it looked like structure.
  //
  // Escape first, format second. Every rule below runs over text that is
  // already HTML-escaped, so there is no path from prompt text to markup —
  // which matters more here than in a chat bubble, because a prompt is
  // attacker-adjacent content on an owner's configuration surface.
  //
  // Deliberately small: the subset prompts actually use. Not a CommonMark
  // implementation, and it does not need to be one.
  const NUL = "\u0000";

  // Inline spans. Code is lifted out first so `**` inside a code span is left
  // alone — the JSON in a prompt is full of characters that look like markup.
  function mdInline(t) {
    const codes = [];
    let s = String(t).replace(/`([^`\n]+)`/g, (_, c) => {
      codes.push(c);
      return `${NUL}C${codes.length - 1}${NUL}`;
    });
    s = s
      // Only http(s). A scheme allowlist rather than a denylist, because the
      // text is already escaped and the href is the one place that is not.
      .replace(/\[([^\]\n]+)\]\((https?:\/\/[^\s)]+)\)/g,
        '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>')
      .replace(/\*\*([^*\n]+)\*\*/g, "<strong>$1</strong>")
      .replace(/(^|[^*\w])\*([^*\n]+)\*(?![*\w])/g, "$1<em>$2</em>");
    return s.replace(new RegExp(NUL + "C(\\d+)" + NUL, "g"),
      (_, i) => `<code>${codes[i]}</code>`);
  }

  function md(src) {
    const text = String(src ?? "");
    if (!text.trim()) return "";

    // Fenced blocks come out whole, before anything else can touch them.
    const blocks = [];
    let body = text.replace(/```([A-Za-z0-9_+-]*)\n?([\s\S]*?)```/g, (_, lang, code) => {
      blocks.push(`<pre class="af-md-pre"${lang ? ` data-lang="${esc(lang)}"` : ""
        }><code>${esc(code.replace(/\n$/, ""))}</code></pre>`);
      return `${NUL}B${blocks.length - 1}${NUL}`;
    });
    body = esc(body);

    const out = [];
    let para = [];
    let list = null;
    let quote = [];
    const flushPara = () => {
      if (!para.length) return;
      out.push(`<p>${mdInline(para.join("<br>"))}</p>`);
      para = [];
    };
    const flushQuote = () => {
      if (!quote.length) return;
      out.push(`<blockquote>${mdInline(quote.join("<br>"))}</blockquote>`);
      quote = [];
    };
    const closeList = () => { if (list) { out.push(`</${list}>`); list = null; } };
    const openList = (kind) => {
      if (list === kind) return;
      closeList();
      out.push(`<${kind}>`);
      list = kind;
    };
    const flushAll = () => { flushPara(); flushQuote(); closeList(); };

    body.split("\n").forEach((raw) => {
      const line = raw.replace(/\s+$/, "");
      const blockRef = new RegExp("^" + NUL + "B(\\d+)" + NUL + "$").exec(line.trim());
      if (blockRef) { flushAll(); out.push(blocks[blockRef[1]]); return; }
      if (!line.trim()) { flushAll(); return; }

      // `TITLE` over `=====` is how prompts on this platform write a heading,
      // and it predates anyone thinking of them as Markdown. Honoured.
      if (/^={2,}$/.test(line.trim()) && para.length) {
        const head = para.pop();
        flushPara();
        out.push(`<h4>${mdInline(head)}</h4>`);
        return;
      }
      if (/^-{3,}$/.test(line.trim())) {
        if (para.length) {
          const head = para.pop();
          flushPara();
          out.push(`<h5>${mdInline(head)}</h5>`);
        } else { flushAll(); out.push("<hr>"); }
        return;
      }

      let m;
      if ((m = /^(#{1,6})\s+(.*)$/.exec(line))) {
        flushAll();
        const n = Math.min(6, m[1].length + 2); // h1 in a panel is a shout
        out.push(`<h${n}>${mdInline(m[2])}</h${n}>`);
        return;
      }
      if ((m = /^\s*(?:[-*+])\s+(.*)$/.exec(line))) {
        flushPara(); flushQuote(); openList("ul");
        out.push(`<li>${mdInline(m[1])}</li>`);
        return;
      }
      if ((m = /^\s*\d+[.)]\s+(.*)$/.exec(line))) {
        flushPara(); flushQuote(); openList("ol");
        out.push(`<li>${mdInline(m[1])}</li>`);
        return;
      }
      if ((m = /^&gt;\s?(.*)$/.exec(line))) {
        flushPara(); closeList();
        quote.push(m[1]);
        return;
      }
      flushQuote(); closeList();
      para.push(line);
    });
    flushAll();
    return out.join("\n");
  }

  // Timestamps, short and sortable. A version list is read down a column.
  const when = (iso) => {
    const d = new Date(iso);
    return isNaN(d.getTime())
      ? String(iso ?? "\u2014")
      : d.toISOString().slice(0, 16).replace("T", " ");
  };

  // `path` is how a value is read out of the served profile, which is nested;
  // `key` is what the endpoint takes, which is flat.
  const FIELDS = [
    // ── intelligence ─────────────────────────────────────────────────────
    { group: "intelligence", key: "llm_provider", path: "substrate.provider",
      label: "fallback provider", kind: "select",
      options: ["anthropic", "openai", "openai-azure", "google", "mistral", "local"],
      help: "Used only when no ladder rung matches the caller's tier. Changing it " +
            "without changing the model is how an agent ends up asking Anthropic " +
            "for a GPT model." },
    { group: "intelligence", key: "model", path: "substrate.model",
      label: "fallback model", kind: "text",
      help: "The default, NOT necessarily what runs. `apply_tier_resolution` picks " +
            "the highest ladder rung at or below the caller's tier and overwrites " +
            "both fields; this stands only when no rung matches." },
    { group: "intelligence", key: "temperature", path: "substrate.temperature",
      label: "temperature", kind: "slider", step: "0.05", min: "0", max: "2", mid: "0.5",
      help: "Higher is more varied. An agent under a field contract is being " +
            "asked for retrievable facts, and variance there is noise rather " +
            "than creativity. Extended thinking (Anthropic) forces this to 1.0." },
    // model_params is the provider-specific sampling config. `kind: \"sampling\"`
    // renders only the params the selected provider actually supports, so authors
    // cannot set top_k for OpenAI or frequency_penalty for Anthropic.
    { group: "intelligence", key: "model_params", path: "model_params",
      label: "sampling parameters", kind: "sampling",
      members: [
        { key: "max_tokens" }, { key: "top_p" }, { key: "top_k" },
        { key: "extended_thinking" }, { key: "thinking_budget_tokens" },
        { key: "frequency_penalty" }, { key: "presence_penalty" },
        { key: "repetition_penalty" }, { key: "random_seed" },
      ],
      help: "Provider-specific sampling parameters (max_tokens, top-P/K, extended " +
            "thinking for Anthropic, frequency/presence penalty for OpenAI, " +
            "repetition penalty for Mistral/local). These are applied on top of " +
            "temperature by `resolve_sampling_params` at every LLM call." },
    // Its own group, and first. The prompt is not documentation: a substring in
    // it decides whether the agent gets a tool loop at all, so it is the first
    // thing an author needs and it was three panels down inside `Brain`.
    { group: "prompt", key: "system_prompt", path: "system_prompt",
      label: "system prompt", kind: "prompt", rows: 16,
      help: "Markdown. `read` renders it the way it is written; `history` is the " +
            "`agent_versions` trail, one row per save, so a pulse read next month " +
            "still resolves to the exact text that produced it." },
    // ── manage ────────────────────────────────────────────────────────
    { group: "manage", key: "display_alias", path: "label",
      label: "display name", kind: "text",
      help: "What surfaces show. The agent name is the identity and does not change." },
    { group: "manage", key: "tags", path: "tags", label: "tags", kind: "tags",
      help: "Comma separated. Used for discovery, not for capability — a tag " +
            "cannot make an agent able to do anything." },
    { group: "manage", key: "min_tier", path: "min_tier",
      label: "minimum tier", kind: "text",
      help: "The lowest tier of caller allowed to invoke this agent." },
    // ── competition ─────────────────────────────────────────────────────
    //
    // What this agent declares about its participation in open-slot selection.
    // Authors fill these; the platform computes fidelity and selection rate
    // from gate history and select_agent_decisions — those are not editable.
    { group: "competition", key: "competition", path: "competition",
      label: "competition", kind: "object",
      help: "How this agent competes for open coordination graph slots. Declare " +
            "domains and price so select_agent can rank you against alternatives. " +
            "Fidelity and selection rate are platform-computed from gate history.",
      members: [
        { key: "domains", label: "domains", kind: "tags" },
        { key: "price_credits_per_call", label: "price (credits/call)", kind: "number" },
        { key: "support_tier", label: "support tier", kind: "text" },
      ] },
    // ── ports ─────────────────────────────────────────────────────────────
    { group: "ports", key: "accepts", path: "accepts",
      label: "accepts", kind: "tags",
      help: "Labels this agent declares it can consume. A stud connects where " +
            "another agent's `produces` carries a matching label. Bare nouns " +
            "and schema IDs (e.g. fermi/forecast_request) are both valid." },
    // Valence is a structured object and was missing entirely, because the spec
    // had no kind for one. A raw JSON box is how somebody writes valid JSON of
    // the wrong shape, so the four members it actually has get four controls:
    // `AgentValence { primary_affect, arousal, valence, personality_traits }`.
    //
    // Not decoration. Composition dreaming audits the valence distribution
    // across a workspace and flags homophily when arousal or valence spread
    // falls below 0.25, which is a real proposal about the team's membership.
    // Personality, as a plane rather than two numbers.
    //
    // `AgentValence { primary_affect, arousal, valence, personality_traits }`.
    // Arousal and valence are the affect circumplex and only mean anything
    // together — calm/excited against negative/positive — so they get one pad
    // with the quadrant words people actually use, and two hidden inputs keep
    // the save path exactly as it was.
    //
    // Not decoration: `propose_composition_change` audits the spread of these
    // across a workspace and flags homophily below 0.25. An all-alike team is a
    // team that agrees too easily.
    { group: "personality", key: "valence", path: "valence", label: "affect", kind: "object",
      help: "Where this agent sits, and how far that is from its teammates.",
      members: [
        { key: "primary_affect", label: "in a word", kind: "text" },
        { key: "personality_traits", label: "traits", kind: "tags" },
      ],
      pad: {
        key: "valence", label: "affect", kind: "pad",
        x: { key: "valence", label: "valence", min: "-1", max: "1" },
        y: { key: "arousal", label: "arousal", min: "0", max: "1" },
        quadrants: [
          { at: "tl", word: "tense" }, { at: "tr", word: "eager" },
          { at: "bl", word: "flat" }, { at: "br", word: "calm" },
        ],
      } },
  ];

  /// The four cognition tiers, in resolution order. A caller asks at a tier and
  /// gets the highest rung at or below it.
  const TIERS = ["local", "free", "standard", "premium"];

  const at = (obj, path) =>
    String(path).split(".").reduce((o, k) => (o == null ? o : o[k]), obj);

  // Provider → which sampling params are relevant.
  // Mirrors the SamplingParams struct that resolve_sampling_params reads.
  const SAMPLING_ROWS = [
    { key: "max_tokens",             label: "max tokens",            type: "number", min: 1,  max: 200000, step: 1,    providers: "*" },
    { key: "top_p",                  label: "top-P",                 type: "number", min: 0,  max: 1,      step: 0.01, providers: "anthropic openai openai-azure google mistral local" },
    { key: "top_k",                  label: "top-K",                 type: "number", min: 0,  max: 100,    step: 1,    providers: "anthropic google local" },
    { key: "extended_thinking",      label: "extended thinking",     type: "bool",                                     providers: "anthropic",
      note: "Forces temperature to 1.0 — enforced by the platform." },
    { key: "thinking_budget_tokens", label: "thinking budget (tokens)", type: "number", min: 1024, step: 1024, providers: "anthropic", extBudget: true },
    { key: "frequency_penalty",      label: "frequency penalty",     type: "number", min: -2, max: 2,      step: 0.01, providers: "openai openai-azure" },
    { key: "presence_penalty",       label: "presence penalty",      type: "number", min: -2, max: 2,      step: 0.01, providers: "openai openai-azure" },
    { key: "repetition_penalty",     label: "repetition penalty",    type: "number", min: 0,  max: 2,      step: 0.01, providers: "mistral local" },
    { key: "random_seed",            label: "seed",                  type: "number", min: 0,               step: 1,    providers: "openai openai-azure google mistral" },
  ];

  function control(f, value) {
    const v = value == null ? "" : value;
    const common = `data-field="${f.key}" data-kind="${f.kind}"`;
    if (f.kind === "select") {
      // A dropdown from a known set. Used for provider (so the options are
      // discoverable) and for any field with a closed vocabulary. Falls back to
      // a text input for any unknown value already on the agent.
      const opts = f.options || [];
      const optHtml = opts.map((o) => {
        const val = typeof o === "string" ? o : o.value;
        const lbl = typeof o === "string" ? o : (o.label || o.value);
        return `<option value="${esc(val)}"${v === val ? " selected" : ""}>${esc(lbl)}</option>`;
      }).join("");
      // If current value is not in the list, prepend it as a placeholder option
      // so the author can see what is set before changing it.
      const known = opts.map((o) => typeof o === "string" ? o : o.value);
      const unknownOpt = v && !known.includes(v)
        ? `<option value="${esc(v)}" selected>${esc(v)} (custom)</option>` : "";
      return `<select ${common}>${unknownOpt}${optHtml}</select>`;
    }
    if (f.kind === "sampling") {
      // Provider-aware sampling params. Each row carries data-sp-providers so
      // a change to llm_provider shows/hides the right rows without a page reload.
      // Rows hidden for the current provider are also excluded from the assembled
      // model_params object, so switching from anthropic to openai does not
      // accidentally carry over top_k.
      const obj = value && typeof value === "object" ? value : {};
      return `<div class="af-sampling" data-sp-container data-object="${f.key}">${
        SAMPLING_ROWS.map((p) => {
          const id = `${f.key}.${p.key}`;
          const pv = obj[p.key];
          const field_attrs = `data-field="${esc(id)}" data-kind="${p.type === "bool" ? "bool" : "number"}"`;
          const inp = p.type === "bool"
            ? `<input ${field_attrs} type="checkbox"${pv ? " checked" : ""}>`
            : `<input ${field_attrs} type="number" step="${p.step || 1}"${
                p.min != null ? ` min="${p.min}"` : ""}${
                p.max != null ? ` max="${p.max}"` : ""} value="${esc(pv ?? "")}">` ;
          return `<div class="af-sp-row" data-sp-providers="${esc(p.providers)}"
                      ${p.extBudget ? "data-sp-ext-budget" : ""}>
            <label class="af-sp-label">${esc(p.label)}</label>${inp}${
              p.note ? `<span class="af-sp-note">${esc(p.note)}</span>` : ""}
          </div>`;
        }).join("")
      }</div>`;
    }
    if (f.kind === "object") {
      // One control per member, each carrying its parent so `read` can rebuild
      // the object. Sub-controls are the members the struct actually has, so a
      // key it does not have cannot be typed.
      const obj = value && typeof value === "object" ? value : {};
      const pad = f.pad ? control({ ...f.pad, key: f.key }, obj) : "";
      return `<div class="af-obj" data-object="${f.key}">${pad}${f.members.map(m =>
        `<div class="af-sub">
          <label class="af-sublabel">${esc(m.label)}</label>
          ${control({ ...m, key: `${f.key}.${m.key}` }, obj[m.key])}
        </div>`).join("")}</div>`;
    }
    if (f.kind === "textarea") {
      return `<textarea ${common} rows="${f.rows || 7}">${esc(v)}</textarea>`;
    }
    if (f.kind === "prompt") {
      // One text, three views of it.
      //
      // `write` is the box that was always here. `read` renders the Markdown
      // the author already writes, so a prompt with headings and a fenced
      // schema reads as a document rather than as a wall to parse by eye.
      // `history` is `agent_versions` — a table that has recorded every prompt
      // edit since mig-024 and had no surface at all, which is the wrong state
      // for the one field that decides whether the agent gets tools.
      //
      // The textarea keeps its `data-field`, so the diff, the save and every
      // check downstream are untouched. This is an interface over the same
      // control, not a second way to write the prompt.
      return `<div class="af-prompt" data-prompt>
        <div class="af-tabs">
          <button type="button" class="af-tab on" data-ptab="write">write</button>
          <button type="button" class="af-tab" data-ptab="read">read</button>
          <button type="button" class="af-tab" data-ptab="history">history</button>
          <span class="af-tab-meta" data-prompt-meta></span>
        </div>
        <div data-ppane="write"><textarea ${common} rows="${f.rows || 14}"
          spellcheck="false">${esc(v)}</textarea></div>
        <div class="af-md" data-ppane="read" hidden></div>
        <div class="af-vers" data-ppane="history" hidden></div>
      </div>`;
    }
    if (f.kind === "slider") {
      // A number you drag, with the value beside it. `temperature` is the case:
      // a free-text box invites 1.7 on an agent under a field contract, and a
      // track with a marked range makes the sane band visible without refusing
      // anything.
      return `<div class="af-slide">
        <input ${common} type="range" step="${f.step || "0.05"}"
               min="${f.min}" max="${f.max}" value="${esc(v === "" ? f.mid ?? f.min : v)}"/>
        <output class="af-val">${esc(v === "" ? "\u2014" : v)}</output>
      </div>`;
    }
    if (f.kind === "pad") {
      // Two dimensions that only mean anything together.
      //
      // Arousal and valence are the affect circumplex: calm/excited against
      // negative/positive. As two number boxes they are two numbers; as a plane
      // they are a personality you can point at, and the quadrant names are the
      // words people actually use for the result.
      const x = Number(v && v[f.x.key] != null ? v[f.x.key] : 0);
      const y = Number(v && v[f.y.key] != null ? v[f.y.key] : 0.5);
      return `<div class="af-pad" data-pad="${f.key}"
                   data-xkey="${f.x.key}" data-ykey="${f.y.key}"
                   data-xmin="${f.x.min}" data-xmax="${f.x.max}"
                   data-ymin="${f.y.min}" data-ymax="${f.y.max}">
        <div class="af-pad-face" tabindex="0" role="slider"
             aria-label="${esc(f.label)}">
          ${f.quadrants.map(q => `<span class="af-q af-q-${q.at}">${esc(q.word)}</span>`).join("")}
          <span class="af-axis af-axis-x"></span><span class="af-axis af-axis-y"></span>
          <span class="af-dot"></span>
        </div>
        <div class="af-pad-read">
          <span>${esc(f.x.label)} <b data-pad-x>${x.toFixed(2)}</b></span>
          <span>${esc(f.y.label)} <b data-pad-y>${y.toFixed(2)}</b></span>
          <span class="af-q-now" data-pad-word></span>
        </div>
        <input type="hidden" data-field="${f.key}.${f.x.key}" data-kind="number" value="${x}"/>
        <input type="hidden" data-field="${f.key}.${f.y.key}" data-kind="number" value="${y}"/>
      </div>`;
    }
    if (f.kind === "number") {
      return `<input ${common} type="number" step="${f.step || "any"}"${
        f.min != null ? ` min="${f.min}"` : ""}${f.max != null ? ` max="${f.max}"` : ""
        } value="${esc(v)}"/>`;
    }
    if (f.kind === "tags") {
      return `<input ${common} type="text" value="${
        esc(Array.isArray(v) ? v.join(", ") : v)}"/>`;
    }
    return `<input ${common} type="text" value="${esc(v)}"/>`;
  }

  // What the browser holds, as the type the endpoint wants.
  function read(el) {
    // Checkboxes are a special case: el.value is always "on", el.checked is
    // the signal. Checked before the kind switch so any kind can be bool.
    if (el && el.type === "checkbox") return el.checked;
    const raw = (el && el.value) || "";
    switch (el && el.dataset && el.dataset.kind) {
      case "slider":    // range input — same numeric coercion as number
      case "number": {
        if (raw.trim() === "") return null;
        const n = Number(raw);
        return Number.isFinite(n) ? n : null;
      }
      case "tags":
        return raw.split(",").map((t) => t.trim()).filter(Boolean);
      default:
        // Empty is null, not "". An empty string is a value the agent would
        // then carry; absent is what the author meant.
        return raw.trim() === "" ? null : raw;
    }
  }

  // Objects and arrays compare by value. `valence` is an object and a member
  // edit has to register as a change to it.
  const same = (a, b) => {
    const structural = (x) => x !== null && typeof x === "object";
    if (structural(a) || structural(b)) {
      return JSON.stringify(a ?? null) === JSON.stringify(b ?? null);
    }
    return (a ?? null) === (b ?? null);
  };

  // Lifecycle is not a field. Which action is offered depends on where the agent
  // is, because offering "publish" to a published agent is offering nothing.
  function lifecycle(profile) {
    const status = String(profile.status || "").toLowerCase();
    const acts = [];
    if (status !== "active" && status !== "published") {
      acts.push(["publish", "publish", "runs the publish checks, charges the fee, and logs it"]);
    }
    if (status !== "archived") {
      acts.push(["archive", "archive", "withdraws it from discovery. Reversible"]);
    } else {
      acts.push(["restore", "restore", "returns it to its previous state"]);
    }
    return `<div class="af-life">
      <div class="af-life-now">status <b>${esc(profile.status || "\u2014")}</b>
        \u00b7 visibility <b>${esc(profile.visibility || "\u2014")}</b></div>
      <div class="af-note">Neither is a field. <code>PUT /api/agents/:id</code> refuses
        both, because lifecycle transitions run through the publish pipeline so the
        checks, the fee and the audit trail are applied. These do that.</div>
      <div class="af-acts">${acts.map(([act, label, why]) =>
        `<button class="af-life-btn" data-lifecycle="${act}" title="${esc(why)}"
          >${esc(label)}</button>`).join("")}</div>
      <div class="af-out" data-life-out></div>
    </div>`;
  }

  // The ladder, and which rung each tier resolves to.
  //
  // Read-only for now, and stated as such. But NOT omitted: an Intelligence
  // panel that shows one model for an agent whose model is chosen per call is
  // lying by omission, and the fallback fields above only make sense once you
  // can see what overrides them.
  //
  // The resolution rule is `AgentCard::apply_tier_resolution`: the highest rung
  // at or below the requested tier wins, and if none matches the fallback
  // stands. Computed here rather than described, because a reader wants to know
  // what a `free` caller gets, not the algorithm.
  function ladderBlock(profile) {
    const raw = at(profile, "substrate.model_ladder");
    const rungs = Array.isArray(raw) ? raw : [];
    const fb = {
      provider: at(profile, "substrate.provider"),
      model: at(profile, "substrate.model"),
    };
    if (!rungs.length) {
      return `<div class="af-ladder">
        <div class="af-sublabel">model ladder</div>
        <div class="af-note">No ladder. Every caller gets the fallback above \u2014
          <b>${esc(fb.model || "no model set")}</b> on
          <b>${esc(fb.provider || "no provider set")}</b> \u2014 whatever tier they ask at.</div>
      </div>`;
    }
    const idx = (t) => TIERS.indexOf(String(t || "").toLowerCase());
    const resolve = (tier) => {
      const want = idx(tier);
      let best = null;
      rungs.forEach((r) => {
        const at_ = idx(r.tier);
        if (at_ >= 0 && at_ <= want && (best === null || at_ > idx(best.tier))) best = r;
      });
      return best;
    };
    return `<div class="af-ladder">
      <div class="af-sublabel">model ladder \u00b7 ${rungs.length} rung(s)</div>
      <div class="af-note">What actually runs, by the tier the caller asks at. The
        highest rung at or below that tier wins; the fallback above stands only where
        no rung matches. Read-only here \u2014 the rung editor is next.</div>
      ${TIERS.map(t => {
        const r = resolve(t);
        return `<div class="af-rung ${r ? "on" : "off"}">
          <span class="af-tier">${esc(t)}</span>
          <span class="af-what">${r
            ? `${esc(r.model)} <span class="af-dim">on ${esc(r.provider)}</span>${
                r.eval_score != null ? ` <span class="af-dim">\u00b7 eval ${
                  esc(String(r.eval_score))}</span>` : ""}`
            : `<span class="af-dim">falls back to ${esc(fb.model || "nothing set")}</span>`}
          </span>
        </div>`;
      }).join("")}
    </div>`;
  }

  function mount(opts) {
    const el = typeof opts.container === "string"
      ? document.getElementById(opts.container) : opts.container;
    if (!el) throw new Error("AgentFields.mount: container not found");
    const group = opts.group;
    const profile = opts.profile || {};
    const fields = FIELDS.filter((f) => f.group === group);

    // The values as served, kept so save can send a diff rather than everything.
    const initial = {};
    fields.forEach((f) => { initial[f.key] = at(profile, f.path); });

    // An object field's value is assembled from its members at save time, so the
    // diff is computed against the whole object rather than per member — the
    // endpoint takes `valence`, not `valence.arousal`.
    //
    // `sampling` is also object-like (same assembly path) but only non-null,
    // non-false values are included, and rows hidden by the provider filter are
    // skipped. A hidden row keeps its DOM value; we just do not send it so the
    // PUT does not carry openai-only params when the provider is anthropic.
    const objectOf = (key) => fields.find(
      (f) => (f.kind === "object" || f.kind === "sampling") && f.key === key);
    const assemble = (f, inputs) => {
      const out = {};
      let any = false;
      const isSampling = f.kind === "sampling";
      // The pad's two axes are members too — they just have one control between
      // them. Listed first so a valence written by hand keeps its key order.
      const keys = (f.pad ? [f.pad.x.key, f.pad.y.key] : [])
        .concat(f.members.map((m) => m.key));
      keys.forEach((k) => {
        const el = inputs.find((i) => i.dataset.field === `${f.key}.${k}`);
        if (!el) return;

        if (isSampling) {
          // Skip rows that are hidden for the current provider.
          const row = el.closest ? el.closest("[data-sp-providers]") : null;
          if (row && row.dataset.spHidden === "1") return;
          const v = read(el);
          // Skip null and false — for model_params, absent means \"use platform
          // default\"; false on a bool means the same thing.
          if (v === null || v === false) return;
          out[k] = v;
          any = true;
        } else {
          const v = read(el);
          if (v !== null && !(Array.isArray(v) && v.length === 0)) any = true;
          out[k] = v;
        }
      });
      // Every member emptied means the author cleared it, which is `null` and
      // not an object of nulls.
      return any ? out : null;
    };

    // The help line is escaped first and inline-formatted second. Every entry
    // in the FIELDS table names platform symbols in backticks —
    // `apply_tier_resolution`, `AgentUpdate`, `resolve_sampling_params` — and
    // printing them as literal backticks made the one sentence explaining a
    // field cost more to read than the field.
    el.innerHTML = `
      ${fields.map((f) => `<div class="af-row">
        <label class="af-label" for="af-${esc(f.key)}">${esc(f.label)}</label>
        ${control(f, initial[f.key])}
        <div class="af-help">${mdInline(esc(f.help))}</div>
      </div>`).join("")}
      ${group === "intelligence" ? ladderBlock(profile) : ""}
      ${group === "manage" ? lifecycle(profile) : ""}
      <div class="af-bar">
        <button class="af-save" data-af-save disabled>save</button>
        <span class="af-out" data-af-out></span>
      </div>`;

    const inputs = [...el.querySelectorAll("[data-field]")];

    // ── the affect pad ───────────────────────────────────────────────
    //
    // Pointer-driven, and it writes through the two hidden inputs so the diff,
    // the save and every check downstream see exactly what a pair of number
    // boxes would have produced. The plane is the interface; the data is
    // unchanged.
    el.querySelectorAll("[data-pad]").forEach((pad) => {
      const face = pad.querySelector(".af-pad-face");
      const dot = pad.querySelector(".af-dot");
      const xIn = pad.querySelector(`[data-field="${pad.dataset.pad}.${pad.dataset.xkey}"]`);
      const yIn = pad.querySelector(`[data-field="${pad.dataset.pad}.${pad.dataset.ykey}"]`);
      if (!face || !dot || !xIn || !yIn) return;
      const xr = [Number(pad.dataset.xmin), Number(pad.dataset.xmax)];
      const yr = [Number(pad.dataset.ymin), Number(pad.dataset.ymax)];
      const words = [...pad.querySelectorAll(".af-q")].map((q) => ({
        at: [...q.classList].find((c) => c.startsWith("af-q-")).slice(5),
        word: q.textContent,
      }));

      const paint = () => {
        const x = Number(xIn.value), y = Number(yIn.value);
        const fx = (x - xr[0]) / (xr[1] - xr[0]);
        const fy = (y - yr[0]) / (yr[1] - yr[0]);
        dot.style.left = `${(fx * 100).toFixed(1)}%`;
        dot.style.bottom = `${(fy * 100).toFixed(1)}%`;
        const rx = pad.querySelector("[data-pad-x]"), ry = pad.querySelector("[data-pad-y]");
        if (rx) rx.textContent = x.toFixed(2);
        if (ry) ry.textContent = y.toFixed(2);
        const at = (fy >= 0.5 ? "t" : "b") + (fx >= 0.5 ? "r" : "l");
        const w = pad.querySelector("[data-pad-word]");
        const found = words.find((q) => q.at === at);
        if (w && found) w.textContent = found.word;
      };

      const round = (n) => Math.round(n * 20) / 20;
      const set = (clientX, clientY) => {
        const r = face.getBoundingClientRect();
        const fx = Math.min(1, Math.max(0, (clientX - r.left) / r.width));
        const fy = Math.min(1, Math.max(0, 1 - (clientY - r.top) / r.height));
        xIn.value = String(round(xr[0] + fx * (xr[1] - xr[0])));
        yIn.value = String(round(yr[0] + fy * (yr[1] - yr[0])));
        paint();
        refresh();
      };

      face.addEventListener("pointerdown", (ev) => {
        ev.preventDefault();
        face.setPointerCapture(ev.pointerId);
        set(ev.clientX, ev.clientY);
        const move = (e) => set(e.clientX, e.clientY);
        const up = () => {
          face.removeEventListener("pointermove", move);
          face.removeEventListener("pointerup", up);
        };
        face.addEventListener("pointermove", move);
        face.addEventListener("pointerup", up);
      });
      // Reachable without a pointer. A personality you can only set by dragging
      // is a personality some people cannot set.
      face.addEventListener("keydown", (ev) => {
        const step = { ArrowLeft: [-0.05, 0], ArrowRight: [0.05, 0],
                       ArrowDown: [0, -0.05], ArrowUp: [0, 0.05] }[ev.key];
        if (!step) return;
        ev.preventDefault();
        xIn.value = String(round(Math.min(xr[1], Math.max(xr[0], Number(xIn.value) + step[0]))));
        yIn.value = String(round(Math.min(yr[1], Math.max(yr[0], Number(yIn.value) + step[1]))));
        paint();
        refresh();
      });
      paint();
    });

    // Sliders show their value as they move.
    el.querySelectorAll('[data-kind="slider"]').forEach((sl) => {
      const outEl = sl.parentNode && sl.parentNode.querySelector(".af-val");
      if (outEl) sl.addEventListener("input", () => { outEl.textContent = sl.value; });
    });

    // Provider-aware sampling params.
    //
    // When llm_provider changes, show only the rows whose data-sp-providers list
    // includes the new provider. Hidden rows are skipped in assemble() so the PUT
    // does not carry anthropic-only params when the provider is openai.
    //
    // Extended thinking locks temperature at 1.0 (Anthropic enforces this in
    // resolve_sampling_params). When the checkbox is toggled, update the note and
    // optionally grey the temperature slider as a visual reminder.
    const spContainer = el.querySelector("[data-sp-container]");
    if (spContainer) {
      const providerInput = inputs.find((i) => i.dataset.field === "llm_provider");
      const tempInput = inputs.find((i) => i.dataset.field === "temperature");

      const showSamplingRows = (provider) => {
        spContainer.querySelectorAll("[data-sp-providers]").forEach((row) => {
          const providers = (row.dataset.spProviders || "").split(" ");
          const show = providers[0] === "*" || providers.includes(provider);
          row.style.display = show ? "" : "none";
          row.dataset.spHidden = show ? "0" : "1";
        });
        // Extended thinking budget: only visible when the checkbox is checked.
        const extEl = inputs.find((i) => i.dataset.field === "model_params.extended_thinking");
        const budgetRow = spContainer.querySelector("[data-sp-ext-budget]");
        if (budgetRow && extEl) {
          const on = extEl.checked;
          budgetRow.style.display = on ? "" : "none";
          budgetRow.dataset.spHidden = on ? "0" : "1";
        }
        // Visual hint on temperature when extended thinking is locked.
        if (tempInput) {
          const extOn = extEl && extEl.checked;
          tempInput.disabled = extOn;
          const tempOut = tempInput.parentNode && tempInput.parentNode.querySelector(".af-val");
          if (tempOut) tempOut.textContent = extOn ? "1 (locked)" : (tempInput.value || "");
        }
      };

      if (providerInput) {
        const onProviderChange = () => showSamplingRows(providerInput.value);
        providerInput.addEventListener("change", onProviderChange);
        providerInput.addEventListener("input", onProviderChange);
        // Extended thinking toggle also updates budget row visibility.
        const extEl = inputs.find((i) => i.dataset.field === "model_params.extended_thinking");
        if (extEl) extEl.addEventListener("change", () => showSamplingRows(providerInput.value));
        // Initialise from the profile's current provider.
        showSamplingRows(at(profile, "substrate.provider") || "anthropic");
      }
    }

    // ── The prompt: write, read, history ─────────────────────────────
    //
    // `read` renders whatever is in the editor right now, not what was served,
    // so the preview follows an unsaved edit. A preview that silently showed
    // the stored text while the author looked at their own change would be the
    // drift this widget exists to prevent, one layer down.
    const promptBox = el.querySelector("[data-prompt]");
    if (promptBox) {
      const ta = inputs.find((i) => i.dataset.field === "system_prompt");
      const panes = {};
      promptBox.querySelectorAll("[data-ppane]").forEach((p) => {
        panes[p.dataset.ppane] = p;
      });
      const tabs = [...promptBox.querySelectorAll("[data-ptab]")];
      const metaEl = promptBox.querySelector("[data-prompt-meta]");
      // Non-null only while an OLD version is on screen. The banner exists so
      // "this is not your prompt" is never something the reader has to infer.
      let viewing = null;
      let versions = null;

      const stats = () => {
        if (!metaEl) return;
        const t = ta ? String(ta.value || "") : "";
        const pv = at(profile, "substrate.persona_version");
        metaEl.textContent = [
          pv != null ? `persona v${pv}` : null,
          `${t.length.toLocaleString()} characters`,
          `${t ? t.split("\n").length : 0} lines`,
        ].filter(Boolean).join(" \u00b7 ");
      };

      const renderRead = () => {
        const body = viewing ? (viewing.system_prompt || "") : (ta ? ta.value : "");
        const banner = viewing
          ? `<div class="af-vers-banner">Showing <b>v${esc(viewing.version_number)}</b>
               as it was saved on ${esc(when(viewing.created_at))} \u2014 not the text in
               the editor.
               <button type="button" class="af-mini" data-vload>load into editor</button>
               <button type="button" class="af-mini" data-vnow>back to current</button>
             </div>`
          : "";
        panes.read.innerHTML = banner + (String(body).trim()
          ? md(body)
          : `<div class="af-note">No prompt. An agent with no prompt is
               unconfigured rather than broken \u2014 it answers as the bare model
               does, with no persona and nothing said about its contract.</div>`);
      };

      const renderHistory = () => {
        if (!versions.length) {
          panes.history.innerHTML = `<div class="af-note">No versions recorded yet. A
            row is written on every save, so the first edit made here starts the
            trail.</div>`;
          return;
        }
        panes.history.innerHTML = `<div class="af-note">One row per save. The version
          a pulse was produced under is the text that produced it, which is what makes
          this an audit trail rather than a curiosity.</div>`
          + versions.map((v) => `<div class="af-ver">
              <span class="af-ver-n">v${esc(v.version_number)}</span>
              <span class="af-ver-when">${esc(when(v.created_at))}</span>
              <span class="af-ver-who">${esc(v.changed_by || "unknown")}</span>
              <span class="af-ver-what">${esc(v.model || "\u2014")}</span>
              <button type="button" class="af-mini"
                      data-vshow="${esc(v.version_number)}">read</button>
            </div>`).join("");
      };

      const loadHistory = async () => {
        if (versions) { renderHistory(); return; }
        if (!opts.agentId) {
          panes.history.innerHTML = `<div class="af-note">Nothing is saved yet, so
            there is no history to read.</div>`;
          return;
        }
        panes.history.innerHTML = `<div class="af-note">reading the version
          trail\u2026</div>`;
        try {
          const r = await fetch(
            `/api/agents/${encodeURIComponent(opts.agentId)}/versions`);
          if (!r.ok) throw new Error(await r.text());
          versions = ((await r.json()) || {}).versions || [];
        } catch (err) {
          // Unknown, not empty. "No versions" is a claim about the agent;
          // a failed read is a claim about this page, and they look identical
          // if the catch renders the empty state.
          panes.history.innerHTML = `<div class="af-out bad">Could not read the
            version trail, so whether this agent has one is unknown:
            ${esc(err.message)}</div>`;
          return;
        }
        renderHistory();
      };

      const show = (name) => {
        tabs.forEach((t) => {
          if (t.classList) t.classList.toggle("on", t.dataset.ptab === name);
        });
        Object.keys(panes).forEach((k) => { panes[k].hidden = k !== name; });
        if (name === "read") renderRead();
        if (name === "history") loadHistory();
      };

      // Delegated, because the read and history panes rewrite their own
      // contents and re-binding after every render is how a button quietly
      // stops working.
      promptBox.addEventListener("click", async (ev) => {
        const t = ev.target;
        if (!t || !t.dataset) return;
        if (t.dataset.ptab) { show(t.dataset.ptab); return; }
        if (t.dataset.vshow) {
          t.disabled = true;
          try {
            const r = await fetch(`/api/agents/${encodeURIComponent(opts.agentId)}`
              + `/versions/${encodeURIComponent(t.dataset.vshow)}`);
            if (!r.ok) throw new Error(await r.text());
            viewing = await r.json();
            show("read");
          } catch (err) {
            panes.read.innerHTML = `<div class="af-out bad">Could not read
              v${esc(t.dataset.vshow)}: ${esc(err.message)}</div>`;
            show("read");
          }
          t.disabled = false;
          return;
        }
        if ("vload" in t.dataset) {
          // Into the editor, not onto the agent. `POST .../restore` exists and
          // would write immediately; putting the old text in the box makes the
          // rollback an edit the author reviews and saves like any other, and
          // it travels through the same diff.
          if (ta && viewing) {
            ta.value = viewing.system_prompt || "";
            stats();
            refresh();
          }
          viewing = null;
          show("write");
          return;
        }
        if ("vnow" in t.dataset) { viewing = null; renderRead(); return; }
      });

      if (ta) {
        ta.addEventListener("input", () => {
          viewing = null;
          stats();
        });
      }
      stats();
    }

    const saveBtn = el.querySelector("[data-af-save]");
    const out = el.querySelector("[data-af-out]");

    // Member controls are not fields; their object is. So a dotted control
    // reports its PARENT as the changed key, and only once.
    const changed = () => {
      const keys = new Set();
      inputs.forEach((i) => {
        const [head, member] = i.dataset.field.split(".");
        if (member) {
          const f = objectOf(head);
          if (f && !same(assemble(f, inputs), initial[head])) keys.add(head);
          return;
        }
        if (!same(read(i), initial[head])) keys.add(head);
      });
      return [...keys];
    };

    const refresh = () => {
      const n = changed().length;
      saveBtn.disabled = n === 0;
      saveBtn.textContent = n === 0 ? "save" : `save ${n} change${n === 1 ? "" : "s"}`;
    };
    inputs.forEach((i) => i.addEventListener("input", refresh));

    saveBtn.addEventListener("click", async () => {
      const diff = {};
      changed().forEach((key) => {
        const f = objectOf(key);
        if (f) { diff[key] = assemble(f, inputs); return; }
        const el = inputs.find((i) => i.dataset.field === key);
        if (el) diff[key] = read(el);
      });
      if (!Object.keys(diff).length) return;

      // No agentId means creation: hand the values back rather than saving.
      if (!opts.agentId) {
        out.className = "af-out";
        out.textContent = "collected";
        if (opts.onCollect) opts.onCollect(diff);
        return;
      }

      saveBtn.disabled = true;
      out.className = "af-out";
      out.textContent = "saving\u2026";
      let text = null;
      try {
        const r = await fetch(`/api/agents/${encodeURIComponent(opts.agentId)}`, {
          method: "PUT",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(diff),
        });
        text = await r.text();
        if (!r.ok) {
          // The body verbatim. A 400 here is the author's and fixable, and the
          // endpoint's own sentence about it is better than a generic one.
          out.className = "af-out bad";
          out.textContent = text;
          refresh();
          return;
        }
        Object.keys(diff).forEach((k) => { initial[k] = diff[k]; });
        out.className = "af-out ok";
        out.textContent = `saved ${Object.keys(diff).join(", ")}`;
        refresh();
        if (opts.onSaved) opts.onSaved(diff);
      } catch (err) {
        out.className = "af-out bad";
        // A throw before the body arrived is the network; after it is this file.
        out.textContent = text === null
          ? "Could not reach the platform: " + err.message
          : "The platform answered and this page failed to read it: " + err.message;
        refresh();
      }
    });

    // Lifecycle, when this group has it.
    el.querySelectorAll("[data-lifecycle]").forEach((btn) => {
      btn.addEventListener("click", async () => {
        const act = btn.dataset.lifecycle;
        const lout = el.querySelector("[data-life-out]");
        btn.disabled = true;
        lout.className = "af-out";
        lout.textContent = act + "\u2026";
        try {
          const r = await fetch(
            `/api/agents/${encodeURIComponent(opts.agentId)}/${act}`,
            { method: "POST", headers: { "Content-Type": "application/json" },
              body: "{}" });
          const t = await r.text();
          lout.className = r.ok ? "af-out ok" : "af-out bad";
          lout.textContent = r.ok ? `${act}ed \u2014 reload to see it` : t;
        } catch (err) {
          lout.className = "af-out bad";
          lout.textContent = "Could not reach the platform: " + err.message;
        }
        btn.disabled = false;
      });
    });

    refresh();
    return { changed };
  }

  // `renderMarkdown` is exported for the render check in
  // `scripts/check_agent_fields.js`. A preview that escapes nothing is an
  // injection on an owner's configuration surface, and that property has to be
  // assertable without a browser.
  return {
    mount,
    FIELDS,
    renderMarkdown: md,
    groups: () => [...new Set(FIELDS.map((f) => f.group))],
  };
})();
