//! Activity panel — the view layer over [`fermi_activity`].
//!
//! The event model, ring buffer and coalescing all live in the
//! `fermi-activity` crate; see its module docs for why the log exists
//! and how it's shaped. This module owns only what needs GPUI:
//! severity colours, the row renderer, and the bridge from the
//! cockpit's `AssistantMessage`.
//!
//! The split exists because `fermi-console` is a bin crate whose GPUI
//! element chains exhaust rustc's stack during macro expansion under
//! `--test` — so unit tests placed here cannot run. Anything worth
//! testing belongs in `fermi-activity`.

use gpui::prelude::*;
use gpui::{div, px, rgb, Div};

use crate::theme;

// Re-exported so the rest of the console can keep referring to
// `activity_log::LogEvent`, `activity_log::Severity`, etc. without
// caring that the model moved into its own crate.
pub use fermi_activity::{ActivityLog, LogEvent, LogFilter, LogSource, Remedy, Severity};

// ── Presentation helpers ────────────────────────────────────────────

/// Theme colour for a severity. Lives here rather than on `Severity`
/// so the model crate stays free of any UI dependency.
fn severity_color(severity: Severity) -> u32 {
    match severity {
        Severity::Trace => theme::FG_FAINT,
        Severity::Info => theme::FG_DIM,
        Severity::Success => theme::GREEN,
        Severity::Warn => theme::GOLD,
        Severity::Error => theme::RED,
    }
}

// ── Cockpit bridge ──────────────────────────────────────────────────

/// Mirror a cockpit banner message into the log.
///
/// This is the bulk-ingest path: it lets all 92 existing
/// `messages.push(AssistantMessage { .. })` call sites in `cockpit.rs`
/// reach the Activity panel with no edits. The trade-off is that
/// `AssistantMessage` carries only `(node, kind, text)`, so mirrored
/// events have no detail, context or payload — call sites that have
/// more to say use `CockpitState::push_rich` instead.
///
/// A free function rather than a `From` impl: both `LogEvent` and the
/// conversion's shape now live outside this module's control, and the
/// orphan rule would force a newtype that buys nothing.
pub fn from_cockpit_message(msg: crate::cockpit::AssistantMessage) -> LogEvent {
    use crate::cockpit::MessageKind;
    let severity = match msg.kind {
        // Suggestions and tips are guidance, not incidents; they'd
        // drown the log at Info. Trace keeps them available under the
        // All filter without competing with real events.
        MessageKind::Suggestion | MessageKind::Tip => Severity::Trace,
        MessageKind::Info => Severity::Info,
        MessageKind::Warning => Severity::Warn,
        MessageKind::Error => Severity::Error,
    };
    LogEvent::new(severity, LogSource::from_node(&msg.node), msg.text).with_node(msg.node)
}

// ── Rendering ───────────────────────────────────────────────────────

fn format_clock(dt: &chrono::DateTime<chrono::Utc>) -> String {
    dt.with_timezone(&chrono::Local)
        .format("%H:%M:%S")
        .to_string()
}

/// Render one row. Pure — no listeners, no `cx`. `main.rs` wraps this
/// in the clickable container and appends the action strip, the same
/// split `chat::render_message` uses.
pub fn render_event(event: &LogEvent, expanded: bool) -> Div {
    let color = severity_color(event.severity);

    // Collapsed header: time · glyph · source chip · summary · xN · ⌄
    let mut header = div()
        .flex()
        .items_start()
        .gap(px(6.0))
        .child(
            div()
                .flex_shrink_0()
                .w(px(52.0))
                .text_size(px(9.0))
                .text_color(theme::fg_faint())
                .child(format_clock(&event.last_at)),
        )
        .child(
            div()
                .flex_shrink_0()
                .w(px(10.0))
                .text_size(px(10.0))
                .text_color(rgb(color))
                .child(event.severity.glyph()),
        )
        .child(
            div()
                .flex_shrink_0()
                .px(px(4.0))
                .rounded(px(3.0))
                .bg(theme::bg_elevated())
                .text_size(px(9.0))
                .text_color(theme::fg_dim())
                .child(event.source.label()),
        )
        .child(
            // The summary wraps instead of truncating. This is the
            // single biggest readability win over the old banner,
            // which chopped every message at 120–150 chars.
            div()
                .flex_grow()
                .min_w(px(0.0))
                .text_size(px(11.0))
                .text_color(rgb(color))
                .child(event.summary.clone()),
        );

    if event.count > 1 {
        header = header.child(
            div()
                .flex_shrink_0()
                .px(px(4.0))
                .rounded(px(3.0))
                .bg(rgb(theme::BG_ACTIVE))
                .text_size(px(9.0))
                .text_color(rgb(theme::GOLD))
                .child(format!("x{}", event.count)),
        );
    }

    if event.has_detail() {
        header = header.child(
            div()
                .flex_shrink_0()
                .text_size(px(9.0))
                .text_color(theme::fg_faint())
                .child(if expanded { "⌃" } else { "⌄" }),
        );
    }

    let row = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .px(px(8.0))
        .py(px(6.0))
        .rounded(px(4.0))
        .border_l_2()
        .border_color(rgb(color))
        .bg(if event.severity.is_problem() {
            rgb(theme::BG_ELEVATED)
        } else {
            rgb(theme::BG)
        })
        .child(header);

    if !expanded {
        return row;
    }

    // ── Expanded body ──────────────────────────────────────────
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .ml(px(62.0))
        .mt(px(2.0));

    if let Some(node) = &event.node {
        body = body.child(
            div()
                .text_size(px(9.0))
                .text_color(theme::fg_faint())
                .child(format!("node: {}", node)),
        );
    }

    if event.count > 1 {
        body = body.child(
            div()
                .text_size(px(9.0))
                .text_color(theme::fg_faint())
                .child(format!(
                    "first seen {} · last seen {} · {} occurrences",
                    format_clock(&event.at),
                    format_clock(&event.last_at),
                    event.count
                )),
        );
    }

    if !event.context.is_empty() {
        let mut table = div().flex().flex_col().gap(px(2.0));
        for (k, v) in &event.context {
            table = table.child(
                div()
                    .flex()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .w(px(88.0))
                            .text_size(px(9.0))
                            .text_color(theme::fg_faint())
                            .child(k.clone()),
                    )
                    .child(
                        div()
                            .flex_grow()
                            .min_w(px(0.0))
                            .text_size(px(10.0))
                            .text_color(theme::fg())
                            .child(v.clone()),
                    ),
            );
        }
        body = body.child(table);
    }

    if let Some(detail) = &event.detail {
        body = body.child(
            div()
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .bg(rgb(theme::BG_DEEP))
                .border_1()
                .border_color(theme::fg_faint())
                .text_size(px(10.0))
                .text_color(theme::fg())
                .child(detail.clone()),
        );
    }

    if let Some(remedy) = &event.remedy {
        body = body.child(
            div()
                .text_size(px(9.0))
                .text_color(theme::fg_dim())
                .child(format!("→ {}", remedy.rationale())),
        );
    }

    if let Some(payload) = &event.payload {
        let pretty = serde_json::to_string_pretty(payload).unwrap_or_else(|_| payload.to_string());
        // Payloads can be enormous (a full agent result). Cap the
        // inline view; the Copy button always exports the whole thing.
        let shown: String = if pretty.chars().count() > 2000 {
            let head: String = pretty.chars().take(2000).collect();
            format!("{}\n… (truncated — use ⧉ Copy for the full payload)", head)
        } else {
            pretty
        };
        body = body.child(
            div()
                .px(px(8.0))
                .py(px(6.0))
                .rounded(px(4.0))
                .bg(rgb(theme::BG_DEEP))
                .border_1()
                .border_color(theme::fg_faint())
                .text_size(px(9.0))
                .text_color(theme::fg_dim())
                .child(shown),
        );
    }

    row.child(body)
}
