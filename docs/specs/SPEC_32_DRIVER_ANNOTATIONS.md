# Spec 32 — Driver annotations: disagree with the input, not the question

**Status:** complete (v0.11.13)
**Answers:** *"teams coordinate on trajectories and research and **assumptions**"*

Specs 26–31 built the first two. Provenance answers *what research went in*,
Trajectory and History answer *how the number moved and who moved it*. The
third had nothing: there was no way for one forecaster to tell another that
an input was wrong, in a place where the next reader would see it.

## 1. The hole

Today that conversation happens in Slack, or as a probability revision with
a `reason` string, or not at all. None of those attach to the thing being
disputed, so the objection is invisible to whoever opens the forecast next
— which is precisely when it matters.

The `reason` string is the closest existing thing and it is the wrong shape
twice over. It requires you to *change the number* to say something, so a
reader who thinks the base rate is wrong but doesn't have a better one has
no move at all. And it is attached to a revision, so it describes a
transition rather than a standing claim: there is no state for "we still
disagree about this".

## 2. Anchored to the driver

A forecast-level comment thread would have been much easier and much less
useful. Disagreement in this product is almost never about the question; it
is about **one input**. Anchoring at `(forecast, driver)` means:

* it renders next to the number it disputes;
* it survives a revision of some *other* driver;
* "which assumptions are contested" becomes a query — which the ops board
  turns into coordination work (§5).

A `NULL` driver is allowed and means the annotation is about the forecast as
a whole. "The whole framing is wrong" is a real and useful thing to say, and
forcing it onto an arbitrary driver would misfile it.

### 2.1 Where drivers actually live — the correction

The first cut of this spec anchored to `fermi_forecasts.drivers`, a JSONB
column that looks exactly like the natural home for driver state. Checking
it against production before building the UI:

```
 typ  | count | max_len
array |    78 |       0     ← every row is an EMPTY array
```

**Nothing populates that column.** A driver is a `driver <name> { ... }`
declaration inside the forecast's FPL program (`fpl_source`) — a language
construct, parsed by `fermi::Parser`, which is what the executor, the LSP
and BayesOps all read. `bayesops_*.driver_name` is keyed by name for exactly
the same reason.

Re-checked against real data after re-anchoring:

| | |
|---|---|
| Forecasts with driver declarations in `fpl_source` | **66 / 78** |
| Forecasts with driver data in the `drivers` column | **0 / 78** |
| Real programs that parse and yield a name set | **66 / 66** (342 names) |

Had this not been checked, the entire feature would have attached to a
phantom: every annotation instantly orphaned, every badge zero, the ops
detector permanently silent — and all of it *working as coded*.

`driver_name` is therefore `TEXT` with no foreign key. Normalising an FPL
language construct into a table would be a far larger change than this
feature justifies.

## 3. The schema (migration 183)

```
driver_annotations
  id, forecast_id → fermi_forecasts ON DELETE CASCADE
  driver_name TEXT NULL          -- NULL = the forecast as a whole
  author_id, body
  kind    challenge | question | note
  status  open | accepted | declined | orphaned
  resolved_by, resolved_at, resolution_note
  at_commit                      -- the Spec 31 sha it was written against
```

**Status, not deletion.** An annotation is a claim someone made. Resolving
it records what happened rather than erasing it: `accepted` (the driver
changed as a result) and `declined` (considered, rejected) are different
outcomes, and the difference is exactly the kind of reasoning a team wants
to be able to re-read.

**Resolution must be attributable.**

```sql
CHECK (status = 'open' OR status = 'orphaned'
    OR (resolved_by IS NOT NULL AND resolved_at IS NOT NULL))
```

This is the same gap Spec 26 existed to close for revisions. There is no
reason to reintroduce it in a new table, so the database refuses.

**`at_commit`** lets the UI say *"raised when this read 1780"* after the
value has moved — the difference between a comment that ages into nonsense
and one that stays legible.

## 4. The orphan sweep, and why it runs backwards too

A name is not a reference, so a driver can be renamed or removed out from
under an annotation. `mark_orphaned_annotations` parses `fpl_source` after
any program edit and reconciles. Two properties are load-bearing:

**Fail-safe.** If the source is missing or doesn't parse, *nothing is
touched*. The composer autosaves mid-keystroke, so a half-written program is
a routine state, not an exceptional one. `driver_names_in` returns
`Option<HashSet<String>>` precisely so that "we couldn't establish the name
set" is a different value from "there are no drivers" — collapsing those two
would mass-orphan every annotation on the forecast the moment someone opened
an unclosed brace.

**Reversible.** Orphaning is a derived observation about the current
program, not a decision, so it is undone when the program is. A Spec 31
revert that restores a deleted driver restores its objections with it.
Without this, undo would be lossy in exactly the way the collaboration model
says it isn't. Only `orphaned` rows are revived — a human's
`accepted`/`declined` is a judgement and stays put.

## 5. The payoff: `contested_assumption` (detector 5)

Spec 27's `contested` detector *infers* disagreement from probabilities
moving in opposite directions. Real, but it can only ever tell you that two
people disagree, never about what. A challenge is the same disagreement made
explicit and anchored, which is the difference between *"reconcile this
forecast"* and *"settle whether the base rate for `elo_current` is right"*.

Only `kind = 'challenge'` becomes an op. A `note` explicitly implies no
action; a `question` is answered by talking rather than by deciding. Neither
is work that should be ranked against a broken cascade.

Orphaned annotations fall out for free, because the sweep moves them off
`open`. That is the point of §4: a board item you cannot act on because its
subject no longer exists would be worse than no board item.

**Urgency 50–79** — the same band as `contested`, deliberately. Neither
stated nor inferred disagreement is more urgent than the other in the
abstract; they are the same class of work and should interleave by age and
size, which only holds if the bands match. Within the band it climbs at 2/day,
so **a single unanswered objection reaches the band ceiling on age alone in a
fortnight**, without needing a pile-on to get attention. The failure mode
this detector exists to catch is not disagreement — it is an objection
nobody ever answered.

`done_when` is *"each open challenge is accepted or declined"*. Both
outcomes close it; the board must not read as pressure to agree.

## 6. Permissions

| action | requires | why |
|---|---|---|
| read | `view` | An objection visible only to editors would leave readers trusting a number the team is actively disputing. |
| **create** | **`view`** | See below. |
| resolve | `edit` | Accepting a challenge is a claim about what the forecast now says; declining one closes someone else's objection. |
| delete | **author only** | For genuine mistakes. |

**Creating is view-gated, and this is the one place the moderate permission
model bends toward the wiki.** A `view` grant exists so people can read *and
react to* a forecast; telling a reader "you may see this but not say it's
wrong" would defeat the point of publishing it. Annotating changes no
forecast state — it is the cheapest possible reversible act.

Delete is author-only because an editor deleting an objection against their
own work is the one way this feature could be used to hide disagreement.
Their route out is `declined`, which stays on the record. The list endpoint
echoes `me` so the client knows which rows are its own, rather than offering
Delete everywhere and letting the server 403 — buttons that lie teach
operators to distrust all of them.

## 7. Console — the Assumptions tab

A list of **every declared driver**, not just the annotated ones. Two
reasons: the uncontested drivers are the context that makes a contested one
meaningful, and this is the only place in the console that answers *"what
does this forecast actually assume?"* as a list. Contested rows are tinted
and badged `contested ×n` so the eye finds them while scanning.

The badge count comes from the server's `open_by_driver`, never derived
client-side. The ops detector counts the same thing, and two implementations
of "is this driver contested" would eventually disagree — at which point the
badge and the board would be telling the team different stories.

Answering is a two-step: *Accept — I changed it* and *Decline — considered,
keeping it* sit side by side, neither styled as primary. A single button
would have to pick one silently, and making Decline the quiet secondary
would put a thumb on the scale toward agreeing.

## 8. What this deliberately does not have

* **Threading / replies.** An annotation is a claim and an answer, not a
  conversation. Add it when someone needs it.
* **Notification fan-out** beyond the existing owner notification.
* **Annotations on evidence or agents.** Same anchor argument would apply,
  but drivers are where the disagreement is.
* **Editing an annotation.** Delete-and-repost, author-only. Editing a claim
  after someone has responded to it rewrites the record.

## 9. Validation

* `scripts/spec26_sql_check.sh` **PART C** — migration 183 applied twice
  (idempotency); the orphan reconcile asserted **reversible** (orphan, then
  revive on the driver's return) and asserted *not* to touch resolved rows;
  the attribution CHECK asserted to refuse an unattributable `accepted`; and
  detector 5 asserted to go quiet for all four reasons it should (resolved,
  orphaned, non-challenge kind, resolved forecast) and to fire with the
  driver named when it should.
* `cargo test --bin api-server annotations` — name extraction from a real
  production program, plus the two cases the fail-safe turns on.
* `cargo test --bin api-server ops::` — band ordering, including that stated
  and inferred disagreement share a band, and that one challenge escalates
  on age alone.
