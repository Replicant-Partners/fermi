# Gift-as-Invite — Design Document

> **Status:** Design complete, implementation deferred to next sprint
> **Author:** Session 2026-02-21
> **Priority:** After core social loop + rich media chat
> **Dependencies:** Minting pipeline, deep links, campaign infrastructure

---

## Overview

Collapse onboarding, invitation, and creature economy into one action:
**the creature IS the invite.** Instead of "here's a link to my rabble",
you send "here's a creature that's your ticket in."

The gift creature is non-transferable (soulbound) until the recipient
develops it to a sufficient cognition level — encouraging engagement
and providing a natural tax on system gaming.

---

## Three Tiers

| Tier | What | Cost | Use case |
|------|------|------|----------|
| **Gift Invite** | Mint 1 creature → send to specific person → deep link to rabble | 3-5cr | "Hey come to my rabble, here's a butterfly" |
| **AR Drop** | Mint creature → encode as AR marker/QR → recipient scans IRL | 3-5cr | Leave a creature at a café, whoever scans it joins |
| **Campaign** | Batch mint N creatures → share link → first N claimants get one + auto-join rabble | 3cr × N (volume discounts) | Pop-up event, concert, meetup — "first 50 people get in" |

---

## Core Principles

1. **Zero-friction onboarding** — new user's first action gives them a creature
   AND a social context. No empty state.
2. **Economic loop** — host pays to mint invite creatures (drives credit economy).
   Invitees arrive with a creature ready to participate.
3. **Non-transferable (soulbound)** — the creature is bound to the recipient on claim.
   Can't be flipped. It's a social bond, not a tradeable asset.
   Transferability unlocks after reaching a cognition level threshold (see below).
4. **Deep link to rabble** — opens directly in context. No "now go find the rabble."
5. **Campaign = event ticketing** — pop-up rabbles with limited creature invites.
   Scarcity drives engagement.
6. **Physical-digital bridge** — AR drops and QR codes make digital creatures
   tangible at real locations.
7. **Giving and receiving should be fun** — creatures look cool, collecting encourages
   learning, trading fosters attachment. This is a social mechanic, not a utility.

---

## Flow: Gift Invite (1:1)

```
Host opens creature card or rabble chat
  → "Invite with creature" action
  → Species picker:
      Option A: Choose specific species (intentional, personal gift)
      Option B: Auto-generate from rabble location/biome (lower friction)
  → Creature minted (status: 'gift_unclaimed', transferable: false)
  → Deep link generated: rabble.world/claim/{token}
  → Share via WhatsApp / SMS / email / QR code
  → Recipient opens link:
      If new user → create account flow → creature in wallet → land in rabble
      If existing user → creature added to collection → land in rabble
  → Gift creature auto-joins the rabble on claim
  → System message in chat: "[creature] arrived as a gift from [host creature]!"
```

### Gift Invite Data Model

```sql
CREATE TABLE creature_gifts (
    gift_id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creature_id     UUID NOT NULL REFERENCES creatures(creature_id),
    sender_user_id  TEXT NOT NULL,
    sender_creature_id UUID REFERENCES creatures(creature_id),  -- the persona who sent it
    recipient_email TEXT,                    -- optional: specific recipient
    recipient_user_id TEXT,                  -- set on claim
    rabble_id       UUID REFERENCES swarm_events(swarm_id),  -- deep link target
    claim_token     TEXT NOT NULL UNIQUE,    -- the shareable token
    status          TEXT NOT NULL DEFAULT 'unclaimed',  -- unclaimed, claimed, expired
    claimed_at      TIMESTAMPTZ,
    expires_at      TIMESTAMPTZ,            -- null = no expiry
    campaign_id     UUID REFERENCES creature_campaigns(campaign_id),  -- null for 1:1 gifts
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_creature_gifts_token ON creature_gifts(claim_token);
CREATE INDEX idx_creature_gifts_recipient ON creature_gifts(recipient_user_id) WHERE recipient_user_id IS NOT NULL;
CREATE INDEX idx_creature_gifts_campaign ON creature_gifts(campaign_id) WHERE campaign_id IS NOT NULL;
CREATE INDEX idx_creature_gifts_unclaimed ON creature_gifts(status) WHERE status = 'unclaimed';
```

---

## Flow: Campaign (1:many)

```
Host opens rabble settings or Rabbles tab
  → "Create Campaign" action
  → Configure:
      - Species template (e.g. "Spring Grasshopper") or species mix
      - Quantity (N creatures)
      - Pricing: 3cr × N (volume discounts for 20+, 50+)
      - Expiry (optional)
      - Geofence (optional: claimants must be within X km)
  → N creatures minted (status: 'gift_unclaimed', transferable: false)
  → Single share link: rabble.world/campaign/{token}
  → Share anywhere
  → Each claimant:
      - Gets a unique creature from the pool
      - Auto-joins the rabble
      - Landing page shows: "12/50 claimed — grab yours!"
  → Campaign dashboard for host:
      - Claimed / remaining counter
      - Claimant list (creature names + user handles)
      - Close campaign early option
```

### Campaign Data Model

```sql
CREATE TABLE creature_campaigns (
    campaign_id     UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    creator_id      TEXT NOT NULL,
    rabble_id       UUID REFERENCES swarm_events(swarm_id),
    name            TEXT NOT NULL,           -- "Spring Launch Party"
    species_template TEXT,                   -- species_group or specific scientific_name
    total_quantity  INTEGER NOT NULL,
    claimed_count   INTEGER NOT NULL DEFAULT 0,
    share_token     TEXT NOT NULL UNIQUE,    -- the shareable link token
    status          TEXT NOT NULL DEFAULT 'active',  -- active, paused, completed, expired
    geofence_lat    DOUBLE PRECISION,       -- optional center
    geofence_lng    DOUBLE PRECISION,
    geofence_radius_m INTEGER,              -- optional radius in meters
    expires_at      TIMESTAMPTZ,
    total_cost      INTEGER NOT NULL,        -- credits charged on creation
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_campaigns_token ON creature_campaigns(share_token);
CREATE INDEX idx_campaigns_rabble ON creature_campaigns(rabble_id);
CREATE INDEX idx_campaigns_status ON creature_campaigns(status) WHERE status = 'active';
```

### Volume Pricing

| Quantity | Per creature | Total | Discount |
|----------|-------------|-------|----------|
| 1-9 | 3cr | 3-27cr | — |
| 10-19 | 2.5cr | 25-47cr | ~17% |
| 20-49 | 2cr | 40-98cr | ~33% |
| 50+ | 1.5cr | 75cr+ | 50% |

---

## Flow: AR Drop (1:1, location-bound)

```
Host in AR viewer
  → "Drop creature" action
  → Pick species (or auto from location biome)
  → Place in AR scene at current location
  → Creature minted + QR code generated
  → QR encodes: rabble.world/claim/{token}
  → Host can:
      - Screenshot the AR scene with QR visible
      - Print QR for physical placement
      - Share QR digitally
  → Scanner claims creature + auto-joins rabble
  → Physical-digital bridge: "I found a butterfly at the café!"
```

AR Drops reuse the Gift Invite data model with `claim_token` encoded in a QR.
The AR placement metadata (lat/lng, scene context) is stored in `creature_gifts.metadata`.

---

## Soulbound → Transferable (Cognition Gate)

Gift creatures are **non-transferable** on claim. This prevents:
- Mass claiming + reselling
- Bot farming
- Devaluing the gift relationship

Transferability unlocks when the creature reaches **Cognition Level 3** (or configurable threshold).
This requires genuine engagement:
- Multiple flights
- Rabble participation
- Dream cycles
- Observations

### Implementation

```sql
-- On creatures table:
ALTER TABLE creatures ADD COLUMN IF NOT EXISTS transferable BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE creatures ADD COLUMN IF NOT EXISTS transfer_unlock_level INTEGER DEFAULT 0;  -- 0 = always transferable

-- Gift creatures minted with:
--   transferable = false
--   transfer_unlock_level = 3
```

Transfer check in `transfer_creature` handler:
```rust
if !creature.transferable {
    let level = get_cognition_level(pool, creature_id).await;
    if level < creature.transfer_unlock_level {
        return Err((StatusCode::FORBIDDEN,
            format!("This creature needs to reach Level {} before it can be transferred. Currently Level {}.",
                creature.transfer_unlock_level, level)));
    }
    // Auto-unlock on first successful transfer attempt at sufficient level
    sqlx::query("UPDATE creatures SET transferable = true WHERE creature_id = $1")
        .bind(creature_id).execute(pool).await.ok();
}
```

---

## Deep Link Architecture

### URL Structure

```
rabble.world/claim/{token}       — single gift creature
rabble.world/campaign/{token}    — campaign landing page
rabble.world/drop/{token}        — AR drop (same as claim)
```

### Universal Link / Deep Link Handling

```dart
// Flutter: handle incoming deep links
// If user is authenticated → claim creature → navigate to rabble
// If user is new → onboarding flow → claim → navigate to rabble

class DeepLinkHandler {
  static Future<void> handle(Uri uri, BuildContext context, ApiClient api) async {
    final path = uri.pathSegments;
    if (path.length == 2) {
      switch (path[0]) {
        case 'claim':
        case 'drop':
          await _handleClaim(path[1], context, api);
          break;
        case 'campaign':
          await _handleCampaign(path[1], context, api);
          break;
      }
    }
  }

  static Future<void> _handleClaim(String token, ...) async {
    // POST /api/gifts/claim/{token}
    // Response: { creature_id, rabble_id, creature_name, ... }
    // Navigate to rabble chat with new creature as active persona
  }

  static Future<void> _handleCampaign(String token, ...) async {
    // GET /api/campaigns/{token} → campaign info + remaining count
    // Show campaign landing page
    // "Claim your creature" button → POST /api/campaigns/{token}/claim
    // Navigate to rabble chat
  }
}
```

---

## API Endpoints

### Gift Invite

```
POST /api/gifts/create
  body: { creature_species?, rabble_id?, recipient_email?, auto_species: bool }
  → mints creature, generates claim_token
  → returns: { gift_id, claim_token, claim_url, creature_id, creature_name }
  → cost: 3-5cr

POST /api/gifts/claim/{token}
  → claims creature for authenticated user
  → auto-joins rabble if rabble_id set
  → returns: { creature_id, rabble_id, creature_name, is_new_user }

GET /api/gifts/sent
  → list gifts I've sent (with claim status)

GET /api/gifts/received
  → list gifts I've received
```

### Campaign

```
POST /api/campaigns/create
  body: { rabble_id, name, species_template, quantity, expires_at?, geofence? }
  → batch mints creatures, generates share_token
  → charges volume-priced credits
  → returns: { campaign_id, share_token, share_url, total_cost, quantity }

GET /api/campaigns/{token}
  → public: campaign info, claimed/total, rabble info
  → no auth required (landing page data)

POST /api/campaigns/{token}/claim
  → claims one creature for authenticated user
  → auto-joins rabble
  → returns: { creature_id, creature_name, claimed_count, remaining }

GET /api/campaigns/mine
  → list campaigns I've created (with stats)

POST /api/campaigns/{id}/pause
POST /api/campaigns/{id}/close
```

---

## Campaign Landing Page (Web)

```
┌────────────────────────────────────────┐
│  🦗 Spring Launch Party               │
│  Hosted at Parque Lincoln              │
│                                        │
│  [Creature image / AR preview]         │
│                                        │
│  "Grab a Spring Grasshopper and join   │
│   the rabble at the park!"             │
│                                        │
│  ████████████░░░░  38/50 claimed       │
│                                        │
│  [🦗 Claim Your Creature]             │
│                                        │
│  By @ivan · rabble.world               │
└────────────────────────────────────────┘
```

This page works without the app installed — progressive web app handles it.
New users create an account inline, claim the creature, and land in the rabble.

---

## Unclaimed Creatures (Future Mechanic)

Unclaimed gift creatures and expired campaign creatures could be:

1. **Returned to sender** — refund credits, creature dissolved
2. **Adoption pool** — public pool of unclaimed creatures available for adoption
   (cost: 1cr adoption fee). Creates a secondary market mechanic.
3. **Wild release** — creature placed at the gift/campaign location as a
   "wild creature" that anyone can find and claim via AR scan

> **Decision:** Defer to future sprint. For now, unclaimed gifts expire
> after 30 days and credits are refunded to sender. The adoption pool
> is a compelling future mechanic but needs its own economic analysis
> to avoid creating an end-run around minting costs.

---

## Flutter UI Integration Points

### Creature Actions (creature_actions.dart)
- New action: "Gift to Friend" → species picker → recipient → generates link
- Existing "Gift" action (transfer) remains separate

### Rabble Chat (rabble_chat.dart)
- Invite button → option: "Invite with creature" alongside existing contact invite
- Shows: "Send a creature as your invitation — they'll arrive ready to join"

### Rabbles Screen (rabbles_screen.dart)
- Hosting tab → "Campaign" button on rabble cards
- New "Create Campaign" in host flow options

### Onboarding (onboarding_screen.dart)
- Deep link claim → simplified onboarding: create account → creature revealed → in rabble
- "Your first creature!" celebration moment

### Profile (profile_screen.dart)
- "Gifts Sent" / "Gifts Received" sections
- Campaign dashboard for hosts

---

## Implementation Phases

| Phase | What | Effort | Dependencies |
|-------|------|--------|-------------|
| **1. Gift Invite** | 1:1 gift + claim + deep link + rabble auto-join | 4-5h | Deep link handler, mint pipeline |
| **2. Campaign** | Batch mint + landing page + claim counter | 4-5h | Phase 1 + landing page |
| **3. AR Drop** | QR generation + scan-to-claim | 2-3h | Phase 1 + AR viewer integration |
| **4. Soulbound Gate** | Transferable unlock at cognition level | 1-2h | Cognition level calculation |
| **5. Campaign Dashboard** | Host management UI + stats | 2-3h | Phase 2 |
| **6. Unclaimed Pool** | Expiry + refund + future adoption mechanic | 2-3h | Phase 1-2 |

**Total estimated:** ~16-20h across 6 phases

---

## Security & Abuse Prevention

- **Rate limiting:** Max 10 gifts per hour, max 3 campaigns per day
- **Soulbound:** Gift creatures can't be transferred until cognition level threshold
- **Claim limits:** One claim per user per campaign
- **Geofencing:** Optional — claimant must be within radius to claim (campaigns)
- **Expiry:** Unclaimed gifts expire after 30 days, credits refunded
- **Bot prevention:** Account creation required before claim (existing auth flow)
- **Volume pricing:** Prevents micro-campaigns from being cheaper than direct minting

---

## Credit Flow Summary

| Action | Who pays | Cost | Revenue |
|--------|---------|------|---------|
| Gift invite | Host | 3-5cr | Platform (minting) |
| Campaign (per creature) | Host | 1.5-3cr (volume pricing) | Platform (minting) |
| Claim gift | Recipient | Free | — |
| Claim campaign creature | Recipient | Free | — |
| AR Drop | Host | 3-5cr | Platform (minting) |
| Unclaimed refund | Platform → Host | -3cr (refund) | — |

The economic model: **hosts invest in social capital** (minting creatures for others).
Recipients arrive with zero-cost engagement. The investment pays off through
rabble participation, creature-to-creature friendships, and the social loop
that drives all credit-consuming interactions.