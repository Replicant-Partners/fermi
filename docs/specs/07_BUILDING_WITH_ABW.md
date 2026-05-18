# Building on ABW — A Practical Guide

**Audience:** internal. You're building an App on ABW with AI coding tools at your side. This doc tells you how to do it without hitting walls blindly.

**Where ABW is right now:** early MVP. The substrate works. The APIs are real. The agents think. Sharp edges exist — this doc says where they are and how to work around them.

---

## Registering an App is the easy bit — three one-liners

You won't spend serious time here. The platform absorbed the recipe:

- **In conversation:** `@xaman_ek help me build an App for <idea>` — answer the questions, click **Create App**.
- **In code:** `abw app new <slug> && $EDITOR <slug>/manifest.json && abw app deploy`.
- **From any working workspace:** click **Save as App** in the header.

All three produce the same artifact. Pick whichever fits the moment. Details in [`03_CREATING_APPS.md`](./03_CREATING_APPS.md); API surface in [`01_APP_PRIMITIVE.md`](./01_APP_PRIMITIVE.md).

**The rest of this doc is about the work *after* registration** — wiring a UI to your App's action grammar, iterating the agent's prompt, handling errors. That's the part that takes real time.

---

## The mental model in 60 seconds

ABW is a platform where agents live and workspaces are where work happens.

```
You have domain expertise.
You define an agent that embodies that expertise.
You define an App that packages it.
Users spawn workspaces from your App.
Your agent talks to users in those workspaces.
The agent emits structured actions.
Your UI (or the CLI, or an MCP client) executes those actions.
```

That's it. Everything else is plumbing.

---

## Important: you never touch the ABW codebase

This is the single biggest source of confusion for new App developers.

**You do not need git access to the ABW platform repo.** You are not
editing files inside the ABW server. You are a tenant, not a maintainer.

Your workflow is entirely through:
- **The ABW web UI** — browse agents, manage your agent's prompt, see workspaces
- **The `abw` CLI** — scaffold, register, and interact with workspaces from your terminal
- **The ABW API** — what your UI calls at runtime
- **Your own project folder** — which you manage however you like (git, Dropbox, whatever)

The `abw` CLI talks to the ABW API on your behalf. When you run
`abw app deploy`, it sends your manifest and agent card to the server
via an API call. No git involved. No ABW repo access needed.

The only person who pushes to the ABW repo is the platform engineer
(Ivan). That's a separate job.

---

## Step 1 — Start here: call the schema endpoint

Once your App is deployed, this is the first thing to call:

```bash
curl https://agent-bestiary.world/api/apps/my_app/schema
```

What comes back is the complete action grammar — every action your agent
can emit, every field it expects, and how to parse the action blocks from
the agent's responses. This is what your UI needs to implement.

There are two live examples you can call right now to see what the response
looks like before you deploy your own:

```bash
# SimOps — a process-modelling App with a rich 6-action grammar
curl https://agent-bestiary.world/api/apps/kask_simops/schema

# efrain — a simpler App (good starting template for something new)
curl https://agent-bestiary.world/api/apps/efrain_ai/schema
```

**Give the output to your AI coding tool.** Paste the JSON into Cursor
or Claude and say: "Generate a JavaScript dispatcher that handles all the
action types in this schema." It will write the switch statement for you
in one pass.

---

## Step 2 — The simplest possible UI

Here's a complete working UI in under 100 lines of HTML. No framework,
no build step, no dependencies. Paste it into a file, open it in a
browser.

```html
<!DOCTYPE html>
<html>
<head>
  <title>My App</title>
  <style>
    body { font-family: monospace; max-width: 800px; margin: 40px auto; padding: 20px; background: #1a1a1a; color: #e0e0e0; }
    #messages { height: 400px; overflow-y: auto; border: 1px solid #333; padding: 12px; margin-bottom: 12px; }
    .agent { color: #7ec8e3; margin: 8px 0; }
    .user  { color: #b5e7a0; margin: 8px 0; }
    .action { color: #f5c518; font-size: 0.85em; margin: 4px 0 4px 16px; }
    input  { width: 80%; padding: 8px; background: #2a2a2a; border: 1px solid #444; color: #e0e0e0; }
    button { padding: 8px 16px; background: #f5c518; color: #000; border: none; cursor: pointer; }
  </style>
</head>
<body>
  <h2>My App</h2>
  <div>
    <label>Workspace ID: <input id="ws-id" placeholder="paste workspace UUID or URL" /></label>
    <label>Token: <input id="token" type="password" placeholder="ferm_..." /></label>
  </div>
  <br>
  <div id="messages"></div>
  <input id="input" placeholder="Type a message..." onkeydown="if(event.key==='Enter') send()" />
  <button onclick="send()">Send</button>

  <script>
    const BASE = 'https://agent-bestiary.world';
    const AGENT = 'my_companion';   // ← your agent's slug
    const APP   = 'my_app';         // ← your App's slug
    const ACTION_RE = /__ACTION__\n([\s\S]*?)\n__END_ACTION__/g;

    function wsId() {
      const v = document.getElementById('ws-id').value.trim();
      const m = v.match(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i);
      return m ? m[0] : v;   // accept full URLs or bare UUIDs
    }
    function token() { return document.getElementById('token').value.trim(); }

    function addMessage(role, text) {
      const el = document.createElement('div');
      el.className = role;
      el.textContent = (role === 'user' ? 'You: ' : 'Agent: ') + text;
      document.getElementById('messages').appendChild(el);
      document.getElementById('messages').scrollTop = 9999;
    }

    function addAction(action) {
      const el = document.createElement('div');
      el.className = 'action';
      el.textContent = `⚡ ${action.type}: ${JSON.stringify(action).slice(0, 80)}...`;
      document.getElementById('messages').appendChild(el);
    }

    async function send() {
      const text = document.getElementById('input').value.trim();
      if (!text) return;
      document.getElementById('input').value = '';
      addMessage('user', text);

      await fetch(`${BASE}/api/workspaces/${wsId()}/messages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token()}` },
        body: JSON.stringify({ content: `@${AGENT} ${text}`, agent: AGENT })
      });

      // Poll for the agent's reply (use SSE stream for real-time)
      setTimeout(async () => {
        const msgs = await fetch(`${BASE}/api/workspaces/${wsId()}/messages?limit=5`, {
          headers: { 'Authorization': `Bearer ${token()}` }
        }).then(r => r.json());

        const last = (msgs.messages || []).filter(m => m.sender_type === 'agent').pop();
        if (!last) return;

        const content = last.content || '';
        const actions = [...content.matchAll(ACTION_RE)].map(m => {
          try { return JSON.parse(m[1]); } catch { return null; }
        }).filter(Boolean);
        const prose = content.replace(ACTION_RE, '').trim();

        if (prose) addMessage('agent', prose);
        actions.forEach(addAction);
        for (const action of actions) await dispatchAction(action);
      }, 3000);
    }

    async function dispatchAction(action) {
      // Your agent's prompt may use domain-specific action names.
      // Get the full alias map from: GET /api/apps/my_app/schema → type_name_map
      const TYPE_MAP = {
        // e.g. 'save_note': 'mutate_document',
        // e.g. 'tag_entry': 'annotate',
      };
      const type = TYPE_MAP[action.type] || action.type;
      const result = await fetch(`${BASE}/api/workspaces/${wsId()}/actions/${type}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token()}` },
        body: JSON.stringify({ ...action, app_schema: APP })
      }).then(r => r.json());
      console.log('action dispatched:', type, result);
    }
  </script>
</body>
</html>
```

Change the two constants at the top (`AGENT` and `APP`) to your values.
That's your entire UI to start.

---

## Step 3 — The development loop

**Everything happens in the browser. No git, no deploys, no waiting.**

```
1. Spawn a workspace from your App
   → https://agent-bestiary.world/apps/my_app → "+ New Workspace"

2. Talk to your agent in that workspace
   → Type a message, see what comes back

3. Notice something wrong (wrong tone, missing action, bad output)

4. Edit the prompt — go to:
   → https://agent-bestiary.world/agent/my_companion → Manage tab
   → Edit the system prompt
   → Bump the version number (1.0 → 1.1)
   → Save

5. The change is live immediately — no deploy, no waiting

6. Go back to the workspace and try again
```

This loop takes 2 minutes per iteration.

**What the Manage tab lets you change:**
- System prompt — the agent's complete instructions
- Model — which LLM runs (Sonnet, Haiku, etc.)
- Temperature — how creative/deterministic the output is
- Model ladder — which model runs for free-tier vs paid users
- Version number — bump this every time you change the prompt

**Checking that your change is live:**

Go to `https://agent-bestiary.world/agent/my_companion` → Overview tab.
The version number shown should match what you just saved. If it does,
the change is live.

**The action log is your feedback signal:**
```bash
abw workspace actions list <ws-id>
```
After each test turn, check what actions the agent actually emitted.
Empty log = the agent isn't producing action blocks, the prompt needs
work. Wrong action type or shape = prompt needs refinement. The log
tells you what the agent is actually doing.

---

## Step 4 — The errors you will see and what they mean

### `401 Missing authorization token`

Your API token isn't being sent. Check:
- The token starts with `ferm_`
- You're passing it as `Authorization: Bearer ferm_...`
- If using the CLI: `export ABW_API_TOKEN=ferm_...`

Get a token: `https://agent-bestiary.world/settings/api-keys`

### `403 Not a workspace member`

You're trying to access a workspace you haven't been added to. Either
spawn a new one from your App, or ask the workspace owner to add you.

### `402 Payment Required` (or `insufficient credits`)

The workspace has run out of gas (credits). Each agent turn costs
roughly 6-15 credits. A 200-credit workspace lasts 15-25 turns.

Fix — ask the platform admin to top up via the admin panel:
`https://agent-bestiary.world/admin` → "⚡ Grant Credits to Workspace"

Or they can run:
```bash
abw workspace actions annotate <ws-id> ...  # not the fix
# The fix is an admin action — ping Ivan
```

This happens constantly during development. Don't buy credits yet —
ask for a grant.

### `500 Internal Server Error` on `/api/workspaces/:id/messages`

Most common causes:

1. **Agent name is wrong.** The `agent` field in your POST must match
   exactly the `agent_id` you registered with `abw app deploy`.
   Check: `https://agent-bestiary.world/agent/my_companion` — if that
   page 404s, the agent isn't registered.

2. **Agent not hired into the workspace.** Your App's `auto_hire` in
   `manifest.json` must include your agent. If you forgot it, re-deploy
   (`abw app deploy`) and spawn a new workspace.

3. **Agent card has a JSON syntax error.** The agent card you deployed
   has invalid JSON. Validate it before deploying:
   ```bash
   cat my_app/agent_card.json | python3 -m json.tool
   ```
   Fix the error, then `abw app deploy` again.

### Agent responds with old behaviour

The prompt you edited in the Manage tab hasn't saved yet, or you're
looking at the wrong agent. Check:

Go to `https://agent-bestiary.world/agent/my_companion` → Overview.
The version shown should match what you typed in the Manage tab. If it
doesn't, open the Manage tab, make a trivial change (add a space,
remove it), and save again.

### Action blocks don't appear / `JSON.parse` fails

The model produced malformed JSON inside an action block. This happens
with weaker models (Haiku, OpenRouter free tier). The action grammar is
complex JSON — it needs Sonnet.

Fix: go to the Manage tab, change the model to `claude-sonnet-4-6`, save.

If you see partial blocks (only `__ACTION__` without `__END_ACTION__`),
the model hit its output limit. Go to Manage tab → bump `max_tokens`
to 4096 in the model params.

### CORS errors in the browser

The ABW API allows `https://agent-bestiary.world` and localhost origins.
If you're developing on a different domain:
- Use localhost during development (any port works)
- Ask the platform admin to add your domain to the allowlist

---

## Step 5 — What to give your AI coding assistant

When you want your coding agent to build a UI or dispatcher for you,
give it exactly these three things:

**1. Your schema:**
```bash
curl https://agent-bestiary.world/api/apps/my_app/schema
```
Paste the output. This tells the AI every action type, every field,
and how to parse the blocks.

**2. The workspace API surface:**
```
POST /api/workspaces/:id/messages        — send a message / invoke agent
GET  /api/workspaces/:id/messages        — list recent messages
GET  /api/workspaces/:id/messages/stream — SSE stream (real-time)
GET  /api/workspaces/:id/files/:path     — read a workspace file
POST /api/workspaces/:id/files           — write a workspace file
POST /api/workspaces/:id/actions/:type   — dispatch a typed action
GET  /api/workspaces/:id/actions         — list action log
GET  /api/workspaces/:id/annotations     — list annotations
All requests need: Authorization: Bearer <your_token>
```

**3. The action parsing snippet:**
```javascript
const ACTION_RE = /__ACTION__\n([\s\S]*?)\n__END_ACTION__/g;
const actions = [...content.matchAll(ACTION_RE)].map(m => JSON.parse(m[1]));
const prose = content.replace(ACTION_RE, '').trim();
```

With these three, any capable AI coding assistant can generate a working
dispatcher and UI in one pass.

---

## Step 6 — What's still rough (known limitations as of May 2026)

These are real gaps. Work around them; don't fight them.

**`mutate_document` with `confirmation: "ask"` pends for human review.**
During alpha, every `mutate_document` is treated as `pending` regardless
of what the agent sends. Your UI needs to call
`POST /api/workspaces/:id/actions/:action-id/accept` after the user
reviews the proposed change. The action block is intent; the accept call
is execution.

**SSE stream delivers complete messages, not token-by-token.**
`GET /api/workspaces/:id/messages/stream` emits one event when the agent
finishes. It does not stream partial tokens.

**The `compare` action records intent only.** The client dispatches
cascade runs and calls `/accept` with the results. The server doesn't
run cascades automatically.

**Credits deplete fast during development.** Each turn: 6-15 credits.
Ask for grants, don't buy yet.

**xaman_ek's MCP tools require a Bearer token.** Workspace-scoped tools
(`read_workspace_file`, `list_workspace_agents`) return `401` without one.
Pass `Authorization: Bearer ferm_...` in your MCP client config.

**Agent version history is not yet in the UI.** When you edit the prompt
in the Manage tab, the old version is saved internally but there's no
browser view to see or restore past versions yet. Coming soon.

---

## Worked example — efrain_ai

`efrain_ai` is a real App built by Mario, an external developer. He built
it without any access to the ABW codebase. It's a good reference because
it's simpler than SimOps.

**What it is:** a research notes App. The companion helps the user capture
and cross-reference notes. The canonical document is `efrain/notes.yaml`.

**How Mario built it:**
```bash
abw app new efrain_ai
# edited manifest.json and agent_card.json in that folder
abw app deploy
```

Then he iterated the prompt in the Manage tab until it behaved correctly.
No git access to ABW. No deploys. Just the CLI and the browser.

**What its schema looks like:**
```bash
curl https://agent-bestiary.world/api/apps/efrain_ai/schema
```

The efrain action grammar is simpler than SimOps — mostly `mutate_document`
(update a note) and `annotate` (tag an observation). That's a good starting
template: two action types is enough to have a working App.

**The lesson from efrain:** start with two action types and one agent. Get
them working end-to-end. The schema endpoint grows as you add more.

---

## Reference

**App lifecycle:**
```bash
abw login
abw app new my_app           # scaffold manifest + agent card locally
abw app validate             # check for errors before deploying
abw app deploy               # register/update the App on ABW
abw app spawn my_app         # spawn a new workspace
```

**Workspace interaction:**
```bash
abw workspace message <ws-id> "hello" --agent my_companion
abw workspace files get <ws-id> my_app/state.yaml
abw workspace files put <ws-id> my_app/state.yaml --content @updated.yaml
abw workspace actions list <ws-id>
abw workspace actions pending <ws-id>
abw workspace actions accept <ws-id> <action-id>
abw workspace actions annotate <ws-id> "note" --kind insight
```

**Key URLs:**
```
https://agent-bestiary.world/apps               — browse all Apps
https://agent-bestiary.world/apps/my_app        — your App's page
https://agent-bestiary.world/agent/my_companion — your agent (Manage tab to edit prompt)
https://agent-bestiary.world/admin              — admin panel (grant credits)
https://agent-bestiary.world/settings/api-keys  — mint API tokens
https://agent-bestiary.world/api/apps/my_app/schema — your action grammar
```

**MCP endpoint — for AI coding tools like Cursor:**
```
https://agent-bestiary.world/mcp/agents/xaman_ek
```
Configure this in Cursor/Claude Desktop with your Bearer token.
xaman_ek can then discover agents, read workspace files, and execute
agents on your behalf — all from inside your coding environment.
