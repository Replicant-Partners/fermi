# Rabble Design Notes

> Living design document for UX decisions, state constraints, and future work.

---

## Creature ↔ Rabble Relationship Model

### Core constraint

A **creature** can only exist in **one rabble at a time** and has **one location at a time**.

```
User (owner)
 └── has many Creatures
       └── each creature is in 0..1 Rabble (current)
       └── each creature has 0..1 Location (current)

Rabble (swarm_event)
 └── has many Creatures (from different owners)
```

- A **user** participates in multiple rabbles simultaneously — via different creatures.
- A **creature** is 1:1 with its current rabble. Moving it to a new rabble removes it from the old one.
- Historical membership is preserved in `creature_flights` / `creature_versions`, but the live state is singular.

### Implication for UI

When the user "joins" a rabble with a creature that's already in another rabble, this is a **relocation**, not a dual-membership. The UI must:

1. Show where the creature currently is before the move.
2. Use "Move here" language, not "Join" or "Add".
3. Confirm the relocation with a dialog naming the source rabble.
4. Make it clear there's no charge for adding creatures to your own rabble ("Free — your rabble").

---

## Chat Multiplexing

### Current state

The user can have **multiple rabbles open** (one per creature), but the chat screen is single-instance — navigating to a rabble replaces the current view.

### Target UX

- After moving a creature to a rabble, the creature tray should **auto-select** the newly arrived creature so you can immediately chat as that creature.
- The user should be able to **switch between rabble chats** without losing scroll position or draft messages. This could be:
  - A tab bar at the top of the chat (if few rabbles), or
  - A rabble switcher drawer / bottom sheet.
- Each rabble chat maintains its own SSE stream / poll independently.

### Post-move flow

When a creature is moved to a new rabble:

1. `joinSwarm` API call completes.
2. Reload the creature list for the target rabble (`_loadMyCreatures`).
3. Auto-set `_activeCreatureId` to the moved creature.
4. The creature should appear in the tray immediately, highlighted.
5. The chat panel should be ready — the creature can talk right away.

If the user was viewing the **source** rabble's chat at the time, they should see a system message like "[Creature] has left the rabble" and the creature should disappear from that tray.

---

## Nearby Search — Radius Tuning

The radius slider on the Nearby tab is tuned for **social density**:

- **Range**: 50m → 2km
- **Default**: 500m
- **Steps**: ~50m increments (39 divisions)

The reasoning: rabbles are place-based social events. A 50km radius defeats the purpose — you want to discover things you could walk to, not things across the city. If we find users need wider search for discovery (e.g., planning), we can add a separate "Explore map" view with wider range, but the dashboard Nearby tab should stay tight.

---

## Social Layer — Future Work

### Creature-mediated relationships

The social graph should be **creature-first**:

- **Befriend a creature**: You encounter another creature in a rabble and "befriend" it. This is creature-to-creature, not user-to-user.
- **Follow an owner**: A separate, opt-in escalation. The owner may remain anonymous (proxied behind their creatures).
- **Privacy model**: Users choose visibility — `public` (name shown), `creature-only` (only creature identities visible), `private` (hidden from search). Creatures have their own visibility (`public`, `contacts`, `private`).

### Discovery surfaces

- **In-rabble members list**: See all creatures + "Befriend" action.
- **Post-rabble recap**: "You met..." screen after a rabble ends.
- **QR scan**: Already exists — promote more prominently.
- **Share links**: `rabble.world/join/[token]` for non-users.

### External invitation

The invite flow needs to be frictionless:

1. Generate a share link (rabble or creature profile).
2. Non-users land on a web preview → sign up → land directly in the rabble.
3. Existing users tap the link → open app → join immediately.

### Data model additions needed

```sql
-- Creature-to-creature friendships (symmetric, canonical order)
CREATE TABLE IF NOT EXISTS creature_friendships (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_a UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    creature_b UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    initiated_by TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending | accepted | declined
    met_in_rabble UUID REFERENCES swarm_events(swarm_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(creature_a, creature_b),
    CHECK (creature_a < creature_b)
);

-- User social visibility preference
ALTER TABLE users ADD COLUMN IF NOT EXISTS social_visibility TEXT
    NOT NULL DEFAULT 'public'
    CHECK (social_visibility IN ('public', 'creature-only', 'private'));
```

---

## Explore / Activity Feed — Future Work

### Problems

- Feed polls every 30s but replaces the whole list, losing scroll position.
- Events are context-free — no indication of whether an event involves your creatures, contacts, or favourited creatures.
- Data goes stale between polls.

### Solutions

- **SSE stream** (`/api/feed/stream`): Push new events, prepend at top without disrupting scroll.
- **Context annotations**: Tag each event with relationship info ("your creature", "contact's creature", "favourited"). Requires backend join against `contacts`, `creature_favourites`, swarm membership.
- **Visual priority**: Events involving your graph get a subtle highlight.

---

## Changelog

| Date | Author | Notes |
|------|--------|-------|
| 2026-02-17 | Session | Initial design notes from dashboard redesign discussion |