//! Fermi Chat — the conversational surface for the Fermi agent.
//!
//! Per `docs/fermi/FERMI_CHAT_AND_AGENT_CREATION_DESIGN.md`, Fermi is
//! an ABW agent (`agents/curated/fermi/agent_card.json`), not a bespoke
//! console subsystem. Chat is a UI pattern that invokes the standard
//! `execute_agent` endpoint against `agent_id="fermi"`, threading a
//! compact **context envelope** describing "what the operator is
//! looking at right now" so Fermi can answer with situational
//! awareness.
//!
//! **Slice 1 (this module):** RAM-only chat drawer.
//!   - `Ctrl+;` toggles a right-edge slide-in drawer.
//!   - Multi-turn message history in memory; lost on restart.
//!   - Send-message flow: POST `/api/agents/fermi/execute` with the
//!     envelope-prefixed query.
//!   - Response's `metadata.reasoning` renders as an Assistant
//!     message; failures render as a system-styled error message.
//!   - **No tool dispatch** — Fermi's replies are text-only. Console-
//!     scoped MCP tools (`open_forecast`, `run_simulation`, …) come
//!     in Slice 2.
//!   - **No persistence** — chat_messages plumbing comes in Slice 3.
//!   - **No design mode** — the create-agent walk-through comes in
//!     Slice 4.
//!
//! Fields for later slices are already scaffolded here (`session_id`,
//! `tool_call`, `tool_result`, `design_step`) so the shape doesn't
//! churn when we ship them — just fill them in.

use chrono::{DateTime, Utc};
use gpui::prelude::*;
use gpui::*;
use serde_json::{json, Value as JsonValue};

use crate::text_input::TextInput;
use crate::theme;

// ── Message shape ────────────────────────────────────────────────────────

/// Who spoke. Matches the standard chat role trichotomy Anthropic /
/// OpenAI use; keeps the door open for tool-role messages in Slice 2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChatRole {
    User,
    Assistant,
    /// Tool result rendered inline in the chat pane. Not yet emitted
    /// in Slice 1, but the render path is wired so Slice 2 doesn't
    /// have to touch the styling.
    #[allow(dead_code)]
    Tool,
    /// Client-side error surfaced as a distinct role so the UI can
    /// style it differently from Fermi's own messages. Not part of
    /// the on-wire role space — never sent to the server.
    Error,
}

/// One entry in the chat transcript. Slice 1 uses `role`, `text`,
/// `created_at`. Later slices fill in the rest.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub text: String,
    pub created_at: DateTime<Utc>,

    // ── Slice 2 (tool dispatch) — currently unused ───────────────────
    /// When `role=Assistant` fires a tool call, this captures the
    /// name + args so the UI can render a compact chip.
    #[allow(dead_code)]
    pub tool_call: Option<JsonValue>,
    /// When `role=Tool`, this captures the tool's result JSON.
    #[allow(dead_code)]
    pub tool_result: Option<JsonValue>,

    // ── Slice 4 (design mode) — currently unused ─────────────────────
    /// 1–9 during the create-agent walk-through, so the UI can show
    /// a progress indicator without introducing a parallel
    /// "conversation kind" concept.
    #[allow(dead_code)]
    pub design_step: Option<u8>,
}

impl ChatMessage {
    pub fn user(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::User,
            text: text.into(),
            created_at: Utc::now(),
            tool_call: None,
            tool_result: None,
            design_step: None,
        }
    }

    pub fn assistant(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Assistant,
            text: text.into(),
            created_at: Utc::now(),
            tool_call: None,
            tool_result: None,
            design_step: None,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            role: ChatRole::Error,
            text: text.into(),
            created_at: Utc::now(),
            tool_call: None,
            tool_result: None,
            design_step: None,
        }
    }
}

// ── Chat state ───────────────────────────────────────────────────────────

/// Per-session Fermi Chat state. Lives on `FermiConsole` (peer of
/// `CockpitState` / `ComposerState`) — chat spans surfaces, so it
/// can't live inside the composer.
pub struct FermiChatState {
    pub messages: Vec<ChatMessage>,
    pub input: Entity<TextInput>,
    pub loading: bool,
    pub drawer_open: bool,

    // ── Slice 3 (persistence) — currently unused ─────────────────────
    /// Server-side session key. `None` means "no persisted history";
    /// Slice 3 will populate this from a `/api/agents/fermi/messages`
    /// bootstrap.
    #[allow(dead_code)]
    pub session_id: Option<String>,
}

impl FermiChatState {
    pub fn new(cx: &mut App) -> Self {
        let input = cx.new(|cx| {
            TextInput::new(cx).with_placeholder("Ask Fermi anything about your forecast…")
        });
        Self {
            messages: Vec::new(),
            input,
            loading: false,
            drawer_open: false,
            session_id: None,
        }
    }
}

// ── Context envelope ─────────────────────────────────────────────────────

/// Build the compact JSON envelope Fermi receives on every turn.
/// Slice 1 shape (from the design doc's §Context envelope):
///
/// ```json
/// {
///   "surface": "composer" | "portfolio" | "dashboard" | …,
///   "forecast_id": "...",
///   "forecast_question": "...",
///   "predicted_probability": 0.42,
///   "drivers": [{"name": "...", "kind": "continuous", "assigned_agent": null}, …],
///   "pm_link": { "event_id": "...", "market_id": "...", "market_price": 0.4 } | null,
///   "portfolios": ["EPL", "Company performance"],
///   "user_display_name": "Ivan"
/// }
/// ```
///
/// Callers pass whatever they have; missing pieces are omitted rather
/// than sent as `null` so the payload stays small.
pub fn build_context_envelope(
    surface: &str,
    forecast_id: Option<&str>,
    forecast_question: Option<&str>,
    predicted_probability: Option<f64>,
    drivers: &[(String, String, Option<String>)],
    pm_link: Option<JsonValue>,
    portfolios: &[String],
    user_display_name: Option<&str>,
) -> JsonValue {
    let mut env = serde_json::Map::new();
    env.insert("surface".into(), json!(surface));
    if let Some(fid) = forecast_id {
        env.insert("forecast_id".into(), json!(fid));
    }
    if let Some(q) = forecast_question {
        env.insert("forecast_question".into(), json!(q));
    }
    if let Some(p) = predicted_probability {
        env.insert("predicted_probability".into(), json!(p));
    }
    if !drivers.is_empty() {
        let arr: Vec<JsonValue> = drivers
            .iter()
            .map(|(name, kind, agent)| {
                json!({
                    "name": name,
                    "kind": kind,
                    "assigned_agent": agent,
                })
            })
            .collect();
        env.insert("drivers".into(), JsonValue::Array(arr));
    }
    if let Some(pm) = pm_link {
        env.insert("pm_link".into(), pm);
    }
    if !portfolios.is_empty() {
        env.insert("portfolios".into(), json!(portfolios));
    }
    if let Some(name) = user_display_name {
        env.insert("user".into(), json!(name));
    }
    JsonValue::Object(env)
}

/// Wrap the operator's message with the envelope in a compact form
/// Fermi's system prompt can lean on. Fermi is a Tetlock-savvy
/// orchestra conductor — it already knows what to do with a
/// `forecast_question` + `drivers` block. The tag + JSON prefix keeps
/// the message parsable without training changes.
pub fn wrap_query_with_envelope(envelope: &JsonValue, user_text: &str) -> String {
    let env_str = serde_json::to_string(envelope).unwrap_or_else(|_| "{}".into());
    format!(
        "[fermi_console_context] {}\n\n[operator] {}",
        env_str, user_text
    )
}

// ── Response extraction ──────────────────────────────────────────────────

/// Pull the assistant's textual reply out of an `AgentExecutionResult`
/// JSON. `metadata.reasoning` is the primary field for a conversational
/// agent; evidence summaries + tool invocations are secondary. Falls
/// back through the plausible shapes so a server-shape change
/// somewhere below us doesn't silently render empty messages.
pub fn extract_reply_text(raw: &JsonValue) -> String {
    // Primary: metadata.reasoning (what execute_agent_handler emits for
    // llm-executor agents in Fermi's category).
    if let Some(reasoning) = raw
        .get("metadata")
        .and_then(|m| m.get("reasoning"))
        .and_then(|v| v.as_str())
    {
        if !reasoning.trim().is_empty() {
            return reasoning.to_string();
        }
    }

    // Secondary: concatenate evidence summaries. Useful if Fermi
    // decomposes into an orchestra call — each specialist's summary
    // is a paragraph.
    if let Some(arr) = raw.get("evidence").and_then(|v| v.as_array()) {
        let joined = arr
            .iter()
            .filter_map(|e| e.get("summary").and_then(|s| s.as_str()))
            .collect::<Vec<_>>()
            .join("\n\n");
        if !joined.trim().is_empty() {
            return joined;
        }
    }

    // Tertiary: some paths (streaming buffer, direct-anthropic fallback)
    // stash the text at the top level.
    for k in ["text", "response", "answer", "output", "final_answer"] {
        if let Some(s) = raw.get(k).and_then(|v| v.as_str()) {
            if !s.trim().is_empty() {
                return s.to_string();
            }
        }
    }

    // Last resort: status + any error metadata the server surfaced.
    let status = raw
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("(unknown status)");
    format!(
        "Fermi replied with no readable text (status: {}). This usually \
         means the executor ran but the assistant produced no reasoning \
         — try rephrasing.",
        status
    )
}

// ── Rendering ───────────────────────────────────────────────────────────────

/// Format a chat message's timestamp as HH:MM local for the message
/// header. Keeps the timeline scannable without cluttering short
/// messages.
fn format_time(dt: &DateTime<Utc>) -> String {
    let local = dt.with_timezone(&chrono::Local);
    local.format("%H:%M").to_string()
}

/// Render one message bubble. Role drives colour + label.
/// `pub(crate)` so `main.rs::render_fermi_chat_drawer` can call it.
pub(crate) fn render_message(msg: &ChatMessage) -> impl IntoElement {
    let (label, label_color, bg_color, border_color, text_color) = match msg.role {
        ChatRole::User => (
            "You",
            theme::CYAN,
            theme::BG_ELEVATED,
            theme::FG_FAINT,
            theme::FG,
        ),
        ChatRole::Assistant => ("Fermi", theme::PURPLE, 0x1A1A2E, theme::PURPLE, theme::FG),
        ChatRole::Tool => (
            "Tool",
            theme::GOLD,
            theme::BG_ELEVATED,
            theme::GOLD,
            theme::FG_DIM,
        ),
        ChatRole::Error => (
            "Error",
            theme::RED,
            theme::BG_ELEVATED,
            theme::RED,
            theme::FG,
        ),
    };

    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .px(px(10.0))
        .py(px(8.0))
        .rounded(px(6.0))
        .bg(rgb(bg_color))
        .border_1()
        .border_color(rgb(border_color))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(rgb(label_color))
                        .font_weight(FontWeight::BOLD)
                        .child(label),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(theme::fg_faint())
                        .child(format_time(&msg.created_at)),
                ),
        )
        .child(
            div()
                .text_size(px(12.0))
                .text_color(rgb(text_color))
                .child(msg.text.clone()),
        )
}

// NOTE: The drawer render itself lives on `FermiConsole` (see
// `render_fermi_chat_drawer` in main.rs) so its interactive elements
// can use `cx.listener` against the console. This module owns state,
// envelope construction, async send, and per-message rendering —
// which is all pure and doesn't need FermiConsole context.
