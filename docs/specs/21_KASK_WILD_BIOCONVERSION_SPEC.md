# kask-app-wild: Wild Bioconversion Intelligence
## Design Specification v0.1 — June 2026

*Status: Foundation built. Flutter layer and image support pending design decisions.*

---

## The Thesis

Foraging expertise is embodied knowledge — it lives in the body of a person who has walked the same forest floor for twenty years. It accumulates through repeated physical encounters with the world. It cannot be transferred via a book, a database, or a language model that has never stood in the rain watching mycelium networks fruit.

ABW is the first platform that can encode this kind of expertise because it has:
- Spatial episodic memory (H3 + SOSA + episodes)
- Knowledge graph accumulation via dream cycles
- Probabilistic forecasting with Brier scoring feedback
- Credit economy for expertise monetisation

kask-app-wild is the demonstration of this thesis applied to wild foraging and bioconversion.

---

## The Bioconversion Chain

The full value chain from wild observation to table:

```
FIELD                    FOREST                    KITCHEN                   TABLE
─────                    ──────                    ───────                   ─────
Location scout      →    Species identification →  Processing pathway   →    Recipe
Condition forecast       Edibility assessment      Preservation method       Flavour pairing
Observation log          Harvest timing            Fermentation tracking     Menu planning
Photo verification       Look-alike check          Yield estimation          Taste development
```

Each stage has distinct intelligence requirements. Each stage accumulates knowledge that improves the next run.

**Stage 1 — Field (Rabble creature):**
The creature provides spatial context. It knows where it is, what it has seen before at this location, what conditions have historically preceded good finds. The creature is the forager's persistent field companion.

**Stage 2 — Forest (kask-app-wild):**
The wild workspace provides foraging intelligence. It knows what's been observed nearby (iNaturalist), what the microclimate predicts (OpenWeather), what the species actually is (MycoBank/GBIF), whether it's safe to harvest (harvest_advisor), and when to pick it.

**Stage 3 — Kitchen (kask-app-wild, gaps to fill):**
Post-harvest intelligence. Processing pathways, preservation methods, fermentation tracking, flavour development over time. The Redzepi layer — where place, season, and technique meet. This lives in the kask-wild workspace — same app, additional agents to be hired in. Current gaps: `recipe_composer`, `fermentation_tracker`, `preservation_log` agents not yet built.

**Stage 4 — Table (kask-app-wild, future agents):**
Recipe composition, menu planning, flavour pairing. Also kask-wild workspace. Connects to restaurant supply chain, seasonal menus, wild ingredient sourcing intelligence via the same workspace the forager already uses.

*Stages 1-2 are built. Stages 3-4 share the kask-wild workspace — agent roster gaps to be filled.*

---

## The Redzepi Reference

René Redzepi's foraging philosophy (Noma, Copenhagen) is the culinary reference point:

1. **Terroir is everything** — the same species from different locations and conditions has measurably different flavour. A chanterelle from chalk soil after drought ≠ one from loamy oak woodland after rain.

2. **What grows together goes together** — ecological co-occurrence is a pairing heuristic. The mycorrhizal oak that hosts the porcini also flavours the lamb grazing nearby.

3. **Processing is transformation** — drying concentrates umami 10-20x. Lacto-fermentation creates entirely new flavour compounds. The harvest is not the end point; the transformation is.

4. **Seasonality is absolute** — not as aesthetic preference but as biological fact. The window for most wild ingredients is days, not weeks.

The flavor_profiler agent encodes this thinking dimensionally:
- Umami intensity (glutamate content)
- Earthiness (geosmin, petrichor compounds)
- Aroma profile (key volatile compounds: 1-octen-3-ol, terpenes)
- Terroir sensitivity (how much location/condition affects flavour)
- Processing transformations (what each method does to the flavour)

---

## Architecture

### Application split

```
ABW (backend)
├── kask-app-wild     ← this spec — full bioconversion chain
│   ├── Stage 1: Field intelligence (creature bridge)
│   ├── Stage 2: Forest intelligence (forage_scout, condition_forecaster)
│   ├── Stage 3: Kitchen intelligence (harvest_advisor, flavor_profiler + gaps)
│   └── Stage 4: Table intelligence (recipe_composer + future agents)
└── rabble            ← consumer surface
    └── creature consumes kask-wild via cross-workspace delegation
```

kask-wild is the full chain. No separate kask-app-table. Stages 3-4 extend the same workspace with additional agents as the product matures.

### Agent roster (kask-wild, v1)

| Agent | Role | Key tools |
|-------|------|-----------|
| `wild_companion` | Primary interface, orchestrator | execute_agent, inat_observations, mycobank_lookup, openweather_forecast, log_observation |
| `forage_scout` | Field intelligence, species likely | inat_observations, openweather_forecast, mycobank_lookup, gbif_species_search |
| `condition_forecaster` | Microclimate probability, Brier-scored | openweather_forecast, inat_observations |
| `harvest_advisor` | Maturity, safety, processing pathway | mycobank_lookup, gbif_species_search |
| `flavor_profiler` | Taste dimensions, terroir, pairing | gbif_species_search |
| `wild_narrator` | Dream cycle narration | (none — synthesis only) |

### Data model

**`creature_goals`** — standing objectives with goal_type, parameters, Brier scoring fields, link to wild_workspace_id

**`forage_observations`** — structured find records:
- Species (name, accepted_name, MycoBank number, GBIF key)
- Location (H3 cell, lat/lng, location_name)
- Habitat and substrate
- Microclimate conditions JSONB
- Harvest and processing fields
- Flavor profile JSONB (taste dimensions, terroir notes, pairing notes)
- Opted-in shared flag (for collective model)
- Links to SOSA observation and iNaturalist observation

### Cross-workspace delegation (the seam)

Rabble creature → kask-wild:

```
creature.forage(scout, lat, lng)
  → dispatch forage_scout (creature workspace, has creature KG)
  → if creature.goals[0].wild_workspace_id exists:
      execute_agent("wild_companion",
                    query,
                    workspace_id=wild_workspace_id)
      → wild_companion runs inside kask-wild workspace
        (has iNaturalist, MycoBank, OpenWeather, workspace git)
      → returns structured forage intelligence
  → result stored as episode → dream cycle → creature KG grows
```

The creature's KG accumulates location-specific foraging knowledge. The wild workspace's KG accumulates species/condition/season knowledge. Both feed into future runs via `enrich_with_kg_context`.

### Goal types

| Type | Description | Brier-scored |
|------|-------------|-------------|
| `species_watch` | Alert when target species found nearby | No |
| `accumulation` | Collect N species / observations | No |
| `location_scout` | Build deep knowledge of a specific place | No |
| `condition_track` | Track microclimate → fruiting correlations | Yes |
| `bioconversion` | Full field → table chain | Partial |
| `custom` | Freeform, evaluated by goal_tracker | No |

---

## The Learning Loop

This is what makes kask-wild different from iNaturalist or any existing foraging app:

```
Foray 1: Scout location (iNaturalist: 12 fungi spp observed nearby)
         → Conditions: 18mm rain 5 days ago, 16°C, oak woodland
         → Find: Cantharellus cibarius, moderate quantity
         → Log observation (episode stored)

Foray 3: Same location, similar conditions
         → condition_forecaster: 72% chanterelle probability (calibrated from 2 prior finds)
         → Find: abundant chanterelles
         → Brier score updates: model was right, confidence increases

Dream cycle after Foray 5:
         → Consolidation extracts rule:
           "IF location=oak_woodland_north AND rainfall_5d > 15mm AND temp 14-18°C
            THEN Cantharellus cibarius HIGH probability"
         → Rule enters KG
         → Future forage_scout runs receive this knowledge via enrich_with_kg_context
         → Prediction accuracy improves without any manual intervention
```

After 3 seasons: the creature demonstrably forecasts better than any generic foraging app because it has accumulated scored, spatially-grounded observations specific to the owner's locations and habits.

---

## The Social / Opt-in Layer

Every observation has `opted_in_shared: bool`. When true:
- The observation feeds a shared regional model (aggregate iNaturalist-style)
- Other foragers in the same region get better predictions
- The contributor gets back improved collective signal

This is the network effect: individual foragers contribute private knowledge, receive collective signal in return. The exchange is explicit and voluntary.

**Privacy model:**
- Default: observations are private, personal KG only
- Opt-in: H3 cell + species + conditions shared (no personal identity)
- Location precision: H3 resolution 9 (≈ 174m²) — sufficient for ecological signal, insufficient for exact spot identification

---

## Open Design Questions

### Image support (BLOCKING for field utility)

Field identification without image support is incomplete. "Is this a chanterelle or a false chanterelle?" is fundamentally a visual question.

**What's needed:**
- Upload path: camera capture → object storage → URL
- Vision agent call: Claude Sonnet vision API accepts image URLs natively
- New agent or extended harvest_advisor: `specimen_identifier` takes image + location context → returns species assessment + confidence + look-alike warnings

**Decision: workspace git.**
Images commit to the workspace git alongside `wild/observations.yaml`. The raw URL becomes the image reference in `forage_observations.photo_urls` and is passed to vision-capable agents. No extra infrastructure — the workspace git is already wired everywhere.

### Post-processing / kitchen stage

**Decision: kask-wild covers the full chain. No separate kask-app-table.**

Stages 3-4 extend the same workspace with additional agents. Agents to build:
- `recipe_composer` — species + quantities + conditions → preparation proposals
- `fermentation_tracker` — batch log (pH, time, temp, tasting notes at intervals)
- `preservation_log` — method, date, expected shelf life, batch reference
- `flavour_journal` — how a preparation develops over time

Workspace git stores the living record alongside observations:
`wild/observations.yaml`, `wild/ferments/`, `wild/recipes/`, `wild/preservation/`

Images (git-based, see above) feed into these agents as visual documentation — fermentation progress photos, plating references, specimen identification.

### Recipe and menu planning

Also kask-wild workspace. `recipe_composer` connects field intelligence to table:
- What's available this week → what can we cook
- Substitution when a target species is unavailable
- Restaurant supply chain: volume estimates, advance planning

Initially a forager-facing tool. B2B (restaurant subscription) is a later layer on the same foundation.

*Not blocking v1.*

---

## What's Built (June 2026)

| Component | Status |
|-----------|--------|
| App manifest (apps/kask_wild.json) | ✅ Done |
| Migrations (creature_goals, forage_observations) | ✅ Done |
| inat_observations tool | ✅ Done |
| mycobank_lookup tool | ✅ Done (GBIF fallback) |
| openweather_forecast tool | ✅ Done (key set in Railway) |
| forage_scout agent | ✅ Done |
| condition_forecaster agent | ✅ Done |
| harvest_advisor agent | ✅ Done |
| flavor_profiler agent | ✅ Done |
| wild_narrator agent | ✅ Done |
| wild_companion agent | ✅ Done |
| Cross-workspace delegation | ✅ Done |
| log_observation action handler | ✅ Done |
| Creature goals API | ✅ Done |
| forage_module (creature bridge) | ✅ Done |
| Flutter: forage pill | ⏸ Pending design decisions |
| Flutter: goal creation UI | ⏸ Pending design decisions |
| Flutter: observation log UI | ⏸ Pending design decisions |
| Image support (specimen identification) | ⏸ Pending: upload target decision |
| Post-processing / kitchen stage | ⏸ Pending: recipe_composer, fermentation_tracker agents |
| Recipe / menu planning | ⏸ Future |
| Shared regional model | ⏸ Future |
| recipe_composer agent | ⏸ Future (kask-wild workspace) |
| fermentation_tracker agent | ⏸ Future (kask-wild workspace) |
| preservation_log agent | ⏸ Future (kask-wild workspace) |

---

## Next Steps When Design Resumes

1. **Decide image upload target** — this is the blocking question for field utility
2. **Decide kitchen stage scope** — extend kask-wild or create kask-table
3. **Build Flutter forage pill** — scout + log, creature image overlay
4. **Build Flutter goal creation** — text input + goal_type + link to wild workspace
5. **Build specimen_identifier agent** — vision-capable, takes image URL
6. **Wire Brier scoring loop** — condition_forecaster predictions scored against actual finds

---

*This document is the authoritative design reference for kask-app-wild.*
*The implementation is in: `apps/kask_wild.json`, `migrations/131_*`, `migrations/132_*`, `agents/curated/forage_scout/`, `agents/curated/wild_companion/`, et al.*
*See also: `docs/INVESTMENT_MEMO.md` for the business case, `docs/FORECASTING_INVESTMENT_MEMO.md` for the Tetlock/Fermi framing.*
