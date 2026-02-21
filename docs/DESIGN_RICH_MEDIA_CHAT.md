# Rich Media Chat — Design Document

> **Status:** Design complete, implementation deferred
> **Author:** Session 2026-02-21
> **Priority:** After core social loop stabilization
> **Estimated effort:** ~12-15h across 5 phases

---

## Overview

Extend the rabble chat from text-only to a full rich media messenger supporting
images, short video, audio notes, polls, and location sharing. All media is
creature-attributed (persona model) and costs credits to send.

Everything that drives interactions drives the economic model.

---

## Message Types

| Type | Payload | Cost | Storage |
|------|---------|------|---------|
| `text` | `content: string` | 1cr | DB only |
| `image` | `media_url, thumbnail_url, width, height` | 1cr | Blob + DB |
| `video` | `media_url, thumbnail_url, duration_secs, width, height` | 1cr (≤30s) / 2cr (>30s) | Blob + DB |
| `audio` | `media_url, duration_secs, waveform_data` | 1cr | Blob + DB |
| `poll` | `question, options[], expires_at, multi_select` | 2cr (creates engagement) | DB only |
| `location` | `lat, lng, name, thumbnail_url` | 1cr | DB only |
| `system` | `content` | free | DB only |
| `narrator` | `content` | free (agent-paid) | DB only |

### Credit Rationale

- 1cr default — same as text, keeps interactions fluid
- 2cr for polls — they create multi-user engagement (everyone votes)
- 2cr for long video (>30s) — higher storage + bandwidth cost
- System/narrator free — platform-generated content
- If system cost becomes more intensive (e.g. video transcoding), revisit

---

## Blob Storage Architecture

### `rabble_media` Table

```sql
CREATE TABLE rabble_media (
    media_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    swarm_id        UUID NOT NULL REFERENCES swarm_events(swarm_id) ON DELETE CASCADE,
    uploader_id     TEXT NOT NULL,           -- user_id (the proxy)
    creature_id     UUID REFERENCES creatures(creature_id) ON DELETE SET NULL,  -- the persona
    media_type      TEXT NOT NULL,           -- 'image', 'video', 'audio'
    mime_type       TEXT NOT NULL,           -- 'image/jpeg', 'video/mp4', 'audio/webm'
    file_size       INTEGER NOT NULL,        -- bytes
    width           INTEGER,                 -- pixels (images/video)
    height          INTEGER,                 -- pixels (images/video)
    duration_secs   DOUBLE PRECISION,        -- seconds (video/audio)
    storage_path    TEXT NOT NULL,            -- blob storage key / URL
    thumbnail_path  TEXT,                     -- generated thumbnail for video
    waveform_data   JSONB,                   -- audio visualization data
    metadata        JSONB DEFAULT '{}',      -- EXIF, compression info, etc.
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_rabble_media_swarm ON rabble_media(swarm_id, created_at DESC);
CREATE INDEX idx_rabble_media_creature ON rabble_media(creature_id);
CREATE INDEX idx_rabble_media_type ON rabble_media(media_type);
```

### Storage Backend Options

| Option | Pros | Cons | Monthly cost (10GB) |
|--------|------|------|---------------------|
| **Vercel Blob** | Zero config, CDN-backed, integrated | $0.02/GB, Vercel lock-in | ~$0.20 |
| **Cloudflare R2** | 10GB free, S3-compatible, no egress | Separate service, setup | Free tier |
| **AWS S3** | Industry standard, unlimited | Egress costs, complex IAM | ~$0.50 |
| **Neon bytea** | Already there, zero infra | 5MB limit, slow, bloats DB | Included |

**Recommendation:** Vercel Blob for MVP (zero config, works with deploy pipeline).
Migrate to Cloudflare R2 if storage exceeds 10GB or costs matter at scale.

---

## Upload Flow

```
1. User taps 📎 (attach) or 🎤 (hold to record)
2. Client: Pick/capture → compress
   - Images: max 1920px, JPEG 80% quality (~200-500KB)
   - Video: max 60s, 720p, H.264 (~5-15MB)
   - Audio: WebM/Opus (~50KB/min)
3. POST /api/rabble/:swarm_id/media (multipart/form-data)
   - Headers: Authorization (standard auth)
   - Fields: file, creature_id, media_type
   → Server validates: size ≤ 20MB, type whitelist, duration ≤ 60s
   → Stores to blob storage
   → Generates thumbnail (video: first frame; image: 200px resize)
   → Returns: { media_id, media_url, thumbnail_url, width, height, duration_secs }
4. POST /api/rabble/:swarm_id/messages
   - body: { content: "", creature_id, message_type: "image"|"video"|"audio",
             metadata: { media_id, media_url, thumbnail_url, ... } }
   → 1-2cr charge (same as text message, plus media premium for video >30s)
5. Broadcast via existing RabbleEvent (real-time delivery)
```

### Validation Rules

| Media type | Max size | Max duration | Allowed MIME types |
|-----------|---------|-------------|-------------------|
| Image | 10MB | — | image/jpeg, image/png, image/webp, image/gif |
| Video | 20MB | 60s | video/mp4, video/webm, video/quicktime |
| Audio | 5MB | 120s | audio/webm, audio/mp4, audio/ogg, audio/wav |

---

## Backend Endpoints

### `POST /api/rabble/:swarm_id/media`

Upload a media file. Returns metadata for embedding in a chat message.

```rust
// Request: multipart/form-data
// Fields: file (binary), creature_id (text), media_type (text)
//
// Response: 201 Created
// {
//   "media_id": "uuid",
//   "media_url": "https://...",
//   "thumbnail_url": "https://...",
//   "width": 1280,
//   "height": 720,
//   "duration_secs": null,
//   "file_size": 245000,
//   "mime_type": "image/jpeg"
// }
```

### `POST /api/rabble/:swarm_id/messages` (extended)

Existing endpoint, extended with media message types:

```json
{
  "content": "",
  "creature_id": "uuid",
  "message_type": "image",
  "metadata": {
    "media_id": "uuid",
    "media_url": "https://...",
    "thumbnail_url": "https://...",
    "width": 1280,
    "height": 720
  }
}
```

---

## Chat Panel Rendering

### Image Messages

- Rounded corners (14px), max 260px wide, aspect-ratio preserved
- Blurhash or shimmer placeholder while loading
- Tap → fullscreen viewer with pinch-to-zoom
- Long-press → save to device / share
- Creature avatar + name header same as text messages

### Video Messages

- Inline player with play button overlay on thumbnail
- Duration badge bottom-right (e.g. "0:24")
- Tap → play/pause inline
- Double-tap or fullscreen button → native video player
- Auto-generate thumbnail from first frame server-side
- Max 60s, shown with progress bar

### Audio Messages

- Waveform visualization bar (generated server-side, stored in `waveform_data`)
- Play/pause button + duration label
- Playback speed toggle (1x / 1.5x / 2x)
- Creature avatar on the left (WhatsApp voice note style)
- Animated waveform during playback

### Location Messages

- Mini map snapshot (static image from tile server)
- Location name + coordinates
- Tap → opens in Explore tab or external maps app

---

## Polls

### Schema

```sql
CREATE TABLE rabble_polls (
    poll_id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    message_id      UUID NOT NULL REFERENCES rabble_messages(message_id) ON DELETE CASCADE,
    swarm_id        UUID NOT NULL REFERENCES swarm_events(swarm_id) ON DELETE CASCADE,
    creator_id      TEXT NOT NULL,
    creature_id     UUID REFERENCES creatures(creature_id) ON DELETE SET NULL,
    question        TEXT NOT NULL,
    options         JSONB NOT NULL,           -- [{id: "a", text: "Yes"}, {id: "b", text: "No"}]
    multi_select    BOOLEAN NOT NULL DEFAULT false,
    anonymous       BOOLEAN NOT NULL DEFAULT false,
    expires_at      TIMESTAMPTZ,              -- null = no expiry
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE rabble_poll_votes (
    vote_id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    poll_id             UUID NOT NULL REFERENCES rabble_polls(poll_id) ON DELETE CASCADE,
    voter_user_id       TEXT NOT NULL,
    voter_creature_id   UUID REFERENCES creatures(creature_id) ON DELETE SET NULL,
    option_id           TEXT NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (poll_id, voter_creature_id, option_id)  -- one vote per creature per option
);

CREATE INDEX idx_poll_votes_poll ON rabble_poll_votes(poll_id);
```

### Poll UX

- Question text + options as tappable horizontal bars
- Live vote count updates via broadcast
- Your vote highlighted (creature-attributed)
- Results shown as percentage bars after voting or expiry
- "X creatures voted" footer
- Creator can close poll early

### Poll Endpoints

```
POST /api/rabble/:swarm_id/poll          — create poll (2cr)
POST /api/rabble/:swarm_id/poll/:id/vote — cast vote (free)
GET  /api/rabble/:swarm_id/poll/:id      — get results
POST /api/rabble/:swarm_id/poll/:id/close — creator closes early
```

---

## Journal Integration (Future)

Media in chat becomes source material for creature journals and rabble summaries:

### Automatic Capture

- **Flight recap video** — auto-compiled from location waypoints + creature images along the route
- **Rabble summary** — agentic narrative assembled from chat highlights, poll results, media shared, creatures present
- **Audio transcription** — voice notes transcribed → searchable → part of creature's episodic memory (ADM)

### Manual Curation

- Long-press any media message → "Add to Journal"
- Journal entries tagged with creature, rabble, location, timestamp
- Rich journal view: timeline of media + text + observations

### Agentic Summary (Credit Cost)

- End-of-rabble summary generated by the lifecycle coordinator agent
- Costs 3-5cr (agentic processing)
- Includes: who was there, what happened, highlight moments, poll outcomes
- Stored as a creature_version entry (part of the versioned state model)
- Becomes source material for the Dream consolidation (ADM Phase 4)

---

## Flutter Dependencies

```yaml
# Already in pubspec (verify):
image_picker: ^1.0.0        # Camera + gallery
video_player: ^2.8.0         # Inline video playback
just_audio: ^0.9.36          # Audio playback + waveform
record: ^5.0.0               # Audio recording
cached_network_image: ^3.3.0 # Already used

# May need:
blurhash_dart: ^1.2.0        # Image placeholder blur
video_compress: ^3.1.0       # Client-side video compression
```

---

## Implementation Phases

| Phase | What | Effort | Dependencies |
|-------|------|--------|-------------|
| **1. Images** | Upload + display + fullscreen | 2-3h | Vercel Blob or equivalent |
| **2. Audio** | Recording (hold button) + waveform playback | 2-3h | `record` + `just_audio` packages |
| **3. Video** | Short video capture + thumbnail + inline player | 3-4h | `video_player` + server-side thumbnail |
| **4. Polls** | Create + vote + results + close | 2-3h | New DB tables |
| **5. Location** | Share current location as message | 1h | Map tile snapshot |
| **6. Journal** | Media → journal curation + agentic summary | Future sprint | ADM integration |

### Phase 1 Detail (Images — first to implement)

```
Backend:
  - Migration: CREATE TABLE rabble_media (schema above)
  - POST /api/rabble/:id/media — accept multipart, store to Vercel Blob
  - Thumbnail generation: resize to 200px width server-side
  - Return media_id + URLs

Flutter:
  - 📎 button in chat input bar (left of text field)
  - image_picker: camera or gallery
  - Compress client-side (1920px max, JPEG 80%)
  - Upload with progress indicator
  - Send as message_type: 'image' with metadata
  - Render: rounded image bubble, tap for fullscreen

Chat panel changes:
  - _buildChatMessage checks message_type
  - 'image' → _buildImageBubble(msg)
  - 'video' → _buildVideoBubble(msg)
  - 'audio' → _buildAudioBubble(msg)
  - 'poll'  → _buildPollBubble(msg)
  - 'text'  → existing text bubble (default)
```

---

## Security & Moderation

- **File type validation**: server-side MIME type check (not just extension)
- **Size limits enforced server-side**: reject before storing
- **Rate limiting**: max 10 media uploads per minute per user
- **Content moderation**: future — hash-based duplicate detection, optional AI moderation
- **Signed URLs**: media URLs should be signed with expiry for private rabbles
- **Cleanup**: when a rabble is ended/deleted, associated media is garbage-collected after 30 days

---

## Cost Model Summary

| Action | Credit cost | Revenue flow |
|--------|-----------|-------------|
| Send text | 1cr | Platform |
| Send image/audio | 1cr | Platform |
| Send short video (≤30s) | 1cr | Platform |
| Send long video (>30s) | 2cr | Platform |
| Create poll | 2cr | Platform |
| Vote in poll | Free | — |
| Share location | 1cr | Platform |
| Journal summary (agentic) | 3-5cr | Platform + Agent |

All media costs are designed to be low enough to not inhibit interaction,
high enough to prevent spam, and consistent with the existing credit model.