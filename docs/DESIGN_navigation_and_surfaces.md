# Navigation and surfaces — what the platform is made of, and what goes away

**Status:** proposal. Written 2026-09-01, after `/api/specimen/:name` was found
taking 46 seconds and a specimen's pulse list was found to be a stripped copy of
the stream's.

**Spirit guides, named because they decide arguments:** Bret Victor (the thing
you are manipulating should be the thing you are looking at), Tufte (density
without decoration; the data is the interface), Tschichold (a grid, a hierarchy,
and the discipline to leave the rest out).

---

## 1. The nouns

Five, and everything on the platform is one of them or a view of one.

| noun | what it is | plural surface | singular surface |
|---|---|---|---|
| **pulse** | one invocation and its output | `/pulses` | — |
| **trace** | the audit of one pulse | — | `/trace/:id` |
| **agent** | a specimen. **simple or compound — both are agents** | `/bestiary` | `/specimen/:name` |
| **workspace** | the place a team of agents works | `/workspaces` | `/flow/:id` → the work surface |
| **app** | a manifest that instantiates as a workspace | `/apps` | `/app/:id` |

**A compound agent is an agent.** `cohere_and_coordinate` has a card, takes a
query, emits a pulse and has a trace, exactly like a simple one. What differs is
*implementation* — its work fans out to a team — and that is a property, not a
fifth noun. Asked and answered: there was no reason for it to be anything else,
and inventing one would have split the bestiary in two for nothing.

What it does need is two things a simple agent does not: a **roster** on its
specimen page, and the knowledge that its pulses have children. The second
already renders — the trace's flow strip draws `▶ it hired` — so the missing
piece is the roster.

Two relations do the rest of the work:

* **an app instantiates as a workspace.** The app is the manifest and the
  configuration; the workspace is the running instance. One app, many workspaces.
* **a pulse belongs to an agent, and happens inside a workspace.** Which is why
  pulses can be listed by agent *and* by workspace *and* by app — three filters
  on one list, not three lists.

> **Blocker, and it is the whole reason app/workspace pulse views do not exist
> yet:** `episodes` has **no `workspace_id`**. The stream endpoint says so in its
> own contract string — *"There is no workspace filter because `episodes` carries
> no workspace column."* Until that column exists and is written at the execute
> boundary, "pulses in this workspace" can only be reconstructed through
> `workspace_messages`, which is a join through prose. **This is the first thing
> to build**, not a UI task.

## 2. What goes away

| surface | why | where its content goes |
|---|---|---|
| `/loops` | mostly documentation; the per-artifact answer is already on the trace | the loops fold on `/trace/:id`; the prose to `/docs` |
| `/gates` | routes to the same handler as `/loops` — literally the same page twice | as above |
| `/agent/:id` (legacy detail) | 6,500-line template, eight tabs, thirteen metrics rendered twice under different names | `/specimen/:name` + the configuration shelf |
| `/rounds`, `/ecology`, `/declarations`, `/catalogue` | already demoted to a "More" dropdown, which is where surfaces go to be forgotten rather than removed | `/declarations` is a real worklist and should merge into the bestiary as a lens; the rest to `/docs` |

`/gates` and `/loops` sharing one handler is the tell. A page that is two routes
to the same render is not a surface, it is a bookmark.

**Rule for removal:** a page dies when every *action* it offers exists elsewhere
and every *number* it shows has one producer that another surface already reads.
Documentation is not an action. Until then it is demoted, not deleted — the same
discipline the "More" dropdown was invented for, applied honestly with a date.

## 3. The configuration shelf

Replaces the legacy tabs. The requirements, from use:

1. **A shelf, not a page.** Configuration is something you do *to the thing you
   are looking at*, so the specimen stays on screen. This is the Bret Victor
   constraint and it is the reason the legacy tabs failed: they replaced the
   agent with a form about the agent.
2. **Wider than the current shelf.** The `Configure ⚙` affordance opens something
   too narrow to hold a card editor. It needs to be resizable, and to remember.
3. **The opposite shelf is Xaman Ek.** Guidance sits on the other side, in some
   modes, and talks about the configuration in progress. Two shelves, one
   subject, and the subject stays in the middle.
4. **Panels, grouped, in the order the platform can rank.** An earlier draft of
   this said "one panel per declaration rung", which was too narrow — **contract
   is not the only configuration aspect.** Three groups:

   | group | what it holds |
   |---|---|
   | **intelligence** | model, provider, executor, system prompt, temperature, persona version |
   | **manage** | price, fork policy, valence, visibility, tier, status |
   | **declaration** | ports, output type, output schema, grounding contract |

   Only the third is rankable by the platform — `/declarations` already computes
   the cheapest next rung per agent — so that group leads with a *recommended
   next action* and the other two are edited on demand. That is the difference
   between a form and a workbench: the platform says what is missing, and the
   author decides what to do about it.

What this kills as *tabs*: Overview · Activity · Knowledge · Contract · Economics
· Field Notes. Overview/Activity/Economics are `/specimen`'s Profile and Record.
Knowledge and Field Notes are read-only content, not configuration. Contract,
Intelligence and Manage are the three groups above.

## 4. Pulse views

One list, three filters. The row is already shared (`static/js/widgets/pulse.js`,
`/static/css/pulse.css`) and the projection has one producer
(`PULSE_SELECT` + `pulse_row` in `handlers/specimen.rs`).

```
/pulses                  every pulse            (today: /stream)
/pulses?agent=…          this specimen's        (today: the Record tab)
/pulses?workspace=…      this workspace's       BLOCKED on episodes.workspace_id
/pulses?app=…            this app's             = union over its workspaces
```

`/stream` becomes `/pulses` with a redirect. The nav already says **Pulses**;
the page said "Stream" until today, which is the vocabulary lagging the map.

## 4.1 What `/flow/:id` is missing

The singular workspace surface is the right shape already. Three additions, in
order of how much they are missed:

1. **The agent roster.** Who is in this workspace. `workspace_agents` has it and
   the flow does not show it, so a diagram of hops names agents without ever
   listing the team.
2. **A link to the actual work surface.** `/flow/:id` is the *record* of a
   workspace; the place work happens is a different screen and there is no way
   to get from one to the other. That surface needs its own cleanup later — the
   link does not.
3. **Which workspaces and apps an agent belongs to.** Missing entirely, in the
   other direction: open a specimen and you cannot tell what teams it is on.
   This belongs on the agent page, and it is the same join in reverse.

## 5. Complexity hiding — the rules already in use

Stated because they were derived on the trace and now apply everywhere:

1. **A lens changes columns and sort, not the page.**
2. **Explain once, not per row.** A reason that belongs to a *state* goes in one
   legend keyed by the token the rows print.
3. **A fold hides detail, never a finding**, and its summary carries the count it
   is hiding.
4. **Absent must look different from bad.**
5. **Value · condition · act, positionally fixed.** Learn one row, read a hundred.
6. **If the platform can name what would close a gap, the name is the control.**
   Never print the name of a remedy you do not offer.

## 6. Order of work

**Nothing legacy is removed until its replacement is complete.** `/agent/:id`
stays reachable — it is the reference for what the shelf has to absorb, and a
half-built shelf plus a deleted page is worse than either.

1. **`episodes.workspace_id`** — **done, mig-226.** Written at the execute
   boundary, which now *requires* every call site to declare a workspace or
   `None`; six of the eleven turned out to have one in scope already, including
   the delegation hop, where `ToolContext.workspace_id` had been sitting unused.
   Not backfilled, for the reason in the migration. Unblocks §4.
2. **The configuration shelf**, one rung panel at a time, against `/specimen`.
   Legacy `/agent/:id` stays reachable until every panel exists.
3. **`/pulses`** with the three filters; `/stream` redirects.
4. **Demote `/loops` and `/gates`** to `/docs` with a date, then delete.
5. **`/apps`** as manifests, with instantiated workspaces listed under each.

## 7. Open questions

* **The Observatory needs rethinking, and the Health tab is its symptom.** The
  tab was built as an *entry into* the Observatory and is not useful on its own —
  eleven panels, each answering "can the platform say anything about this agent",
  which is a question about the platform rather than about the agent you opened.
  It is also the only reason `/api/specimen/:name` computes a fleet-wide census:
  46s before the conformance fix, 9.4s after, and the remaining 9s is
  `Observation::collect` plus eleven serial `resolve_for_subject` calls.

  Three things to decide together, and they are one decision:

  1. What is the Observatory *for*? If it is "what is wrong across the fleet",
     it is a worklist and should look like `/declarations` — rows with an owner
     and a next action — not a grid of panels.
  2. What does an agent's page owe it? Probably one line: *what the platform
     cannot currently say about this agent, and whose job that is*. One line,
     not eleven panels.
  3. Either way the census belongs on a clock with a cache. A page about one
     agent computing a fleet-wide observation is the defect, and it will come
     back under a different name until that is fixed.
* **App manifest format.** Named as having "their own configurations, manifests
  etc." — it needs a home in the repo before the UI can read it.
