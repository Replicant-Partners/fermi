# Outreach: grounded vision APIs

**Status:** draft, not sent. Blocked on two inputs — see §5.
**Companion artifact:** `docs/specs/GROUNDED_VISION_ONE_PAGER.md` (the thing to attach).

This document exists because the strategy kept being reconstructed from conversation
memory instead of read from the repo. A plan that lives only in prose gets re-planned every
time the thread resets. This is the plan.

---

## 1. Who, and why them

Kindwise (Brno, CZ; formerly FlowerChecker s.r.o.). Products: Plant.id, Crop.health,
Insect.id, and a mushroom identification endpoint.

Why this target rather than a hyperscaler vision API:

- Research-oriented and small enough that an architecture argument reaches an engineer
  rather than a procurement form.
- Their domain has the sharpest version of the defect: a fabricated trait in a fungal ID is
  not a bad recommendation, it is a hospital admission. Hodgson et al. measured it.
- We are a plausible *consumer* of the thing we are asking for, which makes this a request
  from a would-be integrator rather than unsolicited advice.

## 2. The technical error we are not shipping

The first draft of this outreach (AI-generated, from a stale version of the paper) contained
this line:

> `Sourced`: Features explicitly isolated via visual coordinates/bounding boxes.

**This is wrong and it is wrong in exactly the way the paper is about.** A bounding box
tells you *where the model looked*. It says nothing about whether the conclusion is true.
Saliency is not evidence. Under our own ladder, a visual trait extracted from pixels is
`inferred` regardless of how precisely it is localised — localisation buys explainability,
not grounding.

Sending that would have proposed, to a company whose engineers would notice, a scheme that
launders model output into `sourced` by attaching coordinates to it. That is the fabrication
defect with a rectangle drawn around it.

Recorded here because it is the most instructive mistake in this workstream: the failure was
not sloppiness, it was a *plausible-looking* mapping. Which is the paper's thesis.

## 3. Also rejected, and why

| Rejected | Why |
|---|---|
| "Guaranteed Safe Bio-AI" | Enterprise counsel reads *guaranteed* as a warranty. This phrase increases their liability while claiming to reduce it. It is also a claim we cannot support: nothing here has been measured against a real image. |
| "I have been analysing your payload structures... it functions as text retrieval" | An assertion about someone else's internals, made from outside, in spec-shaped language. Precisely the defect class. Replaced with a question. |
| "Unlocks Enterprise Tier Clients / Redefines the AI SLA / First-Mover Paradigm" | Telling a company its own business, in a register neither of us uses. Their commercial strategy is theirs; our contribution is one observation from the integrator's side of the fence. |
| Four-rung ladder | The paper has **five**: Presence, Liveness, Truth, Grounding, Binding. The draft was written against a stale copy. Liveness — does the writer ever run — was the cheapest rung and the last one we added; dropping it from the summary drops the most transferable finding. |
| "Implement my ladder natively in your endpoint ecosystem" | Too large an ask for a first contact, and the wrong one. Four of five rungs are about *their* internal state and are none of our business. Only Grounding is a payload question, and within Grounding only three fields matter. |
| A 15-minute call with the CTO as the ask | The ask is an answer to three questions. Answering them costs an engineer ten minutes and requires no meeting. A call can follow if the answers are interesting. |

## 4. The ask, in order of value to them

Restricted to things **only they can supply** — i.e. things that cannot be computed from a
response after the fact.

1. **`not_visible`** — diagnostic features absent or occluded in the input. A fact about the
   image, checkable, and the one signal no downstream caller can reconstruct. Maps to
   `unsourced` in our vocabulary.
2. **Separate retrieval from inference** — their trait database is a genuine lookup; their
   classifier is a judgement. Two arrays, not one. This is design rule §5.5, and it is the
   rule that makes the contract usable rather than punitive.
3. **Calibration, not confidence** — score band, measured hit rate, `n`, interval.

Ordering is deliberate: (1) is the cheapest for them and the most valuable to us, (3) is the
most expensive and the most valuable to the market.

## 5. Blocked on

- **Signature block.** Name, title/organisation, and a link to the paper. Is the paper
  public, or repo-only? If repo-only, the email needs either a public URL or an attachment,
  and the one-pager becomes the primary artifact.
- **Optional: their actual payload.** The email is deliberately written so this is *not*
  required — it asks rather than asserts. But if `docs.kindwise.com` is read first and
  field (1) turns out to already exist under another name, question 1 should be reworded
  to reference it. Do not invent field names in this email under any circumstances.

## 6. The draft

Subject: **A question about what your mushroom endpoint can say about the image**

> Hello,
>
> I build verification infrastructure for AI agent systems — specifically, checks that catch
> output which is schema-valid, cleanly parsed, and fabricated. I have a paper on it and a
> working implementation; both linked below.
>
> One domain I have been using as a test case is fungal identification, because the failure
> mode there is unusually legible. Hodgson et al. (Clin Toxicol 2023, PMID 36794335) put 78
> expert-confirmed specimens through three consumer mushroom apps: the best scored 49%
> overall, and *Amanita phalloides* was falsely identified by two of the three. That paper is
> the reason my own foraging path is structurally incapable of emitting an edibility verdict.
>
> I would like to ask you three things about your mushroom endpoint. I am asking rather than
> asserting, because I can only see it from outside and I would rather not guess at your
> payload.
>
> **1. Does the response express anything about what the image could not establish?**
> Concretely: a field indicating that a diagnostic feature was absent or occluded — no stipe
> base in frame, gills not visible, no spore print. This is the one signal I cannot compute
> downstream at any price. Absence of evidence is invisible in a ranked candidate list, since
> every candidate is present with a score; and it is not the same thing as low confidence.
> Low confidence says *maybe*. Missing-diagnostic says *photograph the base*.
>
> **2. Do returned traits distinguish per-specimen observation from species-level reference
> text?** A trait read off this image and a trait retrieved from a species record are both
> true statements, and they carry very different weight when someone is deciding whether to
> eat something. Arriving in one array, a caller cannot tell them apart; arriving in two, a
> caller can render them differently.
>
> **3. Is there published calibration for the confidence score, as distinct from the score
> itself?** A measured hit rate within a score band, with `n` and an interval. A probability
> describes the model's internal ordering. A calibration curve describes what the number
> means.
>
> If (1) already exists under a name I have missed, I would be glad to be corrected — that
> would be the most useful of the three answers.
>
> Why I think this is commercial and not only architectural, from the integrator's side of
> the fence: in safety-adjacent domains, developers do not decline vision APIs because
> accuracy is too low. They decline because there is no defensible way to write down what the
> API did and did not establish, which leaves them holding all of the risk and none of the
> evidence. An explicit not-established signal does not reduce exposure by promising more. It
> reduces it by making the boundary of the claim machine-readable, so that the integrator's
> caveat is derived from your output rather than pasted in by hand.
>
> On where I actually am: the verification architecture is built, tested and running in CI.
> The foraging application on top of it is pre-MVP, has no users, and has no accuracy figure
> of its own — which is precisely why it claims none. I am not asking you to adopt anything.
> I am asking whether these three signals exist, and if they do not, whether they are
> interesting enough to talk about.
>
> There is a one-page architecture summary I am happy to send, or I am equally happy to just
> answer questions.
>
> Regards,
> [name]
> [title / organisation]
> [paper]

## 7. If they engage

Lead with **design rule §5.5, distinguish retrieval from judgement** — it is the rule that
makes the contract adoptable, because it is the one that stops the checker from flagging
everything. A checker that flags everything is indistinguishable from a broken one, and the
first thing a vendor will (correctly) fear about a grounding contract is that it condemns
their entire product.

Do **not** open with the ladder. Five rungs is our internal architecture; four of them are
about state they own and we cannot see. The payload conversation is Grounding only.

Second thing to say, if there is appetite: `not_visible` is not a new model capability. It
is a *reporting* capability over information their pipeline already has — a detector that
found no stipe already knows it found no stipe. The expensive part is deciding the
vocabulary, not computing the value.

Concession available if they balk at (1) as a public field: emit it as telemetry on an
opt-in header first. It answers the question for integrators who want it without changing
the default contract for everyone.

## 8. What this must not become

An endorsement. If they ship all three signals, our position on their accuracy is
unchanged, because the three signals are about *legibility*, not correctness. A perfectly
legible 49% is still 49%. The corpus in `docs/specs/VERIFICATION_CORPUS.md` is how accuracy
gets established, and it does not care who the vendor is.
