# Rabble UX Test Checklist v2

Clean-slate integration test cases for the Rabble creature lifecycle.
Updated after Sprint Z bug fixes (Feb 14 session 2).

## Pre-flight: Service Health

- [ ] P1. https://agent-bestiary.world/ loads (landing page)
- [ ] P2. https://rabble.world/ loads (Flutter SPA)
- [ ] P3. Sign in works (Google or GitHub)

## A. Onboarding

- [ ] A1. Open app fresh (no creatures) — onboarding flow appears
- [ ] A2. Search for a butterfly species (e.g. "Vanessa atalanta")
- [ ] A3. Search for a beetle species (e.g. "Scarabaeus") — verify BEETLE tag, not BUTTERFLY
- [ ] A4. Select art style (try ukiyo-e) — verify preview hint text appears
- [ ] A5. Mint creature — wait for image generation, verify image renders

## B. Collection

- [ ] B1. New creature appears in collection without manual refresh (BUG-3 fix)
- [ ] B2. Tap creature card — creature detail loads with tabs (Actions | Live | Log)
- [ ] B3. Species badge shows correct group + color
- [ ] B4. Field notes overlay — tap hero image, taxonomy + description slides up

## C. Perch

- [ ] C1. Tap Perch — location picker appears
- [ ] C2. Configure perch — walk-in budget label shows real number (not raw ${} — BUG-4 fix)
- [ ] C3. Perch succeeds — snackbar confirms cost
- [ ] C4. Creature now shows Fly/Join/Tether/Gift actions (not Perch)
- [ ] C5. Rabble visibility: check if perch is public/free/paid as configured

## D. Fly

- [ ] D1. Tap Fly — pick destination + optional route description
- [ ] D2. Fly returns immediately ("Flight planned!") — no 15s wait (BUG-1 fix)
- [ ] D3. Flight plan arrives async (check workspace messages or Log tab)
- [ ] D4. Creature visible on Explore map
- [ ] D5. Tap creature card after fly — no "creature not found" (BUG-2 fix)

## E. Live Activity

- [ ] E1. Live tab shows active flight data (map, chat)
- [ ] E2. Map shows creature location
- [ ] E3. Chat message tap → navigates to creature detail (not user profile — chat nav fix)

## F. Tether

- [ ] F1. Tether creature to GPS — tracking status appears
- [ ] F2. Tether points label shows count (not raw ${} — escaped $ fix)
- [ ] F3. Telemetry points accumulate (check DB or UI)
- [ ] F4. Untether — creature returns to flyable state (Fly action visible, not Perch — BUG-5 fix)
- [ ] F5. No "already in flight" error after untether (orphan flight fix)

## G. Rabble (needs 2nd creature or 2nd user)

- [ ] G1. Second creature joins perch
- [ ] G2. Chat works in rabble
- [ ] G3. Rabble appears in Explore tab

## H. Admin / Cross-cutting

- [ ] H1. Admin screen shows creature with correct status (perch vs fly)
- [ ] H2. Wallet reflects charges (mint 3cr, perch 2cr, fly 1cr, tether 1cr)
- [ ] H3. Explore tab shows creatures with images and correct card sizes
- [ ] H4. Workspace has 7 system agents (including flight_coordinator)
