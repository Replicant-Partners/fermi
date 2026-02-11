# Voice Features - Design Document

**Status**: Phase 1 Implemented (TTS only), Deferred for future sprints  
**Created**: 2026-02-11  
**Last Updated**: 2026-02-11

---

## Overview

Voice AI integration for Agent Bestiary using Cartesia Sonic for text-to-speech synthesis. Enables agents to generate natural speech output for narratives, summaries, and conversational interfaces.

**Decision**: Cartesia Sonic chosen over ElevenLabs/PlayHT/Smallest.ai for:
- Best latency (90ms time-to-first-audio)
- Lowest cost ($0.50/1M chars vs ElevenLabs $2.50/1M)
- Purpose-built for real-time dialogue

---

## Phase 1: Text-to-Speech (Implemented)

### ✅ What's Built

**Voice Module** (`src/voice/`)
- `CartesiaClient` - API client for Cartesia Sonic
- 3 voice styles:
  - `narrator` - British Narrator (ID: `79a125e8-cd45-4c13-8a67-188112f4dd22`)
  - `conversational` - Friendly Guy (ID: `a0e99841-438c-4a64-b679-ae501e7d6091`)
  - `storyteller` - Calm Woman (ID: `71a7ad14-091c-4e8e-a314-022ece01c121`)

**speak_text Tool**
```json
{
  "name": "speak_text",
  "description": "Convert text to natural speech using Cartesia Sonic",
  "input_schema": {
    "type": "object",
    "properties": {
      "text": { "type": "string", "description": "Max 5000 characters" },
      "voice": { 
        "type": "string", 
        "enum": ["narrator", "conversational", "storyteller"],
        "default": "narrator"
      }
    }
  }
}
```

**Returns:**
```json
{
  "audio": "base64_encoded_pcm_f32le_data",
  "format": "pcm_f32le",
  "sample_rate": 44100,
  "duration_ms": 4800,
  "character_count": 125
}
```

**Database Schema** (Migration 048)
```sql
CREATE TABLE voice_assets (
    asset_id UUID PRIMARY KEY,
    object_type TEXT NOT NULL,     -- 'episode', 'message', 'creature', 'synopsis'
    object_id TEXT NOT NULL,
    provider TEXT NOT NULL,         -- 'cartesia', 'elevenlabs'
    voice_id TEXT,
    duration_ms INTEGER,
    character_count INTEGER NOT NULL,
    storage_url TEXT NOT NULL,      -- R2/S3 URL
    created_at TIMESTAMPTZ DEFAULT NOW()
);

ALTER TABLE episodes ADD COLUMN audio_url TEXT;
ALTER TABLE workspace_messages ADD COLUMN audio_url TEXT;
ALTER TABLE ontology_snapshots ADD COLUMN audio_url TEXT;
```

**Gas Fees**
- `voice_synthesis: 2` credits per synthesis
- Covers Cartesia API cost ($0.50/1M chars) + 10% platform gas + margin

**Environment Variables**
- `CARTESIA_API_KEY` - API key (set in Railway)
- `GAS_VOICE_SYNTHESIS` - Gas fee override (default: 2)

---

## Phase 2: Audio Storage & Playback (Deferred)

### Storage Backend

**Cloudflare R2** (S3-compatible, zero egress fees)
- Bucket: `abw-voice-assets`
- Path structure: `{object_type}/{object_id}/{timestamp}.pcm`
- Convert PCM → MP3 for web playback (ffmpeg)

### API Endpoints

```rust
// POST /api/voice/synthesize
// Quick test endpoint for voice generation
pub async fn voice_synthesize_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Json(body): Json<SynthesizeRequest>,
) -> Result<Json<SynthesizeResponse>, (StatusCode, String)>

// POST /api/agents/:id/speak
// Synthesize agent output to speech
pub async fn agent_speak_handler(
    State(state): State<AppState>,
    principal: AuthPrincipal,
    Path(agent_id): Path<String>,
    Json(body): Json<SpeakRequest>,
) -> Result<Json<Value>, (StatusCode, String)>
```

### Workspace UI

**Audio Player Component**
- Inline player for voice-enabled messages
- Play/pause, progress bar, download button
- Auto-play option for dream synopses

---

## Phase 3: Speech-to-Text (Deferred)

### Provider: Deepgram Nova-2

**Why Deepgram:**
- Best accuracy-to-cost ratio
- Real-time streaming support
- Speaker diarization (multi-speaker)
- Competitive with AssemblyAI, faster than Whisper

**New Tool: `listen_transcribe`**
```json
{
  "name": "listen_transcribe",
  "description": "Transcribe audio to text using Deepgram",
  "input_schema": {
    "type": "object",
    "properties": {
      "audio_url": { "type": "string" },
      "language": { "type": "string", "default": "en" },
      "speaker_labels": { "type": "boolean", "default": false }
    }
  }
}
```

**Gas Fee:** `voice_transcribe: 1` credit per minute

---

## Integration Scenarios

### 1. Voice-Enabled Dream Narrator ⭐ **Recommended First Use Case**

**Agent:** `dream_narrator`  
**Capability:** Speak dream synopses after consolidation

```json
// agents/curated/dream_narrator/agent_card.json
{
  "agent_id": "dream_narrator",
  "capabilities": {
    "executor": "llm",
    "model": "claude-haiku-4-5",
    "voice_enabled": true,
    "voice_provider": "cartesia",
    "voice_id": "71a7ad14-091c-4e8e-a314-022ece01c121" // Storyteller
  }
}
```

**Workflow:**
1. Consolidation completes → dream synopsis generated
2. Auto-trigger: call `speak_text` with synopsis
3. Store audio in R2 → save `audio_url` to `ontology_snapshots`
4. UI shows "Listen to Dream" button

### 2. Rabble Voice Chat (Deferred)

**Creature Narration:**
- Speak creature descriptions in naturalist voice
- Audio field notes for specimens
- Swarm host agent voice interactions

**Flutter Integration:**
- `flutter_sound` package for playback
- Stream from Cartesia WebSocket
- Cache in `static/creatures/audio/`

### 3. Notebook Voice Cells (Deferred)

**Fermi Notebook Extensions:**
- `VoiceQuestionCell` - Speak forecasting questions
- `VoiceNoteCell` - Record audio research notes
- `PodcastCell` - Generate audio forecast summaries

**Flow:** User speaks → Deepgram STT → FPL execution → Cartesia TTS → Audio output

---

## Cost Analysis

**Per-synthesis cost** (500 characters average):

| Provider | Cost/1M chars | Cost/500 chars | With 10% gas | Credits |
|----------|---------------|----------------|--------------|---------|
| **Cartesia** | $0.50 | $0.00025 | $0.000275 | **1cr** |
| ElevenLabs | $2.50 | $0.00125 | $0.001375 | 3cr |
| Smallest.ai | $0.10/min | ~$0.0002 | $0.00022 | 1cr |

**Gas Model:** 2 credits covers API cost + 10% platform gas + margin.

---

## Technical Notes

### Cartesia API Details

**Endpoint:** `POST https://api.cartesia.ai/tts/bytes`  
**Headers:**
- `X-API-Key: {CARTESIA_API_KEY}`
- `Cartesia-Version: 2024-06-10`

**Request:**
```json
{
  "model_id": "sonic-english",
  "transcript": "Your text here",
  "voice": {
    "mode": "id",
    "id": "79a125e8-cd45-4c13-8a67-188112f4dd22"
  },
  "output_format": {
    "container": "raw",
    "encoding": "pcm_f32le",
    "sample_rate": 44100
  }
}
```

**Response:** Binary PCM audio stream

### Audio Format Conversion

PCM → MP3 (for web playback):
```bash
ffmpeg -f f32le -ar 44100 -ac 1 -i input.pcm output.mp3
```

Or use `wasm-audio` in browser for client-side conversion.

---

## Future Enhancements

### Multi-Voice Conversations
- Different voices for different agents in workspace
- Voice personality matching (e.g., macro_forecaster = authoritative)

### Voice Cloning
- User custom voices via Cartesia Instant Clone
- Agent-specific voice profiles

### Real-Time Voice Agents
- WebSocket streaming for live conversations
- Voice-first workspace interface
- Phone integration via Twilio

### Multilingual Support
- Cartesia supports 15 languages
- Per-agent language preferences
- Auto-detect language from text

---

## References

- [Cartesia Sonic](https://cartesia.ai/sonic) - Official docs
- [Voice AI Architecture Guide 2026](https://www.teamday.ai/blog/voice-ai-architecture-guide-2026)
- [Cartesia vs ElevenLabs Comparison](https://cartesia.ai/vs/cartesia-vs-elevenlabs)
- GLOSSARY.md - Add voice terms when system goes live
- WALLET_SYSTEM.md - Gas fee model

---

## Decision Log

**2026-02-11:** Cartesia Sonic chosen over alternatives  
**2026-02-11:** Phase 1 (TTS only) implemented, Phases 2-3 deferred  
**2026-02-11:** 2 credit gas fee set based on cost analysis  
**2026-02-11:** Deferred audio storage/playback until user demand established
