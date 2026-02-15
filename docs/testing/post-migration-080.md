# Post-Migration Testing — 079/080 (creature_conditions)

Deploy commit: `d4cfe03`  
Date: 2026-02-15  
Tester: ___

## Creature Reads
- [ ] `GET /api/creatures` — list returns visibility, presence
- [ ] `GET /api/creatures/:id` — detail returns sosa_opt_in, visibility, presence, creature_state

## State Transitions
- [ ] **Fly** — tap Fly on owned creature, flight starts (not 500)
- [ ] **Perch** — end a flight, creature returns to perch_solo
- [ ] **Join Rabble** — join a swarm, creature enters perch_rabble
- [ ] **Leave Rabble** — leave swarm, back to perch_solo

## Presence
- [ ] **Sleep** — set creature sleeping, Fly/Join return 409
- [ ] **Wake** — set creature active, Fly works again
- [ ] **Tether** — tether creature, presence becomes "tracking"
- [ ] **Untether** — untether, presence back to "active"

## Conditions
- [ ] **Visibility** — change visibility (public/private), reflected in GET
- [ ] **SOSA opt-in** — toggle, reflected in GET
- [ ] **Walk-in price** — set on rabble, visible in swarm list

## Species Integrity
- [ ] Beetle shows BEETLE badge (bronze), not BUTTERFLY
- [ ] Locust shows LOCUST badge, not BUTTERFLY
- [ ] Art style: ukiyo-e visibly different from naturalist
- [ ] Art style: field-guide white background specimen

## Admin
- [ ] Admin creatures page loads

## Chat
- [ ] Rabble chat works for active creature
- [ ] Sleeping creature blocked from chat (409)

---

## Notes

<!-- Add notes, screenshots, or errors here -->

