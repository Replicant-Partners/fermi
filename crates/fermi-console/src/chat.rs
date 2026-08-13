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
//! **Slice 1** shipped in v0.10.0: RAM-only chat drawer, `Ctrl+;`
//! toggle, multi-turn message history in memory, `execute_agent`
//! POST with envelope-prefixed query, `metadata.reasoning` extraction.
//!
//! **Slice 2 (v0.10.2):** action markers — Fermi can propose console
//! actions (open a forecast, navigate to a panel, run a simulation)
//! by embedding a fenced ```action JSON block in its reply. The
//! client parses these markers out of the reply text, hides them
//! from the transcript display, and renders each as a clickable
//! chip: `⚡ Open forecast a3b7…`. Chips are *proposed*, not
//! auto-executed — the operator retains agency ("click-to-cancel"
//! is the natural affordance: don't click). Dispatch runs on the
//! client side, so a wide range of UI-only actions is fair game
//! without adding server-side tool handlers or looping the
//! execute_agent LLM loop over tool_use rounds.
//!
//! Still to come:
//!   - **Slice 3** — chat persistence (server-side history table).
//!   - **Slice 4** — design mode (create-agent walk-through).
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
use crate::ui;

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

/// A single action proposal parsed from a fenced ```action block in
/// Fermi's reply. Slice 2 renders these as clickable chips beneath
/// the message; dispatch is client-side (`FermiConsole::execute_chat_action`).
///
/// The `tool` string is a stable identifier the dispatcher matches on.
/// `args` is a free-form JSON object — each tool defines its own
/// arg shape (see the docs on `execute_chat_action` in `main.rs`).
/// `reason` is optional human text Fermi can supply to explain WHY
/// it's proposing the action; rendered as a subtitle on the chip.
#[derive(Debug, Clone)]
pub struct ChatAction {
    pub tool: String,
    pub args: JsonValue,
    pub reason: Option<String>,
    /// Set true once the operator clicks the chip and dispatch runs,
    /// so the UI can swap the button for a "✓ done" marker instead
    /// of leaving the chip re-clickable (which would fire the action
    /// again — usually harmless, occasionally confusing).
    pub executed: bool,
    /// Set true if the operator explicitly dismisses the proposal.
    /// Also disables the button and greys the chip.
    pub dismissed: bool,
}

impl ChatAction {
    pub fn new(tool: impl Into<String>, args: JsonValue) -> Self {
        Self {
            tool: tool.into(),
            args,
            reason: None,
            executed: false,
            dismissed: false,
        }
    }
}

/// One entry in the chat transcript. Slice 1 uses `role`, `text`,
/// `created_at`; Slice 2 adds `actions` for the proposal chips.
/// Later slices fill in the rest.
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub text: String,
    pub created_at: DateTime<Utc>,

    // ── Slice 2 (tool dispatch) ─────────────────────────────
    /// Action chips parsed from `\`\`\`action` fenced blocks in the
    /// assistant's reply. Empty on user/error messages. Populated
    /// from `parse_actions` after `extract_reply_text`.
    pub actions: Vec<ChatAction>,

    // ── Slice 2 (tool dispatch, deeper) — currently unused ──────────
    /// When `role=Assistant` fires a server-side tool call, this
    /// captures the name + args so the UI can render a compact
    /// chip. Not yet wired since Slice 2 uses fenced action markers
    /// (client-side dispatch) rather than the Anthropic tool_use
    /// protocol (server-side dispatch inside the executor loop).
    #[allow(dead_code)]
    pub tool_call: Option<JsonValue>,
    /// When `role=Tool`, this captures the tool's result JSON.
    #[allow(dead_code)]
    pub tool_result: Option<JsonValue>,

    // ── Slice 4 (design mode) — currently unused ─────────────────
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
            actions: Vec::new(),
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
            actions: Vec::new(),
            tool_call: None,
            tool_result: None,
            design_step: None,
        }
    }

    /// Assistant message with the reply text AND parsed action chips.
    /// Actions are stripped from the visible text so the fenced JSON
    /// doesn't clutter the transcript — the operator sees prose + a
    /// chip strip, not raw JSON.
    pub fn assistant_with_actions(text: String, actions: Vec<ChatAction>) -> Self {
        Self {
            role: ChatRole::Assistant,
            text,
            created_at: Utc::now(),
            actions,
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
            actions: Vec::new(),
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
    /// Which tab of the Fermi panel is showing. The panel is the
    /// console's one right-edge surface for "talking to the system":
    /// [`FermiPanelTab::Chat`] asks Fermi questions,
    /// [`FermiPanelTab::Activity`] inspects what the system actually
    /// did. They share a panel deliberately — seeing an error and
    /// asking Fermi about it should be one click, not a context
    /// switch.
    pub tab: FermiPanelTab,

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
            tab: FermiPanelTab::Chat,
            session_id: None,
        }
    }
}

/// The two faces of the right-edge Fermi panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FermiPanelTab {
    /// Conversational surface over `execute_agent("fermi", ..)`.
    Chat,
    /// Structured, inspectable log of system interactions.
    Activity,
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
///
/// Slice 2 adds the ACTION-MARKER instruction block, which teaches
/// Fermi to embed structured action proposals in its replies so the
/// console can offer them as clickable chips. This is orthogonal to
/// Anthropic's tool_use protocol — we intentionally stay in the
/// LLM's text output rather than the server-executed tool loop, so
/// the operator always sees + approves the action before it fires.
pub fn wrap_query_with_envelope(envelope: &JsonValue, user_text: &str) -> String {
    let env_str = serde_json::to_string(envelope).unwrap_or_else(|_| "{}".into());
    format!(
        "{}\n\n[fermi_console_context] {}\n\n[operator] {}",
        ACTION_MARKER_INSTRUCTIONS, env_str, user_text
    )
}

/// The prompt segment that teaches Fermi to embed action markers.
/// Prepended to every operator message so the LLM has the format in
/// context each turn (system prompts aren't touched — we don't own
/// Fermi's card wholesale, this segment lives alongside operator
/// messages instead).
///
/// Kept intentionally concise: three short paragraphs and one
/// example. LLMs follow this format reliably when it's this clear.
const ACTION_MARKER_INSTRUCTIONS: &str = concat!(
    "[fermi_console_actions] When you want to propose a console action ",
    "(open a forecast, navigate a panel, run a simulation), embed a ",
    "fenced JSON block using the ```action language tag. Each block ",
    "is one action; you can include multiple blocks per reply. The ",
    "console renders each as a clickable chip — the operator decides ",
    "whether to fire it. Do NOT act without proposing first.\n\n",
    "Available tools:\n",
    "  - open_forecast          args: {\"forecast_id\": \"...\"}\n",
    "  - open_panel             args: {\"panel\": \"dashboard\"|\"portfolio\"|\"agent_fleet\"|\"composer\"|\"leaderboard\"|\"teams\"}\n",
    "  - run_simulation         args: {}\n",
    "  - search_polymarket      args: {\"query\": \"...\"}\n",
    // ── Model edits ────────────────────────────────────────────
    // These write to the forecast. They are what closes the loop:
    // without them Fermi could only ever describe the change it
    // wanted and leave the operator to type it in by hand.
    "  - set_driver_distribution  args: {\"driver\": \"...\", \"p5\": 0.8, \"p50\": 1.0, \"p95\": 1.3}\n",
    "  - set_driver_probability   args: {\"driver\": \"...\", \"probability\": 0.15, \"impact_multiplier\": 0.1}\n",
    "  - set_base_rate            args: {\"historical_frequency\": 0.24, \"reference_class\": \"...\", \"sample_size\": 34, \"reasoning\": \"...\"}\n",
    "  - assign_agent             args: {\"driver\": \"...\", \"agent_id\": \"football_analyst\"}\n\n",
    "Model-edit rules:\n",
    "  * Drivers are MULTIPLIERS on the base rate, centred on 1.0. ",
    "1.2 means +20%, 0.8 means -20%. Never send a percentage (65) where ",
    "a multiplier (0.65) belongs.\n",
    "  * p5 <= p50 <= p95 always. A backwards distribution is rejected.\n",
    "  * Use set_driver_probability ONLY for binary drivers and ",
    "set_driver_distribution ONLY for continuous ones.\n",
    "  * set_base_rate REQUIRES a reference_class, and the class must not ",
    "be the subject of the question. Prefer the broadest class that ",
    "shares the causal structure, and include sample_size.\n",
    "  * Propose set_driver_* when you have a specific number and a ",
    "reason for it. Say what changed and why in `reason` — the operator ",
    "is approving your arithmetic, so show it.\n",
    "  * After changing drivers, propose run_simulation as a separate ",
    "action so the operator can see the effect.\n\n",
    "Include a `reason` field for the operator so the chip explains ",
    "itself. Example:\n\n",
    "```action\n",
    "{\"tool\": \"open_forecast\", \"args\": {\"forecast_id\": \"a3b7f1e0-...\"}, \"reason\": \"You asked about Manchester City — this is the forecast you have on that.\"}\n",
    "```\n\n",
    "Rules: only propose actions the operator plausibly wants. Only ",
    "use forecast_ids that appear in the context envelope you were ",
    "given. If no action is relevant, just reply in prose without a ",
    "fenced action block.",
);

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

// ── Action parsing ───────────────────────────────────────────────────────────

/// Extract fenced ```action JSON blocks from an assistant reply.
/// Returns `(cleaned_text, actions)` where `cleaned_text` has the
/// fenced blocks stripped (leaving surrounding prose intact) so the
/// transcript shows readable prose, not raw JSON. Malformed blocks
/// are left in place — that way a JSON typo becomes visible instead
/// of silently disappearing.
///
/// Slice 2 lives on this parser being tolerant: we accept
/// ```action, ```json:action, and ```fermi_action as aliases; we
/// tolerate leading/trailing whitespace inside the block; and if the
/// JSON parses but doesn't have a recognised `tool` field, we still
/// keep it (the chip renders as "unknown", not lost).
pub fn parse_actions(reply: &str) -> (String, Vec<ChatAction>) {
    let mut actions = Vec::new();
    let mut cleaned = String::with_capacity(reply.len());
    let mut cursor = 0usize;

    while cursor < reply.len() {
        // Look for the next fenced block that starts with an action tag.
        let Some((start, tag_len)) = find_action_fence_start(&reply[cursor..]) else {
            cleaned.push_str(&reply[cursor..]);
            break;
        };
        let abs_start = cursor + start;
        // Copy everything before the fence to cleaned output.
        cleaned.push_str(&reply[cursor..abs_start]);

        // Find the closing ``` fence after the tag.
        let body_start = abs_start + tag_len;
        let Some(rel_end) = reply[body_start..].find("```") else {
            // Unterminated block — leave as-is in the transcript so the
            // operator can see the malformed input.
            cleaned.push_str(&reply[abs_start..]);
            break;
        };
        let body = reply[body_start..body_start + rel_end].trim();
        let end_after_fence = body_start + rel_end + 3; // + "```"

        // Try to parse body as JSON. On failure, leave the whole block
        // in the visible transcript (nothing to render as a chip).
        match serde_json::from_str::<JsonValue>(body) {
            Ok(mut v) => {
                // Support both `{"tool":..., "args":..., "reason":...}`
                // and flat `{"tool":..., "forecast_id":..., "reason":...}`
                // (some LLMs drift to the latter). Coerce flat shape
                // into nested by moving unknown top-level keys into args.
                let tool = v
                    .get("tool")
                    .and_then(|t| t.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let reason = v.get("reason").and_then(|r| r.as_str()).map(str::to_string);
                let args = if let Some(a) = v.get("args").cloned() {
                    a
                } else {
                    // Flatten mode: strip the recognised keys and pass
                    // the rest as args.
                    if let Some(obj) = v.as_object_mut() {
                        obj.remove("tool");
                        obj.remove("reason");
                    }
                    v
                };
                let mut action = ChatAction::new(tool, args);
                action.reason = reason;
                actions.push(action);
            }
            Err(e) => {
                log::warn!(
                    "[fermi-chat] action block parse failed: {} — body={:?}",
                    e,
                    body
                );
                cleaned.push_str(&reply[abs_start..end_after_fence]);
            }
        }

        cursor = end_after_fence;
    }

    // Collapse any triple-newline runs the removals produced.
    let cleaned = compact_blank_lines(&cleaned).trim().to_string();
    (cleaned, actions)
}

/// Find the byte offset + length of the opening fence of an action
/// block. Recognises `\`\`\`action`, `\`\`\`json:action`, and
/// `\`\`\`fermi_action`. Returns None if no such fence appears.
fn find_action_fence_start(text: &str) -> Option<(usize, usize)> {
    let candidates = ["```action", "```json:action", "```fermi_action"];
    let mut best: Option<(usize, usize)> = None;
    for tag in candidates {
        if let Some(pos) = text.find(tag) {
            let end = pos + tag.len();
            // Consume the following newline if present, so the body
            // starts on the next line and doesn't include the tag.
            let after = if text[end..].starts_with('\n') {
                end + 1
            } else {
                end
            };
            let tag_len = after - pos;
            best = match best {
                Some((p, _)) if p <= pos => best,
                _ => Some((pos, tag_len)),
            };
        }
    }
    best
}

/// Collapse runs of blank lines produced by stripping fenced blocks
/// out of the middle of a message.
fn compact_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blank_count = 0;
    for line in s.split_inclusive('\n') {
        if line.trim().is_empty() {
            blank_count += 1;
            if blank_count <= 1 {
                out.push_str(line);
            }
        } else {
            blank_count = 0;
            out.push_str(line);
        }
    }
    out
}

#[cfg(test)]
mod action_parsing_tests {
    use super::*;

    #[test]
    fn extracts_single_action_and_strips_from_text() {
        let reply = "Sure! Let me open that forecast for you.\n\n\
            ```action\n\
            {\"tool\": \"open_forecast\", \"args\": {\"forecast_id\": \"abc-123\"}, \"reason\": \"You asked about it\"}\n\
            ```\n\
            Then we can run a simulation.";
        let (text, actions) = parse_actions(reply);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].tool, "open_forecast");
        assert_eq!(
            actions[0].args.get("forecast_id").and_then(|v| v.as_str()),
            Some("abc-123")
        );
        assert_eq!(actions[0].reason.as_deref(), Some("You asked about it"));
        assert!(
            !text.contains("```"),
            "cleaned text still has fence: {}",
            text
        );
        assert!(text.contains("open that forecast"));
        assert!(text.contains("run a simulation"));
    }

    #[test]
    fn extracts_multiple_actions_in_one_reply() {
        let reply = "Two things:\n\n\
            ```action\n{\"tool\":\"open_panel\",\"args\":{\"panel\":\"portfolio\"}}\n```\n\
            ```action\n{\"tool\":\"run_simulation\",\"args\":{}}\n```\n\n\
            Done.";
        let (_, actions) = parse_actions(reply);
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].tool, "open_panel");
        assert_eq!(actions[1].tool, "run_simulation");
    }

    #[test]
    fn tolerates_flat_shape_without_nested_args() {
        // Some LLMs put args at the top level. Slice 2 coerces this
        // into the nested shape.
        let reply = "```action\n{\"tool\": \"open_forecast\", \"forecast_id\": \"xyz\"}\n```";
        let (_, actions) = parse_actions(reply);
        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].args.get("forecast_id").and_then(|v| v.as_str()),
            Some("xyz")
        );
    }

    #[test]
    fn leaves_malformed_json_visible_in_transcript() {
        let reply = "Hmm:\n```action\n{not valid json}\n```";
        let (text, actions) = parse_actions(reply);
        assert!(actions.is_empty());
        assert!(
            text.contains("```action"),
            "malformed block should stay visible: {}",
            text
        );
    }

    #[test]
    fn accepts_json_action_alias() {
        let reply =
            "```json:action\n{\"tool\":\"open_panel\",\"args\":{\"panel\":\"composer\"}}\n```";
        let (_, actions) = parse_actions(reply);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].tool, "open_panel");
    }

    #[test]
    fn no_action_blocks_returns_reply_unchanged() {
        let reply = "Just some prose about forecasting methodology.";
        let (text, actions) = parse_actions(reply);
        assert_eq!(text, reply);
        assert!(actions.is_empty());
    }
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
            theme::BORDER,
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
        .gap(ui::s(4.0))
        .px(ui::s(10.0))
        .py(ui::s(8.0))
        .rounded(ui::s(6.0))
        .bg(rgb(bg_color))
        .border_1()
        .border_color(rgb(border_color))
        .child(
            div()
                .flex()
                .items_center()
                .gap(ui::s(6.0))
                .child(
                    div()
                        .text_size(ui::TEXT_XS)
                        .text_color(rgb(label_color))
                        .font_weight(FontWeight::BOLD)
                        .child(label),
                )
                .child(
                    div()
                        .text_size(ui::TEXT_XS)
                        .text_color(theme::fg_muted())
                        .child(format_time(&msg.created_at)),
                ),
        )
        .child(
            div()
                .text_size(ui::TEXT_MD)
                .text_color(rgb(text_color))
                .child(msg.text.clone()),
        )
}

// NOTE: The drawer render itself lives on `FermiConsole` (see
// `render_fermi_chat_drawer` in main.rs) so its interactive elements
// can use `cx.listener` against the console. This module owns state,
// envelope construction, async send, and per-message rendering —
// which is all pure and doesn't need FermiConsole context.
