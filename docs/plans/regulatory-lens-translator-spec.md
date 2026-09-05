# Demo Spec — Regulatory Lens Translator (Claims L10n Engine)

**Reframe from the earlier "EU vs. US" idea:** not a gotcha tool, not a fear-vs-innovation
frame. Same source truth (real composition, real ingredients) rendered compliantly across
multiple regulatory grammars, so the divergence itself becomes the educational content —
where regimes actually differ, and where the difference is stringency vs. just different
philosophy (e.g. positive-list vs. risk-based approaches), not "some countries care and
others don't."

**Explicit design constraint:** treat "Asian regulation" as several distinct regimes, not one
lens. China (SAMR/GB standards), Japan (MHLW/Consumer Affairs Agency), and South Korea (MFDS)
have meaningfully different frameworks from each other, not a shared "Asian" position — same
mistake as treating "EU" and "US" as the only two possible postures. Scoping this correctly
is itself part of the demystification argument you want to make.

---

## 1. Thesis
Same product, same actual composition, run through N regulatory lenses (EU, US, China,
optionally Japan/Korea) — the output for each is a legitimately compliant label for that
market, not a "translation of lies." The demo's payoff is showing *where* regimes diverge
(what claim language is allowed, what disclosure is mandatory, what's a positive-list
ingredient question vs. a claims-substantiation question) and making the shape of that
divergence legible rather than mysterious.

## 2. Data honesty note — read before building
I can sketch the *structure* of each regime (EU Reg 1924/2006 + 1169/2011 claims/allergen
framework; US FDA/FTC claims and labeling framework; China's GB food safety standards under
SAMR oversight) from general knowledge, but I do not have live search access this session and
should not be treated as current or complete on:
- current EFSA-authorized claim wording (flagged in the earlier spec too)
- current GB standard numbers/thresholds or SAMR claims-approval specifics
- current Japanese/Korean functional-claim system details (both have their own structured
  systems — Japan's FOSHU/functional-labeling system, Korea's health functional food system —
  each with real substantive differences from EU/US, not just translation targets)

Treat every regime's ruleset in this build as **synthetic-but-structurally-representative**,
the same caveat as the EFSA claims list in the earlier spec, and verify against primary
sources (each regulator's actual published standard) before any of this goes in front of a
partner or gets published. This matters more here than in the EU/US spec — getting a real
regulatory regime's substance wrong in a "demystifying" tool undercuts the entire premise.

## 3. Scope for the Two-Week Build
Given the verification load above, two lenses beyond home-market is the honest ceiling for
a 2-week synthetic build, not four:
- **Lens A: EU** (reuse the existing claims-register structure from the info-card spec)
- **Lens B: US** (FDA/FTC structure — structured claims substantiation, different allergen
  labeling requirements, different "natural" language conventions)
- **Lens C: China** (GB standard structure — different positive-list logic for ingredients
  and additives, different claims-approval pathway) — pick this over Japan/Korea for the
  first build specifically because it's the regime most likely to be treated as opaque by a
  Western audience, so it does the most demystifying work per lens built
- Japan/Korea explicitly parked as a "if this lands, here's the obvious next lens" line in
  the pitch, not built now — naming them shows the frame is genuinely multi-regime, not a
  three-lens tool pretending to be general

## 4. Synthetic Dataset
Same base SKU as the other two demos (Precision Kombucha, Hibiscus, Cold F2) — same
real-composition anchor, same "one product across the whole pitch" coherence.

**Per-lens synthetic ruleset structure** (not real regulatory text — structurally
representative only, per §2):
- Allowed/disallowed claim categories and required substantiation type
- Mandatory disclosure fields (allergens, additive labeling conventions) and how they differ
  in format, not just content — e.g. allergen labeling that must be bolded vs. listed
  separately vs. inline
- One ingredient-status divergence point: something treated as an approved/common ingredient
  in one regime and requiring additional approval or additional labeling in another (this is
  the single most educational beat — showing a real *category* of divergence, e.g.
  novel-food/positive-list questions, rather than just claim-wording differences)

## 5. The Demo Beat
Same shape as the trap-claim and wrong-then-corrected-forecast beats in the other two specs —
one scripted moment carries the demo:
- Run the same source composition through Lens A, B, C live
- Show one claim that's fine in one regime and gets rewritten/stripped in another — and
  narrate *why*, in one sentence, tied to the actual regulatory logic (not "they're stricter,"
  but e.g. "this regime requires claim-specific clinical substantiation, this one uses an
  approved-list model instead")
- Close on the ingredient-status divergence point (§4) — this is the moment that does the
  most demystifying work, because it shows a difference that isn't about strictness at all,
  just a different regulatory philosophy

## 6. Build Plan
- Days 1–4: build the three synthetic rulesets (structure representative, values flagged
  synthetic per §2) — this is the long pole, budget real time here
- Days 5–8: reuse the grounding/gate pipeline from the info-card build, extend to
  target-a-specific-lens generation instead of pass/fail
- Days 9–11: wire the three-lens side-by-side output and the ingredient-divergence beat
- Days 12–14: presentation layer (side-by-side label view), demo script, and — importantly —
  a visible "sources to verify before production use" appendix in the demo itself, so the
  honesty-as-credibility move from the BD whitepaper carries into this artifact too

## 7. Explicitly Out of Scope
- No claim that any generated label is currently compliant/filed — same "compatible with, not
  certified for" caution as the other two specs, more important here given three jurisdictions
- No Japan/Korea build in this pass (named as roadmap, not delivered)
- No implication that any regime "doesn't care" — the framing throughout should be regulatory
  *philosophy* difference (positive-list vs. risk-based, claim-specific-substantiation vs.
  approved-category), not a stringency ranking

## 8. Roadmap — Live Label-Mutation Renderer (future work, not in this build)
Interaction target for a later phase: point a camera at a physical label, extract source
facts (composition, claims as printed), run against a selected target ruleset, and render the
label's compliant mutation live, in place — the same interaction shape as a camera filter,
but not the same computation. A filter transforms pixels with no claim about ground truth;
this transforms a claim through a ruleset and either produces a valid rendering or surfaces
the gap where no valid rendering exists. Worth stating that distinction explicitly wherever
this is pitched, for the same reason as the cultural-translator caveat in the framing
document — the resemblance is in the UX, not the underlying operation.

This is the concrete form of the AR roadmap line from the info-card spec: not "AR overlay" in
the abstract, but specifically a live label-mutation renderer — same grounding pipeline,
camera input instead of static scan or upload, mutated label instead of a generated info
card. Depends on the same source/ruleset/gate architecture already specified in §1–7; the
only new component is the live camera capture and real-time re-render loop. Not scoped for
the two-week build — the two-week build should produce the static side-by-side demo (§5) that
proves the underlying mechanism works before any live-camera interaction layer is attempted.

## 9. How This Sits Next to the Other Two
Three demos, one SKU, one engine, three directions: generate-safely (info-card), 
forecast-and-improve (cold-chain), and translate-across-regimes (this one). The pitch line
that ties all three: the same grounding architecture that stops a fabricated claim also
produces a legitimately different, legitimately compliant claim depending on where you're
standing — trust isn't one fixed output, it's the same honesty rendered correctly per context.
