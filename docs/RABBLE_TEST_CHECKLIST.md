# Rabble UX Test Checklist v2

Clean-slate integration test cases for the Rabble creature lifecycle.
Updated after Sprint Z bug fixes (Feb 14 session 2).

## Pre-flight: Service Health

- [ P] P1. https://agent-bestiary.world/ loads (landing page)
- [ P] P2. https://rabble.world/ loads (Flutter SPA)
- [ ?] P3. Sign in works (Google or GitHub) 

## A. Onboarding

- [ P] A1. Open app fresh (no creatures) — onboarding flow appears
- [ P] A2. Search for a butterfly species (e.g. "Vanessa atalanta")
- [ P] A3. Search for a beetle species (e.g. "Scarabaeus") — verify BEETLE tag, not BUTTERFLY
- [ P] A4. Select art style (try ukiyo-e) — verify preview hint text appears
- [ P] A5. Mint creature — wait for image generation, verify image renders (slow and on intial laod the new artisnot there - but it works!!)

## B. Collection

- [P ] B1. New creature appears in collection without manual refresh (BUG-3 fix)
- [ P] B2. Tap creature card — creature detail loads with tabs (Actions | Live | Log)
- [ P] B3. Species badge shows correct group + color
- [ X] B4. Field notes overlay — tap hero image, taxonomy + description slides up (prompt isshowing up infield notes)

## C. Perch

- [ p] C1. Tap Perch — location picker appears
- [p?n ] C2. Configure perch — walk-in budget label shows real number (not raw ${} — BUG-4 fix) (on first perch this worked on second teh label showd up starnge - but reperch failed anayway beacsue creatire was "already in perch")
- [ p] C3. Perch succeeds — snackbar confirms cost (very slow, we need an affordance or a speed pickup or it feels like it hangs - but it works!)
- [ ?] C4. Creature now shows Fly/Join/Tether/Gift actions (not Perch) (on intial completion yes - but on return after roundtriping to collection perch is visible again - so the ux lost state)
- [ X] C5. Rabble visibility: check if perch is public/free/paid as configured (after perching nothing show up in the explore screen - the workspace islaunched andit is labeld "perched in rabble"  again i dont understadn the smeantics - i though rabble happend on 2nd creature  - until then its just a solo place?)

## D. Fly

- [ p] D1. Tap Fly — pick destination + optional route description (only 1 credit?)
- [ P] D2. Fly returns immediately ("Flight planned!") — no 15s wait (BUG-1 fix) (immediatly is a strech but it was mch faster)
- [ P] D3. Flight plan arrives async (check workspace messages or Log tab)
- [ p/n] D4. Creature visible on Explore map(only creature sent on flight and not just perchedend up not a map and also can get backto creaure detial after going bakc to colleciton)
- [ X] D5. Tap creature card after fly — no "creature not found" (BUG-2 fix)( this fails and in explore mode (which has no creature pcitures teh creature it fails for has a rable that works, and a ended flight that is not found so a bit starnge))

## E. Live Activity

- [ P] E1. Live tab shows active flight data (map, chat)
- [ P] E2. Map shows creature location (satatic location)
- [ P] E3. Chat message tap → navigates to creature detail (not user profile — chat nav fix) (yes here not in the context of the rabble whre chat still clicks though to profile - should we just rebuild the explore ux?)

## F. Tether

- [ p] F1. Tether creature to GPS — tracking status appears
- [p ] F2. Tether points label shows count (not raw ${} — escaped $ fix)
- [ ] F3. Telemetry points accumulate (check DB or UI)
- [ p] F4. Untether — creature returns to flyable state (Fly action visible, not Perch — BUG-5 fix)
- [ X] F5. No "already injust flight" error after untether (orphan flight fix) (Just wierd  - what is mainaintng state betwen obkects here?)

## G. Rabble (needs 2nd creature or 2nd user)

- [ x] G1. Second creature joins perch
- [ p] G2. Chat works in rabble
- [? ] G3. Rabble appears in Explore tab ( yes ut its random and context free - confusing)

## H. Admin / Cross-cutting

- [x ] H1. Admin screen shows creature with correct status (perch vs fly) ( it shoing old data and no satus context)
- [p/n ] H2. Wallet reflects charges (mint 3cr, perch 2cr, fly 1cr, tether 1cr) ( someof this has to do with no having visbility into agent wallets and so not unerstanding where the xt value is)
- [ x] H3. Explore tab shows creatures with images and correct card sizes
- [ ?] H4. Workspace has 7 system agents (including flight_coordinator)
