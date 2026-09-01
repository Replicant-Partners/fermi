// One pulse row, for every surface that lists pulses.
//
// # Why this is a widget and not a function in one page
//
// The stream and a specimen's Record tab list the same object. The stream showed
// the hop — who addressed whom, with a glyph for each — whether grounding graded
// the contents, and whether any checkpoint recorded a decision. The Record tab
// showed a date, a truncated query and a cost, in a plain table, with none of
// it. Same rows, two renderings, and the stripped one was on the page an agent's
// owner actually opens.
//
// The rule this exists to hold is the one already written down about the
// bestiary and the trace: **one object, one rendering.** A second copy of a
// renderer is a second answer to the same question, and the two had already
// drifted a long way apart.
//
// # The iconography is load-bearing
//
// Four kinds of addresser, four marks. A person and an agent looked identical
// before the glyphs existed, which made the stream unreadable: you could not see
// at a glance whether the platform was working or a human was doing all the
// asking. `unattributed` is a **gap in the record** rather than an actor — 514
// of 3,651 pulses — so it is drawn dashed and in the colour reserved for
// indeterminate, never as a benign default.
//
// # Grounding has three states, never two
//
// `clean`, `violations`, and `ungraded` — no contract applied, or no path
// enforced one. `ungraded` is **not a pass**, so it must not be rendered as one
// or as a failure either.
//
// Usage:
//   <link rel="stylesheet" href="/static/css/pulse.css">   (or inline the CSS)
//   <script src="/static/js/widgets/pulse.js"></script>
//   el.innerHTML = Pulse.rows(list);
//   Pulse.wire(el);   // one delegated click, survives re-render
window.Pulse = (function () {
  const esc = (s) =>
    String(s ?? "").replace(/[&<>"']/g, (c) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

  const GLYPH = { human: "◍", agent: "▣", system: "⚙", unattributed: "?" };

  function entity(e) {
    e = e || {};
    const k = GLYPH[e.kind] ? e.kind : "unattributed";
    return (
      `<span class="ent"><span class="glyph g-${k}" title="${esc(k)}">${GLYPH[k]}</span>` +
      `<span class="ent-n">${esc(e.name || (k === "unattributed" ? "not recorded" : k))}</span></span>`
    );
  }

  // Relative for anything inside a day, absolute after. A reader scanning for
  // "did this just happen" needs the first; a reader reconciling against a log
  // needs the second.
  function when(iso) {
    if (!iso) return "";
    const d = new Date(iso), mins = (Date.now() - d.getTime()) / 60000;
    if (mins < 60) return `${Math.max(0, Math.round(mins))}m ago`;
    if (mins < 1440) return `${Math.round(mins / 60)}h ago`;
    return d.toISOString().slice(0, 16).replace("T", " ");
  }

  function marks(x) {
    const g = x.grounding;
    const grounded =
      g === "clean" ? `<span class="m m-clean">grounded</span>`
      : g === "violations" ? `<span class="m m-viol">violations</span>`
      : `<span class="m m-ungraded" title="No contract applied, or no path enforced one. Not a pass.">ungraded</span>`;
    const rec = x.recorded
      ? `<span class="m m-rec" title="at least one checkpoint recorded a decision about this pulse">recorded</span>`
      : "";
    const failed =
      x.status && x.status !== "success"
        ? `<span class="m m-viol">${esc(x.status)}</span>`
        : "";
    return grounded + rec + failed;
  }

  function row(x) {
    const href = x.episode_id ? `/trace/${encodeURIComponent(x.episode_id)}` : "";
    return `<div class="ex${href ? "" : " ex-dead"}"${href ? ` data-href="${href}"` : ""}>
      <div class="when">${esc(when(x.at))}</div>
      <div class="body">
        <div class="hop">${entity(x.from)}<span class="arrow">→</span>${entity(x.to)}</div>
        <div class="q">${esc(x.query || "—")}</div>
        ${x.error ? `<div class="q bad">${esc(String(x.error).slice(0, 120))}</div>` : ""}
      </div>
      <div class="marks">${marks(x)}</div>
      <div class="cost">${x.cost_usd != null ? "$" + Number(x.cost_usd).toFixed(4) : "—"}</div>
    </div>`;
  }

  // One delegated listener on the container. `render` replaces its contents, and
  // handlers wired per element at render time are how a second render leaves
  // half a list dead.
  function wire(root) {
    if (!root || root.dataset.pulseWired === "1") return;
    root.dataset.pulseWired = "1";
    root.addEventListener("click", (e) => {
      const r = e.target.closest("[data-href]");
      if (r) location.href = r.dataset.href;
    });
  }

  return {
    rows: (list) => (list || []).map(row).join(""),
    row,
    entity,
    when,
    marks,
    wire,
    GLYPH,
  };
})();
