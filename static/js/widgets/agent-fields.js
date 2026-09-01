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
      label: "provider", kind: "text",
      help: "The vendor. Changing it without changing the model is how an agent " +
            "ends up asking Anthropic for a GPT model." },
    { group: "intelligence", key: "model", path: "substrate.model",
      label: "model", kind: "text",
      help: "The exact model id the provider expects." },
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
  ];

  const at = (obj, path) =>
    String(path).split(".").reduce((o, k) => (o == null ? o : o[k]), obj);

  function control(f, value) {
    const v = value == null ? "" : value;
    const common = `data-field="${f.key}" data-kind="${f.kind}"`;
    if (f.kind === "textarea") {
      return `<textarea ${common} rows="7">${esc(v)}</textarea>`;
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

  const same = (a, b) =>
    Array.isArray(a) || Array.isArray(b)
      ? JSON.stringify(a || []) === JSON.stringify(b || [])
      : (a ?? null) === (b ?? null);

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

    el.innerHTML = `
      ${fields.map((f) => `<div class="af-row">
        <label class="af-label" for="af-${esc(f.key)}">${esc(f.label)}</label>
        ${control(f, initial[f.key])}
        <div class="af-help">${esc(f.help)}</div>
      </div>`).join("")}
      ${group === "manage" ? lifecycle(profile) : ""}
      <div class="af-bar">
        <button class="af-save" data-af-save disabled>save</button>
        <span class="af-out" data-af-out></span>
      </div>`;

    const inputs = [...el.querySelectorAll("[data-field]")];
    const saveBtn = el.querySelector("[data-af-save]");
    const out = el.querySelector("[data-af-out]");

    const changed = () =>
      inputs.filter((i) => !same(read(i), initial[i.dataset.field]));

    const refresh = () => {
      const n = changed().length;
      saveBtn.disabled = n === 0;
      saveBtn.textContent = n === 0 ? "save" : `save ${n} change${n === 1 ? "" : "s"}`;
    };
    inputs.forEach((i) => i.addEventListener("input", refresh));

    saveBtn.addEventListener("click", async () => {
      const diff = {};
      changed().forEach((i) => { diff[i.dataset.field] = read(i); });
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
    return { changed: () => changed().map((i) => i.dataset.field) };
  }

  return { mount, FIELDS, groups: () => [...new Set(FIELDS.map((f) => f.group))] };
})();
