//! Activity event model for the Fermi Console's system-interaction
//! inspector.
//!
//! ## Why this exists
//!
//! Before this crate the console had two disjoint, lossy notification
//! surfaces:
//!
//!   1. `CockpitState.messages` (92 push sites) rendered through
//!      `cockpit::render_fermi_banner` — a 3-line strip that filtered
//!      out every `Info` message, showed only the two most recent
//!      non-info entries, and truncated them at 120/150 chars.
//!   2. `FermiConsole::show_toast` (23 call sites) — a bottom-right
//!      pill that auto-dismissed after 3 seconds with zero history.
//!
//! Everything auth-, RBAC-, team- and updater-shaped went to (2), so
//! precisely the failures an operator most needs to debug were the
//! ones that vanished fastest. Meanwhile the console *already*
//! computed genuinely good diagnostics — `friendly_backend_save_error`
//! has a six-branch error taxonomy, `format_self_check_diagnosis`
//! makes a live server round-trip to classify RBAC drift — and then
//! concatenated all of it into a single `String` whose remediation
//! half always landed past the banner's truncation point.
//!
//! [`ActivityLog`] is the durable, structured, inspectable sink both
//! surfaces now feed. A [`LogEvent`] keeps the one-line `summary`
//! separate from the `detail` prose, the `context` key/values, the raw
//! `payload`, and the machine-readable [`Remedy`] — so the panel can
//! render a scannable row that expands into a full debugging view
//! instead of a truncated sentence.
//!
//! ## Design notes
//!
//! * **Ring buffer, not unbounded.** `messages` grew forever. We cap at
//!   [`MAX_EVENTS`] and drop from the front.
//! * **Coalescing.** The motivating bug report showed the same backend
//!   save error three times, because the save path deliberately leaves
//!   `dirty` set so autosave retries. Identical `(source, summary)`
//!   pairs inside [`COALESCE_WINDOW_SECS`] collapse into one row with a
//!   `xN` counter and a last-seen timestamp.
//! * **No GPUI.** Rendering lives in the console's `activity_log`
//!   module. See this crate's `Cargo.toml` for why that matters.

use chrono::{DateTime, Utc};
use serde_json::Value as JsonValue;
use std::collections::{HashSet, VecDeque};

/// Hard cap on retained events. At ~10 events/minute of heavy agent
/// traffic this is roughly an hour of scrollback, which is well past
/// the point where an operator would rather re-run the thing than
/// scroll. Old events drop from the front.
pub const MAX_EVENTS: usize = 500;

/// Two events with the same `(source, summary)` inside this many
/// seconds are the same event happening again, not two things worth
/// two rows.
pub const COALESCE_WINDOW_SECS: i64 = 90;

/// How far back `push` scans for a coalescing partner. Bounded so push
/// stays cheap; deep enough that interleaved traffic (a market poll
/// landing between two autosave retries) doesn't defeat it.
const COALESCE_SCAN_DEPTH: usize = 12;

// ── Severity ────────────────────────────────────────────────────────

/// How much the operator should care. Ordered least → most severe so
/// `>=` comparisons work for filtering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// Routine chatter — polls, ticks, "started researching…".
    Trace,
    /// Something happened worth recording but nothing to act on.
    Info,
    /// An operation completed successfully.
    Success,
    /// Degraded, recoverable, or "we fell back to something".
    Warn,
    /// The operation did not happen. Work may be at risk.
    Error,
}

impl Severity {
    pub fn glyph(self) -> &'static str {
        match self {
            Severity::Trace => "·",
            Severity::Info => "ℹ",
            Severity::Success => "✓",
            Severity::Warn => "⚠",
            Severity::Error => "✗",
        }
    }

    /// True for the two severities the "Problems" filter keeps.
    pub fn is_problem(self) -> bool {
        matches!(self, Severity::Warn | Severity::Error)
    }
}

// ── Source ──────────────────────────────────────────────────────────

/// Which subsystem emitted the event. Drives the source chip on each
/// row and gives coalescing a coarse namespace so a save failure and
/// an agent failure with coincidentally identical text stay distinct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogSource {
    /// Local + backend persistence of the open forecast.
    Save,
    /// Publish / snapshot / share fan-out.
    Publish,
    /// A research agent run (SSE stream or direct execute).
    Agent(String),
    /// Monte Carlo run + recomposition.
    Simulation,
    /// Scheduled agent cadences.
    Schedule,
    /// Sign-in, tokens, RBAC provisioning.
    Auth,
    /// Polymarket linkage and price polling.
    Market,
    /// Cascade groups, redistribution, provenance.
    Cascade,
    /// Posterior fits and their accept/reject lifecycle.
    BayesOps,
    /// The Fermi chat drawer itself.
    Chat,
    /// Console-level: updater, teams, invites, navigation.
    System,
}

impl LogSource {
    pub fn label(&self) -> String {
        match self {
            LogSource::Save => "save".into(),
            LogSource::Publish => "publish".into(),
            LogSource::Agent(id) => format!("agent:{}", id),
            LogSource::Simulation => "sim".into(),
            LogSource::Schedule => "schedule".into(),
            LogSource::Auth => "auth".into(),
            LogSource::Market => "market".into(),
            LogSource::Cascade => "cascade".into(),
            LogSource::BayesOps => "bayesops".into(),
            LogSource::Chat => "chat".into(),
            LogSource::System => "system".into(),
        }
    }

    /// Map the `node` string the cockpit already tags every
    /// `AssistantMessage` with (`"save"`, `"driver:x"`, `"agent:y"`, …)
    /// onto a source. This is what lets the mirror classify all 92
    /// existing push sites without touching any of them.
    pub fn from_node(node: &str) -> Self {
        if let Some(rest) = node.strip_prefix("agent:") {
            return LogSource::Agent(rest.to_string());
        }
        match node {
            "save" => LogSource::Save,
            "publish" | "share" => LogSource::Publish,
            "simulation" | "model" => LogSource::Simulation,
            "schedule" => LogSource::Schedule,
            "market" | "polymarket" => LogSource::Market,
            "cascade" => LogSource::Cascade,
            "bayesops" => LogSource::BayesOps,
            _ => LogSource::System,
        }
    }
}

// ── Remedy ──────────────────────────────────────────────────────────

/// A machine-readable next step. The prose for these already existed
/// inside `friendly_backend_save_error`'s branches ("sign out and back
/// in first", "check GET /api/rbac/self-check", "Reset the composer
/// (Ctrl+N)") — it was just narrative text inside a string that got
/// truncated. Promoting it to a variant makes it a button.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Remedy {
    /// Re-run the operation that failed.
    Retry,
    /// The session JWT predates a backend migration.
    SignOut,
    /// Ask the server to classify the RBAC drift.
    RunSelfCheck,
    /// Local draft references a row the server dropped.
    ResetComposer,
    /// Nothing automatic — just make the details easy to paste.
    CopyDiagnostics,
}

impl Remedy {
    pub fn label(&self) -> &'static str {
        match self {
            Remedy::Retry => "↻ Retry",
            Remedy::SignOut => "⇥ Sign out & back in",
            Remedy::RunSelfCheck => "🩺 Run self-check",
            Remedy::ResetComposer => "⌫ Reset composer",
            Remedy::CopyDiagnostics => "⧉ Copy diagnostics",
        }
    }

    /// One line explaining why this is the suggested fix.
    pub fn rationale(&self) -> &'static str {
        match self {
            Remedy::Retry => "The failure looked transient.",
            Remedy::SignOut => {
                "Your session token was minted before the users-row backfill; \
                 a fresh sign-in remints it."
            }
            Remedy::RunSelfCheck => {
                "GET /api/rbac/self-check tells you definitively whether this is \
                 a stale JWT, a stale deploy, or a missing users row."
            }
            Remedy::ResetComposer => {
                "This draft points at a server row that no longer exists. \
                 Ctrl+N and paste your work into a fresh forecast."
            }
            Remedy::CopyDiagnostics => "Puts the full event on your clipboard.",
        }
    }
}

// ── Event ───────────────────────────────────────────────────────────

/// One inspectable system interaction.
///
/// The split between `summary` and `detail` is the whole point: the
/// summary is what the collapsed row shows and what coalescing keys
/// on, so it must be short and stable. Everything long, raw, or
/// variable — stack-shaped error text, server diagnoses, response
/// bodies — belongs in `detail` / `context` / `payload`, which are only
/// rendered when the operator expands the row and are never truncated.
#[derive(Debug, Clone)]
pub struct LogEvent {
    /// Monotonic id. Stable across coalescing, so it works as the
    /// expand/collapse key and as a GPUI element id.
    pub seq: u64,
    /// When the event first occurred.
    pub at: DateTime<Utc>,
    /// When it most recently recurred. Equals `at` unless `count > 1`.
    pub last_at: DateTime<Utc>,
    pub severity: Severity,
    pub source: LogSource,
    /// The FPL node the event is scoped to, if any (`"driver:price"`).
    /// Preserved verbatim from `AssistantMessage.node`.
    pub node: Option<String>,
    /// One line. Never truncated by the panel.
    pub summary: String,
    /// Multi-line prose: raw error text, server diagnosis, remediation.
    pub detail: Option<String>,
    /// Structured key/values: endpoint, HTTP status, forecast id,
    /// duration, credits charged.
    pub context: Vec<(String, String)>,
    /// Raw JSON we'd otherwise throw away — agent results, error bodies.
    pub payload: Option<JsonValue>,
    pub remedy: Option<Remedy>,
    /// How many times this event coalesced. 1 = happened once.
    pub count: u32,
}

impl LogEvent {
    pub fn new(severity: Severity, source: LogSource, summary: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            seq: 0, // assigned by ActivityLog::push
            at: now,
            last_at: now,
            severity,
            source,
            node: None,
            summary: summary.into(),
            detail: None,
            context: Vec::new(),
            payload: None,
            remedy: None,
            count: 1,
        }
    }

    /// Attach detail prose. Blank input is ignored so callers can pass
    /// a possibly-empty server field without producing an empty
    /// expansion panel.
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        let d = detail.into();
        if !d.trim().is_empty() {
            self.detail = Some(d);
        }
        self
    }

    pub fn with_node(mut self, node: impl Into<String>) -> Self {
        let n = node.into();
        if !n.is_empty() {
            self.node = Some(n);
        }
        self
    }

    /// Add a context row. Empty values are dropped — several call sites
    /// pass `unwrap_or_default()` on an optional server field, and a
    /// row reading `http_status:` with nothing after it is worse than
    /// no row.
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let v = value.into();
        if !v.is_empty() {
            self.context.push((key.into(), v));
        }
        self
    }

    pub fn with_payload(mut self, payload: JsonValue) -> Self {
        self.payload = Some(payload);
        self
    }

    pub fn with_remedy(mut self, remedy: Remedy) -> Self {
        self.remedy = Some(remedy);
        self
    }

    /// True when there's anything worth expanding the row for.
    pub fn has_detail(&self) -> bool {
        self.detail.is_some() || !self.context.is_empty() || self.payload.is_some()
    }

    /// Render the event as plain text for the clipboard and for the
    /// "Ask Fermi about this" hand-off. Deliberately verbose — this is
    /// the artefact that ends up in a bug report.
    pub fn to_plain_text(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "[{}] {} {} — {}\n",
            self.at
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S"),
            self.severity.glyph(),
            self.source.label(),
            self.summary
        ));
        if self.count > 1 {
            out.push_str(&format!(
                "Occurred {}x (last at {})\n",
                self.count,
                self.last_at
                    .with_timezone(&chrono::Local)
                    .format("%H:%M:%S")
            ));
        }
        if let Some(node) = &self.node {
            out.push_str(&format!("Node: {}\n", node));
        }
        if !self.context.is_empty() {
            out.push_str("\nContext:\n");
            for (k, v) in &self.context {
                out.push_str(&format!("  {}: {}\n", k, v));
            }
        }
        if let Some(detail) = &self.detail {
            out.push_str(&format!("\nDetail:\n{}\n", detail));
        }
        if let Some(remedy) = &self.remedy {
            out.push_str(&format!(
                "\nSuggested fix: {} — {}\n",
                remedy.label(),
                remedy.rationale()
            ));
        }
        if let Some(payload) = &self.payload {
            let pretty =
                serde_json::to_string_pretty(payload).unwrap_or_else(|_| payload.to_string());
            out.push_str(&format!("\nPayload:\n{}\n", pretty));
        }
        out
    }
}

// ── Filter ──────────────────────────────────────────────────────────

/// Which events the panel shows. `Problems` is the debugging default
/// once something has gone wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFilter {
    All,
    Problems,
}

// ── The log ─────────────────────────────────────────────────────────

/// Bounded, coalescing, app-scoped event store. Lives on
/// `FermiConsole` so it outlives any single `CockpitState` and can
/// capture pre-composer events (sign-in, updater, team ops).
pub struct ActivityLog {
    events: VecDeque<LogEvent>,
    next_seq: u64,
    /// `seq` values whose rows are expanded.
    pub expanded: HashSet<u64>,
    pub filter: LogFilter,
    /// Problems logged since the operator last had the Activity tab
    /// open. Drives the sidebar badge and the banner chip.
    pub unseen_problems: u32,
}

impl Default for ActivityLog {
    fn default() -> Self {
        Self::new()
    }
}

impl ActivityLog {
    pub fn new() -> Self {
        Self {
            events: VecDeque::new(),
            next_seq: 1,
            expanded: HashSet::new(),
            filter: LogFilter::All,
            unseen_problems: 0,
        }
    }

    /// Append an event, coalescing against a recent identical one.
    pub fn push(&mut self, mut event: LogEvent) {
        let now = Utc::now();
        for existing in self.events.iter_mut().rev().take(COALESCE_SCAN_DEPTH) {
            let same = existing.source == event.source
                && existing.summary == event.summary
                && existing.severity == event.severity;
            let recent = (now - existing.last_at).num_seconds() <= COALESCE_WINDOW_SECS;
            if same && recent {
                existing.count += 1;
                existing.last_at = now;
                // A repeat often carries fresher context (a new
                // request id, a different retry count). Prefer the
                // newest detail so the expanded view isn't stale.
                if event.detail.is_some() {
                    existing.detail = event.detail.take();
                }
                if !event.context.is_empty() {
                    existing.context = std::mem::take(&mut event.context);
                }
                if event.payload.is_some() {
                    existing.payload = event.payload.take();
                }
                if event.severity.is_problem() {
                    self.unseen_problems = self.unseen_problems.saturating_add(1);
                }
                return;
            }
        }

        event.seq = self.next_seq;
        self.next_seq += 1;
        if event.severity.is_problem() {
            self.unseen_problems = self.unseen_problems.saturating_add(1);
        }
        self.events.push_back(event);
        while self.events.len() > MAX_EVENTS {
            if let Some(dropped) = self.events.pop_front() {
                // Otherwise the expanded set grows without bound and
                // a recycled seq could render pre-expanded.
                self.expanded.remove(&dropped.seq);
            }
        }
    }

    /// Newest-first, honouring the active filter. The panel renders
    /// newest-first so the thing that just broke is at eye level
    /// without scrolling.
    pub fn visible(&self) -> Vec<&LogEvent> {
        self.events
            .iter()
            .rev()
            .filter(|e| match self.filter {
                LogFilter::All => true,
                LogFilter::Problems => e.severity.is_problem(),
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Total warn+error rows currently retained (not the unseen count).
    pub fn problem_count(&self) -> usize {
        self.events
            .iter()
            .filter(|e| e.severity.is_problem())
            .count()
    }

    pub fn get(&self, seq: u64) -> Option<&LogEvent> {
        self.events.iter().find(|e| e.seq == seq)
    }

    pub fn toggle_expanded(&mut self, seq: u64) {
        if !self.expanded.remove(&seq) {
            self.expanded.insert(seq);
        }
    }

    /// Called when the Activity tab becomes visible.
    pub fn mark_seen(&mut self) {
        self.unseen_problems = 0;
    }

    pub fn clear(&mut self) {
        self.events.clear();
        self.expanded.clear();
        self.unseen_problems = 0;
    }

    /// Whole-log export for bug reports.
    pub fn to_plain_text(&self) -> String {
        let mut out = format!(
            "Fermi Console activity log — {} event(s), generated {}\n\
             ════════════════════════════════════════════════════\n\n",
            self.events.len(),
            Utc::now()
                .with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
        );
        for e in &self.events {
            out.push_str(&e.to_plain_text());
            out.push_str("────────────────────────────────────────────────────\n");
        }
        out
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(sev: Severity, src: LogSource, summary: &str) -> LogEvent {
        LogEvent::new(sev, src, summary)
    }

    #[test]
    fn assigns_monotonic_sequence_numbers() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Info, LogSource::System, "one"));
        log.push(ev(Severity::Info, LogSource::System, "two"));
        // visible() is newest-first.
        let seqs: Vec<u64> = log.visible().iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![2, 1]);
    }

    #[test]
    fn coalesces_identical_consecutive_events() {
        // The motivating case: autosave retries the same failing save
        // three times and the old banner rendered three lines.
        let mut log = ActivityLog::new();
        for _ in 0..3 {
            log.push(ev(Severity::Error, LogSource::Save, "Backend save failed"));
        }
        assert_eq!(log.len(), 1, "three identical errors should be one row");
        assert_eq!(log.visible()[0].count, 3);
    }

    #[test]
    fn coalesces_across_interleaved_traffic() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Error, LogSource::Save, "Backend save failed"));
        log.push(ev(Severity::Trace, LogSource::Market, "price tick"));
        log.push(ev(Severity::Trace, LogSource::Market, "price tick"));
        log.push(ev(Severity::Error, LogSource::Save, "Backend save failed"));
        assert_eq!(log.len(), 2, "save error + market tick = two rows");
        let save = log
            .visible()
            .into_iter()
            .find(|e| e.source == LogSource::Save)
            .unwrap();
        assert_eq!(save.count, 2);
    }

    #[test]
    fn does_not_coalesce_across_sources() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Error, LogSource::Save, "failed"));
        log.push(ev(Severity::Error, LogSource::Publish, "failed"));
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn does_not_coalesce_across_severity() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Warn, LogSource::Save, "same text"));
        log.push(ev(Severity::Error, LogSource::Save, "same text"));
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn does_not_coalesce_distinct_agents() {
        let mut log = ActivityLog::new();
        log.push(ev(
            Severity::Error,
            LogSource::Agent("macro_forecaster".into()),
            "run failed",
        ));
        log.push(ev(
            Severity::Error,
            LogSource::Agent("regulatory_monitor".into()),
            "run failed",
        ));
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn coalescing_keeps_freshest_detail_and_payload() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Error, LogSource::Chat, "500").with_detail("first body"));
        log.push(
            ev(Severity::Error, LogSource::Chat, "500")
                .with_detail("second body")
                .with_payload(serde_json::json!({"attempt": 2})),
        );
        let e = log.visible()[0];
        assert_eq!(e.count, 2);
        assert_eq!(e.detail.as_deref(), Some("second body"));
        assert_eq!(e.payload, Some(serde_json::json!({"attempt": 2})));
    }

    #[test]
    fn coalescing_preserves_first_seen_timestamp() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Error, LogSource::Save, "boom"));
        let first_at = log.visible()[0].at;
        log.push(ev(Severity::Error, LogSource::Save, "boom"));
        let e = log.visible()[0];
        assert_eq!(e.at, first_at, "`at` must remain the first occurrence");
        assert!(e.last_at >= e.at);
    }

    #[test]
    fn ring_buffer_caps_and_drops_oldest() {
        let mut log = ActivityLog::new();
        for i in 0..(MAX_EVENTS + 25) {
            // Distinct summaries so nothing coalesces.
            log.push(ev(
                Severity::Info,
                LogSource::System,
                &format!("event {}", i),
            ));
        }
        assert_eq!(log.len(), MAX_EVENTS);
        // Oldest survivor is event 25.
        assert!(log.visible().last().unwrap().summary.ends_with("event 25"));
    }

    #[test]
    fn problems_filter_keeps_only_warn_and_error() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Trace, LogSource::System, "t"));
        log.push(ev(Severity::Info, LogSource::System, "i"));
        log.push(ev(Severity::Success, LogSource::System, "s"));
        log.push(ev(Severity::Warn, LogSource::System, "w"));
        log.push(ev(Severity::Error, LogSource::System, "e"));
        assert_eq!(log.visible().len(), 5);
        log.filter = LogFilter::Problems;
        assert_eq!(log.visible().len(), 2);
        assert_eq!(log.problem_count(), 2);
    }

    #[test]
    fn unseen_problem_badge_counts_and_clears() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Info, LogSource::System, "quiet"));
        assert_eq!(log.unseen_problems, 0);
        log.push(ev(Severity::Error, LogSource::Save, "loud"));
        log.push(ev(Severity::Error, LogSource::Save, "loud")); // coalesced
        assert_eq!(log.unseen_problems, 2, "repeats still count as unseen");
        assert_eq!(log.len(), 2);
        log.mark_seen();
        assert_eq!(log.unseen_problems, 0);
    }

    #[test]
    fn expand_toggle_round_trips_and_survives_eviction() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Error, LogSource::Save, "boom"));
        let seq = log.visible()[0].seq;
        log.toggle_expanded(seq);
        assert!(log.expanded.contains(&seq));
        log.toggle_expanded(seq);
        assert!(!log.expanded.contains(&seq));

        log.toggle_expanded(seq);
        for i in 0..MAX_EVENTS {
            log.push(ev(
                Severity::Info,
                LogSource::System,
                &format!("fill {}", i),
            ));
        }
        assert!(
            !log.expanded.contains(&seq),
            "evicted rows must not leak into the expanded set"
        );
    }

    #[test]
    fn get_finds_by_seq_and_misses_gracefully() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Error, LogSource::Save, "boom"));
        let seq = log.visible()[0].seq;
        assert_eq!(log.get(seq).unwrap().summary, "boom");
        assert!(log.get(seq + 999).is_none());
    }

    #[test]
    fn clear_resets_everything() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Error, LogSource::Save, "boom"));
        log.toggle_expanded(1);
        log.clear();
        assert!(log.is_empty());
        assert_eq!(log.unseen_problems, 0);
        assert!(log.expanded.is_empty());
    }

    #[test]
    fn source_from_node_maps_cockpit_vocabulary() {
        assert_eq!(LogSource::from_node("save"), LogSource::Save);
        assert_eq!(LogSource::from_node("schedule"), LogSource::Schedule);
        assert_eq!(
            LogSource::from_node("agent:macro_forecaster"),
            LogSource::Agent("macro_forecaster".into())
        );
        // Driver-scoped messages have no dedicated subsystem.
        assert_eq!(LogSource::from_node("driver:price"), LogSource::System);
        assert_eq!(LogSource::from_node("question"), LogSource::System);
    }

    #[test]
    fn has_detail_is_false_for_bare_events() {
        let bare = ev(Severity::Info, LogSource::System, "nothing to see");
        assert!(!bare.has_detail());
        assert!(bare.clone().with_detail("something").has_detail());
        assert!(bare.clone().with_context("status", "500").has_detail());
        // Empty values are dropped rather than rendering blank rows.
        assert!(!bare.clone().with_context("status", "").has_detail());
        assert!(!bare.with_detail("   ").has_detail());
    }

    #[test]
    fn plain_text_export_includes_every_section() {
        let e = ev(Severity::Error, LogSource::Save, "Backend save failed")
            .with_node("save")
            .with_context("http_status", "400")
            .with_detail("predicted_probability must be between 0 and 1")
            .with_remedy(Remedy::CopyDiagnostics)
            .with_payload(serde_json::json!({"error": "bad request"}));
        let text = e.to_plain_text();
        assert!(text.contains("Backend save failed"));
        assert!(text.contains("Node: save"));
        assert!(text.contains("http_status: 400"));
        assert!(text.contains("must be between 0 and 1"));
        assert!(text.contains("Suggested fix"));
        assert!(text.contains("\"error\""));
    }

    #[test]
    fn whole_log_export_covers_all_events() {
        let mut log = ActivityLog::new();
        log.push(ev(Severity::Error, LogSource::Save, "first failure"));
        log.push(ev(Severity::Warn, LogSource::Market, "second problem"));
        let dump = log.to_plain_text();
        assert!(dump.contains("2 event(s)"));
        assert!(dump.contains("first failure"));
        assert!(dump.contains("second problem"));
    }
}
