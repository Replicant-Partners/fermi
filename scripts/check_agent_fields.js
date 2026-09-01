// The agent field editor, over the things that fail silently.
//
// This widget writes to a live agent, so the properties worth guarding are not
// cosmetic:
//
//   1. **Every key it sends is a key the endpoint accepts.** Asserted against
//      the real `AgentUpdate` struct read off disk, not against a list in this
//      file — a list here would be a second copy of the schema and would drift.
//   2. **status and visibility are never fields.** `PUT /api/agents/:agent_id`
//      REFUSES them: "lifecycle transitions run through the publish pipeline so
//      publish checks, fees and audit logging are applied." A control the
//      endpoint refuses is worse than no control, because the refusal arrives
//      after the click.
//   3. **Save sends a diff.** A PUT echoing unchanged values makes the
//      `agent_card.updated` event claim the author changed twenty things when
//      they changed one, which is what `collect_changed_fields` exists to avoid.
//   4. **Empty means null, not "".** An empty string is a value the agent then
//      carries; absent is what the author meant.
//
// Usage: node scripts/check_agent_fields.js

const fs = require("fs");
const path = require("path");

const ROOT = path.join(__dirname, "..");
globalThis.window = {};

// ── a DOM, only as much as the widget touches ────────────────────────────
let LAST_FETCH = null;
function makeEl(tag) {
  const el = {
    tag, html: "", className: "", value: "", disabled: false, rows: [],
    dataset: {}, _kids: [], _on: {},
    set innerHTML(v) { this.html = String(v); this._kids = parse(String(v), this); },
    get innerHTML() { return this.html; },
    set textContent(v) { this._text = String(v); },
    get textContent() { return this._text || ""; },
    addEventListener(k, fn) { (this._on[k] = this._on[k] || []).push(fn); },
    removeEventListener() {},
    fire(k) { (this._on[k] || []).forEach((f) => f({})); },
    querySelector(sel) { return this.querySelectorAll(sel)[0] || null; },
    querySelectorAll(sel) {
      const m = /^\[([a-z-]+)(?:="([^"]*)")?\]$/.exec(sel.trim());
      if (!m) return [];
      const attr = m[1].replace(/^data-/, "").replace(/-([a-z])/g, (_, c) => c.toUpperCase());
      return this._kids.filter((k) =>
        attr in k.dataset && (m[2] === undefined || k.dataset[attr] === m[2]));
    },
  };
  return el;
}
// Pull the widget's own controls back out of the markup it wrote, so the test
// drives the real elements rather than a hand-built stand-in.
// A textarea's content, by field name. Extracted in its own pass rather than as
// an optional group on the tag regex: a lazy `[\s\S]*?</textarea>` on an <input>
// match swallows every element up to the first textarea, which silently ate the
// controls this harness is meant to drive.
function textareaContent(html, field) {
  const m = new RegExp(
    '<textarea[^>]*data-field="' + field + '"[^>]*>([\\s\\S]*?)</textarea>').exec(html);
  // Unescaped, because that is what a browser hands back from `.value`.
  return String((m && m[1]) || "")
    .replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"').replace(/&#39;/g, "'");
}

function parse(html, parent) {
  const kids = [];
  // A textarea holds its content BETWEEN the tags, not in a `value` attribute.
  // Reading only the attribute made every prompt field read as emptied at mount,
  // so the harness reported a spurious diff — the third time in this repo that
  // the fixture was the bug rather than the subject.
  const re = /<(input|textarea|button|span|div)\b([^>]*)>/g;
  let m;
  while ((m = re.exec(html))) {
    const attrs = m[2];
    const el = makeEl(m[1]);
    const get = (a) => (new RegExp(a + '="([^"]*)"').exec(attrs) || [])[1];
    const f = get("data-field"), kind = get("data-kind");
    if (f) {
      el.dataset.field = f;
      el.dataset.kind = kind || "text";
      el.value = m[1] === "textarea"
        ? textareaContent(html, f)
        : get("value") || "";
    }
    if (/data-af-save/.test(attrs)) el.dataset.afSave = "";
    if (/data-af-out/.test(attrs)) el.dataset.afOut = "";
    if (/data-life-out/.test(attrs)) el.dataset.lifeOut = "";
    const life = get("data-lifecycle");
    if (life) el.dataset.lifecycle = life;
    if (el.dataset.field || "afSave" in el.dataset || "afOut" in el.dataset
        || "lifeOut" in el.dataset || el.dataset.lifecycle) kids.push(el);
  }
  return kids;
}
globalThis.document = { getElementById: () => null, createElement: makeEl };
globalThis.fetch = (url, opts) => {
  LAST_FETCH = { url, opts, body: opts && opts.body ? JSON.parse(opts.body) : null };
  return Promise.resolve({ ok: true, status: 200, text: () => Promise.resolve("{}") });
};

new Function(fs.readFileSync(
  path.join(ROOT, "static", "js", "widgets", "agent-fields.js"), "utf8"))();
const AF = globalThis.window.AgentFields;

const FAIL = [];
const ok = (c, what) => { if (!c) FAIL.push(what); };
ok(!!AF, "the widget did not define window.AgentFields");

// ── 1. every key exists on the real AgentUpdate ──────────────────────────
const types = fs.readFileSync(
  path.join(ROOT, "agent-bestiary", "memory", "src", "types.rs"), "utf8");
const upd = types.slice(types.indexOf("pub struct AgentUpdate"));
const accepted = new Set(
  [...upd.slice(0, upd.indexOf("\n}")).matchAll(/pub ([a-z_]+):/g)].map((m) => m[1]));
ok(accepted.size > 10, `read only ${accepted.size} AgentUpdate fields; the parse is wrong`);
for (const f of AF.FIELDS) {
  ok(accepted.has(f.key),
    `\`${f.key}\` is offered as a field and \`AgentUpdate\` has no such member, so ` +
    `the PUT would ignore it silently`);
}

// ── 2. the two the endpoint refuses are never fields ─────────────────────
const agents = fs.readFileSync(path.join(ROOT, "src", "handlers", "agents.rs"), "utf8");
ok(/fn reject_lifecycle_fields/.test(agents),
  "reject_lifecycle_fields is gone — re-check whether the PUT still refuses " +
  "status and visibility before offering them");
for (const refused of ["status", "visibility"]) {
  ok(!AF.FIELDS.some((f) => f.key === refused),
    `\`${refused}\` is offered as an editable field and the PUT refuses it. The ` +
    `refusal would arrive after the click, which is worse than offering nothing`);
}

// ── 3. the groups, and every field explains itself ───────────────────────
ok(AF.groups().includes("intelligence") && AF.groups().includes("manage"),
  `groups are ${AF.groups().join(", ")}`);
for (const f of AF.FIELDS) {
  ok(f.help && f.help.length > 30,
    `\`${f.key}\` has no real help. A number anyone can set and nobody can price ` +
    `is how temperature gets raised on an agent under a field contract`);
  ok(f.label && !/[A-Z]/.test(f.label[0]), `\`${f.key}\`'s label should be lower case`);
}

// Wrapped, because `require` and top-level `await` cannot coexist in one file
// and this harness needs both: the schema is read off disk synchronously and
// the save path is a promise.
(async () => {
  // ── 4. mount, then drive the real controls ───────────────────────────────
  const host = makeEl("div");
  const PROFILE = {
    agent_name: "football_analyst", label: "Football Analyst", status: "active",
    visibility: "public", min_tier: "free", tags: ["football", "research"],
    system_prompt: "You analyse football.",
    substrate: { provider: "anthropic", model: "claude-opus-4", temperature: 0.7 },
  };
  const api = AF.mount({ container: host, agentId: "football_analyst",
                         group: "intelligence", profile: PROFILE });
  ok(/af-row/.test(host.html), "the intelligence group rendered nothing");
  ok(host.html.includes("claude-opus-4"), "the served model was not loaded into the control");
  ok(host.html.includes("You analyse football."), "the system prompt was not loaded");

  const save = host.querySelector("[data-af-save]");
  ok(!!save, "there is no save control");
  ok(save.disabled === true, "save is live before anything changed");
  ok(api.changed().length === 0, `${api.changed().length} fields read as changed at mount`);

  // Change one, and only one must travel.
  const model = host.querySelectorAll("[data-field]").find((i) => i.dataset.field === "model");
  model.value = "claude-sonnet-4";
  model.fire("input");
  ok(save.disabled === false, "save stayed disabled after a change");
  ok(api.changed().join(",") === "model", `changed reads ${api.changed().join(",")}`);

  LAST_FETCH = null;
  save.fire("click");
  await new Promise((r) => setTimeout(r, 0));
  ok(LAST_FETCH !== null, "save sent no request");
  ok(LAST_FETCH.opts.method === "PUT", `save used ${LAST_FETCH.opts.method}`);
  ok(LAST_FETCH.url === "/api/agents/football_analyst", `posted to ${LAST_FETCH.url}`);
  ok(Object.keys(LAST_FETCH.body).join(",") === "model",
    `the PUT carried ${Object.keys(LAST_FETCH.body).join(",")} — save must send a diff, ` +
    `or agent_card.updated claims the author changed things they did not`);
  ok(LAST_FETCH.body.model === "claude-sonnet-4", "the new value did not travel");

  // ── 5. empty is null, and numbers are numbers ────────────────────────────
  const host2 = makeEl("div");
  AF.mount({ container: host2, agentId: "x", group: "intelligence", profile: PROFILE });
  const inputs2 = host2.querySelectorAll("[data-field]");
  const temp = inputs2.find((i) => i.dataset.field === "temperature");
  temp.value = "0.2"; temp.fire("input");
  const prompt = inputs2.find((i) => i.dataset.field === "system_prompt");
  prompt.value = "   "; prompt.fire("input");
  LAST_FETCH = null;
  host2.querySelector("[data-af-save]").fire("click");
  await new Promise((r) => setTimeout(r, 0));
  ok(LAST_FETCH.body.temperature === 0.2,
    `temperature travelled as ${JSON.stringify(LAST_FETCH.body.temperature)}, not a number`);
  ok(LAST_FETCH.body.system_prompt === null,
    `an emptied field travelled as ${JSON.stringify(LAST_FETCH.body.system_prompt)}; ` +
    `an empty string is a value the agent would carry`);

  // ── 6. manage carries lifecycle as actions, not fields ───────────────────
  const host3 = makeEl("div");
  AF.mount({ container: host3, agentId: "football_analyst", group: "manage",
             profile: PROFILE });
  ok(/af-life/.test(host3.html), "the manage group has no lifecycle block");
  ok(/publish pipeline/.test(host3.html),
    "the lifecycle block does not say why these are not fields");
  const life = host3.querySelectorAll("[data-lifecycle]");
  ok(life.length > 0, "no lifecycle action is offered");
  // An active agent must not be offered `publish`; that is offering nothing.
  ok(!life.some((b) => b.dataset.lifecycle === "publish"),
    "an already-active agent is offered publish");
  ok(life.some((b) => b.dataset.lifecycle === "archive"), "an active agent cannot be archived");
  LAST_FETCH = null;
  life.find((b) => b.dataset.lifecycle === "archive").fire("click");
  await new Promise((r) => setTimeout(r, 0));
  ok(LAST_FETCH.url === "/api/agents/football_analyst/archive",
    `archive posted to ${LAST_FETCH.url} — lifecycle must not go through the PUT`);

  // An archived agent gets restore instead.
  const host4 = makeEl("div");
  AF.mount({ container: host4, agentId: "x", group: "manage",
             profile: { ...PROFILE, status: "archived" } });
  const life4 = host4.querySelectorAll("[data-lifecycle]");
  ok(life4.some((b) => b.dataset.lifecycle === "restore"),
    "an archived agent is not offered restore");

  // ── 7. creation mode collects instead of saving ──────────────────────────
  const host5 = makeEl("div");
  let collected = null;
  AF.mount({ container: host5, agentId: null, group: "intelligence", profile: {},
             onCollect: (v) => { collected = v; } });
  const m5 = host5.querySelectorAll("[data-field]").find((i) => i.dataset.field === "model");
  m5.value = "gpt-4o"; m5.fire("input");
  LAST_FETCH = null;
  host5.querySelector("[data-af-save]").fire("click");
  await new Promise((r) => setTimeout(r, 0));
  ok(LAST_FETCH === null, "creation mode sent a PUT for an agent that does not exist yet");
  ok(collected && collected.model === "gpt-4o",
    `creation mode collected ${JSON.stringify(collected)} — this is what makes a ` +
    `creation flow this widget in another mode rather than a fourth copy of the fields`);

  if (FAIL.length) {
    console.error(`\n${FAIL.length} failure(s):`);
    FAIL.forEach((f) => console.error("  ✗ " + f));
    process.exit(1);
  }
  console.log("agent fields: all checks pass");
})();
