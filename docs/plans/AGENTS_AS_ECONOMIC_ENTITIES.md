# Agents as economic entities — finishing the shelf, and what it turned up

Status: the shelf half is **shipped**. Item 1 of §7 (`agents.skills`) is
**shipped** — see §4. The fleet half is **proposed**, ordered, and not started.

The brief was two sentences: *the configuration shelf is missing skills, MCP,
forking, pricing and dream budget — and importantly the ability to fund the
agent's dreaming.* Then: *this comes back to needing to rethink the dashboard
and profile, because the real job is managing fleets of agents, workspaces and
apps as economic entities.*

Those are one problem. An agent that cannot be funded, priced or credentialled
from the surface you configure it on is not an economic entity; it is a card
with a wallet attached somewhere else.

---

## 1. What was actually missing, measured

The old Manage tab (`templates/agent_detail.html#manage`) carries twelve
sections. The shelf (`templates/specimen.html`, `Configure ⚙`) carried four of
them. The audit:

| Manage section | was on the shelf | now |
|---|---|---|
| Lifecycle (publish / archive / restore) | yes | yes |
| Edit: system prompt, tags, min tier | yes | yes |
| Edit: **description** | **no** | **yes** |
| Valence | yes | yes |
| Version history | yes (inside the prompt) | yes |
| Orchestras | no | no — see §5 |
| **Agent wallet** | no | **yes**, and the endpoint now works — §2 |
| **Dreaming budget + top-up** | read-only figure | **yes, fundable** |
| **Consolidation** | no | **yes** |
| **Fork pricing** | no | **yes** |
| Fork / duplicate | no | link to the flow |
| **Remote MCP servers** | no | **read + test + supply the key**; the form is still the old page |
| **Published tools** | no | **read + phantom warning**; the editor is still the old page |
| **Skills** | no | **editable**, split into executable vs label — §4 |

Three of those turned out not to be UI work at all.

---

## 2. The wallet endpoint had never worked from a browser

`AGENT WALLET → Could not load wallet` on the screenshot is not a loading
state. Every handler in `src/handlers/agent_wallet.rs` opened with

```rust
let agent_uuid = Uuid::parse_str(&agent_id)
    .map_err(|_| (StatusCode::BAD_REQUEST, "Invalid agent ID".to_string()))?;
```

and **every URL on this platform carries the agent name.** `/agent/football_analyst`,
`/specimen/football_analyst`, `/api/agents/football_analyst/…`. The uuid appears
in no link a page or a human ever holds. So the panel called
`/api/agents/football_analyst/wallet`, got a 400, and printed the fallback —
for every agent, for as long as the panel has existed.

Forty-odd other agent routes go through `resolve_agent`, which takes either.
This file spoke a different dialect and nothing could notice, because each
handler is correct in isolation.

Fixed by `resolve_agent_for_wallet`. One trap worth recording: the agent
wallet's `owner_ref` **is the uuid string**, so passing the path parameter
straight to `get_or_create_wallet` would have minted a second, empty wallet
named `football_analyst` beside the funded one — a zero balance with no error.
`wallet_ref(agent_uuid)` is now the only thing the wallet layer is handed, and
a test reads the file to keep it that way.

## 3. Money must never be a field

The fast way to add fork pricing and budgets to the shelf is three entries in
the `FIELDS` table. It is also a privilege escalation.

`PUT /api/agents/:agent_id` requires **edit**. Every money route requires
**admin**, deliberately, with the reason written at the call site:

> `update_fork_pricing_handler` — *"Fork pricing is a monetary policy decision —
> Admin (owner or platform admin) only. No shares."*

But `AgentUpdate` **has** `fork_pricing` and `education_budget_credits`, so
nothing in the type system stops the field being added, and a share-holder
could then re-price somebody else's agent through the general endpoint.

This is the publish-gate bypass (`reject_lifecycle_fields`) one door along, and
it gets the same answer: money is an **action against the endpoint that guards
it**, never a field on the endpoint that does not. `scripts/check_agent_fields.js`
now asserts this rather than trusting it, and also asserts that fork pricing
still requires admin — so if that ever relaxes, the guard is re-read rather
than silently outgrown.

Consequence for the widget: a group can be all actions and no fields.
`economics` and `instruments` are, and they render no save bar. A permanently
disabled `save` beside controls that write immediately is the worst of both.

## 4. Skills were not editable by anyone — now shipped

`capabilities.skills` carries **two unrelated things under one name**, and
`validate_card_skills` already draws the line at execution time:

* **executable** — a name in `SkillRegistry`; a deterministic function the
  executor invokes directly, no model in the loop;
* **label** — free text like `market-analysis`, read by `xaman_ek` for
  discovery and by nothing else.

Every surface printed them as one flat chip row, so a **typo in an executable
skill name is indistinguishable from a taxonomy label** and degrades silently
into one. The shelf now splits them, from the same registry the executor reads.

And the part that mattered: there was **no `agents.skills` column and no
`AgentUpdate.skills`**. The filesystem card was the only writer and deployed
filesystems are read-only, so an owner could not grant their agent a skill by
any route.

That is the lever on composability, because `execute_list_agents` — the index
every navigator scans — returns exactly:

```json
{ "id": "…", "type": "…", "description": "…", "skills": […] }
```

One quarter of what any strategist composes on, and it was nailed shut.

### 4.1 What shipped

* **`migrations/239_agents_skills.sql`** — `skills TEXT[]`, nullable, **no
  default**, plus a GIN index for the "who declares this" query. Nullable with
  no default is the whole design: `DEFAULT '{}'` would have made all 743
  existing rows say *explicitly none* on first read and stripped every agent
  of its card-declared skills.
* **`AgentUpdate.skills` / `Agent.skills`** as `Option<Vec<String>>`. The
  `Option` carries the precedence; a bare `Vec` would make "inherit" and
  "remove everything" the same value.
* **`resolve_agent_card`** applies NULL / `[]` / non-empty, exactly as it does
  for `mcp_servers`. Simpler than `interpret_db_column` because `Vec<String>`
  admits no shape ambiguity — there is no legacy spill to defend against.
* **`agent_card_from_db`** now fills skills from the row. Unlike the two MCP
  columns beside it, this one is not misnamed, so a DB-only agent (709 rows
  against ~95 card files) reaches the fleet index with its skills instead of
  empty.
* **`GET /api/skills`** — the executable vocabulary, unauthenticated. It did
  not exist: `SkillRegistry` lived in a Rust `vec!` and appeared in no API, no
  page and no tool response, so an author was declaring against a list they
  could not read and `xaman_ek` could not enumerate what the platform can run.
* **`normalise_skills`** on the write path — trim, drop blanks, order-preserving
  dedupe, a 64-label cap, and one refusal (below).
* **The shelf** edits it, classifying each entry live against `/api/skills`.

### 4.2 The one refusal, and why only one

Any string is a valid taxonomy label, so an unknown name **cannot** be
rejected without making half the field unusable. The consequence is the
silent failure: a typo in an executable name is not an error, it is a new
label, and the agent quietly does not get the capability.

Exactly one case is decidable without guessing: a name matching a registered
skill **case-insensitively but not exactly**. Nobody wants a discovery keyword
that differs from a real capability only in capitalisation. That is refused,
naming the exact spelling that would work.

Fuzzy "did you mean" over edit distance is deliberately not attempted. It
would start refusing legitimate labels that happen to resemble a skill name,
and **a validator that rejects correct input is worse than one that misses
some incorrect input.**

Everything else is reported rather than refused: the PUT response returns the
`{executable, labels}` split, and the editor shows it as you type.

### 4.3 The staleness trap this exposed

`resolve_agent_card` bridges the DB over the boot-time registry for every
*execution* path — but the five fleet tools call `registry.list_cards()`
directly, because they are about the fleet rather than about one agent.

Harmless while skills lived in a file nobody could write. The moment the
column existed it became the worst possible failure: an owner edits what their
agent is discoverable as, the index keeps serving the boot-time value, and
**the edit appears to do nothing until the process restarts.** A discovery
field that takes a deploy to take effect is not a discovery field.

`apply_skill_overrides` resolves it with one query over the rows that have an
opinion (`skills IS NOT NULL`). Deliberately **not** done by mutating the
registry cache on write: that would leave the in-memory card holding a
DB-derived value with no way back to "inherit the card" if the column is later
cleared — a staleness bug traded for a staleness bug.

---

## 5. What the shelf still does not have

Named so they are decisions rather than omissions.

* **The MCP server form.** Add / edit / remove a server is ~600 lines living
  inside `agent_detail.html`, and a second copy of it is the drift this repo
  keeps finding. The shelf has the read, the credential state, the connection
  test and the one action an owner is actually blocked by — supplying the key —
  and links out for the rest. The port is §7.2: move that code into
  `static/js/widgets/agent-instruments.js` and mount it from **both** pages, the
  way `AgentFields` is already mounted from the shelf and the create wizard.
* **The published-tool checkboxes.** Same shape, same reason. The shelf shows
  what is exported, what could be, and the phantom entries (advertised tools
  with no dispatch arm, which fail with `Unknown tool`).
* **Orchestras.** Membership of `fermi` is admin-gated and reviewed. It is a
  *relationship*, not a setting, and it belongs with compositions (§6) rather
  than in a per-agent drawer.

---

## 6. The fleet half — proposed

The dashboard is a portfolio, five loop cards, an app grid, a composition list
and an agent list. The profile is a bio, three stat tiles, a credit balance,
provider keys, connections and an audit log. Between them sit **104 agents**,
and there is no surface that answers an operator's actual questions:

> Which of my agents cost more than they earn?
> Which are one key away from running?
> Which have stopped learning because their dream budget is dry?
> Which composition is carrying which agent?

Every input to those exists. `episodes.cost_usd`, `agent_episode_payouts`,
`dreaming_budget_credits`, `/api/agents/:id/funding`, `select_agent_decisions`,
`gate_decisions`. None is aggregated anywhere an owner looks.

### 6.1 One vocabulary: the agent has a service level

The pieces of an SLA are already in the schema under other names, which is why
this is a naming job before it is a building job:

| SLA term | what the platform already stores |
|---|---|
| price | `competition.price_credits_per_call`, `fork_pricing` |
| capability | the output contract, `accepts` / `produces`, instruments |
| reliability | `Gate::OutputSchema` fidelity — approved / (approved + refused) |
| calibration | Brier, and Loop 5's warm/cold |
| demand | `select_agent_decisions` selection rate |
| cost | `episodes.cost_usd` |
| revenue | `agent_episode_payouts` |
| solvency | agent wallet balance, dream credits left |
| runnable at all | `/api/agents/:id/funding` |

An agent's shelf already shows nine of these. **Nothing sums them over a
fleet.** That is the whole gap.

### 6.2 The dashboard becomes an operations surface

Replace the portfolio strip and the agent list with one table whose rows are
agents and whose columns are the table above, sorted by the thing an operator
is actually hunting: **unrunnable first, then unprofitable, then idle.** The
five loop cards stay — they are the platform's own health and they are good —
but they move below the fold, because `127 items need attention` is a number
you act on after you know which of your own agents are dead.

Three rules it must obey, all paid for already:

1. **Absent is not zero.** An agent with no payouts and an agent whose payout
   read failed are different rows. `rollup_trust` exists because
   `agents.total_executions` was believed once.
2. **One producer per number.** Server-side composition, like
   `/api/specimen/:agent_name` does. The old agent page rendered thirteen
   metrics twice under different names because it composed from a dozen
   endpoints on the client.
3. **Every cell is a link to the thing that fixes it.** A red `no key` cell
   goes to `/profile#connections`; a dry `dream` cell opens the shelf's
   economics panel. A dashboard that grades and cannot act is the checklist
   this project keeps rebuilding by accident.

### 6.3 Compositions get the same treatment

A composition is an economic entity with members, and the manager-effect metric
(`Team Brier − Counterfactual Brier`) already exists to say whether the
coordinator is earning its cut. Per composition: cost, revenue, manager effect,
coherence score, and which member is dragging. Orchestras belong here.

### 6.4 Profile becomes the credential ledger, and nothing else

Today it mixes identity, vanity stats, a wallet and two different kinds of
secret. Two of those belong elsewhere:

* stats → the operations table (§6.2);
* credits → the wallet, which the dashboard already leads with.

What stays is the thing only the profile can hold: **keys, and who spent them.**
Provider keys (scope `*`) and connections (scope `agent_name`) are the same
mechanism at two scopes and should be one list with a scope column, sorted by
**which agents each key unblocks** — the join that has never been rendered and
is now half-built, because the Instruments panel does exactly this join for one
agent. Doing it for the fleet is the same query without the `WHERE`.

---

## 7. Order

1. ~~**`agents.skills`**~~ — **shipped**. (§4)
2. **Port the MCP editor** to `static/js/widgets/agent-instruments.js`, mount it
   from the shelf and from `agent_detail.html`, delete the inline copy. Removes
   ~1,000 lines from a 6,500-line template and is the last reason to send an
   owner to the old page. (§5)
3. **The fleet table** — one server-composed endpoint, one page. (§6.2)
4. **Compositions** on the same footing. (§6.3)
5. **Profile → credential ledger**, with the key → agents join. (§6.4)
6. Retire `agent_detail.html` once 2 and 3 land, which is what "lean this thing
   out" resolves to in lines of code.

---

## 8. Measurements worth keeping

Written down because each one was surprising and each one is the kind of thing
that regresses quietly.

* The agent wallet endpoint returned 400 for every browser request. Nothing
  reported it, because the panel's failure text was indistinguishable from an
  empty wallet.
* `AgentUpdate` accepts three money fields that the dedicated money endpoints
  gate on admin. The type system is not going to catch the next one.
* `capabilities.skills` was unwritable through the API, on every agent — while
  being one of the four fields the fleet index serves to every navigator.
  `SkillRegistry` was published nowhere at all, so the vocabulary could not be
  read either. Both fixed in §4.
* The fleet tools read the boot-time registry, not resolved cards. Any future
  card field that becomes DB-writable inherits the §4.3 trap unless it is added
  to `apply_skill_overrides` or those tools start resolving.
* A DB-only agent (709 rows against ~95 card files) is still absent from
  `registry.list_cards()` entirely, so the fleet index cannot see it at all.
  That predates this work and is the next thing to measure: the navigator's
  index covers an eighth of the fleet.
* `agent_versions` snapshots seven columns — description, system_prompt, tags,
  model, temperature, visibility, display_alias. **Skills are not versioned,
  and neither are `accepts`, `produces`, `output_contract`, `mcp_servers`,
  `valence` or `competition`.** So "restore v3" restores the prompt and leaves
  the agent's declared interface wherever it is now. Pre-existing and
  consistent across every card field added since mig-024; named here because
  the prompt panel presents that trail as *the* audit trail, and it is an
  audit trail of one field.
* `mcp_servers` / `mcp_tools` NULL means *the card decides*, and `[]` means
  *the owner removed what the card declared*. Printing both as `0` tells an
  owner their agent reaches nothing while the executor hands it a full card —
  which is why the shelf's summary line says "from its card file" rather than a
  number it does not have.
