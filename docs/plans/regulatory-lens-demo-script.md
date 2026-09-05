# Demo Script — Regulatory Lens Translator
## Precision Kombucha, Hibiscus, Cold F2 · EU / US / China

**Total runtime:** ~6 min (tight), ~8 min (with questions taken mid-demo)  
**Format:** live browser demo from `static/adaptogen-lab/index.html`  
**Pre-load:** page open, demo data loaded, table visible — do not start on a blank page  
**Audience:** BD meeting, regulatory/compliance professional, or technical partner  
**Tone:** demystification, not alarm. The divergences are interesting, not dangerous.

---

## Before you walk in

Check that:
- [ ] The demo tab is open at `static/adaptogen-lab/index.html` (demo data auto-loads)
- [ ] The page is scrolled to the top, comparison table fully visible
- [ ] Browser zoom is at 90% — the full table should be readable without scrolling sideways
- [ ] API inspector is collapsed (the `▶ Show raw API response` row should be closed)
- [ ] If live workspace: token and workspace ID are already entered; click "Load demo data" to reset to the clean pre-baked state

---

## The setup (~45 sec)

**[SCREEN]** Header: "Regulatory Lens Translator — Precision Kombucha, Hibiscus, Cold F2"

**[SAY]**

> "I'm going to show you one product — one ingredient list, one composition, one source of truth — run through three regulatory grammars at once.
>
> EU, US, China.
>
> The product doesn't change. The composition doesn't change. What changes is what you're allowed to say about it, and *how* the rules work underneath.
>
> The output for each market is a legitimately compliant label for that market. Not a translation of the source. Not a softened version. The actual rendering for that regulatory context.
>
> Most brands either hire four regulatory consultants, one per market, or skip the markets entirely. We're going to show you why that's more expensive than it needs to be — and more importantly, why the divergences themselves aren't mysterious once you can see them."

**[NOTE]** Don't rush this. The framing is the whole pitch. "Demystification" not "compliance automation."

---

## Beat 1 — No divergence (~20 sec)

**[SCREEN]** Point to the `fermentation_process` row in the comparison table — all three markets show a green "Allowed" badge.

**[SAY]**

> "Let's start with the easy one. 'Naturally fermented using traditional kombucha culture.' Process descriptor. Factual. All three markets: allowed, no rewriting required. 
>
> This is what zero divergence looks like. Divergence score zero. Good baseline."

**[NOTE]** Don't linger here. This row exists to set up the contrast.

---

## Beat 2 — The outlier you didn't expect (~35 sec)

**[SCREEN]** Point to the `low_sugar` row — EU: green "Conditionally allowed", CN: green "Conditionally allowed", US: blue "Rewritten".

**[SAY]**

> "'Low in sugar.' Two markets say yes, one says no. Guess which one.
>
> EU and China both define 'low sugar' for liquid foods: five grams per hundred milliliters. This product is at 2.67. Both pass.
>
> The US doesn't have a defined 'low sugar' nutrient content claim for sugars. FDA has definitions for 'low fat,' 'low sodium,' 'low calorie' — but not 'low sugar.' So the safe rendering for US is just the factual declaration: '8.8 grams sugar per serving.'
>
> US is the outlier here. Not because they care less about sugar. Just because the regulatory vocabulary doesn't have that term yet. EU and China align. That's an interesting shape."

**[NOTE]** This beat matters because it breaks the assumption that "US = permissive, EU = strict." The framing throughout is regulatory philosophy, not stringency ranking.

---

## Beat 3 — The primary demo beat (~2 min)

**[SCREEN]** Scroll down to the **★ Primary demo beat** callout section. The three market cards for `hibiscus_wellness` are visible: EU (red "Not allowed"), US (teal "Allowed (caution)"), CN (teal "Allowed (note)").

**[ACTION]** Point to each card in turn as you name it.

**[SAY]**

> "Here's the moment. 'Hibiscus — traditionally used for wellbeing.' Same claim. Three markets.

**[Point to EU card — red]**

> "EU: stripped. Doesn't appear on the label at all.
>
> Why? The EU uses what's called a positive-list model for health claims. A claim is only allowed if it appears on the EFSA authorized list. Hibiscus isn't on the list. And here's the part that surprises most brands: traditional use doesn't count. You can't say 'this is how it's been used for centuries' and have that stand in for clinical authorization. The EFSA system requires the claim to have been submitted, evaluated, and authorized — full stop.

**[Point to US card — teal]**

> "US: allowed, with care. 'Hibiscus — a traditional wellness botanical' is defensible as contextual framing — it describes the ingredient's cultural history, not a specific body function. The US uses a risk-based substantiation model. You can make a claim if you can substantiate it, without getting pre-approval. But the FTC applies its standard to any implied benefit, so the wording matters.

**[Point to CN card — teal]**

> "China: we get the ingredient on the label, in Chinese — 玫瑰茄, Hibiscus sabdariffa — but no wellness framing at all. Here's the interesting part.
>
> Hibiscus is on something called the food-medicine homologous list in China. It's approved for use as both a food ingredient and a traditional Chinese medicine. That sounds like it would *help* the claim — it doesn't. The homologous status is an ingredient authorization, not a claims authorization. Citing its traditional medicinal properties on a food label crosses into the health food registration track, which is a multi-year process. So you get the ingredient, not the story.

**[Pause. Let this land.]**

> "Same claim. EU uses a pre-authorized list. US uses after-the-fact substantiation. China routes through a categorical system where the claim type determines the product registration track. Three completely different regulatory architectures. Not three different levels of strictness — three different *kinds* of rules."

**[NOTE]** This is the money moment. The one-sentence-per-market WHY is what makes the demo educational rather than just informational. Say each one clearly.

---

## Beat 4 — The probiotic beat (~50 sec)

**[SCREEN]** Scroll back up to the comparison table. Point to the `live_cultures_present` row — all three markets show green badges, but hover over any cell to reveal the divergence note.

**[SAY]**

> "This one is counterintuitive. 'Contains live cultures.' All three markets: allowed. But what allowed means here is completely different.
>
> EU: the word 'probiotic' is prohibited as a label term. EFSA evaluated every probiotic health claim submitted to them between 2008 and 2011 — hundreds of them — and rejected all of them. Insufficient specificity. So you can say what organisms are in the product. You cannot call them probiotics.
>
> US: 'probiotic' is usable as a conventional food descriptor. No pre-authorization needed. 'Contains live and active cultures' is standard industry language.
>
> China: and this is the important one. In China, the word 益生菌 — probiotic — is fine as a descriptor. You can say what's in the product. But the moment you link those organisms to a health function — improve gut health, support immunity, anything like that — the *entire product* gets reclassified into the health food registration track. Not just the claim gets refused. The product.

> "So EU prohibits the word. US allows the word. China allows the word but the minute you use it with a function, the regulatory category of the whole product changes. Three completely different constraint mechanisms. Surface outcome: all three markets allow the claim. Underneath: nothing in common."

**[NOTE]** This beat is for technical and regulatory audiences. If the room is more BD-oriented, compress to 20 seconds.

---

## Beat 5 — The ingredient-status close (~50 sec)

**[SCREEN]** Scroll to the **Ingredient-status divergence** section — three cards: EU "Conventional food ingredient", US "GRAS", CN "药食同源 — Food-medicine homologous". Plus the fourth card "⟷ The point."

**[SAY]**

> "Same ingredient. Hibiscus. Three markets. All three: approved for use.
>
> US: GRAS — generally recognized as safe. No pre-market authorization needed.
>
> EU: conventional food ingredient with pre-1997 use in the EU. Not a novel food.
>
> China: 药食同源 — food-medicine homologous. Approved for use as both food and traditional medicine.
>
> Three different regulatory pathways to the same outcome. And here's why that matters beyond the label:
>
> If you're filing documentation to justify your ingredient in a new market, citing 'GRAS' doesn't help your China submission. Citing 药食同源 doesn't help your US or EU one. The underlying approval is the same thing — this ingredient is fine — but the mechanism is jurisdiction-specific, and the documentation has to match the mechanism.
>
> That's what we mean when we say the divergence is about regulatory philosophy, not stringency. It's not that one market cares more. It's that they built different systems."

---

## The API moment (~30 sec)

**[ACTION]** Click the `▶ Show raw API response` row to expand the API inspector.

**[SCREEN]** The formatted JSON appears, showing `comparison_table`, `primary_demo_beat`, `ingredient_divergence_beat`, `verification_appendix`.

**[SAY]**

> "One API call. Everything you've seen on this page — the comparison table, the demo beat callout, the ingredient cards, the verification sources — came from this JSON object.
>
> If you're building a compliance dashboard, a product launch tool, a partner portal — the integration is one `POST` request and however you want to render the result. The embed example in the next tab is 120 lines of HTML with zero dependencies."

**[NOTE]** For technical audiences: offer to pull up `embed-example.html`. For BD audiences: one sentence and move on.

---

## The honesty close (~35 sec)

**[SCREEN]** Scroll to the **Verification appendix** section — visible list of primary sources (EFSA register, 21 CFR, SAMR portal).

**[SAY]**

> "Before I close — this section.
>
> The rulesets underlying this demo are structurally representative of each regime's regulatory logic. The architecture is accurate. The specific claim wording, current threshold values, and ingredient approval status need to be verified against primary sources before any of this goes on a physical label. Those sources are listed here, as links.
>
> We put this in the output as a first-class field, not a footer disclaimer. Because the argument for trusting this tool is that it tells you what it doesn't know.
>
> The same grounding architecture that prevents this system from fabricating a claim about your product also produces a legitimately different, legitimately compliant rendering depending on where you're standing. Trust isn't one fixed output. It's the same honesty rendered correctly per context."

**[NOTE]** This is the line. Say it slowly.

---

## Close (~20 sec)

**[SAY]**

> "That's EU, US, and China in six minutes. Japan and Korea are the obvious next lenses — they have their own structurally distinct systems, FOSHU and the MFDS health functional food framework, neither of which maps to the EU or US model. We named them in the roadmap rather than building them now because the right move with three is to prove the mechanism works before adding more.
>
> Questions?"

---

## Anticipated questions

**"Is this legal advice?"**
> "No. This is a screening and orientation tool. The output says exactly what it is: structurally representative, verify before commercial use. It replaces the 'I have no idea where to start' phase, not the regulatory consultant."

**"How current is this?"**
> "The structural logic of each regime is stable over years. The specific authorized claim wording and threshold values change — that's why the verification appendix exists and why the primary sources are linked. The tool is designed to be updated when those change."

**"Can you add [jurisdiction]?"**
> "Yes. Japan and Korea are next. The architecture supports N lenses. Adding one is writing a ruleset YAML and an output contract sketch — the validation pipeline runs automatically. We scoped three for this build to prove the mechanism before claiming generality."

**"What's the data provenance?"**
> "The structural logic of each regulatory regime — how the claims authorization system works, what the allergen labeling format requires, how ingredients are categorized — that's grounded in the actual regulatory framework. Specific values are flagged synthetic. The demo shows you the shape of what's different, not certified compliance output."

**"How does a third party integrate this?"**
> "One POST request to the compare_lenses endpoint. The response is structured JSON — comparison_table, primary_demo_beat, verification_appendix — designed to be rendered directly. The embed example tab shows the full integration in 120 lines."

**"What are the other two demos?"**
> "Same product, same engine, two other directions. The info-card generator shows the same grounding architecture stopping a fabricated claim — generate safely. The cold-chain forecast shows the same product with a predictive model for temperature excursion risk — forecast and improve. All three share one SKU so the pitch is coherent: one product, one engine, three problems it solves."

---

## Things not to say

| Don't say | Say instead |
|---|---|
| "EU is stricter" | "EU uses a positive-list model — the claim must be on the authorized list" |
| "China doesn't care about allergens" | "CN allergen labeling is recommended, not mandated — a different regulatory instrument, not less concern" |
| "This is AI-generated" | "This is structurally derived from the regulatory framework — the engine applies the rules, it doesn't guess them" |
| "We'll keep it updated automatically" | "The structural logic is stable; specific values need verification against primary sources when they change" |
| "Asian regulation" (as one block) | "China, Japan, and Korea each have distinct systems — China's two-track model is completely different from Japan's FOSHU structure" |
| "This certifies your label" | "This orientates your label — verify with primary sources before filing" |

---

## Timing summary

| Section | Time |
|---|---|
| Setup | ~45 sec |
| Beat 1 — fermentation (no divergence) | ~20 sec |
| Beat 2 — low sugar (outlier) | ~35 sec |
| Beat 3 — hibiscus wellness (primary beat) | ~2 min |
| Beat 4 — probiotic (mechanism divergence) | ~50 sec |
| Beat 5 — ingredient status (close) | ~50 sec |
| API moment | ~30 sec |
| Honesty close | ~35 sec |
| Close line | ~20 sec |
| **Total** | **~6 min 45 sec** |

Questions typically add 3–5 minutes. Budget 10–12 minutes total for the demo segment of a meeting.
