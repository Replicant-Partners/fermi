# Building on ABW — A Practical Guide for Vibe Coders

**Audience:** You have domain expertise and use AI coding tools (Cursor,
Claude, Copilot) to build things. You're comfortable iterating fast and
debugging with AI help. You may not have a traditional software engineering
background. This guide helps you build an App on the Agent Bestiary
Workspace (ABW) substrate without hitting the walls blindly.

**Where ABW is right now:** early MVP. The substrate works. The APIs are
real. The agents think. But sharp edges exist — this doc tells you where
they are and how to work around them.

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

## Step 1 — Understand what an App actually is

An App on ABW has three parts:

**1. The manifest** (`manifest.json` in your app directory):
```json
{
  "slug": "my_app",
  "name": "My App",
  "description": "What it does",
  "visibility": "private",
  "workspace_template": {
    "initial_budget": 200,
    "auto_hire": ["my_companion_agent"],
    "initial_files": [
      { "path": "my_app/state.yaml", "content": "# canonical document\n" },
      { "path": ".app/manifest.yaml", "content": "app_slug: my_app\n" }
    ]
  }
}
```

**2. An agent card** (the agent that talks to users in your App's workspaces):
```
agents/curated/my_companion/agent_card.json
```

**3. A UI** (optional — can be as simple as a single HTML file that
talks to the ABW API, or as complex as a full web app).

---

## Step 2 — Start here: call the schema endpoint

Once your App is deployed, this is the first thing to call:

```bash
curl https://agent-bestiary.world/api/apps/your_app_slug/schema
```

What comes back is the complete action grammar — every action your agent
can emit, every field it expects, and how to parse the action blocks from
the agent's responses. This is what your UI needs to implement.

There are two live examples you can call right now to see what the response
looks like:

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

## Step 3 — The simplest possible UI

Here's a complete working UI in under 100 lines of HTML. No framework,
no build step, no dependencies. Paste it into a file, open it in a
browser.

```html
<!DOCTYPE html>
<html>
<head>
  <title>My ABW App</title>
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
  <h2>My ABW App</h2>

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
    const ACTION_RE = /__ACTION__\n([\s\S]*?)\n__END_ACTION__/g;

    function wsId() {
      const v = document.getElementById('ws-id').value.trim();
      // Accept full URLs — strip to UUID
      const m = v.match(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i);
      return m ? m[0] : v;
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

      const resp = await fetch(`${BASE}/api/workspaces/${wsId()}/messages`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token()}` },
        body: JSON.stringify({ content: `@my_companion ${text}`, agent: 'my_companion' })
      });

      // Poll for agent response (or connect to SSE stream for real-time)
      setTimeout(async () => {
        const msgs = await fetch(`${BASE}/api/workspaces/${wsId()}/messages?limit=5`, {
          headers: { 'Authorization': `Bearer ${token()}` }
        }).then(r => r.json());

        const last = (msgs.messages || []).filter(m => m.sender_type === 'agent').pop();
        if (!last) return;

        const content = last.content || '';
        // Split prose from action blocks
        const actions = [...content.matchAll(ACTION_RE)].map(m => {
          try { return JSON.parse(m[1]); } catch { return null; }
        }).filter(Boolean);
        const prose = content.replace(ACTION_RE, '').trim();

        if (prose) addMessage('agent', prose);
        actions.forEach(addAction);

        // Dispatch each action to the server
        for (const action of actions) {
          await dispatchAction(action);
        }
      }, 3000);
    }

    async function dispatchAction(action) {
      // Your agent's system prompt may use domain-specific action names.
      // Map them to the canonical API endpoint names here.
      // Example for SimOps: 'edit_process' → 'mutate_document'
      // For a new App: define your own aliases or use the canonical names directly.
      // Get the full map from: GET /api/apps/your_app_slug/schema → type_name_map
      const TYPE_MAP = {
        // Add your App's aliases here, e.g.:
        // 'save_note': 'mutate_document',
        // 'tag_entry': 'annotate',
      };
      const type = TYPE_MAP[action.type] || action.type;
      const resp = await fetch(`${BASE}/api/workspaces/${wsId()}/actions/${type}`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Authorization': `Bearer ${token()}` },
        body: JSON.stringify({ ...action, app_schema: 'my_app' })
      });
      const result = await resp.json();
      console.log('Action dispatched:', type, result);
    }
  </script>
</body>
</html>
```

Replace `my_companion` with your agent's ID. Replace `my_app` in the
`app_schema` field with your App slug. That's your entire UI to start.

---

## Step 4 — The errors you will see and what they mean

### `401 Missing authorization token`

Your API token isn't being sent. Check:
- The token starts with `ferm_`
- You're passing it as `Authorization: Bearer ferm_...`
- If using the CLI: `export ABW_API_TOKEN=ferm_...`

Get a token at: `https://agent-bestiary.world/settings/api-keys`

### `403 Not a workspace member`

You're trying to access a workspace you haven't been added to.
Either spawn a new workspace from your App, or ask the workspace owner
to add you.

### `402 Payment Required` (or `insufficient credits`)

The workspace has run out of gas (credits). Fix:
- Ask the platform admin to top up: `POST /api/admin/workspaces/:id/grant`
  with `{ "credits": 200, "reason": "top-up" }`
- Or via the admin panel at `https://agent-bestiary.world/admin`
  → "⚡ Grant Credits to Workspace"

This happens often during development. Budget 200+ credits per workspace
for testing.

### `500 Internal Server Error` on `/api/workspaces/:id/messages`

Most common causes in order of likelihood:

1. **The agent name is wrong.** Double-check the `agent` field in your
   POST body matches exactly the `agent_id` in the agent card JSON.

2. **The workspace doesn't have the agent hired.** Check that your App's
   `auto_hire` array includes the agent, or hire it manually:
   `POST /api/workspaces/:id/hire` with `{ "agent_id": "my_companion" }`

3. **The agent card has a JSON syntax error.** Validate your agent card:
   ```bash
   cat agents/curated/my_companion/agent_card.json | python3 -m json.tool
   ```

4. **The server hasn't restarted since you pushed.** Agent cards are
   seeded from the filesystem at startup — a git push alone doesn't
   update the live agent. **Don't wait for a deploy.** Update the agent
   directly in the browser:

   → Go to `https://agent-bestiary.world/agent/my_companion`
   → Click the **Intelligence** tab
   → Edit the system prompt, bump the version number (e.g. `1.0.0` → `1.1.0`)
   → Click **Save**

   The change is live immediately. No deploy, no command line.

### Agent responds with old behaviour (wrong prompt)

The agent is running the old prompt from the database. This always
means the same thing: your git push updated the file on disk but the
running server hasn't picked it up yet.

**Fix: use the Intelligence tab, not git.**

Go to `https://agent-bestiary.world/agent/my_companion`, click
**Intelligence**, paste your new prompt, bump the version, save.
Takes 30 seconds. No git, no deploy, no waiting.

Once your prompt is stable and you're ready to commit it permanently,
then git push — the next server restart will pick it up and sync the
file with the database row. Until then, the Intelligence tab is your
edit loop.

### Action blocks don't parse / `JSON.parse` fails

The model produced malformed JSON inside an action block. This happens
with smaller models (Haiku, OpenRouter free). The fix:

1. Use Sonnet for the companion agent (the action grammar is complex
   enough to need it)
2. Add a `try/catch` around your `JSON.parse` — log the raw block and
   skip it rather than crashing
3. If you see partial blocks (only `__ACTION__` without `__END_ACTION__`),
   the model hit its `max_tokens` limit — increase to 4096 in the agent card

### `workspace_action_log` is empty after dispatching

The action POSTs are reaching the server (you'd get a 4xx otherwise),
but if you see `{ "actions": [] }` on `GET /api/workspaces/:id/actions`,
the rows are there — check you're using the right workspace UUID.
Workspace URLs look like `/workspace/abc123...` — the UUID is the part
after `/workspace/`.

### CORS errors in the browser

The ABW API allows `https://agent-bestiary.world` and localhost origins.
Known third-party App domains (like `kask.bio`) are also allowed on request.
If you're developing on a different domain, either:
- Develop on localhost (any port works — `http://localhost:3000` etc.)
- Use a proxy that forwards requests to the ABW API
- Ask the platform admin to add your domain to the CORS allowlist

---

## Step 5 — The development loop

**The primary loop — all in the browser, no deploys:**

```
1. Talk to your agent in a workspace — see what it does
2. Notice something wrong (wrong tone, missing action type, bad output)
3. Go to agent-bestiary.world/agent/my_companion → Intelligence tab
4. Edit the system prompt directly, bump the version number
5. Click Save — change is live in seconds
6. Go back to the workspace and try again
```

That's it. This loop takes 2 minutes per iteration, not 20.

**Git push is for when your prompt is stable**, not for active
iteration. Think of the Intelligence tab like a REPL — you experiment
there first, then commit to git once it's working. The next server
restart will sync the file with whatever is in the database.

**What the Intelligence tab lets you edit:**
- System prompt (the most important thing)
- Model and temperature
- Model ladder (which model runs for free vs standard vs premium tier users)
- Version number (bump this every time you change the prompt so you can
  tell which version a workspace is running)

**Checking what's actually live:**
```bash
curl "https://agent-bestiary.world/api/agents?search=my_companion&limit=1" \
  | python3 -c "
import sys, json
a = json.load(sys.stdin)['agents'][0]
print('version:', a['version'])
print('prompt starts with:', a.get('system_prompt','')[:80])
"
```

If the version and first line of the prompt match what you saved in the
Intelligence tab, you're live. If they don't, wait 30 seconds and try
again — the save is async.

**The action log is your feedback signal:**
```bash
abw workspace actions list <ws-id>
```
After each test turn, check what actions the agent actually emitted.
If the log is empty, the agent isn't producing action blocks — the
prompt needs work. If the log has entries but they're wrong type or
wrong shape — the prompt needs refinement. The log tells you the truth
about what the agent is doing, not what you hope it's doing.

---

## Step 6 — What to give your AI coding assistant

When you want your coding agent to build a UI or dispatcher for you,
give it exactly these three things:

**1. The schema endpoint output:**
```
GET https://agent-bestiary.world/api/apps/your_app_slug/schema
```
This returns your App's full action grammar as structured JSON. If you
haven't deployed your App yet, use `kask_simops` or `efrain_ai` as a
reference to understand the shape before writing your own.

**2. The workspace API surface:**
```
POST /api/workspaces/:id/messages        — send a message / invoke agent
GET  /api/workspaces/:id/messages        — list recent messages
GET  /api/workspaces/:id/messages/stream — SSE stream for real-time
GET  /api/workspaces/:id/files/:path     — read a file
POST /api/workspaces/:id/files           — write a file
POST /api/workspaces/:id/actions/:type   — dispatch a typed action
GET  /api/workspaces/:id/actions         — list action log
GET  /api/workspaces/:id/annotations     — list annotations
```

**3. The action parsing snippet:**
```javascript
const ACTION_RE = /__ACTION__\n([\s\S]*?)\n__END_ACTION__/g;
const actions = [...content.matchAll(ACTION_RE)].map(m => JSON.parse(m[1]));
const prose = content.replace(ACTION_RE, '').trim();
```

With these three inputs, any capable AI coding assistant can generate
a working UI in one pass.

---

## Step 7 — What's still rough (known limitations as of May 2026)

These are real gaps. Work around them; don't fight them.

**Agent prompt updates need a redeploy** (or the PUT workaround above)
to reflect in the API. The seeder runs at server startup, not on push.

**The diff modal for `mutate_document` with `confirmation: "ask"` is
alpha.** During the alpha period, all `mutate_document` actions are treated
as `pending` regardless of the confirmation field. Your UI needs to call
`POST /api/workspaces/:id/actions/:action-id/accept` after the user
reviews. The companion's action block is intent; the accept call is
execution.

**SSE streaming works but the agent response arrives as a single message**
at the end of execution, not token-by-token. If you want streaming tokens,
use `GET /api/workspaces/:id/messages/stream` — it emits one SSE event
per message when the agent finishes. It doesn't stream partial tokens.

**The `compare` action records intent only.** The client is responsible
for running the cascade on each variant and calling `/accept` with the
results. The server doesn't run cascades automatically yet.

**Credits deplete fast during development.** Each agent turn costs
6-15 credits depending on the model and context size. A 200-credit
workspace lasts about 15-25 turns. Ask the admin for a grant rather
than buying credits until you're ready to launch.

**xaman_ek's MCP tools require a Bearer token.** Unlike public GET endpoints,
workspace-scoped tools (`read_workspace_file`, `list_workspace_agents`)
return `401` without a valid token. Pass `Authorization: Bearer ferm_...`
in your MCP client configuration.

---

## Worked example — efrain_ai

`efrain_ai` is a real App on ABW built by an external developer (Mario).
It's a good reference because it's simpler than SimOps and shows the
pattern without the domain complexity.

**What it is:** a research notes App. The companion agent helps the user
capture, organise, and cross-reference notes on a topic. The canonical
document is a YAML file under `efrain/notes.yaml`.

**What the schema looks like:**
```bash
curl https://agent-bestiary.world/api/apps/efrain_ai/schema
```

Because `efrain_ai` is a simpler App, its action grammar has fewer types —
mostly `mutate_document` (update a note) and `annotate` (tag an observation).
That's a good starting template: you don't need all six action types to
have a working App.

**What its workspace looks like:**
```bash
# Read the current notes document
abw workspace files get <ws-id> efrain/notes.yaml

# See what the companion has been doing
abw workspace actions list <ws-id>

# Send a message to the companion
abw workspace message <ws-id> "what connections do you see between these entries?" \
  --agent efrain_companion
```

**The lesson from efrain:** start with two action types (`mutate_document`
and `annotate`), get them working end-to-end, then add more as your domain
demands them. The schema endpoint will reflect whatever your agent card
declares — it grows with the App.

---

## Reference

**App lifecycle:**
```bash
abw login
abw app new my_app
abw app validate
abw app deploy
abw app spawn my_app
```

**Workspace interaction:**
```bash
abw workspace message <ws-id> "hello" --agent my_companion
abw workspace files get <ws-id> my_app/state.yaml
abw workspace actions pending <ws-id>
abw workspace actions accept <ws-id> <action-id>
abw workspace actions annotate <ws-id> "note" --kind insight
```

**Key URLs:**
```
https://agent-bestiary.world/apps               — App catalogue
https://agent-bestiary.world/agent/<id>         — Agent detail + Intelligence tab
https://agent-bestiary.world/admin              — Admin panel (grant credits, manage agents)
https://agent-bestiary.world/settings/api-keys  — Mint API tokens
https://agent-bestiary.world/api/apps/<slug>/schema — Your App's action grammar
```

**MCP endpoint (for AI coding tools like Cursor):**
```
https://agent-bestiary.world/mcp/agents/xaman_ek
```
Configure this in your MCP client with your Bearer token. xaman_ek
can then tell you what agents exist, read workspace files, and execute
agents on your behalf — all from inside your coding environment.
