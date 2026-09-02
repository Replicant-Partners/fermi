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

  // `path` is how a value is read out of the served profile, which is nested;
  // `key` is what the endpoint takes, which is flat.
  const FIELDS = [
    // ── intelligence ──────────────────────────────────────────────────
    { group: "intelligence", key: "llm_provider", path: "substrate.provider",
      label: "fallback provider", kind: "text",
      help: "Used only when no ladder rung matches the caller's tier. Changing it " +
            "without changing the model is how an agent ends up asking Anthropic " +
            "for a GPT model." },
    { group: "intelligence", key: "model", path: "substrate.model",
      label: "fallback model", kind: "text",
      help: "The default, NOT necessarily what runs. `apply_tier_resolution` picks " +
            "the highest ladder rung at or below the caller's tier and overwrites " +
            "both fields; this stands only when no rung matches." },
    { group: "intelligence", key: "temperature", path: "substrate.temperature",
      label: "temperature", kind: "number", step: "0.05", min: "0", max: "2",
      help: "Higher is more varied. An agent under a field contract is being " +
            "asked for retrievable facts, and variance there is noise rather " +
            "than creativity." },
    { group: "intelligence", key: "system_prompt", path: "system_prompt",
      label: "system prompt", kind: "textarea",
      help: "The agent's standing instruction. It is versioned — the persona " +
            "version on every pulse says which text produced it, so a trace " +
            "read next month still resolves to the prompt that ran." },
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

  function control(f, value) {
    const v = value == null ? "" : value;
    const common = `data-field="${f.key}" data-kind="${f.kind}"`;
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
      return `<textarea ${common} rows="7">${esc(v)}</textarea>`;
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
    const raw = el.value;
    switch (el.dataset.kind) {
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
    const objectOf = (key) => fields.find((f) => f.kind === "object" && f.key === key);
    const assemble = (f, inputs) => {
      const out = {};
      let any = false;
      // The pad's two axes are members too — they just have one control between
      // them. Listed first so a valence written by hand keeps its key order.
      const keys = (f.pad ? [f.pad.x.key, f.pad.y.key] : [])
        .concat(f.members.map((m) => m.key));
      keys.forEach((k) => {
        const el = inputs.find((i) => i.dataset.field === `${f.key}.${k}`);
        if (!el) return;
        const v = read(el);
        if (v !== null && !(Array.isArray(v) && v.length === 0)) any = true;
        out[k] = v;
      });
      // Every member emptied means the author cleared it, which is `null` and
      // not an object of nulls.
      return any ? out : null;
    };

    el.innerHTML = `
      ${fields.map((f) => `<div class="af-row">
        <label class="af-label" for="af-${esc(f.key)}">${esc(f.label)}</label>
        ${control(f, initial[f.key])}
        <div class="af-help">${esc(f.help)}</div>
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

  return { mount, FIELDS, groups: () => [...new Set(FIELDS.map((f) => f.group))] };
})();
