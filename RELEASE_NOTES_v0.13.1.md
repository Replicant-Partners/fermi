# v0.13.1 — Hire any agent, not just the curated ones

An operator hired his own fine-tuned agent onto a driver and got back:

```
404 — Agent 'efra_valuation_strength_factor' not found
```

There is no agent by that name, and there never was. It's two names glued
together: his agent, `efra_valuation`, and the driver he hired it onto,
`strength_factor`.

When you assign an agent to a driver, the console gives that *pairing* its own
identifier inside the forecast — `<agent>_<driver>` — so the same agent can
work on several drivers with a different question and schedule on each. That
part is fine. The problem was getting back out: to actually run the agent, the
console has to take the driver half off again, and it did that by checking the
name against a **hardcoded list of 29 curated agents**. Anything not on the
list didn't get split, and the glued-together name went to the server as if it
were an agent id.

Nobody's own agent is on that list. Only those 29 ever were.

The same list decided which evidence belonged to which agent, so the second
symptom followed from the first: research **did** run, and its findings landed
in the forecast — you could read them in the Wiki tab — but they never attached
to the driver that paid for them. The driver kept saying it was waiting for
evidence that was sitting three tabs away.

It looked like a funding problem, because the first run had worked and charged
credits and the retry hadn't. It wasn't. An unfunded agent fails with its own
message, naming the provider and telling its owner exactly which key to set.
A 404 was always about identity, never about money.

## Fixes

- **Hiring your own agent onto a driver works.** So does **Retry**, so does
  Fermi's automatic assignment, and so do scheduled re-runs. Curated,
  community and private agents now take exactly the same path.

- **Evidence attaches to the driver that hired the agent.** Findings, the
  confidence dots, the evidence count, the suggested-p50 prompts — all of it
  was computed from the same broken match and is now correct.

- **Existing forecasts repair themselves on open.** Evidence that was already
  stranded gets picked up. There's nothing to migrate and nothing to re-run.

- **▶ Run now / 📅 Daily / 📅 Weekly in the Schedules tab.** These were
  gluing the driver name on a *second* time, so they fired a name no server
  could resolve and wrote a duplicate schedule row while they were at it.

- **The Agent Fleet panel's "THIS SESSION" list.** Any agent hired to a
  driver was missing from it, along with the list of drivers it was working
  on. Both were looking up an agent id and finding a pairing name.

- **Driver cards show the real agent id.** The label was built by taking the
  first two words of the pairing name, which is right only for agents whose id
  happens to be two words long.

- **"Update outside rate" finishes.** The row it created was tracked under one
  name and completed under another, so it span forever whatever the outcome.

## Under the hood

The split is not something to guess at. A forecast already records which
drivers an agent is bound to, and that record round-trips through FPL, so the
suffix is *known* — exactly, for every agent, with no list to keep up to date.
That is now how it's done, in one place
(`crates/fermi-console/src/agent_naming.rs`) with tests, and agent runs carry
the platform agent id explicitly rather than having it re-derived from a name
at each of the nine places that needed it.

Both hardcoded lists are gone — the 29-name one and an out-of-sync 8-name copy
of it that had drifted in a second file.

## Known issues

- For evidence saved **before** this release, we can tell which agent produced
  it but not which driver, so an agent hired onto two drivers in the same
  forecast will show its old findings under both. Anything gathered from now
  on records the pairing and is exact.

- A scheduled auto-run still doesn't add a visible row to the driver card until
  you reopen the forecast. Unchanged by this release.

## Breaking changes

None. FPL on disk is unchanged, and no saved forecast needs editing.

## Upgrade notes

Nothing to do. Update, restart, open a forecast — drivers pick up the evidence
their agents already gathered.

If a hire still 404s after updating, read the id in the error: it will now be
the bare agent id, with no driver glued to it. That means the platform can't
resolve *that id*, which is a different problem — usually a legacy agent name
containing `-` or `/`, which can't be routed and needs an admin rename (see
v0.10.20).
