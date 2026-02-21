# Governance Mechanics — Design Document

> **Status:** Design complete, minimal MVP set identified
> **Author:** Session 2026-02-21
> **Priority:** Required for soft launch — no social platform without governance
> **Estimated effort:** ~6-8h for MVP (Block + Eject + Report)

---

## Overview

Three governance primitives that cover all essential social safety:

| Mechanic | Who | What | Why |
|----------|-----|------|-----|
| **Block** | Any user | Hide another user/creature from your experience | Personal safety boundary |
| **Eject** | Rabble host | Remove a creature from your rabble | Host moderation |
| **Report** | Any user | Flag content/behavior for review | Platform safety |

These three cover: "I don't want to see you" (Block), "You can't be here" (Eject),
and "Something's wrong" (Report). Everything else is future.

---

## 1. Block — Personal Safety

### Hybrid Model: Creature-level + User-level escalation

**Creature blocks creature** (default, light boundary):
- "Luna doesn't want to interact with Bad Bunny"
- The creatures have a relationship boundary, not the humans
- You can still interact through different creatures

**User blocks user** (escalation, full boundary):
- "I don't want any interaction with this person"
- ALL creatures owned by the blocked user become invisible to ALL of yours
- The nuclear option — for harassment, not disagreements

### Privacy Rule

**The blocked person does not know they are blocked.**
Their messages silently don't reach you. Their friendship requests silently fail.
No "you have been blocked" message. This preserves safety for the blocker
and prevents retaliation.

### What "blocked" means

#### Creature Block (creature_a blocks creature_b)

| Action | Result |
|--------|--------|
| Send friendship request | Silently rejected |
| Existing friendship | Ended (status → 'ended_by_block') |
| Chat in same rabble | Messages visible (shared space) |
| See in flock viz | Visible (shared space) |
| Send creature invite | Silently rejected |
| View creature card | Allowed (public info) |
| Join same rabble | Allowed (can't prevent public presence) |
| Mention in chat (@creature) | Blocked — mention not delivered |
| Appear in friend suggestions | Hidden |
| Gift creature to | Silently rejected |

#### User Block (user_a blocks user_b) — escalation

| Action | Result |
|--------|--------|
| All creature-to-creature interactions | Blocked (applies to ALL creature pairs) |
| Chat messages in shared rabble | **Hidden** — their messages invisible to you |
| Flock viz | Their creatures **hidden** from your view |
| View any of their creature cards | "Creature not found" |
| Search results | Their creatures/profile hidden |
| Notifications from them | Suppressed |
| Join same rabble | Allowed (can't prevent presence in public space) |
| Co-presence tracking | Excluded |

### Reversibility

- Blocks can be unblocked at any time
- Cooldown: max 3 block/unblock cycles per pair per 24h (prevents harassment-by-blocking)
- Unblocking does NOT restore ended friendships — must re-request

### Data Model

```sql
CREATE TABLE creature_blocks (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    blocker_creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    blocked_creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(blocker_creature_id, blocked_creature_id)
);

CREATE TABLE user_blocks (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    blocker_user_id TEXT NOT NULL,
    blocked_user_id TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(blocker_user_id, blocked_user_id)
);

CREATE INDEX idx_creature_blocks_blocker ON creature_blocks(blocker_creature_id);
CREATE INDEX idx_creature_blocks_blocked ON creature_blocks(blocked_creature_id);
CREATE INDEX idx_user_blocks_blocker ON user_blocks(blocker_user_id);
CREATE INDEX idx_user_blocks_blocked ON user_blocks(blocked_user_id);
```

### API Endpoints

```
POST   /api/creatures/:id/block           — creature blocks creature
DELETE /api/creatures/:id/block           — creature unblocks creature
POST   /api/users/:id/block              — user blocks user (escalation)
DELETE /api/users/:id/block              — user unblocks user
GET    /api/my/blocks                    — list my blocks (creature + user level)
```

### Block Check Helper (called from social endpoints)

```rust
/// Returns true if any block exists between these two creatures or their owners.
pub(crate) async fn is_blocked(
    pool: &PgPool,
    creature_a: Uuid,
    creature_b: Uuid,
) -> bool {
    // Check creature-level block (either direction)
    let creature_blocked = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM creature_blocks
            WHERE (blocker_creature_id = $1 AND blocked_creature_id = $2)
               OR (blocker_creature_id = $2 AND blocked_creature_id = $1)
        )"
    ).bind(creature_a).bind(creature_b)
     .fetch_one(pool).await.unwrap_or(false);

    if creature_blocked { return true; }

    // Check user-level block (either direction)
    let user_blocked = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(
            SELECT 1 FROM user_blocks ub
            JOIN creatures ca ON ca.owner_id = ub.blocker_user_id
            JOIN creatures cb ON cb.owner_id = ub.blocked_user_id
            WHERE (ca.creature_id = $1 AND cb.creature_id = $2)
               OR (ca.creature_id = $2 AND cb.creature_id = $1)
        )"
    ).bind(creature_a).bind(creature_b)
     .fetch_one(pool).await.unwrap_or(false);

    user_blocked
}
```

### Where Block Checks Are Inserted

| Endpoint | Check |
|----------|-------|
| `send_friendship_request` | If blocked → silent 200 OK (no error, just doesn't create) |
| `post_rabble_message` | If user-blocked → message stored but excluded from blocked user's feed |
| `list_rabble_members` | If user-blocked → excluded from member list for the blocker |
| `send_creature_invite` | If blocked → silent 200 OK |
| `get_creature_handler` | If user-blocked → 404 "Creature not found" |
| `search_users_handler` | If user-blocked → excluded from results |
| `flock_history_handler` | If user-blocked → excluded from flock data for the blocker |

### Flutter UI

#### Creature Card (viewing someone else's creature)
```
⋮ (overflow menu, top-right)
  → Block [creature name]
  → Report [creature name]
```

#### Profile / Settings
```
Blocked creatures (3)
  🦋 Bad Bunny          [Unblock]
  🪲 Moth Thing         [Unblock]

Blocked users (1)
  @spammer_42           [Unblock]
```

#### Escalation Flow
```
Block [creature name]?
  → [Block this creature]     — creature-level
  → [Block this user entirely] — user-level (shows owner name)
  → [Cancel]
```

---

## 2. Eject — Host Moderation

The rabble host (anchor creature's owner) can remove any creature from their rabble.
This is moderation, not personal blocking — the ejected creature can still interact
with the host in other contexts.

### What happens on eject

1. Creature's flight `swarm_id` cleared
2. `creature_state` set to 'perched', `rabble_id` cleared
3. Creature count decremented
4. System message: "[creature name] was removed from the rabble"
5. Notification to the ejected creature's owner
6. Ejected creature can NOT rejoin this specific rabble for 24h (cooldown)
7. Host can optionally ban permanently from this rabble

### Data Model

```sql
CREATE TABLE rabble_ejections (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    swarm_id        UUID NOT NULL REFERENCES swarm_events(swarm_id) ON DELETE CASCADE,
    ejected_creature_id UUID NOT NULL REFERENCES creatures(creature_id) ON DELETE CASCADE,
    ejected_by_user TEXT NOT NULL,       -- the host who ejected
    reason          TEXT,                -- optional reason (visible to platform, not ejected user)
    permanent       BOOLEAN NOT NULL DEFAULT false,  -- permanent ban from this rabble
    ejected_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    cooldown_until  TIMESTAMPTZ          -- null if permanent, otherwise NOW() + 24h
);

CREATE INDEX idx_ejections_swarm ON rabble_ejections(swarm_id);
CREATE INDEX idx_ejections_creature ON rabble_ejections(ejected_creature_id);
```

### Ejection Check

Added to `join_swarm_handler`:
```rust
// Check if creature has been ejected from this rabble
let ejection = sqlx::query(
    "SELECT permanent, cooldown_until FROM rabble_ejections
     WHERE swarm_id = $1 AND ejected_creature_id = $2
     ORDER BY ejected_at DESC LIMIT 1"
).bind(swarm_id).bind(creature_id)
 .fetch_optional(pool).await.ok().flatten();

if let Some(row) = ejection {
    let permanent: bool = row.get("permanent");
    let cooldown: Option<DateTime<Utc>> = row.try_get("cooldown_until").ok().flatten();
    if permanent {
        return Err((StatusCode::FORBIDDEN,
            "This creature has been permanently removed from this rabble.".into()));
    }
    if let Some(until) = cooldown {
        if Utc::now() < until {
            return Err((StatusCode::FORBIDDEN,
                format!("This creature was removed. You can rejoin after {}.",
                    until.format("%H:%M UTC"))));
        }
    }
}
```

### API Endpoints

```
POST /api/rabble/:id/eject
  body: { creature_id, reason?, permanent: bool }
  auth: must be rabble host (anchor creature owner)
  → removes creature, system message, notification

GET /api/rabble/:id/ejections
  auth: rabble host only
  → list ejections for this rabble

DELETE /api/rabble/:id/eject/:creature_id
  auth: rabble host only
  → lift ban / allow rejoin
```

### Flutter UI

#### Rabble Chat — Members Section
Each member (except anchor) gets a context menu:
```
Long-press on member creature
  → Remove from rabble (24h cooldown)
  → Ban from rabble (permanent)
  → Report creature
```

#### Rabble Chat — Host Controls
```
App bar → Settings icon (host only)
  → Manage members
  → Ejected creatures (lift bans)
  → End rabble
```

---

## 3. Report — Platform Safety

Reports flag content or behavior for review. For MVP, reports are stored in a table
and reviewed manually (admin screen). Future: agentic moderation with LLM classification.

### Report Types

| Type | What can be reported | Context |
|------|---------------------|---------|
| `creature` | Inappropriate creature name/image | Creature card |
| `message` | Chat message content | Rabble chat |
| `user` | User behavior pattern | Profile |
| `rabble` | Inappropriate rabble name/description | Rabble card |

### Data Model

```sql
CREATE TABLE reports (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    reporter_user_id TEXT NOT NULL,
    report_type     TEXT NOT NULL,           -- 'creature', 'message', 'user', 'rabble'
    target_id       TEXT NOT NULL,           -- UUID of the reported entity (as text for polymorphism)
    target_type     TEXT NOT NULL,           -- same as report_type (redundant but explicit)
    reason          TEXT NOT NULL,           -- 'inappropriate_content', 'harassment', 'spam', 'impersonation', 'other'
    description     TEXT,                    -- free-text from reporter
    context         JSONB DEFAULT '{}',      -- snapshot of reported content (message text, creature name, etc.)
    status          TEXT NOT NULL DEFAULT 'pending',  -- 'pending', 'reviewed', 'action_taken', 'dismissed'
    reviewed_by     TEXT,                    -- admin user_id
    review_notes    TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    reviewed_at     TIMESTAMPTZ
);

CREATE INDEX idx_reports_status ON reports(status) WHERE status = 'pending';
CREATE INDEX idx_reports_target ON reports(target_type, target_id);
CREATE INDEX idx_reports_reporter ON reports(reporter_user_id);
```

### Report Reasons (standardized)

| Reason code | Label | Description |
|------------|-------|-------------|
| `inappropriate_content` | Inappropriate content | Offensive names, images, or messages |
| `harassment` | Harassment | Targeted negativity, threats, bullying |
| `spam` | Spam | Repetitive, unsolicited, or promotional content |
| `impersonation` | Impersonation | Pretending to be someone else |
| `other` | Other | Free-text description required |

### API Endpoints

```
POST /api/reports
  body: { report_type, target_id, reason, description? }
  → creates report, captures context snapshot
  → returns: { report_id, status: 'pending' }
  → 200 OK always (don't reveal whether action will be taken)

GET /api/admin/reports?status=pending
  auth: admin only
  → list reports for review

PUT /api/admin/reports/:id
  auth: admin only
  body: { status, review_notes }
  → update report status
```

### Context Snapshots

When a report is filed, the system captures a snapshot of the reported content
so it can be reviewed even if the content is later edited or deleted:

```rust
let context = match report_type {
    "message" => {
        let msg = get_message(pool, target_id).await;
        json!({
            "content": msg.content,
            "sender_id": msg.sender_id,
            "creature_name": msg.creature_name,
            "swarm_id": msg.swarm_id,
            "created_at": msg.created_at,
        })
    },
    "creature" => {
        let creature = get_creature(pool, target_id).await;
        json!({
            "specimen_name": creature.specimen_name,
            "owner_id": creature.owner_id,
            "asset_path": creature.asset_path,
        })
    },
    // ... etc
};
```

### Flutter UI

#### Report Flow (universal)
```
⋮ overflow menu or long-press on any reportable entity
  → "Report"
  → Pick reason: [Inappropriate] [Harassment] [Spam] [Impersonation] [Other]
  → Optional: "Tell us more" text field
  → [Submit Report]
  → "Thanks for reporting. We'll review this."
```

#### Combined with Block
```
Block [creature name]?
  → [Block this creature]
  → [Block this user entirely]
  → [Block & Report]           ← files report + blocks in one action
  → [Cancel]
```

---

## Integration: Block + Eject + Report Working Together

### Scenario: Harassment in a rabble

1. User sees offensive message from creature X
2. Long-press message → **Report** (captures message snapshot)
3. **Block creature X** (X's messages hidden from user's view)
4. Rabble host sees the report notification → **Eject creature X** (removed from rabble)
5. If behavior continues → host **Bans permanently** from rabble
6. If severe → user escalates to **Block user** (all creatures hidden)
7. Admin reviews report → takes platform-level action if needed

### Scenario: Unwanted friendship request

1. Creature A sends friendship request to creature B
2. B declines
3. A sends again → B long-presses → **Block creature A**
4. Future requests from A silently fail
5. B never sees A in friend suggestions again
6. If A creates new creatures to circumvent → B escalates to **Block user**

---

## Implementation Phases

| Phase | What | Effort | Priority |
|-------|------|--------|----------|
| **1. Block** | Creature + user block/unblock, check helper, friendship ending | 2-3h | **MVP** |
| **2. Eject** | Host removes creature, cooldown, rejoin check | 2h | **MVP** |
| **3. Report** | Report creation, context snapshot, admin list | 2h | **MVP** |
| **4. Chat filtering** | Hide blocked user messages in real-time | 1h | **MVP** |
| **5. Admin review** | Admin screen for reviewing reports + taking action | 1-2h | Post-launch |
| **6. Agentic moderation** | LLM classification of reports, auto-action for clear cases | Future | Future |

**Total MVP: ~7-8h** (Phases 1-4)

---

## Admin Actions (Post-Launch)

When reviewing reports, admins can:

| Action | What it does |
|--------|-------------|
| **Dismiss** | Report was unfounded, no action |
| **Warn** | Send warning notification to reported user |
| **Mute** | Temporarily disable chat for user (24h/7d/30d) |
| **Suspend creature** | Set creature status to 'suspended' (can't interact) |
| **Suspend user** | Disable account temporarily |
| **Ban user** | Permanent account ban |
| **Delete content** | Remove specific message/creature/rabble |

---

## Privacy & Legal Considerations

- Blocks are private — the blocked party is never informed
- Reports are confidential — the reporter's identity is not shared with the reported party
- Context snapshots are retained for 90 days after resolution, then purged
- User data deletion requests (GDPR) must also delete their reports and blocks
- Block data is user-controlled and deletable at any time
- Ejection records are retained as long as the rabble exists