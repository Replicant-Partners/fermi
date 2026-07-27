//! Fermi Console — MMOG-style forecasting command center
//!
//! Built on GPUI (Zed's GPU-accelerated UI framework).
//! Sprint 2: real API integration, portfolio panel with live data.

mod api;
mod charts;
mod cockpit;
mod composer;
mod text_input;
mod updater;

use api::client::{
    ApiClient, ApiConfig, CalibrationData, CreatePortfolioRequest, CreateTeamRequest, Forecast,
    ForecastQuery, Invite, InviteRequest, LeaderboardEntry, LeaderboardQuery, MyStats,
    PatchPortfolioRequest, Portfolio, PortfolioForecast, PortfolioStats, ShareEntry, ShareRequest,
    Team, TeamDetail, Wallet,
};
use cockpit::CockpitState;
use composer::ComposerState;
use fermi::agent_backend::{
    agent_card::AgentCard, llm_executor::LLMExecutor, registry::AgentRegistry,
};
use gpui::prelude::*;
use gpui::*;
use serde_json::Value as JsonValue;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use text_input::TextInput;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// How often the background refresh loop wakes up to re-fetch
/// forecasts, stats, and the pending-cascade queue. 30 s balances
/// "live enough" for operator UX against API load. See
/// `FermiConsole::start_background_refresh`.
const BACKGROUND_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

// ─── Menu builder ───────────────────────────────────────────────────────────────────

fn build_menus() -> Vec<Menu> {
    vec![
        // ── Application menu ──────────────────────────────────────
        Menu {
            name: "Fermi Console".into(),
            items: vec![
                MenuItem::action("About Fermi Console", ShowDashboard),
                MenuItem::separator(),
                MenuItem::action("New Forecast          Ctrl+N", NewForecast),
                MenuItem::separator(),
                MenuItem::action("Quit Fermi Console    Ctrl+Q", Quit),
            ],
        },
        // ── File menu ─────────────────────────────────────────────
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New Forecast          Ctrl+N", NewForecast),
                MenuItem::separator(),
                MenuItem::action("Publish Forecast      Ctrl+P", PublishForecast),
            ],
        },
        // ── View menu ─────────────────────────────────────────────
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Dashboard             Ctrl+1", ShowDashboard),
                MenuItem::action("Portfolio             Ctrl+2", ShowPortfolio),
                MenuItem::action("Agent Fleet           Ctrl+3", ShowAgentFleet),
                MenuItem::action("Composer              Ctrl+4", ShowComposer),
                MenuItem::action("Leaderboard           Ctrl+5", ShowLeaderboard),
                MenuItem::action("Teams                 Ctrl+6", ShowTeams),
                MenuItem::separator(),
                MenuItem::action("Toggle FPL Source     Ctrl+E", ToggleFplSource),
            ],
        },
        // ── Forecast menu ─────────────────────────────────────────
        Menu {
            name: "Forecast".into(),
            items: vec![
                MenuItem::action(
                    "Research Question     Ctrl+Enter",
                    TriggerQuestionOrchestration,
                ),
                MenuItem::action("Run Simulation        Ctrl+R", RunSimulation),
                MenuItem::action("Publish               Ctrl+P", PublishForecast),
                MenuItem::separator(),
                MenuItem::action("Reset Cockpit", ResetCockpit),
            ],
        },
        // ── Window menu ─────────────────────────────────────────────
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize              Ctrl+M", MinimizeWindow),
                MenuItem::action("Zoom", ZoomWindow),
                MenuItem::action("Toggle Fullscreen     Ctrl+Shift+F", ToggleFullscreen),
            ],
        },
        // ── Help menu ──────────────────────────────────────────────
        Menu {
            name: "Help".into(),
            items: vec![
                MenuItem::action("Keyboard Shortcuts    Ctrl+/", ShowShortcuts),
                MenuItem::separator(),
                MenuItem::action("Check for Updates…", CheckForUpdates),
                MenuItem::action("Release Notes…", ShowUpdateModal),
            ],
        },
    ]
}

// ─── Ayu Mirage Theme Colors ──────────────────────────────────────────────────

mod theme {
    use gpui::rgb;

    pub const BG_DEEP: u32 = 0x171B24; // deepest background (sidebar)
    pub const BG: u32 = 0x1F2430; // primary background
    pub const BG_ELEVATED: u32 = 0x272D38; // panels, cards
    pub const BG_HOVER: u32 = 0x303845; // hover state
    pub const BG_ACTIVE: u32 = 0x3D4455; // active/selected state

    pub const FG: u32 = 0xCBCCC6; // primary text
    pub const FG_DIM: u32 = 0x5C6773; // muted text, labels
    pub const FG_FAINT: u32 = 0x3E4B59; // very muted (borders, separators)

    pub const CYAN: u32 = 0x5CCFE6; // primary accent (links, active tab)
    pub const GREEN: u32 = 0xBAE67E; // success, positive Brier
    pub const GOLD: u32 = 0xFFCC66; // warnings, highlights
    pub const ORANGE: u32 = 0xFFAE57; // secondary accent
    pub const RED: u32 = 0xFF6666; // errors, negative Brier
    pub const PURPLE: u32 = 0xD4BFFF; // special (tournaments, premium)
    pub const BLUE: u32 = 0x73D0FF; // info, agent fleet

    pub fn bg_deep() -> gpui::Hsla {
        rgb(BG_DEEP).into()
    }
    pub fn bg() -> gpui::Hsla {
        rgb(BG).into()
    }
    pub fn bg_elevated() -> gpui::Hsla {
        rgb(BG_ELEVATED).into()
    }
    pub fn bg_hover() -> gpui::Hsla {
        rgb(BG_HOVER).into()
    }
    pub fn bg_active() -> gpui::Hsla {
        rgb(BG_ACTIVE).into()
    }
    pub fn fg() -> gpui::Hsla {
        rgb(FG).into()
    }
    pub fn fg_dim() -> gpui::Hsla {
        rgb(FG_DIM).into()
    }
    pub fn fg_faint() -> gpui::Hsla {
        rgb(FG_FAINT).into()
    }
    pub fn cyan() -> gpui::Hsla {
        rgb(CYAN).into()
    }
    pub fn green() -> gpui::Hsla {
        rgb(GREEN).into()
    }
    pub fn gold() -> gpui::Hsla {
        rgb(GOLD).into()
    }
    pub fn orange() -> gpui::Hsla {
        rgb(ORANGE).into()
    }
    pub fn red() -> gpui::Hsla {
        rgb(RED).into()
    }
    pub fn purple() -> gpui::Hsla {
        rgb(PURPLE).into()
    }
    pub fn blue() -> gpui::Hsla {
        rgb(BLUE).into()
    }
}

// ─── Actions ──────────────────────────────────────────────────────────────────

actions!(
    fermi_console,
    [
        Quit,
        ShowDashboard,
        ShowPortfolio,
        ShowAgentFleet,
        ShowComposer,
        ShowLeaderboard,
        ShowTeams,
        NewForecast,
        RunSimulation,
        ToggleCommandPalette,
        TriggerQuestionOrchestration,
        PublishForecast,
        SaveForecast,
        ImportForecast,
        ToggleFplSource,
        MinimizeWindow,
        ZoomWindow,
        ToggleFullscreen,
        ResetCockpit,
        CheckForUpdates,
        ShowUpdateModal,
        DismissUpdateModal,
        ShowShortcuts,
        DismissShortcuts,
    ]
);

// ─── Navigation ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
enum Panel {
    Dashboard,
    Portfolio,
    AgentFleet,
    Composer,
    Leaderboard,
    Teams,
}

/// Sub-tabs inside the Teams panel's right-pane team detail view.
/// Splits the team surface into three concerns: the people (roster +
/// invites + delete), the things (forecasts + portfolios owned by or
/// shared with the team), and the motion (recent revisions,
/// publications, resolutions across the team's forecasts). Each
/// concern was previously either siloed on another panel or missing
/// entirely; the tabbed layout keeps them findable without stretching
/// the detail pane to an unmanageable length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TeamTab {
    Roster,
    Shared,
    Activity,
}

/// Selection state for the Portfolio panel's virtual buckets. These
/// aren't backed by `fermi_portfolios` rows — they're derived views
/// that give homeless forecasts a place to live in the UX.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VirtualPortfolio {
    /// Forecasts the caller can view but doesn't own (team-shared,
    /// object-shared, or public/shared visibility). Fixes the
    /// "shared forecasts orphaned in the UX" bug.
    SharedWithMe,
    /// Forecasts the caller owns that aren't in any named portfolio
    /// yet — including drafts saved via Ctrl+S. Gives loose work a
    /// discoverable home.
    Unassigned,
    /// All active (committed, Brier-scored) forecasts the caller owns,
    /// across every named portfolio. Replaces the Dashboard's Live
    /// section — the Dashboard is a command center, not a book view.
    Live,
    /// All draft forecasts the caller owns — the WIP inbox. Replaces
    /// the Dashboard's Drafts section. `Unassigned` overlaps for drafts
    /// without a portfolio; this bucket lists every draft regardless of
    /// portfolio membership.
    Drafts,
    /// The caller's most recent resolved forecasts. Replaces the
    /// Dashboard's Recently Resolved section. Cheaper than opening
    /// each portfolio to hunt for resolved rows.
    RecentlyResolved,
}

/// Ordering options for the portfolio detail's forecast list. Each maps to
/// a sort key on `PortfolioForecast` so the operator can find what they're
/// looking for without scrolling 48 rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PortfolioSortMode {
    /// fermi_forecasts.updated_at desc — find what you've worked on lately.
    RecentActivity,
    /// |pm_divergence_pp| desc — biggest delta from the crowd first.
    /// Drives the "where's the opportunity?" question.
    BiggestPmDelta,
    /// n_recent_updates desc — which rows have moved recently.
    BiggestMovement,
    /// predicted_probability desc — leaderboard / favourites.
    HighestProb,
    /// alphabetical question_text — old default.
    Alphabetical,
}

impl PortfolioSortMode {
    fn label(self) -> &'static str {
        match self {
            PortfolioSortMode::RecentActivity => "Recent",
            PortfolioSortMode::BiggestPmDelta => "vs Crowd",
            PortfolioSortMode::BiggestMovement => "Movement",
            PortfolioSortMode::HighestProb => "Highest",
            PortfolioSortMode::Alphabetical => "A→Z",
        }
    }

    const ALL: &'static [PortfolioSortMode] = &[
        PortfolioSortMode::RecentActivity,
        PortfolioSortMode::BiggestPmDelta,
        PortfolioSortMode::BiggestMovement,
        PortfolioSortMode::HighestProb,
        PortfolioSortMode::Alphabetical,
    ];
}

impl Panel {
    fn label(&self) -> &'static str {
        match self {
            Panel::Dashboard => "Dashboard",
            Panel::Portfolio => "Portfolio",
            Panel::AgentFleet => "Agent Fleet",
            Panel::Composer => "Composer",
            Panel::Leaderboard => "Leaderboard",
            Panel::Teams => "Teams",
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            Panel::Dashboard => "⌂",
            Panel::Portfolio => "◈",
            Panel::AgentFleet => "⚙",
            Panel::Composer => "✎",
            Panel::Leaderboard => "⚑",
            Panel::Teams => "👥",
        }
    }

    fn shortcut_hint(&self) -> &'static str {
        match self {
            Panel::Dashboard => "Ctrl+1",
            Panel::Portfolio => "Ctrl+2",
            Panel::AgentFleet => "Ctrl+3",
            Panel::Composer => "Ctrl+4",
            Panel::Leaderboard => "Ctrl+5",
            Panel::Teams => "Ctrl+6",
        }
    }

    fn all() -> &'static [Panel] {
        &[
            Panel::Dashboard,
            Panel::Portfolio,
            Panel::AgentFleet,
            Panel::Composer,
            Panel::Leaderboard,
            Panel::Teams,
        ]
    }
}

// ─── Root Application View ────────────────────────────────────────────────────

#[derive(Clone)]
struct LocalForecast {
    filename: String,
    question: String,
    timestamp: String,
    probability: f64,
    base_rate: f64,
    version: u32,
    driver_count: usize,
    evidence_count: usize,
    agent_count: usize,
    confidence: f64,
    version_probs: Vec<f64>,
    /// Tags for grouping into portfolios (e.g., "nba", "tech", "biotech")
    tags: Vec<String>,
    /// Forecast lifecycle status: draft, active, resolved, archived
    status: String,
    /// Auto-detected domain (e.g., "finance", "sports", "technology")
    domain: String,
    /// If resolved: the actual outcome (true/false) and Brier score
    resolved_outcome: Option<bool>,
    brier_score: Option<f64>,
}

/// Modal state for the "just-created invite" affordance. Populated
/// when an invite POST returns; cleared when the operator dismisses.
///
/// The modal is the operator's one-click path to share an invite
/// externally. It's especially important when the server doesn't have
/// `RESEND_API_KEY` configured (the invite row exists but no email
/// went out) — the operator NEEDS to see and copy the link
/// immediately, or the invitee will never know they were invited.
#[derive(Clone)]
struct InviteShareModal {
    /// Absolute URL the invitee should visit. Constructed from
    /// `api.base_url_sync() + "/invites/" + token`.
    invite_url: String,
    /// Human-readable target label, e.g. "team ‘WC-analysts’",
    /// "portfolio ‘Q1 macro watch’", or "forecast ‘Will …’".
    target_label: String,
    /// Recipient string (email or user_id) as sent to the server.
    recipient: String,
    /// The permission granted ("view", "edit", "admin", team roles).
    permission: String,
    /// True when the server had `RESEND_API_KEY` configured and the
    /// email dispatch was spawned. Drives the copy "emailed to …" vs
    /// "email delivery not configured" affordance in the modal.
    email_sent: bool,
}

/// Modal state for the three-tier agent hire flow (Sprint C polish).
///
/// Semantics: an agent is hired *into a forecast* and *bound to a driver*.
/// The operator picks the forecast first (contract locus), then the
/// driver within that forecast (research target), then reviews the
/// terms placeholder and confirms.
///
/// The confirmation step is currently a placeholder that surfaces a
/// hint into the cockpit and navigates the operator to the Composer
/// with the target forecast open. Full "click Confirm → driver-agent
/// binding is created and the agent fires" wiring is a follow-up
/// that touches `assign_agent_to_driver` on the cockpit and needs a
/// server-side per-forecast agent binding endpoint.
#[derive(Clone)]
struct HireModalState {
    /// The agent being hired — identifier and human display name.
    agent_id: String,
    agent_display: String,
    /// Current step: 1 = pick forecast, 2 = pick driver, 3 = review terms.
    /// Advances on selection; back button decrements.
    step: u8,
    /// Selected forecast id (populated after step 1).
    forecast_id: Option<String>,
    /// Human display of the selected forecast (question text truncated).
    forecast_label: Option<String>,
    /// Selected driver name (populated after step 2). None allowed —
    /// operator may hire an agent as an ambient research agent bound
    /// to no specific driver (e.g. macro-context agents that populate
    /// factors).
    driver_name: Option<String>,
    /// Free-text notes on the hire (contract addendum). Empty = default
    /// terms accepted verbatim.
    notes: Entity<TextInput>,
}

#[derive(Clone)]
struct WorkspaceForecast {
    workspace_id: String,
    workspace_name: String,
    /// The linked `fermi_forecasts.id`, if this workspace is backed by a
    /// fermi_forecast row (the only path that surfaces the FPL + drivers in
    /// the cockpit). Populated by the server via LEFT JOIN on
    /// `fermi_forecasts.workspace_id` in `/api/apps/fermi_forecast/workspaces`.
    forecast_id: Option<String>,
    team_id: Option<String>,      // from params, e.g. "ARG"
    team_name: Option<String>,    // from params, e.g. "Argentina"
    group: Option<String>,        // from params, e.g. "B"
    program_type: Option<String>, // TEAM_PRIOR, TOURNAMENT_PATH, H2H_MATCH
    probability: Option<f64>,     // from workspace outputs
    elo: Option<f64>,             // from params
    created_at: String,
}

struct FermiConsole {
    active_panel: Panel,
    focus_handle: FocusHandle,

    // API client (shared, thread-safe)
    api: Arc<ApiClient>,
    // Local agent registry (same as MCP server)
    registry: Arc<AgentRegistry>,

    // Connection state
    connected: bool,
    user_display_name: Option<String>,
    api_key_input: String,

    // Sign-in UI
    sign_in_token_input: Entity<TextInput>,
    sign_in_error: Option<String>,
    sign_in_loading: bool,
    oauth_port: Option<u16>,
    sign_in_fallback_message: bool,

    // Dashboard data (from /api/forecasts/my-stats)
    my_stats: Option<MyStats>,
    stats_loading: bool,

    // Portfolio data
    portfolios: Vec<Portfolio>,
    portfolios_loading: bool,

    // Forecast lists
    active_forecasts: Vec<Forecast>,
    resolved_forecasts: Vec<Forecast>,
    draft_forecasts: Vec<Forecast>,
    forecasts_loading: bool,

    // Activity feed (derived from recent forecasts)
    recent_activity: Vec<ActivityItem>,

    // Composer state (legacy linear form)
    composer: ComposerState,

    // Research Cockpit state (OODA loop workspace) — Entity for async channel integration
    cockpit: Option<Entity<CockpitState>>,

    // Forecast detail view (click a row in Portfolio to expand)
    selected_forecast_id: Option<String>,

    // Agent Fleet data (from /api/agents)
    agent_cards: Vec<JsonValue>,
    agents_loading: bool,
    agent_search: String,
    /// Marketplace tier filter chip. "all" | "popular" | "established"
    /// | "rising" | "fresh". Backs the Agent Fleet's tier chips.
    /// Sprint C.
    agent_marketplace_tier: String,
    /// Marketplace sort mode. "score" (default) | "cost_asc" |
    /// "cost_desc" | "executions" | "success" | "contribution".
    /// Sprint C.
    agent_marketplace_sort: String,
    /// Agent IDs whose marketplace card is expanded to show rich
    /// detail (accepts / produces / sample queries / model config /
    /// MCP tools). Collapsed by default so the list stays scannable.
    agent_marketplace_expanded: std::collections::HashSet<String>,
    /// Hire modal state — pops when the operator clicks Hire on a
    /// marketplace card. Three-tier flow: pick forecast, pick driver,
    /// review terms, confirm.
    hire_modal: Option<HireModalState>,

    // Leaderboard data (from /api/leaderboard)
    leaderboard: Vec<LeaderboardEntry>,
    leaderboard_loading: bool,

    // Local forecasts (from forecasts/ directory)
    local_forecasts: Vec<LocalForecast>,

    // Workspace forecasts (from ABW fermi_forecast app)
    workspace_forecasts: Vec<WorkspaceForecast>,
    workspace_forecasts_loading: bool,
    workspace_section_collapsed: bool,

    // Polymarket integration
    pm_search_input: Entity<TextInput>,
    pm_search_results: Vec<JsonValue>,
    pm_search_loading: bool,
    pm_show_search: bool,
    pm_search_error: Option<String>,
    pm_resolutions_loading: bool,
    pm_resolutions_last_result: Option<String>,

    // Named portfolio management
    portfolio_create_showing: bool,
    portfolio_create_input: Entity<TextInput>,
    portfolio_create_loading: bool,
    portfolio_create_error: Option<String>,
    selected_portfolio_id: Option<String>,
    /// Selection state for the Portfolio panel's *virtual* buckets —
    /// forecasts that don't live in a named portfolio but still need
    /// a home in the UX:
    ///
    /// * `SharedWithMe`: forecasts owned by teammates/collaborators
    ///   that the caller can see (team share, object share, or
    ///   public/shared visibility). Fixes the "shared forecasts are
    ///   orphaned" reported bug.
    /// * `Unassigned`: forecasts the caller owns that aren't a member
    ///   of any named portfolio — including drafts saved with Ctrl+S
    ///   before the operator picks a home for them.
    ///
    /// Mutually exclusive with `selected_portfolio_id`; selecting a
    /// virtual bucket clears the named-portfolio selection and vice
    /// versa.
    selected_virtual_portfolio: Option<VirtualPortfolio>,
    /// Forecasts shared with the caller by others. Populated on demand
    /// when the operator selects the "📥 Shared with me" bucket, and
    /// refetched whenever the parent forecast list refreshes.
    shared_with_me_forecasts: Vec<Forecast>,
    shared_with_me_loading: bool,
    /// Forecasts the caller owns that aren't in any portfolio.
    /// Populated on demand for the "📌 Unassigned" bucket.
    unassigned_forecasts: Vec<Forecast>,
    unassigned_loading: bool,
    portfolio_stats_cache: HashMap<String, PortfolioStats>,
    portfolio_forecasts: HashMap<String, Vec<PortfolioForecast>>,
    portfolio_forecasts_loading: HashSet<String>,
    portfolio_rename_id: Option<String>,
    portfolio_rename_input: Entity<TextInput>,
    portfolio_confirm_delete_id: Option<String>,
    // ── Portfolio Access panel (Spec 24 §3.5.3) ───────────────────
    portfolio_share_showing: bool,
    portfolio_shares: Vec<ShareEntry>,
    portfolio_shares_loading: bool,
    portfolio_share_input: Entity<TextInput>,
    portfolio_share_permission: String,
    portfolio_share_error: Option<String>,
    /// Which portfolio the loaded `portfolio_shares` belong to.
    portfolio_shares_loaded_for: Option<String>,
    /// Team IDs currently being shared with a portfolio (button disabled
    /// until the API call returns). Mirrors `share_team_in_flight` in the
    /// cockpit Access tab — same UX, same guardrail against dupe posts.
    portfolio_team_share_in_flight: std::collections::HashSet<String>,
    /// Sort mode for the portfolio detail's forecast rows.
    portfolio_sort_mode: PortfolioSortMode,
    /// Free-text filter for the portfolio detail. The entity owns the
    /// source-of-truth string; render reads it live with
    /// `portfolio_filter_input.read(cx).text()`. Matches case-insensitively
    /// against question_text + tags.
    portfolio_filter_input: Entity<TextInput>,
    /// Forecast IDs whose Constellation row is expanded to show the
    /// drill-down panel (full question text, tags, brier, resolution
    /// notes, deep actions). Empty by default — the compact row keeps
    /// the table scannable, and the operator opts in per-row.
    portfolio_expanded_rows: std::collections::HashSet<String>,
    /// Quick-filter chips active on the Constellation table. Each chip
    /// is a mutually-inclusive predicate on `PortfolioForecast` — e.g.
    /// `"hot"` keeps only rows with `n_recent_updates > 0`, `"linked"`
    /// keeps only rows with a Polymarket link, etc. Combined with AND
    /// with the free-text filter.
    portfolio_quick_filters: std::collections::HashSet<String>,
    /// Correlation assumption for the Portfolio Risk view's P(any yes)
    /// slider. 0 = independent, +1 = perfectly positively correlated,
    /// −1 = mutually exclusive. Stored on the console so it survives
    /// panel navigation within a session.
    portfolio_risk_rho: f64,

    // ── Cascade / Relationships UI (Sprint B) ─────────────────────────
    //
    // Server has forecast_relationships fully wired (mutex, implies,
    // at_most_n, exhaustive_cover, conditional, conjunction). Missing
    // piece was UX to declare them. `all_relationships` holds every
    // relationship the caller owns; the Portfolio detail's
    // Relationships sub-panel filters this by
    // `forecast_ids ∩ current_portfolio_forecasts`.
    all_relationships: Vec<JsonValue>,
    all_relationships_loading: bool,
    /// True when the operator has expanded the Portfolio detail's
    /// "⛓ Relationships" section. Collapsed by default to keep the
    /// dense panel scannable.
    relationships_showing: bool,
    /// True when the operator has opened the inline "+ Declare
    /// relationship" sheet. Rendered inside the Relationships section.
    relationship_create_showing: bool,
    /// Selected kind for the pending declaration.
    relationship_create_kind: String,
    /// Selected forecast_ids for the pending declaration — the
    /// forecasts that will participate in the relationship. Defaults
    /// to the current portfolio's active forecasts.
    relationship_create_forecast_ids: std::collections::HashSet<String>,
    /// String-typed "n" parameter for `at_most_n` (only used when the
    /// selected kind requires it). Free-form text; parsed at submit.
    relationship_create_n: String,
    /// Optional description for the pending declaration.
    relationship_create_description: Entity<TextInput>,
    relationship_create_loading: bool,
    relationship_create_error: Option<String>,
    /// Set of relationship IDs currently being deleted; disables the
    /// per-row button until the DELETE resolves.
    relationship_delete_in_flight: std::collections::HashSet<String>,

    /// "Just-created invite" modal state (Sprint A). When an invite is
    /// created (from team, portfolio, or forecast), the response's
    /// `token` is stashed here along with target metadata so we can
    /// pop a modal offering an immediate one-click Copy Link. Cleared
    /// when the operator dismisses. Also used to communicate the
    /// email-delivery status: when the server has RESEND_API_KEY the
    /// modal reads "invite emailed — you can also copy this link",
    /// otherwise it reads "email delivery not configured — share this
    /// link directly."
    invite_share_modal: Option<InviteShareModal>,

    // Commit sheet (shown on ⌘P before publishing)
    commit_sheet_showing: bool,
    commit_sheet_visibility: String,
    commit_sheet_question: String,
    commit_sheet_probability: f64,
    /// Spec 24 §3.5.1: collaborators to share with on commit. Each entry is
    /// (target, permission); target is an email or user_id. Applied after
    /// the forecast row is written, so one Commit click is one logical op.
    commit_share_targets: Vec<(String, String)>,
    /// Teams selected for post-publish sharing. Parallel to
    /// `commit_share_targets` but keyed by team_id, applied server-side
    /// as `share_type='team'`. Same permission chip drives both.
    commit_share_team_targets: Vec<(String, String)>,
    commit_share_input: Entity<TextInput>,
    /// Permission for the next "add" — view | edit | admin (cycle chip).
    commit_share_permission: String,

    // Resolve sheet (record actual outcome of an active forecast)
    resolve_sheet_showing: bool,
    resolve_forecast_id: Option<String>,
    resolve_forecast_question: String,
    resolve_outcome: Option<bool>,
    resolve_loading: bool,
    resolve_error: Option<String>,

    // Cascade-after-resolve state. After a successful resolve, the cockpit
    // queries `/api/forecast-relationships?forecast_id=<resolved>` to find
    // any declared inter-forecast dependencies (e.g. WC sims mutex group).
    // If any exist, the Resolve sheet stays open with a "Cascade to N
    // forecasts" affordance per relationship; clicking propagates the
    // resolution across siblings via the relationship's per-kind handler.
    cascade_relationships: Vec<JsonValue>,
    cascade_resolved_forecast_id: Option<String>,
    cascade_resolved_outcome: Option<bool>,
    cascade_loading: bool,
    cascade_summary: Option<String>,

    // Pending cascades queue — server-queued operator-gate reviews.
    // When a forecast resolves (manually or via upstream workspace
    // resolution), the server queues a pending_cascade row per
    // relationship. We poll periodically + after every resolve so the
    // badge stays fresh. Operator clicks the badge → review sheet
    // opens → Apply / Dismiss each entry.
    pending_cascades: Vec<JsonValue>,
    pending_cascades_sheet_showing: bool,
    pending_cascades_loading: bool,
    /// Set of cascade IDs currently being applied or dismissed; UI
    /// disables their buttons until the action completes so a
    /// double-click can't double-fire.
    cascade_action_in_flight: std::collections::HashSet<String>,
    /// Guard so we only spawn the background refresh loop once per
    /// process. Set true the first time try_connect succeeds; the
    /// loop runs for the lifetime of the app and pauses itself
    /// while `connected` is false (e.g. after sign-out).
    background_refresh_started: bool,

    // ── Teams panel (Spec 24 §3.5.4) ──────────────────────────────────
    /// Teams the user belongs to (left pane of Panel::Teams).
    teams: Vec<Team>,
    teams_loading: bool,
    /// Cached team-share associations per forecast id — the set of
    /// team_ids each forecast has been shared with via an
    /// `object_shares` row with `share_type='team'`. Populated lazily
    /// by `refresh_forecast_shares_cache` after every `fetch_forecasts`
    /// run, walking only the own-forecasts with `share_count > 0` so
    /// the fan-out cost scales with real sharing activity rather than
    /// the full book size.
    ///
    /// Used by `primary_team_id_for_forecast` to colour the team-dot
    /// on forecast rows and activity items: a forecast's "primary
    /// team" is its owning `team_id` when present, else the first
    /// team it's been shared with. When neither exists no dot is
    /// rendered.
    forecast_team_shares: std::collections::HashMap<String, Vec<String>>,
    /// Forecast ids currently mid-flight in the shares fan-out.
    /// Guards against duplicate fetches when `fetch_forecasts` fires
    /// while an earlier refresh is still in progress.
    forecast_shares_in_flight: std::collections::HashSet<String>,
    /// Mirror of `forecast_team_shares` for portfolios. Powers the
    /// Teams panel's Shared tab: portfolios explicitly shared with a
    /// team appear even when their owning `team_id` is None. Populated
    /// by `refresh_portfolio_shares_cache`; same fan-out shape, keyed
    /// off portfolio id.
    portfolio_team_shares: std::collections::HashMap<String, Vec<String>>,
    /// Portfolio ids currently mid-flight in the portfolio shares
    /// fan-out. De-dup guard mirroring `forecast_shares_in_flight`.
    portfolio_shares_in_flight: std::collections::HashSet<String>,
    /// Which sub-tab of the team detail pane is showing. Persists
    /// across team selections (opening a different team preserves the
    /// active tab), which matches how the tab bars work in the
    /// composer's right pane.
    selected_team_tab: TeamTab,
    /// Source filter chip state for the Dashboard's activity feed.
    /// `All` is the default; `Mine` / `Team` gate on `ActivityItem`'s
    /// source. Marketplace stays a placeholder until we ingest
    /// marketplace publish events.
    dashboard_activity_filter: ActivityFilter,
    /// Team detail cache keyed by team_id. Populated by a background
    /// fan-out that fires after `fetch_teams` completes so the
    /// Dashboard's team cards can render member counts and initials
    /// without waiting for the operator to open the Teams panel.
    /// Sits alongside (not replacing) `selected_team_detail`, which
    /// is scoped to the Teams-panel right-pane selection.
    team_details: std::collections::HashMap<String, TeamDetail>,
    /// Team ids currently mid-flight in the detail fan-out. Guards
    /// against duplicate fetches when `fetch_teams` fires again while
    /// a previous refresh is still in progress.
    team_details_in_flight: std::collections::HashSet<String>,
    /// Which Dashboard team card is currently hovered. Drives the
    /// hover-reveal roster strip: when set, the matching card
    /// expands downward with an initials strip drawn from
    /// `team_details`. Cleared on pointer-leave. Transient UI
    /// state; not persisted.
    hovered_team_id: Option<String>,
    /// The team selected in the left pane; drives the right-pane detail.
    selected_team_id: Option<String>,
    /// Loaded detail (team + members) for `selected_team_id`.
    selected_team_detail: Option<TeamDetail>,
    team_detail_loading: bool,
    /// Pending + terminal invites addressed to the selected team,
    /// keyed by team_id. Populated by `fetch_team_invites` after each
    /// team-detail load so the operator can see outbound invitations
    /// ("Alice — pending (member) · [Revoke]") alongside the accepted
    /// members list.
    team_invites: Vec<Invite>,
    team_invites_loading: bool,
    /// Invite IDs with a revoke in flight; disables the button.
    team_invite_revoke_in_flight: std::collections::HashSet<String>,
    /// Team ID pending a delete-confirmation click. Two-step confirm
    /// to protect against fat-fingering — first click sets this to
    /// Some(team_id) and the button flips to a red "Really delete?"
    /// state; second click fires the DELETE. Cleared on team switch
    /// so a stale confirmation can't leak to a different team.
    team_delete_confirm_id: Option<String>,
    team_delete_loading: bool,
    /// "Create team" modal state.
    team_create_showing: bool,
    team_create_name_input: Entity<TextInput>,
    team_create_slug_input: Entity<TextInput>,
    team_create_loading: bool,
    team_create_error: Option<String>,
    /// Inline "+ Invite member" row state inside the team detail pane.
    team_invite_showing: bool,
    team_invite_input: Entity<TextInput>,
    /// Selected team role for the pending invite (owner/admin/member/viewer).
    team_invite_role: String,
    team_invite_loading: bool,
    team_action_error: Option<String>,

    // ── Inbox: pending invites (Spec 24 §3.5.5) ───────────────────
    inbox_invites: Vec<Invite>,
    inbox_loading: bool,
    inbox_sheet_showing: bool,
    /// Invite IDs with an accept/decline in flight (disables their buttons).
    inbox_action_in_flight: std::collections::HashSet<String>,

    // Toast notification (auto-dismiss after 3 s)
    // (message, icon, color)
    toast: Option<(String, &'static str, u32)>,

    // ── Wallet state ─────────────────────────────────────────────────────
    //
    // Cached wallet snapshot from `/api/wallet`. Fetched on connect
    // and again by the background refresh loop. `None` = not fetched
    // yet; renders as "…" in the sidebar chip.
    wallet: Option<Wallet>,
    /// True while the operator has never dismissed the first-run
    /// welcome modal AND the wallet appears to be a fresh onboarding
    /// (balance <= granted_balance and no spend yet). Cleared once
    /// the user hits Continue.
    welcome_modal_showing: bool,

    // ── Self-update state ─────────────────────────────────────────
    //
    // Populated by a background check fired at startup + every time
    // the user picks Help → Check for Updates…. `None` means "no
    // update available (or we haven't checked yet)". When Some, the
    // sidebar shows a badge and the modal is available.
    available_update: Option<updater::ReleaseInfo>,
    /// True while an update check or download is in flight. Used to
    /// disable the "Check for Updates" menu item during the check so
    /// impatient clicks don't queue five HEAD requests.
    update_check_in_flight: bool,
    /// True when the operator has explicitly opened the release-notes
    /// modal. Distinct from `available_update.is_some()` because we
    /// want the badge visible without the modal blocking the UI.
    update_modal_showing: bool,
    /// Progress + error state for the download-and-install phase.
    update_download: updater::DownloadState,
    /// True when the operator has opened the keyboard-shortcuts help
    /// modal (via Ctrl+/, the sidebar "❔ Shortcuts" chip, or the Help
    /// menu). Rendered as a full-window overlay listing every bound
    /// shortcut grouped by category — the entry point for anyone who
    /// doesn't yet know the console's hotkeys, which is currently a
    /// major usability wall.
    shortcuts_modal_showing: bool,
}

#[derive(Clone)]
struct ActivityItem {
    icon: &'static str,
    text: String,
    time: String,
    color: u32,
    /// Forecast id the row represents. Non-optional because every
    /// candidate we surface is derived from a real fermi_forecasts row
    /// (active_forecasts / resolved_forecasts). Powers the click-to-open
    /// behaviour on the Dashboard's Recent Activity feed.
    forecast_id: String,
    /// Which source stream this item came from. Drives the source
    /// filter chips on the Dashboard's activity feed. Historically all
    /// items were `Mine` (own-forecast events); v0.8.11 adds Team
    /// items sourced from `shared_with_me_forecasts`.
    source: ActivitySource,
}

/// Which stream an ActivityItem came from. Team = a forecast belonging
/// to a team the operator is a member of, but authored by someone else
/// (i.e. work by teammates on shared surfaces). Marketplace stays a
/// placeholder until we ingest marketplace publish events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivitySource {
    Mine,
    Team,
    #[allow(dead_code)]
    Marketplace,
}

/// Chip filter over the Dashboard's activity feed. `All` shows every
/// source; the other variants gate on `ActivityItem.source`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivityFilter {
    All,
    Mine,
    Team,
    #[allow(dead_code)]
    Marketplace,
}

impl FermiConsole {
    fn new(api: Arc<ApiClient>, registry: Arc<AgentRegistry>, cx: &mut Context<Self>) -> Self {
        let sign_in_token_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("Paste your ABW token or API key")
                .with_label("Sign In")
                .with_large(true)
        });

        let pm_search_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("Search or paste a Polymarket URL…")
                .with_label("Search Polymarket")
        });

        let portfolio_create_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("Portfolio name…")
                .with_label("New Portfolio")
        });

        let portfolio_rename_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("New name…")
                .with_label("Rename Portfolio")
        });

        // Inline search box at the top of the portfolio detail. Free-text
        // matches against question_text + tags, lower-cased.
        let portfolio_filter_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("Filter forecasts (team, tag, free text)…")
                .with_label("Filter")
        });

        let team_create_name_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("Team name…")
                .with_label("Team name")
        });
        let team_create_slug_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("team-slug (lowercase, no spaces)")
                .with_label("Slug")
        });
        let team_invite_input =
            cx.new(|cx| TextInput::new(cx).with_placeholder("email or user id…"));
        let commit_share_input =
            cx.new(|cx| TextInput::new(cx).with_placeholder("Add person by email or user id…"));
        let portfolio_share_input =
            cx.new(|cx| TextInput::new(cx).with_placeholder("email or user id…"));
        // Sprint B: description field for the "+ Declare relationship"
        // sheet. Optional freeform notes so the operator can remember
        // *why* they declared a cascade ("WC 2026 group L mutex — only
        // one team advances").
        let rel_desc_input = cx.new(|cx| {
            TextInput::new(cx).with_placeholder("Optional description — why this relationship?")
        });

        let mut console = Self {
            active_panel: Panel::Dashboard,
            focus_handle: cx.focus_handle(),
            api,
            registry: registry.clone(),
            connected: false,
            user_display_name: None,
            api_key_input: String::new(),
            sign_in_token_input,
            sign_in_error: None,
            sign_in_loading: false,
            oauth_port: None,
            sign_in_fallback_message: false,
            my_stats: None,
            stats_loading: false,
            portfolios: Vec::new(),
            portfolios_loading: false,
            active_forecasts: Vec::new(),
            resolved_forecasts: Vec::new(),
            draft_forecasts: Vec::new(),
            forecasts_loading: false,
            recent_activity: Vec::new(),
            composer: ComposerState::new(),
            cockpit: None,
            selected_forecast_id: None,
            agent_cards: Vec::new(),
            agents_loading: false,
            agent_search: String::new(),
            agent_marketplace_tier: "all".into(),
            agent_marketplace_sort: "score".into(),
            agent_marketplace_expanded: std::collections::HashSet::new(),
            hire_modal: None,
            leaderboard: Vec::new(),
            leaderboard_loading: false,
            local_forecasts: Vec::new(),
            workspace_forecasts: Vec::new(),
            workspace_forecasts_loading: false,
            workspace_section_collapsed: false,
            pm_search_input,
            pm_search_results: Vec::new(),
            pm_search_loading: false,
            pm_show_search: false,
            pm_search_error: None,
            pm_resolutions_loading: false,
            pm_resolutions_last_result: None,
            portfolio_create_showing: false,
            portfolio_create_input,
            portfolio_create_loading: false,
            portfolio_create_error: None,
            selected_portfolio_id: None,
            selected_virtual_portfolio: None,
            shared_with_me_forecasts: Vec::new(),
            shared_with_me_loading: false,
            unassigned_forecasts: Vec::new(),
            unassigned_loading: false,
            portfolio_stats_cache: HashMap::new(),
            portfolio_forecasts: HashMap::new(),
            portfolio_forecasts_loading: HashSet::new(),
            portfolio_rename_id: None,
            portfolio_rename_input,
            portfolio_confirm_delete_id: None,
            portfolio_share_showing: false,
            portfolio_shares: Vec::new(),
            portfolio_shares_loading: false,
            portfolio_share_input,
            portfolio_share_permission: "view".into(),
            portfolio_share_error: None,
            portfolio_shares_loaded_for: None,
            portfolio_team_share_in_flight: std::collections::HashSet::new(),
            portfolio_sort_mode: PortfolioSortMode::RecentActivity,
            portfolio_filter_input,
            portfolio_expanded_rows: std::collections::HashSet::new(),
            portfolio_quick_filters: std::collections::HashSet::new(),
            portfolio_risk_rho: 0.0,
            all_relationships: Vec::new(),
            all_relationships_loading: false,
            relationships_showing: false,
            relationship_create_showing: false,
            relationship_create_kind: "mutually_exclusive".into(),
            relationship_create_forecast_ids: std::collections::HashSet::new(),
            relationship_create_n: "1".into(),
            relationship_create_description: rel_desc_input,
            relationship_create_loading: false,
            relationship_create_error: None,
            relationship_delete_in_flight: std::collections::HashSet::new(),
            invite_share_modal: None,
            commit_sheet_showing: false,
            commit_sheet_visibility: "private".into(),
            commit_sheet_question: String::new(),
            commit_sheet_probability: 0.5,
            commit_share_targets: Vec::new(),
            commit_share_team_targets: Vec::new(),
            commit_share_input,
            commit_share_permission: "view".into(),
            resolve_sheet_showing: false,
            resolve_forecast_id: None,
            resolve_forecast_question: String::new(),
            resolve_outcome: None,
            resolve_loading: false,
            resolve_error: None,
            cascade_relationships: Vec::new(),
            cascade_resolved_forecast_id: None,
            cascade_resolved_outcome: None,
            cascade_loading: false,
            cascade_summary: None,
            pending_cascades: Vec::new(),
            pending_cascades_sheet_showing: false,
            pending_cascades_loading: false,
            cascade_action_in_flight: std::collections::HashSet::new(),
            background_refresh_started: false,
            teams: Vec::new(),
            teams_loading: false,
            forecast_team_shares: std::collections::HashMap::new(),
            forecast_shares_in_flight: std::collections::HashSet::new(),
            portfolio_team_shares: std::collections::HashMap::new(),
            portfolio_shares_in_flight: std::collections::HashSet::new(),
            selected_team_tab: TeamTab::Roster,
            dashboard_activity_filter: ActivityFilter::All,
            team_details: std::collections::HashMap::new(),
            team_details_in_flight: std::collections::HashSet::new(),
            hovered_team_id: None,
            selected_team_id: None,
            selected_team_detail: None,
            team_detail_loading: false,
            team_invites: Vec::new(),
            team_invites_loading: false,
            team_invite_revoke_in_flight: std::collections::HashSet::new(),
            team_delete_confirm_id: None,
            team_delete_loading: false,
            team_create_showing: false,
            team_create_name_input,
            team_create_slug_input,
            team_create_loading: false,
            team_create_error: None,
            team_invite_showing: false,
            team_invite_input,
            team_invite_role: "member".into(),
            team_invite_loading: false,
            team_action_error: None,
            inbox_invites: Vec::new(),
            inbox_loading: false,
            inbox_sheet_showing: false,
            inbox_action_in_flight: std::collections::HashSet::new(),
            toast: None,
            wallet: None,
            welcome_modal_showing: false,
            available_update: None,
            update_check_in_flight: false,
            update_modal_showing: false,
            update_download: updater::DownloadState::Idle,
            shortcuts_modal_showing: false,
        };

        // Try to load API key from environment (fallback for dev)
        if let Ok(key) = std::env::var("FERMI_API_KEY").or_else(|_| std::env::var("ABW_API_KEY")) {
            console.api_key_input = key;
            console.try_connect(cx);
        }

        // Fire an update check on launch. This is intentionally silent
        // on failure — the user hasn't asked for it yet, so a network
        // hiccup shouldn't produce a scary toast. If a new release is
        // out we surface it as a passive sidebar badge; the user opens
        // the modal on their own schedule.
        console.check_for_updates(false, cx);

        console
    }

    // ── API connection ────────────────────────────────────────────────

    /// Sign in with a token entered in the UI.
    fn sign_in_from_ui(&mut self, cx: &mut Context<Self>) {
        let token = self.sign_in_token_input.read(cx).text().to_string();
        let token = token.trim().to_string();
        if token.is_empty() {
            self.sign_in_error = Some("Please enter a token or API key".into());
            cx.notify();
            return;
        }
        self.api_key_input = token;
        self.sign_in_error = None;
        self.sign_in_loading = true;
        cx.notify();
        self.try_connect(cx);
    }

    /// Start OAuth flow: spin up a localhost listener, open the browser,
    /// wait for ABW to redirect back with the token.
    fn start_oauth_flow(&mut self, provider: &str, cx: &mut Context<Self>) {
        self.sign_in_loading = true;
        self.sign_in_error = None;
        cx.notify();

        let provider = provider.to_string();
        let api = self.api.clone();

        cx.spawn(async move |this, cx| {
            // 1. Bind a TCP listener on a random port
            let listener = match tokio::net::TcpListener::bind("127.0.0.1:0").await {
                Ok(l) => l,
                Err(e) => {
                    this.update(cx, |this, cx| {
                        this.sign_in_loading = false;
                        this.sign_in_error = Some(format!("Failed to start auth server: {}", e));
                        cx.notify();
                    })
                    .ok();
                    return;
                }
            };
            let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);

            this.update(cx, |this, cx| {
                this.oauth_port = Some(port);
                cx.notify();
            })
            .ok();

            // 2. Open browser to ABW OAuth
            // We try the localhost callback flow first. If the server doesn't
            // support localhost redirects yet, the user lands on ABW's dashboard
            // with a valid session cookie. They can then copy their token from
            // the ABW settings page and paste it into the manual token field.
            let base_url = api.base_url().await;
            let callback_url = format!("http://127.0.0.1:{}/callback", port);
            // `app=fermi_console` tags this signup on the ABW side so
            // admins can see the Fermi Console cohort in isolation.
            // Applies to NEW signups only — ignored for existing users.
            let auth_url = format!(
                "{}/auth/{}?redirect={}&app=fermi_console",
                base_url, provider, callback_url
            );
            log::info!("[oauth] Opening browser: {}", auth_url);
            let _ = open::that(&auth_url);

            // 3. Wait for the callback (with short timeout — if the server
            // doesn't support localhost redirects, we fall back gracefully)
            let result = tokio::time::timeout(
                std::time::Duration::from_secs(30),
                accept_oauth_callback(&listener),
            )
            .await;

            match result {
                Ok(Ok(token)) => {
                    log::info!("[oauth] Got token via localhost callback");
                    api.set_api_key(&token).await;

                    match api.auth_me().await {
                        Ok(me) => {
                            this.update(cx, |this, cx| {
                                this.api_key_input = token;
                                this.connected = true;
                                this.sign_in_loading = false;
                                this.sign_in_error = None;
                                this.oauth_port = None;
                                this.user_display_name = Some(me.friendly_label());
                                log::info!("[oauth] Connected as: {:?}", me.friendly_label());
                                this.fetch_all_data(cx);
                                this.start_background_refresh(cx);
                                cx.notify();
                            })
                            .ok();
                        }
                        Err(e) => {
                            api.clear_api_key().await;
                            this.update(cx, |this, cx| {
                                this.sign_in_loading = false;
                                this.sign_in_error = Some(format!("Auth failed: {}", e));
                                this.oauth_port = None;
                                cx.notify();
                            })
                            .ok();
                        }
                    }
                }
                Ok(Err(e)) => {
                    log::warn!("[oauth] Callback error: {}", e);
                    this.update(cx, |this, cx| {
                        this.sign_in_loading = false;
                        this.sign_in_error = Some(format!("OAuth error: {}", e));
                        this.oauth_port = None;
                        cx.notify();
                    })
                    .ok();
                }
                Err(_) => {
                    // Timeout — the server probably doesn't support localhost
                    // redirects yet. The user signed in on ABW but the redirect
                    // went to /dashboard instead of our callback.
                    log::info!("[oauth] Localhost callback timed out — server may not support desktop redirect yet");
                    this.update(cx, |this, cx| {
                        this.sign_in_loading = false;
                        this.sign_in_error = None;
                        this.oauth_port = None;
                        // Show a helpful message instead of an error
                        this.sign_in_fallback_message = true;
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// Show a transient toast notification. Auto-dismisses after 3 seconds.
    /// `icon` is a short emoji/symbol, `color` is a theme constant.
    fn show_toast(
        &mut self,
        message: impl Into<String>,
        icon: &'static str,
        color: u32,
        cx: &mut Context<Self>,
    ) {
        self.toast = Some((message.into(), icon, color));
        cx.notify();
        // Schedule dismissal after 3 s.
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(3))
                .await;
            this.update(cx, |console, cx| {
                console.toast = None;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── Self-update ──────────────────────────────────────────────────────────

    /// Query GitHub for a newer release. `verbose=true` when triggered
    /// from the Help menu (surfaces both success and "up to date"
    /// toasts); `false` on startup where we only make noise if there's
    /// actually an update.
    fn check_for_updates(&mut self, verbose: bool, cx: &mut Context<Self>) {
        if self.update_check_in_flight {
            return;
        }
        self.update_check_in_flight = true;
        cx.notify();

        let current = env!("CARGO_PKG_VERSION").to_string();

        cx.spawn(async move |this, cx| {
            let result = updater::check_latest(&current).await;
            this.update(cx, |this, cx| {
                this.update_check_in_flight = false;
                match result {
                    Ok(Some(release)) => {
                        log::info!(
                            "[updater] new version available: {} (running {})",
                            release.tag,
                            current
                        );
                        if verbose {
                            this.show_toast(
                                format!("Update available: {}", release.tag),
                                "⬆",
                                theme::CYAN,
                                cx,
                            );
                        }
                        this.available_update = Some(release);
                    }
                    Ok(None) => {
                        if verbose {
                            this.show_toast(
                                format!("You're on the latest version (v{})", current),
                                "✓",
                                theme::GREEN,
                                cx,
                            );
                        }
                    }
                    Err(e) => {
                        log::warn!("[updater] check failed: {}", e);
                        if verbose {
                            this.show_toast(
                                format!("Update check failed: {}", e),
                                "⚠",
                                theme::GOLD,
                                cx,
                            );
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Kick off the download + atomic self-replace + restart. Called
    /// when the operator clicks "Update & Restart" in the release-notes
    /// modal. The modal stays open while download progresses; on
    /// success the process re-execs and this closure never returns.
    fn perform_update(&mut self, cx: &mut Context<Self>) {
        let Some(release) = self.available_update.clone() else {
            return;
        };
        if matches!(
            self.update_download,
            updater::DownloadState::Downloading { .. }
                | updater::DownloadState::Installing
                | updater::DownloadState::Restarting
        ) {
            return; // already running
        }

        self.update_download = updater::DownloadState::Downloading {
            received: 0,
            total: release.size_bytes,
        };
        cx.notify();

        // Progress callback needs to be Send + Sync + 'static. GPUI
        // Entity handles can't be captured directly by non-cx closures,
        // so we use an atomic pair the render loop polls.
        // Simpler approach: use a channel and drain it from a small
        // ticker task.
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(u64, u64)>();
        let progress: updater::ProgressFn = std::sync::Arc::new(move |received, total| {
            let _ = tx.send((received, total));
        });

        // Drain progress into the entity.
        let this_for_progress = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            while let Some((received, total)) = rx.recv().await {
                if this_for_progress
                    .update(cx, |this, cx| {
                        this.update_download =
                            updater::DownloadState::Downloading { received, total };
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();

        // Do the download.
        cx.spawn(async move |this, cx| {
            let install_result = updater::download_and_install(&release, progress).await;
            match install_result {
                Ok(new_exe) => {
                    // Flip UI to "restarting", give the render loop one
                    // frame to paint it, then re-exec.
                    this.update(cx, |this, cx| {
                        this.update_download = updater::DownloadState::Restarting;
                        cx.notify();
                    })
                    .ok();

                    // Short delay so the user sees the state change.
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(250))
                        .await;

                    if let Err(e) = updater::restart(&new_exe) {
                        log::error!("[updater] restart failed: {}", e);
                        this.update(cx, |this, cx| {
                            this.update_download =
                                updater::DownloadState::Failed(format!("Restart failed: {}", e));
                            cx.notify();
                        })
                        .ok();
                    }
                    // If restart() succeeded, this process has exited.
                }
                Err(e) => {
                    log::error!("[updater] install failed: {}", e);
                    this.update(cx, |this, cx| {
                        this.update_download = updater::DownloadState::Failed(format!("{}", e));
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    // ── Menu actions for updates ──────────────────────────────────────────

    fn on_check_for_updates(
        &mut self,
        _: &CheckForUpdates,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.check_for_updates(true, cx);
    }

    fn on_show_update_modal(
        &mut self,
        _: &ShowUpdateModal,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.available_update.is_some() {
            self.update_modal_showing = true;
            cx.notify();
        } else {
            // No update queued — treat this as "check now, and if
            // there's one, open the modal automatically".
            self.check_for_updates(true, cx);
        }
    }

    fn on_dismiss_update_modal(
        &mut self,
        _: &DismissUpdateModal,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_modal_showing = false;
        cx.notify();
    }

    // ── Keyboard shortcuts help modal ─────────────────────────────────────
    //
    // Discoverability affordance. The console has a dozen useful
    // shortcuts (Ctrl+Enter to research, Ctrl+R to simulate, Ctrl+P
    // to publish, Ctrl+1–6 to navigate, …) but no visible surface
    // that lists them — testers currently have to know them or
    // trawl menus. `Ctrl+/` opens the modal; Escape or clicking the
    // backdrop dismisses.

    fn on_show_shortcuts(&mut self, _: &ShowShortcuts, _w: &mut Window, cx: &mut Context<Self>) {
        self.shortcuts_modal_showing = true;
        cx.notify();
    }

    fn on_dismiss_shortcuts(
        &mut self,
        _: &DismissShortcuts,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.shortcuts_modal_showing = false;
        cx.notify();
    }

    /// Populate `invite_share_modal` from a POST /invites response so
    /// the operator sees a one-click Copy Link affordance immediately
    /// after creating an invite. Idempotent w.r.t. previous modal
    /// content — replaces cleanly on repeat.
    ///
    /// `target_label` is a human-readable string like "forecast
    /// 'Will Spain win…'" or "team 'WC-analysts'". Callers own this
    /// because only they know the display name of the target.
    ///
    /// No-op when the invite lacks a shareable token — that means it
    /// was created against a known user_id (surfaces in their inbox
    /// automatically, no link needed).
    fn open_invite_share_modal(
        &mut self,
        invite_json: &JsonValue,
        target_label: String,
        recipient: String,
        cx: &mut Context<Self>,
    ) {
        let token = match invite_json.get("token").and_then(|v| v.as_str()) {
            Some(t) if !t.is_empty() => t.to_string(),
            _ => {
                // Direct user-id invite — no link required. Just toast
                // and move on; the recipient sees it in their inbox.
                self.show_toast("Invite sent to inbox", "✓", theme::GREEN, cx);
                return;
            }
        };
        let permission = invite_json
            .get("permission")
            .and_then(|v| v.as_str())
            .unwrap_or("view")
            .to_string();
        // Server tells us whether Resend dispatch was spawned.
        let email_sent = invite_json
            .get("email_sent")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        // Prefer server-provided invite_url (uses APP_BASE_URL), fall
        // back to constructing locally.
        let invite_url = invite_json
            .get("invite_url")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| {
                let base = self.api.base_url_sync();
                format!("{}/invites/{}", base.trim_end_matches('/'), token)
            });
        self.invite_share_modal = Some(InviteShareModal {
            invite_url,
            target_label,
            recipient,
            permission,
            email_sent,
        });
        cx.notify();
    }

    fn try_connect(&mut self, cx: &mut Context<Self>) {
        if self.api_key_input.is_empty() {
            return;
        }

        let api = self.api.clone();
        let key = self.api_key_input.clone();

        cx.spawn(async move |this, cx| {
            api.set_api_key(&key).await;

            match api.auth_me().await {
                Ok(me) => {
                    this.update(cx, |this, cx| {
                        this.connected = true;
                        this.sign_in_loading = false;
                        this.sign_in_error = None;
                        this.user_display_name = Some(me.friendly_label());
                        log::info!("Connected as: {:?}", me.friendly_label());
                        this.fetch_all_data(cx);
                        this.start_background_refresh(cx);
                    })
                    .ok();
                }
                Err(e) => {
                    log::error!("Auth failed: {}", e);
                    api.clear_api_key().await;
                    this.update(cx, |this, cx| {
                        this.connected = false;
                        this.sign_in_loading = false;
                        this.sign_in_error = Some(format!("Sign in failed: {}", e));
                        this.user_display_name = None;
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn fetch_all_data(&mut self, cx: &mut Context<Self>) {
        self.fetch_stats(cx);
        self.fetch_forecasts(cx);
        self.fetch_portfolios(cx);
        self.fetch_agents(cx);
        self.fetch_leaderboard(cx);
        self.load_local_forecasts();
        self.fetch_workspace_forecasts(cx);
        // Pending cascades queue — the operator's inbox of probability-
        // mutating actions awaiting human approval.
        self.fetch_pending_cascades(cx);
        // All declared relationships — hydrates the Portfolio panel's
        // Relationships sub-panel.
        self.fetch_all_relationships(cx);
        // Collaboration inbox — pending forecast/portfolio/team invites.
        self.fetch_my_invites(cx);
        // Wallet snapshot for the sidebar chip + welcome modal.
        self.fetch_wallet(cx);
        // Teams list — previously fetched lazily on Panel::Teams open
        // or when a share modal needed it. The Dashboard's Teams strip
        // wants it on first render, and the payload is small (bare
        // team headers), so we pull it up-front.
        self.fetch_teams(cx);
        // Shared-with-me forecasts — previously fetched lazily on
        // Portfolio panel's SharedWithMe bucket. Now feeds the
        // Dashboard's activity feed Team-source stream too, so pull
        // it eagerly. Idempotent; the ListForecasts response is
        // small.
        self.fetch_shared_with_me(cx);
    }

    /// Pull the current wallet snapshot. First fetch after sign-in
    /// also decides whether to pop the welcome modal: if the balance
    /// matches the granted amount and nothing has been spent, we treat
    /// this as a brand-new account and celebrate. Idempotent — the
    /// welcome modal only shows once per process because we clear the
    /// dismissed flag but never re-arm it.
    fn fetch_wallet(&mut self, cx: &mut Context<Self>) {
        let api = self.api.clone();
        cx.spawn(async move |this, cx| match api.get_wallet().await {
            Ok(wallet) => {
                this.update(cx, |this, cx| {
                    let is_fresh = this.wallet.is_none()
                        && wallet.total_spent == 0
                        && wallet.granted_balance > 0
                        && wallet.purchased_balance == 0;
                    this.wallet = Some(wallet);
                    if is_fresh {
                        this.welcome_modal_showing = true;
                    }
                    cx.notify();
                })
                .ok();
            }
            Err(e) => {
                log::warn!("[wallet] fetch failed: {}", e);
            }
        })
        .detach();
    }

    /// Kick off a background poll that keeps the dashboard live without
    /// requiring a restart. Fires once, guarded by
    /// `background_refresh_started`. Every `BACKGROUND_REFRESH_INTERVAL`
    /// the loop refreshes forecasts + stats + pending cascades so that
    /// upstream events (PM auto-resolution, a teammate resolving a
    /// shared forecast, workspace-scheduled resolves) surface without
    /// operator intervention.
    ///
    /// Refresh is paused whenever a modal sheet is open (resolve, commit,
    /// pending-cascades review) so a mid-review swap of the underlying
    /// list can't shift the ground under the operator's feet.
    fn start_background_refresh(&mut self, cx: &mut Context<Self>) {
        if self.background_refresh_started {
            return;
        }
        self.background_refresh_started = true;

        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(BACKGROUND_REFRESH_INTERVAL)
                    .await;

                let keep_going = this
                    .update(cx, |this, cx| {
                        if !this.connected {
                            // Session ended; end the loop so we don't
                            // hammer the API in the background.
                            return false;
                        }
                        // Pause during modal reviews — don't yank rows
                        // out from under the operator.
                        if this.resolve_sheet_showing
                            || this.commit_sheet_showing
                            || this.pending_cascades_sheet_showing
                        {
                            return true;
                        }
                        this.fetch_pending_cascades(cx);
                        this.fetch_forecasts(cx);
                        this.fetch_stats(cx);
                        true
                    })
                    .ok()
                    .unwrap_or(false);

                if !keep_going {
                    break;
                }
            }
        })
        .detach();
    }

    fn fetch_agents(&mut self, cx: &mut Context<Self>) {
        self.agents_loading = true;
        let api = self.api.clone();

        cx.spawn(async move |this, cx| match api.list_agents().await {
            Ok(data) => {
                let cards = data
                    .as_array()
                    .cloned()
                    .or_else(|| data.get("agents").and_then(|a| a.as_array()).cloned())
                    .unwrap_or_default();

                this.update(cx, |this, cx| {
                    this.agent_cards = cards;
                    this.agents_loading = false;
                    cx.notify();
                })
                .ok();
            }
            Err(e) => {
                log::error!("Failed to fetch agents: {}", e);
                this.update(cx, |this, cx| {
                    this.agents_loading = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn fetch_leaderboard(&mut self, cx: &mut Context<Self>) {
        self.leaderboard_loading = true;
        let api = self.api.clone();

        cx.spawn(async move |this, cx| {
            let query = LeaderboardQuery {
                domain: None,
                team_id: None,
                min_forecasts: Some(3),
                limit: Some(50),
                offset: None,
            };

            match api.leaderboard(&query).await {
                Ok(resp) => {
                    this.update(cx, |this, cx| {
                        this.leaderboard = resp.leaderboard;
                        this.leaderboard_loading = false;
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::error!("Failed to fetch leaderboard: {}", e);
                    this.update(cx, |this, cx| {
                        this.leaderboard_loading = false;
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    // ── Teams (Spec 24 §3.5.4) ─────────────────────────────────────────

    /// Populate `forecast_team_shares` for own forecasts that have
    /// non-zero `share_count`. One /api/forecasts/:id/shares call per
    /// eligible forecast — skipped when the share_count is zero (nothing
    /// to fetch) and when the id is already in-flight or resolved.
    ///
    /// Fires-and-forgets; failures log and are silently ignored so a
    /// single 403/500 doesn't stall the dot-rendering for the rest of
    /// the book. Cheap enough that we call it after every
    /// `fetch_forecasts` refresh: real users have O(10) shared
    /// forecasts, not O(1000).
    fn refresh_forecast_shares_cache(&mut self, cx: &mut Context<Self>) {
        if !self.connected {
            return;
        }
        let mut targets: Vec<String> = Vec::new();
        for f in self
            .active_forecasts
            .iter()
            .chain(self.draft_forecasts.iter())
            .chain(self.resolved_forecasts.iter())
        {
            let has_shares = f.share_count.unwrap_or(0) > 0;
            if !has_shares {
                continue;
            }
            if self.forecast_team_shares.contains_key(&f.id) {
                continue;
            }
            if !self.forecast_shares_in_flight.insert(f.id.clone()) {
                continue;
            }
            targets.push(f.id.clone());
        }
        for fid in targets {
            let api = self.api.clone();
            cx.spawn(async move |this, cx| {
                let result = api.list_forecast_shares(&fid).await;
                this.update(cx, |this, cx| {
                    this.forecast_shares_in_flight.remove(&fid);
                    if let Ok(resp) = result {
                        let team_ids: Vec<String> = resp
                            .shares
                            .into_iter()
                            .filter(|s| s.share_type == "team")
                            .map(|s| s.share_target)
                            .collect();
                        this.forecast_team_shares.insert(fid.clone(), team_ids);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    /// Return the primary team_id to use when colouring a forecast's
    /// team-dot. Precedence:
    ///   1. The forecast's owning `team_id` (Spec 24 §3.5.6).
    ///   2. The first team-share in `forecast_team_shares` for its id.
    ///   3. None — no dot rendered.
    fn primary_team_id_for_forecast(&self, forecast: &Forecast) -> Option<String> {
        if let Some(ref tid) = forecast.team_id {
            if !tid.is_empty() {
                return Some(tid.clone());
            }
        }
        self.forecast_team_shares
            .get(&forecast.id)
            .and_then(|v| v.first().cloned())
    }

    /// O(N) lookup of a team's display name by id. N is tiny (users are
    /// in ≤ handful of teams); no need for a HashMap.
    fn team_name_by_id(&self, team_id: &str) -> Option<String> {
        self.teams
            .iter()
            .find(|t| t.id == team_id)
            .map(|t| t.name.clone())
    }

    /// Portfolio counterpart of `refresh_forecast_shares_cache`. Walks
    /// the portfolio list and fires one /api/portfolios/:id/shares call
    /// per portfolio we haven't already resolved. Unlike forecasts we
    /// don't have a `share_count` hint on the portfolio row, so we
    /// fan out for all portfolios — acceptable because users have
    /// O(10) portfolios, not O(hundreds).
    fn refresh_portfolio_shares_cache(&mut self, cx: &mut Context<Self>) {
        if !self.connected {
            return;
        }
        let mut targets: Vec<String> = Vec::new();
        for p in &self.portfolios {
            if self.portfolio_team_shares.contains_key(&p.id) {
                continue;
            }
            if !self.portfolio_shares_in_flight.insert(p.id.clone()) {
                continue;
            }
            targets.push(p.id.clone());
        }
        for pid in targets {
            let api = self.api.clone();
            cx.spawn(async move |this, cx| {
                let result = api.list_portfolio_shares(&pid).await;
                this.update(cx, |this, cx| {
                    this.portfolio_shares_in_flight.remove(&pid);
                    if let Ok(resp) = result {
                        let team_ids: Vec<String> = resp
                            .shares
                            .into_iter()
                            .filter(|s| s.share_type == "team")
                            .map(|s| s.share_target)
                            .collect();
                        this.portfolio_team_shares.insert(pid.clone(), team_ids);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    /// Return all forecasts associated with a team:
    ///   1. Own forecasts whose owning `team_id` matches, OR
    ///   2. Own forecasts with a team-share pointing at this team, OR
    ///   3. Shared-with-me forecasts whose owning `team_id` matches.
    ///
    /// Bucketed newest-first by updated_at (fallback created_at).
    /// Cheap to compute because everything is already in memory —
    /// this is a filter, not a fetch.
    fn forecasts_for_team(&self, team_id: &str) -> Vec<&Forecast> {
        let mut out: Vec<&Forecast> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        let matches_own = |f: &Forecast| -> bool {
            if f.team_id.as_deref() == Some(team_id) {
                return true;
            }
            self.forecast_team_shares
                .get(&f.id)
                .map(|v| v.iter().any(|t| t == team_id))
                .unwrap_or(false)
        };

        for f in self
            .active_forecasts
            .iter()
            .chain(self.draft_forecasts.iter())
            .chain(self.resolved_forecasts.iter())
        {
            if matches_own(f) && seen.insert(f.id.clone()) {
                out.push(f);
            }
        }
        // Shared-with-me only qualifies via owning team_id — the
        // object_shares fan-out is only run on OWN forecasts.
        for f in &self.shared_with_me_forecasts {
            if f.team_id.as_deref() == Some(team_id) && seen.insert(f.id.clone()) {
                out.push(f);
            }
        }

        out.sort_by(|a, b| {
            let ka = a.updated_at.as_ref().or(a.created_at.as_ref());
            let kb = b.updated_at.as_ref().or(b.created_at.as_ref());
            kb.cmp(&ka)
        });
        out
    }

    /// Return all portfolios associated with a team. Same shape as
    /// `forecasts_for_team`: owning team match OR team-share match.
    fn portfolios_for_team(&self, team_id: &str) -> Vec<&Portfolio> {
        let mut out: Vec<&Portfolio> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

        for p in &self.portfolios {
            let owned = p.team_id.as_deref() == Some(team_id);
            let shared = self
                .portfolio_team_shares
                .get(&p.id)
                .map(|v| v.iter().any(|t| t == team_id))
                .unwrap_or(false);
            if (owned || shared) && seen.insert(p.id.clone()) {
                out.push(p);
            }
        }

        out.sort_by(|a, b| {
            let ka = a.updated_at.as_ref().or(a.created_at.as_ref());
            let kb = b.updated_at.as_ref().or(b.created_at.as_ref());
            kb.cmp(&ka)
        });
        out
    }

    /// Fetch the user's teams for the left pane. Auto-selects the first
    /// team (and loads its detail) when none is selected yet.
    fn fetch_teams(&mut self, cx: &mut Context<Self>) {
        self.teams_loading = true;
        let api = self.api.clone();

        cx.spawn(async move |this, cx| match api.list_my_teams().await {
            Ok(resp) => {
                this.update(cx, |this, cx| {
                    // ABW is shared substrate: /api/teams returns every
                    // vertical's teams (rabble swarms, kask workspaces, …).
                    // The Fermi console only manages fermi_forecast teams.
                    // Filter twice: the fermi vertical AND out the
                    // auto-created workspace-prior team wrappers (one
                    // per Team-Prior workspace, 62+ for the WC event)
                    // that would otherwise drown out real collaboration
                    // teams in the left pane.
                    this.teams = resp
                        .teams
                        .into_iter()
                        .filter(is_collaboration_team)
                        .collect();
                    this.teams_loading = false;
                    // Keep the current selection if it's still present,
                    // otherwise default to the first team.
                    let still_valid = this
                        .selected_team_id
                        .as_ref()
                        .map(|id| this.teams.iter().any(|t| &t.id == id))
                        .unwrap_or(false);
                    if !still_valid {
                        this.selected_team_id = this.teams.first().map(|t| t.id.clone());
                        this.selected_team_detail = None;
                    }
                    if let Some(id) = this.selected_team_id.clone() {
                        this.fetch_team_detail(&id, cx);
                    }
                    // Warm the shared team_details cache in the
                    // background so the Dashboard's team cards can
                    // render member counts + hover-reveal rosters
                    // without the operator having to open each team
                    // manually.
                    this.refresh_team_details_cache(cx);
                    cx.notify();
                })
                .ok();
            }
            Err(e) => {
                log::error!("Failed to fetch teams: {}", e);
                this.update(cx, |this, cx| {
                    this.teams_loading = false;
                    this.team_action_error = Some(e.to_string());
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Background fan-out: warm the `team_details` cache for every
    /// team the operator is a member of. Powers the Dashboard's team
    /// cards (member counts + hover-reveal roster). Cheap because
    /// users are in ≤ handful of teams; skipped for teams already in
    /// the cache or in-flight.
    ///
    /// Fires-and-forgets each request; a single 403/500 doesn't stall
    /// the rest. Called at the tail of `fetch_teams`.
    fn refresh_team_details_cache(&mut self, cx: &mut Context<Self>) {
        if !self.connected {
            return;
        }
        let mut targets: Vec<String> = Vec::new();
        for t in &self.teams {
            if self.team_details.contains_key(&t.id) {
                continue;
            }
            if !self.team_details_in_flight.insert(t.id.clone()) {
                continue;
            }
            targets.push(t.id.clone());
        }
        for tid in targets {
            let api = self.api.clone();
            cx.spawn(async move |this, cx| {
                let result = api.get_team_detail(&tid).await;
                this.update(cx, |this, cx| {
                    this.team_details_in_flight.remove(&tid);
                    if let Ok(detail) = result {
                        this.team_details.insert(tid.clone(), detail);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    /// Load the member roster for one team into the right pane. Also
    /// kicks off a parallel fetch of the team's pending/terminal invite
    /// list so the operator sees invitations they've sent alongside the
    /// members list — the two views together tell the whole story.
    fn fetch_team_detail(&mut self, team_id: &str, cx: &mut Context<Self>) {
        self.team_detail_loading = true;
        let api = self.api.clone();
        let team_id = team_id.to_string();
        // Kick off the invite fetch in parallel with the detail fetch;
        // both target the same team_id but have disjoint state slots.
        self.fetch_team_invites(team_id.clone(), cx);

        cx.spawn(
            async move |this, cx| match api.get_team_detail(&team_id).await {
                Ok(detail) => {
                    this.update(cx, |this, cx| {
                        // Stash in the shared cache so the Dashboard's
                        // team cards pick up the fresh roster too.
                        this.team_details
                            .insert(detail.team.id.clone(), detail.clone());
                        // Ignore stale responses if the user clicked away.
                        if this.selected_team_id.as_deref() == Some(detail.team.id.as_str()) {
                            this.selected_team_detail = Some(detail);
                        }
                        this.team_detail_loading = false;
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::error!("Failed to fetch team detail: {}", e);
                    this.update(cx, |this, cx| {
                        this.team_detail_loading = false;
                        this.team_action_error = Some(e.to_string());
                        cx.notify();
                    })
                    .ok();
                }
            },
        )
        .detach();
    }

    /// Fetch the pending+terminal invite list for a team into
    /// `team_invites`. Called by `fetch_team_detail` and after every
    /// send/revoke action so the UI stays in sync.
    fn fetch_team_invites(&mut self, team_id: String, cx: &mut Context<Self>) {
        self.team_invites_loading = true;
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = api.list_team_invites(&team_id).await;
            this.update(cx, |this, cx| {
                this.team_invites_loading = false;
                match result {
                    Ok(resp) => {
                        // Guard against stale responses if the user clicked away.
                        if this.selected_team_id.as_deref() == Some(team_id.as_str()) {
                            this.team_invites = resp.invites;
                        }
                    }
                    Err(e) => {
                        log::warn!("Failed to fetch team invites: {}", e);
                        // Non-fatal — leave existing list alone so a
                        // transient blip doesn't clear the UI.
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Revoke a pending invite for the selected team, then refresh both
    /// the members roster (no-op since it hasn't accepted) and the
    /// invite list (which will show status='revoked' next fetch).
    fn revoke_team_invite(&mut self, invite_id: String, cx: &mut Context<Self>) {
        if !self.team_invite_revoke_in_flight.insert(invite_id.clone()) {
            return;
        }
        cx.notify();
        let api = self.api.clone();
        let team_id = self.selected_team_id.clone();
        cx.spawn(async move |this, cx| {
            let result = api.revoke_invite(&invite_id).await;
            this.update(cx, |this, cx| {
                this.team_invite_revoke_in_flight.remove(&invite_id);
                match result {
                    Ok(()) => {
                        this.show_toast("Invite revoked", "✓", theme::FG_DIM, cx);
                        if let Some(tid) = team_id {
                            this.fetch_team_invites(tid, cx);
                        }
                    }
                    Err(e) => this.show_toast(format!("Revoke failed: {}", e), "✕", theme::RED, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn select_team(&mut self, team_id: String, cx: &mut Context<Self>) {
        if self.selected_team_id.as_deref() == Some(team_id.as_str()) {
            return;
        }
        self.selected_team_id = Some(team_id.clone());
        self.selected_team_detail = None;
        self.team_invites.clear();
        self.team_invite_showing = false;
        self.team_action_error = None;
        // Drop any pending delete-confirmation from the previous team
        // so a stale confirm can't accidentally delete the newly
        // selected team on the next click.
        self.team_delete_confirm_id = None;
        self.fetch_team_detail(&team_id, cx);
        cx.notify();
    }

    /// Create a team from the modal inputs, then refresh the team list.
    fn create_team_from_input(&mut self, cx: &mut Context<Self>) {
        let name = self
            .team_create_name_input
            .read(cx)
            .text()
            .trim()
            .to_string();
        let slug = self
            .team_create_slug_input
            .read(cx)
            .text()
            .trim()
            .to_string();
        if name.is_empty() || slug.is_empty() {
            self.team_create_error = Some("Name and slug are required".into());
            cx.notify();
            return;
        }
        self.team_create_loading = true;
        self.team_create_error = None;
        cx.notify();

        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let req = CreateTeamRequest {
                name,
                slug,
                description: None,
                // Tag as a fermi team so it shows in the (origin-scoped)
                // Teams panel and not in other verticals' lists.
                origin: Some("fermi_forecast".into()),
            };
            match api.create_team(&req).await {
                Ok(team_json) => {
                    let new_id = team_json
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    this.update(cx, |this, cx| {
                        this.team_create_loading = false;
                        this.team_create_showing = false;
                        this.team_create_error = None;
                        if let Some(id) = new_id {
                            this.selected_team_id = Some(id);
                            this.selected_team_detail = None;
                        }
                        this.fetch_teams(cx);
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    this.update(cx, |this, cx| {
                        this.team_create_loading = false;
                        this.team_create_error = Some(e.to_string());
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// Invite a member to the selected team. If the input is an email we
    /// send it as `invitee_email` (server resolves to a user / queues a
    /// pending invite); otherwise we treat it as a `user_id`.
    fn invite_team_member_from_input(&mut self, cx: &mut Context<Self>) {
        let Some(team_id) = self.selected_team_id.clone() else {
            return;
        };
        let raw = self.team_invite_input.read(cx).text().trim().to_string();
        if raw.is_empty() {
            return;
        }
        self.team_invite_loading = true;
        self.team_action_error = None;
        cx.notify();

        let role = self.team_invite_role.clone();
        let api = self.api.clone();
        let invite_input = self.team_invite_input.clone();
        cx.spawn(async move |this, cx| {
            let is_email = raw.contains('@');
            let req = InviteRequest {
                invitee_user_id: (!is_email).then(|| raw.clone()),
                invitee_email: is_email.then(|| raw.clone()),
                permission: role,
                message: None,
            };
            let result = api.invite_to_team(&team_id, &req).await;
            this.update(cx, |this, cx| {
                this.team_invite_loading = false;
                match result {
                    Ok(invite_json) => {
                        this.team_invite_showing = false;
                        invite_input.update(cx, |inp, cx| inp.set_text("", cx));
                        // Look up the team label for the modal so the
                        // operator sees "team ‘WC-analysts’" rather than
                        // a raw UUID.
                        let team_label = this
                            .teams
                            .iter()
                            .find(|t| t.id == team_id)
                            .map(|t| format!("team ‘{}’", t.name))
                            .unwrap_or_else(|| format!("team {}", team_id));
                        this.open_invite_share_modal(&invite_json, team_label, raw.clone(), cx);
                        // Refresh both the roster (in case the invitee
                        // was already a member and it degenerated to
                        // an idempotent noop) AND the invite list so
                        // the pending row appears immediately.
                        this.fetch_team_detail(&team_id, cx);
                    }
                    Err(e) => {
                        this.team_action_error = Some(e.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Remove a member from the selected team, then refresh the roster.
    fn remove_team_member(&mut self, member_id: String, cx: &mut Context<Self>) {
        let Some(team_id) = self.selected_team_id.clone() else {
            return;
        };
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = api.remove_team_member(&team_id, &member_id).await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.show_toast("Member removed", "✓", theme::GREEN, cx);
                        this.fetch_team_detail(&team_id, cx);
                    }
                    Err(e) => this.team_action_error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Two-step team deletion. First call arms the confirmation
    /// (button flips to red "Really delete?"); second call actually
    /// fires the DELETE. This is a destructive irreversible action —
    /// once the team is gone all object_shares/invites that target it
    /// become orphan rows (correctly denied by the visibility model,
    /// but not automatically revoked), so a single misclick shouldn't
    /// wipe out group access.
    ///
    /// Only the team OWNER can delete — the server enforces via
    /// `WHERE id = $1 AND owner_id = $2`. Non-owners get 403 which
    /// we surface via team_action_error.
    fn delete_selected_team(&mut self, cx: &mut Context<Self>) {
        let Some(team_id) = self.selected_team_id.clone() else {
            return;
        };
        // Two-step: arm on first click, fire on second.
        if self.team_delete_confirm_id.as_deref() != Some(team_id.as_str()) {
            self.team_delete_confirm_id = Some(team_id);
            self.team_action_error = None;
            cx.notify();
            // Auto-cancel the arm after 5 s so a stale red button
            // can't sit there indefinitely and get clicked absent-
            // mindedly later.
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(5))
                    .await;
                this.update(cx, |this, cx| {
                    if !this.team_delete_loading {
                        this.team_delete_confirm_id = None;
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
            return;
        }
        // Second click: fire.
        self.team_delete_loading = true;
        self.team_action_error = None;
        cx.notify();
        let api = self.api.clone();
        let team_name = self
            .selected_team_detail
            .as_ref()
            .map(|d| d.team.name.clone())
            .unwrap_or_else(|| "team".to_string());
        cx.spawn(async move |this, cx| {
            let result = api.delete_team(&team_id).await;
            this.update(cx, |this, cx| {
                this.team_delete_loading = false;
                this.team_delete_confirm_id = None;
                match result {
                    Ok(()) => {
                        this.show_toast(
                            format!("Deleted team “{}”", team_name),
                            "✓",
                            theme::GREEN,
                            cx,
                        );
                        // Drop the selection, clear detail state,
                        // then refetch the team list. fetch_teams
                        // will auto-select the next available team
                        // (or leave selection empty on last delete).
                        this.selected_team_id = None;
                        this.selected_team_detail = None;
                        this.team_invites.clear();
                        this.fetch_teams(cx);
                    }
                    Err(e) => {
                        this.team_action_error = Some(e.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── Inbox / invites (Spec 24 §3.5.5) ───────────────────────────────

    fn fetch_my_invites(&mut self, cx: &mut Context<Self>) {
        self.inbox_loading = true;
        let api = self.api.clone();
        cx.spawn(async move |this, cx| match api.list_my_invites().await {
            Ok(resp) => {
                this.update(cx, |this, cx| {
                    this.inbox_invites = resp.invites;
                    this.inbox_loading = false;
                    cx.notify();
                })
                .ok();
            }
            Err(e) => {
                log::error!("Failed to fetch invites: {}", e);
                this.update(cx, |this, cx| {
                    this.inbox_loading = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn accept_invite(&mut self, invite_id: String, cx: &mut Context<Self>) {
        if !self.inbox_action_in_flight.insert(invite_id.clone()) {
            return; // already in flight
        }
        cx.notify();
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = api.accept_invite(&invite_id).await;
            this.update(cx, |this, cx| {
                this.inbox_action_in_flight.remove(&invite_id);
                match result {
                    Ok(_) => {
                        this.show_toast("Invite accepted", "✓", theme::GREEN, cx);
                        // Refresh inbox + content the grant now exposes.
                        this.fetch_my_invites(cx);
                        this.fetch_forecasts(cx);
                        this.fetch_portfolios(cx);
                    }
                    Err(e) => this.show_toast(format!("Accept failed: {}", e), "✕", theme::RED, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn decline_invite(&mut self, invite_id: String, cx: &mut Context<Self>) {
        if !self.inbox_action_in_flight.insert(invite_id.clone()) {
            return;
        }
        cx.notify();
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = api.decline_invite(&invite_id).await;
            this.update(cx, |this, cx| {
                this.inbox_action_in_flight.remove(&invite_id);
                match result {
                    Ok(_) => {
                        this.show_toast("Invite declined", "✓", theme::FG_DIM, cx);
                        this.fetch_my_invites(cx);
                    }
                    Err(e) => {
                        this.show_toast(format!("Decline failed: {}", e), "✕", theme::RED, cx)
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    // ── Portfolio Access panel (Spec 24 §3.5.3) ────────────────────────

    /// Toggle the portfolio Access panel, loading shares on open.
    fn toggle_portfolio_share(&mut self, cx: &mut Context<Self>) {
        self.portfolio_share_showing = !self.portfolio_share_showing;
        self.portfolio_share_error = None;
        if self.portfolio_share_showing {
            if let Some(pid) = self.selected_portfolio_id.clone() {
                if self.portfolio_shares_loaded_for.as_deref() != Some(pid.as_str()) {
                    self.load_portfolio_shares(&pid, cx);
                }
            }
            // Team-pill picker in the Access panel needs the user's
            // collaboration teams. `fetch_teams` is idempotent-ish (it
            // just re-hits /api/teams); skip when we already have some
            // to avoid a redundant round-trip on every toggle.
            if self.teams.is_empty() && !self.teams_loading {
                self.fetch_teams(cx);
            }
        }
        cx.notify();
    }

    /// Share the currently-selected portfolio with a team. Mirrors the
    /// forecast-level `share_with_team` in the cockpit — same server
    /// endpoint family (object_shares with share_type='team'), same
    /// in-flight guardrail against double-clicks.
    fn share_portfolio_with_team(&mut self, team_id: String, cx: &mut Context<Self>) {
        let Some(pid) = self.selected_portfolio_id.clone() else {
            return;
        };
        if !self.portfolio_team_share_in_flight.insert(team_id.clone()) {
            return;
        }
        self.portfolio_share_error = None;
        cx.notify();
        let api = self.api.clone();
        let permission = self.portfolio_share_permission.clone();
        cx.spawn(async move |this, cx| {
            let body = ShareRequest {
                share_type: "team".into(),
                share_target: team_id.clone(),
                permission: Some(permission),
            };
            let result = api.add_portfolio_share(&pid, &body).await;
            this.update(cx, |this, cx| {
                this.portfolio_team_share_in_flight.remove(&team_id);
                match result {
                    Ok(_) => {
                        // Refresh so the new team-share row appears with
                        // its resolved display name.
                        this.portfolio_shares_loaded_for = None;
                        this.load_portfolio_shares(&pid, cx);
                        this.show_toast("Shared with team", "✓", theme::GREEN, cx);
                    }
                    Err(e) => {
                        this.portfolio_share_error = Some(e.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn load_portfolio_shares(&mut self, portfolio_id: &str, cx: &mut Context<Self>) {
        self.portfolio_shares_loading = true;
        self.portfolio_share_error = None;
        self.portfolio_shares_loaded_for = Some(portfolio_id.to_string());
        cx.notify();
        let api = self.api.clone();
        let pid = portfolio_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = api.list_portfolio_shares(&pid).await;
            this.update(cx, |this, cx| {
                this.portfolio_shares_loading = false;
                match result {
                    Ok(resp) => {
                        // Ignore if the user switched portfolios meanwhile.
                        if this.selected_portfolio_id.as_deref() == Some(pid.as_str()) {
                            this.portfolio_shares = resp.shares;
                        }
                    }
                    Err(e) => this.portfolio_share_error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn add_portfolio_share_from_input(&mut self, cx: &mut Context<Self>) {
        let Some(pid) = self.selected_portfolio_id.clone() else {
            return;
        };
        let raw = self
            .portfolio_share_input
            .read(cx)
            .text()
            .trim()
            .to_string();
        if raw.is_empty() {
            return;
        }
        self.portfolio_share_error = None;
        cx.notify();
        let api = self.api.clone();
        let permission = self.portfolio_share_permission.clone();
        let input = self.portfolio_share_input.clone();
        cx.spawn(async move |this, cx| {
            let is_email = raw.contains('@');
            let resolved = if is_email {
                api.lookup_user(&raw)
                    .await
                    .ok()
                    .flatten()
                    .map(|u| u.user_id)
            } else {
                Some(raw.clone())
            };
            // Two shapes of Ok: direct share (nothing to copy) vs.
            // email-invite (returns full invite json with token — pop
            // the share modal). Err carries the failure string.
            enum ShareResult {
                Shared,
                Invited(JsonValue),
            }
            let result: Result<ShareResult, String> = match resolved {
                Some(user_id) => {
                    let body = ShareRequest {
                        share_type: "user".into(),
                        share_target: user_id,
                        permission: Some(permission),
                    };
                    api.add_portfolio_share(&pid, &body)
                        .await
                        .map(|_| ShareResult::Shared)
                        .map_err(|e| e.to_string())
                }
                None => {
                    let body = InviteRequest {
                        invitee_user_id: None,
                        invitee_email: Some(raw.clone()),
                        permission,
                        message: None,
                    };
                    api.invite_to_portfolio(&pid, &body)
                        .await
                        .map(ShareResult::Invited)
                        .map_err(|e| e.to_string())
                }
            };
            this.update(cx, |this, cx| {
                match result {
                    Ok(ShareResult::Shared) => {
                        input.update(cx, |inp, cx| inp.set_text("", cx));
                        this.portfolio_shares_loaded_for = None;
                        this.load_portfolio_shares(&pid, cx);
                        this.show_toast("Shared with existing user", "✓", theme::GREEN, cx);
                    }
                    Ok(ShareResult::Invited(invite_json)) => {
                        input.update(cx, |inp, cx| inp.set_text("", cx));
                        this.portfolio_shares_loaded_for = None;
                        this.load_portfolio_shares(&pid, cx);
                        // Portfolio label for the modal — look up the
                        // display title from the current list.
                        let pf_label = this
                            .portfolios
                            .iter()
                            .find(|p| p.id == pid)
                            .map(|p| format!("portfolio ‘{}’", p.title))
                            .unwrap_or_else(|| format!("portfolio {}", pid));
                        this.open_invite_share_modal(&invite_json, pf_label, raw.clone(), cx);
                    }
                    Err(e) => this.portfolio_share_error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn revoke_portfolio_share(&mut self, share_id: String, cx: &mut Context<Self>) {
        let Some(pid) = self.selected_portfolio_id.clone() else {
            return;
        };
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = api.revoke_portfolio_share(&pid, &share_id).await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(()) => {
                        this.portfolio_shares_loaded_for = None;
                        this.load_portfolio_shares(&pid, cx);
                    }
                    Err(e) => this.portfolio_share_error = Some(e.to_string()),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Renders the collapsible "⛓ Relationships" sub-panel (Sprint B).
    ///
    /// Filters `all_relationships` by `forecast_ids ∩ forecasts_in_portfolio`
    /// so operators only see cascade rules that touch this portfolio.
    /// When collapsed, shows just a count. When expanded, shows each
    /// relationship as a row and offers a "+ Declare relationship"
    /// inline sheet.
    fn render_relationships_panel(
        &self,
        portfolio_forecasts: &[PortfolioForecast],
        cx: &Context<Self>,
    ) -> impl IntoElement {
        // Set of forecast_ids in this portfolio — used to filter the
        // global relationships list down to what's relevant here.
        let portfolio_fids: std::collections::HashSet<String> =
            portfolio_forecasts.iter().map(|f| f.id.clone()).collect();

        // Filter to relationships whose forecast_ids intersect this
        // portfolio's forecasts. Empty intersection = irrelevant.
        let relevant: Vec<&JsonValue> = self
            .all_relationships
            .iter()
            .filter(|r| {
                r.get("forecast_ids")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .any(|s| portfolio_fids.contains(s))
                    })
                    .unwrap_or(false)
            })
            .collect();
        let n_relevant = relevant.len();

        let mut container = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(14.0))
            .py(px(8.0))
            .border_b_1()
            .border_color(theme::fg_faint())
            // Header row — title + counts + toggle chevron + declare button.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("relationships-toggle")
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.relationships_showing = !this.relationships_showing;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(if self.relationships_showing {
                                        theme::cyan()
                                    } else {
                                        theme::fg_dim()
                                    })
                                    .child(if self.relationships_showing {
                                        "▾"
                                    } else {
                                        "▸"
                                    }),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_faint())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(format!("⛓ RELATIONSHIPS ({})", n_relevant)),
                            ),
                    )
                    .child(div().flex_grow())
                    .when(self.relationships_showing, |el| {
                        el.child(
                            div()
                                .id("rel-declare-toggle")
                                .px(px(10.0))
                                .py(px(3.0))
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(rgb(theme::CYAN))
                                .text_size(px(10.0))
                                .text_color(rgb(theme::CYAN))
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::bg_hover()))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.relationship_create_showing =
                                        !this.relationship_create_showing;
                                    this.relationship_create_error = None;
                                    cx.notify();
                                }))
                                .child(if self.relationship_create_showing {
                                    "Cancel"
                                } else {
                                    "+ Declare"
                                }),
                        )
                    }),
            );

        if !self.relationships_showing {
            return container;
        }

        // Inline create sheet.
        if self.relationship_create_showing {
            container =
                container.child(self.render_relationship_create_sheet(portfolio_forecasts, cx));
        }

        // Existing relationships list. Titles look up via portfolio
        // forecasts (best-effort — relationships can involve forecasts
        // outside this portfolio too, which we render as "other").
        let title_lookup: std::collections::HashMap<String, String> = portfolio_forecasts
            .iter()
            .map(|f| (f.id.clone(), f.question_text.clone()))
            .collect();

        if relevant.is_empty() {
            container = container.child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .py(px(6.0))
                    .child(
                        "No cascades declared yet. Use + Declare to link forecasts — \
                         e.g. mutex on all group-stage sim forecasts so an elimination \
                         propagates.",
                    ),
            );
        } else {
            for rel in relevant {
                container = container.child(self.render_relationship_row(rel, &title_lookup, cx));
            }
        }

        container
    }

    /// Single row in the Relationships list: kind + count + description.
    fn render_relationship_row(
        &self,
        rel: &JsonValue,
        title_lookup: &std::collections::HashMap<String, String>,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let id = rel
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let kind = rel
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let forecast_ids: Vec<String> = rel
            .get("forecast_ids")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let description = rel
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let parameters = rel
            .get("parameters")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let n_in_portfolio = forecast_ids
            .iter()
            .filter(|fid| title_lookup.contains_key(*fid))
            .count();
        let n_total = forecast_ids.len();

        // Kind-specific accent + label.
        let (accent, kind_label) = match kind {
            "mutually_exclusive" | "mutex" => (theme::PURPLE, "MUTEX"),
            "logical_implies" | "implies" => (theme::CYAN, "IMPLIES"),
            "conjunction" => (theme::CYAN, "CONJUNCTION"),
            "conditional" => (theme::GOLD, "CONDITIONAL"),
            "exhaustive_cover" => (theme::GREEN, "EXHAUSTIVE"),
            "at_most_n" => (theme::ORANGE, "AT MOST N"),
            _ => (theme::FG_DIM, "OTHER"),
        };
        let param_str = if let Some(n_val) = parameters.get("n").and_then(|v| v.as_i64()) {
            format!(" n={}", n_val)
        } else {
            String::new()
        };

        let id_for_delete = id.clone();
        let delete_in_flight = self.relationship_delete_in_flight.contains(&id);

        // Preview first 3 titles that this row applies to.
        let preview_titles: Vec<String> = forecast_ids
            .iter()
            .filter_map(|fid| title_lookup.get(fid).map(|t| truncate(t, 34)))
            .take(3)
            .collect();
        let preview_str = if preview_titles.is_empty() {
            format!("{} forecasts (outside this portfolio)", n_total)
        } else if n_in_portfolio > preview_titles.len() {
            format!(
                "{}, +{} more",
                preview_titles.join(", "),
                n_in_portfolio - preview_titles.len()
            )
        } else {
            preview_titles.join(", ")
        };

        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(4.0))
            .border_l_2()
            .border_color(rgb(accent))
            .bg(theme::bg_elevated())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(accent))
                            .font_weight(FontWeight::BOLD)
                            .child(format!("{}{}", kind_label, param_str)),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .child(format!("{} forecasts", n_total)),
                    )
                    .child(div().flex_grow())
                    .child(
                        div()
                            .id(SharedString::from(format!("rel-del-{}", id)))
                            .px(px(6.0))
                            .py(px(1.0))
                            .rounded(px(3.0))
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|s| s.text_color(theme::red()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.delete_relationship(id_for_delete.clone(), cx);
                            }))
                            .child(if delete_in_flight { "…" } else { "Remove" }),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::fg())
                    .child(preview_str),
            )
            .when(!description.is_empty(), |el| {
                el.child(
                    div()
                        .text_size(px(9.0))
                        .text_color(theme::fg_faint())
                        .child(description),
                )
            })
    }

    /// Inline "+ Declare relationship" sheet, rendered inside the
    /// Relationships sub-panel.
    fn render_relationship_create_sheet(
        &self,
        portfolio_forecasts: &[PortfolioForecast],
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let kinds: &[(&str, &str, &str)] = &[
            (
                "mutually_exclusive",
                "Mutex",
                "exactly one YES; if one resolves, the rest go NO",
            ),
            (
                "exhaustive_cover",
                "Exhaustive",
                "one and only one is TRUE (partition)",
            ),
            (
                "logical_implies",
                "Implies",
                "if A resolves YES then B resolves YES",
            ),
            ("conjunction", "Conjunction", "all resolve together"),
            (
                "conditional",
                "Conditional",
                "B is scored only if A resolves YES",
            ),
            (
                "at_most_n",
                "At most n",
                "at most n forecasts can resolve YES",
            ),
        ];
        let selected_kind = self.relationship_create_kind.clone();
        let show_n_field = selected_kind == "at_most_n";

        // Selected count for the header hint.
        let n_selected = self.relationship_create_forecast_ids.len();

        // Kind chip row.
        let mut kind_row = div().flex().flex_wrap().gap(px(4.0)).child(
            div()
                .text_size(px(9.0))
                .text_color(theme::fg_faint())
                .child("KIND:"),
        );
        for (key, label, _desc) in kinds {
            let is_on = *key == selected_kind;
            let key_owned = (*key).to_string();
            kind_row = kind_row.child(
                div()
                    .id(SharedString::from(format!("rel-kind-{}", key)))
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(if is_on {
                        theme::cyan()
                    } else {
                        theme::fg_faint()
                    })
                    .bg(if is_on {
                        theme::bg_active()
                    } else {
                        theme::bg_elevated()
                    })
                    .text_size(px(10.0))
                    .text_color(if is_on {
                        theme::cyan()
                    } else {
                        theme::fg_dim()
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::bg_hover()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.relationship_create_kind = key_owned.clone();
                        cx.notify();
                    }))
                    .child(label.to_string()),
            );
        }

        // Kind description hint below the chip row.
        let kind_desc = kinds
            .iter()
            .find(|(k, _, _)| *k == selected_kind)
            .map(|(_, _, d)| *d)
            .unwrap_or("");

        // Forecast picker — all portfolio forecasts as toggleable chips.
        let mut forecast_picker = div().flex().flex_wrap().gap(px(4.0)).child(
            div()
                .text_size(px(9.0))
                .text_color(theme::fg_faint())
                .child(format!("FORECASTS ({} selected):", n_selected)),
        );
        for f in portfolio_forecasts {
            let is_on = self.relationship_create_forecast_ids.contains(&f.id);
            let fid_owned = f.id.clone();
            let title = truncate(&f.question_text, 28);
            forecast_picker = forecast_picker.child(
                div()
                    .id(SharedString::from(format!("rel-fpick-{}", f.id)))
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(3.0))
                    .border_1()
                    .border_color(if is_on {
                        theme::cyan()
                    } else {
                        theme::fg_faint()
                    })
                    .bg(if is_on {
                        theme::bg_active()
                    } else {
                        theme::bg_elevated()
                    })
                    .text_size(px(9.0))
                    .text_color(if is_on {
                        theme::cyan()
                    } else {
                        theme::fg_dim()
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::bg_hover()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.relationship_create_forecast_ids.contains(&fid_owned) {
                            this.relationship_create_forecast_ids.remove(&fid_owned);
                        } else {
                            this.relationship_create_forecast_ids
                                .insert(fid_owned.clone());
                        }
                        cx.notify();
                    }))
                    .child(if is_on {
                        format!("✓ {}", title)
                    } else {
                        title
                    }),
            );
        }

        // n field (only visible for at_most_n).
        let n_field: Option<AnyElement> = if show_n_field {
            let current = self.relationship_create_n.clone();
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(theme::fg_faint())
                            .child("n:"),
                    )
                    // Simple stepper: - / value / +.
                    .child(
                        div()
                            .id("rel-n-dec")
                            .px(px(6.0))
                            .py(px(1.0))
                            .rounded(px(3.0))
                            .border_1()
                            .border_color(theme::fg_faint())
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let cur: i64 =
                                    this.relationship_create_n.trim().parse().unwrap_or(1);
                                this.relationship_create_n = (cur - 1).max(1).to_string();
                                cx.notify();
                            }))
                            .child("–"),
                    )
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(1.0))
                            .text_size(px(11.0))
                            .text_color(theme::cyan())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(current),
                    )
                    .child(
                        div()
                            .id("rel-n-inc")
                            .px(px(6.0))
                            .py(px(1.0))
                            .rounded(px(3.0))
                            .border_1()
                            .border_color(theme::fg_faint())
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                let cur: i64 =
                                    this.relationship_create_n.trim().parse().unwrap_or(1);
                                this.relationship_create_n = (cur + 1).to_string();
                                cx.notify();
                            }))
                            .child("+"),
                    )
                    .into_any_element(),
            )
        } else {
            None
        };

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(10.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .bg(theme::bg())
            .border_1()
            .border_color(rgb(theme::CYAN))
            .child(kind_row)
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(theme::fg_dim())
                    .child(kind_desc.to_string()),
            )
            .child(forecast_picker)
            .children(n_field)
            .child(self.relationship_create_description.clone())
            .when(self.relationship_create_error.is_some(), |el| {
                el.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::red())
                        .child(self.relationship_create_error.clone().unwrap_or_default()),
                )
            })
            .child(
                div().flex().justify_end().gap(px(6.0)).child(
                    div()
                        .id("rel-submit")
                        .px(px(14.0))
                        .py(px(5.0))
                        .rounded(px(6.0))
                        .bg(rgb(theme::CYAN))
                        .text_size(px(11.0))
                        .text_color(rgb(theme::BG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .cursor_pointer()
                        .hover(|s| s.opacity(0.85))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.submit_relationship_create(cx);
                        }))
                        .child(if self.relationship_create_loading {
                            "Declaring…"
                        } else {
                            "Declare"
                        }),
                ),
            )
    }

    /// Renders the collapsible portfolio Access panel (Spec 24 §3.5.3).
    fn render_portfolio_access_panel(&self, cx: &Context<Self>) -> impl IntoElement {
        let perm = self.portfolio_share_permission.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(14.0))
            .py(px(10.0))
            .border_b_1()
            .border_color(theme::fg_faint())
            .bg(theme::bg())
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::cyan())
                    .child("🔗 Access"),
            )
            // Add row
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().flex_grow().child(self.portfolio_share_input.clone()))
                    .child(
                        div()
                            .id("pf-share-perm")
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .bg(rgb(theme::BG_ACTIVE))
                            .text_size(px(11.0))
                            .text_color(theme::gold())
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                            .on_click(cx.listener(|this, _, _w, cx| {
                                this.portfolio_share_permission =
                                    match this.portfolio_share_permission.as_str() {
                                        "view" => "edit",
                                        "edit" => "admin",
                                        _ => "view",
                                    }
                                    .into();
                                cx.notify();
                            }))
                            .child(perm),
                    )
                    .child(
                        div()
                            .id("pf-share-add")
                            .px(px(12.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .bg(rgb(theme::BLUE))
                            .text_size(px(11.0))
                            .text_color(rgb(theme::BG))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.85))
                            .on_click(cx.listener(|this, _, _w, cx| {
                                this.add_portfolio_share_from_input(cx);
                            }))
                            .child("Add"),
                    ),
            )
            .when(self.portfolio_share_error.is_some(), |el| {
                el.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::red())
                        .child(self.portfolio_share_error.clone().unwrap_or_default()),
                )
            })
            .child(div().text_size(px(10.0)).text_color(theme::fg_dim()).child(
                if self.portfolio_shares_loading {
                    "Loading shares…".to_string()
                } else {
                    format!("Shared with ({})", self.portfolio_shares.len())
                },
            ))
            .children(self.portfolio_shares.iter().map(|s| {
                let sid = s.id.clone();
                let icon = if s.share_type == "team" {
                    "👥"
                } else {
                    "🧑"
                };
                let primary_label = s
                    .share_target_display_name
                    .clone()
                    .unwrap_or_else(|| short_user_label(&s.share_target));
                let show_subtitle = s.share_target_display_name.is_some();
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .bg(theme::bg_elevated())
                    .child(div().text_size(px(12.0)).child(icon))
                    .child(
                        div()
                            .flex_grow()
                            .overflow_hidden()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::fg())
                                    .child(primary_label),
                            )
                            .when(show_subtitle, |el| {
                                el.child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(theme::fg_faint())
                                        .child(short_user_label(&s.share_target)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .bg(theme::bg_active())
                            .text_size(px(10.0))
                            .text_color(theme::gold())
                            .child(s.permission.clone()),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("pf-share-rm-{}", sid)))
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()).text_color(theme::red()))
                            .on_click(cx.listener({
                                let sid = sid.clone();
                                move |this, _, _w, cx| {
                                    this.revoke_portfolio_share(sid.clone(), cx);
                                }
                            }))
                            .child("✕"),
                    )
            }))
            // Team-pill picker — same shape as the forecast Access tab's
            // team share section so portfolio-level and forecast-level
            // sharing feel identical to the operator.
            .child(self.render_portfolio_team_share_pills(cx))
    }

    /// Render the “Share with a team” pill row for the commit sheet.
    /// Unlike the portfolio/cockpit versions, this one is *toggle*-based
    /// (nothing is server-side yet, the share only applies on Commit),
    /// so pills flip between selected (cyan check) and unselected.
    fn render_commit_team_share_pills(&self, cx: &Context<Self>) -> AnyElement {
        let selected_team_ids: std::collections::HashSet<String> = self
            .commit_share_team_targets
            .iter()
            .map(|(t, _)| t.clone())
            .collect();

        let mut container = div().flex().flex_col().gap(px(6.0)).mt(px(8.0)).child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme::fg_faint())
                        .child(format!("OR SHARE WITH A TEAM ({})", self.teams.len())),
                )
                .when(self.teams_loading, |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child("loading…"),
                    )
                }),
        );

        if self.teams.is_empty() && !self.teams_loading {
            container = container.child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .child(
                        "No collaboration teams yet. Create one in the Teams panel to share with a group.",
                    ),
            );
            return container.into_any_element();
        }

        let pills = self.teams.iter().map(|t| {
            let tid = t.id.clone();
            let selected = selected_team_ids.contains(&t.id);
            let (label, color) = if selected {
                (format!("✓ {}", t.name), rgb(theme::CYAN))
            } else {
                (t.name.clone(), rgb(theme::FG_DIM))
            };
            div()
                .id(SharedString::from(format!("commit-team-share-{}", tid)))
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(12.0))
                .bg(if selected {
                    theme::bg_active()
                } else {
                    theme::bg_elevated()
                })
                .text_size(px(11.0))
                .text_color(color)
                .cursor_pointer()
                .hover(|s| s.bg(theme::bg_hover()))
                .on_click(cx.listener({
                    let tid = tid.clone();
                    move |this, _, _w, cx| {
                        this.toggle_commit_team_share_target(tid.clone(), cx);
                    }
                }))
                .child(label)
        });

        container
            .child(div().flex().flex_wrap().gap(px(6.0)).children(pills))
            .into_any_element()
    }

    /// Render the “Share with a team” pill row for the portfolio Access
    /// panel. Mirrors `render_team_share_section` in the cockpit: skip
    /// already-shared teams, show in-flight state, respect the current
    /// permission chip.
    fn render_portfolio_team_share_pills(&self, cx: &Context<Self>) -> AnyElement {
        let already_shared_team_ids: std::collections::HashSet<String> = self
            .portfolio_shares
            .iter()
            .filter(|s| s.share_type == "team")
            .map(|s| s.share_target.clone())
            .collect();

        let mut container = div().flex().flex_col().gap(px(6.0)).mt(px(8.0)).child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(11.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::fg_dim())
                        .child(format!("Share with a team ({})", self.teams.len())),
                )
                .when(self.teams_loading, |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child("loading…"),
                    )
                }),
        );

        if self.teams.is_empty() && !self.teams_loading {
            container = container.child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .child(
                        "No collaboration teams yet. Create one in the Teams panel to share with a group.",
                    ),
            );
            return container.into_any_element();
        }

        let pills = self.teams.iter().map(|t| {
            let tid = t.id.clone();
            let already_shared = already_shared_team_ids.contains(&t.id);
            let in_flight = self.portfolio_team_share_in_flight.contains(&t.id);
            let interactive = !already_shared && !in_flight;
            let (label, color) = if already_shared {
                (format!("✓ {}", t.name), rgb(theme::GREEN))
            } else if in_flight {
                (format!("… {}", t.name), rgb(theme::FG_FAINT))
            } else {
                (t.name.clone(), rgb(theme::CYAN))
            };
            let pill = div()
                .id(SharedString::from(format!("pf-team-share-{}", tid)))
                .px(px(10.0))
                .py(px(4.0))
                .rounded(px(12.0))
                .bg(theme::bg_elevated())
                .text_size(px(11.0))
                .text_color(color)
                .child(label);
            if interactive {
                pill.cursor_pointer()
                    .hover(|s| s.bg(theme::bg_hover()))
                    .on_click(cx.listener({
                        let tid = tid.clone();
                        move |this, _, _w, cx| {
                            this.share_portfolio_with_team(tid.clone(), cx);
                        }
                    }))
            } else {
                pill
            }
        });

        container
            .child(div().flex().flex_wrap().gap(px(6.0)).children(pills))
            .into_any_element()
    }

    fn load_local_forecasts(&mut self) {
        self.local_forecasts.clear();
        if let Ok(entries) = std::fs::read_dir("forecasts") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "fpl").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let filename = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let question = content
                            .lines()
                            .find(|l| l.starts_with("question"))
                            .and_then(|l| l.split('"').nth(1))
                            .unwrap_or(&filename)
                            .to_string();
                        let timestamp = entry
                            .metadata()
                            .and_then(|m| m.modified())
                            .map(|t| {
                                let dt: chrono::DateTime<chrono::Utc> = t.into();
                                dt.format("%Y-%m-%d %H:%M").to_string()
                            })
                            .unwrap_or_else(|_| "unknown".into());
                        let driver_count =
                            content.lines().filter(|l| l.starts_with("driver ")).count();

                        // Load state.json for probability and version
                        let state_path = path.with_extension("state.json");
                        let (
                            probability,
                            base_rate,
                            version,
                            evidence_count,
                            agent_count,
                            confidence,
                            version_probs,
                            tags,
                            status,
                            resolved_outcome,
                            brier_score,
                        ) = if let Ok(state_text) = std::fs::read_to_string(&state_path) {
                            if let Ok(sj) = serde_json::from_str::<serde_json::Value>(&state_text) {
                                (
                                    sj.get("predicted_probability")
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.5),
                                    sj.get("base_rate")
                                        .and_then(|b| b.get("historical_frequency"))
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0),
                                    sj.get("current_version")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0) as u32,
                                    sj.get("evidence")
                                        .and_then(|v| v.as_array())
                                        .map(|a| a.len())
                                        .unwrap_or(0),
                                    sj.get("agents")
                                        .and_then(|v| v.as_array())
                                        .map(|a| a.len())
                                        .unwrap_or(0),
                                    sj.get("forecast_confidence")
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0),
                                    sj.get("versions")
                                        .and_then(|v| v.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|v| {
                                                    v.get("probability").and_then(|p| p.as_f64())
                                                })
                                                .collect()
                                        })
                                        .unwrap_or_default(),
                                    sj.get("tags")
                                        .and_then(|v| v.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                                .collect()
                                        })
                                        .unwrap_or_default(),
                                    sj.get("status")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("active")
                                        .to_string(),
                                    sj.get("resolved_outcome").and_then(|v| v.as_bool()),
                                    sj.get("brier_score").and_then(|v| v.as_f64()),
                                )
                            } else {
                                (
                                    0.5,
                                    0.0,
                                    0,
                                    0,
                                    0,
                                    0.0,
                                    vec![],
                                    vec![],
                                    "draft".into(),
                                    None,
                                    None,
                                )
                            }
                        } else {
                            (
                                0.5,
                                0.0,
                                0,
                                0,
                                0,
                                0.0,
                                vec![],
                                vec![],
                                "draft".into(),
                                None,
                                None,
                            )
                        };

                        // Auto-detect domain from question keywords
                        let q_lower = question.to_lowercase();
                        let domain = if q_lower.contains("nba")
                            || q_lower.contains("lakers")
                            || q_lower.contains("knicks")
                            || q_lower.contains("celtics")
                            || q_lower.contains("basketball")
                        {
                            "sports_nba"
                        } else if q_lower.contains("nfl")
                            || q_lower.contains("football") && !q_lower.contains("soccer")
                        {
                            "sports_nfl"
                        } else if q_lower.contains("world cup")
                            || q_lower.contains("euro")
                            || q_lower.contains("premier league")
                            || q_lower.contains("soccer")
                            || q_lower.contains("uefa")
                        {
                            "sports_football"
                        } else if q_lower.contains("stock")
                            || q_lower.contains("share price")
                            || q_lower.contains("revenue")
                            || q_lower.contains("ipo")
                            || q_lower.contains("nasdaq")
                            || q_lower.contains("earnings")
                        {
                            "finance"
                        } else if q_lower.contains("fda")
                            || q_lower.contains("trial")
                            || q_lower.contains("drug")
                            || q_lower.contains("biotech")
                            || q_lower.contains("pharma")
                        {
                            "biotech"
                        } else if q_lower.contains("ai")
                            || q_lower.contains("technology")
                            || q_lower.contains("software")
                            || q_lower.contains("chip")
                            || q_lower.contains("semiconductor")
                        {
                            "technology"
                        } else if q_lower.contains("election")
                            || q_lower.contains("president")
                            || q_lower.contains("congress")
                            || q_lower.contains("vote")
                        {
                            "politics"
                        } else if q_lower.contains("war")
                            || q_lower.contains("conflict")
                            || q_lower.contains("treaty")
                            || q_lower.contains("nato")
                        {
                            "geopolitics"
                        } else {
                            "general"
                        }
                        .to_string();

                        self.local_forecasts.push(LocalForecast {
                            filename,
                            question,
                            timestamp,
                            probability,
                            base_rate,
                            version,
                            driver_count,
                            evidence_count,
                            agent_count,
                            confidence,
                            version_probs,
                            tags,
                            status,
                            domain,
                            resolved_outcome,
                            brier_score,
                        });
                    }
                }
            }
        }
        self.local_forecasts
            .sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    }

    fn fetch_workspace_forecasts(&mut self, cx: &mut Context<Self>) {
        if !self.connected {
            return; // Don't fetch if not signed in
        }
        self.workspace_forecasts_loading = true;
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            // Spawn onto tokio runtime for proper I/O
            let result = tokio::spawn(async move { api.list_forecast_workspaces().await }).await;

            match result {
                Ok(Ok(resp)) => {
                    let workspaces = resp
                        .get("workspaces")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    log::info!(
                        "[workspaces] Fetched {} fermi_forecast workspaces",
                        workspaces.len()
                    );

                    // Parse workspace metadata from the list response — no per-workspace
                    // HTTP calls. Params are extracted from workspace name pattern.
                    let mut forecasts: Vec<WorkspaceForecast> = Vec::new();
                    for ws in &workspaces {
                        let ws_id = ws
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let name = ws
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let created = ws
                            .get("created_at")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let forecast_id = ws
                            .get("forecast_id")
                            .and_then(|v| v.as_str())
                            .map(String::from);

                        // Parse team info from workspace name pattern:
                        // "Team Prior — Argentina (ARG)" or "Tournament Path — Group B"
                        let (team_name, team_id, group, program_type, elo) = if name
                            .starts_with("Team Prior")
                        {
                            // Extract team name from "Team Prior — Name (ID)"
                            let after_dash = name.strip_prefix("Team Prior — ").unwrap_or(&name);
                            let tn = after_dash
                                .split(" (")
                                .next()
                                .unwrap_or(after_dash)
                                .to_string();
                            let tid = after_dash
                                .split('(')
                                .nth(1)
                                .and_then(|s| s.strip_suffix(')'))
                                .map(|s| s.to_string());
                            (Some(tn), tid, None, Some("TEAM_PRIOR".to_string()), None)
                        } else if name.starts_with("Tournament Path") {
                            let grp = name
                                .strip_prefix("Tournament Path — Group ")
                                .map(|s| s.to_string());
                            (None, None, grp, Some("TOURNAMENT_PATH".to_string()), None)
                        } else {
                            (None, None, None, None, None)
                        };

                        forecasts.push(WorkspaceForecast {
                            workspace_id: ws_id,
                            workspace_name: name,
                            forecast_id,
                            team_id,
                            team_name,
                            group,
                            program_type,
                            probability: None, // populated later when outputs exist
                            elo,
                            created_at: created,
                        });
                    }

                    this.update(cx, |this, cx| {
                        // Dedup by workspace name (batch script re-runs create duplicates)
                        let mut seen = std::collections::HashSet::new();
                        let deduped: Vec<WorkspaceForecast> = forecasts
                            .into_iter()
                            .filter(|wf| seen.insert(wf.workspace_name.clone()))
                            .collect();
                        this.workspace_forecasts = deduped;
                        this.workspace_forecasts_loading = false;
                        cx.notify();
                    })
                    .ok();
                }
                Ok(Err(e)) => {
                    log::error!("[workspaces] Failed to fetch: {}", e);
                    this.update(cx, |this, cx| {
                        this.workspace_forecasts_loading = false;
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::error!("[workspaces] Task error: {}", e);
                    this.update(cx, |this, cx| {
                        this.workspace_forecasts_loading = false;
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn fetch_stats(&mut self, cx: &mut Context<Self>) {
        self.stats_loading = true;
        let api = self.api.clone();

        cx.spawn(async move |this, cx| match api.my_stats().await {
            Ok(stats) => {
                this.update(cx, |this, cx| {
                    this.my_stats = Some(stats);
                    this.stats_loading = false;
                    cx.notify();
                })
                .ok();
            }
            Err(e) => {
                log::error!("Failed to fetch stats: {}", e);
                this.update(cx, |this, cx| {
                    this.stats_loading = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Rebuild the Dashboard's Recent Activity feed by merging recent
    /// resolutions with recent live-forecast probability updates.
    ///
    /// Design goals (each solving a specific complaint from testers):
    ///   * Drafts and probability revisions surface — the feed used
    ///     to be resolved-only, which meant WIP work vanished until
    ///     publish + resolution.
    ///   * Repeat-shape resolutions collapse. A user with a
    ///     bulk-imported WC portfolio was seeing 8 rows of
    ///     "Resolved No: Will X win the 2026 FIFA World Cup" — pure
    ///     noise. Runs of same-family resolutions collapse to one
    ///     summary row so the other 7 slots go to useful content.
    ///   * Trivial-Brier resolutions (< 0.05) yield the floor when
    ///     there's other content to show. A perfectly-called long
    ///     shot at ~99% predicted is not a learning event; keep it
    ///     only if the feed would otherwise be empty.
    fn recompute_recent_activity(&mut self) {
        #[derive(Clone)]
        struct Candidate {
            sort_key: String,
            item: ActivityItem,
            /// True for trivial-Brier resolutions that we're willing
            /// to drop when the feed has better content.
            is_low_signal: bool,
            /// Bucket key used for run-length collapse of same-family
            /// resolutions. Empty for non-resolutions.
            family_key: String,
        }

        let mut candidates: Vec<Candidate> = Vec::new();

        // Drafts — forecasts the operator is composing but hasn't
        // published yet.
        for f in &self.draft_forecasts {
            let ts = f
                .updated_at
                .clone()
                .or_else(|| f.created_at.clone())
                .unwrap_or_default();
            if ts.is_empty() {
                continue;
            }
            candidates.push(Candidate {
                sort_key: ts.clone(),
                item: ActivityItem {
                    icon: "✎",
                    text: format!(
                        "Draft: {} — {:.0}%",
                        truncate(&f.question_text, 40),
                        f.predicted_probability * 100.0,
                    ),
                    time: format_relative_time(&ts),
                    color: theme::GOLD,
                    forecast_id: f.id.clone(),
                    source: ActivitySource::Mine,
                },
                is_low_signal: false,
                family_key: String::new(),
            });
        }

        // Resolved forecasts → ✓ icon, Brier-colored.
        for f in &self.resolved_forecasts {
            let ts = f
                .resolved_at
                .clone()
                .or_else(|| f.updated_at.clone())
                .unwrap_or_default();
            let brier = f.brier_score.unwrap_or(0.5);
            let color = if brier < 0.15 {
                theme::GREEN
            } else if brier < 0.3 {
                theme::GOLD
            } else {
                theme::ORANGE
            };
            let outcome = if f.actual_outcome == Some(true) {
                "Yes"
            } else {
                "No"
            };
            candidates.push(Candidate {
                sort_key: ts.clone(),
                item: ActivityItem {
                    icon: "✓",
                    text: format!(
                        "Resolved {}: {} (Brier {:.2})",
                        outcome,
                        truncate(&f.question_text, 36),
                        brier,
                    ),
                    time: format_relative_time(&ts),
                    color,
                    forecast_id: f.id.clone(),
                    source: ActivitySource::Mine,
                },
                // Perfect-Brier calls are noise for a learning-oriented
                // feed; drop when better content is available.
                is_low_signal: brier < 0.05,
                family_key: activity_family_key(&f.question_text, f.actual_outcome),
            });
        }

        // Active forecasts → ◐ icon, cyan. Uses updated_at as the sort key
        // so a probability edit today lifts the row above a week-old
        // resolution. Fall back to created_at if updated_at is missing.
        for f in &self.active_forecasts {
            let ts = f
                .updated_at
                .clone()
                .or_else(|| f.created_at.clone())
                .unwrap_or_default();
            if ts.is_empty() {
                continue;
            }
            // Distinguish "just published" (created within the last
            // hour) from "probability revised" so the operator can
            // tell at a glance what changed. `updated_at != created_at`
            // is the signal; when they match, no revision has landed
            // yet and the row reads as a fresh publish.
            let was_revised = f.created_at.as_ref().map(|c| c != &ts).unwrap_or(false);
            let (icon, verb) = if was_revised {
                ("→", "Revised")
            } else {
                ("◐", "Published")
            };
            candidates.push(Candidate {
                sort_key: ts.clone(),
                item: ActivityItem {
                    icon,
                    text: format!(
                        "{}: {} — {:.0}%",
                        verb,
                        truncate(&f.question_text, 40),
                        f.predicted_probability * 100.0,
                    ),
                    time: format_relative_time(&ts),
                    color: theme::CYAN,
                    forecast_id: f.id.clone(),
                    source: ActivitySource::Mine,
                },
                is_low_signal: false,
                family_key: String::new(),
            });
        }

        // Team activity — events on forecasts shared with (or owned
        // by) a team the operator is in, authored by someone else.
        // The visibility layer already gave us the row via
        // `shared_with_me_forecasts`; we filter to those with a
        // team_id matching one of our teams and tag them Team-source
        // so the Dashboard's Team filter chip has content.
        //
        // The status derivation matches the own-forecast branches
        // (Resolved / Published / Revised / Draft), just with a
        // "by teammate" framing in the text. Draft state is unusual
        // for shared-with-me (drafts are typically private), but we
        // handle it defensively.
        let my_team_ids: std::collections::HashSet<String> =
            self.teams.iter().map(|t| t.id.clone()).collect();
        for f in &self.shared_with_me_forecasts {
            let Some(ref tid) = f.team_id else { continue };
            if !my_team_ids.contains(tid) {
                continue;
            }
            let ts = f
                .resolved_at
                .clone()
                .or_else(|| f.updated_at.clone())
                .or_else(|| f.created_at.clone())
                .unwrap_or_default();
            if ts.is_empty() {
                continue;
            }
            let (icon, color, text, is_low_signal, family_key): (
                &'static str,
                u32,
                String,
                bool,
                String,
            ) = if f.status == "resolved" {
                let brier = f.brier_score.unwrap_or(0.5);
                let outcome = if f.actual_outcome == Some(true) {
                    "Yes"
                } else {
                    "No"
                };
                let color = if brier < 0.15 {
                    theme::GREEN
                } else if brier < 0.3 {
                    theme::GOLD
                } else {
                    theme::ORANGE
                };
                (
                    "✓",
                    color,
                    format!(
                        "Team resolved {}: {} (Brier {:.2})",
                        outcome,
                        truncate(&f.question_text, 32),
                        brier,
                    ),
                    brier < 0.05,
                    activity_family_key(&f.question_text, f.actual_outcome),
                )
            } else if f.status == "draft" {
                (
                    "✎",
                    theme::GOLD,
                    format!(
                        "Team draft: {} — {:.0}%",
                        truncate(&f.question_text, 36),
                        f.predicted_probability * 100.0,
                    ),
                    false,
                    String::new(),
                )
            } else {
                let was_revised = f.created_at.as_ref().map(|c| c != &ts).unwrap_or(false);
                let (icon, verb) = if was_revised {
                    ("→", "revised")
                } else {
                    ("◐", "published")
                };
                (
                    icon,
                    theme::BLUE,
                    format!(
                        "Team {}: {} — {:.0}%",
                        verb,
                        truncate(&f.question_text, 36),
                        f.predicted_probability * 100.0,
                    ),
                    false,
                    String::new(),
                )
            };
            candidates.push(Candidate {
                sort_key: ts.clone(),
                item: ActivityItem {
                    icon,
                    text,
                    time: format_relative_time(&ts),
                    color,
                    forecast_id: f.id.clone(),
                    source: ActivitySource::Team,
                },
                is_low_signal,
                family_key,
            });
        }

        // Newest first — lexicographic works because timestamps are
        // ISO-8601 ("2026-07-15T…").
        candidates.sort_by(|a, b| b.sort_key.cmp(&a.sort_key));

        // Two-pass curation.
        //
        // Pass 1: run-length collapse. Walk the sorted list and,
        // whenever N ≥ 2 consecutive rows share the same family key,
        // replace them with a single summary row ("Resolved 8x: 2026
        // FIFA World Cup group winners — all No"). Non-resolution
        // rows have an empty family_key so they never collapse.
        let mut collapsed: Vec<Candidate> = Vec::new();
        let mut i = 0;
        while i < candidates.len() {
            let head = &candidates[i];
            if head.family_key.is_empty() {
                collapsed.push(head.clone());
                i += 1;
                continue;
            }
            let mut j = i + 1;
            while j < candidates.len() && candidates[j].family_key == head.family_key {
                j += 1;
            }
            let run_len = j - i;
            if run_len < 2 {
                collapsed.push(head.clone());
                i += 1;
                continue;
            }
            // Build one summary row for the whole run. Keep the
            // head's timestamp so ordering is preserved.
            let family_label = describe_activity_family(&head.family_key);
            let mut summary = head.clone();
            summary.item.text = format!("Resolved {}×: {}", run_len, family_label,);
            summary.is_low_signal = false; // aggregated row is signal.
            collapsed.push(summary);
            i = j;
        }

        // Pass 2: prefer signal over floor. Keep the top-15 while
        // preferring non-low-signal rows (was 8 pre-multi-source; the
        // wider cap gives the Mine / Team filter chips enough
        // material to meaningfully filter without going stale). If
        // the high-signal pool has fewer than 15 items, backfill with
        // low-signal ones to avoid a near-empty feed.
        const FEED_MAX: usize = 15;
        let (signal, floor): (Vec<Candidate>, Vec<Candidate>) =
            collapsed.into_iter().partition(|c| !c.is_low_signal);
        let mut final_items: Vec<ActivityItem> =
            signal.into_iter().take(FEED_MAX).map(|c| c.item).collect();
        if final_items.len() < FEED_MAX {
            let need = FEED_MAX - final_items.len();
            final_items.extend(floor.into_iter().take(need).map(|c| c.item));
        }

        self.recent_activity = final_items;
    }

    fn fetch_forecasts(&mut self, cx: &mut Context<Self>) {
        self.forecasts_loading = true;
        // Whenever we refresh the main forecast lists, also refresh
        // whichever virtual bucket is currently open. Without this,
        // publishing/saving a new forecast leaves the Portfolio panel's
        // "📌 Unassigned" or "📥 Shared with me" list stale until
        // the operator manually re-clicks the bucket. Both fetches are
        // idempotent and gated on `connected` internally.
        match self.selected_virtual_portfolio {
            Some(VirtualPortfolio::SharedWithMe) => self.fetch_shared_with_me(cx),
            Some(VirtualPortfolio::Unassigned) => self.fetch_unassigned_forecasts(cx),
            // Live / Drafts / RecentlyResolved are pure client-side
            // filters over data `fetch_forecasts` itself just wrote to
            // `active_forecasts` / `draft_forecasts` / `resolved_forecasts`.
            // No extra round-trip needed.
            Some(VirtualPortfolio::Live)
            | Some(VirtualPortfolio::Drafts)
            | Some(VirtualPortfolio::RecentlyResolved) => {}
            None => {}
        }
        let api = self.api.clone();

        cx.spawn(async move |this, cx| {
            // Bind queries to variables so they outlive the tokio::join! macro
            let active_q = ForecastQuery::active();
            let resolved_q = ForecastQuery {
                status: Some("resolved".into()),
                sort: Some("brier_score".into()),
                order: Some("asc".into()),
                limit: Some(20),
                ..Default::default()
            };
            let draft_q = ForecastQuery::drafts();

            // Fetch active, resolved, and draft forecasts in parallel
            let (active_res, resolved_res, draft_res) = tokio::join!(
                api.list_forecasts(&active_q),
                api.list_forecasts(&resolved_q),
                api.list_forecasts(&draft_q),
            );

            this.update(cx, |this, cx| {
                if let Ok(resp) = active_res {
                    this.active_forecasts = resp.forecasts;
                }
                if let Ok(resp) = resolved_res {
                    // `resolved_forecasts` keeps the server-side brier-ASC order
                    // that the Portfolio's Resolved section renders directly.
                    this.resolved_forecasts = resp.forecasts;
                }
                if let Ok(resp) = draft_res {
                    this.draft_forecasts = resp.forecasts;
                }
                // Recompute Recent Activity from whatever we just refreshed.
                // Merges recently-updated live forecasts with recently-resolved
                // ones so the operator sees today's probability edits alongside
                // yesterday's resolutions — not just a wall of week-old bulk
                // WC group-stage eliminations.
                this.recompute_recent_activity();
                this.forecasts_loading = false;
                // Background-populate the team-share cache for any
                // shared forecast we haven't looked up yet. Fire-and-
                // forget; the team-dot rendering treats missing entries
                // as "no team association" and re-renders when the
                // cache lands.
                this.refresh_forecast_shares_cache(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Fetch the "📥 Shared with me" virtual portfolio: forecasts
    /// the caller can view but doesn't own. Backend applies
    /// `scope=shared` to `list_forecasts_handler` — same ACL, just
    /// filtered to the non-owned slice. Idempotent; safe to call
    /// repeatedly.
    fn fetch_shared_with_me(&mut self, cx: &mut Context<Self>) {
        if !self.connected {
            return;
        }
        self.shared_with_me_loading = true;
        cx.notify();
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let query = ForecastQuery::shared_with_me();
            let result = api.list_forecasts(&query).await;
            this.update(cx, |this, cx| {
                this.shared_with_me_loading = false;
                match result {
                    Ok(resp) => {
                        this.shared_with_me_forecasts = resp.forecasts;
                    }
                    Err(e) => {
                        log::warn!("[portfolio-virtual] shared_with_me fetch failed: {}", e);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Fetch the "📌 Unassigned" virtual portfolio: forecasts the
    /// caller owns that aren't a member of any named portfolio. Backend
    /// applies `scope=mine&unassigned=true` on top of the standard
    /// list projection.
    fn fetch_unassigned_forecasts(&mut self, cx: &mut Context<Self>) {
        if !self.connected {
            return;
        }
        self.unassigned_loading = true;
        cx.notify();
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let query = ForecastQuery::mine_unassigned();
            let result = api.list_forecasts(&query).await;
            this.update(cx, |this, cx| {
                this.unassigned_loading = false;
                match result {
                    Ok(resp) => {
                        this.unassigned_forecasts = resp.forecasts;
                    }
                    Err(e) => {
                        log::warn!("[portfolio-virtual] unassigned fetch failed: {}", e);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Switch the Portfolio panel to a virtual bucket. Clears any
    /// named-portfolio selection so the right-hand pane doesn't try to
    /// render two things at once, and warms the appropriate cache.
    fn select_virtual_portfolio(&mut self, bucket: VirtualPortfolio, cx: &mut Context<Self>) {
        // Toggle: clicking the currently-selected virtual bucket
        // deselects it (matches the named-portfolio card behaviour).
        if self.selected_virtual_portfolio == Some(bucket) {
            self.selected_virtual_portfolio = None;
            cx.notify();
            return;
        }
        self.selected_virtual_portfolio = Some(bucket);
        self.selected_portfolio_id = None;
        self.portfolio_confirm_delete_id = None;
        self.portfolio_rename_id = None;
        match bucket {
            VirtualPortfolio::SharedWithMe => self.fetch_shared_with_me(cx),
            VirtualPortfolio::Unassigned => self.fetch_unassigned_forecasts(cx),
            // Client-side buckets — the underlying vectors are already
            // populated by `fetch_forecasts`.
            VirtualPortfolio::Live
            | VirtualPortfolio::Drafts
            | VirtualPortfolio::RecentlyResolved => {}
        }
        cx.notify();
    }

    fn fetch_portfolios(&mut self, cx: &mut Context<Self>) {
        self.portfolios_loading = true;
        let api = self.api.clone();

        cx.spawn(async move |this, cx| match api.list_portfolios().await {
            Ok(resp) => {
                this.update(cx, |this, cx| {
                    this.portfolios = resp.portfolios;
                    this.portfolios_loading = false;
                    // Background-populate the team-share cache for any
                    // portfolio we haven't looked up yet. Powers the
                    // Teams panel's Shared tab; missing entries mean
                    // "no team shares" until the fan-out lands.
                    this.refresh_portfolio_shares_cache(cx);
                    cx.notify();
                })
                .ok();
            }
            Err(e) => {
                log::error!("Failed to fetch portfolios: {}", e);
                this.update(cx, |this, cx| {
                    this.portfolios_loading = false;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Poll the server for pending cascades. Refreshed on app startup,
    /// after every resolve, and periodically by the background refresh
    /// loop (so a cascade queued by an upstream workspace resolution
    /// appears without the operator having to do anything).
    ///
    /// When the queue grows between fetches we surface a toast so the
    /// operator knows there's new work — that's the whole reason we
    /// poll in the background. Growth detection is gated on
    /// `background_refresh_started` so the initial load doesn't toast
    /// "N new cascades" just because the queue was non-empty already.
    fn fetch_pending_cascades(&mut self, cx: &mut Context<Self>) {
        self.pending_cascades_loading = true;
        let prev_count = self.pending_cascades.len();
        let notify_on_growth = self.background_refresh_started;
        let api = self.api.clone();
        cx.spawn(
            async move |this, cx| match api.list_pending_cascades().await {
                Ok(resp) => {
                    let pending = resp
                        .get("pending")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    this.update(cx, |this, cx| {
                        let new_count = pending.len();
                        this.pending_cascades = pending;
                        this.pending_cascades_loading = false;
                        if notify_on_growth && new_count > prev_count {
                            let delta = new_count - prev_count;
                            let msg = if delta == 1 {
                                "1 new cascade pending review".to_string()
                            } else {
                                format!("{} new cascades pending review", delta)
                            };
                            this.show_toast(msg, "⚡", theme::GOLD, cx);
                        }
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::warn!("[pending-cascades] fetch failed: {}", e);
                    this.update(cx, |this, cx| {
                        this.pending_cascades_loading = false;
                        cx.notify();
                    })
                    .ok();
                }
            },
        )
        .detach();
    }

    // ── Cascades / Relationships (Sprint B) ───────────────────────────────

    /// Load every relationship the caller owns. Fired once on connect
    /// and after any create/delete mutation so the Portfolio panel's
    /// Relationships sub-panel stays fresh.
    fn fetch_all_relationships(&mut self, cx: &mut Context<Self>) {
        if self.all_relationships_loading {
            return;
        }
        self.all_relationships_loading = true;
        cx.notify();
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = api.list_all_relationships().await;
            this.update(cx, |this, cx| {
                this.all_relationships_loading = false;
                match result {
                    Ok(resp) => {
                        this.all_relationships = resp
                            .get("relationships")
                            .and_then(|v| v.as_array())
                            .cloned()
                            .unwrap_or_default();
                    }
                    Err(e) => {
                        log::warn!("[relationships] fetch failed: {}", e);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Submit the pending relationship declaration. Reads the sheet's
    /// state (kind, forecast_ids, n, description), POSTs to the
    /// server, and refreshes the local list on success.
    fn submit_relationship_create(&mut self, cx: &mut Context<Self>) {
        // Validate: at least 2 forecasts.
        if self.relationship_create_forecast_ids.len() < 2 {
            self.relationship_create_error = Some("Pick at least 2 forecasts to link.".into());
            cx.notify();
            return;
        }
        let kind = self.relationship_create_kind.clone();
        let forecast_ids: Vec<String> = self
            .relationship_create_forecast_ids
            .iter()
            .cloned()
            .collect();
        // Parameters shape depends on kind. `at_most_n` needs an `n`.
        let parameters = match kind.as_str() {
            "at_most_n" => {
                let n: i64 = self.relationship_create_n.trim().parse().unwrap_or(1);
                serde_json::json!({ "n": n })
            }
            _ => serde_json::json!({}),
        };
        let description_str = self
            .relationship_create_description
            .read(cx)
            .text()
            .trim()
            .to_string();
        let description = if description_str.is_empty() {
            None
        } else {
            Some(description_str)
        };

        self.relationship_create_loading = true;
        self.relationship_create_error = None;
        cx.notify();

        let api = self.api.clone();
        let desc_input = self.relationship_create_description.clone();
        cx.spawn(async move |this, cx| {
            let result = api
                .create_relationship(&kind, &forecast_ids, parameters, description.as_deref())
                .await;
            this.update(cx, |this, cx| {
                this.relationship_create_loading = false;
                match result {
                    Ok(_) => {
                        this.relationship_create_showing = false;
                        this.relationship_create_forecast_ids.clear();
                        desc_input.update(cx, |inp, cx| inp.set_text("", cx));
                        this.show_toast("Relationship declared", "⛓", theme::CYAN, cx);
                        this.fetch_all_relationships(cx);
                    }
                    Err(e) => {
                        this.relationship_create_error = Some(e.to_string());
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Delete a relationship by id. Two-step confirmation not needed —
    /// relationships are lightweight declarative rows; deleting one
    /// just stops future cascade propagations, no data is lost.
    fn delete_relationship(&mut self, rid: String, cx: &mut Context<Self>) {
        if !self.relationship_delete_in_flight.insert(rid.clone()) {
            return;
        }
        cx.notify();
        let api = self.api.clone();
        cx.spawn(async move |this, cx| {
            let result = api.delete_relationship(&rid).await;
            this.update(cx, |this, cx| {
                this.relationship_delete_in_flight.remove(&rid);
                match result {
                    Ok(_) => {
                        this.show_toast("Relationship removed", "✓", theme::GREEN, cx);
                        this.fetch_all_relationships(cx);
                    }
                    Err(e) => {
                        log::warn!("[relationships] delete failed: {}", e);
                        this.show_toast(&format!("Delete failed: {}", e), "⚠", theme::RED, cx);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn apply_pending_cascade(&mut self, cascade_id: String, cx: &mut Context<Self>) {
        if !self.cascade_action_in_flight.insert(cascade_id.clone()) {
            return; // already in flight
        }
        let api = self.api.clone();
        let cid = cascade_id.clone();
        cx.spawn(async move |this, cx| {
            let result = api.apply_pending_cascade(&cid, None).await;
            this.update(cx, |this, cx| {
                this.cascade_action_in_flight.remove(&cid);
                match result {
                    Ok(resp) => {
                        let n = resp.get("n_updated").and_then(|v| v.as_u64()).unwrap_or(0);
                        this.toast = Some((
                            format!("Cascade applied — {} forecasts updated.", n),
                            "✓",
                            theme::GREEN,
                        ));
                        // Refresh the queue + the dashboard data so
                        // the new sibling probabilities show up.
                        this.fetch_pending_cascades(cx);
                        this.fetch_forecasts(cx);
                        this.portfolio_forecasts.clear();
                        this.portfolio_stats_cache.clear();
                        if let Some(pid) = this.selected_portfolio_id.clone() {
                            this.fetch_portfolio_forecasts(pid, cx);
                        }
                    }
                    Err(e) => {
                        this.toast = Some((format!("Apply failed: {}", e), "✗", theme::RED));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn dismiss_pending_cascade(&mut self, cascade_id: String, cx: &mut Context<Self>) {
        if !self.cascade_action_in_flight.insert(cascade_id.clone()) {
            return;
        }
        let api = self.api.clone();
        let cid = cascade_id.clone();
        cx.spawn(async move |this, cx| {
            let result = api.dismiss_pending_cascade(&cid, None).await;
            this.update(cx, |this, cx| {
                this.cascade_action_in_flight.remove(&cid);
                match result {
                    Ok(_) => {
                        this.toast = Some(("Cascade dismissed.".into(), "○", theme::FG_DIM));
                        this.fetch_pending_cascades(cx);
                    }
                    Err(e) => {
                        this.toast = Some((format!("Dismiss failed: {}", e), "✗", theme::RED));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn navigate(&mut self, panel: Panel, cx: &mut Context<Self>) {
        let changed = self.active_panel != panel;
        self.active_panel = panel;
        // Refresh data when switching to a panel
        if changed && self.connected {
            match panel {
                Panel::Dashboard => self.fetch_stats(cx),
                Panel::Portfolio => {
                    self.fetch_forecasts(cx);
                    self.load_local_forecasts();
                    self.fetch_workspace_forecasts(cx);
                    self.check_pm_resolutions(cx);
                }
                _ => {}
            }
        }
        // Create cockpit Entity on first visit to Composer
        if panel == Panel::Composer && self.cockpit.is_none() {
            let api = self.api.clone();
            let cockpit_entity = cx.new(|cx| CockpitState::new(api, self.registry.clone(), cx));
            // Kick off the portfolio list fetch so the composer's
            // inline chip strip has options as soon as the operator
            // publishes their first forecast in this session.
            cockpit_entity.update(cx, |cockpit, cx| {
                cockpit.load_portfolios_list(cx);
            });
            // Observe cockpit changes to drain queued cross-tab events
            // (toast notifications, invite-share modal requests). These
            // fields must live on FermiConsole (the parent), so the
            // cockpit stashes intent here and the observe callback picks
            // it up on the next tick.
            cx.observe(&cockpit_entity, |this, cockpit_ref, cx| {
                let (toasts, invite_share, refresh_forecasts): (
                    Vec<String>,
                    Option<(JsonValue, String, String)>,
                    bool,
                ) = cockpit_ref.update(cx, |state, _| {
                    let refresh = state.pending_forecasts_refresh;
                    state.pending_forecasts_refresh = false;
                    (
                        std::mem::take(&mut state.pending_toasts),
                        state.pending_invite_share.take(),
                        refresh,
                    )
                });
                for msg in toasts {
                    this.show_toast(msg, "✓", theme::GREEN, cx);
                }
                if let Some((invite_json, target_label, recipient)) = invite_share {
                    this.open_invite_share_modal(&invite_json, target_label, recipient, cx);
                }
                if refresh_forecasts {
                    // Forecast list on the parent has drifted (publish
                    // just added a new active row, or a resolution just
                    // came in). Refetch so the Dashboard's Live section
                    // and the Recent Activity feed reflect it without
                    // waiting for the 30s background loop.
                    this.fetch_forecasts(cx);
                    this.fetch_stats(cx);
                }
            })
            .detach();
            self.cockpit = Some(cockpit_entity);
        }
        // Refresh data when switching to agent fleet or leaderboard
        if changed && self.connected {
            match panel {
                Panel::AgentFleet => self.fetch_agents(cx),
                Panel::Leaderboard => self.fetch_leaderboard(cx),
                Panel::Teams => self.fetch_teams(cx),
                _ => {}
            }
        }
        // Re-entering the cockpit: reconcile the open forecast against the
        // server so a session that went stale (resolved/eliminated while
        // open) snaps to the settled state and locks itself.
        if changed && self.connected && panel == Panel::Composer {
            if let Some(ref cockpit) = self.cockpit {
                let cockpit = cockpit.clone();
                cockpit.update(cx, |cockpit, cx| {
                    if cockpit.forecast_id.is_some() {
                        cockpit.reconcile_forecast(cx);
                    }
                });
            }
        }
        cx.notify();
    }

    /// Handle question submission — read the question from the cockpit's
    /// text input and fire the agent orchestration.
    fn on_trigger_question_orchestration(
        &mut self,
        _: &TriggerQuestionOrchestration,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ref cockpit) = self.cockpit {
            let cockpit = cockpit.clone();
            cockpit.update(cx, |cockpit, cx| {
                // ── 8A: Debounce — prevent double-fire while already researching ──
                if cockpit.orchestration_running {
                    cockpit.messages.push(crate::cockpit::AssistantMessage {
                        node: "question".into(),
                        kind: crate::cockpit::MessageKind::Warning,
                        text: "⏳ Already researching — please wait for the current decomposition to finish.".into(),
                    });
                    cx.notify();
                    return;
                }
                let question = cockpit.question_input.read(cx).text().to_string();
                if !question.trim().is_empty() {
                    cockpit.orchestrate_question(&question, cx);
                }
            });
            cx.notify();
        }
    }

    /// ⌘R — Run local Monte Carlo simulation from cockpit drivers.
    fn on_run_simulation(
        &mut self,
        _: &RunSimulation,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ref cockpit) = self.cockpit {
            let cockpit = cockpit.clone();
            cockpit.update(cx, |cockpit, cx| {
                // ── 8A: Debounce — prevent double-fire while sim is running ──
                if cockpit.sim_running {
                    cockpit.messages.push(crate::cockpit::AssistantMessage {
                        node: "simulation".into(),
                        kind: crate::cockpit::MessageKind::Warning,
                        text: "⏳ Simulation already running.".into(),
                    });
                    cx.notify();
                    return;
                }
                cockpit.run_simulation(cx);
            });
            cx.notify();
        }
    }

    /// Ctrl+S — Save FPL to disk with version snapshot.
    fn on_import_forecast(
        &mut self,
        _: &ImportForecast,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            let file = rfd::AsyncFileDialog::new()
                .add_filter("FPL Forecasts", &["fpl"])
                .set_title("Import FPL Forecast")
                .pick_file()
                .await;
            if let Some(file) = file {
                let path = file.path().to_string_lossy().to_string();
                this.update(cx, |this, cx| {
                    if this.cockpit.is_none() {
                        let api = this.api.clone();
                        this.cockpit =
                            Some(cx.new(|cx| CockpitState::new(api, this.registry.clone(), cx)));
                    }
                    if let Some(ref cockpit) = this.cockpit {
                        let cockpit = cockpit.clone();
                        cockpit.update(cx, |cockpit, cx| {
                            cockpit.load_forecast(&path, cx);
                        });
                    }
                    this.active_panel = Panel::Composer;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    fn on_save_forecast(&mut self, _: &SaveForecast, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ref cockpit) = self.cockpit {
            let cockpit = cockpit.clone();
            cockpit.update(cx, |cockpit, cx| {
                cockpit.save_forecast(cx);
            });
            cx.notify();
        }
    }

    /// Ctrl+P — Show the commit sheet before publishing.
    fn on_publish_forecast(
        &mut self,
        _: &PublishForecast,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ref cockpit) = self.cockpit {
            let state = cockpit.read(cx);
            let question = state
                .program
                .question()
                .map(|q| q.text.clone())
                .unwrap_or_default();
            if question.is_empty() {
                return;
            }
            self.commit_sheet_question = question;
            self.commit_sheet_probability = cockpit.read(cx).predicted_probability;
            // The picker is binary now (private | public); coerce any legacy
            // "team" value so a stale tile selection can't slip through.
            if self.commit_sheet_visibility != "public" {
                self.commit_sheet_visibility = "private".into();
            }
            self.commit_share_targets.clear();
            self.commit_share_team_targets.clear();
            self.commit_share_input
                .update(cx, |inp, cx| inp.set_text("", cx));
            self.commit_share_permission = "view".into();
            // Warm the team-pill picker in the commit sheet. Same
            // idempotent-ish guard as the portfolio Access panel.
            if self.teams.is_empty() && !self.teams_loading {
                self.fetch_teams(cx);
            }
            self.commit_sheet_showing = true;
            cx.notify();
        }
    }

    /// Append the current share-input value to the pending share list.
    fn add_commit_share_target(&mut self, cx: &mut Context<Self>) {
        let raw = self.commit_share_input.read(cx).text().trim().to_string();
        if raw.is_empty() {
            return;
        }
        let perm = self.commit_share_permission.clone();
        // De-dupe by target; last permission wins.
        self.commit_share_targets.retain(|(t, _)| t != &raw);
        self.commit_share_targets.push((raw, perm));
        self.commit_share_input
            .update(cx, |inp, cx| inp.set_text("", cx));
        cx.notify();
    }

    /// Toggle a team in the commit-sheet team-share pending list.
    /// Selection is on/off (rather than three-state per-row like the
    /// user shares), and the current permission chip drives the grant.
    fn toggle_commit_team_share_target(&mut self, team_id: String, cx: &mut Context<Self>) {
        if let Some(pos) = self
            .commit_share_team_targets
            .iter()
            .position(|(t, _)| t == &team_id)
        {
            self.commit_share_team_targets.remove(pos);
        } else {
            let perm = self.commit_share_permission.clone();
            self.commit_share_team_targets.push((team_id, perm));
        }
        cx.notify();
    }

    /// Called when the user confirms in the commit sheet.
    fn do_commit_forecast(&mut self, cx: &mut Context<Self>) {
        self.commit_sheet_showing = false;
        let visibility = self.commit_sheet_visibility.clone();
        let shares = std::mem::take(&mut self.commit_share_targets);
        let team_shares = std::mem::take(&mut self.commit_share_team_targets);

        if let Some(ref cockpit) = self.cockpit {
            let cockpit = cockpit.clone();
            cockpit.update(cx, |cockpit, cx| {
                // Hand the share list to the cockpit so it can apply the
                // grants once the forecast row (and its id) exists.
                cockpit.pending_publish_shares = shares;
                cockpit.pending_publish_team_shares = team_shares;
                cockpit.publish_forecast(visibility, cx);
            });

            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;
                this.update(cx, |this, cx| {
                    this.fetch_forecasts(cx);
                    this.fetch_stats(cx);
                    this.fetch_portfolios(cx);
                })
                .ok();
            })
            .detach();

            cx.notify();
        }
    }

    /// ⌘E — Toggle FPL source view in the Driver Map zone.
    fn on_toggle_fpl_source(
        &mut self,
        _: &ToggleFplSource,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ref cockpit) = self.cockpit {
            let cockpit = cockpit.clone();
            cockpit.update(cx, |cockpit, cx| {
                // Cycle order matches the tab bar's left→right reading
                // flow: Trajectory → Wiki → Schedules → Access → Fpl →
                // Edit → (back to Trajectory).
                cockpit.right_tab = match cockpit.right_tab {
                    crate::cockpit::RightTab::Trajectory => crate::cockpit::RightTab::Provenance,
                    crate::cockpit::RightTab::Provenance => crate::cockpit::RightTab::Wiki,
                    crate::cockpit::RightTab::Wiki => crate::cockpit::RightTab::Schedules,
                    crate::cockpit::RightTab::Schedules => crate::cockpit::RightTab::Access,
                    crate::cockpit::RightTab::Access => crate::cockpit::RightTab::Fpl,
                    crate::cockpit::RightTab::Fpl => crate::cockpit::RightTab::Edit,
                    crate::cockpit::RightTab::Edit => crate::cockpit::RightTab::Trajectory,
                };
                if cockpit.right_tab == crate::cockpit::RightTab::Trajectory {
                    cockpit.load_timeline(cx);
                }
                if cockpit.right_tab == crate::cockpit::RightTab::Provenance {
                    cockpit.load_provenance(cx);
                }
                if cockpit.right_tab == crate::cockpit::RightTab::Access
                    && cockpit.shares_loaded_for != cockpit.forecast_id
                {
                    cockpit.load_shares(cx);
                }
            });
            cx.notify();
        }
    }

    // ── Window management ─────────────────────────────────────────────

    fn on_minimize_window(
        &mut self,
        _: &MinimizeWindow,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        window.minimize_window();
    }

    fn on_zoom_window(&mut self, _: &ZoomWindow, window: &mut Window, _cx: &mut Context<Self>) {
        window.zoom_window();
    }

    fn on_toggle_fullscreen(
        &mut self,
        _: &ToggleFullscreen,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        window.toggle_fullscreen();
    }

    /// Reset the cockpit to a fresh state (new forecast).
    fn on_new_forecast(&mut self, _: &NewForecast, _window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        self.cockpit = Some(cx.new(|cx| CockpitState::new(api, self.registry.clone(), cx)));
        self.active_panel = Panel::Composer;
        cx.notify();
    }

    fn on_reset_cockpit(&mut self, _: &ResetCockpit, _window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        self.cockpit = Some(cx.new(|cx| CockpitState::new(api, self.registry.clone(), cx)));
        self.active_panel = Panel::Composer;
        cx.notify();
    }

    fn on_show_dashboard(&mut self, _: &ShowDashboard, _w: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Panel::Dashboard, cx);
    }
    fn on_show_portfolio(&mut self, _: &ShowPortfolio, _w: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Panel::Portfolio, cx);
    }
    fn on_show_agent_fleet(&mut self, _: &ShowAgentFleet, _w: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Panel::AgentFleet, cx);
    }
    fn on_show_composer(&mut self, _: &ShowComposer, _w: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Panel::Composer, cx);
    }
    fn on_show_leaderboard(
        &mut self,
        _: &ShowLeaderboard,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate(Panel::Leaderboard, cx);
    }
    fn on_show_teams(&mut self, _: &ShowTeams, _w: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Panel::Teams, cx);
    }

    // ── Sidebar ───────────────────────────────────────────────────────────

    fn render_sidebar(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .w(px(220.0))
            .h_full()
            .bg(theme::bg_deep())
            .border_r_1()
            .border_color(theme::fg_faint())
            .child(
                // Logo / title + window controls
                div()
                    .px(px(16.0))
                    .py(px(14.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    // Window control buttons row
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            // Close
                            .child(
                                div()
                                    .id("win-close")
                                    .w(px(12.0))
                                    .h(px(12.0))
                                    .rounded_full()
                                    .bg(rgb(theme::RED))
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.8))
                                    .on_click(cx.listener(|_this, _event, _window, cx| {
                                        cx.quit();
                                    })),
                            )
                            // Minimize
                            .child(
                                div()
                                    .id("win-minimize")
                                    .w(px(12.0))
                                    .h(px(12.0))
                                    .rounded_full()
                                    .bg(rgb(theme::GOLD))
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.8))
                                    .on_click(cx.listener(|_this, _event, window, _cx| {
                                        window.minimize_window();
                                    })),
                            )
                            // Maximize / Zoom
                            .child(
                                div()
                                    .id("win-zoom")
                                    .w(px(12.0))
                                    .h(px(12.0))
                                    .rounded_full()
                                    .bg(rgb(theme::GREEN))
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.8))
                                    .on_click(cx.listener(|_this, _event, window, _cx| {
                                        window.zoom_window();
                                    })),
                            )
                            // Spacer pushes title right
                            .child(div().flex_grow())
                            // Fullscreen toggle (double-click area or explicit button)
                            .child(
                                div()
                                    .id("win-fullscreen")
                                    .text_size(px(10.0))
                                    .text_color(theme::fg_faint())
                                    .cursor_pointer()
                                    .hover(|s| s.text_color(theme::fg_dim()))
                                    .on_click(cx.listener(|_this, _event, window, _cx| {
                                        window.toggle_fullscreen();
                                    }))
                                    .child("⛶"),
                            ),
                    )
                    // Title
                    .child(
                        div()
                            .text_size(px(18.0))
                            .text_color(theme::cyan())
                            .font_weight(FontWeight::BOLD)
                            .child("⟐ Fermi Console"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .child("Forecasting Command Center"),
                    ),
            )
            .child(
                // Separator
                div().h(px(1.0)).mx(px(12.0)).bg(theme::fg_faint()),
            )
            .child(
                // Navigation items
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .mt(px(12.0))
                    .px(px(8.0))
                    .children(
                        Panel::all()
                            .iter()
                            .map(|panel| self.render_nav_item(*panel, cx)),
                    ),
            )
            // Pending cascades badge — visible only when there's at least
            // one entry awaiting operator review. Click → opens the
            // queue review sheet.
            .when(!self.pending_cascades.is_empty(), |el| {
                let n = self.pending_cascades.len();
                el.child(
                    div()
                        .id("pending-cascades-badge")
                        .mt(px(8.0))
                        .mx(px(8.0))
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(rgb(theme::GOLD))
                        .bg(rgb(theme::BG_ELEVATED))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.pending_cascades_sheet_showing = true;
                            // Refresh before showing so the operator
                            // sees the latest state, not a stale poll.
                            this.fetch_pending_cascades(cx);
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_size(px(14.0))
                                .text_color(rgb(theme::GOLD))
                                .child("⚠"),
                        )
                        .child(
                            div()
                                .flex_grow()
                                .flex()
                                .flex_col()
                                .gap(px(1.0))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme::FG))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(format!(
                                            "{} cascade{} pending",
                                            n,
                                            if n == 1 { "" } else { "s" }
                                        )),
                                )
                                .child(
                                    div()
                                        .text_size(px(9.0))
                                        .text_color(theme::fg_dim())
                                        .child("Review · Apply / Dismiss"),
                                ),
                        ),
                )
            })
            // Inbox — pending collaboration invites (Spec 24 §3.5.5). Shown
            // when connected; the badge count drives attention.
            .when(self.connected, |el| {
                let n = self.inbox_invites.len();
                let has = n > 0;
                el.child(
                    div()
                        .id("inbox-badge")
                        .mt(px(8.0))
                        .mx(px(8.0))
                        .px(px(10.0))
                        .py(px(8.0))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(if has {
                            rgb(theme::CYAN)
                        } else {
                            rgb(theme::FG_FAINT)
                        })
                        .bg(rgb(theme::BG_ELEVATED))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.inbox_sheet_showing = true;
                            this.fetch_my_invites(cx);
                            cx.notify();
                        }))
                        .child(
                            div()
                                .text_size(px(14.0))
                                .text_color(if has {
                                    rgb(theme::CYAN)
                                } else {
                                    rgb(theme::FG_DIM)
                                })
                                .child("📥"),
                        )
                        .child(
                            div()
                                .flex_grow()
                                .text_size(px(11.0))
                                .text_color(rgb(theme::FG))
                                .child(if has {
                                    format!("Inbox · {} pending", n)
                                } else {
                                    "Inbox".to_string()
                                }),
                        ),
                )
            })
            .child(
                // Spacer
                div().flex_grow(),
            )
            .child(
                // Bottom status
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .border_t_1()
                    .border_color(theme::fg_faint())
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(if self.connected {
                                theme::green()
                            } else {
                                theme::fg_dim()
                            })
                            .child(if self.connected {
                                format!(
                                    "● {}",
                                    self.user_display_name.as_deref().unwrap_or("Connected")
                                )
                            } else {
                                "○ Not connected".into()
                            }),
                    )
                    // Version + update-check chip. Always visible, always
                    // clickable. Three states drive the label + color:
                    //   (1) `available_update.is_some()` → "⬆ Update to vX"
                    //       (cyan) — clicking opens the release-notes modal.
                    //   (2) `update_check_in_flight`      → "vX — checking…"
                    //       (dim) — button disabled while a check runs.
                    //   (3) idle                          → "vX — up to date"
                    //       (green when we've checked at least once, dim
                    //       until then).
                    .child({
                        let current = env!("CARGO_PKG_VERSION");
                        let (label, color, has_update) =
                            if let Some(ref release) = self.available_update {
                                (format!("⬆ Update to {}", release.tag), theme::CYAN, true)
                            } else if self.update_check_in_flight {
                                (format!("v{} — checking…", current), theme::FG_DIM, false)
                            } else {
                                (format!("v{} — up to date", current), theme::GREEN, false)
                            };
                        div()
                            .id("sidebar-version-chip")
                            .text_size(px(10.0))
                            .text_color(rgb(color))
                            .mt(px(2.0))
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(theme::FG)))
                            .on_click(cx.listener(move |this, _, _w, cx| {
                                if has_update {
                                    this.update_modal_showing = true;
                                    cx.notify();
                                } else {
                                    // Verbose=true so the operator gets a
                                    // visible toast when they're on the
                                    // latest — without it, clicking the
                                    // "up to date" chip looks like a no-op.
                                    this.check_for_updates(true, cx);
                                }
                            }))
                            .child(label)
                    })
                    // Wallet chip — keeps the current credit balance
                    // visible so testers know they can still run
                    // agents. Rendered only when authenticated because
                    // the balance is only meaningful post-signin.
                    .when(self.connected, |el| {
                        let (label, color) = match &self.wallet {
                            Some(w) if w.balance <= 0 => {
                                ("0 credits — out".to_string(), theme::red())
                            }
                            Some(w) if w.balance < 10 => {
                                (format!("{} credits — low", w.balance), theme::gold())
                            }
                            Some(w) => (format!("{} credits", w.balance), theme::green()),
                            None => ("… credits".to_string(), theme::fg_dim()),
                        };
                        el.child(
                            div()
                                .id("sidebar-wallet-chip")
                                .mt(px(6.0))
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(px(4.0))
                                .bg(theme::bg_hover())
                                .border_1()
                                .border_color(theme::fg_faint())
                                .text_size(px(10.0))
                                .text_color(color)
                                .child(label),
                        )
                    })
                    // Shortcuts help chip — the discoverability entry
                    // point for operators who don't know the console's
                    // hotkeys. Same click behavior as the Ctrl+/ hotkey
                    // and the Help → Keyboard Shortcuts menu item.
                    .child(
                        div()
                            .id("sidebar-shortcuts-chip")
                            .mt(px(6.0))
                            .px(px(8.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .bg(theme::bg_hover())
                            .border_1()
                            .border_color(theme::fg_faint())
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_active()).text_color(theme::fg()))
                            .on_click(cx.listener(|this, _, _w, cx| {
                                this.shortcuts_modal_showing = true;
                                cx.notify();
                            }))
                            .child("⌨ Shortcuts · Ctrl+/"),
                    )
                    // ⬆ Update-available badge (Sprint distribution).
                    // Only rendered when the background check has
                    // returned a strictly-newer release. Clicking
                    // opens the release-notes modal.
                    .when(self.available_update.is_some(), |el| {
                        let tag = self
                            .available_update
                            .as_ref()
                            .map(|r| r.tag.clone())
                            .unwrap_or_default();
                        el.child(
                            div()
                                .id("sidebar-update-badge")
                                .mt(px(6.0))
                                .px(px(8.0))
                                .py(px(4.0))
                                .rounded(px(4.0))
                                .bg(theme::bg_hover())
                                .border_1()
                                .border_color(rgb(theme::CYAN))
                                .text_size(px(10.0))
                                .text_color(theme::cyan())
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::bg_active()))
                                .on_click(cx.listener(|this, _, _w, cx| {
                                    this.update_modal_showing = true;
                                    cx.notify();
                                }))
                                .child(format!("⬆ Update to {}", tag)),
                        )
                    }),
            )
    }

    fn render_nav_item(&self, panel: Panel, cx: &Context<Self>) -> impl IntoElement {
        let is_active = self.active_panel == panel;

        div()
            .id(SharedString::from(format!("nav-{}", panel.label())))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(6.0))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    this.navigate(panel, cx);
                }),
            )
            .when(is_active, |el| {
                el.bg(theme::bg_active()).text_color(theme::cyan())
            })
            .when(!is_active, |el| {
                el.text_color(theme::fg_dim())
                    .hover(|style| style.bg(theme::bg_hover()).text_color(theme::fg()))
            })
            .child(
                div()
                    .text_size(px(16.0))
                    .w(px(20.0))
                    .text_color(if is_active {
                        theme::cyan()
                    } else {
                        theme::fg_dim()
                    })
                    .child(panel.icon()),
            )
            .child(div().flex_grow().text_size(px(13.0)).child(panel.label()))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .child(panel.shortcut_hint()),
            )
    }

    // ── Dashboard Panel ─────────────────────────────────────────────────────────────

    /// Big-button action bar on the Dashboard. Encodes the top-level
    /// verbs an operator wants one click away: start a fresh forecast,
    /// pull one from Polymarket, or paste a URL. Each button routes
    /// to the same primitive the menu / composer type-ahead uses —
    /// this is discoverability, not a new codepath.
    fn render_dashboard_hero(&self, cx: &Context<Self>) -> impl IntoElement {
        let hero_btn = |id: &'static str,
                        icon: &'static str,
                        title: &'static str,
                        subtitle: &'static str,
                        accent: u32| {
            div()
                .id(id)
                .flex()
                .flex_col()
                .gap(px(2.0))
                .px(px(16.0))
                .py(px(12.0))
                .min_w(px(200.0))
                .flex_grow()
                .rounded(px(8.0))
                .border_1()
                .border_color(rgb(accent))
                .bg(theme::bg_elevated())
                .cursor_pointer()
                .hover(|s| s.bg(theme::bg_hover()))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(18.0))
                                .text_color(rgb(accent))
                                .child(icon),
                        )
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(theme::fg())
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(title),
                        ),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::fg_dim())
                        .child(subtitle),
                )
        };

        div()
            .flex()
            .flex_wrap()
            .gap(px(12.0))
            .child(
                hero_btn(
                    "dash-new-forecast",
                    "＋",
                    "New forecast",
                    "Type a question. Fermi decomposes into drivers.",
                    theme::CYAN,
                )
                .on_click(cx.listener(|this, _, _w, cx| {
                    let api = this.api.clone();
                    this.cockpit =
                        Some(cx.new(|cx| CockpitState::new(api, this.registry.clone(), cx)));
                    this.active_panel = Panel::Composer;
                    cx.notify();
                })),
            )
            .child(
                hero_btn(
                    "dash-from-pm",
                    "🔮",
                    "From Polymarket",
                    "Browse active prediction markets. Import as a Fermi forecast.",
                    theme::PURPLE,
                )
                .on_click(cx.listener(|this, _, _w, cx| {
                    // In-place: the PM search card renders on the
                    // Dashboard itself (see `render_dashboard`). No
                    // panel switch — the operator stays where they
                    // clicked. Same UI, same handlers as the Portfolio
                    // panel; both call `render_pm_search_card`.
                    this.pm_show_search = true;
                    cx.notify();
                })),
            )
            .child(
                hero_btn(
                    "dash-paste-url",
                    "📎",
                    "Paste Polymarket URL",
                    "Have a market in mind? Paste the link, we import it.",
                    theme::GOLD,
                )
                .on_click(cx.listener(|this, _, _w, cx| {
                    // Same in-place card; the search field accepts
                    // pasted URLs and resolves them to a single match
                    // automatically via `search_polymarket`.
                    this.pm_show_search = true;
                    cx.notify();
                })),
            )
    }

    fn render_dashboard(&self, cx: &Context<Self>) -> impl IntoElement {
        // Extract stats from API response or use defaults
        let (brier, active, resolved, drafts, rank, days_30d) =
            if let Some(ref stats) = self.my_stats {
                (
                    stats.stats.avg_brier.unwrap_or(0.0),
                    stats.stats.active_count.unwrap_or(0) as u32,
                    stats.stats.resolved_count.unwrap_or(0) as u32,
                    stats.stats.draft_count.unwrap_or(0) as u32,
                    stats.rank.unwrap_or(0) as u32,
                    stats.stats.active_days_30d.unwrap_or(0) as u32,
                )
            } else {
                (0.0, 0, 0, 0, 0, 0)
            };

        // Polymarket sync chip: shows count of PM-linked active
        // forecasts and a click-to-refresh affordance. Duplicated from
        // the Portfolio panel so the operator can trigger a resolution
        // check from the Dashboard as well — both are places where
        // "is anything I forecasted resolved yet?" is a natural
        // question.
        let n_pm_linked = self
            .active_forecasts
            .iter()
            .filter(|f| {
                f.metadata
                    .as_ref()
                    .and_then(|m| m.get("polymarket"))
                    .is_some()
            })
            .count();
        let pm_loading = self.pm_resolutions_loading;
        let pm_result = self.pm_resolutions_last_result.clone();
        let pm_label = if pm_loading {
            "⚡ Syncing…".to_string()
        } else if let Some(ref r) = pm_result {
            r.clone()
        } else if n_pm_linked > 0 {
            format!("⚡ Sync {} PM markets", n_pm_linked)
        } else {
            "⚡ PM sync".to_string()
        };
        let pm_accent = if pm_result
            .as_deref()
            .map(|r| r.starts_with("✓"))
            .unwrap_or(false)
        {
            theme::GREEN
        } else if n_pm_linked > 0 {
            theme::GOLD
        } else {
            theme::FG_DIM
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .p(px(24.0))
            .gap(px(20.0))
            .child(
                // Header
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(22.0))
                            .text_color(theme::fg())
                            .font_weight(FontWeight::BOLD)
                            .child("Dashboard"),
                    )
                    .child(if self.connected {
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            // PM sync chip — same handler as the Portfolio
                            // panel's Check Resolutions button so muscle
                            // memory carries over.
                            .child(
                                div()
                                    .id("dashboard-pm-sync-btn")
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .px(px(10.0))
                                    .py(px(4.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(0x1A1A1A))
                                    .border_1()
                                    .border_color(rgb(pm_accent))
                                    .text_size(px(11.0))
                                    .text_color(rgb(pm_accent))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when(!pm_loading, |s| s.cursor_pointer())
                                    .when(!pm_loading, |s| s.hover(|s| s.bg(rgb(theme::BG_HOVER))))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.check_pm_resolutions(cx);
                                    }))
                                    .child(pm_label),
                            )
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme::fg_dim())
                                    .child(format!("🔥 {} active days (30d)", days_30d)),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::fg_faint())
                            .child("Sign in to sync forecasts and use agents")
                            .into_any_element()
                    }),
            )
            // (Sign-in card removed — the auth gate splash shown by
            // Render for FermiConsole handles the unauthenticated case
            // for the entire window, not just this panel.)
            //
            // Hero action bar — the three most common Dashboard verbs
            // rendered as prominent buttons so testers don't have to
            // hunt through menus / other panels. Sits above the stats
            // cards where the operator's eye lands first.
            .when(self.connected, |el| {
                el.child(self.render_dashboard_hero(cx))
            })
            // In-place Polymarket search. When the hero "From
            // Polymarket" / "Paste PM URL" buttons toggle
            // `pm_show_search`, this renders right below the hero —
            // no navigation away from the Dashboard.
            .when(self.connected && self.pm_show_search, |el| {
                el.child(self.render_pm_search_card(cx))
            })
            .child(
                // Stats cards row
                div()
                    .flex()
                    .gap(px(16.0))
                    .child(self.render_stat_card(
                        "Brier Score",
                        &if brier > 0.0 {
                            format!("{:.3}", brier)
                        } else {
                            "—".into()
                        },
                        "Lower is better",
                        theme::GREEN,
                    ))
                    .child(self.render_stat_card(
                        "Active Forecasts",
                        &active.to_string(),
                        &format!("{} resolved · {} drafts", resolved, drafts),
                        theme::CYAN,
                    ))
                    .child(self.render_stat_card(
                        "Portfolios",
                        &self.portfolios.len().to_string(),
                        "research collections",
                        theme::BLUE,
                    ))
                    .child(self.render_stat_card(
                        "Global Rank",
                        &if rank > 0 {
                            format!("#{}", rank)
                        } else {
                            "—".into()
                        },
                        &format!("{} resolved to qualify", 5_u32.saturating_sub(resolved)),
                        theme::GOLD,
                    )),
            )
            // ── Command-center lanes ──────────────────────────────────
            //
            // The Dashboard used to end with three big lists (Live /
            // Drafts / Recently Resolved) followed by an activity feed.
            // That was a book view, not a command center: it told you
            // "what forecasts do I own", nothing about the research
            // team behind them or the marketplace they draw from.
            //
            // The three lists are now virtual portfolio buckets
            // (Portfolio panel, sidebar). What lives here instead:
            //
            //   Row 1  RESEARCH  |  MARKETPLACE     two-column grid
            //   Row 2  TEAMS strip                    full-width
            //   Row 3  RECENT ACTIVITY                full-width, wider
            //
            // This is the "self-improving agentic research team, powered
            // by an expanding marketplace of specialist agents" narrative,
            // above the fold, at the operator's first glance.
            .when(self.connected, |el| {
                el.child(
                    div()
                        .flex()
                        .flex_row()
                        .gap(px(16.0))
                        .child(
                            div()
                                .flex_grow()
                                .flex_basis(px(0.0))
                                .child(self.render_dashboard_research_card(cx)),
                        )
                        .child(
                            div()
                                .flex_grow()
                                .flex_basis(px(0.0))
                                .child(self.render_dashboard_marketplace_card(cx)),
                        ),
                )
            })
            .when(self.connected, |el| {
                el.child(self.render_dashboard_teams_strip(cx))
            })
            .child(self.render_dashboard_activity_feed(cx))
    }

    // ── Dashboard: Research card ───────────────────────────────────────────
    //
    // Left lane of the command center. Tells the "research economy"
    // story: how much evidence has your fleet gathered lately, roughly
    // what did it cost, and which forecasts are the active ones.
    //
    // Cost is *estimated* — ABW doesn't yet expose a per-forecast
    // cost rollup, so we approximate as
    //   sum over agents in `agents_used` of (avg_cost_per_run × 1).
    // That's an honest lower bound ("this forecast has consumed at
    // least one run of each of these agents") and is clearly labeled
    // "est." in the UI so nobody mistakes it for authoritative spend.
    // A follow-up commit can swap this for a real rollup without
    // changing the visual shell.
    fn render_dashboard_research_card(&self, cx: &Context<Self>) -> impl IntoElement {
        // Index server agent execution_stats so we can look up
        // avg_cost_per_run by agent_id in O(1).
        let mut avg_cost_by_id: std::collections::HashMap<String, f64> =
            std::collections::HashMap::new();
        for sc in &self.agent_cards {
            let Some(id) = sc.get("agent_id").and_then(|v| v.as_str()) else {
                continue;
            };
            let stats = sc.get("execution_stats");
            let total_exec = stats
                .and_then(|s| s.get("total_executions"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let total_cost = stats
                .and_then(|s| s.get("total_cost_usd"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if total_exec > 0 && total_cost > 0.0 {
                avg_cost_by_id.insert(id.to_string(), total_cost / total_exec as f64);
            }
        }

        // Walk the operator's own forecasts, summing evidence and
        // estimated cost. `agents_used` shape from the server is an
        // array of `{agent_id, driver_refs: […]}` objects.
        let mut total_evidence: usize = 0;
        let mut total_est_cost: f64 = 0.0;
        // Per-row: (forecast, evidence_count, est_cost, agent_ids)
        let mut rows: Vec<(&Forecast, usize, f64, Vec<String>)> = Vec::new();
        let all_own: Vec<&Forecast> = self
            .active_forecasts
            .iter()
            .chain(self.draft_forecasts.iter())
            .chain(self.resolved_forecasts.iter())
            .collect();
        for f in &all_own {
            let evidence_count = f
                .evidence
                .as_ref()
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            let agent_ids: Vec<String> = f
                .agents_used
                .as_ref()
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|e| {
                            e.get("agent_id")
                                .and_then(|v| v.as_str())
                                .map(str::to_string)
                        })
                        .collect()
                })
                .unwrap_or_default();
            let est_cost: f64 = agent_ids
                .iter()
                .filter_map(|id| avg_cost_by_id.get(id).copied())
                .sum();
            total_evidence += evidence_count;
            total_est_cost += est_cost;
            if evidence_count > 0 || !agent_ids.is_empty() {
                rows.push((*f, evidence_count, est_cost, agent_ids));
            }
        }
        // Newest first — use updated_at where present, fall back to
        // created_at.
        rows.sort_by(|a, b| {
            let ka = a.0.updated_at.as_ref().or(a.0.created_at.as_ref());
            let kb = b.0.updated_at.as_ref().or(b.0.created_at.as_ref());
            kb.cmp(&ka)
        });
        let top: Vec<_> = rows.into_iter().take(5).collect();

        let header_summary = if total_evidence == 0 && total_est_cost == 0.0 {
            "no research yet".to_string()
        } else {
            format!(
                "{} evidence · ⚡ {:.2} est.",
                total_evidence, total_est_cost
            )
        };

        let is_empty = top.is_empty();

        div()
            .flex()
            .flex_col()
            .bg(theme::bg_elevated())
            .rounded(px(8.0))
            .border_1()
            .border_color(theme::fg_faint())
            .overflow_hidden()
            // Header
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(16.0))
                    .py(px(12.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(theme::CYAN))
                                    .child("🔬"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::fg())
                                    .child("Research"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .child(header_summary),
                    ),
            )
            // Body
            .when(is_empty, |el| {
                el.child(
                    div()
                        .p(px(16.0))
                        .text_size(px(11.0))
                        .text_color(theme::fg_faint())
                        .child(
                            "No agent runs on your forecasts yet. Open a forecast \
                             in the Composer and hire an agent to gather evidence.",
                        ),
                )
            })
            .children(top.into_iter().map(|(f, ev_count, est_cost, agent_ids)| {
                let fid = f.id.clone();
                let question = truncate(&f.question_text, 48).to_string();
                let n_agents = agent_ids.len();
                let subline = if est_cost > 0.0 {
                    format!(
                        "{} evidence · {} agent{} · ⚡ {:.2} est.",
                        ev_count,
                        n_agents,
                        if n_agents == 1 { "" } else { "s" },
                        est_cost,
                    )
                } else if n_agents > 0 {
                    format!(
                        "{} evidence · {} agent{} · cost n/a",
                        ev_count,
                        n_agents,
                        if n_agents == 1 { "" } else { "s" },
                    )
                } else {
                    format!("{} evidence", ev_count)
                };
                div()
                    .id(SharedString::from(format!("dash-research-{}", fid)))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(px(16.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::bg_hover()))
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        this.open_forecast(&fid, cx);
                    }))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::fg())
                            .child(question),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .child(subline),
                    )
            }))
            // Footer: link to Agent Fleet for the full picture.
            .child(
                div()
                    .id("dash-research-see-all")
                    .px(px(16.0))
                    .py(px(8.0))
                    .text_size(px(11.0))
                    .text_color(rgb(theme::CYAN))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::bg_hover()))
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        this.navigate(Panel::AgentFleet, cx);
                    }))
                    .child("See all research →"),
            )
    }

    // ── Dashboard: Marketplace card ───────────────────────────────────────
    //
    // Right lane. Surfaces the "expanding community of specialist
    // agents" pillar. Shows the top few Fresh/Rising agents so the
    // operator can discover new hires without opening Agent Fleet.
    //
    // Depends on `build_agent_marketplace` iterating the union of
    // local + server agent_ids (see P0 fix): before that fix, server-
    // only agents were invisible here and this card would always be
    // empty on a fresh install.
    fn render_dashboard_marketplace_card(&self, cx: &Context<Self>) -> impl IntoElement {
        let local_cards = self.registry.list_cards().unwrap_or_default();
        // Same filter render_agent_fleet_panel uses: agents tagged
        // `fermi-orchestra` in their card metadata. The old
        // `agent_type == "forecast_analyst"` filter matched nothing
        // (no curated agent uses that type), which is why the Fresh
        // tier read empty on the Dashboard.
        // Match render_agent_fleet_panel: exclude tier=System agents
        // (Fermi itself, xaman_ek, other infra) so the Dashboard
        // marketplace card only surfaces genuinely hireable specialists.
        let fermi_cards: Vec<&AgentCard> = local_cards
            .iter()
            .filter(|c| c.metadata.tags.iter().any(|t| t == "fermi-orchestra"))
            .filter(|c| !matches!(c.tier, fermi::agent_backend::agent_card::AgentTier::System))
            .collect();
        // Session runs are only relevant when a cockpit is open; on
        // the Dashboard we don't have one so pass an empty slice.
        let no_runs: Vec<cockpit::AgentExecution> = Vec::new();
        let mut entries = build_agent_marketplace(&fermi_cards, &self.agent_cards, &no_runs);
        // Fresh + rising — the "try me" pool. Sort by tier priority
        // first, then by descending score within each tier.
        entries.retain(|e| e.tier == "fresh" || e.tier == "rising");
        entries.sort_by(|a, b| {
            let ta = if a.tier == "fresh" { 0 } else { 1 };
            let tb = if b.tier == "fresh" { 0 } else { 1 };
            ta.cmp(&tb).then(
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });
        let fresh_count = entries.iter().filter(|e| e.tier == "fresh").count();
        let total_agents = self.agent_cards.len().max(fermi_cards.len());
        let top: Vec<AgentMarketplaceEntry> = entries.into_iter().take(4).collect();
        let is_empty = top.is_empty();

        let header_summary = if total_agents == 0 {
            "loading…".to_string()
        } else if fresh_count == 0 {
            format!("{} agents · no new arrivals", total_agents)
        } else {
            format!("{} agents · {} fresh", total_agents, fresh_count)
        };

        div()
            .flex()
            .flex_col()
            .bg(theme::bg_elevated())
            .rounded(px(8.0))
            .border_1()
            .border_color(theme::fg_faint())
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(16.0))
                    .py(px(12.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(theme::PURPLE))
                                    .child("✨"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::fg())
                                    .child("Marketplace"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .child(header_summary),
                    ),
            )
            .when(is_empty, |el| {
                el.child(
                    div()
                        .p(px(16.0))
                        .text_size(px(11.0))
                        .text_color(theme::fg_faint())
                        .child(
                            "No new agents to try right now. Check back \
                             later — the community ships specialists all \
                             the time.",
                        ),
                )
            })
            .children(top.into_iter().map(|e| {
                let aid = e.agent_id.clone();
                let tier_glyph = match e.tier {
                    "fresh" => "✨",
                    "rising" => "▲",
                    _ => "◉",
                };
                let cost_text = e
                    .avg_cost_per_run
                    .map(|c| format!("⚡ {:.2}/run", c))
                    .unwrap_or_else(|| "cost n/a".into());
                let desc = truncate(&e.description, 60).to_string();
                div()
                    .id(SharedString::from(format!("dash-mkt-{}", aid)))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(px(16.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::bg_hover()))
                    .on_click(cx.listener(move |this, _e, _w, cx| {
                        // Take the operator to the marketplace with the
                        // clicked agent already expanded so they can
                        // review terms before hiring. Same primitive
                        // the Fleet panel uses for its cards.
                        this.agent_marketplace_expanded.insert(aid.clone());
                        this.navigate(Panel::AgentFleet, cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(e.tier_color))
                                    .child(tier_glyph),
                            )
                            .child(
                                div()
                                    .flex_grow()
                                    .text_size(px(12.0))
                                    .text_color(theme::fg())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(e.display_name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme::fg_dim())
                                    .child(cost_text),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .child(desc),
                    )
            }))
            .child(
                div()
                    .id("dash-mkt-see-all")
                    .px(px(16.0))
                    .py(px(8.0))
                    .text_size(px(11.0))
                    .text_color(rgb(theme::PURPLE))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::bg_hover()))
                    .on_click(cx.listener(|this, _e, _w, cx| {
                        this.navigate(Panel::AgentFleet, cx);
                    }))
                    .child("Browse marketplace →"),
            )
    }

    // ── Dashboard: Teams strip ──────────────────────────────────────────────
    //
    // A single-row strip of team cards. Each card is a small tile with
    // the team name, one-line description, and a click handler that
    // navigates to the Teams panel with the team preselected.
    //
    // Rosters are not fetched eagerly here — that would fan out one
    // `get_team(id)` per team on every Dashboard visit. The user's
    // "elegant and minimal" ask means we surface the team _existence_
    // here and defer the full roster + shared-forecasts view to the
    // Teams panel one click away.
    fn render_dashboard_teams_strip(&self, cx: &Context<Self>) -> impl IntoElement {
        // Filter to Fermi-vertical teams the same way
        // `render_teams_panel` does. ABW returns every vertical's teams
        // through `/api/teams`; the console only cares about ones
        // tagged `fermi_forecast`.
        let teams: Vec<&Team> = self
            .teams
            .iter()
            .filter(|t| {
                t.origin
                    .as_deref()
                    .map(|o| o == "fermi_forecast")
                    .unwrap_or(true)
            })
            .collect();

        let is_empty = teams.is_empty();

        div()
            .flex()
            .flex_col()
            .bg(theme::bg_elevated())
            .rounded(px(8.0))
            .border_1()
            .border_color(theme::fg_faint())
            .overflow_hidden()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(16.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(theme::GOLD))
                                    .child("👥"),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::fg())
                                    .child("Teams"),
                            )
                            .child(div().text_size(px(11.0)).text_color(theme::fg_dim()).child(
                                if is_empty {
                                    "none yet".to_string()
                                } else {
                                    format!(
                                        "{} team{}",
                                        teams.len(),
                                        if teams.len() == 1 { "" } else { "s" }
                                    )
                                },
                            )),
                    )
                    .child(
                        div()
                            .id("dash-teams-manage")
                            .text_size(px(11.0))
                            .text_color(rgb(theme::GOLD))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()))
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .on_click(cx.listener(|this, _e, _w, cx| {
                                this.navigate(Panel::Teams, cx);
                            }))
                            .child("Manage →"),
                    ),
            )
            .when(is_empty, |el| {
                el.child(
                    div()
                        .p(px(16.0))
                        .text_size(px(11.0))
                        .text_color(theme::fg_faint())
                        .child(
                            "You're not in any teams yet. Create one from the \
                             Teams panel to share forecasts with collaborators.",
                        ),
                )
            })
            .when(!is_empty, |el| {
                el.child(
                    div()
                        .flex()
                        .flex_row()
                        .flex_wrap()
                        .gap(px(10.0))
                        .p(px(12.0))
                        .children(
                            teams
                                .into_iter()
                                .map(|t| self.render_dashboard_team_card(t, cx)),
                        ),
                )
            })
    }

    // Individual team tile in the Dashboard's Teams strip. Uses the
    // team's slug initial as an inline glyph so cards read at a glance
    // even with the roster collapsed. On hover the card expands to
    // reveal member names — a lightweight roster peek that keeps
    // rosters findable without a panel switch.
    //
    // Data sources:
    //   * team.name/description — always available from `fetch_teams`.
    //   * team_details[id] — warmed in the background by
    //     `refresh_team_details_cache`; may be None on the first
    //     render tick, in which case the sub-line reads "loading…"
    //     and the hover state gracefully degrades.
    //
    // Click navigates to the Teams panel with the team selected — the
    // full roster + shared items + activity view live there.
    fn render_dashboard_team_card(&self, team: &Team, cx: &Context<Self>) -> impl IntoElement {
        let tid = team.id.clone();
        let tid_hover_enter = tid.clone();
        let tid_hover_leave = tid.clone();
        let tid_click = tid.clone();
        let name = team.name.clone();
        let initial = team
            .name
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_else(|| "?".into());
        // Team-specific accent — same colour the team-dot uses on
        // forecast rows and activity items. Renders the initial badge
        // in the team's colour so the operator's mental model links
        // this card to "the amber dot on those three forecasts".
        let accent = team_color(&team.id);

        let detail = self.team_details.get(&tid);
        let member_count = detail.map(|d| d.members.len());
        let is_hovered = self.hovered_team_id.as_deref() == Some(&tid);

        // Sub-line: prefer the concrete member count once loaded;
        // fall back to the team description if there is one; else
        // "loading…" during the first cache-warm tick.
        let subline: String = if let Some(n) = member_count {
            format!("{} member{}", n, if n == 1 { "" } else { "s" })
        } else if let Some(d) = team.description.as_deref().filter(|d| !d.is_empty()) {
            truncate(d, 44).to_string()
        } else {
            "loading…".into()
        };

        div()
            .id(SharedString::from(format!("dash-team-{}", tid)))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(12.0))
            .py(px(10.0))
            .min_w(px(200.0))
            .max_w(px(280.0))
            .flex_grow()
            .rounded(px(6.0))
            .border_1()
            .border_color(if is_hovered {
                rgb(accent).into()
            } else {
                theme::fg_faint()
            })
            .bg(if is_hovered {
                theme::bg_hover()
            } else {
                theme::bg()
            })
            .cursor_pointer()
            .on_hover(cx.listener(move |this, is_over: &bool, _w, cx| {
                // on_hover fires with a bool: true on enter, false on
                // leave. We flip `hovered_team_id` accordingly, but
                // only clear it if it still points at this card —
                // otherwise a fast pointer sweep could clear the state
                // of the card that's currently under the pointer.
                if *is_over {
                    this.hovered_team_id = Some(tid_hover_enter.clone());
                } else if this.hovered_team_id.as_deref() == Some(tid_hover_leave.as_str()) {
                    this.hovered_team_id = None;
                }
                cx.notify();
            }))
            .on_click(cx.listener(move |this, _e, _w, cx| {
                this.selected_team_id = Some(tid_click.clone());
                this.selected_team_detail = None;
                this.navigate(Panel::Teams, cx);
            }))
            // Row 1: initial glyph + name + member count.
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .w(px(32.0))
                            .h(px(32.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.0))
                            .bg(rgb(accent))
                            .text_size(px(14.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(theme::BG))
                            .child(initial),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_grow()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::fg())
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme::fg_dim())
                                    .child(subline),
                            ),
                    ),
            )
            // Row 2 (hover only): compact roster — up to 4 member
            // names truncated + '+N' overflow. When the cache hasn't
            // landed yet, shows a discreet "roster loading…" hint
            // so the hover doesn't feel broken.
            .when(is_hovered, |el| {
                el.child(self.render_team_card_roster_peek(&tid, accent))
            })
    }

    /// Roster peek shown on team-card hover. Bordered strip below the
    /// main row with up to 4 member labels + "+N" overflow. Reads
    /// from `team_details`; degrades gracefully with a "loading…"
    /// message when the cache-warm hasn't landed yet.
    fn render_team_card_roster_peek(&self, team_id: &str, accent: u32) -> AnyElement {
        let Some(detail) = self.team_details.get(team_id) else {
            return div()
                .px(px(4.0))
                .py(px(4.0))
                .border_t_1()
                .border_color(theme::fg_faint())
                .text_size(px(10.0))
                .text_color(theme::fg_faint())
                .child("roster loading…")
                .into_any_element();
        };

        // Best-effort display: prefer server-resolved display name;
        // fall back to short_user_label (first 8 of a UUID) so the
        // row stays readable when the users JOIN missed.
        let names: Vec<String> = detail
            .members
            .iter()
            .map(|m| {
                m.member_display_name
                    .clone()
                    .unwrap_or_else(|| short_user_label(&m.member_id))
            })
            .collect();

        let show: Vec<&String> = names.iter().take(4).collect();
        let overflow = names.len().saturating_sub(4);

        let mut row = div()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(px(4.0))
            .px(px(4.0))
            .py(px(6.0))
            .border_t_1()
            .border_color(theme::fg_faint());

        for label in show {
            row = row.child(
                div()
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(4.0))
                    .bg(theme::bg_elevated())
                    .border_1()
                    .border_color(rgb(accent))
                    .text_size(px(10.0))
                    .text_color(theme::fg())
                    .child(truncate(label, 20)),
            );
        }
        if overflow > 0 {
            row = row.child(
                div()
                    .px(px(6.0))
                    .py(px(2.0))
                    .text_size(px(10.0))
                    .text_color(theme::fg_dim())
                    .child(format!("+{}", overflow)),
            );
        }
        row.into_any_element()
    }

    // ── Dashboard: Activity feed (wider, filterable) ────────────────────────
    //
    // The activity ticker at the bottom of the Dashboard. Extracted
    // from the old `render_dashboard` body so it can grow filter chips
    // (all/mine/team/marketplace) without inflating the top-level fn.
    //
    // Today only "mine" has content — the events are all derived from
    // the operator's own forecasts by `recompute_recent_activity`.
    // Team and marketplace chips are rendered but disabled with a
    // "coming soon" tooltip so the future information architecture is
    // visible without pretending we have data we don't.
    fn render_dashboard_activity_feed(&self, cx: &Context<Self>) -> impl IntoElement {
        let filter = self.dashboard_activity_filter;
        // Filter feed by source. `All` shows every item; the other
        // variants gate on `ActivityItem.source`. Marketplace stays
        // empty until we ingest marketplace publish events.
        let filtered: Vec<&ActivityItem> = self
            .recent_activity
            .iter()
            .filter(|item| match filter {
                ActivityFilter::All => true,
                ActivityFilter::Mine => item.source == ActivitySource::Mine,
                ActivityFilter::Team => item.source == ActivitySource::Team,
                ActivityFilter::Marketplace => item.source == ActivitySource::Marketplace,
            })
            .collect();
        let n = filtered.len();

        // Counts per source — drives the disabled/enabled state and
        // the numeric badge on each chip. Marketplace is always 0
        // until we ingest events, so its chip stays disabled.
        let mut mine_count = 0usize;
        let mut team_count = 0usize;
        for item in &self.recent_activity {
            match item.source {
                ActivitySource::Mine => mine_count += 1,
                ActivitySource::Team => team_count += 1,
                ActivitySource::Marketplace => {}
            }
        }
        let total = self.recent_activity.len();

        div()
            .flex()
            .flex_col()
            .flex_grow()
            .bg(theme::bg_elevated())
            .rounded(px(8.0))
            .border_1()
            .border_color(theme::fg_faint())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px(px(16.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme::fg())
                                    .child("Recent Activity"),
                            )
                            .child(div().text_size(px(11.0)).text_color(theme::fg_dim()).child(
                                if n == 0 {
                                    "nothing yet".to_string()
                                } else {
                                    format!("{} event{}", n, if n == 1 { "" } else { "s" })
                                },
                            )),
                    )
                    // Source filter chips — now live. Mine + Team
                    // gate on ActivityItem.source; Marketplace stays
                    // a disabled placeholder until we ingest
                    // marketplace publish events.
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .child(activity_filter_chip(
                                "act-chip-all",
                                &format!("All ({})", total),
                                ActivityFilter::All,
                                filter,
                                false,
                                cx,
                            ))
                            .child(activity_filter_chip(
                                "act-chip-mine",
                                &format!("👤 Mine ({})", mine_count),
                                ActivityFilter::Mine,
                                filter,
                                mine_count == 0,
                                cx,
                            ))
                            .child(activity_filter_chip(
                                "act-chip-team",
                                &format!("👥 Team ({})", team_count),
                                ActivityFilter::Team,
                                filter,
                                team_count == 0,
                                cx,
                            ))
                            // Marketplace: no ingestion path yet
                            // (surface P4 in the v0.8.6 plan). Kept
                            // visible as a disabled chip so the
                            // architecture is legible.
                            .child(activity_source_chip("✨ Marketplace", false, true)),
                    ),
            )
            .child(
                div().flex().flex_col().p(px(8.0)).gap(px(2.0)).children(
                    filtered
                        .into_iter()
                        .map(|item| self.render_activity_item(item, cx)),
                ),
            )
    }

    fn render_stat_card(
        &self,
        label: &str,
        value: &str,
        subtitle: &str,
        accent: u32,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_grow()
            .bg(theme::bg_elevated())
            .rounded(px(8.0))
            .border_1()
            .border_color(theme::fg_faint())
            .p(px(16.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme::fg_dim())
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(px(28.0))
                    .text_color(rgb(accent))
                    .font_weight(FontWeight::BOLD)
                    .mt(px(8.0))
                    .child(value.to_string()),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme::fg_dim())
                    .mt(px(4.0))
                    .child(subtitle.to_string()),
            )
    }

    /// Look up a Forecast by id across the three own-lists (active,
    /// draft, resolved). Returns None for shared-with-me / unassigned
    /// forecasts and for stale ids from an older ActivityItem.
    fn find_own_forecast(&self, forecast_id: &str) -> Option<&Forecast> {
        self.active_forecasts
            .iter()
            .chain(self.draft_forecasts.iter())
            .chain(self.resolved_forecasts.iter())
            .find(|f| f.id == forecast_id)
    }

    /// Return every team_id a forecast is associated with. Owning
    /// team first (deduplicated against the share cache), then any
    /// team-share targets in the order they came back from
    /// `/api/forecasts/:id/shares`. Used by the dot-stack renderer to
    /// show up to N dots per forecast row.
    fn team_ids_for_forecast(&self, forecast: &Forecast) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        if let Some(ref tid) = forecast.team_id {
            if !tid.is_empty() && seen.insert(tid.clone()) {
                out.push(tid.clone());
            }
        }
        if let Some(shares) = self.forecast_team_shares.get(&forecast.id) {
            for tid in shares {
                if !tid.is_empty() && seen.insert(tid.clone()) {
                    out.push(tid.clone());
                }
            }
        }
        out
    }

    /// Team-affiliation glyph strip for a forecast row. Behaviour by
    /// team count:
    ///   * 0 teams — empty element (no visual noise).
    ///   * 1 team — dot + tiny team-name label (the v0.8.7 look).
    ///   * 2–3 teams — overlapping dot stack, no label (space would
    ///     eat the row).
    ///   * 4+ teams — top 3 dots plus a "+N" chip.
    ///
    /// Colour comes from `team_color(team_id)` — a deterministic hash
    /// into the theme palette — so the same team is the same colour
    /// everywhere (forecast rows, activity items, team cards in the
    /// Dashboard strip). Overlapping stack uses a small negative
    /// margin so multi-team dots read as "a set" rather than a row
    /// of unrelated dots.
    fn render_team_dots(&self, team_ids: &[String]) -> AnyElement {
        // Filter empties defensively; a shares row with an empty
        // string is a schema bug, not a real team.
        let ids: Vec<&String> = team_ids.iter().filter(|t| !t.is_empty()).collect();
        if ids.is_empty() {
            return div().into_any_element();
        }

        // Single-team path preserves the v0.8.7 look: dot + label.
        // Multi-team drops the label because the strip would push the
        // rest of the row off-screen; the colours + "+N" carry the
        // story on their own.
        if ids.len() == 1 {
            let tid = ids[0];
            let color = team_color(tid);
            let label = self
                .team_name_by_id(tid)
                .unwrap_or_else(|| format!("team …{}", tid.chars().take(4).collect::<String>()));
            return div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(div().w(px(8.0)).h(px(8.0)).rounded(px(4.0)).bg(rgb(color)))
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(theme::fg_faint())
                        .child(truncate(&label, 14)),
                )
                .into_any_element();
        }

        // Multi-team stack. Show at most 3 dots; append "+N" when
        // more exist.
        let visible = ids.iter().take(3);
        let overflow = ids.len().saturating_sub(3);
        let mut stack = div().flex().items_center();
        for (i, tid) in visible.enumerate() {
            let color = team_color(tid);
            let dot = div()
                .w(px(9.0))
                .h(px(9.0))
                .rounded(px(5.0))
                .bg(rgb(color))
                // Thin dark border between overlapping dots so the
                // colour boundaries stay legible against the row's bg.
                .border_1()
                .border_color(theme::bg_elevated());
            // Overlap all but the first dot by a few pixels so the
            // stack reads as one unit.
            stack = stack.child(if i == 0 { dot } else { dot.ml(px(-3.0)) });
        }
        if overflow > 0 {
            stack = stack.child(
                div()
                    .ml(px(4.0))
                    .text_size(px(9.0))
                    .text_color(theme::fg_faint())
                    .child(format!("+{}", overflow)),
            );
        }
        stack.into_any_element()
    }

    /// Back-compat wrapper: single-team rendering, unchanged from
    /// v0.8.7 semantics. New callers should use `render_team_dots`
    /// with the full list.
    fn render_team_dot(&self, team_id: Option<&str>) -> AnyElement {
        match team_id {
            None => div().into_any_element(),
            Some(tid) if tid.is_empty() => div().into_any_element(),
            Some(tid) => self.render_team_dots(&[tid.to_string()]),
        }
    }

    fn render_activity_item(&self, item: &ActivityItem, cx: &Context<Self>) -> impl IntoElement {
        let fid = item.forecast_id.clone();
        // Full team-affiliation set for the activity row's forecast
        // — drives the multi-dot stack. Spans own + shared so team
        // activity items (source: Team, backed by shared_with_me)
        // still show a dot from the owning team.
        let team_ids: Vec<String> = self
            .find_own_forecast(&item.forecast_id)
            .map(|f| self.team_ids_for_forecast(f))
            .or_else(|| {
                self.shared_with_me_forecasts
                    .iter()
                    .find(|f| f.id == item.forecast_id)
                    .map(|f| {
                        // Shared-with-me items don't get the shares
                        // fan-out (that only runs on own forecasts),
                        // so we can only surface the owning team_id
                        // here. Better than no dot.
                        f.team_id
                            .as_ref()
                            .filter(|s| !s.is_empty())
                            .cloned()
                            .into_iter()
                            .collect()
                    })
            })
            .unwrap_or_default();
        div()
            .id(SharedString::from(format!("activity-{}", item.forecast_id)))
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(4.0))
            .cursor_pointer()
            .hover(|style| style.bg(theme::bg_hover()))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.open_forecast(&fid, cx);
            }))
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(rgb(item.color))
                    .w(px(20.0))
                    .child(item.icon),
            )
            .child(
                div()
                    .flex_grow()
                    .text_size(px(13.0))
                    .text_color(theme::fg())
                    .child(item.text.clone()),
            )
            .child(self.render_team_dots(&team_ids))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme::fg_dim())
                    .child(item.time.clone()),
            )
    }

    // ── Auth gate (splash) ────────────────────────────────────────────
    //
    // Full-window sign-in / sign-up splash rendered whenever the user
    // is not authenticated. Blocks every other panel until they
    // complete OAuth — no ad-hoc empty states scattered across each
    // panel, no accidental use of endpoints that need a session.
    //
    // "Log in" and "Sign up" both call the same `start_oauth_flow`
    // because Google/GitHub OAuth doesn't distinguish the two; the
    // server auto-creates the user + wallet + onboarding grant on
    // first callback. We render them as separate buttons anyway
    // because that's the mental model non-technical testers expect,
    // and the copy under "Sign up" tells them what they'll get.
    fn render_auth_gate(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .size_full()
            .bg(theme::bg())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(28.0))
                    .w(px(520.0))
                    .child(
                        // Wordmark + tagline block.
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(32.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme::cyan())
                                    .child("Fermi Console"),
                            )
                            .child(div().text_size(px(13.0)).text_color(theme::fg_dim()).child(
                                "Probabilistic forecasting workspace \
                                         with AI research agents.",
                            ))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme::fg_faint())
                                    .child(format!("v{}  •  BETA", env!("CARGO_PKG_VERSION"))),
                            ),
                    )
                    .child(self.render_sign_in_card(cx))
                    .child(
                        // Footer copy explaining what happens on sign-up.
                        // Keep it short: three bullets, no marketing.
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .items_center()
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .child("New here? Signing in creates your account.")
                            .child("Every new account gets 100 free credits to start.")
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme::fg_faint())
                                    .child(
                                        "Accounts are hosted by Agent Bestiary World, \
                                         the shared backend that powers Fermi Console.",
                                    ),
                            ),
                    ),
            )
    }

    // ── Sign-in Card ──────────────────────────────────────────────────

    fn render_sign_in_card(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(24.0))
            .bg(theme::bg_elevated())
            .rounded(px(8.0))
            .border_1()
            .border_color(theme::fg_faint())
            .max_w(px(480.0))
            .overflow_hidden()
            .child(
                div()
                    .text_size(px(18.0))
                    .text_color(theme::cyan())
                    .font_weight(FontWeight::BOLD)
                    .child("Sign In to Agent Bestiary World"),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme::fg_dim())
                    .child("Sign in to use AI research agents, save forecasts, and compete on the leaderboard."),
            )
            // ── OAuth buttons ─────────────────────────────────────
            .child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .child(
                        div()
                            .id("oauth-google")
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(20.0))
                            .py(px(10.0))
                            .rounded(px(6.0))
                            .bg(rgb(0xFFFFFF))
                            .text_color(rgb(0x333333))
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.9))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.start_oauth_flow("google", cx);
                            }))
                            .child("G")
                            .child("Sign in with Google"),
                    )
                    .child(
                        div()
                            .id("oauth-github")
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(20.0))
                            .py(px(10.0))
                            .rounded(px(6.0))
                            .bg(rgb(0x24292E))
                            .text_color(rgb(0xFFFFFF))
                            .text_size(px(13.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.9))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.start_oauth_flow("github", cx);
                            }))
                            .child("⬡")
                            .child("Sign in with GitHub"),
                    ),
            )
            // Loading / waiting state
            .when(self.sign_in_loading, |el| {
                el.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(theme::gold())
                        .child(if self.oauth_port.is_some() {
                            "Waiting for sign-in in your browser…"
                        } else {
                            "Connecting…"
                        }),
                )
            })
            // Error message
            .when(self.sign_in_error.is_some(), |el| {
                el.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme::red())
                        .child(
                            self.sign_in_error
                                .as_deref()
                                .unwrap_or("")
                                .to_string(),
                        ),
                )
            })
            // ── Fallback message (shown after OAuth timeout) ──────
            .when(self.sign_in_fallback_message, |el| {
                el.child(
                    div()
                        .px(px(12.0))
                        .py(px(10.0))
                        .bg(rgb(0x2A2D3A))
                        .rounded(px(6.0))
                        .border_1()
                        .border_color(theme::gold())
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme::gold())
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Almost there! You signed in on ABW."),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::fg_dim())
                                .child("Copy your session token from your browser:"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::fg())
                                .child("1. Open agent-bestiary.world in your browser"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::fg())
                                .child("2. Open DevTools (F12) → Application → Cookies"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::fg())
                                .child("3. Copy the value of 'abw_session'"),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::fg())
                                .child("4. Paste it below and click Connect"),
                        ),
                )
            })
            // ── Token entry ───────────────────────────────────────
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .pt(px(8.0))
                    .border_t_1()
                    .border_color(theme::fg_faint())
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child(if self.sign_in_fallback_message {
                                "Paste your session token here:"
                            } else {
                                "Or paste a session token directly:"
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .items_end()
                            .child(div().flex_grow().overflow_hidden().child(self.sign_in_token_input.clone()))
                            .child(
                                div()
                                    .id("sign-in-btn")
                                    .px(px(16.0))
                                    .py(px(7.0))
                                    .rounded(px(4.0))
                                    .bg(rgb(theme::CYAN))
                                    .text_color(rgb(theme::BG_DEEP))
                                    .text_size(px(12.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.85))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.sign_in_from_ui(cx);
                                    }))
                                    .child("Connect"),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .child("You can also use the app offline — Ctrl+4 to open the Composer and create local forecasts."),
            )
    }

    // ── Polymarket Search & Import ────────────────────────────────────────

    fn search_polymarket(&mut self, cx: &mut Context<Self>) {
        let raw = self.pm_search_input.read(cx).text().to_string();
        if raw.trim().is_empty() {
            return;
        }
        // If the user pasted a Polymarket URL, extract the event slug.
        // URL format: https://polymarket.com/event/{slug}[/{market-slug}][?...]
        let query = if let Some(rest) = raw
            .trim()
            .strip_prefix("https://polymarket.com/event/")
            .or_else(|| raw.trim().strip_prefix("http://polymarket.com/event/"))
            .or_else(|| raw.trim().strip_prefix("polymarket.com/event/"))
        {
            // Take only the first path segment (drop any sub-market path or query string)
            rest.split(['/', '?', '#'])
                .next()
                .unwrap_or(rest)
                .to_string()
        } else {
            raw.clone()
        };

        self.pm_search_loading = true;
        self.pm_search_results.clear();
        self.pm_search_error = None;
        cx.notify();

        let api = self.api.clone();
        let q = query.clone();

        cx.spawn(async move |this, cx| {
            let result: Result<Vec<serde_json::Value>, String> = async {
                let handle = tokio::spawn(async move {
                    let url = format!("{}/api/polymarket/search", api.base_url().await);
                    let key = api.api_key().unwrap_or_default();
                    let client = reqwest::Client::new();
                    let resp = client
                        .post(&url)
                        .header("Authorization", format!("Bearer {}", key))
                        .header("Content-Type", "application/json")
                        .json(&serde_json::json!({"query": q, "limit": 10}))
                        .timeout(std::time::Duration::from_secs(30))
                        .send()
                        .await
                        .map_err(|e| format!("Network error: {}", e))?;

                    let status = resp.status();
                    if status == 401 {
                        return Err("Not signed in — sign in first to search Polymarket".into());
                    }
                    if status == 402 {
                        return Err(
                            "Insufficient credits — top up your ABW balance to search Polymarket"
                                .into(),
                        );
                    }
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        return Err(format!(
                            "Server error {}: {}",
                            status.as_u16(),
                            body.chars().take(120).collect::<String>()
                        ));
                    }

                    let bytes = resp
                        .bytes()
                        .await
                        .map_err(|e| format!("Failed to read body: {}", e))?;
                    let body = String::from_utf8_lossy(&bytes);
                    log::debug!(
                        "[polymarket] Raw response ({} bytes): {}",
                        bytes.len(),
                        body.chars().take(500).collect::<String>()
                    );
                    let data: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
                        format!(
                            "Bad response ({}): {}",
                            e,
                            body.chars().take(120).collect::<String>()
                        )
                    })?;
                    let matches = data
                        .get("matches")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    Ok(matches)
                })
                .await
                .map_err(|e| format!("Task error: {}", e))?;
                handle
            }
            .await;

            this.update(cx, |this, cx| {
                this.pm_search_loading = false;
                match result {
                    Ok(matches) => {
                        this.pm_search_results = matches;
                        if this.pm_search_results.is_empty() {
                            this.pm_search_error = Some("No matching markets found".into());
                        }
                    }
                    Err(e) => {
                        log::error!("[polymarket] Search failed: {}", e);
                        this.pm_search_error = Some(e);
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn check_pm_resolutions(&mut self, cx: &mut Context<Self>) {
        if self.pm_resolutions_loading {
            return;
        }
        self.pm_resolutions_loading = true;
        self.pm_resolutions_last_result = None;
        let api = self.api.clone();

        cx.spawn(async move |this, cx| {
            match api.check_polymarket_resolutions().await {
                Ok(resp) => {
                    let checked = resp.get("checked").and_then(|v| v.as_i64()).unwrap_or(0);
                    let resolved = resp.get("resolved").and_then(|v| v.as_i64()).unwrap_or(0);
                    this.update(cx, |this, cx| {
                        this.pm_resolutions_loading = false;
                        this.pm_resolutions_last_result = Some(if resolved > 0 {
                            format!("✓ {} resolved", resolved)
                        } else if checked > 0 {
                            format!("{} checked — none resolved", checked)
                        } else {
                            "No linked markets".into()
                        });
                        // Refresh forecasts so resolved ones move to the Resolved bucket
                        if resolved > 0 {
                            this.fetch_forecasts(cx);
                            this.fetch_stats(cx);
                        }
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::warn!("[polymarket] check-resolutions failed: {}", e);
                    this.update(cx, |this, cx| {
                        this.pm_resolutions_loading = false;
                        this.pm_resolutions_last_result = Some("⚠ check failed".into());
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    // ── Portfolio management ──────────────────────────────────────────────────

    fn create_portfolio_from_input(&mut self, cx: &mut Context<Self>) {
        let title = self
            .portfolio_create_input
            .read(cx)
            .text()
            .trim()
            .to_string();
        if title.is_empty() {
            return;
        }
        self.portfolio_create_loading = true;
        self.portfolio_create_error = None;
        let api = self.api.clone();

        cx.spawn(async move |this, cx| {
            let req = CreatePortfolioRequest {
                title,
                description: None,
                domain: None,
                visibility: Some("private".into()),
                team_id: None,
            };
            match api.create_portfolio(&req).await {
                Ok(_) => {
                    this.update(cx, |this, cx| {
                        this.portfolio_create_loading = false;
                        this.portfolio_create_showing = false;
                        this.fetch_portfolios(cx);
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    this.update(cx, |this, cx| {
                        this.portfolio_create_loading = false;
                        this.portfolio_create_error = Some(e.to_string());
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    fn fetch_portfolio_stats_if_needed(&mut self, portfolio_id: String, cx: &mut Context<Self>) {
        if self.portfolio_stats_cache.contains_key(&portfolio_id) {
            return;
        }
        let api = self.api.clone();
        let pid = portfolio_id.clone();

        cx.spawn(
            async move |this, cx| match api.portfolio_stats(&pid).await {
                Ok(stats) => {
                    this.update(cx, |this, cx| {
                        this.portfolio_stats_cache.insert(pid, stats);
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::warn!("Failed to fetch portfolio stats {}: {}", pid, e);
                }
            },
        )
        .detach();
    }

    fn add_forecast_to_portfolio(
        &mut self,
        forecast_id: String,
        portfolio_id: String,
        cx: &mut Context<Self>,
    ) {
        let api = self.api.clone();
        let pid = portfolio_id.clone();

        cx.spawn(async move |this, cx| {
            match api.add_to_portfolio(&portfolio_id, &forecast_id).await {
                Ok(_) => {
                    this.update(cx, |this, cx| {
                        this.portfolio_stats_cache.remove(&pid);
                        this.portfolio_forecasts.remove(&pid);
                        this.fetch_portfolios(cx);
                        if this.selected_portfolio_id.as_deref() == Some(&pid) {
                            this.fetch_portfolio_forecasts(pid.clone(), cx);
                        }
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::warn!(
                        "Failed to add forecast {} to portfolio {}: {}",
                        forecast_id,
                        pid,
                        e
                    );
                }
            }
        })
        .detach();
    }

    fn fetch_portfolio_forecasts(&mut self, portfolio_id: String, cx: &mut Context<Self>) {
        if self.portfolio_forecasts_loading.contains(&portfolio_id) {
            return;
        }
        self.portfolio_forecasts_loading
            .insert(portfolio_id.clone());
        let api = self.api.clone();
        let pid = portfolio_id.clone();

        cx.spawn(
            async move |this, cx| match api.list_portfolio_forecasts(&pid).await {
                Ok(resp) => {
                    this.update(cx, |this, cx| {
                        this.portfolio_forecasts.insert(pid.clone(), resp.forecasts);
                        this.portfolio_forecasts_loading.remove(&pid);
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    log::warn!("Failed to fetch portfolio forecasts {}: {}", pid, e);
                    this.update(cx, |this, cx| {
                        this.portfolio_forecasts_loading.remove(&pid);
                        cx.notify();
                    })
                    .ok();
                }
            },
        )
        .detach();
    }

    fn remove_from_portfolio(
        &mut self,
        portfolio_id: String,
        forecast_id: String,
        cx: &mut Context<Self>,
    ) {
        let api = self.api.clone();
        let pid = portfolio_id.clone();
        let fid = forecast_id.clone();

        cx.spawn(async move |this, cx| {
            match api.remove_from_portfolio(&portfolio_id, &forecast_id).await {
                Ok(_) => {
                    this.update(cx, |this, cx| {
                        if let Some(list) = this.portfolio_forecasts.get_mut(&pid) {
                            list.retain(|f| f.id != fid);
                        }
                        this.portfolio_stats_cache.remove(&pid);
                        this.fetch_portfolios(cx);
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => log::warn!("Failed to remove forecast from portfolio: {}", e),
            }
        })
        .detach();
    }

    fn delete_portfolio(&mut self, portfolio_id: String, cx: &mut Context<Self>) {
        let api = self.api.clone();
        let pid = portfolio_id.clone();

        cx.spawn(
            async move |this, cx| match api.delete_portfolio(&portfolio_id).await {
                Ok(_) => {
                    this.update(cx, |this, cx| {
                        this.portfolios.retain(|p| p.id != pid);
                        this.portfolio_forecasts.remove(&pid);
                        this.portfolio_stats_cache.remove(&pid);
                        if this.selected_portfolio_id.as_deref() == Some(&pid) {
                            this.selected_portfolio_id = None;
                        }
                        this.portfolio_confirm_delete_id = None;
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => log::warn!("Failed to delete portfolio {}: {}", pid, e),
            },
        )
        .detach();
    }

    fn rename_portfolio(
        &mut self,
        portfolio_id: String,
        new_title: String,
        cx: &mut Context<Self>,
    ) {
        let api = self.api.clone();
        let pid = portfolio_id.clone();
        let title = new_title.clone();

        cx.spawn(async move |this, cx| {
            let req = PatchPortfolioRequest {
                title: Some(title),
                description: None,
            };
            match api.patch_portfolio(&portfolio_id, &req).await {
                Ok(_) => {
                    this.update(cx, |this, cx| {
                        if let Some(p) = this.portfolios.iter_mut().find(|p| p.id == pid) {
                            p.title = new_title;
                        }
                        this.portfolio_rename_id = None;
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => log::warn!("Failed to rename portfolio {}: {}", pid, e),
            }
        })
        .detach();
    }

    fn submit_resolve(&mut self, cx: &mut Context<Self>) {
        let outcome = match self.resolve_outcome {
            Some(v) => v,
            None => return,
        };
        let forecast_id = match self.resolve_forecast_id.clone() {
            Some(id) => id,
            None => return,
        };

        self.resolve_loading = true;
        self.resolve_error = None;
        let api = self.api.clone();

        cx.spawn(async move |this, cx| {
            use api::client::ResolveForecastRequest;
            let req = ResolveForecastRequest {
                actual_outcome: outcome,
                resolution_notes: None,
            };
            match api.resolve_forecast(&forecast_id, &req).await {
                Ok(resp) => {
                    // After a successful resolve, look up any
                    // forecast_relationships involving this forecast.
                    // The Resolve sheet then surfaces a Cascade button
                    // per relationship instead of closing immediately —
                    // the operator gets one click to propagate the
                    // resolution across siblings (e.g. WC mutex group:
                    // Brazil eliminated → 47 survivors get probability
                    // bumps proportional to their current p).
                    let relationships = api
                        .list_relationships_for_forecast(&forecast_id)
                        .await
                        .ok()
                        .and_then(|v| v.get("relationships").cloned())
                        .and_then(|v| v.as_array().cloned())
                        .unwrap_or_default();

                    let n_rel = relationships.len();

                    this.update(cx, |this, cx| {
                        this.resolve_loading = false;
                        // Move forecast from active to resolved in local state
                        if let Some(pos) = this
                            .active_forecasts
                            .iter()
                            .position(|f| f.id == forecast_id)
                        {
                            let mut f = this.active_forecasts.remove(pos);
                            f.status = "resolved".into();
                            f.brier_score = Some(resp.brier_score);
                            f.actual_outcome = Some(resp.actual_outcome);
                            this.resolved_forecasts.insert(0, f);
                        }
                        this.portfolio_forecasts.clear();
                        this.portfolio_stats_cache.clear();

                        if n_rel == 0 {
                            // No relationships → close the sheet as before.
                            this.resolve_sheet_showing = false;
                            this.resolve_forecast_id = None;
                        } else {
                            // Stash for the cascade UI; keep the sheet
                            // open so the operator sees the cascade
                            // affordance immediately.
                            this.cascade_relationships = relationships;
                            this.cascade_resolved_forecast_id = Some(forecast_id.clone());
                            this.cascade_resolved_outcome = Some(outcome);
                            this.cascade_summary = None;
                        }
                        // Refresh the pending-cascades badge so the
                        // server-queued entry (from resolve_forecast_handler)
                        // appears in the inbox count.
                        this.fetch_pending_cascades(cx);
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    this.update(cx, |this, cx| {
                        this.resolve_loading = false;
                        this.resolve_error = Some(format!("{}", e));
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// Fire propagation on a relationship using the cascade-resolved
    /// forecast as the trigger. Stashed state from submit_resolve carries
    /// the trigger forecast id + outcome.
    fn fire_cascade(&mut self, relationship_id: String, cx: &mut Context<Self>) {
        let Some(trigger) = self.cascade_resolved_forecast_id.clone() else {
            return;
        };
        let outcome = self.cascade_resolved_outcome;
        self.cascade_loading = true;
        self.cascade_summary = None;
        let api = self.api.clone();

        cx.spawn(async move |this, cx| {
            let body = serde_json::json!({
                "trigger_forecast_id": trigger,
                "trigger_kind": "resolved",
                "outcome": outcome,
            });
            match api.propagate_relationship(&relationship_id, &body).await {
                Ok(resp) => {
                    let n_updated = resp.get("n_updated").and_then(|v| v.as_u64()).unwrap_or(0);
                    let note = resp.get("note").and_then(|v| v.as_str()).unwrap_or("");
                    this.update(cx, |this, cx| {
                        this.cascade_loading = false;
                        this.cascade_summary = Some(if note.is_empty() {
                            format!("Cascaded to {} forecasts.", n_updated)
                        } else {
                            format!("Cascaded to {} forecasts. {}", n_updated, note)
                        });
                        // Keep the toast persistent here — operator
                        // confirms by closing the sheet.
                        // Refresh portfolio data so the UI shows the
                        // new probabilities on the dashboard.
                        this.portfolio_forecasts.clear();
                        this.portfolio_stats_cache.clear();
                        // If we're currently viewing the WC sims
                        // portfolio, force-reload its forecast list.
                        if let Some(pid) = this.selected_portfolio_id.clone() {
                            this.fetch_portfolio_forecasts(pid, cx);
                        }
                        cx.notify();
                    })
                    .ok();
                }
                Err(e) => {
                    this.update(cx, |this, cx| {
                        this.cascade_loading = false;
                        this.cascade_summary = Some(format!("Cascade failed: {}", e));
                        cx.notify();
                    })
                    .ok();
                }
            }
        })
        .detach();
    }

    /// Close the cascade affordance and finalize the resolve flow.
    fn dismiss_cascade(&mut self, cx: &mut Context<Self>) {
        self.resolve_sheet_showing = false;
        self.resolve_forecast_id = None;
        self.cascade_relationships.clear();
        self.cascade_resolved_forecast_id = None;
        self.cascade_resolved_outcome = None;
        self.cascade_summary = None;
        cx.notify();
    }

    /// Open a workspace forecast in the Composer. Creates a FRESH cockpit
    /// connected to the workspace, sets the question from params, and — if
    /// the workspace is backed by a `fermi_forecasts` row — fetches the
    /// forecast and parses its FPL into `cockpit.program` so the Edit tab
    /// shows the actual drivers/agents/decomposition instead of "# Empty
    /// program".
    ///
    /// Without the FPL load, opening a workspace-backed team prior shows a
    /// blank cockpit even though the DB row has a fully-rendered 6-factor +
    /// 2-learnable-driver FPL (~6KB). This is the path used by R-1 refit
    /// (triggers off workspace observations), R-2 sparkles (read from
    /// workspace_bayesops_state), and R-3 trajectory (forecast_timeline).
    fn open_workspace_forecast(&mut self, workspace_id: &str, cx: &mut Context<Self>) {
        let ws_id = workspace_id.to_string();

        // Find the workspace forecast data
        let wf = self
            .workspace_forecasts
            .iter()
            .find(|w| w.workspace_id == ws_id)
            .cloned();
        let forecast_id = wf.as_ref().and_then(|w| w.forecast_id.clone());

        // Always create a fresh cockpit for each workspace — don't reuse
        let api = self.api.clone();
        let cockpit = cx.new(|cx| CockpitState::new(api.clone(), self.registry.clone(), cx));
        self.cockpit = Some(cockpit.clone());

        cockpit.update(cx, |cockpit, cx| {
            // Set workspace_id so outputs publish to the right workspace
            cockpit.workspace_id = Some(ws_id.clone());
            // Wire forecast_id so Trajectory, refit, and sparkline endpoints
            // know which forecast to query.
            cockpit.forecast_id = forecast_id.clone();
            // Fetch the workspace's params output so the next Ctrl+R
            // binds elo_current / gdp_per_capita_log / etc. into the
            // Executor's evaluation context.
            cockpit.load_workspace_params(cx);
            // Load persisted schedules so the Schedules tab shows the
            // 6 already-saved rows (gold On-demand active) instead of
            // re-presenting them as drafts. Without this call,
            // self.schedules is empty, fpl_declared_schedule_drafts
            // dedup check returns nothing-already-persisted, and the
            // tab shows '6 schedule drafts declared by FPL' + Save
            // buttons even though the server has them.
            cockpit.load_schedules(cx);
            // Trajectory is the default right-panel tab; pre-warm it so
            // the worm renders alongside the composer on first open.
            cockpit.load_timeline(cx);
            // Pre-warm the cascade-group chip strip so the operator can
            // see "Not in any cascade group" (or the existing chips)
            // the moment the composer lands, not after they open the
            // Provenance tab.
            cockpit.load_forecast_cascade_groups(cx);

            // Set question and data from workspace params
            if let Some(ref wf) = wf {
                let question = match wf.program_type.as_deref() {
                    Some("TEAM_PRIOR") => {
                        if let Some(ref team_name) = wf.team_name {
                            format!("Will {} win the 2026 FIFA World Cup?", team_name)
                        } else {
                            wf.workspace_name.clone()
                        }
                    }
                    Some("TOURNAMENT_PATH") => {
                        if let Some(ref group) = wf.group {
                            format!("Which team will win Group {}?", group)
                        } else {
                            wf.workspace_name.clone()
                        }
                    }
                    _ => wf.workspace_name.clone(),
                };
                cockpit.question_input.update(cx, |input, cx| {
                    input.set_text(&question, cx);
                });

                if let Some(prob) = wf.probability {
                    cockpit.predicted_probability = prob;
                }
            }

            cockpit.messages.push(crate::cockpit::AssistantMessage {
                node: "workspace".into(),
                kind: crate::cockpit::MessageKind::Info,
                text: format!(
                    "Workspace: {}. Press Ctrl+Enter to decompose, or Ctrl+R to simulate.",
                    wf.as_ref()
                        .map(|w| w.workspace_name.as_str())
                        .unwrap_or(&ws_id),
                ),
            });
            cx.notify();
        });

        // Fetch + parse FPL from the linked forecast row (if any). Same async
        // pattern as fetch_workspace_forecasts: spawn onto the tokio runtime
        // so reqwest has its driver, then update the cockpit entity.
        if let Some(fid) = forecast_id {
            let cockpit_handle = cockpit.clone();
            cx.spawn(async move |_this, cx| {
                let result = tokio::spawn(async move { api.get_forecast(&fid).await }).await;

                match result {
                    Ok(Ok(forecast)) => {
                        let Some(fpl_text) = forecast.fpl_source else {
                            log::warn!(
                                "[workspace-open] forecast {} has no fpl_source",
                                forecast.id
                            );
                            return;
                        };
                        // Parse on the runtime thread; mutate cockpit on the UI thread.
                        let parsed = ::fermi::lexer::Lexer::new(&fpl_text)
                            .tokenize()
                            .ok()
                            .and_then(|tokens| ::fermi::parser::Parser::new(tokens).parse().ok());

                        // Metadata-level base_rate override. Mirrors
                        // `open_forecast`: metadata.base_rate is the
                        // durable channel that survives FPL-round-trip
                        // limits (factor blocks etc), so a persisted
                        // "Update base rate" doesn't silently revert to
                        // the template's initial anchor when re-opened
                        // via the workspace path.
                        let meta_base_rate = forecast
                            .metadata
                            .as_ref()
                            .and_then(|m| m.get("base_rate").cloned());

                        cockpit_handle
                            .update(cx, |cockpit, cx| {
                                cockpit.cached_fpl = fpl_text.clone();
                                match parsed {
                                    Some(program) => {
                                        cockpit.program = program;
                                        // Re-derive question from parsed program if present —
                                        // the FPL is the source of truth for resolution criteria.
                                        if let Some(q) = cockpit.program.question() {
                                            cockpit.question_input.update(cx, |input, cx| {
                                                input.set_text(&q.text, cx);
                                            });
                                        }
                                        cockpit.predicted_probability =
                                            forecast.predicted_probability;
                                        cockpit.messages.push(crate::cockpit::AssistantMessage {
                                            node: "load".into(),
                                            kind: crate::cockpit::MessageKind::Info,
                                            text: format!(
                                                "Loaded FPL from forecast ({} bytes).",
                                                fpl_text.len()
                                            ),
                                        });
                                    }
                                    None => {
                                        cockpit.messages.push(crate::cockpit::AssistantMessage {
                                            node: "load".into(),
                                            kind: crate::cockpit::MessageKind::Warning,
                                            text: "FPL parse failed — showing raw source only."
                                                .into(),
                                        });
                                    }
                                }
                                // Override the parsed base_rate with the
                                // authoritative metadata copy when present.
                                if let Some(br) = meta_base_rate {
                                    let reference_class = br
                                        .get("reference_class")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let historical_frequency = br
                                        .get("historical_frequency")
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0)
                                        .clamp(0.0, 1.0);
                                    let sample_size = br
                                        .get("sample_size")
                                        .and_then(|v| v.as_u64())
                                        .map(|n| n as usize);
                                    let source = br
                                        .get("source")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("fermi")
                                        .to_string();
                                    let reasoning = br
                                        .get("reasoning")
                                        .and_then(|v| v.as_str())
                                        .map(str::to_string);
                                    let generated_by = br
                                        .get("generated_by")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("fermi")
                                        .to_string();
                                    if let Some(q) = cockpit.program.question_mut() {
                                        let prev = q.base_rate.as_ref().map(|b| b.historical_frequency);
                                        log::info!(
                                            "[base-rate-hydrate/workspace] applying metadata.base_rate: prev_from_fpl={:?}, new_from_metadata={:.4} ({})",
                                            prev, historical_frequency, reference_class
                                        );
                                        q.base_rate = Some(fermi::ast::BaseRate {
                                            reference_class,
                                            historical_frequency,
                                            sample_size,
                                            source,
                                            reasoning,
                                            generated_by: if generated_by == "human" {
                                                fermi::ast::GeneratedBy::Human
                                            } else {
                                                fermi::ast::GeneratedBy::Agent(generated_by)
                                            },
                                        });
                                    }
                                }
                                cx.notify();
                            })
                            .ok();
                    }
                    Ok(Err(e)) => {
                        log::error!("[workspace-open] get_forecast failed: {}", e);
                    }
                    Err(e) => {
                        log::error!("[workspace-open] task join error: {}", e);
                    }
                }
            })
            .detach();
        }

        self.active_panel = Panel::Composer;
        cx.notify();
    }

    /// Open a forecast in the Composer by forecast_id. Used by the Portfolio
    /// detail view: clicking a forecast row drops the user into a hydrated
    /// cockpit without making them navigate via the workspace dashboard.
    ///
    /// Same hydration path as open_workspace_forecast — fetch the forecast,
    /// parse fpl_source into cockpit.program, set forecast_id + workspace_id
    /// + predicted_probability so the Trajectory tab, refit, and sparkline
    /// endpoints all work. The only difference is the starting point is the
    /// forecast itself, not a workspace card.
    fn open_forecast(&mut self, forecast_id: &str, cx: &mut Context<Self>) {
        let fid = forecast_id.to_string();

        // Fresh cockpit per drill-in. The "reuse existing cockpit" path
        // bites us with stale pm_*, versions, agent_runs from the
        // previous forecast — see the comment block around CockpitState
        // reset for the long-form reasoning.
        let api = self.api.clone();
        let cockpit = cx.new(|cx| CockpitState::new(api.clone(), self.registry.clone(), cx));
        self.cockpit = Some(cockpit.clone());

        cockpit.update(cx, |cockpit, _cx| {
            cockpit.forecast_id = Some(fid.clone());
        });

        let cockpit_handle = cockpit.clone();
        cx.spawn(async move |_this, cx| {
            let result = tokio::spawn(async move {
                api.get_forecast(&fid).await
            }).await;

            match result {
                Ok(Ok(forecast)) => {
                    let fpl_text = forecast.fpl_source.clone();
                    let parsed = fpl_text.as_ref().and_then(|s| {
                        ::fermi::lexer::Lexer::new(s)
                            .tokenize()
                            .ok()
                            .and_then(|tokens| ::fermi::parser::Parser::new(tokens).parse().ok())
                    });
                    let q_text = forecast.question_text.clone();
                    let prob = forecast.predicted_probability;
                    // Authoritative lifecycle state — drives the cockpit lock
                    // so an eliminated/resolved forecast can't be re-simmed or
                    // re-saved (Spec: reconcile server context, don't make the
                    // user carry it).
                    let f_status = forecast.status.clone();
                    let f_outcome = forecast.actual_outcome;
                    let f_resolution_note = forecast.resolution_notes.clone();
                    // Workspace link, when present, lets the cockpit fire
                    // workspace-scoped endpoints (BayesOps state, refit,
                    // workspace outputs). Without this, the Trajectory tab
                    // would render but the live refit cascade would 404.
                    let ws_id = forecast.workspace_id.clone();
                    // metadata.polymarket carries the linked market shape
                    // written by polymarket::link_handler. Hydrate the PM
                    // fields off this so the cockpit shows the current
                    // crowd price + delta even on first open.
                    let pm = forecast
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("polymarket").cloned());
                    // metadata.base_rate is the durable-anchor channel for
                    // "Update base rate" persistence (see
                    // cockpit.rs `apply_base_rate_only`). We prefer the FPL
                    // path when it round-trips (the emitter now writes
                    // `question { base_rate { … } }`), but this metadata
                    // copy is the belt-and-braces fallback — e.g. when
                    // the FPL contains factor blocks the emitter can't
                    // round-trip and `regenerate_cached_fpl_if_safe` had
                    // to skip.
                    let meta_base_rate = forecast
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("base_rate").cloned());

                    cockpit_handle.update(cx, |cockpit, cx| {
                        // Wire the question text — even if FPL parse fails
                        // the user at least sees the question.
                        cockpit.question_input.update(cx, |input, cx| {
                            input.set_text(&q_text, cx);
                        });
                        cockpit.predicted_probability = prob;
                        cockpit.forecast_status = Some(f_status);
                        cockpit.forecast_outcome = f_outcome;
                        cockpit.resolution_note = f_resolution_note;
                        cockpit.workspace_id = ws_id;
                        // If we have a workspace, fetch its params output
                        // so the next Ctrl+R can bind per-team scalars
                        // (elo_current, gdp_per_capita_log, …) and any
                        // BayesOps-fitted distributions (`<driver>_fitted`)
                        // into the Executor.
                        if cockpit.workspace_id.is_some() {
                            cockpit.load_workspace_params(cx);
                        }
                        // Load persisted schedules so the Schedules tab
                        // shows already-saved rows (gold active) rather
                        // than re-presenting them as FPL drafts. Mirror
                        // of the open_workspace_forecast path.
                        if cockpit.forecast_id.is_some() {
                            cockpit.load_schedules(cx);
                            // Trajectory is the default right-panel tab
                            // now, so pre-warm the timeline so operators
                            // see the worm the moment the cockpit lands.
                            cockpit.load_timeline(cx);
                            // Chip strip prewarm — see open_workspace_forecast.
                            cockpit.load_forecast_cascade_groups(cx);
                        }
                        // Populate the composer's inline portfolio chips.
                        // These come from the operator's portfolios list;
                        // membership starts empty and gets hydrated below
                        // from the freshly-loaded Forecast's `portfolios`.
                        cockpit.load_portfolios_list(cx);
                        cockpit.current_portfolio_ids = forecast
                            .portfolios
                            .clone()
                            .unwrap_or_default()
                            .into_iter()
                            .collect();

                        // ── Polymarket hydration ────────────────────────────
                        // metadata.polymarket shape is what
                        // polymarket::link_handler wrote. The pm_market_price
                        // etc. fields on the cockpit are what the right-side
                        // panel reads to render the crowd-vs-fermi delta.
                        //
                        // These cached values can be MINUTES–HOURS stale
                        // depending on when the market was last snapshotted.
                        // We seed them so the panel isn't blank, then fire
                        // `refresh_pm_price_now` to force a fresh Gamma-API
                        // fetch — without this, opening a WC-team forecast
                        // showed e.g. "France 18.4%" for 5 min when the
                        // real live crowd was already at 33%.
                        if let Some(pm) = pm.as_ref() {
                            cockpit.pm_event_id = pm.get("pm_event_id").and_then(|v| v.as_str()).map(String::from);
                            cockpit.pm_market_id = pm.get("pm_market_id").and_then(|v| v.as_str()).map(String::from);
                            cockpit.pm_question = pm.get("pm_question").and_then(|v| v.as_str()).map(String::from);
                            cockpit.pm_market_price = pm.get("last_market_price").and_then(|v| v.as_f64());
                            cockpit.pm_volume_24h = pm.get("last_volume_24h").and_then(|v| v.as_f64());
                            cockpit.pm_url = pm.get("pm_url").and_then(|v| v.as_str()).map(String::from);
                            cockpit.pm_confidence = pm.get("last_confidence").and_then(|v| v.as_str()).map(String::from);
                            // Resume PM price polling at 5 min — matches the
                            // legacy local-mode restore behavior.
                            if cockpit.pm_event_id.is_some() {
                                cockpit.pm_poll_interval = Some(std::time::Duration::from_secs(5 * 60));
                                // Force an immediate fresh snapshot so the
                                // crowd number is live-accurate on load,
                                // not "whatever the DB happened to cache".
                                cockpit.refresh_pm_price_now(cx);
                            }
                        }

                        if let Some(fpl) = fpl_text.as_ref() {
                            cockpit.cached_fpl = fpl.clone();
                        }

                        // Apply metadata.base_rate. Runs after the FPL
                        // parse arm below and *unconditionally overrides*
                        // any base_rate the FPL carried. Rationale:
                        //
                        // `metadata.base_rate` is written by every
                        // "Update base rate" / "Anchor base rate" click,
                        // whereas `fpl_source` may still carry the
                        // original template's embedded base_rate when the
                        // FPL contains constructs the cockpit AST can't
                        // round-trip (factor blocks, custom estimates —
                        // very common on WC team_prior templates).
                        //
                        // Bug this fixes: user clicks Update base rate
                        // → 52% → leaves cockpit → returns → sees 2.08%
                        // because open_forecast reparsed the FPL and
                        // the template's equal-prior baseline won over
                        // the actual persisted metadata value.
                        //
                        // If both channels have a base_rate, metadata
                        // wins. If only FPL has one, keep it (it's the
                        // template's initial anchor, no user has
                        // overridden it yet). If only metadata has one,
                        // hydrate it into the AST.
                        let apply_meta_base_rate = |cockpit: &mut crate::cockpit::CockpitState| {
                            let Some(ref br) = meta_base_rate else { return };
                            let reference_class = br
                                .get("reference_class")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let historical_frequency = br
                                .get("historical_frequency")
                                .and_then(|v| v.as_f64())
                                .unwrap_or(0.0)
                                .clamp(0.0, 1.0);
                            let sample_size = br
                                .get("sample_size")
                                .and_then(|v| v.as_u64())
                                .map(|n| n as usize);
                            let source = br
                                .get("source")
                                .and_then(|v| v.as_str())
                                .unwrap_or("fermi")
                                .to_string();
                            let reasoning = br
                                .get("reasoning")
                                .and_then(|v| v.as_str())
                                .map(str::to_string);
                            let generated_by = br
                                .get("generated_by")
                                .and_then(|v| v.as_str())
                                .unwrap_or("fermi")
                                .to_string();
                            if let Some(q) = cockpit.program.question_mut() {
                                let prev = q.base_rate.as_ref().map(|b| b.historical_frequency);
                                log::info!(
                                    "[base-rate-hydrate] applying metadata.base_rate: prev_from_fpl={:?}, new_from_metadata={:.4} ({})",
                                    prev, historical_frequency, reference_class
                                );
                                q.base_rate = Some(fermi::ast::BaseRate {
                                    reference_class,
                                    historical_frequency,
                                    sample_size,
                                    source,
                                    reasoning,
                                    generated_by: if generated_by == "human" {
                                        fermi::ast::GeneratedBy::Human
                                    } else {
                                        fermi::ast::GeneratedBy::Agent(generated_by)
                                    },
                                });
                            }
                        };

                        match parsed {
                            Some(program) => {
                                cockpit.program = program;
                                // FPL is the source of truth for the question
                                // when present (resolution criteria live in
                                // the program's question node).
                                if let Some(q) = cockpit.program.question() {
                                    cockpit.question_input.update(cx, |input, cx| {
                                        input.set_text(&q.text, cx);
                                    });
                                }

                                // ── Hydrate accumulated research from
                                //    fermi_forecasts.evidence ────────────
                                //
                                // The FPL template ships agent + driver
                                // skeletons but no evidence. As agents run,
                                // process_agent_evidence pushes the
                                // accumulated evidence list to
                                // fermi_forecasts.evidence JSONB via
                                // push_research_state_to_server. On open
                                // we merge that back into the AST so
                                // research survives across sessions.
                                //
                                // Dedup by id: an evidence stmt that ALSO
                                // appears in the loaded FPL (rare for the
                                // WC template, common for hand-authored
                                // local FPLs) takes precedence over the
                                // server copy with the same id.
                                let mut n_hydrated = 0usize;
                                if let Some(serde_json::Value::Array(arr)) = forecast.evidence.as_ref().map(|v| v.clone()) {
                                    let existing_ids: std::collections::HashSet<String> = cockpit
                                        .program
                                        .evidence_items()
                                        .iter()
                                        .map(|e| e.id.clone())
                                        .collect();
                                    for raw in arr {
                                        let Some(id) = raw.get("id").and_then(|v| v.as_str()) else { continue };
                                        if existing_ids.contains(id) {
                                            continue;
                                        }
                                        let ev = fermi::ast::EvidenceStmt {
                                            id: id.to_string(),
                                            source: raw
                                                .get("source")
                                                .and_then(|v| v.as_str())
                                                .unwrap_or("unknown")
                                                .to_string(),
                                            summary: raw
                                                .get("summary")
                                                .and_then(|v| v.as_str())
                                                .map(String::from),
                                            url: raw
                                                .get("url")
                                                .and_then(|v| v.as_str())
                                                .map(String::from),
                                            relevance: raw
                                                .get("relevance")
                                                .and_then(|v| v.as_f64()),
                                            date: raw
                                                .get("date")
                                                .and_then(|v| v.as_str())
                                                .map(String::from),
                                            strength: raw
                                                .get("strength")
                                                .and_then(|v| v.as_f64()),
                                            key_findings: raw
                                                .get("key_findings")
                                                .and_then(|v| v.as_array())
                                                .map(|a| {
                                                    a.iter()
                                                        .filter_map(|v| v.as_str().map(String::from))
                                                        .collect()
                                                })
                                                .unwrap_or_default(),
                                        };
                                        cockpit.program.add_evidence(ev);
                                        n_hydrated += 1;
                                    }
                                }

                                cockpit.messages.push(crate::cockpit::AssistantMessage {
                                    node: "load".into(),
                                    kind: crate::cockpit::MessageKind::Info,
                                    text: if n_hydrated > 0 {
                                        format!(
                                            "Loaded forecast ({} bytes FPL, {} evidence restored from server).",
                                            fpl_text.as_ref().map(String::len).unwrap_or(0),
                                            n_hydrated
                                        )
                                    } else {
                                        format!(
                                            "Loaded forecast ({} bytes FPL).",
                                            fpl_text.as_ref().map(String::len).unwrap_or(0)
                                        )
                                    },
                                });
                            }
                            None if fpl_text.is_some() => {
                                cockpit.messages.push(crate::cockpit::AssistantMessage {
                                    node: "load".into(),
                                    kind: crate::cockpit::MessageKind::Warning,
                                    text: "FPL parse failed — showing raw source only.".into(),
                                });
                            }
                            None => {
                                cockpit.messages.push(crate::cockpit::AssistantMessage {
                                    node: "load".into(),
                                    kind: crate::cockpit::MessageKind::Info,
                                    text: "Forecast has no FPL — starting empty cockpit.".into(),
                                });
                            }
                        }
                        // Fallback: if the AST doesn't already carry a
                        // base_rate (FPL didn't have one, or parse
                        // failed), pick it up from metadata.
                        apply_meta_base_rate(cockpit);
                        // Opportunistic backfill: if the AST now has a
                        // base_rate but the server's metadata.base_rate
                        // is empty, PATCH it up so a future re-open
                        // sees the value even after the FPL is
                        // regenerated/edited. This closes the migration
                        // gap for forecasts whose base_rate was set in
                        // memory before the Stage 1 persist wiring
                        // landed.
                        let server_had_meta_br = meta_base_rate.is_some();
                        let ast_has_br = cockpit
                            .program
                            .question()
                            .and_then(|q| q.base_rate.as_ref())
                            .is_some();
                        if ast_has_br && !server_had_meta_br {
                            log::info!(
                                "[base-rate-backfill] AST carries base_rate but server metadata is empty — persisting"
                            );
                            cockpit.persist_base_rate(cx);
                        }
                        cx.notify();
                    }).ok();
                }
                Ok(Err(e)) => {
                    log::error!("[open-forecast] get_forecast failed: {}", e);
                }
                Err(e) => {
                    log::error!("[open-forecast] task join error: {}", e);
                }
            }
        }).detach();

        self.active_panel = Panel::Composer;
        cx.notify();
    }

    fn import_polymarket_forecast(
        &mut self,
        pm_event_id: &str,
        pm_market_id: &str,
        question: &str,
        market_price: f64,
        volume_24h: Option<f64>,
        liquidity: Option<f64>,
        confidence: Option<String>,
        price_change_1w: Option<f64>,
        cx: &mut Context<Self>,
    ) {
        self.pm_show_search = false;

        // Always start from a fresh cockpit. The old code only
        // created a new one when `self.cockpit.is_none()`, which
        // meant importing a second PM market while a previous forecast
        // was still open reused the same CockpitState — leaking its
        // forecast_id, program (drivers/evidence/base rate), timeline,
        // provenance, PM price history, resolved metadata, session
        // cost, agent_runs, and messages into the newly-imported
        // forecast. Symptom: the newly-imported question rendered on
        // top of the previous forecast's trajectory, resolution
        // banner, cascade events, and "Locked: Resolved→No" chrome.
        //
        // Same pattern as `on_new_forecast` / `on_reset_cockpit`:
        // unconditional replace. GC drops the old CockpitState the
        // moment we overwrite the Option.
        let api = self.api.clone();
        self.cockpit = Some(cx.new(|cx| CockpitState::new(api, self.registry.clone(), cx)));
        // Selection tracker on the FermiConsole itself — not owned by
        // the cockpit — must also be cleared so panel views that key
        // off it (Portfolio expand, forecast detail) don't keep
        // pointing at the previous forecast.
        self.selected_forecast_id = None;

        // Pre-populate cockpit with the PM question and link data
        if let Some(ref cockpit) = self.cockpit {
            let cockpit = cockpit.clone();
            let q = question.to_string();
            let eid = pm_event_id.to_string();
            let mid = pm_market_id.to_string();
            let price = market_price;
            let vol = volume_24h;
            let liq = liquidity;
            let conf = confidence;
            let chg_1w = price_change_1w;

            cockpit.update(cx, |cockpit, cx| {
                // Set the question
                cockpit.question_input.update(cx, |input, cx| {
                    input.set_text(&q, cx);
                });

                // Store PM link data
                cockpit.pm_event_id = Some(eid.clone());
                cockpit.pm_market_id = Some(mid.clone());
                cockpit.pm_question = Some(q.clone());
                cockpit.pm_market_price = Some(price);
                cockpit.pm_url = Some(format!(
                    "https://polymarket.com/event/{}",
                    eid
                ));
                cockpit.pm_volume_24h = vol;
                cockpit.pm_liquidity = liq;
                cockpit.pm_confidence = conf;
                cockpit.pm_price_change_1w = chg_1w;

                // Start polling PM price every 5 minutes by default
                cockpit.set_pm_poll_interval(
                    std::time::Duration::from_secs(5 * 60),
                    cx,
                );

                // Set initial probability to market price
                cockpit.predicted_probability = price.clamp(0.01, 0.99);

                cockpit.messages.push(crate::cockpit::AssistantMessage {
                    node: "question".into(),
                    kind: crate::cockpit::MessageKind::Info,
                    text: format!(
                        "🔮 Imported from Polymarket: \"{}\". Crowd price: {:.1}%. Press Ctrl+Enter to run Fermi decomposition.",
                        q,
                        price * 100.0
                    ),
                });

                cx.notify();
            });
        }

        // Switch to Composer panel
        self.active_panel = Panel::Composer;
        cx.notify();
    }

    // ── Portfolio Panel ───────────────────────────────────────────────────

    fn render_portfolios_section(&self, cx: &Context<Self>) -> impl IntoElement {
        let selected = self.selected_portfolio_id.clone();

        div()
            .flex()
            .flex_col()
            .bg(theme::bg_elevated())
            .rounded(px(8.0))
            .border_1()
            .border_color(theme::fg_faint())
            // ── Header ───────────────────────────────────────────────
            .child(
                div()
                    .px(px(16.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(theme::BLUE))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("Portfolios ({})", self.portfolios.len())),
                    )
                    .child(
                        div()
                            .id("create-portfolio-btn")
                            .px(px(10.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(rgb(theme::CYAN))
                            .text_size(px(11.0))
                            .text_color(rgb(theme::CYAN))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.portfolio_create_showing = !this.portfolio_create_showing;
                                this.portfolio_create_error = None;
                                cx.notify();
                            }))
                            .child("+ New"),
                    ),
            )
            // ── Create form (inline, collapsible) ────────────────────
            .when(self.portfolio_create_showing, |el| {
                el.child(
                    div()
                        .px(px(16.0))
                        .py(px(10.0))
                        .border_b_1()
                        .border_color(theme::fg_faint())
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().flex_grow().child(self.portfolio_create_input.clone()))
                        .when(!self.portfolio_create_loading, |el| {
                            el.child(
                                div()
                                    .id("portfolio-create-submit")
                                    .px(px(12.0))
                                    .py(px(5.0))
                                    .rounded(px(4.0))
                                    .bg(rgb(theme::CYAN))
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::BG))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.85))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.create_portfolio_from_input(cx);
                                    }))
                                    .child("Create"),
                            )
                        })
                        .when(self.portfolio_create_loading, |el| {
                            el.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_dim())
                                    .child("Creating…"),
                            )
                        })
                        .child(
                            div()
                                .id("portfolio-create-cancel")
                                .px(px(8.0))
                                .py(px(5.0))
                                .rounded(px(4.0))
                                .text_size(px(11.0))
                                .text_color(theme::fg_dim())
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::bg_hover()))
                                .on_click(cx.listener(|this, _event, _window, cx| {
                                    this.portfolio_create_showing = false;
                                    this.portfolio_create_error = None;
                                    cx.notify();
                                }))
                                .child("Cancel"),
                        )
                        .when(self.portfolio_create_error.is_some(), |el| {
                            el.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::red())
                                    .child(
                                        self.portfolio_create_error
                                            .as_deref()
                                            .unwrap_or("")
                                            .to_string(),
                                    ),
                            )
                        }),
                )
            })
            // ── Two-column body ───────────────────────────────────────
            .child(
                div()
                    .flex()
                    .flex_row()
                    .min_h(px(200.0))
                    // ── Left: portfolio list ──────────────────────────────
                    .child(
                        div()
                            .w(px(220.0))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .border_r_1()
                            .border_color(theme::fg_faint())
                            // Virtual buckets pinned to the top: give
                            // homeless forecasts (shared with me,
                            // unassigned/drafts) a discoverable UX
                            // surface. Rendered even when the
                            // `portfolios` list is empty so a fresh
                            // user still has a landing spot for their
                            // saved-but-not-yet-organised work.
                            .child(self.render_virtual_portfolio_row(
                                VirtualPortfolio::Live,
                                cx,
                            ))
                            .child(self.render_virtual_portfolio_row(
                                VirtualPortfolio::Drafts,
                                cx,
                            ))
                            .child(self.render_virtual_portfolio_row(
                                VirtualPortfolio::RecentlyResolved,
                                cx,
                            ))
                            .child(self.render_virtual_portfolio_row(
                                VirtualPortfolio::SharedWithMe,
                                cx,
                            ))
                            .child(self.render_virtual_portfolio_row(
                                VirtualPortfolio::Unassigned,
                                cx,
                            ))
                            .when(self.portfolios.is_empty(), |el| {
                                el.child(
                                    div()
                                        .px(px(14.0))
                                        .py(px(12.0))
                                        .text_size(px(11.0))
                                        .text_color(theme::fg_faint())
                                        .child("No named portfolios yet."),
                                )
                            })
                            .children(self.portfolios.iter().map(|p| {
                                let is_selected = selected.as_deref() == Some(p.id.as_str());
                                let is_rename = self.portfolio_rename_id.as_deref() == Some(p.id.as_str());
                                let is_confirm_delete = self.portfolio_confirm_delete_id.as_deref() == Some(p.id.as_str());
                                let pid = p.id.clone();
                                let pid2 = p.id.clone();
                                let pid3 = p.id.clone();
                                let pid4 = p.id.clone();
                                let count = p.forecast_count.unwrap_or(0);
                                let title = p.title.clone();

                                div()
                                    .flex()
                                    .flex_col()
                                    .border_b_1()
                                    .border_color(theme::fg_faint())
                                    .when(is_selected, |el| el.bg(theme::bg_hover()))
                                    // Rename row
                                    .when(is_rename, |el| {
                                        el.child(
                                            div()
                                                .px(px(10.0))
                                                .py(px(6.0))
                                                .flex()
                                                .items_center()
                                                .gap(px(6.0))
                                                .child(div().flex_grow().child(self.portfolio_rename_input.clone()))
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!("rename-ok-{}", pid4)))
                                                        .px(px(8.0))
                                                        .py(px(3.0))
                                                        .rounded(px(3.0))
                                                        .bg(rgb(theme::CYAN))
                                                        .text_size(px(10.0))
                                                        .text_color(rgb(theme::BG))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .cursor_pointer()
                                                        .hover(|s| s.opacity(0.8))
                                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                                            let new_title = this.portfolio_rename_input.read(cx).text().trim().to_string();
                                                            if !new_title.is_empty() {
                                                                this.rename_portfolio(pid4.clone(), new_title, cx);
                                                            } else {
                                                                this.portfolio_rename_id = None;
                                                                cx.notify();
                                                            }
                                                        }))
                                                        .child("✓"),
                                                )
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!("rename-cancel-{}", pid3)))
                                                        .px(px(6.0))
                                                        .py(px(3.0))
                                                        .text_size(px(10.0))
                                                        .text_color(theme::fg_dim())
                                                        .cursor_pointer()
                                                        .hover(|s| s.bg(theme::bg_hover()))
                                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                                            this.portfolio_rename_id = None;
                                                            cx.notify();
                                                        }))
                                                        .child("✕"),
                                                ),
                                        )
                                    })
                                    // Confirm-delete row
                                    .when(is_confirm_delete && !is_rename, |el| {
                                        el.child(
                                            div()
                                                .px(px(12.0))
                                                .py(px(8.0))
                                                .flex()
                                                .flex_col()
                                                .gap(px(4.0))
                                                .child(
                                                    div()
                                                        .text_size(px(10.0))
                                                        .text_color(theme::red())
                                                        .child("Delete this portfolio?"),
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(6.0))
                                                        .child(
                                                            div()
                                                                .id(SharedString::from(format!("del-confirm-{}", pid2)))
                                                                .px(px(8.0))
                                                                .py(px(3.0))
                                                                .rounded(px(3.0))
                                                                .bg(theme::red())
                                                                .text_size(px(10.0))
                                                                .text_color(rgb(theme::BG))
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .cursor_pointer()
                                                                .hover(|s| s.opacity(0.8))
                                                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                                                    this.delete_portfolio(pid2.clone(), cx);
                                                                }))
                                                                .child("Delete"),
                                                        )
                                                        .child(
                                                            div()
                                                                .id(SharedString::from(format!("del-cancel-{}", pid)))
                                                                .px(px(8.0))
                                                                .py(px(3.0))
                                                                .text_size(px(10.0))
                                                                .text_color(theme::fg_dim())
                                                                .cursor_pointer()
                                                                .hover(|s| s.bg(theme::bg_hover()))
                                                                .on_click(cx.listener(|this, _event, _window, cx| {
                                                                    this.portfolio_confirm_delete_id = None;
                                                                    cx.notify();
                                                                }))
                                                                .child("Cancel"),
                                                        ),
                                                ),
                                        )
                                    })
                                    // Normal portfolio card row
                                    .when(!is_rename && !is_confirm_delete, |el| {
                                        let pid_sel = p.id.clone();
                                        let pid_ren = p.id.clone();
                                        let pid_del = p.id.clone();
                                        let title_ren = title.clone();
                                        el.child(
                                            div()
                                                .id(SharedString::from(format!("portfolio-card-{}", pid_sel)))
                                                .px(px(12.0))
                                                .py(px(8.0))
                                                .flex()
                                                .items_center()
                                                .gap(px(6.0))
                                                .cursor_pointer()
                                                .hover(|s| s.bg(theme::bg_hover()))
                                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                                    if this.selected_portfolio_id.as_deref() == Some(&pid_sel) {
                                                        this.selected_portfolio_id = None;
                                                    } else {
                                                        this.selected_portfolio_id = Some(pid_sel.clone());
                                                        // Selecting a named portfolio clears any
                                                        // active virtual bucket so the right pane
                                                        // isn't rendering two headers at once.
                                                        this.selected_virtual_portfolio = None;
                                                        this.fetch_portfolio_forecasts(pid_sel.clone(), cx);
                                                        this.fetch_portfolio_stats_if_needed(pid_sel.clone(), cx);
                                                    }
                                                    this.portfolio_confirm_delete_id = None;
                                                    this.portfolio_rename_id = None;
                                                    cx.notify();
                                                }))
                                                // Portfolio icon
                                                .child(
                                                    div()
                                                        .text_size(px(11.0))
                                                        .text_color(rgb(theme::BLUE))
                                                        .child("◈"),
                                                )
                                                // Title + count
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .flex_grow()
                                                        .overflow_hidden()
                                                        .child(
                                                            div()
                                                                .text_size(px(12.0))
                                                                .text_color(theme::fg())
                                                                .font_weight(FontWeight::SEMIBOLD)
                                                                .overflow_hidden()
                                                                .child(truncate(&title, 22)),
                                                        )
                                                        .child(
                                                            div()
                                                                .text_size(px(10.0))
                                                                .text_color(theme::fg_faint())
                                                                .child(format!("{} forecast{}", count, if count == 1 { "" } else { "s" })),
                                                        ),
                                                )
                                                // Action icons (pencil + trash)
                                                .child(
                                                    div()
                                                        .flex()
                                                        .items_center()
                                                        .gap(px(4.0))
                                                        .child(
                                                            div()
                                                                .id(SharedString::from(format!("portfolio-rename-btn-{}", pid_ren)))
                                                                .text_size(px(10.0))
                                                                .text_color(theme::fg_faint())
                                                                .cursor_pointer()
                                                                .hover(|s| s.text_color(rgb(theme::BLUE)))
                                                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                                                    this.portfolio_rename_id = Some(pid_ren.clone());
                                                                    let title_clone = title_ren.clone();
                                                                    this.portfolio_rename_input.update(cx, |input, cx| {
                                                                        input.set_text(title_clone, cx);
                                                                    });
                                                                    cx.notify();
                                                                }))
                                                                .child("✎"),
                                                        )
                                                        .child(
                                                            div()
                                                                .id(SharedString::from(format!("portfolio-delete-btn-{}", pid_del)))
                                                                .text_size(px(10.0))
                                                                .text_color(theme::fg_faint())
                                                                .cursor_pointer()
                                                                .hover(|s| s.text_color(theme::red()))
                                                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                                                    this.portfolio_confirm_delete_id = Some(pid_del.clone());
                                                                    this.portfolio_rename_id = None;
                                                                    cx.notify();
                                                                }))
                                                                .child("⌫"),
                                                        ),
                                                ),
                                        )
                                    })
                            })),
                    )
                    // ── Right: selected portfolio detail ──────────
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_grow()
                            // Virtual bucket path: rendered instead of the
                            // named-portfolio detail. `select_virtual_portfolio`
                            // clears `selected_portfolio_id` so these branches
                            // are mutually exclusive.
                            .when(self.selected_virtual_portfolio.is_some(), |el| {
                                let bucket = self.selected_virtual_portfolio.unwrap();
                                el.child(self.render_virtual_portfolio_detail(bucket, cx))
                            })
                            .when(selected.is_none() && self.selected_virtual_portfolio.is_none(), |el| {
                                el.child(
                                    div()
                                        .flex()
                                        .flex_grow()
                                        .items_center()
                                        .justify_center()
                                        .p(px(24.0))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(theme::fg_faint())
                                                .child("Select a portfolio to view its forecasts"),
                                        ),
                                )
                            })
                            .when(selected.is_some(), |el| {
                                let pid = selected.clone().unwrap_or_default();
                                let is_loading = self.portfolio_forecasts_loading.contains(&pid);
                                let forecasts = self.portfolio_forecasts.get(&pid).cloned().unwrap_or_default();
                                let portfolio_title = self.portfolios.iter()
                                    .find(|p| p.id == pid)
                                    .map(|p| p.title.clone())
                                    .unwrap_or_default();
                                let avg_brier = self.portfolios.iter()
                                    .find(|p| p.id == pid)
                                    .and_then(|p| p.avg_brier);

                                el
                                    // Portfolio detail header
                                    .child(
                                        div()
                                            .px(px(14.0))
                                            .py(px(8.0))
                                            .border_b_1()
                                            .border_color(theme::fg_faint())
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .text_color(theme::fg())
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(portfolio_title),
                                            )
                                            .when(avg_brier.is_some(), move |el| {
                                                el.child(
                                                    div()
                                                        .px(px(6.0))
                                                        .py(px(2.0))
                                                        .rounded(px(4.0))
                                                        .bg(theme::bg_hover())
                                                        .text_size(px(10.0))
                                                        .text_color(rgb(theme::CYAN))
                                                        .child(format!("avg Brier {:.3}", avg_brier.unwrap())),
                                                )
                                            })
                                            // Push the Share chip to the right edge.
                                            .child(div().flex_grow())
                                            // 🔗 Share toggle (Spec 24 §3.5.3)
                                            .child(
                                                div()
                                                    .id("portfolio-share-btn")
                                                    .px(px(10.0))
                                                    .py(px(3.0))
                                                    .rounded(px(4.0))
                                                    .border_1()
                                                    .border_color(if self.portfolio_share_showing {
                                                        rgb(theme::CYAN)
                                                    } else {
                                                        rgb(theme::FG_FAINT)
                                                    })
                                                    .text_size(px(11.0))
                                                    .text_color(if self.portfolio_share_showing {
                                                        rgb(theme::CYAN)
                                                    } else {
                                                        rgb(theme::FG_DIM)
                                                    })
                                                    .cursor_pointer()
                                                    .hover(|s| s.bg(theme::bg_hover()))
                                                    .on_click(cx.listener(|this, _, _w, cx| {
                                                        this.toggle_portfolio_share(cx);
                                                    }))
                                                    .child("🔗 Share"),
                                            ),
                                    )
                                    // Access panel (collapsible)
                                    .when(self.portfolio_share_showing, |el| {
                                        el.child(self.render_portfolio_access_panel(cx))
                                    })
                                    // Stats + calibration curve (when stats fetched)
                                    .when(self.portfolio_stats_cache.contains_key(&pid), |el| {
                                        let stats = self.portfolio_stats_cache.get(&pid).unwrap().clone();
                                        el.child(render_portfolio_stats_panel(&stats))
                                    })
                                    // ---- Portfolio HUD (Stage 3) ----
                                    // Six live KPI tiles derived from the currently
                                    // loaded portfolio_forecasts. Rendered unconditionally
                                    // once the list is loaded so the operator sees the
                                    // book at a glance even before opening any row.
                                    .when(!forecasts.is_empty(), {
                                        let forecasts_for_hud = forecasts.clone();
                                        move |el| el.child(render_portfolio_hud(&forecasts_for_hud))
                                    })
                                    // ---- Rollup strip: BIGGEST EDGES (Stage 3) ----
                                    // Top six active forecasts by |Delta-vs-crowd|, each
                                    // rendered as a one-line mini-worm with a probability
                                    // bar + crowd tick + divergence chip.
                                    .when(!forecasts.is_empty(), {
                                        let forecasts_for_rollup = forecasts.clone();
                                        move |el| el.child(render_portfolio_rollup_strip(&forecasts_for_rollup))
                                    })
                                    // Sprint B: Portfolio Risk view. Six
                                    // metrics inline plus a correlation
                                    // slider that recomputes P(any yes).
                                    // Uses `WeakEntity` so the click
                                    // handler on rho chips can update the
                                    // console without borrow gymnastics.
                                    .when(!forecasts.is_empty(), {
                                        let forecasts_for_risk = forecasts.clone();
                                        let rho = self.portfolio_risk_rho;
                                        let handle = cx.weak_entity();
                                        move |el| {
                                            el.child(render_portfolio_risk_view(
                                                &forecasts_for_risk,
                                                rho,
                                                move |new_rho, _w, app_cx| {
                                                    if let Some(this) = handle.upgrade() {
                                                        this.update(app_cx, |this, cx| {
                                                            this.portfolio_risk_rho = new_rho;
                                                            cx.notify();
                                                        });
                                                    }
                                                },
                                            ))
                                        }
                                    })
                                    // Sprint B: Relationships / cascades
                                    // sub-panel. Collapsible; declare +
                                    // remove per-relationship inline.
                                    .child(self.render_relationships_panel(&forecasts, cx))
                                    // Loading spinner
                                    .when(is_loading, |el| {
                                        el.child(
                                            div()
                                                .p(px(14.0))
                                                .text_size(px(11.0))
                                                .text_color(theme::fg_faint())
                                                .child("Loading forecasts…"),
                                        )
                                    })
                                    // Empty state
                                    .when(!is_loading && forecasts.is_empty(), |el| {
                                        el.child(
                                            div()
                                                .p(px(14.0))
                                                .text_size(px(11.0))
                                                .text_color(theme::fg_faint())
                                                .child("No forecasts in this portfolio yet."),
                                        )
                                    })
                                    // Search + sort toolbar + enriched rows
                                    .when(!is_loading && !forecasts.is_empty(), move |el| {
                                        // Read live from the input entity. We don't keep
                                        // portfolio_filter_text in sync via on_change because
                                        // the entity already owns the source-of-truth string —
                                        // pulling it at render time keeps wiring minimal and
                                        // avoids a callback that would need a weak handle to
                                        // FermiConsole anyway.
                                        let filter_text = self
                                            .portfolio_filter_input
                                            .read(cx)
                                            .text()
                                            .to_string();
                                        let active_sort = self.portfolio_sort_mode;

                                        // Filter by free-text (question + tags).
                                        let lc_filter = filter_text.to_lowercase();
                                        // Snapshot the quick-filter set so the sort closure
                                        // below doesn't borrow &self across the move boundary.
                                        let quick_filters = self.portfolio_quick_filters.clone();
                                        let mut filtered: Vec<PortfolioForecast> = forecasts
                                            .into_iter()
                                            .filter(|f| {
                                                if !lc_filter.is_empty() {
                                                    let q_match = f.question_text.to_lowercase().contains(&lc_filter);
                                                    let tag_match = f
                                                        .tags
                                                        .as_ref()
                                                        .map(|t| t.iter().any(|tag| tag.to_lowercase().contains(&lc_filter)))
                                                        .unwrap_or(false);
                                                    if !(q_match || tag_match) { return false; }
                                                }
                                                // Quick-filter chips: AND across all active
                                                // chips (an operator with both `hot` and `linked`
                                                // selected wants the intersection, not the union).
                                                for chip in quick_filters.iter() {
                                                    let keep = match chip.as_str() {
                                                        "active" => f.status == "active",
                                                        "resolved" => f.status == "resolved",
                                                        "hot" => f.n_recent_updates.unwrap_or(0) > 0,
                                                        "linked" => f.pm_market_price.is_some(),
                                                        "edge" => f.pm_divergence_pp.map(|d| d.abs() >= 5.0).unwrap_or(false),
                                                        "shared" => {
                                                            f.team_id.as_deref().map(|s| !s.is_empty()).unwrap_or(false)
                                                                || f.share_count.unwrap_or(0) > 0
                                                                || f.visibility.as_deref() == Some("public")
                                                        }
                                                        _ => true,
                                                    };
                                                    if !keep { return false; }
                                                }
                                                true
                                            })
                                            .collect();

                                        // Sort by the active mode. Ties broken by question_text so
                                        // the order is stable across renders.
                                        filtered.sort_by(|a, b| {
                                            use std::cmp::Ordering::*;
                                            let cmp = match active_sort {
                                                PortfolioSortMode::RecentActivity => {
                                                    // Lex compare on RFC3339 timestamps works since
                                                    // they're zero-padded. Reverse for desc.
                                                    b.updated_at.cmp(&a.updated_at)
                                                }
                                                PortfolioSortMode::BiggestPmDelta => {
                                                    let av = a.pm_divergence_pp.map(f64::abs).unwrap_or(-1.0);
                                                    let bv = b.pm_divergence_pp.map(f64::abs).unwrap_or(-1.0);
                                                    bv.partial_cmp(&av).unwrap_or(Equal)
                                                }
                                                PortfolioSortMode::BiggestMovement => {
                                                    let av = a.n_recent_updates.unwrap_or(0);
                                                    let bv = b.n_recent_updates.unwrap_or(0);
                                                    bv.cmp(&av)
                                                }
                                                PortfolioSortMode::HighestProb => {
                                                    let av = a.predicted_probability.unwrap_or(0.0);
                                                    let bv = b.predicted_probability.unwrap_or(0.0);
                                                    bv.partial_cmp(&av).unwrap_or(Equal)
                                                }
                                                PortfolioSortMode::Alphabetical => {
                                                    a.question_text.cmp(&b.question_text)
                                                }
                                            };
                                            if cmp != Equal { cmp } else { a.question_text.cmp(&b.question_text) }
                                        });

                                        let shown_count = filtered.len();
                                        let total_count_for_summary = self
                                            .portfolio_forecasts
                                            .get(&pid)
                                            .map(|v| v.len())
                                            .unwrap_or(0);
                                        let pid_for_filter = pid.clone();

                                        // Toolbar: filter input + sort buttons + count
                                        let mut toolbar = div()
                                            .px(px(14.0))
                                            .py(px(8.0))
                                            .border_b_1()
                                            .border_color(theme::fg_faint())
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(theme::fg_faint())
                                                    .child("🔍"),
                                            )
                                            .child(
                                                div().flex_grow().child(
                                                    self.portfolio_filter_input.clone(),
                                                ),
                                            );
                                        for mode in PortfolioSortMode::ALL.iter().copied() {
                                            let is_active = mode == active_sort;
                                            let pid_for_sort = pid_for_filter.clone();
                                            toolbar = toolbar.child(
                                                div()
                                                    .id(SharedString::from(format!(
                                                        "pf-sort-{:?}",
                                                        mode
                                                    )))
                                                    .px(px(8.0))
                                                    .py(px(2.0))
                                                    .rounded(px(4.0))
                                                    .border_1()
                                                    .border_color(
                                                        if is_active { theme::cyan() } else { theme::fg_faint() }
                                                    )
                                                    .text_size(px(10.0))
                                                    .text_color(
                                                        if is_active { theme::cyan() } else { theme::fg_dim() }
                                                    )
                                                    .text_size(px(10.0))
                                                    .text_color(
                                                        if is_active { theme::cyan() } else { theme::fg_dim() }
                                                    )
                                                    .cursor_pointer()
                                                    .hover(|s| s.bg(theme::bg_hover()))
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.portfolio_sort_mode = mode;
                                                        // Keep selected portfolio focused; just
                                                        // re-render with the new sort.
                                                        let _ = &pid_for_sort;
                                                        cx.notify();
                                                    }))
                                                    .child(mode.label()),
                                            );
                                        }
                                        toolbar = toolbar.child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(theme::fg_faint())
                                                .child(format!(
                                                    "{}/{}",
                                                    shown_count, total_count_for_summary
                                                )),
                                        );

                                        // Quick-filter chip row (Stage 3). One-click drilldowns
                                        // that toggle a predicate on the row set. Compose with
                                        // free-text and sort mode. Distinct visual style from
                                        // sort chips (rounded, filled when active) so the operator
                                        // reads them as "what am I looking at" rather than "how is
                                        // it ordered".
                                        let chip_defs: &[(&str, &str)] = &[
                                            ("active", "● active"),
                                            ("hot", "🔥 hot"),
                                            ("linked", "⛓ linked"),
                                            ("edge", "⚡ has edge"),
                                            ("shared", "👥 shared"),
                                            ("resolved", "✓ resolved"),
                                        ];
                                        let mut chip_row = div()
                                            .px(px(14.0))
                                            .py(px(6.0))
                                            .border_b_1()
                                            .border_color(theme::fg_faint())
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(
                                                div()
                                                    .text_size(px(9.0))
                                                    .text_color(theme::fg_faint())
                                                    .child("FILTER:"),
                                            );
                                        for (key, label) in chip_defs {
                                            let is_on = self.portfolio_quick_filters.contains(*key);
                                            let key_owned = (*key).to_string();
                                            chip_row = chip_row.child(
                                                div()
                                                    .id(SharedString::from(format!("pf-chip-{}", key)))
                                                    .px(px(8.0))
                                                    .py(px(2.0))
                                                    .rounded(px(10.0))
                                                    .border_1()
                                                    .border_color(if is_on { theme::cyan() } else { theme::fg_faint() })
                                                    .bg(if is_on { theme::bg_active() } else { theme::bg_elevated() })
                                                    .text_size(px(10.0))
                                                    .text_color(if is_on { theme::cyan() } else { theme::fg_dim() })
                                                    .cursor_pointer()
                                                    .hover(|s| s.bg(theme::bg_hover()))
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        if this.portfolio_quick_filters.contains(&key_owned) {
                                                            this.portfolio_quick_filters.remove(&key_owned);
                                                        } else {
                                                            this.portfolio_quick_filters.insert(key_owned.clone());
                                                        }
                                                        cx.notify();
                                                    }))
                                                    .child(label.to_string()),
                                            );
                                        }
                                        // Clear-all affordance — only rendered when at least
                                        // one chip is active, so it doesn't compete for
                                        // attention when the row is quiet.
                                        if !self.portfolio_quick_filters.is_empty() {
                                            chip_row = chip_row.child(
                                                div()
                                                    .id(SharedString::from("pf-chip-clear"))
                                                    .px(px(6.0))
                                                    .py(px(2.0))
                                                    .text_size(px(9.0))
                                                    .text_color(theme::fg_dim())
                                                    .cursor_pointer()
                                                    .hover(|s| s.text_color(theme::red()))
                                                    .on_click(cx.listener(|this, _, _, cx| {
                                                        this.portfolio_quick_filters.clear();
                                                        cx.notify();
                                                    }))
                                                    .child("clear"),
                                            );
                                        }

                                        let el = el.child(toolbar).child(chip_row);

                                        // Stage 3 row rendering: compact row with a
                                        // space-time-mini-worm + chips, plus an
                                        // optional expanded drill-down when the
                                        // row is in `portfolio_expanded_rows`.
                                        el.children(filtered.into_iter().map(|f| {
                                            self.render_constellation_row(&f, &pid, cx)
                                        }))
                                    })
                            }),
                    ),
            )
    }

    /// Render one row in the portfolio Constellation table (Stage 3).
    ///
    /// Two layers:
    ///   * Compact row — always visible. Chevron + title + mini-worm +
    ///     chip stack (probability, crowd, divergence, activity, status,
    ///     sharing badge). Click the chevron to toggle drill-down;
    ///     clicking anywhere else opens the forecast in the cockpit.
    ///   * Drill-down panel — only visible when `fid` is in
    ///     `portfolio_expanded_rows`. Shows the full question text,
    ///     tag chips, Brier + resolution note (if resolved), and
    ///     explicit action buttons (Open in cockpit, Refresh, Remove).
    fn render_constellation_row(
        &self,
        f: &PortfolioForecast,
        pid: &str,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let fid = f.id.clone();
        let pid_owned = pid.to_string();
        let is_expanded = self.portfolio_expanded_rows.contains(&fid);

        let prob_val = f.predicted_probability.unwrap_or(0.0);
        let prob_pct = (prob_val * 100.0).round() as u32;
        let prob_color = if prob_pct >= 70 {
            theme::CYAN
        } else if prob_pct >= 40 {
            theme::BLUE
        } else {
            theme::FG_DIM
        };
        let status_color = match f.status.as_str() {
            "active" => theme::CYAN,
            "resolved" => theme::GREEN,
            _ => theme::FG_DIM,
        };

        let recent_str = f
            .updated_at
            .as_deref()
            .map(|t| format_relative_time(t))
            .unwrap_or_else(|| "—".into());

        // Δ vs crowd chip — color-graded so an operator scanning the
        // list can spot the big edges without reading numbers.
        let delta_str = f.pm_divergence_pp.map(|d| {
            let sign = if d >= 0.0 { "+" } else { "" };
            format!("{}{:.1}pp", sign, d)
        });
        let delta_color: gpui::Hsla = match f.pm_divergence_pp {
            Some(d) if d.abs() >= 10.0 => theme::gold(),
            Some(d) if d.abs() >= 3.0 => theme::cyan(),
            Some(_) => theme::fg_dim(),
            None => theme::fg_faint(),
        };
        let movement_n = f.n_recent_updates.unwrap_or(0);

        // Sharing badge (Spec 24 §3.5.6): public > team > shared > private.
        let pf_vis = f.visibility.as_deref().unwrap_or("private");
        let pf_has_team = f.team_id.as_deref().map(|s| !s.is_empty()).unwrap_or(false);
        let pf_shares = f.share_count.unwrap_or(0);
        let share_badge: Option<(&'static str, gpui::Hsla)> = if pf_vis == "public" {
            Some(("🌐", theme::cyan()))
        } else if pf_has_team {
            Some(("👥", theme::blue()))
        } else if pf_shares > 0 {
            Some(("🔗", theme::gold()))
        } else {
            None
        };

        let brier_str = f.brier_score.map(|b| format!("{:.3}", b));

        // ── Compact row ──────────────────────────────────────────────────
        let fid_chevron = fid.clone();
        let fid_click = fid.clone();
        let chevron_glyph = if is_expanded { "▾" } else { "▸" };

        let mut compact = div()
            .id(SharedString::from(format!("pf-row-{}", fid)))
            .px(px(10.0))
            .py(px(7.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .hover(|s| s.bg(theme::bg_hover()))
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.open_forecast(&fid_click, cx);
            }))
            // Chevron — own click handler, doesn't propagate to row.
            .child(
                div()
                    .id(SharedString::from(format!("pf-chev-{}", fid)))
                    .w(px(16.0))
                    .text_size(px(10.0))
                    .text_color(if is_expanded {
                        theme::cyan()
                    } else {
                        theme::fg_dim()
                    })
                    .cursor_pointer()
                    .hover(|s| s.text_color(theme::cyan()))
                    .on_click(cx.listener(move |this, _ev, _w, cx| {
                        // GPUI's on_click delivers a `ClickEvent`, but
                        // click events do not bubble through the parent
                        // row's own on_click here — they're routed by
                        // ID. So we just toggle; no stop_propagation needed.
                        if this.portfolio_expanded_rows.contains(&fid_chevron) {
                            this.portfolio_expanded_rows.remove(&fid_chevron);
                        } else {
                            this.portfolio_expanded_rows.insert(fid_chevron.clone());
                        }
                        cx.notify();
                    }))
                    .child(chevron_glyph),
            )
            // Title — truncated for scan-ability.
            .child(
                div()
                    .w(px(280.0))
                    .overflow_hidden()
                    .text_size(px(11.0))
                    .text_color(theme::fg())
                    .child(truncate(&f.question_text, 44)),
            )
            // Mini space-time worm — the visual heart of the row.
            // Same grammar as the trajectory worm: cyan bar = model,
            // purple tick = crowd, gold tick = base rate (elided here
            // since PortfolioForecast doesn't carry it; keep the visual
            // grammar consistent so operators recognise the widget).
            .child(render_mini_worm(prob_val, f.pm_market_price, None, 72.0))
            // Probability numeric.
            .child(
                div()
                    .w(px(38.0))
                    .text_size(px(10.0))
                    .text_color(rgb(prob_color))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("{}%", prob_pct)),
            )
            // Δ vs crowd chip — anchored to a fixed width so rows align.
            .child({
                let mut chip = div()
                    .w(px(58.0))
                    .text_size(px(10.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(delta_color);
                if let Some(s) = delta_str.clone() {
                    chip = chip.child(s);
                } else {
                    chip = chip.child(div().text_color(theme::fg_faint()).child("no crowd"));
                }
                chip
            })
            // Activity chip — present only when the forecast moved.
            .child({
                let mut cell = div().w(px(48.0)).text_size(px(10.0));
                if movement_n > 0 {
                    cell = cell
                        .text_color(rgb(theme::BLUE))
                        .child(format!("↑ {}× 7d", movement_n));
                } else {
                    cell = cell.text_color(theme::fg_faint()).child("quiet");
                }
                cell
            })
            // Recent activity (relative time).
            .child(
                div()
                    .w(px(42.0))
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .child(recent_str),
            )
            // Sharing badge — fixed slot so rows align even when absent.
            .child({
                let mut cell = div().w(px(16.0)).text_size(px(10.0));
                if let Some((icon, color)) = share_badge {
                    cell = cell.text_color(color).child(icon);
                }
                cell
            })
            // Status.
            .child(
                div()
                    .w(px(52.0))
                    .text_size(px(10.0))
                    .text_color(rgb(status_color))
                    .child(f.status.clone()),
            );

        // Brier only when resolved.
        if let Some(bs) = brier_str.clone() {
            compact = compact.child(
                div()
                    .w(px(48.0))
                    .text_size(px(10.0))
                    .text_color(theme::fg_dim())
                    .child(format!("B {}", bs)),
            );
        }

        // ── Drill-down panel ────────────────────────────────────────────────
        let mut drill: Option<gpui::AnyElement> = None;
        if is_expanded {
            let fid_open = fid.clone();
            let fid_remove = fid.clone();
            let pid_remove = pid_owned.clone();
            let tags = f.tags.clone().unwrap_or_default();
            let resolution_note: Option<String> = None; // Not carried on
                                                        // PortfolioForecast; drill-down for
                                                        // resolution notes would need a
                                                        // /forecast/:id fetch. Reserved.

            let mut panel = div()
                .px(px(38.0)) // indent past the chevron + title column
                .py(px(8.0))
                .bg(theme::bg_active())
                .border_l_2()
                .border_color(rgb(theme::CYAN))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme::fg())
                        .child(f.question_text.clone()),
                );

            // Tag chips.
            if !tags.is_empty() {
                let mut tag_row = div().flex().flex_wrap().gap(px(4.0)).child(
                    div()
                        .text_size(px(9.0))
                        .text_color(theme::fg_faint())
                        .child("TAGS:"),
                );
                for t in tags {
                    tag_row = tag_row.child(
                        div()
                            .px(px(6.0))
                            .py(px(1.0))
                            .rounded(px(3.0))
                            .bg(theme::bg_elevated())
                            .text_size(px(9.0))
                            .text_color(theme::fg_dim())
                            .child(t),
                    );
                }
                panel = panel.child(tag_row);
            }

            // Metrics summary row inside the drill-down.
            let mut metrics = div()
                .flex()
                .flex_wrap()
                .gap(px(14.0))
                .child(render_detail_kv(
                    "Model",
                    &format!("{:.1}%", prob_val * 100.0),
                ));
            if let Some(c) = f.pm_market_price {
                metrics = metrics.child(render_detail_kv("Crowd", &format!("{:.1}%", c * 100.0)));
            }
            if let Some(d) = f.pm_divergence_pp {
                metrics = metrics.child(render_detail_kv(
                    "Δ",
                    &format!("{}{:.1}pp", if d >= 0.0 { "+" } else { "" }, d),
                ));
            }
            if let Some(bs) = brier_str.clone() {
                metrics = metrics.child(render_detail_kv("Brier", &bs));
            }
            if let Some(outcome) = f.actual_outcome {
                metrics = metrics.child(render_detail_kv(
                    "Resolved",
                    if outcome { "Yes" } else { "No" },
                ));
            }
            if movement_n > 0 {
                metrics = metrics.child(render_detail_kv("Updates 7d", &format!("{}", movement_n)));
            }
            if let Some(url) = f.pm_url.as_deref() {
                let url_str = url.to_string();
                metrics = metrics.child(
                    div()
                        .id(SharedString::from(format!("pf-pm-open-{}", fid)))
                        .text_size(px(10.0))
                        .text_color(theme::purple())
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme::cyan()))
                        .on_click(cx.listener(move |_this, _, _, _cx| {
                            let _ = open::that(&url_str);
                        }))
                        .child("Open Polymarket ↗"),
                );
            }
            panel = panel.child(metrics);

            if let Some(rn) = resolution_note {
                panel = panel.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::fg_dim())
                        .child(rn),
                );
            }

            // Action row — explicit buttons for the operations the
            // compact row's whole-row click can't disambiguate.
            panel = panel.child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("pf-open-{}", fid)))
                            .px(px(10.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(rgb(theme::CYAN))
                            .text_size(px(10.0))
                            .text_color(rgb(theme::CYAN))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_forecast(&fid_open, cx);
                            }))
                            .child("Open in cockpit →"),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("pf-remove-{}", fid)))
                            .px(px(10.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(theme::fg_faint())
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()).text_color(theme::red()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.remove_from_portfolio(
                                    pid_remove.clone(),
                                    fid_remove.clone(),
                                    cx,
                                );
                            }))
                            .child("Remove from portfolio"),
                    ),
            );

            drill = Some(panel.into_any_element());
        }

        // Container: row + optional drill-down, with a bottom border so
        // rows visually separate whether or not one is expanded.
        div()
            .border_b_1()
            .border_color(theme::fg_faint())
            .child(compact)
            .when_some(drill, |el, d| el.child(d))
    }

    fn render_forecast_portfolio_row(
        &self,
        forecast_id: &str,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let fid = forecast_id.to_string();

        // Split portfolios into two groups so the meaning of each chip
        // is unambiguous: memberships on one line, add-actions on
        // another. Previously they were interleaved in a single row
        // labelled 'Add to portfolio:' — which read as if the '✓ WC
        // sims' chip was also an action, and as if the '+ company
        // performance' chip on a WC forecast was an editorial
        // recommendation instead of a raw add-to affordance.
        let (member_of, addable): (Vec<_>, Vec<_>) = self.portfolios.iter().partition(|p| {
            self.portfolio_forecasts
                .get(&p.id)
                .map(|fs| fs.iter().any(|f| f.id == fid))
                .unwrap_or(false)
        });

        let container = div()
            .px(px(24.0))
            .py(px(8.0))
            .border_t_1()
            .border_color(theme::fg_faint())
            .flex()
            .flex_col()
            .gap(px(6.0));

        if self.portfolios.is_empty() {
            return container
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::fg_faint())
                        .child("Create a portfolio to organise this forecast."),
                )
                .into_any_element();
        }

        container
            // Row 1 — memberships. Shows only if the forecast is in at
            // least one portfolio; the chip is read-only (no click).
            .when(!member_of.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme::fg_faint())
                                .child("In portfolios:"),
                        )
                        .children(member_of.into_iter().map(|p| {
                            let label = truncate(&p.title, 22);
                            div()
                                .id(SharedString::from(format!("in-{}-{}", p.id, fid)))
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded(px(4.0))
                                .bg(theme::bg_hover())
                                .text_size(px(10.0))
                                .text_color(theme::fg())
                                .child(format!("✓ {}", label))
                        })),
                )
            })
            // Row 2 — add-to affordances. Shows only for portfolios the
            // forecast is NOT yet in, so the chips are unambiguously
            // actionable.
            .when(!addable.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme::fg_faint())
                                .child("Add to:"),
                        )
                        .children(addable.into_iter().map(|p| {
                            let pid = p.id.clone();
                            let fid2 = fid.clone();
                            let label = truncate(&p.title, 22);
                            div()
                                .id(SharedString::from(format!("add-to-{}-{}", pid, fid)))
                                .px(px(8.0))
                                .py(px(3.0))
                                .rounded(px(4.0))
                                .border_1()
                                .border_color(rgb(theme::CYAN))
                                .text_size(px(10.0))
                                .text_color(rgb(theme::CYAN))
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::bg_hover()))
                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                    this.add_forecast_to_portfolio(fid2.clone(), pid.clone(), cx);
                                }))
                                .child(format!("+ {}", label))
                        })),
                )
            })
            .into_any_element()
    }

    fn render_portfolio(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .id("portfolio-scroll")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            .p(px(24.0))
            .gap(px(16.0))
            .child(
                // Header
                div().flex().items_center().justify_between().child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .text_size(px(22.0))
                                .text_color(theme::fg())
                                .font_weight(FontWeight::BOLD)
                                .child("Portfolio"),
                        )
                        // Import from Polymarket button
                        .when(self.connected, |el| {
                            el.child(
                                div()
                                    .id("pm-import-btn")
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .px(px(12.0))
                                    .py(px(5.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(0x1A1A2E))
                                    .border_1()
                                    .border_color(rgb(theme::PURPLE))
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::PURPLE))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(rgb(theme::BG_HOVER)).border_color(rgb(theme::CYAN))
                                    })
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.pm_show_search = !this.pm_show_search;
                                        cx.notify();
                                    }))
                                    .child("🔮 Import from Polymarket"),
                            )
                        })
                        // Check Resolutions button (flywheel trigger)
                        .when(self.connected, |el| {
                            let loading = self.pm_resolutions_loading;
                            let result = self.pm_resolutions_last_result.clone();
                            let label = if loading {
                                "⚡ Checking…".to_string()
                            } else if let Some(ref r) = result {
                                r.clone()
                            } else {
                                "⚡ Check Resolutions".to_string()
                            };
                            let accent = if result
                                .as_deref()
                                .map(|r| r.starts_with("✓"))
                                .unwrap_or(false)
                            {
                                theme::GREEN
                            } else {
                                theme::GOLD
                            };
                            el.child(
                                div()
                                    .id("pm-check-resolutions-btn")
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .px(px(12.0))
                                    .py(px(5.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(0x1A1A1A))
                                    .border_1()
                                    .border_color(rgb(accent))
                                    .text_size(px(11.0))
                                    .text_color(rgb(accent))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when(!loading, |s| s.cursor_pointer())
                                    .when(!loading, |s| s.hover(|s| s.bg(rgb(theme::BG_HOVER))))
                                    .on_click(cx.listener(|this, _event, _window, cx| {
                                        this.check_pm_resolutions(cx);
                                    }))
                                    .child(label),
                            )
                        }),
                ),
                // The old "N active · M resolved · K drafts" strip that
                // used to live on the right side of this header was
                // aggregating counts across every forecast the user
                // owned — which is misleading here: the Portfolio panel
                // is scoped to named portfolios, and each portfolio's
                // HUD already renders its own counts. Global live/draft
                // counts stay on the Dashboard.
            )
            // ── Polymarket Search Panel ──────────────────────────────
            // Extracted body lives in `render_pm_search_card`. Both
            // this Portfolio-panel entry point and the Dashboard hero
            // buttons render the same card so a UX/feature change
            // only has to be made in one place.
            .when(self.pm_show_search, |el| {
                el.child(self.render_pm_search_card(cx))
            })
            .when(self.forecasts_loading, |el| {
                el.child(
                    div()
                        .text_size(px(13.0))
                        .text_color(theme::fg_dim())
                        .child("Loading forecasts…"),
                )
            })
            .when(!self.connected, |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .flex_grow()
                        .gap(px(12.0))
                        .child(
                            div()
                                .text_size(px(16.0))
                                .text_color(theme::fg_dim())
                                .child("Connect to view your portfolio"),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme::fg_faint())
                                .child("Set FERMI_API_KEY environment variable"),
                        ),
                )
            })
            // ── Named Portfolios ──────────────────────────────
            //
            // The Portfolio panel is now scoped entirely to *named*
            // portfolios. The previous incarnation also stacked Drafts,
            // Resolved, Workspaces, and Lab sections underneath — four
            // orthogonal lists that duplicated the Dashboard's Live view
            // and the Composer's Lab pane, and buried the actual
            // portfolio content under a mile of scrolling. Those were
            // removed; the surviving section owns the whole panel below
            // the header.
            .when(self.connected, |el| {
                el.child(self.render_portfolios_section(cx))
            })
        // Historically this method continued with four more stacked
        // sections (Drafts / Resolved / Workspaces / Lab). Those
        // were removed once the panel was rescoped to *named*
        // portfolios: the Dashboard owns the Live (active) view,
        // each portfolio's HUD owns its per-portfolio counts, the
        // Composer owns Lab, and Workspaces have their own home.
        // The old block is gone — `git log` has it if we need it.
    }

    // ── Polymarket search card ──────────────────────────────────────────────
    //
    // Shared between:
    //   * Portfolio panel — rendered inline when the operator clicks
    //     "+ Import from Polymarket" in the portfolio header.
    //   * Dashboard — rendered as an in-place overlay when the hero
    //     buttons ("🔮 From Polymarket" / "📎 Paste PM URL") toggle
    //     `pm_show_search`. Testers no longer get punted to the
    //     Portfolio panel just to import a market.
    //
    // Gated on `self.pm_show_search`; callers control visibility.
    fn render_pm_search_card(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .px(px(16.0))
            .py(px(12.0))
            .rounded(px(8.0))
            .bg(rgb(0x1A1A2E))
            .border_1()
            .border_color(rgb(theme::PURPLE))
            // Header
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .text_color(rgb(theme::PURPLE))
                            .font_weight(FontWeight::BOLD)
                            .child("🔮 Browse Polymarket"),
                    )
                    .child(
                        div()
                            .id("pm-close-search")
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .px(px(8.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.pm_show_search = false;
                                cx.notify();
                            }))
                            .child("✕"),
                    ),
            )
            // Search input + button
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .items_end()
                    .child(div().flex_grow().child(self.pm_search_input.clone()))
                    .child(
                        div()
                            .id("pm-search-btn")
                            .px(px(14.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .bg(rgb(theme::PURPLE))
                            .text_color(rgb(theme::BG_DEEP))
                            .text_size(px(11.0))
                            .font_weight(FontWeight::BOLD)
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.85))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.search_polymarket(cx);
                            }))
                            .child(if self.pm_search_loading {
                                "Searching…"
                            } else {
                                "Search"
                            }),
                    ),
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(theme::fg_faint())
                    .child(
                    "Search active prediction markets. Select one to import as a Fermi forecast.",
                ),
            )
            // Loading indicator
            .when(self.pm_search_loading, |el| {
                el.child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(theme::PURPLE))
                        .child("⟳ Searching Polymarket…"),
                )
            })
            // Error
            .when(
                self.pm_search_error.is_some() && !self.pm_search_loading,
                |el| {
                    el.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(rgb(theme::RED))
                            .px(px(10.0))
                            .py(px(6.0))
                            .rounded(px(4.0))
                            .bg(rgb(0x2A1A1A))
                            .child(format!(
                                "⚠ {}",
                                self.pm_search_error.as_deref().unwrap_or("Unknown error")
                            )),
                    )
                },
            )
            // Results
            .when(!self.pm_search_results.is_empty(), |el| {
                el.child(
                    div()
                        .id("pm-results-scroll")
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .overflow_y_scroll()
                        .max_h(px(480.0))
                        .children(
                            self.pm_search_results
                                .iter()
                                .enumerate()
                                .map(|(i, result)| {
                                    let question_str = result
                                        .get("question")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("Unknown")
                                        .to_string();
                                    let question_display = question_str.clone();
                                    let event_title = result
                                        .get("event_title")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let price_pct = result
                                        .get("market_price_pct")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("?")
                                        .to_string();
                                    let vol_fmt = result
                                        .get("volume_24h_fmt")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let liq_fmt = result
                                        .get("liquidity_fmt")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let confidence = result
                                        .get("confidence_signal")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("Low")
                                        .to_string();
                                    let pm_event_id = result
                                        .get("pm_event_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let pm_market_id = result
                                        .get("pm_market_id")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                    let market_price = result
                                        .get("market_price")
                                        .and_then(|v| v.as_f64())
                                        .unwrap_or(0.0);
                                    let end_date = result
                                        .get("end_date")
                                        .and_then(|v| v.as_str())
                                        .map(|s| s[..10.min(s.len())].to_string())
                                        .unwrap_or_default();
                                    let change_1w =
                                        result.get("price_change_1w").and_then(|v| v.as_f64());
                                    let volume_24h_raw =
                                        result.get("volume_24h").and_then(|v| v.as_f64());
                                    let liquidity_raw =
                                        result.get("liquidity").and_then(|v| v.as_f64());

                                    let conf_color = match confidence.as_str() {
                                        "Very High" => theme::GREEN,
                                        "High" => theme::CYAN,
                                        "Medium" => theme::GOLD,
                                        _ => theme::FG_FAINT,
                                    };

                                    div()
                                        .id(ElementId::Name(format!("pm-result-{}", i).into()))
                                        .flex()
                                        .items_center()
                                        .gap(px(10.0))
                                        .px(px(10.0))
                                        .py(px(8.0))
                                        .rounded(px(6.0))
                                        .bg(rgb(theme::BG_ELEVATED))
                                        .border_1()
                                        .border_color(rgb(theme::FG_FAINT))
                                        .cursor_pointer()
                                        .hover(|s| {
                                            s.border_color(rgb(theme::PURPLE))
                                                .bg(rgb(theme::BG_HOVER))
                                        })
                                        .on_click({
                                            let confidence_import = confidence.clone();
                                            cx.listener(move |this, _event, _window, cx| {
                                                this.import_polymarket_forecast(
                                                    &pm_event_id,
                                                    &pm_market_id,
                                                    &question_str,
                                                    market_price,
                                                    volume_24h_raw,
                                                    liquidity_raw,
                                                    Some(confidence_import.clone()),
                                                    change_1w,
                                                    cx,
                                                );
                                            })
                                        })
                                        // Price
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .items_center()
                                                .w(px(60.0))
                                                .child(
                                                    div()
                                                        .text_size(px(18.0))
                                                        .text_color(rgb(theme::PURPLE))
                                                        .font_weight(FontWeight::BOLD)
                                                        .child(price_pct),
                                                )
                                                .when(change_1w.is_some(), |el| {
                                                    let c = change_1w.unwrap();
                                                    let (arrow, color) = if c > 0.005 {
                                                        ("↑", theme::GREEN)
                                                    } else if c < -0.005 {
                                                        ("↓", theme::RED)
                                                    } else {
                                                        ("→", theme::FG_DIM)
                                                    };
                                                    el.child(
                                                        div()
                                                            .text_size(px(8.0))
                                                            .text_color(rgb(color))
                                                            .child(format!(
                                                                "{}{:.1}pp",
                                                                arrow,
                                                                c * 100.0
                                                            )),
                                                    )
                                                }),
                                        )
                                        // Question + metadata
                                        .child(
                                            div()
                                                .flex_grow()
                                                .min_w(px(0.0))
                                                .flex()
                                                .flex_col()
                                                .gap(px(2.0))
                                                .child(
                                                    div()
                                                        .text_size(px(12.0))
                                                        .text_color(theme::fg())
                                                        .child(question_display.clone()),
                                                )
                                                .when(
                                                    !event_title.is_empty()
                                                        && event_title != question_display,
                                                    |el| {
                                                        el.child(
                                                            div()
                                                                .text_size(px(9.0))
                                                                .text_color(theme::fg_faint())
                                                                .child(event_title),
                                                        )
                                                    },
                                                )
                                                .child(
                                                    div()
                                                        .flex()
                                                        .gap(px(8.0))
                                                        .text_size(px(9.0))
                                                        .text_color(theme::fg_faint())
                                                        .when(!vol_fmt.is_empty(), |el| {
                                                            el.child(format!("{} vol", vol_fmt))
                                                        })
                                                        .when(!liq_fmt.is_empty(), |el| {
                                                            el.child(format!("{} liq", liq_fmt))
                                                        })
                                                        .child(
                                                            div()
                                                                .text_color(rgb(conf_color))
                                                                .child(confidence.clone()),
                                                        )
                                                        .when(!end_date.is_empty(), |el| {
                                                            el.child(format!("ends {}", end_date))
                                                        }),
                                                ),
                                        )
                                        // Import button
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(rgb(theme::PURPLE))
                                                .px(px(10.0))
                                                .py(px(4.0))
                                                .rounded(px(4.0))
                                                .bg(rgb(0x1A1A2E))
                                                .border_1()
                                                .border_color(rgb(theme::PURPLE))
                                                .child("Import →"),
                                        )
                                }),
                        ),
                )
            })
            // Empty state
            .when(
                !self.pm_search_loading
                    && self.pm_search_results.is_empty()
                    && !self.pm_show_search,
                |el| {
                    el.child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .child("Search for a Polymarket question to import into Fermi."),
                    )
                },
            )
    }

    fn render_forecast_section(
        &self,
        title: &str,
        subtitle: &str,
        forecasts: &[Forecast],
        accent: u32,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(theme::bg_elevated())
            .rounded(px(8.0))
            .border_1()
            .border_color(theme::fg_faint())
            .child(
                div()
                    .px(px(16.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(accent))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{} ({})", title, forecasts.len())),
                    )
                    .when(!subtitle.is_empty(), |el| {
                        el.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme::fg_faint())
                                .child(subtitle.to_string()),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .children(forecasts.iter().map(|f| self.render_forecast_row(f, cx))),
            )
    }

    /// Renders one entry in the Portfolio panel's virtual-bucket band
    /// (“📥 Shared with me” / “📌 Unassigned”). Styled to be
    /// visually distinct from named-portfolio cards (dashed border,
    /// gold accent) so operators don't mistake them for real portfolios
    /// they can rename or delete. Clicking selects/deselects the
    /// bucket via `select_virtual_portfolio`, which handles the fetch.
    fn render_virtual_portfolio_row(
        &self,
        bucket: VirtualPortfolio,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let is_selected = self.selected_virtual_portfolio == Some(bucket);
        let (icon, title, subtitle, count, loading, id_suffix) = match bucket {
            VirtualPortfolio::SharedWithMe => (
                "📥",
                "Shared with me",
                "Team + collaborator shares",
                self.shared_with_me_forecasts.len(),
                self.shared_with_me_loading,
                "shared-with-me",
            ),
            VirtualPortfolio::Unassigned => (
                "📌",
                "Unassigned",
                "Mine, no portfolio yet",
                self.unassigned_forecasts.len(),
                self.unassigned_loading,
                "unassigned",
            ),
            VirtualPortfolio::Live => (
                "◐",
                "Live",
                "Active · Brier-scored",
                self.active_forecasts.len(),
                self.forecasts_loading,
                "live",
            ),
            VirtualPortfolio::Drafts => (
                "✎",
                "Drafts",
                "Unpublished WIP",
                self.draft_forecasts.len(),
                self.forecasts_loading,
                "drafts",
            ),
            VirtualPortfolio::RecentlyResolved => (
                "✓",
                "Recently Resolved",
                "Last resolutions · Brier-scored",
                self.resolved_forecasts.len(),
                self.forecasts_loading,
                "recently-resolved",
            ),
        };

        div()
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(theme::fg_faint())
            .when(is_selected, |el| el.bg(theme::bg_hover()))
            .child(
                // `id` must live on the interactive element for gpui
                // to attach `on_click`; the outer div is a plain
                // layout container.
                div()
                    .id(SharedString::from(format!(
                        "virtual-portfolio-{}",
                        id_suffix
                    )))
                    .px(px(12.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::bg_hover()))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        this.select_virtual_portfolio(bucket, cx);
                    }))
                    // Distinct icon uses gold to signal "virtual"
                    // (vs the blue ◈ used for named portfolios).
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(rgb(theme::GOLD))
                            .child(icon),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_grow()
                            .overflow_hidden()
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme::fg())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .overflow_hidden()
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme::fg_faint())
                                    .child(if loading {
                                        "loading…".to_string()
                                    } else if count == 0 {
                                        subtitle.to_string()
                                    } else {
                                        format!(
                                            "{} forecast{}",
                                            count,
                                            if count == 1 { "" } else { "s" }
                                        )
                                    }),
                            ),
                    ),
            )
    }

    /// Renders the right-hand pane of the Portfolio panel when a
    /// virtual bucket is selected. Straight vertical list of forecast
    /// rows using the same `render_forecast_row` widget as the
    /// Dashboard, so click → detail → “→ Open in Cockpit” round-trips
    /// via `open_forecast` for free.
    fn render_virtual_portfolio_detail(
        &self,
        bucket: VirtualPortfolio,
        cx: &Context<Self>,
    ) -> AnyElement {
        // For RecentlyResolved we take a slice of the newest 20 so the
        // list doesn't grow unbounded for prolific forecasters. `Live`
        // and `Drafts` show every row — the operator asked for these
        // to become the canonical home of that data.
        let recently_resolved: Vec<Forecast> =
            self.resolved_forecasts.iter().take(20).cloned().collect();
        let (title, blurb, forecasts, loading): (&'static str, &'static str, &Vec<Forecast>, bool) =
            match bucket {
                VirtualPortfolio::SharedWithMe => (
                    "📥 Shared with me",
                    "Forecasts other people have shared with you — via team \
                 membership, direct share, or public visibility. Read-only \
                 unless the share grants edit/admin.",
                    &self.shared_with_me_forecasts,
                    self.shared_with_me_loading,
                ),
                VirtualPortfolio::Unassigned => (
                    "📌 Unassigned",
                    "Your forecasts that aren't in any portfolio yet. Drafts \
                 saved with Ctrl+S land here first — open one, click a \
                 portfolio chip in the composer, and it moves to that \
                 portfolio.",
                    &self.unassigned_forecasts,
                    self.unassigned_loading,
                ),
                VirtualPortfolio::Live => (
                    "◐ Live",
                    "Every active forecast you own, across every portfolio. \
                 Brier-scored, updating on schedule. The Dashboard used \
                 to show this section directly; it lives here now so the \
                 Dashboard can stay a command center.",
                    &self.active_forecasts,
                    self.forecasts_loading,
                ),
                VirtualPortfolio::Drafts => (
                    "✎ Drafts",
                    "Unpublished forecasts. Click any row to open it in \
                 the Composer and finish drafting. Publishing moves the \
                 row into Live.",
                    &self.draft_forecasts,
                    self.forecasts_loading,
                ),
                VirtualPortfolio::RecentlyResolved => (
                    "✓ Recently Resolved",
                    "Your last 20 resolved forecasts, newest first. Full \
                 resolution history is available through each portfolio.",
                    &recently_resolved,
                    self.forecasts_loading,
                ),
            };

        let count = forecasts.len();
        div()
            .flex()
            .flex_col()
            .flex_grow()
            .child(
                div()
                    .px(px(14.0))
                    .py(px(8.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(theme::fg())
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .px(px(6.0))
                                    .py(px(1.0))
                                    .rounded(px(4.0))
                                    .bg(theme::bg_hover())
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme::GOLD))
                                    .child(format!("{}", count)),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .child(blurb),
                    ),
            )
            .when(loading, |el| {
                el.child(
                    div()
                        .p(px(14.0))
                        .text_size(px(11.0))
                        .text_color(theme::fg_faint())
                        .child("Loading forecasts…"),
                )
            })
            .when(!loading && count == 0, |el| {
                el.child(
                    div()
                        .p(px(24.0))
                        .text_size(px(12.0))
                        .text_color(theme::fg_faint())
                        .child(match bucket {
                            VirtualPortfolio::SharedWithMe => {
                                "Nothing shared with you yet. When a teammate \
                                 shares a forecast or portfolio, it will show up here."
                            }
                            VirtualPortfolio::Unassigned => {
                                "No unassigned forecasts. Every forecast you own \
                                 is already in a portfolio — nice."
                            }
                            VirtualPortfolio::Live => {
                                "No live forecasts yet. Publish a draft (Ctrl+P) \
                                 and it will appear here."
                            }
                            VirtualPortfolio::Drafts => {
                                "No drafts. Start a new forecast from the \
                                 Dashboard hero or Ctrl+N."
                            }
                            VirtualPortfolio::RecentlyResolved => {
                                "No resolutions yet. Once a forecast reaches its \
                                 resolution date, it will appear here with its Brier score."
                            }
                        }),
                )
            })
            .when(!loading && count > 0, |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .children(forecasts.iter().map(|f| self.render_forecast_row(f, cx))),
                )
            })
            .into_any_element()
    }

    fn render_forecast_row(&self, forecast: &Forecast, cx: &Context<Self>) -> impl IntoElement {
        let status_color = match forecast.status.as_str() {
            "active" => theme::CYAN,
            "resolved" => theme::GREEN,
            "draft" => theme::FG_DIM,
            "voided" => theme::RED,
            _ => theme::FG_DIM,
        };

        let prob_text = format!("{:.0}%", forecast.predicted_probability * 100.0);
        let brier_text = forecast
            .brier_score
            .map(|b| format!("Brier {:.3}", b))
            .unwrap_or_default();

        let is_selected = self.selected_forecast_id.as_deref() == Some(&forecast.id);
        let fid_toggle = forecast.id.clone();
        // Full team-affiliation set for the multi-dot stack (owning
        // team + all team shares from the fan-out cache). When empty,
        // the strip renders nothing — no visual noise for unassigned
        // forecasts.
        let team_ids = self.team_ids_for_forecast(forecast);

        div()
            .id(SharedString::from(format!("forecast-{}", forecast.id)))
            .flex()
            .flex_col()
            .border_b_1()
            .border_color(theme::fg_faint())
            .when(is_selected, |el| el.bg(theme::bg_active()))
            // ── Summary row (click to toggle detail) ──────────────
            .child(
                div()
                    .id(SharedString::from(format!("forecast-row-{}", forecast.id)))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(16.0))
                    .py(px(10.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::bg_hover()))
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        if this.selected_forecast_id.as_deref() == Some(&fid_toggle) {
                            this.selected_forecast_id = None;
                        } else {
                            this.selected_forecast_id = Some(fid_toggle.clone());
                        }
                        cx.notify();
                    }))
                    .child(self.render_team_dots(&team_ids))
                    .child(
                        // Probability badge
                        div()
                            .w(px(48.0))
                            .text_size(px(14.0))
                            .text_color(rgb(status_color))
                            .font_weight(FontWeight::BOLD)
                            .child(prob_text),
                    )
                    .child(
                        // Question text
                        div()
                            .flex_grow()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(theme::fg())
                                    .child(truncate(&forecast.question_text, 60)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.0))
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_dim())
                                    .when(forecast.domain.is_some(), |el| {
                                        el.child(
                                            forecast.domain.as_deref().unwrap_or("").to_string(),
                                        )
                                    })
                                    .when(!brier_text.is_empty(), |el| el.child(brier_text.clone()))
                                    .when(forecast.target_date.is_some(), |el| {
                                        el.child(format!(
                                            "→ {}",
                                            forecast
                                                .target_date
                                                .as_deref()
                                                .and_then(|s| s.split('T').next())
                                                .unwrap_or("?")
                                        ))
                                    }),
                            ),
                    )
                    .child(
                        // Status badge
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(status_color))
                            .px(px(8.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .bg(theme::bg_active())
                            .child(forecast.status.clone()),
                    )
                    // Visibility badge (Live forecasts only). Spec 24 §3.5.6:
                    // read visibility + team_id + share_count, not the dead
                    // "team" visibility literal.
                    .when(forecast.status == "active", |el| {
                        let share_count = forecast.share_count.unwrap_or(0);
                        let has_team = forecast
                            .team_id
                            .as_deref()
                            .map(|s| !s.is_empty())
                            .unwrap_or(false);
                        let (icon, label, color) = if forecast.visibility == "public" {
                            ("🌐", "public".to_string(), theme::CYAN)
                        } else if has_team {
                            ("👥", "team".to_string(), theme::BLUE)
                        } else if share_count > 0 {
                            ("🔗", format!("shared · {}", share_count), theme::GOLD)
                        } else {
                            ("🔒", "private".to_string(), theme::FG_DIM)
                        };
                        el.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(rgb(color))
                                .child(format!("{} {}", icon, label)),
                        )
                    })
                    .when(forecast.actual_outcome.is_some(), |el| {
                        el.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(if forecast.actual_outcome == Some(true) {
                                    theme::green()
                                } else {
                                    theme::red()
                                })
                                .child(if forecast.actual_outcome == Some(true) {
                                    "Yes"
                                } else {
                                    "No"
                                }),
                        )
                    })
                    // Expand/collapse indicator
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child(if is_selected { "▾" } else { "▸" }),
                    ),
            )
            // ── Detail panel (visible when selected) ──────────────
            .when(is_selected, |el| {
                let is_active = forecast.status == "active";
                let fid = forecast.id.clone();
                let fid_open = forecast.id.clone();
                let fq = forecast.question_text.clone();
                let fprob = forecast.predicted_probability;
                let _ = fprob;
                el.child(render_forecast_detail(forecast))
                    .child(self.render_forecast_portfolio_row(&forecast.id, cx))
                    // Action row: Open in cockpit + (when active) Resolve.
                    // The inline detail above shows metadata; the cockpit
                    // shows the FPL, drivers, Trajectory tab, and BayesOps
                    // events. The button is the explicit handoff between
                    // those two views.
                    .child(
                        div()
                            .px(px(24.0))
                            .py(px(10.0))
                            .border_t_1()
                            .border_color(theme::fg_faint())
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .id(SharedString::from(format!("open-cockpit-{}", fid_open)))
                                    .px(px(14.0))
                                    .py(px(5.0))
                                    .rounded(px(5.0))
                                    .border_1()
                                    .border_color(rgb(theme::CYAN))
                                    .text_size(px(11.0))
                                    .text_color(rgb(theme::CYAN))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::bg_hover()))
                                    .on_click(cx.listener(move |this, _event, _window, cx| {
                                        this.open_forecast(&fid_open, cx);
                                    }))
                                    .child("→ Open in Cockpit"),
                            )
                            .when(is_active, |el| {
                                el.child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(theme::fg_faint())
                                        .child("Outcome known?"),
                                )
                                .child(
                                    div()
                                        .id(SharedString::from(format!("resolve-btn-{}", fid)))
                                        .px(px(14.0))
                                        .py(px(5.0))
                                        .rounded(px(5.0))
                                        .border_1()
                                        .border_color(rgb(theme::GREEN))
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme::GREEN))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme::bg_hover()))
                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                            this.resolve_forecast_id = Some(fid.clone());
                                            this.resolve_forecast_question = fq.clone();
                                            this.resolve_outcome = None;
                                            this.resolve_error = None;
                                            this.resolve_loading = false;
                                            this.resolve_sheet_showing = true;
                                            cx.notify();
                                        }))
                                        .child("⚡ Resolve"),
                                )
                            }),
                    )
            })
    }

    // ── Agent Fleet Panel ─────────────────────────────────────────────────

    fn render_agent_fleet_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let cards = self.registry.list_cards().unwrap_or_default();
        // Show only hireable orchestra members. `tier=System` filters
        // out meta/infrastructure agents like Fermi itself (the
        // conductor) and xaman_ek: they're always-on by design, not
        // discoverable hires. Same signal the conformance test uses,
        // so a future system agent tagged fermi-orchestra by mistake
        // won't accidentally appear here.
        let fermi_agents: Vec<_> = cards
            .iter()
            .filter(|c| c.metadata.tags.iter().any(|t| t == "fermi-orchestra"))
            .filter(|c| !matches!(c.tier, fermi::agent_backend::agent_card::AgentTier::System))
            .collect();

        // Pull live cockpit data if we're in a forecast session.
        let (agent_runs, session_cost, assigned_map) =
            if let Some(ref cockpit_entity) = self.cockpit {
                let state = cockpit_entity.read(cx);
                let runs = state.agent_runs.clone();
                let cost = state.session_cost;
                // Build a map: agent_name → Vec<driver_name>
                let mut amap: std::collections::HashMap<String, Vec<String>> =
                    std::collections::HashMap::new();
                for agent_stmt in state.program.agents() {
                    for driver_ref in &agent_stmt.driver_refs {
                        amap.entry(agent_stmt.name.clone())
                            .or_default()
                            .push(driver_ref.clone());
                    }
                }
                (runs, cost, amap)
            } else {
                (vec![], 0.0, std::collections::HashMap::new())
            };

        // Session credit summary
        let running_count = agent_runs
            .iter()
            .filter(|r| r.status == cockpit::AgentRunStatus::Running)
            .count();
        let completed_count = agent_runs
            .iter()
            .filter(|r| r.status == cockpit::AgentRunStatus::Completed)
            .count();
        let failed_count = agent_runs
            .iter()
            .filter(|r| r.status == cockpit::AgentRunStatus::Failed)
            .count();

        div()
            .id("agent-fleet-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            // ── Header ────────────────────────────────────────────────
            .child(
                div()
                    .px(px(24.0))
                    .py(px(16.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .text_color(theme::cyan())
                            .font_weight(FontWeight::BOLD)
                            .child("⚙ Research Fleet"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .child(format!("{} fermi-orchestra agents", fermi_agents.len())),
                    )
                    // Session credit cost
                    .when(session_cost > 0.0, |el| {
                        el.child(
                            div()
                                .ml_auto()
                                .text_size(px(11.0))
                                .text_color(theme::fg_dim())
                                .child(format!("⚡ {:.1} credits this session", session_cost)),
                        )
                    }),
            )
            // ── Status summary bar ────────────────────────────────────
            .when(!agent_runs.is_empty(), |el| {
                el.child(
                    div()
                        .px(px(24.0))
                        .py(px(8.0))
                        .border_b_1()
                        .border_color(theme::fg_faint())
                        .flex()
                        .items_center()
                        .gap(px(16.0))
                        .when(running_count > 0, |el| {
                            el.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::gold())
                                    .child(format!("⟳ {} running", running_count)),
                            )
                        })
                        .when(completed_count > 0, |el| {
                            el.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::green())
                                    .child(format!("✓ {} done", completed_count)),
                            )
                        })
                        .when(failed_count > 0, |el| {
                            el.child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::red())
                                    .child(format!("✗ {} failed", failed_count)),
                            )
                        }),
                )
            })
            // ── Marketplace (Sprint C) ────────────────────────────────────────────
            .child(self.render_agent_marketplace(&fermi_agents, &agent_runs, cx))
            // ── Session agent cards (per-forecast run status) ────────────
            //
            // Only render one card per agent that actually has a run
            // in the currently-open cockpit session. Rendering every
            // fermi-orchestra agent here was a pure duplicate of the
            // marketplace list above and made the Agent Fleet panel
            // scroll forever with the same names.
            .when(!agent_runs.is_empty(), |el| {
                let mut session_rows: Vec<gpui::AnyElement> = Vec::new();
                for card in fermi_agents.iter() {
                    let agent_id = &card.agent_id;
                    if let Some(run) = agent_runs.iter().find(|r| r.agent_name == *agent_id) {
                        let drivers = assigned_map.get(agent_id).cloned().unwrap_or_default();
                        session_rows.push(
                            render_fleet_agent_row(card, Some(run), &drivers).into_any_element(),
                        );
                    }
                }
                if session_rows.is_empty() {
                    el
                } else {
                    el.child(
                        div()
                            .px(px(16.0))
                            .py(px(8.0))
                            .border_t_1()
                            .border_color(theme::fg_faint())
                            .text_size(px(11.0))
                            .text_color(theme::fg_faint())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("THIS SESSION"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .p(px(16.0))
                            .children(session_rows),
                    )
                }
            })
            .when(fermi_agents.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .py(px(48.0))
                        .w_full()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .text_color(theme::fg_dim())
                                .child("No fermi-orchestra agents found"),
                        )
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme::fg_faint())
                                .mt(px(4.0))
                                .child("Open a forecast in the Composer to assign agents"),
                        ),
                )
            })
    }

    // ── Agent marketplace (Sprint C) ──────────────────────────────────────
    //
    // Ranked cards with cost / success / usage / contribution stats and
    // a Hire button that assigns the agent in the currently-open
    // cockpit forecast. Rendered above the per-session status list.

    fn render_agent_marketplace(
        &self,
        local_cards: &[&AgentCard],
        session_runs: &[cockpit::AgentExecution],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut entries = build_agent_marketplace(local_cards, &self.agent_cards, session_runs);
        sort_marketplace(&mut entries, &self.agent_marketplace_sort);

        // Apply tier filter.
        if self.agent_marketplace_tier != "all" {
            let want = self.agent_marketplace_tier.clone();
            entries.retain(|e| e.tier == want);
        }

        // Tier chip row.
        let tier_defs: &[(&str, &str)] = &[
            ("all", "All"),
            ("popular", "🏆 Popular"),
            ("established", "◉ Established"),
            ("rising", "▲ Rising"),
            ("fresh", "✨ Fresh"),
        ];
        let mut tier_row = div().flex().flex_wrap().gap(px(6.0)).child(
            div()
                .text_size(px(9.0))
                .text_color(theme::fg_faint())
                .child("TIER:"),
        );
        for (key, label) in tier_defs {
            let is_on = *key == self.agent_marketplace_tier;
            let key_owned = (*key).to_string();
            tier_row = tier_row.child(
                div()
                    .id(SharedString::from(format!("mkt-tier-{}", key)))
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(if is_on {
                        theme::cyan()
                    } else {
                        theme::fg_faint()
                    })
                    .bg(if is_on {
                        theme::bg_active()
                    } else {
                        theme::bg_elevated()
                    })
                    .text_size(px(10.0))
                    .text_color(if is_on {
                        theme::cyan()
                    } else {
                        theme::fg_dim()
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::bg_hover()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.agent_marketplace_tier = key_owned.clone();
                        cx.notify();
                    }))
                    .child(label.to_string()),
            );
        }

        // Sort chip row.
        let sort_defs: &[(&str, &str)] = &[
            ("score", "Ranked"),
            ("contribution", "Best contribution"),
            ("cost_asc", "Cost low→high"),
            ("cost_desc", "Cost high→low"),
            ("success", "Most reliable"),
            ("executions", "Most used"),
        ];
        let mut sort_row = div().flex().flex_wrap().gap(px(6.0)).child(
            div()
                .text_size(px(9.0))
                .text_color(theme::fg_faint())
                .child("SORT:"),
        );
        for (key, label) in sort_defs {
            let is_on = *key == self.agent_marketplace_sort;
            let key_owned = (*key).to_string();
            sort_row = sort_row.child(
                div()
                    .id(SharedString::from(format!("mkt-sort-{}", key)))
                    .px(px(8.0))
                    .py(px(2.0))
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(if is_on {
                        theme::cyan()
                    } else {
                        theme::fg_faint()
                    })
                    .bg(if is_on {
                        theme::bg_active()
                    } else {
                        theme::bg_elevated()
                    })
                    .text_size(px(10.0))
                    .text_color(if is_on {
                        theme::cyan()
                    } else {
                        theme::fg_dim()
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::bg_hover()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.agent_marketplace_sort = key_owned.clone();
                        cx.notify();
                    }))
                    .child(label.to_string()),
            );
        }

        // Snapshot before the list moves out of `entries` — used
        // for the "no usage data yet, listed alphabetically" hint
        // that fires when NOTHING in the visible set has been run.
        // Alphabetical fallback happens implicitly (sort_marketplace's
        // secondary key), so this is a diagnostic, not a config knob.
        let no_data_at_all = entries.iter().all(|e| !e.has_data);
        let visible_count = entries.len();

        // Card list.
        let cards = div().flex().flex_col().gap(px(8.0)).children(
            entries
                .into_iter()
                .enumerate()
                .map(|(rank, e)| self.render_marketplace_card(rank + 1, &e, cx)),
        );

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .px(px(16.0))
            .py(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(theme::fg())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("AGENT MARKETPLACE"),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child("hire agents to research your drivers"),
                    ),
            )
            .child(tier_row)
            .child(sort_row)
            .when(no_data_at_all && visible_count > 0, |el| {
                el.child(
                    div()
                        .px(px(10.0))
                        .py(px(6.0))
                        .rounded(px(4.0))
                        .bg(theme::bg())
                        .border_1()
                        .border_color(theme::fg_faint())
                        .text_size(px(10.0))
                        .text_color(theme::fg_dim())
                        .child(
                            "No usage data yet on these agents — listed alphabetically. \
                             Sorting activates once agents have runs, cost, or success data.",
                        ),
                )
            })
            .when(visible_count == 0, |el| {
                el.child(
                    div()
                        .px(px(10.0))
                        .py(px(10.0))
                        .text_size(px(11.0))
                        .text_color(theme::fg_faint())
                        .child("No agents match the current filter."),
                )
            })
            .child(cards)
    }

    fn render_marketplace_card(
        &self,
        rank: usize,
        e: &AgentMarketplaceEntry,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_expanded = self.agent_marketplace_expanded.contains(&e.agent_id);

        // Cost cell — hide entirely when no data. "cost n/a" was
        // confusing when every agent showed it; better to omit and let
        // the eye go straight to real signals.
        let cost_cell: Option<(String, u32)> = e.avg_cost_per_run.map(|c| {
            let color = if c <= 0.05 {
                theme::GREEN
            } else if c <= 0.15 {
                theme::CYAN
            } else if c <= 0.30 {
                theme::GOLD
            } else {
                theme::ORANGE
            };
            (format!("${:.3}/run", c), color)
        });

        // Success cell — hide when there are no runs (was reading as
        // "— no runs" for every agent in a fresh install).
        let success_cell: Option<(String, u32)> = if e.total_executions > 0 {
            let color = if e.success_rate >= 0.9 {
                theme::GREEN
            } else if e.success_rate >= 0.75 {
                theme::CYAN
            } else {
                theme::GOLD
            };
            Some((format!("{:.0}% success", e.success_rate * 100.0), color))
        } else {
            None
        };

        // Usage cell — skip when zero.
        let usage_cell: Option<String> = if e.total_executions > 0 {
            Some(format!("{} runs", e.total_executions))
        } else {
            None
        };

        let contribution_str = e
            .avg_confidence_this_session
            .map(|c| format!("session confidence {:.0}%", c * 100.0));

        // Tag pills (up to 3).
        let tag_row = {
            let mut row = div().flex().flex_wrap().gap(px(4.0));
            for tag in e.tags.iter().take(3) {
                row = row.child(
                    div()
                        .px(px(5.0))
                        .py(px(1.0))
                        .rounded(px(3.0))
                        .bg(theme::bg_hover())
                        .text_size(px(9.0))
                        .text_color(theme::fg_dim())
                        .child(tag.clone()),
                );
            }
            row
        };

        let agent_id_for_hire = e.agent_id.clone();
        let agent_display_for_hire = e.display_name.clone();
        let agent_id_for_toggle = e.agent_id.clone();

        // Score chip: only render when there's usage data to score
        // against. Otherwise show a subtle "unrated" pill so the
        // operator can tell "we don't know yet" from "we know and it's
        // bad".
        let score_chip: AnyElement = if e.has_data {
            div()
                .text_size(px(10.0))
                .text_color(theme::gold())
                .font_weight(FontWeight::SEMIBOLD)
                .child(format!("score {:.0}", e.score))
                .into_any_element()
        } else {
            div()
                .text_size(px(9.0))
                .text_color(theme::fg_faint())
                .child("unrated")
                .into_any_element()
        };

        let compact = div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(theme::fg_faint())
            .bg(theme::bg_elevated())
            // Header row: chevron + rank + name + tier + score
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("mkt-chev-{}", e.agent_id)))
                            .w(px(14.0))
                            .text_size(px(11.0))
                            .text_color(if is_expanded {
                                theme::cyan()
                            } else {
                                theme::fg_dim()
                            })
                            .cursor_pointer()
                            .hover(|s| s.text_color(theme::cyan()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if this
                                    .agent_marketplace_expanded
                                    .contains(&agent_id_for_toggle)
                                {
                                    this.agent_marketplace_expanded.remove(&agent_id_for_toggle);
                                } else {
                                    this.agent_marketplace_expanded
                                        .insert(agent_id_for_toggle.clone());
                                }
                                cx.notify();
                            }))
                            .child(if is_expanded { "▾" } else { "▸" }),
                    )
                    .child(
                        div()
                            .w(px(28.0))
                            .text_size(px(14.0))
                            .text_color(theme::fg_dim())
                            .font_weight(FontWeight::BOLD)
                            .child(format!("#{}", rank)),
                    )
                    .child(
                        div()
                            .flex_grow()
                            .text_size(px(13.0))
                            .text_color(theme::fg())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(e.display_name.clone()),
                    )
                    .child(
                        div()
                            .px(px(6.0))
                            .py(px(1.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(rgb(e.tier_color))
                            .bg(theme::bg())
                            .text_size(px(9.0))
                            .text_color(rgb(e.tier_color))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(e.tier.to_uppercase()),
                    )
                    .child(score_chip),
            )
            // Description
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme::fg_dim())
                    .child(truncate(&e.description, 140)),
            )
            // Stats row — hidden entirely when the agent has no data,
            // and each cell is Option so the row doesn't leave gaps.
            .when(e.has_data, |el| {
                let mut row = div().flex().items_center().flex_wrap().gap(px(10.0));
                if let Some((str_, color)) = cost_cell.clone() {
                    row = row.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(color))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(str_),
                    );
                }
                if let Some((str_, color)) = success_cell.clone() {
                    row = row.child(div().text_size(px(10.0)).text_color(rgb(color)).child(str_));
                }
                if let Some(u) = usage_cell.clone() {
                    row = row.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .child(u),
                    );
                }
                if let Some(c) = contribution_str.clone() {
                    row = row.child(div().text_size(px(10.0)).text_color(theme::cyan()).child(c));
                }
                el.child(row)
            })
            // Tags + hire button.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(tag_row)
                    .child(div().flex_grow())
                    .child(
                        div()
                            .id(SharedString::from(format!("mkt-hire-{}", e.agent_id)))
                            .px(px(12.0))
                            .py(px(4.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(rgb(theme::CYAN))
                            .bg(rgb(theme::CYAN))
                            .text_size(px(10.0))
                            .text_color(rgb(theme::BG))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.85))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_hire_modal(
                                    agent_id_for_hire.clone(),
                                    agent_display_for_hire.clone(),
                                    cx,
                                );
                            }))
                            .child("Hire"),
                    ),
            );

        // Drill-down panel — accepts/produces, model, sample queries,
        // MCP tools, and the ABW agent page link.
        let drill: Option<AnyElement> = if is_expanded {
            Some(self.render_marketplace_card_drill(e, cx).into_any_element())
        } else {
            None
        };

        div()
            .flex()
            .flex_col()
            .child(compact)
            .when_some(drill, |el, d| el.child(d))
    }

    /// Expanded rich-detail panel for a marketplace card. Reads from
    /// the AgentCard fields that ABW already exposes: accepts/produces
    /// contract, sample queries, MCP tools, skills, model + temperature,
    /// author + version, and secrets requirement.
    fn render_marketplace_card_drill(
        &self,
        e: &AgentMarketplaceEntry,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Row helper — label + value pair.
        let row = |label: &'static str, value: String| {
            div()
                .flex()
                .items_start()
                .gap(px(8.0))
                .py(px(2.0))
                .child(
                    div()
                        .w(px(88.0))
                        .text_size(px(9.0))
                        .text_color(theme::fg_faint())
                        .child(label),
                )
                .child(
                    div()
                        .flex_grow()
                        .text_size(px(10.0))
                        .text_color(theme::fg())
                        .child(value),
                )
        };

        // Contract row: accepts → produces. Shows the agent's IO
        // contract as a data-flow arrow. If either side is empty, we
        // dash it so the shape stays legible.
        let accepts_str = if e.accepts.is_empty() {
            "—".to_string()
        } else {
            e.accepts.join(", ")
        };
        let produces_str = if e.produces.is_empty() {
            "—".to_string()
        } else {
            e.produces.join(", ")
        };
        let contract = row("CONTRACT", format!("{}  →  {}", accepts_str, produces_str));

        // Model + temperature.
        let model_row = row("MODEL", format!("{} · temp {:.2}", e.model, e.temperature));

        // Version + author.
        let version_row = row("VERSION", format!("v{} · {}", e.version, e.author));

        // Sample queries (up to 3, each truncated). Rendered as a
        // label + column-of-bullets pair so the bullets stack under
        // one another instead of sitting on a single wrapped line —
        // matches the CONTRACT / MODEL / VERSION row layout above.
        let sample_queries_section: Option<AnyElement> = if e.sample_queries.is_empty() {
            None
        } else {
            let mut bullets = div().flex().flex_col().gap(px(3.0)).flex_grow();
            for q in e.sample_queries.iter().take(3) {
                bullets = bullets.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::fg_dim())
                        .child(format!("• {}", truncate(q, 120))),
                );
            }
            Some(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .py(px(2.0))
                    .child(
                        div()
                            .w(px(88.0))
                            .flex_shrink_0()
                            .text_size(px(9.0))
                            .text_color(theme::fg_faint())
                            .child("SAMPLE"),
                    )
                    .child(bullets)
                    .into_any_element(),
            )
        };

        // MCP tools + skills as pill rows. Structured as label +
        // flex-wrap pill container so the label stays anchored on the
        // left edge while the pills wrap on their own line — the old
        // layout let pills wrap *around* the label, breaking column
        // alignment with the other drill rows.
        let make_pill_row = |title: &'static str, items: &[String]| -> Option<AnyElement> {
            if items.is_empty() {
                return None;
            }
            let mut pills = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(4.0))
                .flex_grow();
            for item in items {
                pills = pills.child(
                    div()
                        .px(px(6.0))
                        .py(px(1.0))
                        .rounded(px(3.0))
                        .bg(theme::bg_elevated())
                        .border_1()
                        .border_color(theme::fg_faint())
                        .text_size(px(9.0))
                        .text_color(theme::fg())
                        .child(item.clone()),
                );
            }
            Some(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .py(px(2.0))
                    .child(
                        div()
                            .w(px(88.0))
                            .flex_shrink_0()
                            .text_size(px(9.0))
                            .text_color(theme::fg_faint())
                            .child(title),
                    )
                    .child(pills)
                    .into_any_element(),
            )
        };
        let tools_row = make_pill_row("TOOLS", &e.mcp_tools);
        let skills_row = make_pill_row("SKILLS", &e.skills);

        // ABW agent page (open in browser) + secrets warning.
        // NB: the ABW page route is `/agent/:id` (singular) — the
        // plural `/agents/...` prefix belongs to the JSON API only, so
        // pointing the browser at it lands on a 404.
        let base_url = self.api.base_url_sync();
        let agent_url = format!("{}/agent/{}", base_url.trim_end_matches('/'), e.agent_id);
        let agent_url_for_open = agent_url.clone();

        div()
            .px(px(38.0)) // indent past chevron + rank
            .py(px(8.0))
            .bg(theme::bg_active())
            .border_l_2()
            .border_color(rgb(theme::CYAN))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(contract)
            .child(model_row)
            .child(version_row)
            .when_some(sample_queries_section, |el, s| el.child(s))
            .when_some(tools_row, |el, s| el.child(s))
            .when_some(skills_row, |el, s| el.child(s))
            .when(e.needs_secrets, |el| {
                el.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::gold())
                        .child("⚠ Requires secrets — configure in ABW before first run."),
                )
            })
            .child(
                div().flex().items_center().gap(px(8.0)).mt(px(4.0)).child(
                    div()
                        .id(SharedString::from(format!("mkt-abw-{}", e.agent_id)))
                        .text_size(px(10.0))
                        .text_color(theme::purple())
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme::cyan()))
                        .on_click(cx.listener(move |_this, _, _, _cx| {
                            let _ = open::that(&agent_url_for_open);
                        }))
                        .child("Open in ABW ↗"),
                ),
            )
    }

    /// Open the three-tier hire modal for the given agent. Step 1 lets
    /// the operator pick the forecast; the modal advances through
    /// steps 2 (driver) and 3 (terms) as selections are made.
    fn open_hire_modal(&mut self, agent_id: String, agent_display: String, cx: &mut Context<Self>) {
        let notes = cx.new(|cx| {
            TextInput::new(cx).with_placeholder(
                "Optional hire notes — e.g. focus on Q4 macro shocks, cap credits to $2…",
            )
        });
        self.hire_modal = Some(HireModalState {
            agent_id,
            agent_display,
            step: 1,
            forecast_id: None,
            forecast_label: None,
            driver_name: None,
            notes,
        });
        cx.notify();
    }

    /// Advance the hire modal to the next step, or (when confirming
    /// from step 3) surface a hint into the cockpit and navigate to
    /// the Composer. Full auto-assign wiring is a follow-up; today
    /// this modal is the affordance + terms review + a clear
    /// hand-off to the Composer's + Assign Agent flow.
    fn advance_hire_modal(&mut self, cx: &mut Context<Self>) {
        let Some(ref modal) = self.hire_modal.clone() else {
            return;
        };
        match modal.step {
            1 => {
                // Forecast selection required.
                if modal.forecast_id.is_none() {
                    return;
                }
                if let Some(m) = self.hire_modal.as_mut() {
                    m.step = 2;
                }
                cx.notify();
            }
            2 => {
                // Driver optional — operator may hire ambient. Advance.
                if let Some(m) = self.hire_modal.as_mut() {
                    m.step = 3;
                }
                cx.notify();
            }
            _ => {
                // Confirm. Route to Composer + hint.
                let msg = match &modal.driver_name {
                    Some(d) => format!(
                        "Hire confirmed: assign {} to driver '{}' via + Assign Agent (wiring soon).",
                        modal.agent_display, d
                    ),
                    None => format!(
                        "Hire confirmed: {} bound as ambient research agent (wiring soon).",
                        modal.agent_display
                    ),
                };
                self.hire_modal = None;
                self.show_toast(msg, "✓", theme::GREEN, cx);
                self.active_panel = Panel::Composer;
                cx.notify();
            }
        }
    }

    /// Legacy shim — preserved so any menu / keyboard action that
    /// still calls the old "hire from marketplace" name compiles. The
    /// current UX routes through `open_hire_modal` instead.
    #[allow(dead_code)]
    fn hire_agent_from_marketplace(&mut self, agent_id: String, cx: &mut Context<Self>) {
        self.open_hire_modal(agent_id.clone(), agent_id, cx);
    }

    // ── Leaderboard Panel ───────────────────────────────────────────────────────

    // ── Teams panel (Spec 24 §3.5.4) ─────────────────────────────────

    fn render_teams_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let selected_id = self.selected_team_id.clone();

        div()
            .id("teams-panel")
            .flex()
            .flex_col()
            .size_full()
            // Header
            .child(
                div()
                    .px(px(24.0))
                    .py(px(16.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .text_color(theme::blue())
                            .font_weight(FontWeight::BOLD)
                            .child("👥 Teams"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .child(format!("{} teams", self.teams.len())),
                    )
                    .when(self.teams_loading, |el| {
                        el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::blue())
                                .child("⟳ Loading…"),
                        )
                    })
                    .child(div().flex_grow())
                    .child(
                        div()
                            .id("team-new-btn")
                            .px(px(12.0))
                            .py(px(5.0))
                            .rounded(px(4.0))
                            .bg(theme::blue())
                            .text_size(px(11.0))
                            .text_color(rgb(theme::BG))
                            .font_weight(FontWeight::SEMIBOLD)
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.85))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.team_create_showing = true;
                                this.team_create_error = None;
                                this.team_create_name_input
                                    .update(cx, |inp, cx| inp.set_text("", cx));
                                this.team_create_slug_input
                                    .update(cx, |inp, cx| inp.set_text("", cx));
                                this.team_create_name_input.read(cx).focus(window);
                                cx.notify();
                            }))
                            .child("+ New Team"),
                    ),
            )
            // Two-pane body
            .child(
                div()
                    .flex()
                    .flex_grow()
                    .overflow_hidden()
                    // ── Left: team list ──────────────────────────────
                    .child(
                        div()
                            .id("teams-list")
                            .flex()
                            .flex_col()
                            .w(px(260.0))
                            .h_full()
                            .border_r_1()
                            .border_color(theme::fg_faint())
                            .overflow_y_scroll()
                            .when(self.teams.is_empty() && !self.teams_loading, |el| {
                                el.child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .gap(px(6.0))
                                        .p(px(20.0))
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .text_color(theme::fg_dim())
                                                .child(
                                                    "No collaboration teams yet. Create one to share forecasts with a group.",
                                                ),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(theme::fg_faint())
                                                .child(
                                                    "Auto-created workspace teams (Team Prior — X, Tournament Path — X) \
                                                     are hidden from this panel — they're implementation plumbing, \
                                                     not human teams.",
                                                ),
                                        ),
                                )
                            })
                            .children(self.teams.iter().map(|team| {
                                let is_sel = selected_id.as_deref() == Some(team.id.as_str());
                                let tid = team.id.clone();
                                div()
                                    .id(SharedString::from(format!("team-{}", team.id)))
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .px(px(14.0))
                                    .py(px(10.0))
                                    .cursor_pointer()
                                    .border_l_2()
                                    .border_color(if is_sel {
                                        rgb(theme::BLUE)
                                    } else {
                                        rgb(theme::BG)
                                    })
                                    .when(is_sel, |el| el.bg(theme::bg_active()))
                                    .when(!is_sel, |el| {
                                        el.hover(|s| s.bg(theme::bg_hover()))
                                    })
                                    .on_click(cx.listener(move |this, _, _w, cx| {
                                        this.select_team(tid.clone(), cx);
                                    }))
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(theme::fg())
                                            .child(team.name.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(theme::fg_faint())
                                            .child(format!("@{}", team.slug)),
                                    )
                            })),
                    )
                    // ── Right: selected team detail ───────────────────
                    .child(self.render_team_detail_pane(cx)),
            )
    }

    fn render_team_detail_pane(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let container = div()
            .id("team-detail")
            .flex()
            .flex_col()
            .flex_grow()
            .h_full()
            .overflow_y_scroll()
            .p(px(24.0))
            .gap(px(16.0));

        let Some(detail) = self.selected_team_detail.as_ref() else {
            return container
                .child(div().text_size(px(12.0)).text_color(theme::fg_dim()).child(
                    if self.selected_team_id.is_some() {
                        "Loading team…"
                    } else {
                        "Select a team to view its members."
                    },
                ))
                .into_any_element();
        };

        let team_id = detail.team.id.clone();
        let team_forecasts = self.forecasts_for_team(&team_id);
        let team_portfolios = self.portfolios_for_team(&team_id);
        let tab = self.selected_team_tab;

        container
            // Team header
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme::fg())
                            .child(detail.team.name.clone()),
                    )
                    .when(detail.team.description.is_some(), |el| {
                        el.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme::fg_dim())
                                .child(detail.team.description.clone().unwrap_or_default()),
                        )
                    }),
            )
            // Sub-tab bar (Roster / Shared / Activity). Each pill's
            // count is computed above from the same helpers the tab
            // bodies read, so the number and content can't drift.
            .child(self.render_team_tab_bar(
                detail.members.len(),
                team_forecasts.len() + team_portfolios.len(),
                cx,
            ))
            // Tab body dispatch. Each branch is self-contained so the
            // scroll position, focus, and lifecycle stay simple.
            .when(tab == TeamTab::Roster, |el| {
                el.child(self.render_team_roster_body(cx))
            })
            .when(tab == TeamTab::Shared, |el| {
                el.child(self.render_team_shared_body(&team_forecasts, &team_portfolios, cx))
            })
            .when(tab == TeamTab::Activity, |el| {
                el.child(self.render_team_activity_body(&team_forecasts, cx))
            })
            .into_any_element()
    }

    /// Tab bar for the team detail pane. Three pills with count
    /// badges. Clicking a pill sets `selected_team_tab`; the current
    /// tab is highlighted with an accent border.
    fn render_team_tab_bar(
        &self,
        member_count: usize,
        shared_count: usize,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let tab = self.selected_team_tab;
        let pill = |id: &'static str,
                    label: String,
                    kind: TeamTab,
                    accent: u32,
                    cx: &mut Context<Self>|
         -> AnyElement {
            let active = tab == kind;
            div()
                .id(id)
                .px(px(12.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(if active {
                    rgb(accent).into()
                } else {
                    theme::fg_faint()
                })
                .bg(if active {
                    theme::bg_hover()
                } else {
                    theme::bg()
                })
                .text_size(px(11.0))
                .text_color(if active {
                    rgb(accent).into()
                } else {
                    theme::fg_dim()
                })
                .font_weight(if active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .cursor_pointer()
                .hover(|s| s.bg(theme::bg_hover()))
                .on_click(cx.listener(move |this, _e, _w, cx| {
                    this.selected_team_tab = kind;
                    cx.notify();
                }))
                .child(label)
                .into_any_element()
        };

        div()
            .flex()
            .flex_row()
            .gap(px(8.0))
            .child(pill(
                "team-tab-roster",
                format!("🧑 Roster ({})", member_count),
                TeamTab::Roster,
                theme::CYAN,
                cx,
            ))
            .child(pill(
                "team-tab-shared",
                format!("◈ Shared ({})", shared_count),
                TeamTab::Shared,
                theme::BLUE,
                cx,
            ))
            .child(pill(
                "team-tab-activity",
                "📊 Activity".into(),
                TeamTab::Activity,
                theme::GOLD,
                cx,
            ))
    }

    /// Roster tab body — the existing team pane content (member
    /// header, invite row, error line, member rows, invites section,
    /// danger zone). Extracted from `render_team_detail_pane` so the
    /// tab dispatch stays readable.
    fn render_team_roster_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(detail) = self.selected_team_detail.as_ref() else {
            return div().into_any_element();
        };
        let invite_role = self.team_invite_role.clone();

        div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            // Members section header + invite toggle
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::cyan())
                            .child(format!("Members ({})", detail.members.len())),
                    )
                    .child(div().flex_grow())
                    .child(
                        div()
                            .id("team-invite-toggle")
                            .px(px(10.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .bg(theme::bg_active())
                            .text_size(px(11.0))
                            .text_color(theme::cyan())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()))
                            .on_click(cx.listener(|this, _, _w, cx| {
                                this.team_invite_showing = !this.team_invite_showing;
                                this.team_action_error = None;
                                cx.notify();
                            }))
                            .child(if self.team_invite_showing {
                                "Cancel"
                            } else {
                                "+ Invite"
                            }),
                    ),
            )
            // Invite row
            .when(self.team_invite_showing, |el| {
                el.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().flex_grow().child(self.team_invite_input.clone()))
                        // Role cycle chip
                        .child(
                            div()
                                .id("team-invite-role")
                                .px(px(10.0))
                                .py(px(6.0))
                                .rounded(px(4.0))
                                .bg(theme::bg_active())
                                .text_size(px(11.0))
                                .text_color(theme::gold())
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::bg_hover()))
                                .on_click(cx.listener(|this, _, _w, cx| {
                                    this.team_invite_role = match this.team_invite_role.as_str() {
                                        "viewer" => "member",
                                        "member" => "admin",
                                        "admin" => "owner",
                                        _ => "viewer",
                                    }
                                    .into();
                                    cx.notify();
                                }))
                                .child(invite_role),
                        )
                        .child(
                            div()
                                .id("team-invite-send")
                                .px(px(12.0))
                                .py(px(6.0))
                                .rounded(px(4.0))
                                .bg(theme::blue())
                                .text_size(px(11.0))
                                .text_color(rgb(theme::BG))
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .hover(|s| s.opacity(0.85))
                                .on_click(cx.listener(|this, _, _w, cx| {
                                    this.invite_team_member_from_input(cx);
                                }))
                                .child(if self.team_invite_loading {
                                    "Sending…"
                                } else {
                                    "Send"
                                }),
                        ),
                )
            })
            // Error line
            .when(self.team_action_error.is_some(), |el| {
                el.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme::red())
                        .child(self.team_action_error.clone().unwrap_or_default()),
                )
            })
            // Member rows
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .children(detail.members.iter().map(|m| {
                        let mid = m.member_id.clone();
                        let is_owner_role = m.role == "owner";
                        // Prefer server-resolved display name. Fall back
                        // to a short-id (first 8 of a UUID) so the row
                        // stays readable when the users JOIN missed.
                        let label = m
                            .member_display_name
                            .clone()
                            .unwrap_or_else(|| short_user_label(&m.member_id));
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .bg(theme::bg_elevated())
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .child(if m.member_type == "agent" {
                                        "🤖"
                                    } else {
                                        "🧑"
                                    }),
                            )
                            .child(
                                div()
                                    .flex_grow()
                                    .flex()
                                    .flex_col()
                                    .gap(px(1.0))
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme::fg())
                                            .child(label),
                                    )
                                    .when(m.member_display_name.is_some(), |el| {
                                        el.child(
                                            div()
                                                .text_size(px(9.0))
                                                .text_color(theme::fg_faint())
                                                .child(short_user_label(&m.member_id)),
                                        )
                                    }),
                            )
                            // Role chip
                            .child(
                                div()
                                    .px(px(8.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(theme::bg_active())
                                    .text_size(px(10.0))
                                    .text_color(theme::gold())
                                    .child(m.role.clone()),
                            )
                            // Remove (✕) — not shown for owners
                            .when(!is_owner_role, |el| {
                                el.child(
                                    div()
                                        .id(SharedString::from(format!("rm-{}", mid)))
                                        .px(px(6.0))
                                        .py(px(2.0))
                                        .rounded(px(4.0))
                                        .text_size(px(12.0))
                                        .text_color(theme::fg_dim())
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme::bg_hover()).text_color(theme::red()))
                                        .on_click(cx.listener({
                                            let mid = mid.clone();
                                            move |this, _, _w, cx| {
                                                this.remove_team_member(mid.clone(), cx);
                                            }
                                        }))
                                        .child("✕"),
                                )
                            })
                    })),
            )
            // ── Pending / recent invites ───────────────────────────
            .child(self.render_team_invites_section(cx))
            // ── Danger zone ─────────────────────────────────
            //
            // Delete-team is destructive + irreversible; server-side
            // only the OWNER can call it. Rendered at the very bottom
            // of the roster tab (not the header) with a two-step
            // confirmation flow so a stray click can't nuke a team.
            .child(self.render_team_danger_zone(cx))
            .into_any_element()
    }

    /// Shared tab body — forecasts + portfolios owned by or shared
    /// with the team. Purely a client-side view over data already
    /// loaded for other panels; the fan-out that populates the team
    /// shares caches runs on `fetch_forecasts` / `fetch_portfolios`
    /// completion. Empty state coaches the operator through the
    /// two ways items get associated with a team.
    fn render_team_shared_body(
        &self,
        forecasts: &[&Forecast],
        portfolios: &[&Portfolio],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let container = div().flex().flex_col().gap(px(16.0));

        if forecasts.is_empty() && portfolios.is_empty() {
            return container
                .child(
                    div()
                        .p(px(16.0))
                        .rounded(px(6.0))
                        .bg(theme::bg_elevated())
                        .border_1()
                        .border_color(theme::fg_faint())
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme::fg())
                                .font_weight(FontWeight::SEMIBOLD)
                                .child("Nothing shared with this team yet"),
                        )
                        .child(div().text_size(px(11.0)).text_color(theme::fg_dim()).child(
                            "Two ways to associate work with a team:\n\
                                       • Own the forecast or portfolio as the team (team_id).\n\
                                       • Share it with the team from the item's Access panel.",
                        )),
                )
                .into_any_element();
        }

        // Forecasts section
        let mut result = container;
        if !forecasts.is_empty() {
            result = result.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::cyan())
                            .child(format!("Forecasts ({})", forecasts.len())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .rounded(px(6.0))
                            .overflow_hidden()
                            .border_1()
                            .border_color(theme::fg_faint())
                            .children(forecasts.iter().map(|f| self.render_forecast_row(f, cx))),
                    ),
            );
        }

        // Portfolios section
        if !portfolios.is_empty() {
            result = result.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::blue())
                            .child(format!("Portfolios ({})", portfolios.len())),
                    )
                    .child(
                        div().flex().flex_col().gap(px(6.0)).children(
                            portfolios
                                .iter()
                                .map(|p| self.render_team_portfolio_row(p, cx)),
                        ),
                    ),
            );
        }

        result.into_any_element()
    }

    /// One row in the Shared tab's portfolio list. Compact tile:
    /// title + counts, click navigates to Portfolio panel with the
    /// portfolio selected. Uses the existing selection state so the
    /// two panels stay in lockstep.
    fn render_team_portfolio_row(
        &self,
        portfolio: &Portfolio,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pid = portfolio.id.clone();
        let pid_click = pid.clone();
        let title = portfolio.title.clone();
        let count = portfolio.forecast_count.unwrap_or(0);
        let resolved = portfolio.resolved_count.unwrap_or(0);
        let brier_text = portfolio
            .avg_brier
            .map(|b| format!("· avg Brier {:.3}", b))
            .unwrap_or_default();

        div()
            .id(SharedString::from(format!("team-portfolio-{}", pid)))
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(theme::fg_faint())
            .bg(theme::bg_elevated())
            .cursor_pointer()
            .hover(|s| s.bg(theme::bg_hover()).border_color(rgb(theme::BLUE)))
            .on_click(cx.listener(move |this, _e, _w, cx| {
                this.selected_portfolio_id = Some(pid_click.clone());
                this.selected_virtual_portfolio = None;
                this.fetch_portfolio_forecasts(pid_click.clone(), cx);
                this.fetch_portfolio_stats_if_needed(pid_click.clone(), cx);
                this.navigate(Panel::Portfolio, cx);
            }))
            .child(
                div()
                    .text_size(px(14.0))
                    .text_color(rgb(theme::BLUE))
                    .child("◈"),
            )
            .child(
                div()
                    .flex_grow()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(theme::fg())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .child(format!(
                                "{} forecast{} · {} resolved {}",
                                count,
                                if count == 1 { "" } else { "s" },
                                resolved,
                                brier_text
                            )),
                    ),
            )
    }

    /// Activity tab body — recent revisions / publications /
    /// resolutions across the team's forecasts. Client-side derived
    /// from the same fields the Dashboard's Recent Activity feed
    /// reads (updated_at, created_at, resolved_at, status). Sorted
    /// newest-first, truncated to the top 15 to keep it scannable.
    ///
    /// This is the team-scoped counterpart of `recompute_recent_activity`;
    /// kept as an on-demand derivation rather than a cached feed
    /// because it changes every time a team gains or loses a
    /// forecast, and the input size is small (< N forecasts * 1
    /// event each).
    fn render_team_activity_body(
        &self,
        forecasts: &[&Forecast],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Build a flat event list. Each forecast contributes at most
        // one event: the most recent milestone we can derive without
        // the full update_history (which isn't populated on list
        // responses). Precedence: resolved > revised > published >
        // draft.
        struct Event<'a> {
            forecast: &'a Forecast,
            ts: String,
            icon: &'static str,
            color: u32,
            verb: &'static str,
            trailing: String,
        }

        let mut events: Vec<Event> = Vec::new();
        for f in forecasts.iter().copied() {
            let ts = f
                .resolved_at
                .clone()
                .or_else(|| f.updated_at.clone())
                .or_else(|| f.created_at.clone())
                .unwrap_or_default();
            if ts.is_empty() {
                continue;
            }
            let (icon, color, verb, trailing): (&'static str, u32, &'static str, String) =
                if f.status == "resolved" {
                    let outcome = if f.actual_outcome == Some(true) {
                        "Yes"
                    } else {
                        "No"
                    };
                    let brier = f.brier_score.unwrap_or(0.5);
                    let color = if brier < 0.15 {
                        theme::GREEN
                    } else if brier < 0.3 {
                        theme::GOLD
                    } else {
                        theme::ORANGE
                    };
                    (
                        "✓",
                        color,
                        "Resolved",
                        format!("{} · Brier {:.2}", outcome, brier),
                    )
                } else if f.status == "draft" {
                    (
                        "✎",
                        theme::GOLD,
                        "Draft",
                        format!("{:.0}%", f.predicted_probability * 100.0),
                    )
                } else {
                    // active — distinguish first publish from later revision
                    let was_revised = f.created_at.as_ref().map(|c| c != &ts).unwrap_or(false);
                    let (icon, verb) = if was_revised {
                        ("→", "Revised")
                    } else {
                        ("◐", "Published")
                    };
                    (
                        icon,
                        theme::CYAN,
                        verb,
                        format!("{:.0}%", f.predicted_probability * 100.0),
                    )
                };
            events.push(Event {
                forecast: f,
                ts,
                icon,
                color,
                verb,
                trailing,
            });
        }

        events.sort_by(|a, b| b.ts.cmp(&a.ts));
        events.truncate(15);

        let container = div().flex().flex_col().gap(px(8.0));

        if events.is_empty() {
            return container
                .child(
                    div()
                        .p(px(16.0))
                        .rounded(px(6.0))
                        .bg(theme::bg_elevated())
                        .border_1()
                        .border_color(theme::fg_faint())
                        .text_size(px(11.0))
                        .text_color(theme::fg_dim())
                        .child(
                            "No activity yet. Publish or revise a forecast \
                             associated with this team to see events here.",
                        ),
                )
                .into_any_element();
        }

        container
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(px(6.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme::fg_faint())
                    .bg(theme::bg_elevated())
                    .children(events.into_iter().map(|e| {
                        let fid = e.forecast.id.clone();
                        let question = truncate(&e.forecast.question_text, 48).to_string();
                        let time = format_relative_time(&e.ts);
                        div()
                            .id(SharedString::from(format!("team-act-{}", fid)))
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .border_b_1()
                            .border_color(theme::fg_faint())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()))
                            .on_click(cx.listener(move |this, _ev, _w, cx| {
                                this.open_forecast(&fid, cx);
                            }))
                            .child(
                                div()
                                    .w(px(20.0))
                                    .text_size(px(13.0))
                                    .text_color(rgb(e.color))
                                    .child(e.icon),
                            )
                            .child(
                                div()
                                    .flex_grow()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme::fg())
                                            .child(format!("{}: {}", e.verb, question)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(theme::fg_dim())
                                            .child(e.trailing),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(theme::fg_faint())
                                    .child(time),
                            )
                    })),
            )
            .into_any_element()
    }

    /// Danger zone at the bottom of the team detail pane: a two-step
    /// delete button. First click arms it (red "Really delete?"),
    /// second click fires the DELETE. The arm auto-cancels after 5 s.
    fn render_team_danger_zone(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(tid) = self.selected_team_id.clone() else {
            return div().into_any_element();
        };
        let armed = self.team_delete_confirm_id.as_deref() == Some(tid.as_str());
        let loading = self.team_delete_loading;

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .mt(px(20.0))
            .pt(px(14.0))
            .border_t_1()
            .border_color(theme::fg_faint())
            .child(
                div()
                    .text_size(px(10.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme::red())
                    .child("Danger zone"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .id("team-delete")
                            .px(px(12.0))
                            .py(px(5.0))
                            .rounded(px(4.0))
                            .text_size(px(11.0))
                            .border_1()
                            .cursor_pointer()
                            .when(!armed && !loading, |el| {
                                el.border_color(theme::red())
                                    .text_color(theme::red())
                                    .bg(theme::bg())
                                    .hover(|s| s.bg(theme::bg_hover()))
                            })
                            .when(armed && !loading, |el| {
                                el.border_color(theme::red())
                                    .bg(theme::red())
                                    .text_color(rgb(theme::BG))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .hover(|s| s.opacity(0.85))
                            })
                            .when(loading, |el| {
                                el.border_color(theme::fg_faint())
                                    .text_color(theme::fg_faint())
                            })
                            .on_click(cx.listener(|this, _, _w, cx| {
                                this.delete_selected_team(cx);
                            }))
                            .child(if loading {
                                "Deleting…".to_string()
                            } else if armed {
                                "Really delete? Click again".to_string()
                            } else {
                                "Delete team".to_string()
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child(if armed {
                                "This is irreversible — members lose access, shares become orphaned."
                            } else {
                                "Only the team owner can delete."
                            }),
                    ),
            )
            .into_any_element()
    }

    /// Pending / recent invite list for the selected team. Rendered
    /// below the members roster so the operator sees invites they've
    /// sent (and their status) without having to leave the team panel.
    fn render_team_invites_section(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let pending: Vec<&Invite> = self
            .team_invites
            .iter()
            .filter(|i| i.status == "pending")
            .collect();
        let terminal: Vec<&Invite> = self
            .team_invites
            .iter()
            .filter(|i| i.status != "pending")
            .take(5) // most recent 5 non-pending, purely informational
            .collect();

        let mut container = div().flex().flex_col().gap(px(6.0)).mt(px(8.0)).child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme::cyan())
                        .child(format!("Invites ({} pending)", pending.len())),
                )
                .when(self.team_invites_loading, |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child("loading…"),
                    )
                }),
        );

        if pending.is_empty() && terminal.is_empty() && !self.team_invites_loading {
            container = container.child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme::fg_faint())
                    .child("No invites yet. Use + Invite above to bring someone in."),
            );
            return container;
        }

        for inv in pending.iter().chain(terminal.iter()) {
            let iid = inv.id.clone();
            let in_flight = self.team_invite_revoke_in_flight.contains(&inv.id);
            let is_pending = inv.status == "pending";
            let recipient = inv
                .invitee_display_name
                .clone()
                .or_else(|| inv.invitee_email.clone())
                .or_else(|| inv.invitee_user_id.clone())
                .unwrap_or_else(|| "(unknown)".to_string());
            let status_color = match inv.status.as_str() {
                "pending" => theme::gold(),
                "accepted" => theme::green(),
                "declined" => theme::red(),
                "revoked" => theme::fg_dim(),
                "expired" => theme::fg_faint(),
                _ => theme::fg_dim(),
            };
            let mut row = div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(12.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .bg(theme::bg_elevated())
                .child(div().text_size(px(12.0)).child("✉"))
                .child(
                    div()
                        .flex_grow()
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme::fg())
                                .child(recipient),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme::fg_faint())
                                .child(format!("role: {}", inv.permission)),
                        ),
                )
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .bg(theme::bg_active())
                        .text_size(px(10.0))
                        .text_color(status_color)
                        .child(inv.status.clone()),
                );
            if is_pending {
                // Copy-link affordance. Mirrors the forecast-invite
                // pattern in cockpit.rs `render_forecast_invites_section`.
                // The token is populated only for email/link invites
                // (see fermi/src/handlers/invites.rs `create_invite_row`);
                // direct user-id invites surface in the recipient's
                // Inbox and don't need a link. This is the operator's
                // fallback when `RESEND_API_KEY` isn't configured on
                // the server — the invite row is written and the URL
                // can be sent via any channel.
                if let Some(token) = inv.token.clone() {
                    let base_url = self.api.base_url_sync();
                    let invite_url =
                        format!("{}/invites/{}", base_url.trim_end_matches('/'), token);
                    let url_for_copy = invite_url.clone();
                    row = row.child(
                        div()
                            .id(SharedString::from(format!("tinv-copy-{}", iid)))
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(4.0))
                            .text_size(px(11.0))
                            .text_color(theme::cyan())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()))
                            .on_click(cx.listener(move |this, _, _w, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    url_for_copy.clone(),
                                ));
                                this.show_toast(
                                    "Invite link copied to clipboard",
                                    "🔗",
                                    theme::CYAN,
                                    cx,
                                );
                            }))
                            .child("🔗 Copy link"),
                    );
                    // Show the URL beneath in tiny text so the operator
                    // has visual confirmation of exactly what will be
                    // shared before hitting Copy.
                    row = row.child(
                        div()
                            .text_size(px(9.0))
                            .text_color(theme::fg_faint())
                            .child(invite_url),
                    );
                }
                row = row.child(
                    div()
                        .id(SharedString::from(format!("tinv-revoke-{}", iid)))
                        .px(px(6.0))
                        .py(px(2.0))
                        .rounded(px(4.0))
                        .text_size(px(11.0))
                        .text_color(theme::fg_dim())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::bg_hover()).text_color(theme::red()))
                        .on_click(cx.listener({
                            let iid = iid.clone();
                            move |this, _, _w, cx| {
                                this.revoke_team_invite(iid.clone(), cx);
                            }
                        }))
                        .child(if in_flight { "…" } else { "Revoke" }),
                );
            }
            container = container.child(row);
        }
        container
    }

    fn render_team_create_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0A0E1499))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .w(px(420.0))
                    .p(px(24.0))
                    .rounded(px(12.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .border_1()
                    .border_color(rgb(theme::BLUE))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme::fg())
                            .child("Create Team"),
                    )
                    .child(self.team_create_name_input.clone())
                    .child(self.team_create_slug_input.clone())
                    .when(self.team_create_error.is_some(), |el| {
                        el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::red())
                                .child(self.team_create_error.clone().unwrap_or_default()),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .justify_end()
                            .child(
                                div()
                                    .id("team-create-cancel")
                                    .px(px(14.0))
                                    .py(px(7.0))
                                    .rounded(px(6.0))
                                    .text_size(px(12.0))
                                    .text_color(theme::fg_dim())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::bg_hover()))
                                    .on_click(cx.listener(|this, _, _w, cx| {
                                        this.team_create_showing = false;
                                        this.team_create_error = None;
                                        cx.notify();
                                    }))
                                    .child("Cancel"),
                            )
                            .child(
                                div()
                                    .id("team-create-submit")
                                    .px(px(16.0))
                                    .py(px(7.0))
                                    .rounded(px(6.0))
                                    .bg(theme::blue())
                                    .text_size(px(12.0))
                                    .text_color(rgb(theme::BG))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.85))
                                    .on_click(cx.listener(|this, _, _w, cx| {
                                        this.create_team_from_input(cx);
                                    }))
                                    .child(if self.team_create_loading {
                                        "Creating…"
                                    } else {
                                        "Create"
                                    }),
                            ),
                    ),
            )
    }

    // ── Invite share modal (Sprint A) ─────────────────────────────────────────
    //
    // Pops immediately after creating an invite so the operator has a
    // one-click Copy Link affordance. This is the primary path when
    // the server doesn't have `RESEND_API_KEY` configured (email is a
    // no-op then), but it's also useful when email IS configured —
    // some testers prefer a link they can send via Slack / WhatsApp
    // rather than an email that might land in spam.

    // ── Welcome modal (first-run, post-signup) ──────────────────────
    //
    // Shown once, immediately after a successful first sign-in when
    // the wallet snapshot looks like a fresh onboarding grant (nothing
    // spent, only granted credits). The point is to give testers a
    // concrete anchor for what they can do: "you have N credits,
    // here's roughly what that buys." Dismissed with any of the
    // buttons; won't reappear during this process.
    fn render_welcome_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let credits = self.wallet.as_ref().map(|w| w.balance).unwrap_or(0);
        let name = self
            .user_display_name
            .clone()
            .unwrap_or_else(|| "forecaster".to_string());

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0A0E14CC))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .w(px(500.0))
                    .p(px(28.0))
                    .rounded(px(12.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .border_1()
                    .border_color(rgb(theme::CYAN))
                    .child(
                        div()
                            .text_size(px(22.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme::cyan())
                            .child(format!("Welcome, {}!", name)),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(theme::fg())
                            .child(format!(
                                "You have {} credits to get started. Credits are the \
                                 fuel for AI research agents — every question you \
                                 ask them costs a little.",
                                credits
                            )),
                    )
                    .child(
                        div()
                            .p(px(12.0))
                            .rounded(px(6.0))
                            .bg(rgb(theme::BG))
                            .border_1()
                            .border_color(theme::fg_faint())
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .child("Rough guide:")
                            .child("  • A quick research question: ~2–5 credits")
                            .child("  • A full 4-driver decomposition: ~10–20 credits")
                            .child("  • Reserve budget for iteration — forecasting is")
                            .child("    a conversation, not a one-shot."),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child(
                                "Your balance stays visible in the sidebar. Ask \
                                 the maintainer for a top-up when you run low.",
                            ),
                    )
                    .child(
                        div().flex().justify_end().gap(px(8.0)).child(
                            div()
                                .id("welcome-dismiss")
                                .px(px(18.0))
                                .py(px(8.0))
                                .rounded(px(6.0))
                                .bg(rgb(theme::CYAN))
                                .text_size(px(12.0))
                                .text_color(rgb(theme::BG))
                                .font_weight(FontWeight::SEMIBOLD)
                                .cursor_pointer()
                                .hover(|s| s.opacity(0.85))
                                .on_click(cx.listener(|this, _, _w, cx| {
                                    this.welcome_modal_showing = false;
                                    cx.notify();
                                }))
                                .child("Let's go"),
                        ),
                    ),
            )
    }

    // ── Update modal ──────────────────────────────────────────────────────────
    //
    // Rendered only when `self.update_modal_showing && self.available_update.is_some()`.
    // Three visual states, driven by `self.update_download`:
    //
    //   Idle          → release notes + "Update & Restart" primary button
    //   Downloading   → progress bar with received/total bytes
    //   Installing    → spinner ("Installing…")
    //   Restarting    → "Restarting…" — process exits during this state
    //   Failed(msg)   → red error banner + "Try again" button
    fn render_update_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let release = self
            .available_update
            .as_ref()
            .expect("render_update_modal called with no update");
        let current = env!("CARGO_PKG_VERSION");

        // Format the notes: keep it readable but truncate absurdly long
        // release bodies so the modal never grows past the window. If
        // testers need the full text, they can hit the GitHub link.
        let notes_display = if release.notes.len() > 2400 {
            format!(
                "{}…\n\n(truncated — see GitHub for full notes)",
                &release.notes[..2400]
            )
        } else {
            release.notes.clone()
        };

        // Progress row content depends on state.
        let (progress_row, primary_label, primary_enabled, primary_color) =
            match &self.update_download {
                updater::DownloadState::Idle => {
                    (None, "Update & Restart".to_string(), true, theme::CYAN)
                }
                updater::DownloadState::Downloading { received, total } => {
                    let pct = if *total > 0 {
                        ((*received as f64 / *total as f64) * 100.0).clamp(0.0, 100.0)
                    } else {
                        0.0
                    };
                    let mb_recv = *received as f64 / 1_048_576.0;
                    let mb_total = *total as f64 / 1_048_576.0;
                    let label = if *total > 0 {
                        format!(
                            "Downloading… {:.1} / {:.1} MB ({:.0}%)",
                            mb_recv, mb_total, pct
                        )
                    } else {
                        format!("Downloading… {:.1} MB", mb_recv)
                    };
                    let bar = div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::fg_dim())
                                .child(label),
                        )
                        .child(
                            div()
                                .h(px(6.0))
                                .w_full()
                                .rounded(px(3.0))
                                .bg(rgb(theme::BG))
                                .border_1()
                                .border_color(theme::fg_faint())
                                .child(
                                    div()
                                        .h_full()
                                        .w(gpui::relative(pct as f32 / 100.0))
                                        .bg(rgb(theme::CYAN))
                                        .rounded(px(3.0)),
                                ),
                        );
                    (Some(bar), "Downloading…".to_string(), false, theme::FG_DIM)
                }
                updater::DownloadState::Installing => (
                    Some(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .child("Installing…"),
                    ),
                    "Installing…".to_string(),
                    false,
                    theme::FG_DIM,
                ),
                updater::DownloadState::Restarting => (
                    Some(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::green())
                            .child("Restarting…"),
                    ),
                    "Restarting…".to_string(),
                    false,
                    theme::FG_DIM,
                ),
                updater::DownloadState::Failed(msg) => (
                    Some(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::red())
                            .child(format!("✗ {}", msg)),
                    ),
                    "Try again".to_string(),
                    true,
                    theme::GOLD,
                ),
            };

        let release_url = format!(
            "https://github.com/{}/releases/tag/{}",
            std::env::var("FERMI_UPDATE_REPO")
                .unwrap_or_else(|_| "Replicant-Partners/fermi".to_string()),
            release.tag
        );
        let release_url_copy = release_url.clone();

        let mut card = div()
            .flex()
            .flex_col()
            .gap(px(14.0))
            .w(px(600.0))
            .max_h(px(640.0))
            .p(px(24.0))
            .rounded(px(12.0))
            .bg(rgb(theme::BG_ELEVATED))
            .border_1()
            .border_color(rgb(theme::CYAN))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().text_size(px(20.0)).child("⬆"))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme::fg())
                            .child(format!("Fermi Console {} is available", release.tag)),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme::fg_dim())
                    .child(format!(
                        "You're on v{}. New build published {}.",
                        current,
                        pretty_timestamp(&release.published_at)
                    )),
            )
            // Release notes body — scrollable, monospace to preserve
            // any markdown formatting the author included.
            .child(
                div()
                    .id("update-modal-notes")
                    .overflow_y_scroll()
                    .max_h(px(340.0))
                    .p(px(12.0))
                    .rounded(px(6.0))
                    .bg(rgb(theme::BG))
                    .border_1()
                    .border_color(theme::fg_faint())
                    .text_size(px(11.0))
                    .text_color(theme::fg())
                    .child(notes_display),
            );

        if let Some(row) = progress_row {
            card = card.child(row);
        }

        // Action row.
        card = card.child(
            div()
                .flex()
                .gap(px(8.0))
                .items_center()
                .child(
                    // View on GitHub link — always available so testers
                    // can eyeball what's actually shipping.
                    div()
                        .id("update-modal-github")
                        .px(px(12.0))
                        .py(px(7.0))
                        .rounded(px(6.0))
                        .text_size(px(11.0))
                        .text_color(theme::fg_dim())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::bg_hover()))
                        .on_click(move |_, _w, _cx| {
                            let _ = open::that(release_url_copy.clone());
                        })
                        .child("View on GitHub ↗"),
                )
                .child(div().flex_grow()) // spacer
                .child(
                    div()
                        .id("update-modal-later")
                        .px(px(14.0))
                        .py(px(7.0))
                        .rounded(px(6.0))
                        .text_size(px(12.0))
                        .text_color(theme::fg_dim())
                        .cursor_pointer()
                        .hover(|s| s.bg(theme::bg_hover()))
                        .on_click(cx.listener(|this, _, _w, cx| {
                            this.update_modal_showing = false;
                            cx.notify();
                        }))
                        .child("Remind me later"),
                )
                .child({
                    let mut btn = div()
                        .id("update-modal-primary")
                        .px(px(16.0))
                        .py(px(7.0))
                        .rounded(px(6.0))
                        .bg(rgb(primary_color))
                        .text_size(px(12.0))
                        .text_color(rgb(theme::BG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(primary_label);
                    if primary_enabled {
                        btn =
                            btn.cursor_pointer()
                                .hover(|s| s.opacity(0.85))
                                .on_click(cx.listener(|this, _, _w, cx| {
                                    this.perform_update(cx);
                                }));
                    } else {
                        btn = btn.opacity(0.6);
                    }
                    btn
                }),
        );

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0A0E14CC))
            .child(card)
    }

    // ── Shortcuts help modal ─────────────────────────────────────────────────────
    //
    // Rendered when `self.shortcuts_modal_showing` is true. The list
    // below is the single source of truth for what the console tells
    // operators about its hotkeys — keep it in sync with the
    // `cx.bind_keys([...])` block in `main()` and with `build_menus()`.
    // When you add a new binding, add a row here too.
    fn render_shortcuts_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // ((section label, [(keys, description)])) tuples.
        // Ordered top-to-bottom by frequency-of-use so the shortcuts
        // an operator hits every session are at the top of the modal.
        type Row = (&'static str, &'static str);
        let sections: Vec<(&'static str, Vec<Row>)> = vec![
            (
                "Forecast workflow",
                vec![
                    ("Ctrl+Enter", "Research question → draft forecast"),
                    ("Ctrl+R", "Run Monte Carlo simulation"),
                    ("Ctrl+P", "Publish forecast"),
                    ("Ctrl+S", "Save forecast (local draft)"),
                    ("Ctrl+O", "Import forecast from file"),
                    ("Ctrl+E", "Toggle FPL source view"),
                ],
            ),
            (
                "Navigation",
                vec![
                    ("Ctrl+1", "Dashboard"),
                    ("Ctrl+2", "Portfolio"),
                    ("Ctrl+3", "Agent Fleet"),
                    ("Ctrl+4", "Composer"),
                    ("Ctrl+5", "Leaderboard"),
                    ("Ctrl+6", "Teams"),
                    ("Ctrl+N", "New forecast"),
                    ("↑ / ↓", "Cycle drivers (in Composer)"),
                ],
            ),
            (
                "Window",
                vec![
                    ("Ctrl+M", "Minimize"),
                    ("Ctrl+Shift+F", "Toggle fullscreen"),
                    ("Ctrl+Q", "Quit Fermi Console"),
                ],
            ),
            (
                "Help",
                vec![
                    ("Ctrl+/", "Show this shortcuts panel"),
                    ("Esc", "Dismiss any modal / overlay"),
                ],
            ),
        ];

        let render_row = |keys: &'static str, desc: &'static str| {
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .py(px(4.0))
                .child(
                    // Key pill — fixed-width so descriptions align in
                    // a clean column regardless of chord length.
                    div()
                        .flex_none()
                        .w(px(140.0))
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(4.0))
                        .bg(rgb(theme::BG))
                        .border_1()
                        .border_color(theme::fg_faint())
                        .text_size(px(11.0))
                        .text_color(theme::cyan())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(keys),
                )
                .child(
                    div()
                        .flex_grow()
                        .text_size(px(12.0))
                        .text_color(theme::fg())
                        .child(desc),
                )
        };

        let mut body = div().flex().flex_col().gap(px(18.0));
        for (label, rows) in sections {
            let mut section = div().flex().flex_col().gap(px(6.0)).child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::fg_dim())
                    .font_weight(FontWeight::BOLD)
                    // Poor-man's letter-spacing via a manual
                    // uppercase transform; GPUI has no CSS-style
                    // letter-spacing prop, so uppercase alone
                    // gives the section header a distinct look.
                    .child(label.to_uppercase()),
            );
            for (keys, desc) in rows {
                section = section.child(render_row(keys, desc));
            }
            body = body.child(section);
        }

        let card = div()
            .id("shortcuts-modal-card")
            .flex()
            .flex_col()
            .gap(px(16.0))
            .w(px(560.0))
            .max_h(px(660.0))
            .p(px(24.0))
            .rounded(px(12.0))
            .bg(rgb(theme::BG_ELEVATED))
            .border_1()
            .border_color(rgb(theme::CYAN))
            // Header row — title + close button.
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().text_size(px(20.0)).child("⌨"))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme::fg())
                            .child("Keyboard shortcuts"),
                    )
                    .child(div().flex_grow())
                    .child(
                        div()
                            .id("shortcuts-modal-close")
                            .px(px(10.0))
                            .py(px(4.0))
                            .rounded(px(4.0))
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|s| s.bg(theme::bg_hover()).text_color(theme::fg()))
                            .on_click(cx.listener(|this, _, _w, cx| {
                                this.shortcuts_modal_showing = false;
                                cx.notify();
                            }))
                            .child("Close  ✕"),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme::fg_dim())
                    .child("Ctrl maps to ⌘ on macOS. Press Ctrl+/ any time to reopen this panel."),
            )
            // Scrollable body so a growing shortcut list never blows
            // past the modal height.
            .child(
                div()
                    .id("shortcuts-modal-body")
                    .overflow_y_scroll()
                    .max_h(px(480.0))
                    .pr(px(8.0))
                    .child(body),
            );

        // Full-window backdrop; clicking it dismisses the modal so the
        // operator has a mouse-first way out that mirrors the Esc
        // keybinding.
        div()
            .id("shortcuts-modal-backdrop")
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0A0E14CC))
            .on_click(cx.listener(|this, _, _w, cx| {
                this.shortcuts_modal_showing = false;
                cx.notify();
            }))
            .child(
                // Stop backdrop-dismiss from firing when the operator
                // clicks inside the card itself.
                div()
                    .id("shortcuts-modal-inner")
                    .on_click(cx.listener(|_, _, _w, cx| {
                        cx.stop_propagation();
                    }))
                    .child(card),
            )
    }

    fn render_invite_share_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // Only rendered when `self.invite_share_modal.is_some()`; caller
        // is responsible for gating.
        let modal = self
            .invite_share_modal
            .as_ref()
            .expect("render_invite_share_modal called with no modal state");
        let invite_url = modal.invite_url.clone();
        let url_for_copy = modal.invite_url.clone();
        let email_line = if modal.email_sent {
            format!(
                "✉ Emailed to {}. You can also share this link directly.",
                modal.recipient
            )
        } else {
            format!(
                "⚠ Email delivery not configured on the server. Share this link with {} directly.",
                modal.recipient
            )
        };
        let email_color = if modal.email_sent {
            theme::green()
        } else {
            theme::gold()
        };

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0A0E14CC))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .w(px(520.0))
                    .p(px(24.0))
                    .rounded(px(12.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .border_1()
                    .border_color(rgb(theme::CYAN))
                    // Header
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(div().text_size(px(18.0)).child("🔗"))
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme::fg())
                                    .child("Invite ready to share"),
                            ),
                    )
                    // Subtitle: target label + permission.
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .child(format!(
                                "{} • {} access",
                                modal.target_label, modal.permission
                            )),
                    )
                    // Email status line — green when Resend fired,
                    // gold otherwise ("share this link directly").
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(email_color)
                            .child(email_line),
                    )
                    // The link itself, in a bordered read-only field.
                    .child(
                        div()
                            .px(px(12.0))
                            .py(px(10.0))
                            .rounded(px(6.0))
                            .bg(rgb(theme::BG))
                            .border_1()
                            .border_color(theme::fg_faint())
                            .text_size(px(10.0))
                            .text_color(theme::cyan())
                            .child(invite_url.clone()),
                    )
                    // Action row: Copy link + Dismiss.
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .justify_end()
                            .child(
                                div()
                                    .id("invite-modal-dismiss")
                                    .px(px(14.0))
                                    .py(px(7.0))
                                    .rounded(px(6.0))
                                    .text_size(px(12.0))
                                    .text_color(theme::fg_dim())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::bg_hover()))
                                    .on_click(cx.listener(|this, _, _w, cx| {
                                        this.invite_share_modal = None;
                                        cx.notify();
                                    }))
                                    .child("Done"),
                            )
                            .child(
                                div()
                                    .id("invite-modal-copy")
                                    .px(px(16.0))
                                    .py(px(7.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(theme::CYAN))
                                    .text_size(px(12.0))
                                    .text_color(rgb(theme::BG))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.85))
                                    .on_click(cx.listener(move |this, _, _w, cx| {
                                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                            url_for_copy.clone(),
                                        ));
                                        this.show_toast(
                                            "Invite link copied to clipboard",
                                            "🔗",
                                            theme::CYAN,
                                            cx,
                                        );
                                    }))
                                    .child("Copy link"),
                            ),
                    ),
            )
    }

    // ── Hire modal (Sprint C polish) ────────────────────────────────────────
    //
    // Three-tier modal for the agent hire flow. Step 1: pick a
    // forecast (from the operator's active book). Step 2: pick a
    // driver within that forecast (or none, for ambient agents).
    // Step 3: review the terms placeholder and confirm.
    //
    // Confirm currently surfaces a hint and navigates to the Composer
    // — full wiring (server-side agent-to-driver binding + auto-fire)
    // is a follow-up that touches the cockpit's `assign_agent_to_driver`
    // path.

    fn render_hire_modal(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let modal = self.hire_modal.as_ref().expect("gated by caller");

        // Stepper header.
        let step_labels = ["1  Forecast", "2  Driver", "3  Terms"];
        let mut stepper = div().flex().items_center().gap(px(8.0));
        for (i, label) in step_labels.iter().enumerate() {
            let n = (i + 1) as u8;
            let is_active = modal.step == n;
            let is_done = modal.step > n;
            let color = if is_active {
                theme::CYAN
            } else if is_done {
                theme::GREEN
            } else {
                theme::FG_FAINT
            };
            stepper = stepper.child(
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(color))
                    .font_weight(if is_active {
                        FontWeight::BOLD
                    } else {
                        FontWeight::MEDIUM
                    })
                    .child(label.to_string()),
            );
            if i < step_labels.len() - 1 {
                stepper = stepper.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme::fg_faint())
                        .child("›"),
                );
            }
        }

        // Body per step.
        let body: AnyElement = match modal.step {
            1 => self.render_hire_step_forecast(cx).into_any_element(),
            2 => self.render_hire_step_driver(cx).into_any_element(),
            _ => self.render_hire_step_terms(cx).into_any_element(),
        };

        // Footer: Back / Cancel / Next|Confirm.
        let can_advance = match modal.step {
            1 => modal.forecast_id.is_some(),
            _ => true,
        };
        let next_label = if modal.step == 3 {
            "Confirm hire"
        } else {
            "Next →"
        };
        let back_visible = modal.step > 1;

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0A0E14CC))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .w(px(560.0))
                    .max_h(px(600.0))
                    .p(px(24.0))
                    .rounded(px(12.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .border_1()
                    .border_color(rgb(theme::CYAN))
                    // Header
                    .child(
                        div().flex().items_center().gap(px(8.0)).child(
                            div()
                                .text_size(px(16.0))
                                .font_weight(FontWeight::BOLD)
                                .text_color(theme::fg())
                                .child(format!("Hire {}", modal.agent_display)),
                        ),
                    )
                    .child(stepper)
                    // Scrollable body — GPUI requires .id() on any element
                    // using .overflow_y_scroll() (needs a stable identity
                    // to persist scroll position across renders).
                    .child(
                        div()
                            .id("hire-modal-body")
                            .flex_grow()
                            .overflow_y_scroll()
                            .child(body),
                    )
                    // Footer
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .id("hire-cancel")
                                    .px(px(14.0))
                                    .py(px(6.0))
                                    .rounded(px(6.0))
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_dim())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::bg_hover()))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.hire_modal = None;
                                        cx.notify();
                                    }))
                                    .child("Cancel"),
                            )
                            .child(div().flex_grow())
                            .when(back_visible, |el| {
                                el.child(
                                    div()
                                        .id("hire-back")
                                        .px(px(12.0))
                                        .py(px(6.0))
                                        .rounded(px(6.0))
                                        .border_1()
                                        .border_color(theme::fg_faint())
                                        .text_size(px(11.0))
                                        .text_color(theme::fg_dim())
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme::bg_hover()))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            if let Some(m) = this.hire_modal.as_mut() {
                                                m.step = (m.step - 1).max(1);
                                                cx.notify();
                                            }
                                        }))
                                        .child("← Back"),
                                )
                            })
                            .child(
                                div()
                                    .id("hire-next")
                                    .px(px(16.0))
                                    .py(px(6.0))
                                    .rounded(px(6.0))
                                    .bg(if can_advance {
                                        rgb(theme::CYAN)
                                    } else {
                                        rgb(theme::BG_ACTIVE)
                                    })
                                    .text_size(px(11.0))
                                    .text_color(if can_advance {
                                        rgb(theme::BG)
                                    } else {
                                        rgb(theme::FG_FAINT)
                                    })
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .when(can_advance, |s| {
                                        s.cursor_pointer().hover(|s| s.opacity(0.85))
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.advance_hire_modal(cx);
                                    }))
                                    .child(next_label),
                            ),
                    ),
            )
    }

    fn render_hire_step_forecast(&self, cx: &Context<Self>) -> impl IntoElement {
        let modal = self.hire_modal.as_ref().expect("gated by caller");
        // Forecast picker — from the operator's active book, sorted by
        // most-recent-activity.
        let mut forecasts: Vec<&Forecast> = self.active_forecasts.iter().collect();
        forecasts.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        let mut container = div().flex().flex_col().gap(px(4.0)).child(
            div()
                .text_size(px(11.0))
                .text_color(theme::fg_dim())
                .child("Pick the forecast this agent should research for."),
        );

        if forecasts.is_empty() {
            container = container.child(
                div()
                    .px(px(10.0))
                    .py(px(10.0))
                    .rounded(px(6.0))
                    .bg(theme::bg())
                    .border_1()
                    .border_color(theme::fg_faint())
                    .text_size(px(11.0))
                    .text_color(theme::fg_faint())
                    .child(
                        "No active forecasts yet. Start a forecast in the Composer, \
                         then come back to hire this agent.",
                    ),
            );
            return container;
        }

        for f in forecasts {
            let fid = f.id.clone();
            let is_selected = modal.forecast_id.as_deref() == Some(&fid);
            let label = truncate(&f.question_text, 76);
            let fid_for_pick = fid.clone();
            let label_for_pick = label.clone();
            container = container.child(
                div()
                    .id(SharedString::from(format!("hire-fc-{}", fid)))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(if is_selected {
                        theme::cyan()
                    } else {
                        theme::fg_faint()
                    })
                    .bg(if is_selected {
                        theme::bg_active()
                    } else {
                        theme::bg_elevated()
                    })
                    .cursor_pointer()
                    .hover(|s| s.bg(theme::bg_hover()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if let Some(m) = this.hire_modal.as_mut() {
                            m.forecast_id = Some(fid_for_pick.clone());
                            m.forecast_label = Some(label_for_pick.clone());
                            // Advance immediately — selection IS the
                            // action, saves one click.
                            m.step = 2;
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(if is_selected {
                                theme::cyan()
                            } else {
                                theme::fg_dim()
                            })
                            .child(if is_selected { "◉" } else { "○" }),
                    )
                    .child(
                        div()
                            .flex_grow()
                            .text_size(px(12.0))
                            .text_color(theme::fg())
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child(format!("{:.0}%", f.predicted_probability * 100.0)),
                    ),
            );
        }

        container
    }

    fn render_hire_step_driver(&self, cx: &Context<Self>) -> impl IntoElement {
        let modal = self.hire_modal.as_ref().expect("gated by caller");

        // Read the drivers from the currently-open cockpit if the
        // chosen forecast matches. If not, we can't enumerate drivers
        // yet (server-side driver-list endpoint would be needed);
        // fall back to allowing an ambient (no-driver) binding.
        let cockpit_forecast_id = self
            .cockpit
            .as_ref()
            .and_then(|c| c.read(cx).forecast_id.clone());
        let same_forecast = cockpit_forecast_id.as_deref() == modal.forecast_id.as_deref();

        let mut container = div().flex().flex_col().gap(px(4.0)).child(
            div().text_size(px(11.0)).text_color(theme::fg_dim()).child(
                "Bind the agent to a driver, or hire it as an ambient research \
                     agent (no driver — the agent adds evidence to the forecast at large).",
            ),
        );

        // Ambient option always present.
        let ambient_selected = modal.driver_name.is_none();
        container = container.child(
            div()
                .id("hire-drv-ambient")
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(if ambient_selected {
                    theme::cyan()
                } else {
                    theme::fg_faint()
                })
                .bg(if ambient_selected {
                    theme::bg_active()
                } else {
                    theme::bg_elevated()
                })
                .cursor_pointer()
                .hover(|s| s.bg(theme::bg_hover()))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(m) = this.hire_modal.as_mut() {
                        m.driver_name = None;
                    }
                    cx.notify();
                }))
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(if ambient_selected {
                            theme::cyan()
                        } else {
                            theme::fg_dim()
                        })
                        .child(if ambient_selected { "◉" } else { "○" }),
                )
                .child(
                    div()
                        .flex_grow()
                        .text_size(px(12.0))
                        .text_color(theme::fg())
                        .child("Ambient research agent (no specific driver)"),
                ),
        );

        // Driver list from the cockpit (best-effort). If the operator
        // picked a forecast that's not the one currently open, we
        // surface an informational note and let them proceed via
        // ambient. Wiring "switch cockpit to the picked forecast" is a
        // follow-up.
        if same_forecast {
            if let Some(cockpit) = self.cockpit.as_ref() {
                let drivers: Vec<String> = {
                    let state = cockpit.read(cx);
                    state
                        .program
                        .drivers()
                        .iter()
                        .map(|d| d.name.clone())
                        .collect()
                };
                if drivers.is_empty() {
                    container = container.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child(
                                "This forecast has no drivers yet — hire ambient, \
                                 then add drivers in the Composer.",
                            ),
                    );
                } else {
                    for driver in drivers {
                        let is_selected = modal.driver_name.as_deref() == Some(&driver);
                        let d_for_pick = driver.clone();
                        container = container.child(
                            div()
                                .id(SharedString::from(format!("hire-drv-{}", driver)))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .px(px(10.0))
                                .py(px(6.0))
                                .rounded(px(6.0))
                                .border_1()
                                .border_color(if is_selected {
                                    theme::cyan()
                                } else {
                                    theme::fg_faint()
                                })
                                .bg(if is_selected {
                                    theme::bg_active()
                                } else {
                                    theme::bg_elevated()
                                })
                                .cursor_pointer()
                                .hover(|s| s.bg(theme::bg_hover()))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    if let Some(m) = this.hire_modal.as_mut() {
                                        m.driver_name = Some(d_for_pick.clone());
                                    }
                                    cx.notify();
                                }))
                                .child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(if is_selected {
                                            theme::cyan()
                                        } else {
                                            theme::fg_dim()
                                        })
                                        .child(if is_selected { "◉" } else { "○" }),
                                )
                                .child(
                                    div()
                                        .flex_grow()
                                        .text_size(px(12.0))
                                        .text_color(theme::fg())
                                        .child(driver),
                                ),
                        );
                    }
                }
            }
        } else {
            container = container.child(div().text_size(px(10.0)).text_color(theme::gold()).child(
                "Driver list requires the picked forecast to be open in the \
                         Composer. For now hire ambient, then open the forecast to \
                         assign the driver via + Assign Agent.",
            ));
        }

        container
    }

    fn render_hire_step_terms(&self, _cx: &Context<Self>) -> impl IntoElement {
        let modal = self.hire_modal.as_ref().expect("gated by caller");
        let forecast_label = modal
            .forecast_label
            .clone()
            .unwrap_or_else(|| "(none)".into());
        let driver_label = modal
            .driver_name
            .clone()
            .unwrap_or_else(|| "(ambient — no driver)".into());

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            // Contract summary.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(6.0))
                    .bg(theme::bg())
                    .border_1()
                    .border_color(theme::fg_faint())
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(theme::fg_faint())
                            .child("HIRE SUMMARY"),
                    )
                    .child(render_detail_kv("Agent", &modal.agent_display))
                    .child(render_detail_kv("Forecast", &forecast_label))
                    .child(render_detail_kv("Driver", &driver_label)),
            )
            // Terms placeholder — will read from the agent's card in a
            // follow-up (fork_pricing / requires_secrets / auto_collect_pct).
            // For now surface the defaults so the tester sees the shape.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(6.0))
                    .bg(theme::bg())
                    .border_1()
                    .border_color(theme::fg_faint())
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(theme::fg_faint())
                            .child("TERMS"),
                    )
                    .child(div().text_size(px(11.0)).text_color(theme::fg_dim()).child(
                        "Placeholder. When ABW ships fork_pricing / royalty terms \
                                 per agent, this section will surface them (per-run credit \
                                 cost, royalty to author, cap per session). For now the \
                                 default is: pay-per-run at the agent's model cost, no \
                                 royalties, no cap.",
                    ))
                    .child(div().text_size(px(10.0)).text_color(theme::gold()).child(
                        "⚠ This hire flow's binding step is scaffolded but the \
                                 auto-assign wiring lands in a follow-up. Confirm below \
                                 will drop a hint into the Composer so you can complete \
                                 the assignment via + Assign Agent.",
                    )),
            )
            // Notes textbox.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(theme::fg_faint())
                            .child("NOTES"),
                    )
                    .child(modal.notes.clone()),
            )
    }

    // ── Inbox sheet (Spec 24 §3.5.5) ─────────────────────────────────────────

    fn render_inbox_sheet(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0A0E1499))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .w(px(520.0))
                    .max_h(px(560.0))
                    .p(px(24.0))
                    .rounded(px(12.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .border_1()
                    .border_color(rgb(theme::CYAN))
                    // Header
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(theme::fg())
                                    .child("📥 Inbox"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_dim())
                                    .child(format!("{} pending", self.inbox_invites.len())),
                            )
                            .child(div().flex_grow())
                            .child(
                                div()
                                    .id("inbox-close")
                                    .px(px(8.0))
                                    .py(px(4.0))
                                    .rounded(px(4.0))
                                    .text_size(px(12.0))
                                    .text_color(theme::fg_dim())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(theme::bg_hover()))
                                    .on_click(cx.listener(|this, _, _w, cx| {
                                        this.inbox_sheet_showing = false;
                                        cx.notify();
                                    }))
                                    .child("✕ Close"),
                            ),
                    )
                    .when(self.inbox_invites.is_empty() && !self.inbox_loading, |el| {
                        el.child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme::fg_dim())
                                .child("No pending invites."),
                        )
                    })
                    // Invite rows
                    .child(
                        div()
                            .id("inbox-list")
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .overflow_y_scroll()
                            .children(self.inbox_invites.iter().map(|inv| {
                                let iid = inv.id.clone();
                                let in_flight = self.inbox_action_in_flight.contains(&inv.id);
                                let (icon, kind) = match inv.target_type.as_str() {
                                    "team" => ("👥", "team"),
                                    "portfolio" => ("◈", "portfolio"),
                                    _ => ("◎", "forecast"),
                                };
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .p(px(12.0))
                                    .rounded(px(8.0))
                                    .bg(rgb(theme::BG))
                                    .border_1()
                                    .border_color(theme::fg_faint())
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(8.0))
                                            .child(div().text_size(px(14.0)).child(icon))
                                            .child(
                                                div()
                                                    .flex_grow()
                                                    .text_size(px(12.0))
                                                    .text_color(theme::fg())
                                                    .child(format!(
                                                        "{} · {} access",
                                                        kind, inv.permission
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(theme::fg_faint())
                                                    .child(format!(
                                                        "from {}",
                                                        inv.inviter_display_name
                                                            .clone()
                                                            .unwrap_or_else(|| short_user_label(
                                                                &inv.inviter_id
                                                            ))
                                                    )),
                                            ),
                                    )
                                    .when(inv.message.is_some(), |el| {
                                        el.child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(theme::fg_dim())
                                                .child(inv.message.clone().unwrap_or_default()),
                                        )
                                    })
                                    // Action buttons
                                    .child(
                                        div()
                                            .flex()
                                            .gap(px(8.0))
                                            .justify_end()
                                            .child(
                                                div()
                                                    .id(SharedString::from(format!(
                                                        "inv-decline-{}",
                                                        iid
                                                    )))
                                                    .px(px(12.0))
                                                    .py(px(5.0))
                                                    .rounded(px(4.0))
                                                    .text_size(px(11.0))
                                                    .text_color(theme::fg_dim())
                                                    .cursor_pointer()
                                                    .hover(|s| {
                                                        s.bg(theme::bg_hover())
                                                            .text_color(theme::red())
                                                    })
                                                    .on_click(cx.listener({
                                                        let iid = iid.clone();
                                                        move |this, _, _w, cx| {
                                                            this.decline_invite(iid.clone(), cx);
                                                        }
                                                    }))
                                                    .child(if in_flight {
                                                        "…"
                                                    } else {
                                                        "Decline"
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .id(SharedString::from(format!(
                                                        "inv-accept-{}",
                                                        iid
                                                    )))
                                                    .px(px(14.0))
                                                    .py(px(5.0))
                                                    .rounded(px(4.0))
                                                    .bg(theme::green())
                                                    .text_size(px(11.0))
                                                    .text_color(rgb(theme::BG))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .cursor_pointer()
                                                    .hover(|s| s.opacity(0.85))
                                                    .on_click(cx.listener({
                                                        let iid = iid.clone();
                                                        move |this, _, _w, cx| {
                                                            this.accept_invite(iid.clone(), cx);
                                                        }
                                                    }))
                                                    .child(if in_flight {
                                                        "…"
                                                    } else {
                                                        "Accept"
                                                    }),
                                            ),
                                    )
                            })),
                    ),
            )
    }

    fn render_leaderboard_panel(&self) -> impl IntoElement {
        div()
            .id("leaderboard-panel")
            .flex()
            .flex_col()
            .size_full()
            .overflow_y_scroll()
            // Header
            .child(
                div()
                    .px(px(24.0))
                    .py(px(16.0))
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(20.0))
                            .text_color(theme::gold())
                            .font_weight(FontWeight::BOLD)
                            .child("⚑ Leaderboard"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .child(format!("{} forecasters", self.leaderboard.len())),
                    )
                    .when(self.leaderboard_loading, |el| {
                        el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::gold())
                                .child("⟳ Loading…"),
                        )
                    })
                    // My rank badge
                    .when(self.my_stats.is_some(), |el| {
                        let rank = self
                            .my_stats
                            .as_ref()
                            .and_then(|s| s.rank)
                            .map(|r| format!("Your rank: #{}", r))
                            .unwrap_or_else(|| "Unranked".into());
                        el.child(div().flex_grow()).child(
                            div()
                                .text_size(px(12.0))
                                .text_color(theme::cyan())
                                .px(px(10.0))
                                .py(px(4.0))
                                .rounded(px(4.0))
                                .bg(theme::bg_active())
                                .child(rank),
                        )
                    }),
            )
            // Column headers
            .child(
                div()
                    .flex()
                    .items_center()
                    .px(px(24.0))
                    .py(px(8.0))
                    .bg(theme::bg_deep())
                    .border_b_1()
                    .border_color(theme::fg_faint())
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .child(div().w(px(40.0)).child("Rank"))
                    .child(div().flex_grow().child("Forecaster"))
                    .child(div().w(px(70.0)).text_right().child("Resolved"))
                    .child(div().w(px(80.0)).text_right().child("Avg Brier"))
                    .child(div().w(px(80.0)).text_right().child("Best"))
                    .child(div().w(px(80.0)).text_right().child("Calibration")),
            )
            // Leaderboard rows
            .child(
                div()
                    .flex()
                    .flex_col()
                    .children(
                        self.leaderboard
                            .iter()
                            .enumerate()
                            .map(|(i, entry)| self.render_leaderboard_row(i, entry)),
                    )
                    .when(
                        self.leaderboard.is_empty() && !self.leaderboard_loading,
                        |el| {
                            el.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .py(px(48.0))
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .text_color(theme::fg_dim())
                                            .child("No leaderboard data"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme::fg_faint())
                                            .mt(px(4.0))
                                            .child(
                                                "Resolve forecasts to appear on the leaderboard",
                                            ),
                                    ),
                            )
                        },
                    ),
            )
            // Calibration legend
            .child(
                div()
                    .px(px(24.0))
                    .py(px(12.0))
                    .border_t_1()
                    .border_color(theme::fg_faint())
                    .flex()
                    .gap(px(16.0))
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .child("Brier score: 0.0 = perfect, 0.25 = coin flip, lower is better")
                    .child("Min 3 resolved forecasts to rank"),
            )
    }

    fn render_leaderboard_row(&self, index: usize, entry: &LeaderboardEntry) -> impl IntoElement {
        let rank = entry.rank.unwrap_or(index as i64 + 1);
        let name = entry.display_name.as_deref().unwrap_or("Anonymous");
        let resolved = entry.total_resolved.unwrap_or(0);
        let avg_brier = entry.avg_brier_score.unwrap_or(0.0);
        let best_brier = entry.best_brier_score.unwrap_or(0.0);

        let is_me = self
            .my_stats
            .as_ref()
            .map(|s| s.owner_id == entry.owner_id.as_deref().unwrap_or(""))
            .unwrap_or(false);

        let rank_color = match rank {
            1 => theme::GOLD,
            2 => theme::FG,
            3 => theme::ORANGE,
            _ => theme::FG_DIM,
        };

        let brier_color = if avg_brier < 0.1 {
            theme::GREEN
        } else if avg_brier < 0.2 {
            theme::CYAN
        } else if avg_brier < 0.3 {
            theme::GOLD
        } else {
            theme::RED
        };

        // Simple calibration indicator from calibration data
        let cal_indicator = entry
            .calibration
            .as_ref()
            .map(|c| render_calibration_mini(c))
            .unwrap_or_else(|| "—".to_string());

        div()
            .flex()
            .items_center()
            .px(px(24.0))
            .py(px(10.0))
            .border_b_1()
            .border_color(theme::fg_faint())
            .hover(|s| s.bg(theme::bg_hover()))
            .when(is_me, |el| el.bg(theme::bg_active()))
            // Rank
            .child(
                div()
                    .w(px(40.0))
                    .text_size(px(14.0))
                    .text_color(rgb(rank_color))
                    .font_weight(FontWeight::BOLD)
                    .child(format!("#{}", rank)),
            )
            // Name
            .child(
                div().flex_grow().flex().flex_col().child(
                    div()
                        .text_size(px(13.0))
                        .text_color(if is_me { theme::cyan() } else { theme::fg() })
                        .font_weight(if is_me {
                            FontWeight::BOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .child(format!("{}{}", name, if is_me { " (you)" } else { "" })),
                ),
            )
            // Resolved count
            .child(
                div()
                    .w(px(70.0))
                    .text_size(px(12.0))
                    .text_color(theme::fg_dim())
                    .text_right()
                    .child(format!("{}", resolved)),
            )
            // Avg Brier
            .child(
                div()
                    .w(px(80.0))
                    .text_size(px(13.0))
                    .text_color(rgb(brier_color))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_right()
                    .child(format!("{:.3}", avg_brier)),
            )
            // Best Brier
            .child(
                div()
                    .w(px(80.0))
                    .text_size(px(12.0))
                    .text_color(theme::fg_dim())
                    .text_right()
                    .child(format!("{:.3}", best_brier)),
            )
            // Calibration mini
            .child(
                div()
                    .w(px(80.0))
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .text_right()
                    .child(cal_indicator),
            )
    }

    // ── Commit Sheet Modal ────────────────────────────────────────────────

    fn render_commit_sheet(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let question = truncate(&self.commit_sheet_question, 80);
        let prob_pct = (self.commit_sheet_probability * 100.0).round() as u32;
        let selected_vis = self.commit_sheet_visibility.clone();

        // Full-screen scrim
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0A0E1499))
            // Sheet card
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    .w(px(480.0))
                    .p(px(28.0))
                    .rounded(px(12.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .border_1()
                    .border_color(rgb(theme::CYAN))
                    // Header
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(theme::CYAN))
                                    .child("Commit Forecast"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_faint())
                                    .child("Once committed, this forecast enters Brier scoring."),
                            ),
                    )
                    // Question + probability summary
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .p(px(12.0))
                            .rounded(px(6.0))
                            .bg(rgb(theme::BG))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .text_color(rgb(theme::FG))
                                    .child(question),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(theme::CYAN))
                                            .child(format!("{}%", prob_pct)),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(theme::fg_faint())
                                            .child("committed probability"),
                                    ),
                            ),
                    )
                    // Visibility picker
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_faint())
                                    .child("WHO CAN SEE THIS FORECAST"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(8.0))
                                    // Private option
                                    .child({
                                        let is_sel = selected_vis == "private";
                                        div()
                                            .id("vis-private")
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap(px(4.0))
                                            .flex_1()
                                            .p(px(12.0))
                                            .rounded(px(8.0))
                                            .border_1()
                                            .border_color(if is_sel {
                                                rgb(theme::CYAN)
                                            } else {
                                                rgb(theme::FG_FAINT)
                                            })
                                            .bg(if is_sel {
                                                rgb(theme::BG_ACTIVE)
                                            } else {
                                                rgb(theme::BG)
                                            })
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.commit_sheet_visibility = "private".into();
                                                cx.notify();
                                            }))
                                            .child(div().text_size(px(18.0)).child("🔒"))
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(rgb(theme::FG))
                                                    .child("Private"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(9.0))
                                                    .text_color(theme::fg_faint())
                                                    .child("only you + invited"),
                                            )
                                    })
                                    // Public option
                                    .child({
                                        let is_sel = selected_vis == "public";
                                        div()
                                            .id("vis-public")
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap(px(4.0))
                                            .flex_1()
                                            .p(px(12.0))
                                            .rounded(px(8.0))
                                            .border_1()
                                            .border_color(if is_sel {
                                                rgb(theme::CYAN)
                                            } else {
                                                rgb(theme::FG_FAINT)
                                            })
                                            .bg(if is_sel {
                                                rgb(theme::BG_ACTIVE)
                                            } else {
                                                rgb(theme::BG)
                                            })
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.commit_sheet_visibility = "public".into();
                                                cx.notify();
                                            }))
                                            .child(div().text_size(px(18.0)).child("🌐"))
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(rgb(theme::FG))
                                                    .child("Public"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(9.0))
                                                    .text_color(theme::fg_faint())
                                                    .child("global Brier"),
                                            )
                                    }),
                            ),
                    )
                    // Share-with list (Spec 24 §3.5.1) — people/emails to grant
                    // access to after the forecast is written. `shared` is
                    // implicit: private + a non-empty list.
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_faint())
                                    .child("SHARE WITH (optional)"),
                            )
                            // Add row: input + permission cycle + Add
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(div().flex_grow().child(self.commit_share_input.clone()))
                                    .child(
                                        div()
                                            .id("commit-share-perm")
                                            .px(px(10.0))
                                            .py(px(6.0))
                                            .rounded(px(4.0))
                                            .bg(rgb(theme::BG_ACTIVE))
                                            .text_size(px(11.0))
                                            .text_color(theme::gold())
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(|this, _, _w, cx| {
                                                this.commit_share_permission =
                                                    match this.commit_share_permission.as_str() {
                                                        "view" => "edit",
                                                        "edit" => "admin",
                                                        _ => "view",
                                                    }
                                                    .into();
                                                cx.notify();
                                            }))
                                            .child(self.commit_share_permission.clone()),
                                    )
                                    .child(
                                        div()
                                            .id("commit-share-add")
                                            .px(px(12.0))
                                            .py(px(6.0))
                                            .rounded(px(4.0))
                                            .bg(rgb(theme::BLUE))
                                            .text_size(px(11.0))
                                            .text_color(rgb(theme::BG))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.85))
                                            .on_click(cx.listener(|this, _, _w, cx| {
                                                this.add_commit_share_target(cx);
                                            }))
                                            .child("Add"),
                                    ),
                            )
                            // Pending share chips
                            .children(self.commit_share_targets.iter().enumerate().map(
                                |(i, (target, perm))| {
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .px(px(10.0))
                                        .py(px(6.0))
                                        .rounded(px(6.0))
                                        .bg(rgb(theme::BG))
                                        .child(div().text_size(px(12.0)).child(
                                            if target.contains('@') { "✉" } else { "🧑" },
                                        ))
                                        .child(
                                            div()
                                                .flex_grow()
                                                .overflow_hidden()
                                                .text_size(px(11.0))
                                                .text_color(rgb(theme::FG))
                                                .child(target.clone()),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(theme::gold())
                                                .child(perm.clone()),
                                        )
                                        .child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "commit-share-rm-{}",
                                                    i
                                                )))
                                                .px(px(6.0))
                                                .py(px(2.0))
                                                .rounded(px(4.0))
                                                .text_size(px(12.0))
                                                .text_color(theme::fg_dim())
                                                .cursor_pointer()
                                                .hover(|s| {
                                                    s.bg(theme::bg_hover()).text_color(theme::red())
                                                })
                                                .on_click(cx.listener(move |this, _, _w, cx| {
                                                    if i < this.commit_share_targets.len() {
                                                        this.commit_share_targets.remove(i);
                                                        cx.notify();
                                                    }
                                                }))
                                                .child("✕"),
                                        )
                                },
                            ))
                            // Team-pill row — same shape as the portfolio
                            // Access panel and the cockpit forecast Access
                            // tab. Clicking toggles the team in the pending
                            // list; selected pills render in CYAN with a
                            // check, unselected in FG_DIM.
                            .child(self.render_commit_team_share_pills(cx)),
                    )
                    // Action buttons
                    .child(
                        div()
                            .flex()
                            .gap(px(12.0))
                            .justify_end()
                            // Cancel
                            .child(
                                div()
                                    .id("commit-cancel")
                                    .px(px(20.0))
                                    .py(px(10.0))
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(rgb(theme::FG_FAINT))
                                    .text_size(px(13.0))
                                    .text_color(theme::fg_faint())
                                    .cursor_pointer()
                                    .hover(|s| {
                                        s.bg(rgb(theme::BG_HOVER)).text_color(rgb(theme::FG))
                                    })
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.commit_sheet_showing = false;
                                        cx.notify();
                                    }))
                                    .child("Cancel"),
                            )
                            // Commit
                            .child(
                                div()
                                    .id("commit-confirm")
                                    .px(px(20.0))
                                    .py(px(10.0))
                                    .rounded(px(6.0))
                                    .bg(rgb(theme::CYAN))
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme::BG_DEEP))
                                    .cursor_pointer()
                                    .hover(|s| s.opacity(0.85))
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.do_commit_forecast(cx);
                                    }))
                                    .child("⚡ Commit"),
                            ),
                    ),
            )
    }

    fn render_resolve_sheet(&self, cx: &mut Context<Self>) -> AnyElement {
        let question = truncate(&self.resolve_forecast_question, 80);
        let selected = self.resolve_outcome;
        let has_selection = selected.is_some();

        // Cascade mode: after a successful resolve, if we found
        // relationships involving the resolved forecast, surface a
        // "Cascade to N forecasts" button per relationship instead of
        // the resolve form. Operator clicks → propagation fires →
        // siblings get their probabilities updated.
        let in_cascade_mode =
            self.cascade_resolved_forecast_id.is_some() && !self.cascade_relationships.is_empty();
        if in_cascade_mode {
            return self.render_cascade_sheet(cx).into_any_element();
        }

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0A0E1499))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    .w(px(480.0))
                    .p(px(28.0))
                    .rounded(px(12.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .border_1()
                    .border_color(rgb(theme::GREEN))
                    // Header
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(theme::GREEN))
                                    .child("Resolve Forecast"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_faint())
                                    .child(
                                    "Record the actual outcome. This locks in your Brier score.",
                                ),
                            ),
                    )
                    // Question summary
                    .child(
                        div()
                            .p(px(12.0))
                            .rounded(px(6.0))
                            .bg(rgb(theme::BG))
                            .text_size(px(13.0))
                            .text_color(rgb(theme::FG))
                            .child(question),
                    )
                    // Outcome picker
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_faint())
                                    .child("WHAT HAPPENED?"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap(px(12.0))
                                    // YES tile
                                    .child({
                                        let is_sel = selected == Some(true);
                                        div()
                                            .id("resolve-yes")
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap(px(6.0))
                                            .flex_1()
                                            .p(px(16.0))
                                            .rounded(px(8.0))
                                            .border_1()
                                            .border_color(if is_sel {
                                                rgb(theme::GREEN)
                                            } else {
                                                rgb(theme::FG_FAINT)
                                            })
                                            .bg(if is_sel {
                                                rgb(theme::BG_ACTIVE)
                                            } else {
                                                rgb(theme::BG)
                                            })
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.resolve_outcome = Some(true);
                                                cx.notify();
                                            }))
                                            .child(div().text_size(px(24.0)).child("✓"))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(theme::GREEN))
                                                    .child("YES"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(theme::fg_faint())
                                                    .child("it happened"),
                                            )
                                    })
                                    // NO tile
                                    .child({
                                        let is_sel = selected == Some(false);
                                        div()
                                            .id("resolve-no")
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap(px(6.0))
                                            .flex_1()
                                            .p(px(16.0))
                                            .rounded(px(8.0))
                                            .border_1()
                                            .border_color(if is_sel {
                                                rgb(theme::RED)
                                            } else {
                                                rgb(theme::FG_FAINT)
                                            })
                                            .bg(if is_sel {
                                                rgb(theme::BG_ACTIVE)
                                            } else {
                                                rgb(theme::BG)
                                            })
                                            .cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.resolve_outcome = Some(false);
                                                cx.notify();
                                            }))
                                            .child(div().text_size(px(24.0)).child("✗"))
                                            .child(
                                                div()
                                                    .text_size(px(13.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(theme::RED))
                                                    .child("NO"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(10.0))
                                                    .text_color(theme::fg_faint())
                                                    .child("it didn't happen"),
                                            )
                                    }),
                            ),
                    )
                    // Error message
                    .when(self.resolve_error.is_some(), |el| {
                        el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::red())
                                .child(self.resolve_error.as_deref().unwrap_or("").to_string()),
                        )
                    })
                    // Action buttons
                    .child(
                        div()
                            .flex()
                            .gap(px(12.0))
                            .justify_end()
                            // Cancel
                            .child(
                                div()
                                    .id("resolve-cancel")
                                    .px(px(16.0))
                                    .py(px(8.0))
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(rgb(theme::FG_FAINT))
                                    .text_size(px(13.0))
                                    .text_color(theme::fg_dim())
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                    .on_click(cx.listener(|this, _, _window, cx| {
                                        this.resolve_sheet_showing = false;
                                        this.resolve_forecast_id = None;
                                        cx.notify();
                                    }))
                                    .child("Cancel"),
                            )
                            // Confirm
                            .child(
                                div()
                                    .id("resolve-confirm")
                                    .px(px(20.0))
                                    .py(px(8.0))
                                    .rounded(px(6.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(13.0))
                                    .when(has_selection && !self.resolve_loading, |el| {
                                        el.bg(rgb(theme::GREEN))
                                            .text_color(rgb(theme::BG))
                                            .cursor_pointer()
                                            .hover(|s| s.opacity(0.85))
                                            .on_click(cx.listener(|this, _, _window, cx| {
                                                this.submit_resolve(cx);
                                            }))
                                    })
                                    .when(!has_selection || self.resolve_loading, |el| {
                                        el.bg(theme::bg_hover()).text_color(theme::fg_faint())
                                    })
                                    .child(if self.resolve_loading {
                                        "Resolving…"
                                    } else {
                                        "Confirm"
                                    }),
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Render the operator-gated cascade queue. Surfaces every
    /// pending_cascade row the server has queued for this user —
    /// rows are created automatically by the resolve handler (manual
    /// or workspace_auto). Operator sees the projected deltas and
    /// Apply / Dismiss each entry.
    fn render_pending_cascades_sheet(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let n = self.pending_cascades.len();
        let loading = self.pending_cascades_loading;

        let mut content = div()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .w(px(720.0))
            .max_h(px(640.0))
            .p(px(24.0))
            .rounded(px(12.0))
            .bg(rgb(theme::BG_ELEVATED))
            .border_1()
            .border_color(rgb(theme::GOLD))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(theme::GOLD))
                            .child("Pending cascades"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(theme::fg_dim())
                            .child(format!("{} pending review · operator-gate enabled", n)),
                    )
                    .child(div().flex_grow())
                    .child(
                        div()
                            .id("pending-cascades-close")
                            .text_size(px(14.0))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|s| s.text_color(theme::fg()))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.pending_cascades_sheet_showing = false;
                                cx.notify();
                            }))
                            .child("✕ Close"),
                    ),
            )
            .child(
                div().text_size(px(11.0)).text_color(theme::fg_dim()).child(
                    "Each entry below was queued by a forecast resolution. \
                         Apply fires the propagation; Dismiss closes the entry \
                         without changing any siblings."
                        .to_string(),
                ),
            );

        if loading && self.pending_cascades.is_empty() {
            content = content.child(
                div()
                    .py(px(24.0))
                    .text_color(theme::fg_dim())
                    .text_size(px(12.0))
                    .child("Loading…"),
            );
        } else if self.pending_cascades.is_empty() {
            content = content.child(
                div()
                    .py(px(24.0))
                    .text_color(theme::fg_dim())
                    .text_size(px(12.0))
                    .child(
                        "No cascades pending. Resolve a forecast that's part of \
                         a relationship (e.g. WC sims mutex) to queue one."
                            .to_string(),
                    ),
            );
        } else {
            // Scrollable list of cascade rows.
            let list_id = "pending-cascades-list";
            let mut list = div()
                .id(list_id)
                .flex()
                .flex_col()
                .gap(px(10.0))
                .overflow_y_scroll();

            for entry in &self.pending_cascades {
                let cid = entry
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let in_flight = self.cascade_action_in_flight.contains(&cid);
                let question = entry
                    .get("trigger_question_text")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no question)")
                    .to_string();
                let outcome = entry.get("outcome").and_then(|v| v.as_bool());
                let outcome_label = match outcome {
                    Some(true) => "YES",
                    Some(false) => "NO",
                    None => "—",
                };
                let outcome_color = match outcome {
                    Some(true) => theme::GREEN,
                    Some(false) => theme::RED,
                    None => theme::FG_DIM,
                };
                let kind = entry
                    .get("relationship_kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let source = entry
                    .get("source")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let n_siblings = entry
                    .get("n_siblings")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let proposed = entry
                    .get("proposed_snapshot")
                    .cloned()
                    .unwrap_or(JsonValue::Null);
                let deltas = proposed
                    .get("deltas")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let n_projected = proposed
                    .get("n_projected")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                // Pre-sort the top 5 by absolute delta — operator wants
                // to see the biggest movers, not the smallest noise.
                let mut top_deltas: Vec<&JsonValue> = deltas.iter().collect();
                top_deltas.sort_by(|a, b| {
                    let av = a
                        .get("delta_pp")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0)
                        .abs();
                    let bv = b
                        .get("delta_pp")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0)
                        .abs();
                    bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
                });
                top_deltas.truncate(5);

                let cid_apply = cid.clone();
                let cid_dismiss = cid.clone();

                let mut row = div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .p(px(14.0))
                    .rounded(px(8.0))
                    .bg(rgb(theme::BG))
                    .border_1()
                    .border_color(theme::fg_faint())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(rgb(theme::FG))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .flex_grow()
                                    .child(truncate(&question, 60)),
                            )
                            .child(
                                div()
                                    .px(px(8.0))
                                    .py(px(2.0))
                                    .rounded(px(4.0))
                                    .bg(rgb(theme::BG_ELEVATED))
                                    .text_size(px(10.0))
                                    .text_color(rgb(outcome_color))
                                    .font_weight(FontWeight::BOLD)
                                    .child(outcome_label.to_string()),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(10.0))
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .child(format!("kind: {}", kind))
                            .child(format!("source: {}", source))
                            .child(format!("affects: {} siblings", n_siblings))
                            .child(format!("projected: {}", n_projected)),
                    );

                if !top_deltas.is_empty() {
                    let label = if deltas.len() > top_deltas.len() {
                        format!("Top movers ({} of {})", top_deltas.len(), deltas.len())
                    } else {
                        format!("Movers ({})", top_deltas.len())
                    };
                    row = row.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .mt(px(2.0))
                            .child(label),
                    );
                    let mut deltas_box = div().flex().flex_col().gap(px(2.0));
                    for d in &top_deltas {
                        let fid = d
                            .get("forecast_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("?")
                            .chars()
                            .take(8)
                            .collect::<String>();
                        let prev = d
                            .get("previous_probability")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        let new_p = d
                            .get("new_probability")
                            .and_then(|v| v.as_f64())
                            .unwrap_or(0.0);
                        let dpp = d.get("delta_pp").and_then(|v| v.as_f64()).unwrap_or(0.0);
                        let arrow_color = if dpp > 0.0 {
                            theme::GREEN
                        } else if dpp < 0.0 {
                            theme::RED
                        } else {
                            theme::FG_DIM
                        };
                        deltas_box =
                            deltas_box.child(
                                div()
                                    .flex()
                                    .gap(px(10.0))
                                    .text_size(px(10.0))
                                    .child(div().w(px(70.0)).text_color(theme::fg_dim()).child(fid))
                                    .child(div().w(px(140.0)).text_color(rgb(theme::FG)).child(
                                        format!("{:.1}% → {:.1}%", prev * 100.0, new_p * 100.0),
                                    ))
                                    .child(
                                        div()
                                            .text_color(rgb(arrow_color))
                                            .font_weight(FontWeight::BOLD)
                                            .child(format!(
                                                "{}{:.2}pp",
                                                if dpp >= 0.0 { "+" } else { "" },
                                                dpp
                                            )),
                                    ),
                            );
                    }
                    row = row.child(deltas_box);
                }

                // Apply / Dismiss buttons
                row = row.child(
                    div()
                        .flex()
                        .gap(px(10.0))
                        .mt(px(4.0))
                        .child({
                            let mut btn = div()
                                .id(SharedString::from(format!("apply-{}", cid)))
                                .px(px(14.0))
                                .py(px(6.0))
                                .rounded(px(6.0))
                                .bg(if in_flight {
                                    rgb(theme::BG_ELEVATED)
                                } else {
                                    rgb(theme::GREEN)
                                })
                                .text_color(if in_flight {
                                    rgb(theme::FG_FAINT)
                                } else {
                                    rgb(theme::BG)
                                })
                                .text_size(px(11.0))
                                .font_weight(FontWeight::SEMIBOLD);
                            if !in_flight {
                                btn = btn.cursor_pointer().hover(|s| s.opacity(0.85));
                                btn = btn.on_click(cx.listener(move |this, _, _, cx| {
                                    this.apply_pending_cascade(cid_apply.clone(), cx);
                                }));
                            }
                            btn.child(if in_flight {
                                "Applying…"
                            } else {
                                "✓ Apply"
                            })
                        })
                        .child({
                            let mut btn = div()
                                .id(SharedString::from(format!("dismiss-{}", cid)))
                                .px(px(14.0))
                                .py(px(6.0))
                                .rounded(px(6.0))
                                .border_1()
                                .border_color(rgb(theme::RED))
                                .text_color(rgb(theme::RED))
                                .text_size(px(11.0));
                            if !in_flight {
                                btn = btn.cursor_pointer().hover(|s| s.bg(rgb(theme::BG_HOVER)));
                                btn = btn.on_click(cx.listener(move |this, _, _, cx| {
                                    this.dismiss_pending_cascade(cid_dismiss.clone(), cx);
                                }));
                            }
                            btn.child("✗ Dismiss")
                        }),
                );

                list = list.child(row);
            }
            content = content.child(list);
        }

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0A0E1499))
            .child(content)
    }

    /// Render the cascade affordance — shown after a resolve completes
    /// AND the resolved forecast was part of one or more declared
    /// relationships. Replaces the resolve form (operator's already
    /// answered the outcome question; now they decide whether to
    /// propagate).
    fn render_cascade_sheet(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let outcome_label = match self.cascade_resolved_outcome {
            Some(true) => "YES — it happened",
            Some(false) => "NO — it didn't happen",
            None => "—",
        };
        let trigger_short = self
            .cascade_resolved_forecast_id
            .as_deref()
            .map(|s| s.chars().take(8).collect::<String>())
            .unwrap_or_default();

        let mut sheet = div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(gpui::rgba(0x0A0E1499))
            .child({
                let mut content = div()
                    .flex()
                    .flex_col()
                    .gap(px(20.0))
                    .w(px(560.0))
                    .p(px(28.0))
                    .rounded(px(12.0))
                    .bg(rgb(theme::BG_ELEVATED))
                    .border_1()
                    .border_color(rgb(theme::CYAN))
                    // Header
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(18.0))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(theme::CYAN))
                                    .child("Cascade resolution"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_faint())
                                    .child(format!(
                                        "Forecast {} resolved: {}. Propagate to siblings?",
                                        trigger_short, outcome_label
                                    )),
                            ),
                    );

                // One card per relationship.
                for rel in &self.cascade_relationships {
                    let rel_id = rel
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let kind = rel
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?")
                        .to_string();
                    let n_forecasts = rel
                        .get("forecast_ids")
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let description = rel
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let n_siblings = n_forecasts.saturating_sub(1);
                    let rel_id_for_click = rel_id.clone();

                    content = content.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .p(px(14.0))
                            .rounded(px(8.0))
                            .bg(rgb(theme::BG))
                            .border_1()
                            .border_color(rgb(theme::FG_FAINT))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .text_size(px(11.0))
                                            .text_color(rgb(theme::CYAN))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child(kind.clone()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(theme::fg_faint())
                                            .child(format!(
                                                "{} sibling forecast{}",
                                                n_siblings,
                                                if n_siblings == 1 { "" } else { "s" }
                                            )),
                                    ),
                            )
                            .child(div().text_size(px(10.0)).text_color(theme::fg_dim()).child(
                                if description.is_empty() {
                                    "(no description)".to_string()
                                } else {
                                    description
                                },
                            ))
                            .child({
                                let is_loading = self.cascade_loading;
                                div()
                                    .id(SharedString::from(format!("cascade-fire-{}", rel_id)))
                                    .px(px(14.0))
                                    .py(px(7.0))
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(rgb(theme::CYAN))
                                    .bg(rgb(theme::BG_ACTIVE))
                                    .text_size(px(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme::CYAN))
                                    .when(!is_loading, |el| {
                                        el.cursor_pointer()
                                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.fire_cascade(rel_id_for_click.clone(), cx);
                                            }))
                                    })
                                    .child(if is_loading {
                                        "Cascading…".to_string()
                                    } else {
                                        format!(
                                            "→ Cascade to {} forecast{}",
                                            n_siblings,
                                            if n_siblings == 1 { "" } else { "s" }
                                        )
                                    })
                            }),
                    );
                }

                // Result summary (if cascade has fired)
                if let Some(ref summary) = self.cascade_summary {
                    content = content.child(
                        div()
                            .p(px(12.0))
                            .rounded(px(6.0))
                            .bg(rgb(theme::BG))
                            .border_1()
                            .border_color(rgb(theme::GREEN))
                            .text_size(px(11.0))
                            .text_color(rgb(theme::GREEN))
                            .child(summary.clone()),
                    );
                }

                // Done / Skip buttons
                content = content.child(
                    div().flex().gap(px(12.0)).justify_end().child(
                        div()
                            .id("cascade-skip")
                            .px(px(16.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(rgb(theme::FG_FAINT))
                            .text_size(px(13.0))
                            .text_color(theme::fg_dim())
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(theme::BG_HOVER)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dismiss_cascade(cx);
                            }))
                            .child(if self.cascade_summary.is_some() {
                                "Done"
                            } else {
                                "Skip"
                            }),
                    ),
                );

                content
            });
        sheet = sheet.child(div());
        sheet
    }
}

impl Focusable for FermiConsole {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FermiConsole {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Pre-compute overlays before chaining to avoid borrow conflicts
        let commit_overlay = self
            .commit_sheet_showing
            .then(|| self.render_commit_sheet(cx).into_any_element());
        let resolve_overlay = self
            .resolve_sheet_showing
            .then(|| self.render_resolve_sheet(cx).into_any_element());
        let pending_cascades_overlay = self
            .pending_cascades_sheet_showing
            .then(|| self.render_pending_cascades_sheet(cx).into_any_element());
        let team_create_overlay = self
            .team_create_showing
            .then(|| self.render_team_create_modal(cx).into_any_element());
        let inbox_overlay = self
            .inbox_sheet_showing
            .then(|| self.render_inbox_sheet(cx).into_any_element());
        // Sprint A: just-created invite share modal. Only rendered
        // when `invite_share_modal` is Some — the child gates on its
        // own presence to unwrap safely.
        let invite_share_overlay = self
            .invite_share_modal
            .is_some()
            .then(|| self.render_invite_share_modal(cx).into_any_element());
        // Sprint C polish: three-tier agent hire modal.
        let hire_overlay = self
            .hire_modal
            .is_some()
            .then(|| self.render_hire_modal(cx).into_any_element());
        // Self-update release-notes modal. Gated on both the flag and
        // presence of a ReleaseInfo so we can't render without data.
        let update_overlay = (self.update_modal_showing && self.available_update.is_some())
            .then(|| self.render_update_modal(cx).into_any_element());
        // Keyboard-shortcuts help modal (Ctrl+/). Rendered on top of
        // every panel; single flag, no data dependencies.
        let shortcuts_overlay = self
            .shortcuts_modal_showing
            .then(|| self.render_shortcuts_modal(cx).into_any_element());
        // First-run welcome modal (fires once, when we detect a fresh
        // onboarding grant on the wallet snapshot). Rendered on top of
        // whatever panel is otherwise visible.
        let welcome_overlay = self
            .welcome_modal_showing
            .then(|| self.render_welcome_modal(cx).into_any_element());

        div()
            .key_context("FermiConsole")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::on_show_dashboard))
            .on_action(cx.listener(Self::on_show_portfolio))
            .on_action(cx.listener(Self::on_show_agent_fleet))
            .on_action(cx.listener(Self::on_show_composer))
            .on_action(cx.listener(Self::on_show_leaderboard))
            .on_action(cx.listener(Self::on_show_teams))
            .on_action(cx.listener(Self::on_trigger_question_orchestration))
            .on_action(cx.listener(Self::on_run_simulation))
            .on_action(cx.listener(Self::on_publish_forecast))
            .on_action(cx.listener(Self::on_save_forecast))
            .on_action(cx.listener(Self::on_import_forecast))
            .on_action(cx.listener(Self::on_toggle_fpl_source))
            .on_action(cx.listener(Self::on_minimize_window))
            .on_action(cx.listener(Self::on_zoom_window))
            .on_action(cx.listener(Self::on_toggle_fullscreen))
            .on_action(cx.listener(Self::on_reset_cockpit))
            .on_action(cx.listener(Self::on_new_forecast))
            .on_action(cx.listener(Self::on_check_for_updates))
            .on_action(cx.listener(Self::on_show_update_modal))
            .on_action(cx.listener(Self::on_dismiss_update_modal))
            .on_action(cx.listener(Self::on_show_shortcuts))
            .on_action(cx.listener(Self::on_dismiss_shortcuts))
            .relative()
            .flex()
            .size_full()
            .bg(theme::bg())
            .text_color(theme::fg())
            .font_family("Ubuntu Mono, DejaVu Sans Mono, Liberation Mono, monospace")
            .child(
                // Sidebar
                self.render_sidebar(cx),
            )
            .child(
                // Main content area. When the user isn't authenticated
                // we replace the entire panel router with a full-window
                // splash — no sidebar-visible nav that leads nowhere,
                // no half-loaded stats, no bare Empty states. Post-auth,
                // the normal panel router takes over.
                div()
                    .flex()
                    .flex_col()
                    .flex_grow()
                    .overflow_hidden()
                    .child(if !self.connected {
                        self.render_auth_gate(cx).into_any_element()
                    } else {
                        match self.active_panel {
                            Panel::Dashboard => self.render_dashboard(cx).into_any_element(),
                            Panel::Portfolio => self.render_portfolio(cx).into_any_element(),
                            Panel::AgentFleet => {
                                self.render_agent_fleet_panel(cx).into_any_element()
                            }
                            Panel::Composer => {
                                if let Some(ref cockpit_entity) = self.cockpit {
                                    cockpit::render_cockpit(cockpit_entity).into_any_element()
                                } else {
                                    // Shouldn't happen — navigate() creates it
                                    composer::render_composer(&self.composer).into_any_element()
                                }
                            }
                            Panel::Leaderboard => {
                                self.render_leaderboard_panel().into_any_element()
                            }
                            Panel::Teams => self.render_teams_panel(cx).into_any_element(),
                        }
                    }),
            )
            // Create-team modal overlay
            .children(team_create_overlay)
            // Inbox sheet overlay (pending invites)
            .children(inbox_overlay)
            // Just-created invite share modal (Sprint A) — immediate
            // Copy Link affordance after any /invites POST succeeds.
            .children(invite_share_overlay)
            // Agent hire modal (Sprint C polish) — three-tier flow:
            // forecast → driver → terms → confirm.
            .children(hire_overlay)
            // Commit sheet overlay (⌘P)
            .children(commit_overlay)
            // Resolve sheet overlay
            .children(resolve_overlay)
            // Pending cascades queue overlay (operator review)
            .children(pending_cascades_overlay)
            // Self-update modal (release notes + download progress)
            .children(update_overlay)
            // Keyboard shortcuts help modal (Ctrl+/)
            .children(shortcuts_overlay)
            // First-run welcome modal (post-signup)
            .children(welcome_overlay)
            // Toast notification overlay (bottom-right, auto-dismiss)
            .when(self.toast.is_some(), |el| {
                if let Some((ref msg, icon, color)) = self.toast {
                    el.child(
                        div()
                            // Fixed to bottom-right corner
                            .absolute()
                            .bottom(px(24.0))
                            .right(px(24.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .px(px(16.0))
                            .py(px(10.0))
                            .rounded(px(8.0))
                            .bg(theme::bg_elevated())
                            .border_1()
                            .border_color(rgb(color))
                            .shadow_lg()
                            .child(div().text_size(px(14.0)).text_color(rgb(color)).child(icon))
                            .child(
                                div()
                                    .text_size(px(12.0))
                                    .text_color(theme::fg())
                                    .min_w(px(0.0))
                                    .child(msg.clone()),
                            ),
                    )
                } else {
                    el
                }
            })
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Truncate a string to at most `max_chars` characters (NOT bytes).
/// Char-aware to avoid UTF-8-boundary panics on agent / question text
/// containing multibyte codepoints (em-dashes, smart quotes, 'Türkiye',
/// 'Côte d'Ivoire', etc.). Byte-indexed slicing — the previous
/// implementation, `&s[..max_len - 1]` — panics with "byte index N is
/// not a char boundary" if the cutoff lands inside a multibyte codepoint.
fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else if max_chars == 0 {
        "…".to_string()
    } else {
        format!("{}…", s.chars().take(max_chars - 1).collect::<String>())
    }
}

/// Deterministic team-id → palette-colour mapping. The same team_id
/// always produces the same colour so operators learn the mapping
/// through repetition ("the amber dot is Macro Desk, the cyan dot is
/// Sports"). Palette is 7 accent colours from the theme; teams beyond
/// the palette size wrap around — collisions are cosmetic, not
/// semantic, so they're acceptable.
fn team_color(team_id: &str) -> u32 {
    const PALETTE: &[u32] = &[
        theme::CYAN,
        theme::GOLD,
        theme::PURPLE,
        theme::BLUE,
        theme::GREEN,
        theme::ORANGE,
        theme::RED,
    ];
    // Cheap FNV-1a-ish rolling hash over the id bytes. Not a security
    // primitive; just needs stability + uniform distribution across
    // the palette. `wrapping_*` avoids arithmetic panics on debug.
    let mut h: u32 = 2166136261;
    for b in team_id.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(16777619);
    }
    PALETTE[(h as usize) % PALETTE.len()]
}

/// Small pill used by the Dashboard's activity feed header to signal
/// the source-filter shape (`All / Mine / Team / Marketplace`). Team
/// and Marketplace are `disabled` today — they render dimmer and are
/// non-interactive — because the underlying event streams aren't
/// ingested yet. Rendering them anyway shows the operator the
/// information architecture the console is growing into.
/// Interactive filter chip for the Dashboard's activity feed. Sets
/// `dashboard_activity_filter` on click; renders in the same visual
/// language as `activity_source_chip` but with an accent border when
/// active, a hover state when inactive, and dimming when disabled
/// (no items in that source's stream).
fn activity_filter_chip(
    id: &'static str,
    label: &str,
    kind: ActivityFilter,
    current: ActivityFilter,
    disabled: bool,
    cx: &Context<FermiConsole>,
) -> impl IntoElement {
    let active = kind == current;
    let (bg_color, fg_color, border_color) = if disabled {
        (theme::BG, theme::FG_FAINT, theme::FG_FAINT)
    } else if active {
        (theme::BG_HOVER, theme::FG, theme::CYAN)
    } else {
        (theme::BG, theme::FG_DIM, theme::FG_FAINT)
    };
    div()
        .id(SharedString::from(id))
        .px(px(8.0))
        .py(px(2.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(rgb(border_color))
        .bg(rgb(bg_color))
        .text_size(px(10.0))
        .text_color(rgb(fg_color))
        .when(!disabled, |el| {
            el.cursor_pointer().hover(|s| s.bg(theme::bg_hover()))
        })
        .when(!disabled, |el| {
            el.on_click(cx.listener(move |this, _e, _w, cx| {
                this.dashboard_activity_filter = kind;
                cx.notify();
            }))
        })
        .child(label.to_string())
}

fn activity_source_chip(label: &str, active: bool, disabled: bool) -> impl IntoElement {
    let (bg_color, fg_color, border_color) = if disabled {
        (theme::BG, theme::FG_FAINT, theme::FG_FAINT)
    } else if active {
        (theme::BG_HOVER, theme::FG, theme::CYAN)
    } else {
        (theme::BG, theme::FG_DIM, theme::FG_FAINT)
    };
    div()
        .px(px(8.0))
        .py(px(2.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(rgb(border_color))
        .bg(rgb(bg_color))
        .text_size(px(10.0))
        .text_color(rgb(fg_color))
        .child(label.to_string())
}

/// Compute the "family" key for an activity row. Two resolutions
/// share a family when their question text collapses to the same
/// stem AND the outcome matches — e.g. "Will Jamaica win the 2026
/// FIFA World Cup" and "Will Morocco win the 2026 FIFA World Cup"
/// both stem to `2026 fifa world cup|no`.
///
/// The stem is derived by:
///   1. Lowercasing
///   2. Stripping leading "will <subject>" pattern (subject is any
///      run of word chars up to the next connective).
///   3. Trimming trailing punctuation.
///
/// Empty result means "don't collapse" — e.g. for questions that
/// don't match the pattern we let each row stand.
fn activity_family_key(question: &str, outcome: Option<bool>) -> String {
    let lower = question.to_lowercase();
    let trimmed = lower.trim_matches(|c: char| !c.is_alphanumeric());
    // Strip common "will <subject>" prefix so per-subject variations
    // collapse. We match "will" + space + one or more word tokens +
    // " win" | " be" | " reach" | " defeat", then keep the rest.
    let stem = if let Some(rest) = trimmed.strip_prefix("will ") {
        // Walk past subject tokens until we hit a verb we recognise.
        let verbs = [
            " win ",
            " be ",
            " reach ",
            " defeat ",
            " beat ",
            " qualify ",
        ];
        let mut found: Option<&str> = None;
        for v in &verbs {
            if let Some(idx) = rest.find(v) {
                found = Some(&rest[idx + v.len()..]);
                break;
            }
        }
        found.unwrap_or(rest).to_string()
    } else {
        trimmed.to_string()
    };
    // Truncate the stem to a stable prefix so wording drift on the
    // tail ("… in Group A" vs "… in Group B") still collapses.
    let stem_prefix: String = stem.chars().take(40).collect();
    let outcome_tag = match outcome {
        Some(true) => "yes",
        Some(false) => "no",
        None => "?",
    };
    format!("{}|{}", stem_prefix.trim(), outcome_tag)
}

/// Human-readable label for a family key produced by
/// `activity_family_key`. Used in the collapsed summary row.
fn describe_activity_family(family_key: &str) -> String {
    let (stem, outcome) = family_key.rsplit_once('|').unwrap_or((family_key, "?"));
    let outcome_word = match outcome {
        "yes" => "all Yes",
        "no" => "all No",
        _ => "mixed",
    };
    let stem_display = truncate(stem, 44);
    format!("{} — {}", stem_display, outcome_word)
}

/// True if a team belongs to the fermi_forecast vertical and should appear
/// in the console's Teams panel. ABW is shared substrate; `/api/teams`
/// returns rabble swarms, kask workspaces, etc. too.
///
/// When the API returns `origin` (after the fermi-auth change ships) this is
/// strict: `origin == "fermi_forecast"`. Against API builds that don't yet
/// return `origin`, fall back to hiding the obvious other-vertical
/// auto-created workspaces by slug/description so the list is usable now.
/// Format a user_id for compact display when the server didn't return
/// a display_name. UUIDs collapse to their leading 8 chars ("9c3a4b12");
/// short opaque IDs pass through unchanged. Keeps the UI readable
/// without leaking the full identifier at hover-glance distance.
fn short_user_label(user_id: &str) -> String {
    let trimmed = user_id.trim();
    if trimmed.is_empty() {
        return "(unknown)".into();
    }
    // UUIDs are 36 chars with dashes; anything longer than ~24 is
    // almost certainly an opaque token — shorten it.
    if trimmed.len() >= 24 {
        // For a canonical UUID keep the pre-dash prefix ("9c3a4b12") so
        // it still looks like a familiar handle.
        if let Some(head) = trimmed.split('-').next() {
            if head.len() >= 6 {
                return format!("{}…", head);
            }
        }
        return format!("{}…", &trimmed[..8]);
    }
    trimmed.to_string()
}

fn is_fermi_team(t: &Team) -> bool {
    match t.origin.as_deref() {
        Some(o) => o == "fermi_forecast",
        None => {
            let slug = t.slug.to_ascii_lowercase();
            let desc = t.description.as_deref().unwrap_or("").to_ascii_lowercase();
            const VERTICAL_PREFIXES: &[&str] =
                &["rabble", "kask", "efrain", "smoketest", "silat", "swarm"];
            let is_vertical = VERTICAL_PREFIXES.iter().any(|p| slug.starts_with(p));
            !(is_vertical || desc.contains("auto-created workspace"))
        }
    }
}

/// Every ABW workspace (spawn_forecast_workspace) auto-creates a team
/// wrapper so shares can bind to it. For the WC forecast infrastructure
/// that means one ABW team per team-prior workspace — 62+ entries with
/// names like "Team Prior — Argentina (ARG)" and slugs like
/// `fermi-forecast-<hex>`. These are IMPLEMENTATION plumbing, not
/// collaboration teams; surfacing them in the Teams panel drowns out
/// the actual human teams the operator wants to manage.
///
/// Detect them by the workspace-team fingerprint:
///   * name starts with "Team Prior —" or "Tournament Path —"
///     (the WC template's two workspace kinds)
///   * OR slug matches `fermi-forecast-<8+ hex chars>` (the
///     auto-spawn slug pattern from spawn_forecast_workspace)
///   * OR description contains "Tournament win probability prior"
///
/// Anything else — slugs the user chose, human names, empty
/// descriptions — is treated as a real collaboration team.
fn is_workspace_prior_team(t: &Team) -> bool {
    // Name prefixes emitted by the WC template's workspace spawner.
    if t.name.starts_with("Team Prior — ")
        || t.name.starts_with("Team Prior - ")
        || t.name.starts_with("Tournament Path — ")
        || t.name.starts_with("Tournament Path - ")
    {
        return true;
    }
    // spawn_forecast_workspace slug pattern: `fermi-forecast-<hex>`.
    // We match by prefix + a trailing hex-ish tail so a user who
    // happens to name their team "fermi-forecast-collab" isn't hidden.
    let slug = t.slug.to_ascii_lowercase();
    if let Some(tail) = slug.strip_prefix("fermi-forecast-") {
        // The auto tail is 8+ chars, all lowercase hex. Reject only
        // when the tail matches that shape exactly — anything with a
        // dash, letter outside a–f, etc. is user-chosen.
        if tail.len() >= 6 && tail.chars().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
    }
    // Fallback: description written by the WC template's team
    // constructor for team-prior workspaces.
    let desc = t.description.as_deref().unwrap_or("").to_ascii_lowercase();
    desc.contains("tournament win probability prior") || desc.contains("auto-created workspace")
}

/// True when a team should appear in the Teams collaboration panel.
fn is_collaboration_team(t: &Team) -> bool {
    is_fermi_team(t) && !is_workspace_prior_team(t)
}

/// Render an RFC3339 timestamp as a compact "now / 5m / 3h / 2d / 4w / 8mo / 2y"
/// relative string for portfolio rows. Falls back to "—" on parse failure
/// rather than poisoning the whole list with a panic.
pub(crate) fn format_relative_time(rfc3339: &str) -> String {
    let parsed = chrono::DateTime::parse_from_rfc3339(rfc3339);
    let Ok(t) = parsed else {
        return "—".into();
    };
    let now = chrono::Utc::now();
    let delta = now.signed_duration_since(t.with_timezone(&chrono::Utc));
    let secs = delta.num_seconds();
    if secs < 0 {
        // Clock skew or future-dated row. Don't show "-3m" — just say "now".
        return "now".into();
    }
    if secs < 60 {
        return "now".into();
    }
    let mins = secs / 60;
    if mins < 60 {
        return format!("{}m", mins);
    }
    let hours = mins / 60;
    if hours < 24 {
        return format!("{}h", hours);
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{}d", days);
    }
    if days < 30 {
        return format!("{}w", days / 7);
    }
    if days < 365 {
        return format!("{}mo", days / 30);
    }
    format!("{}y", days / 365)
}

/// Render the expanded detail panel for a forecast.
fn render_forecast_detail(f: &Forecast) -> impl IntoElement {
    let created = f
        .created_at
        .as_deref()
        .and_then(|s| s.split('T').next())
        .unwrap_or("—");
    let updated = f
        .updated_at
        .as_deref()
        .and_then(|s| s.split('T').next())
        .unwrap_or("—");

    div()
        .px(px(24.0))
        .py(px(12.0))
        .bg(theme::bg())
        .border_t_1()
        .border_color(theme::fg_faint())
        .flex()
        .flex_col()
        .gap(px(8.0))
        // Full question
        .child(
            div()
                .text_size(px(14.0))
                .text_color(theme::fg())
                .font_weight(FontWeight::SEMIBOLD)
                .child(f.question_text.clone()),
        )
        // Metadata grid
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_x(px(24.0))
                .gap_y(px(6.0))
                .text_size(px(11.0))
                .child(render_detail_kv(
                    "Domain",
                    f.domain.as_deref().unwrap_or("—"),
                ))
                .child(render_detail_kv(
                    "Target Date",
                    f.target_date
                        .as_deref()
                        .and_then(|s| s.split('T').next())
                        .unwrap_or("—"),
                ))
                .child(render_detail_kv(
                    "Probability",
                    &format!("{:.1}%", f.predicted_probability * 100.0),
                ))
                .child(render_detail_kv(
                    "Brier Score",
                    &f.brier_score
                        .map(|b| format!("{:.4}", b))
                        .unwrap_or_else(|| "—".into()),
                ))
                .child(render_detail_kv("Created", created))
                .child(render_detail_kv("Updated", updated))
                .child(render_detail_kv("Visibility", &f.visibility)),
        )
        // Resolution criteria
        .when(f.resolution_criteria.is_some(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child("Resolution Criteria"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .child(f.resolution_criteria.as_deref().unwrap_or("").to_string()),
                    ),
            )
        })
        // Confidence interval
        .when(
            f.confidence_interval_low.is_some() || f.confidence_interval_high.is_some(),
            |el| {
                let low = f.confidence_interval_low.unwrap_or(0.0);
                let high = f.confidence_interval_high.unwrap_or(0.0);
                el.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme::fg_dim())
                        .child(format!("Confidence interval: [{:.1}, {:.1}]", low, high)),
                )
            },
        )
        // Resolution info
        .when(f.resolved_at.is_some(), |el| {
            el.child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .text_size(px(11.0))
                    .child(render_detail_kv(
                        "Resolved",
                        f.resolved_at
                            .as_deref()
                            .and_then(|s| s.split('T').next())
                            .unwrap_or("—"),
                    ))
                    .child(render_detail_kv(
                        "Outcome",
                        match f.actual_outcome {
                            Some(true) => "Yes",
                            Some(false) => "No",
                            None => "—",
                        },
                    )),
            )
        })
        // Update history
        .when(
            f.update_history
                .as_ref()
                .map(|h| !h.is_empty())
                .unwrap_or(false),
            |el| {
                let updates = f.update_history.as_ref().unwrap();
                el.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme::fg_faint())
                                .child(format!("Update History ({})", updates.len())),
                        )
                        .children(updates.iter().map(|u| {
                            let prev = u
                                .previous_probability
                                .map(|p| format!("{:.0}%", p * 100.0))
                                .unwrap_or_else(|| "—".into());
                            let new_p = u
                                .new_probability
                                .map(|p| format!("{:.0}%", p * 100.0))
                                .unwrap_or_else(|| "—".into());
                            let reason = u.reason.as_deref().unwrap_or("");
                            let date = u
                                .created_at
                                .as_deref()
                                .and_then(|s| s.split('T').next())
                                .unwrap_or("");
                            div()
                                .flex()
                                .gap(px(8.0))
                                .text_size(px(10.0))
                                .text_color(theme::fg_dim())
                                .child(format!("{} → {}", prev, new_p))
                                .when(!reason.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .text_color(theme::fg_faint())
                                            .child(truncate(reason, 40)),
                                    )
                                })
                                .child(div().text_color(theme::fg_faint()).child(date.to_string()))
                        })),
                )
            },
        )
        // Forecast ID
        .child(
            div()
                .text_size(px(9.0))
                .text_color(theme::fg_faint())
                .child(format!("ID: {}", f.id)),
        )
}

/// Render a key-value pair for the forecast detail view.
/// Full-width fleet row: agent identity + live run status + driver assignments + credits.
// ═══════════════════════════════════════════════════════════════════
// Agent Marketplace (Sprint C)
//
// Turns the flat fleet list into a ranked marketplace. Each entry is
// a synthesis of:
//   - Local card definition (name, description, tags, capabilities)
//   - Server /api/agents `execution_stats` (executions, cost, success
//     rate) — the outcome-contribution signal Sprint C promised
//   - Local session `agent_runs` (per-session confidence)
//
// Ranking is transparent and simple: a weighted score the operator
// can eyeball, not a black-box ML model. Weights are conservative:
//   base = success_rate * 100         // 0–100
//   + popularity_bonus (log-scaled)   // up to +25
//   + confidence_bonus                // up to +25 if this session ran it
//   - cost_penalty                    // per $ per run above $0.10
//
// Tier is a bucket over total_executions so operators can filter
// "who's proven" vs. "who's new":
//   Popular      ≥ 100 runs
//   Established  ≥ 20
//   Rising       ≥ 5
//   Fresh        < 5
// ═══════════════════════════════════════════════════════════════════

struct AgentMarketplaceEntry {
    agent_id: String,
    display_name: String,
    description: String,
    tags: Vec<String>,
    total_executions: i64,
    success_rate: f64,
    avg_cost_per_run: Option<f64>,
    avg_confidence_this_session: Option<f64>,
    tier: &'static str,
    tier_color: u32,
    score: f64,
    /// True when we have any usage data at all (server
    /// `total_executions > 0` or a completed session run). Used to
    /// suppress "score 0" and "cost n/a" when the agent is genuinely
    /// unrated instead of pretending zero is a real score.
    has_data: bool,
    /// Whether this session already invoked this agent (→ hire
    /// button label switches to "Assigned").
    already_used: bool,
    // ── Rich detail (populated from AgentCard) ───────────────────────────
    version: String,
    author: String,
    model: String,
    temperature: f64,
    accepts: Vec<String>,
    produces: Vec<String>,
    sample_queries: Vec<String>,
    /// MCP tool names (skip descriptions here — they're long).
    mcp_tools: Vec<String>,
    /// Skills declared by the agent card.
    skills: Vec<String>,
    /// Whether the agent needs any external secrets set.
    needs_secrets: bool,
}

fn build_agent_marketplace(
    local_cards: &[&AgentCard],
    server_cards: &[JsonValue],
    session_runs: &[cockpit::AgentExecution],
) -> Vec<AgentMarketplaceEntry> {
    // Index server cards by agent_id (the string handle, not the UUID).
    let server_by_id: std::collections::HashMap<String, &JsonValue> = server_cards
        .iter()
        .filter_map(|c| {
            let id = c.get("agent_id").and_then(|v| v.as_str())?;
            Some((id.to_string(), c))
        })
        .collect();
    // Index local cards for O(1) lookup while iterating the union.
    let local_by_id: std::collections::HashMap<String, &AgentCard> = local_cards
        .iter()
        .map(|c| (c.agent_id.clone(), *c))
        .collect();

    // Index session runs by agent_name (with confidence averages).
    let mut session_confidence: std::collections::HashMap<String, (f64, usize)> =
        std::collections::HashMap::new();
    for run in session_runs {
        if let Some(c) = run.confidence {
            let entry = session_confidence
                .entry(base_agent_name_local(&run.agent_name).to_string())
                .or_insert((0.0, 0));
            entry.0 += c;
            entry.1 += 1;
        }
    }

    // Build the union of agent_ids: locals first (to preserve the
    // registry's ordering), then any server-only ids the local install
    // doesn't know about. Server-only ids are what makes the "Fresh"
    // and "Rising" tiers actually discoverable — previously the
    // marketplace only ever showed agents you already had on disk, so
    // truly new community agents were invisible from this surface.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut ordered_ids: Vec<String> = Vec::new();
    for card in local_cards {
        if seen.insert(card.agent_id.clone()) {
            ordered_ids.push(card.agent_id.clone());
        }
    }
    for sc in server_cards {
        if let Some(id) = sc.get("agent_id").and_then(|v| v.as_str()) {
            if seen.insert(id.to_string()) {
                ordered_ids.push(id.to_string());
            }
        }
    }

    let mut entries: Vec<AgentMarketplaceEntry> = Vec::new();
    for agent_id in ordered_ids {
        let local = local_by_id.get(&agent_id).copied();
        let server = server_by_id.get(&agent_id).copied();

        // Never surface meta / system agents in the marketplace — Fermi
        // is the conductor of the orchestra, not a hire; xaman_ek is
        // ABW's always-on navigator; both are invoked implicitly by the
        // app itself. Same signal as render_agent_fleet_panel's
        // filter, applied here too so a server-only registration of
        // one of these ids can't sneak back in via the Fresh tier.
        let is_system_local = local
            .map(|c| matches!(c.tier, fermi::agent_backend::agent_card::AgentTier::System))
            .unwrap_or(false);
        let is_system_server = server
            .map(|s| {
                let agent_type = s.get("agent_type").and_then(|v| v.as_str()).unwrap_or("");
                let tier = s.get("tier").and_then(|v| v.as_str()).unwrap_or("");
                let hireable = s.get("hireable").and_then(|v| v.as_bool()).unwrap_or(true);
                agent_type == "meta" || tier == "system" || !hireable
            })
            .unwrap_or(false);
        if is_system_local || is_system_server {
            continue;
        }

        let stats = server.and_then(|s| s.get("execution_stats"));
        let total_executions = stats
            .and_then(|s| s.get("total_executions"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let successful = stats
            .and_then(|s| s.get("successful_executions"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        let total_cost = stats
            .and_then(|s| s.get("total_cost_usd"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let success_rate = if total_executions > 0 {
            successful as f64 / total_executions as f64
        } else {
            0.0
        };
        let avg_cost = if total_executions > 0 && total_cost > 0.0 {
            Some(total_cost / total_executions as f64)
        } else {
            None
        };

        let (sum_conf, n_conf) = session_confidence
            .get(&agent_id)
            .copied()
            .unwrap_or((0.0, 0));
        let avg_session_conf = if n_conf > 0 {
            Some(sum_conf / n_conf as f64)
        } else {
            None
        };

        // Tier bucket.
        let (tier, tier_color) = match total_executions {
            n if n >= 100 => ("popular", theme::GOLD),
            n if n >= 20 => ("established", theme::CYAN),
            n if n >= 5 => ("rising", theme::BLUE),
            _ => ("fresh", theme::FG_DIM),
        };

        // Score.
        let base = success_rate * 100.0;
        let popularity_bonus = if total_executions > 0 {
            ((total_executions as f64 + 1.0).log10() * 10.0).min(25.0)
        } else {
            0.0
        };
        let confidence_bonus = avg_session_conf.map(|c| c * 25.0).unwrap_or(0.0);
        // Penalty: $0.10 per run is "cheap", each extra $ costs 5 points.
        let cost_penalty = avg_cost
            .map(|c| ((c - 0.10).max(0.0) * 5.0).min(30.0))
            .unwrap_or(0.0);
        let score = base + popularity_bonus + confidence_bonus - cost_penalty;

        let has_data = total_executions > 0 || n_conf > 0;
        // Display name: server display_alias > local card > agent_id.
        let display_name = server
            .and_then(|s| s.get("display_alias"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| agent_id.clone());
        // Description: server > local card > empty.
        let description = server
            .and_then(|s| s.get("description"))
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .or_else(|| local.map(|c| c.metadata.description.clone()))
            .unwrap_or_default();
        // Tags: local card tags first, else server tags array.
        let tags: Vec<String> = local.map(|c| c.metadata.tags.clone()).unwrap_or_else(|| {
            server
                .and_then(|s| s.get("tags"))
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        });
        // MCP tool names come from the local card's capabilities block;
        // server-only entries render with no MCP tools listed (correct —
        // the operator hasn't downloaded the card yet).
        let mcp_tools: Vec<String> = local
            .map(|c| {
                c.capabilities
                    .mcp_tools
                    .iter()
                    .map(|t| t.name.clone())
                    .collect()
            })
            .unwrap_or_default();
        let needs_secrets = local
            .map(|c| !c.requires_secrets.is_empty())
            .unwrap_or(false)
            || server
                .and_then(|s| s.get("requires_secrets"))
                .and_then(|v| v.as_array())
                .map(|a| !a.is_empty())
                .unwrap_or(false);
        // Version / author / model prefer local; fall back to server
        // JSON so server-only entries still render a meaningful header.
        let version = local.map(|c| c.version.clone()).unwrap_or_else(|| {
            server
                .and_then(|s| s.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string()
        });
        let author = local.map(|c| c.metadata.author.clone()).unwrap_or_else(|| {
            server
                .and_then(|s| s.get("author"))
                .and_then(|v| v.as_str())
                .unwrap_or("community")
                .to_string()
        });
        let model = local
            .map(|c| c.capabilities.model.clone())
            .unwrap_or_else(|| {
                server
                    .and_then(|s| s.get("model"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            });
        let temperature = local
            .map(|c| c.capabilities.temperature)
            .or_else(|| {
                server
                    .and_then(|s| s.get("temperature"))
                    .and_then(|v| v.as_f64())
            })
            .unwrap_or(0.0);
        let accepts = local.map(|c| c.accepts.clone()).unwrap_or_else(|| {
            server
                .and_then(|s| s.get("accepts"))
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        });
        let produces = local.map(|c| c.produces.clone()).unwrap_or_else(|| {
            server
                .and_then(|s| s.get("produces"))
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        });
        let sample_queries = local
            .map(|c| c.metadata.sample_queries.clone())
            .unwrap_or_default();
        let skills = local
            .map(|c| c.capabilities.skills.clone())
            .unwrap_or_default();

        entries.push(AgentMarketplaceEntry {
            agent_id: agent_id.clone(),
            display_name,
            description,
            tags,
            total_executions,
            success_rate,
            avg_cost_per_run: avg_cost,
            avg_confidence_this_session: avg_session_conf,
            tier,
            tier_color,
            score,
            has_data,
            already_used: n_conf > 0,
            version,
            author,
            model,
            temperature,
            accepts,
            produces,
            sample_queries,
            mcp_tools,
            skills,
            needs_secrets,
        });
    }
    entries
}

/// Sort a marketplace list by the operator's chosen mode.
fn sort_marketplace(entries: &mut Vec<AgentMarketplaceEntry>, mode: &str) {
    entries.sort_by(|a, b| {
        use std::cmp::Ordering::Equal;
        let cmp = match mode {
            "cost_asc" => {
                // Cost low→high; None (no cost data) sorts last so
                // the ranked list still starts with real data.
                let ac = a.avg_cost_per_run.unwrap_or(f64::MAX);
                let bc = b.avg_cost_per_run.unwrap_or(f64::MAX);
                ac.partial_cmp(&bc).unwrap_or(Equal)
            }
            "cost_desc" => {
                // Cost high→low; None still sorts last (unknown cost
                // shouldn't jump the queue at either end).
                let ac = a.avg_cost_per_run.unwrap_or(-1.0);
                let bc = b.avg_cost_per_run.unwrap_or(-1.0);
                bc.partial_cmp(&ac).unwrap_or(Equal)
            }
            "executions" => b.total_executions.cmp(&a.total_executions),
            "success" => b.success_rate.partial_cmp(&a.success_rate).unwrap_or(Equal),
            "contribution" => {
                // Highest session confidence first; None sorts last.
                let ac = a.avg_confidence_this_session.unwrap_or(-1.0);
                let bc = b.avg_confidence_this_session.unwrap_or(-1.0);
                bc.partial_cmp(&ac).unwrap_or(Equal)
            }
            _ => b.score.partial_cmp(&a.score).unwrap_or(Equal),
        };
        cmp.then_with(|| a.display_name.cmp(&b.display_name))
    });
}

/// Local copy of the cockpit's base_agent_name helper. Used to align
/// session runs (whose IDs may be compound like "macro_forecaster_x")
/// with catalog agent_ids ("macro_forecaster"). The cockpit's version
/// is private to that module; keep this mirror in sync with any changes
/// there.
fn base_agent_name_local(name: &str) -> &str {
    // Common bound-agent suffix pattern: "<agent>_<driver>". The
    // registry doesn't expose the split rule, but every known compound
    // uses a `_` separator and the catalog agent_ids are all lowercase
    // snake. Match longest catalog prefix.
    const KNOWN_BASES: &[&str] = &[
        "macro_forecaster",
        "fermi",
        "market_research",
        "simops_advisor",
        "simops_optimizer",
        "simops_cascade",
        "simops_narrator_local",
        "valuechain_mapper",
    ];
    for base in KNOWN_BASES {
        if name == *base || name.starts_with(&format!("{}_", base)) {
            return base;
        }
    }
    name
}

fn render_fleet_agent_row(
    card: &AgentCard,
    run: Option<&cockpit::AgentExecution>,
    assigned_drivers: &[String],
) -> impl IntoElement {
    use cockpit::AgentRunStatus;

    let tier_color = match card.tier {
        fermi::agent_backend::agent_card::AgentTier::Curated => theme::CYAN,
        _ => theme::FG_DIM,
    };

    let (status_icon, status_text, status_color) = match run.map(|r| &r.status) {
        Some(AgentRunStatus::Running) => ("⟳", "Running", theme::GOLD),
        Some(AgentRunStatus::Completed) => ("✓", "Completed", theme::GREEN),
        Some(AgentRunStatus::Failed) => ("✗", "Failed", theme::RED),
        Some(AgentRunStatus::Idle) | None => ("○", "Idle", theme::FG_FAINT),
    };

    div()
        .w_full()
        .bg(theme::bg_elevated())
        .border_1()
        .border_color(theme::fg_faint())
        .rounded(px(6.0))
        .p(px(14.0))
        .flex()
        .gap(px(12.0))
        // Left: status indicator column
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap(px(4.0))
                .w(px(48.0))
                .child(
                    div()
                        .text_size(px(18.0))
                        .text_color(rgb(status_color))
                        .child(status_icon),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(rgb(status_color))
                        .child(status_text),
                ),
        )
        // Center: agent details
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .flex_grow()
                .min_w(px(0.0))
                // Name + tier badge
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(theme::fg())
                                .font_weight(FontWeight::BOLD)
                                .min_w(px(0.0))
                                .child(card.agent_id.clone()),
                        )
                        .child(
                            div()
                                .text_size(px(9.0))
                                .text_color(rgb(tier_color))
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(3.0))
                                .bg(theme::bg_active())
                                .child(card.agent_type.clone()),
                        ),
                )
                // Description
                .child(
                    div()
                        .text_size(px(11.0))
                        .text_color(theme::fg_dim())
                        .min_w(px(0.0))
                        .child(card.metadata.description.clone()),
                )
                // Driver assignments
                .when(!assigned_drivers.is_empty(), |el| {
                    el.child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .flex_wrap()
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(theme::fg_faint())
                                    .child("Assigned to:"),
                            )
                            .children(assigned_drivers.iter().map(|d| {
                                div()
                                    .text_size(px(9.0))
                                    .text_color(theme::cyan())
                                    .px(px(4.0))
                                    .py(px(1.0))
                                    .rounded(px(2.0))
                                    .bg(theme::bg())
                                    .child(d.clone())
                            })),
                    )
                })
                .when(assigned_drivers.is_empty(), |el| {
                    el.child(
                        div()
                            .text_size(px(9.0))
                            .text_color(theme::fg_faint())
                            .child("Not assigned to any driver"),
                    )
                })
                // Latest finding
                .when(
                    run.and_then(|r| r.latest_finding.as_ref()).is_some(),
                    |el| {
                        let finding = run.and_then(|r| r.latest_finding.as_deref()).unwrap_or("");
                        el.child(
                            div()
                                .text_size(px(10.0))
                                .text_color(theme::fg_dim())
                                .bg(theme::bg())
                                .px(px(6.0))
                                .py(px(3.0))
                                .rounded(px(3.0))
                                .min_w(px(0.0))
                                .child(format!("💬 {}", finding)),
                        )
                    },
                )
                // Error
                .when(run.and_then(|r| r.error.as_ref()).is_some(), |el| {
                    let err = run.and_then(|r| r.error.as_deref()).unwrap_or("");
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::red())
                            .min_w(px(0.0))
                            .child(format!("Error: {}", err)),
                    )
                }),
        )
        // Right: stats column
        .child(
            div()
                .flex()
                .flex_col()
                .items_end()
                .gap(px(4.0))
                .w(px(80.0))
                .flex_shrink_0()
                // Evidence count
                .when(run.is_some(), |el| {
                    let ev = run.map(|r| r.evidence_count).unwrap_or(0);
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .child(format!("{} evidence", ev)),
                    )
                })
                // Credits charged
                .when(run.and_then(|r| r.credits_charged).is_some(), |el| {
                    let c = run.and_then(|r| r.credits_charged).unwrap_or(0.0);
                    el.child(
                        div()
                            .text_size(px(9.0))
                            .text_color(theme::fg_faint())
                            .child(format!("⚡ {:.1} cr", c)),
                    )
                })
                // Model
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(theme::fg_faint())
                        .child(card.capabilities.model.clone()),
                )
                // Total runs (lifetime)
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(theme::fg_faint())
                        .child(format!("{} runs", card.usage.total_executions)),
                ),
        )
}

fn render_local_agent_card(card: &AgentCard) -> impl IntoElement {
    let tier_color = match card.tier {
        fermi::agent_backend::agent_card::AgentTier::Curated => theme::CYAN,
        _ => theme::FG_DIM,
    };

    div()
        .w(px(300.0))
        .bg(theme::bg_elevated())
        .border_1()
        .border_color(theme::fg_faint())
        .rounded(px(6.0))
        .p(px(14.0))
        .flex()
        .flex_col()
        .gap(px(6.0))
        .hover(|s| s.border_color(rgb(tier_color)))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(theme::fg())
                        .font_weight(FontWeight::BOLD)
                        .child(card.agent_id.clone()),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .text_color(rgb(tier_color))
                        .px(px(5.0))
                        .py(px(1.0))
                        .rounded(px(3.0))
                        .bg(theme::bg_active())
                        .child(card.agent_type.clone()),
                ),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme::fg_dim())
                .min_w(px(0.0))
                .child(card.metadata.description.clone()),
        )
        .when(!card.capabilities.skills.is_empty(), |el| {
            el.child(div().flex().flex_wrap().gap(px(4.0)).children(
                card.capabilities.skills.iter().take(4).map(|s| {
                    div()
                        .text_size(px(9.0))
                        .text_color(rgb(theme::CYAN))
                        .px(px(4.0))
                        .py(px(1.0))
                        .rounded(px(2.0))
                        .bg(theme::bg())
                        .child(s.clone())
                }),
            ))
        })
        .child(
            div()
                .flex()
                .gap(px(12.0))
                .text_size(px(10.0))
                .text_color(theme::fg_faint())
                .child(card.capabilities.model.clone())
                .child(format!("{} runs", card.usage.total_executions)),
        )
}

// ═══════════════════════════════════════════════════════════════════
// Portfolio HUD — the top-of-panel "gaming console" strip
//
// Six KPI tiles computed directly from the currently-loaded
// `portfolio_forecasts` list. Rendered as a horizontal row of chips
// with a big value + label + small trend/context line.
//
// KPIs (left to right):
//   1. Book size       — total forecasts loaded (n)
//   2. Avg conviction  — mean of predicted_probability across active
//   3. “Hot”           — count of forecasts with n_recent_updates > 0
//   4. Edge vs crowd   — avg |pm_divergence_pp| across linked forecasts
//   5. Book Brier      — mean brier over resolved rows (higher tier)
//   6. Resolution rate — resolved / total (progress toward mature book)
//
// Data source is the `Vec<PortfolioForecast>` the client already has
// in `portfolio_forecasts[pid]` — no additional API round-trip. When a
// KPI can't be computed (empty book / no linked markets / no resolved
// rows yet) we render — as the value so the operator can tell "we don't
// know yet" from "we know and the value is zero".
// ═══════════════════════════════════════════════════════════════════
fn render_portfolio_hud(forecasts: &[PortfolioForecast]) -> impl IntoElement {
    let n_total = forecasts.len();
    let active: Vec<&PortfolioForecast> =
        forecasts.iter().filter(|f| f.status == "active").collect();
    let resolved: Vec<&PortfolioForecast> = forecasts
        .iter()
        .filter(|f| f.status == "resolved")
        .collect();

    // Mean conviction across ACTIVE forecasts — resolved values drift
    // toward 0/1 by definition, so including them distorts the number.
    let avg_prob = if active.is_empty() {
        None
    } else {
        let sum: f64 = active.iter().filter_map(|f| f.predicted_probability).sum();
        let n = active
            .iter()
            .filter(|f| f.predicted_probability.is_some())
            .count();
        if n == 0 {
            None
        } else {
            Some(sum / n as f64)
        }
    };

    // "Hot" — forecasts that moved in the last 7 days. A book's action
    // concentrates on a handful of files at any given time; this KPI
    // lets the operator jump straight to "where's the work happening".
    let n_hot = forecasts
        .iter()
        .filter(|f| f.n_recent_updates.unwrap_or(0) > 0)
        .count();

    // Mean |edge vs crowd| across markets that have a linked crowd
    // price. Absolute value is what matters — the sign averages out
    // and a book with a lot of two-sided divergence is more
    // interesting than a book uniformly in one direction.
    let (avg_abs_divergence, n_linked) = {
        let deltas: Vec<f64> = forecasts
            .iter()
            .filter_map(|f| f.pm_divergence_pp)
            .map(f64::abs)
            .collect();
        if deltas.is_empty() {
            (None, 0)
        } else {
            let n = deltas.len();
            (Some(deltas.iter().sum::<f64>() / n as f64), n)
        }
    };

    // Book Brier — average across resolved rows only. Same shape as
    // the per-user Brier on the Dashboard, but scoped to this book.
    let avg_brier = {
        let bs: Vec<f64> = resolved.iter().filter_map(|f| f.brier_score).collect();
        if bs.is_empty() {
            None
        } else {
            let n = bs.len();
            Some(bs.iter().sum::<f64>() / n as f64)
        }
    };

    let resolution_rate = if n_total == 0 {
        None
    } else {
        Some(resolved.len() as f64 / n_total as f64)
    };

    // Individual tile renderer. Big value on top, small label+context
    // below. Border + subtle bg gives the “read-out card” feel.
    let tile = |value: String, label: &'static str, sub: String, color: u32| {
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .px(px(12.0))
            .py(px(8.0))
            .min_w(px(110.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(theme::fg_faint())
            .bg(theme::bg_elevated())
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(color))
                    .child(value),
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(theme::fg_faint())
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(theme::fg_dim())
                    .child(sub),
            )
    };

    let dash = || "—".to_string();

    div()
        .flex()
        .flex_wrap()
        .gap(px(8.0))
        .px(px(14.0))
        .py(px(10.0))
        .border_b_1()
        .border_color(theme::fg_faint())
        .child(tile(
            n_total.to_string(),
            "BOOK SIZE",
            format!("{} active · {} resolved", active.len(), resolved.len()),
            theme::CYAN,
        ))
        .child(tile(
            avg_prob
                .map(|p| format!("{:.0}%", p * 100.0))
                .unwrap_or_else(dash),
            "AVG CONVICTION",
            format!("across {} live", active.len()),
            theme::BLUE,
        ))
        .child(tile(
            n_hot.to_string(),
            "HOT (7d)",
            if n_hot == 0 {
                "quiet book".into()
            } else {
                format!("of {} live", active.len().max(1))
            },
            if n_hot == 0 {
                theme::FG_DIM
            } else {
                theme::GOLD
            },
        ))
        // Divergence tile. Historically labeled just "|EDGE vs CROWD|"
        // with the raw number — which readers understandably confused
        // for a realised gain/loss. This is neither realised nor a P&L:
        // it's the mean absolute gap between our current model
        // probability and the crowd's current price on *open* linked
        // markets, in percentage points. It measures "how far off the
        // crowd we sit right now", not "how much we've won or lost".
        // Book Brier + Resolution rate are the actually-realised score
        // tiles beside it.
        .child(tile(
            avg_abs_divergence
                .map(|d| format!("±{:.1}pp", d))
                .unwrap_or_else(dash),
            "MODEL vs CROWD",
            if n_linked == 0 {
                "no linked markets".into()
            } else {
                format!("mean |gap| on {} live", n_linked)
            },
            if avg_abs_divergence.map(|d| d >= 5.0).unwrap_or(false) {
                theme::GOLD
            } else {
                theme::PURPLE
            },
        ))
        .child(tile(
            avg_brier.map(|b| format!("{:.3}", b)).unwrap_or_else(dash),
            "BOOK BRIER",
            if resolved.is_empty() {
                "nothing resolved yet".into()
            } else {
                format!("n={}", resolved.len())
            },
            match avg_brier {
                Some(b) if b <= 0.10 => theme::GREEN,
                Some(b) if b <= 0.20 => theme::CYAN,
                Some(b) if b <= 0.30 => theme::GOLD,
                Some(_) => theme::ORANGE,
                None => theme::FG_DIM,
            },
        ))
        .child(tile(
            resolution_rate
                .map(|r| format!("{:.0}%", r * 100.0))
                .unwrap_or_else(dash),
            "RESOLVED",
            format!("{} of {}", resolved.len(), n_total),
            theme::GREEN,
        ))
}

/// Shared mini-worm renderer used by BOTH the rollup strip and the
/// Constellation table rows.
///
/// Renders a compact horizontal bar showing the model probability (cyan
/// fill from left), a purple tick at the crowd price, and an optional
/// gold tick at the outside-view base rate. Reads as a Tufte-style
/// space-time-worm-in-miniature: at 90px wide you can eyeball
/// (a) how confident the model is, (b) how far off the crowd sits, and
/// (c) how far off the base rate sits — all without a numeric.
///
/// Width is a parameter so this can shrink for dense Constellation
/// rows (60–80px) or expand for the rollup strip (90–110px).
fn render_mini_worm(
    prob: f64,
    crowd: Option<f64>,
    base_rate: Option<f64>,
    width_px: f32,
) -> impl IntoElement {
    let clamp = |v: f64| v.clamp(0.0, 1.0) as f32;
    let model_w = width_px * clamp(prob);
    div()
        .relative()
        .w(px(width_px))
        .h(px(6.0))
        .rounded(px(3.0))
        .bg(theme::bg_active())
        // Filled model portion (cyan bar left-aligned).
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .h(px(6.0))
                .w(px(model_w))
                .rounded(px(3.0))
                .bg(rgb(theme::CYAN)),
        )
        // Crowd tick (purple, tall) at the crowd's x-coord — lets the
        // eye see the divergence pixel-for-pixel.
        .when(crowd.is_some(), |el| {
            let x = width_px * clamp(crowd.unwrap()) - 1.0;
            el.child(
                div()
                    .absolute()
                    .top(px(-2.0))
                    .left(px(x))
                    .w(px(2.0))
                    .h(px(10.0))
                    .bg(rgb(theme::PURPLE)),
            )
        })
        // Base-rate tick (gold, thinner) at the outside view. Same
        // visual grammar as the trajectory-worm's gold dashed line.
        .when(base_rate.is_some(), |el| {
            let x = width_px * clamp(base_rate.unwrap()) - 0.5;
            el.child(
                div()
                    .absolute()
                    .top(px(-2.0))
                    .left(px(x))
                    .w(px(1.0))
                    .h(px(10.0))
                    .bg(rgb(theme::GOLD)),
            )
        })
}

// ═══════════════════════════════════════════════════════════════════
// Portfolio Rollup Strip — the "constellation at a glance" band
//
// A vertical stack of one-line rows, one per forecast, showing the
// probability bar, current probability, and crowd-divergence chip.
// Ordered by pm_divergence_pp descending (biggest edges up top),
// falling back to alphabetical. Capped at 6 rows so the strip stays
// compact; the full list still lives in the Constellation table.
// ═══════════════════════════════════════════════════════════════════
fn render_portfolio_rollup_strip(forecasts: &[PortfolioForecast]) -> impl IntoElement {
    // Sort by |divergence| desc, then n_recent_updates desc, then
    // probability desc. Filter out resolved rows since they no longer
    // carry live divergence signal.
    let mut ranked: Vec<&PortfolioForecast> =
        forecasts.iter().filter(|f| f.status == "active").collect();
    ranked.sort_by(|a, b| {
        let ad = a.pm_divergence_pp.map(f64::abs).unwrap_or(-1.0);
        let bd = b.pm_divergence_pp.map(f64::abs).unwrap_or(-1.0);
        bd.partial_cmp(&ad)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                b.n_recent_updates
                    .unwrap_or(0)
                    .cmp(&a.n_recent_updates.unwrap_or(0))
            })
            .then_with(|| {
                let ap = a.predicted_probability.unwrap_or(0.0);
                let bp = b.predicted_probability.unwrap_or(0.0);
                bp.partial_cmp(&ap).unwrap_or(std::cmp::Ordering::Equal)
            })
    });
    let top: Vec<&PortfolioForecast> = ranked.into_iter().take(6).collect();

    // Empty state: nothing active or nothing linked yet. Skip the
    // strip entirely rather than render an awkward placeholder — the
    // Constellation table below already handles empty gracefully.
    if top.is_empty() {
        return div().into_any_element();
    }

    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .px(px(14.0))
        .py(px(8.0))
        .border_b_1()
        .border_color(theme::fg_faint())
        .child(
            div()
                .text_size(px(10.0))
                .text_color(theme::fg_faint())
                .font_weight(FontWeight::SEMIBOLD)
                .child("BIGGEST EDGES"),
        )
        .children(top.iter().map(|f| {
            let prob = f.predicted_probability.unwrap_or(0.0);
            let prob_pct = (prob * 100.0).round() as u32;
            let crowd_str = f
                .pm_market_price
                .map(|p| format!("crowd {:.0}%", p * 100.0));
            let (delta_str, delta_color) = match f.pm_divergence_pp {
                Some(d) if d.abs() >= 10.0 => (
                    Some(format!("{}{:.1}pp", if d >= 0.0 { "+" } else { "" }, d)),
                    theme::GOLD,
                ),
                Some(d) if d.abs() >= 3.0 => (
                    Some(format!("{}{:.1}pp", if d >= 0.0 { "+" } else { "" }, d)),
                    theme::CYAN,
                ),
                Some(d) => (
                    Some(format!("{}{:.1}pp", if d >= 0.0 { "+" } else { "" }, d)),
                    theme::FG_DIM,
                ),
                None => (None, theme::FG_FAINT),
            };
            let title = truncate(&f.question_text, 44);
            let bar = render_mini_worm(prob, f.pm_market_price, None, 90.0);

            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .py(px(2.0))
                .child(
                    div()
                        .w(px(180.0))
                        .text_size(px(11.0))
                        .text_color(theme::fg())
                        .child(title),
                )
                .child(bar)
                .child(
                    div()
                        .w(px(38.0))
                        .text_size(px(11.0))
                        .text_color(theme::cyan())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("{}%", prob_pct)),
                )
                .when(crowd_str.is_some(), |el| {
                    el.child(
                        div()
                            .w(px(70.0))
                            .text_size(px(10.0))
                            .text_color(theme::purple())
                            .child(crowd_str.clone().unwrap_or_default()),
                    )
                })
                .when(delta_str.is_some(), |el| {
                    el.child(
                        div()
                            .text_size(px(10.0))
                            .text_color(rgb(delta_color))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(delta_str.clone().unwrap_or_default()),
                    )
                })
        }))
        .into_any_element()
}

// ═══════════════════════════════════════════════════════════════════
// Portfolio Risk view (Sprint B)
//
// Six inline metrics. All computed client-side from the loaded
// PortfolioForecast list — no additional API round-trip. Values that
// are ill-defined (empty book, no resolved rows) render as — so "we
// don't know yet" is distinguishable from "we know and it's zero".
//
//   1. HHI concentration           — Herfindahl on normalised probability mass
//   2. P(any yes) — independent    — 1 - ∏(1 - pᵢ)
//   3. P(any yes) — correlated ρ   — approximation with correlation slider
//   4. Expected book Brier         — Monte Carlo over Bernoulli(pᵢ)
//   5. Drawdown scenario           — biggest edge resolves against you
//   6. Joint-tree (top 4)          — 2⁴ outcomes with joint probability
// ═══════════════════════════════════════════════════════════════════

/// Package of computed risk metrics for a portfolio. Kept separate
/// from the render fn so unit tests can exercise the math without
/// pulling in the GPUI surface.
struct PortfolioRiskMetrics {
    /// Herfindahl-Hirschman Index on normalised probability shares.
    /// 1/n = perfectly diversified; 1.0 = one bet dominates.
    hhi: Option<f64>,
    n_effective: Option<f64>,
    /// P(at least one active forecast resolves YES) assuming independence.
    p_any_yes_indep: Option<f64>,
    /// Same, adjusted for pairwise correlation ρ via a Frank-style
    /// bound. ρ = 0 recovers `p_any_yes_indep`; ρ → 1 collapses to
    /// max(pᵢ); ρ → -1 collapses to min(1, ∑ pᵢ).
    p_any_yes_corr: Option<f64>,
    /// Expected Brier over the ACTIVE book, computed via 1000-sample
    /// Monte Carlo where each active forecast resolves Bernoulli(pᵢ)
    /// and the score is (pᵢ - outcome)² averaged across the book.
    expected_brier: Option<f64>,
    /// Worst-case impact if the largest-edge forecast (biggest
    /// |Δ-vs-crowd|) resolves opposite to the model. Returns the
    /// forecast title, its p, its edge, and the Brier cost of being wrong.
    drawdown: Option<PortfolioDrawdown>,
    /// Top-4 joint outcome tree. Each entry is a 4-bit outcome pattern
    /// (bit i = forecast i resolves YES) and the joint probability
    /// assuming independence.
    joint_tree: Vec<JointOutcome>,
    /// Titles that back the joint tree, index-aligned. Rendered as
    /// column headers above the tree.
    joint_titles: Vec<String>,
}

struct PortfolioDrawdown {
    title: String,
    model_prob: f64,
    edge_pp: f64,
    brier_if_wrong: f64,
}

struct JointOutcome {
    /// Bitmask of which forecasts resolve YES. Bit 0 = first title, etc.
    mask: u8,
    joint_prob: f64,
}

fn compute_portfolio_risk(
    forecasts: &[PortfolioForecast],
    correlation_rho: f64,
) -> PortfolioRiskMetrics {
    let active: Vec<&PortfolioForecast> =
        forecasts.iter().filter(|f| f.status == "active").collect();
    let probs: Vec<f64> = active
        .iter()
        .filter_map(|f| f.predicted_probability)
        .collect();

    // HHI on normalised probability shares. When ∑p_i is zero (all
    // active forecasts pin at 0%), fall back to "we don't know".
    let (hhi, n_effective) = if probs.is_empty() {
        (None, None)
    } else {
        let total: f64 = probs.iter().sum();
        if total <= 1e-9 {
            (None, None)
        } else {
            let hhi: f64 = probs.iter().map(|p| (p / total).powi(2)).sum();
            let n_eff = if hhi > 0.0 { 1.0 / hhi } else { 0.0 };
            (Some(hhi), Some(n_eff))
        }
    };

    // Independent P(any yes) = 1 - ∏(1 - p_i).
    let p_any_yes_indep = if probs.is_empty() {
        None
    } else {
        let mut prod_no: f64 = 1.0;
        for &p in &probs {
            prod_no *= (1.0 - p).max(0.0);
        }
        Some((1.0 - prod_no).clamp(0.0, 1.0))
    };

    // Correlated P(any yes). At ρ = 0 we recover the independent
    // value. At ρ = 1 the events are perfectly positively correlated
    // — the union probability collapses to the max single event. We
    // interpolate linearly between the two bounds for the UI slider.
    // This is a rough approximation — correct in the limits, honest
    // about being an approximation in the middle — but useful for
    // eyeballing "how much does correlation move the number".
    let p_any_yes_corr = match p_any_yes_indep {
        Some(indep) => {
            let max_single = probs.iter().cloned().fold(0.0_f64, f64::max);
            let sum_probs: f64 = probs.iter().sum();
            let interp = if correlation_rho >= 0.0 {
                // Blend indep → max_single as ρ goes 0 → 1
                indep * (1.0 - correlation_rho) + max_single * correlation_rho
            } else {
                // Blend indep → min(1, ∑p) as ρ goes 0 → -1 (anti-corr
                // pushes toward mutual exclusivity, which raises P(any)
                // toward the sum of probabilities capped at 1).
                let anti = sum_probs.min(1.0);
                indep * (1.0 + correlation_rho) + anti * (-correlation_rho)
            };
            Some(interp.clamp(0.0, 1.0))
        }
        None => None,
    };

    // Expected book Brier over active forecasts. 1000-sample MC where
    // each forecast independently resolves Bernoulli(p_i). Reports the
    // mean per-forecast Brier score (comparable to the /my-stats one).
    // We use a simple xorshift for determinism across renders — the
    // number should be stable per data snapshot.
    let expected_brier = if probs.is_empty() {
        None
    } else {
        const N_SAMPLES: usize = 1000;
        let mut rng_state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            rng_state ^= rng_state << 13;
            rng_state ^= rng_state >> 7;
            rng_state ^= rng_state << 17;
            // Convert to [0, 1).
            ((rng_state >> 11) as f64) / ((1u64 << 53) as f64)
        };
        let mut total_brier = 0.0;
        for _ in 0..N_SAMPLES {
            let mut brier_sum = 0.0;
            for &p in &probs {
                let u = next();
                let outcome = if u < p { 1.0 } else { 0.0 };
                brier_sum += (p - outcome).powi(2);
            }
            total_brier += brier_sum / probs.len() as f64;
        }
        Some(total_brier / N_SAMPLES as f64)
    };

    // Drawdown — what if your biggest edge is wrong? Find the active
    // forecast with the largest absolute divergence from the crowd,
    // then compute the Brier cost if the outcome comes in opposite
    // to the model.
    let drawdown = active
        .iter()
        .filter(|f| f.pm_divergence_pp.is_some() && f.predicted_probability.is_some())
        .max_by(|a, b| {
            let ad = a.pm_divergence_pp.map(f64::abs).unwrap_or(0.0);
            let bd = b.pm_divergence_pp.map(f64::abs).unwrap_or(0.0);
            ad.partial_cmp(&bd).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|f| {
            let p = f.predicted_probability.unwrap_or(0.5);
            let edge = f.pm_divergence_pp.unwrap_or(0.0);
            // If model says p = 0.60 and edge is +5pp above crowd, the
            // "model wrong" outcome is where the crowd was right —
            // i.e. outcome = 0 (crowd expected NO). Brier cost of that
            // is (p - 0)² = p². Symmetric for negative edge.
            let outcome_if_crowd_right: f64 = if edge >= 0.0 { 0.0 } else { 1.0 };
            let brier_if_wrong = (p - outcome_if_crowd_right).powi(2);
            PortfolioDrawdown {
                title: truncate(&f.question_text, 50),
                model_prob: p,
                edge_pp: edge,
                brier_if_wrong,
            }
        });

    // Joint tree — top 4 highest-conviction (by |p - 0.5|, i.e.
    // strongest deviations from coin flip) so the tree captures the
    // book's most opinionated positions. All 2⁴ = 16 outcomes; if the
    // book has <4 active forecasts we tile to whatever's there.
    let mut top: Vec<&PortfolioForecast> = active.iter().cloned().collect();
    top.sort_by(|a, b| {
        let ac = (a.predicted_probability.unwrap_or(0.5) - 0.5).abs();
        let bc = (b.predicted_probability.unwrap_or(0.5) - 0.5).abs();
        bc.partial_cmp(&ac).unwrap_or(std::cmp::Ordering::Equal)
    });
    let top: Vec<&PortfolioForecast> = top.into_iter().take(4).collect();
    let joint_titles: Vec<String> = top.iter().map(|f| truncate(&f.question_text, 22)).collect();
    let joint_probs: Vec<f64> = top
        .iter()
        .map(|f| f.predicted_probability.unwrap_or(0.5))
        .collect();
    let n = joint_probs.len();
    let mut joint_tree: Vec<JointOutcome> = Vec::new();
    if n > 0 {
        for mask in 0u8..(1 << n) {
            let mut prob = 1.0;
            for (i, &p) in joint_probs.iter().enumerate() {
                let yes = (mask >> i) & 1 == 1;
                prob *= if yes { p } else { 1.0 - p };
            }
            joint_tree.push(JointOutcome {
                mask,
                joint_prob: prob,
            });
        }
        // Sort by joint probability descending so the most likely
        // scenario reads first.
        joint_tree.sort_by(|a, b| {
            b.joint_prob
                .partial_cmp(&a.joint_prob)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    PortfolioRiskMetrics {
        hhi,
        n_effective,
        p_any_yes_indep,
        p_any_yes_corr,
        expected_brier,
        drawdown,
        joint_tree,
        joint_titles,
    }
}

/// Render the Portfolio risk-view band. All six metrics inline, in a
/// three-column responsive layout that mirrors the HUD's visual
/// grammar (big value + label + one-line context). Includes an
/// inline correlation slider that recomputes `p_any_yes_corr` and
/// visually links the two P(any) numbers so the operator can eyeball
/// how sensitive the union is to their assumption about correlation.
fn render_portfolio_risk_view(
    forecasts: &[PortfolioForecast],
    correlation_rho: f64,
    on_rho_change: impl Fn(f64, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let m = compute_portfolio_risk(forecasts, correlation_rho);
    let dash = || "—".to_string();

    // Metric tile helper — same visual grammar as the HUD.
    let tile = |value: String, label: &'static str, sub: String, color: u32| {
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .px(px(12.0))
            .py(px(8.0))
            .min_w(px(150.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(theme::fg_faint())
            .bg(theme::bg_elevated())
            .child(
                div()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(color))
                    .child(value),
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(theme::fg_faint())
                    .child(label.to_string()),
            )
            .child(
                div()
                    .text_size(px(9.0))
                    .text_color(theme::fg_dim())
                    .child(sub),
            )
    };

    // Convert HHI to a plain-English concentration label.
    let (hhi_label, hhi_color) = match m.hhi {
        Some(h) if h >= 0.6 => ("very concentrated", theme::ORANGE),
        Some(h) if h >= 0.3 => ("concentrated", theme::GOLD),
        Some(h) if h >= 0.15 => ("balanced", theme::CYAN),
        Some(_) => ("well-diversified", theme::GREEN),
        None => ("no data", theme::FG_DIM),
    };

    // Header row + tiles.
    let mut header_row = div().flex().items_center().justify_between().child(
        div()
            .text_size(px(11.0))
            .text_color(theme::fg_faint())
            .font_weight(FontWeight::SEMIBOLD)
            .child("RISK VIEW"),
    );
    // Inline correlation slider (rendered as chips: −0.5 / 0 / 0.3 / 0.6 / 0.9).
    let rho_stops: &[f64] = &[-0.5, 0.0, 0.3, 0.6, 0.9];
    let mut slider = div().flex().items_center().gap(px(4.0)).child(
        div()
            .text_size(px(9.0))
            .text_color(theme::fg_faint())
            .child(format!("ρ = {:.1}", correlation_rho)),
    );
    let on_rho = Arc::new(on_rho_change);
    for &stop in rho_stops {
        let is_on = (stop - correlation_rho).abs() < 1e-3;
        let on_click = on_rho.clone();
        slider = slider.child(
            div()
                .id(SharedString::from(
                    format!("rho-chip-{:.1}", stop)
                        .replace('.', "_")
                        .replace('-', "n"),
                ))
                .px(px(6.0))
                .py(px(1.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(if is_on {
                    theme::cyan()
                } else {
                    theme::fg_faint()
                })
                .text_size(px(9.0))
                .text_color(if is_on {
                    theme::cyan()
                } else {
                    theme::fg_dim()
                })
                .cursor_pointer()
                .hover(|s| s.bg(theme::bg_hover()))
                .on_click(move |_ev, w, cx| on_click(stop, w, cx))
                .child(format!("{:.1}", stop)),
        );
    }
    header_row = header_row.child(slider);

    // Tiles grid — three tiles per row.
    let tiles_row_1 = div()
        .flex()
        .flex_wrap()
        .gap(px(8.0))
        .child(tile(
            m.hhi.map(|h| format!("{:.2}", h)).unwrap_or_else(dash),
            "CONCENTRATION",
            format!(
                "{} · n_eff {}",
                hhi_label,
                m.n_effective
                    .map(|n| format!("{:.1}", n))
                    .unwrap_or_else(dash),
            ),
            hhi_color,
        ))
        .child(tile(
            m.p_any_yes_indep
                .map(|p| format!("{:.1}%", p * 100.0))
                .unwrap_or_else(dash),
            "P(ANY YES) IF INDEPENDENT",
            "1 - ∏(1 - pᵢ)".into(),
            theme::CYAN,
        ))
        .child(tile(
            m.p_any_yes_corr
                .map(|p| format!("{:.1}%", p * 100.0))
                .unwrap_or_else(dash),
            "P(ANY YES) AT ρ",
            format!("correlation {:.2}", correlation_rho),
            if correlation_rho.abs() >= 0.1 {
                theme::GOLD
            } else {
                theme::FG_DIM
            },
        ));

    let tiles_row_2 = div()
        .flex()
        .flex_wrap()
        .gap(px(8.0))
        .child(tile(
            m.expected_brier
                .map(|b| format!("{:.3}", b))
                .unwrap_or_else(dash),
            "EXPECTED BRIER",
            "MC over active book (n=1000)".into(),
            match m.expected_brier {
                Some(b) if b <= 0.15 => theme::GREEN,
                Some(b) if b <= 0.25 => theme::CYAN,
                Some(b) if b <= 0.35 => theme::GOLD,
                Some(_) => theme::ORANGE,
                None => theme::FG_DIM,
            },
        ))
        .child(tile(
            m.drawdown
                .as_ref()
                .map(|d| format!("{:.3}", d.brier_if_wrong))
                .unwrap_or_else(dash),
            "DRAWDOWN IF BIG EDGE WRONG",
            m.drawdown
                .as_ref()
                .map(|d| {
                    format!(
                        "{} — model {:.0}%, edge {:+.1}pp",
                        d.title,
                        d.model_prob * 100.0,
                        d.edge_pp
                    )
                })
                .unwrap_or_else(|| "no linked markets".into()),
            theme::ORANGE,
        ))
        .child(tile(
            format!("{}", m.joint_tree.len()),
            "JOINT SCENARIOS (TOP 4)",
            // Caveat surfaces the model assumption directly on the
            // tile. Today the tree enumerates 2⁴ outcomes and computes
            // each as ∏ p_i for YES bits, ∏ (1−p_i) for NO bits —
            // classic independence. When the top-4 belong to a
            // scenario constraint (e.g. a mutex group like "EPL
            // winner"), some rows here are structurally impossible.
            // See docs/fermi/SCENARIO_TREE_DESIGN.md for the
            // scenario-aware upgrade path.
            "assumes independence · see tree ↓".to_string(),
            theme::PURPLE,
        ));

    // Joint outcome tree — up to 8 rows (half the 2⁴ space, top by
    // joint probability) with a per-forecast YES/NO bit row.
    let tree_rows: Vec<AnyElement> = if m.joint_tree.is_empty() {
        vec![]
    } else {
        let titles = m.joint_titles.clone();
        let n_titles = titles.len();
        // Header row for the tree columns.
        let mut header = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(4.0))
            .py(px(2.0))
            .child(
                div()
                    .w(px(60.0))
                    .text_size(px(9.0))
                    .text_color(theme::fg_faint())
                    .child("P(joint)"),
            );
        for t in &titles {
            header = header.child(
                div()
                    .w(px(72.0))
                    .text_size(px(9.0))
                    .text_color(theme::fg_faint())
                    .child(t.clone()),
            );
        }
        let mut rows: Vec<AnyElement> = vec![header.into_any_element()];
        for outcome in m.joint_tree.iter().take(8) {
            let mut row = div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .px(px(4.0))
                .py(px(2.0))
                .child(
                    div()
                        .w(px(60.0))
                        .text_size(px(10.0))
                        .text_color(theme::cyan())
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(format!("{:.1}%", outcome.joint_prob * 100.0)),
                );
            for i in 0..n_titles {
                let yes = (outcome.mask >> i) & 1 == 1;
                row = row.child(
                    div()
                        .w(px(72.0))
                        .text_size(px(9.0))
                        .text_color(if yes { theme::green() } else { theme::red() })
                        .child(if yes { "YES" } else { "NO" }),
                );
            }
            rows.push(row.into_any_element());
        }
        rows
    };

    // Footer summary line beneath the joint-scenario tree. Answers
    // the two questions an operator has after scanning the rows:
    //
    //   1. "Why don't these sum to 100?"  → shown mass + total-rows.
    //   2. "Should I trust the multi-YES rows?" → assumption label.
    //
    // A future scenario-aware upgrade (see design doc) turns the
    // assumption chip into either "scenario-aware · N/M valid" or
    // keeps the current "assumes independence" depending on whether
    // the top-4 belong to any scenario constraint.
    let n_shown = m.joint_tree.iter().take(8).count();
    let total_rows = m.joint_tree.len();
    let shown_mass_pct: f64 = m
        .joint_tree
        .iter()
        .take(8)
        .map(|o| o.joint_prob * 100.0)
        .sum();

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .px(px(14.0))
        .py(px(10.0))
        .border_b_1()
        .border_color(theme::fg_faint())
        .child(header_row)
        .child(tiles_row_1)
        .child(tiles_row_2)
        .when(!tree_rows.is_empty(), |el| {
            el.child(
                div()
                    .mt(px(6.0))
                    .flex()
                    .flex_col()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(theme::fg_faint())
                    .bg(theme::bg_elevated())
                    .children(tree_rows)
                    // Footer strip — shown mass + assumption label.
                    .child(
                        div()
                            .px(px(8.0))
                            .py(px(4.0))
                            .border_t_1()
                            .border_color(theme::fg_faint())
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .text_size(px(9.0))
                            .text_color(theme::fg_faint())
                            .child(div().child(format!(
                                "Σ shown = {:.1}% · {}/{} scenarios",
                                shown_mass_pct, n_shown, total_rows
                            )))
                            .child(
                                div()
                                    .text_color(rgb(theme::GOLD))
                                    .child("assumes independence"),
                            ),
                    ),
            )
        })
}

fn render_portfolio_stats_panel(stats: &PortfolioStats) -> impl IntoElement {
    let s = &stats.stats;
    let c = &stats.calibration;
    let cal_buckets: &[(&str, Option<f64>, f64)] = &[
        ("0-20", c.bucket_0_20, 0.10),
        ("20-40", c.bucket_20_40, 0.30),
        ("40-60", c.bucket_40_60, 0.50),
        ("60-80", c.bucket_60_80, 0.70),
        ("80-100", c.bucket_80_100, 0.90),
    ];

    div()
        .px(px(24.0))
        .py(px(12.0))
        .bg(theme::bg())
        .border_t_1()
        .border_color(theme::fg_faint())
        .flex()
        .flex_col()
        .gap(px(10.0))
        // Stats row
        .child(
            div()
                .flex()
                .flex_wrap()
                .gap_x(px(20.0))
                .gap_y(px(4.0))
                .children([
                    render_detail_kv(
                        "Total",
                        &s.total_forecasts
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "—".into()),
                    ),
                    render_detail_kv(
                        "Active",
                        &s.active_count
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "—".into()),
                    ),
                    render_detail_kv(
                        "Resolved",
                        &s.resolved_count
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "—".into()),
                    ),
                    render_detail_kv(
                        "Avg Brier",
                        &s.avg_brier
                            .map(|b| format!("{:.4}", b))
                            .unwrap_or_else(|| "—".into()),
                    ),
                    render_detail_kv(
                        "Best",
                        &s.best_brier
                            .map(|b| format!("{:.4}", b))
                            .unwrap_or_else(|| "—".into()),
                    ),
                ]),
        )
        // Calibration row
        .when(cal_buckets.iter().any(|(_, v, _)| v.is_some()), |el| {
            el.child(
                div()
                    .flex()
                    .items_end()
                    .gap(px(10.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child("Calibration:"),
                    )
                    .children(cal_buckets.iter().map(|(label, val, ideal)| {
                        let error = val.map(|v| (v - ideal).abs()).unwrap_or(0.5);
                        let bar_color = if error < 0.08 {
                            theme::GREEN
                        } else if error < 0.18 {
                            theme::GOLD
                        } else {
                            theme::ORANGE
                        };
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(rgb(bar_color))
                                    .font_weight(FontWeight::BOLD)
                                    .child(
                                        val.map(|v| format!("{:.0}%", v * 100.0))
                                            .unwrap_or_else(|| "—".into()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(px(9.0))
                                    .text_color(theme::fg_faint())
                                    .child(label.to_string()),
                            )
                    })),
            )
        })
        // Recent resolutions
        .when(!stats.recent_resolutions.is_empty(), |el| {
            el.child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child("Recent Resolutions"),
                    )
                    .children(stats.recent_resolutions.iter().take(3).map(|r| {
                        let outcome_color = match r.actual_outcome {
                            Some(true) => theme::GREEN,
                            Some(false) => theme::RED,
                            None => theme::FG_DIM,
                        };
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(10.0))
                                    .text_color(rgb(outcome_color))
                                    .child(match r.actual_outcome {
                                        Some(true) => "Yes",
                                        Some(false) => "No",
                                        None => "?",
                                    }),
                            )
                            .child(
                                div()
                                    .flex_grow()
                                    .text_size(px(11.0))
                                    .text_color(theme::fg_dim())
                                    .child(truncate(r.question_text.as_deref().unwrap_or("?"), 52)),
                            )
                            .when(r.brier_score.is_some(), |el| {
                                el.child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(theme::fg_faint())
                                        .child(format!("{:.3}", r.brier_score.unwrap_or(0.0))),
                                )
                            })
                    })),
            )
        })
}

fn render_detail_kv(key: &str, value: &str) -> impl IntoElement {
    div()
        .flex()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(10.0))
                .text_color(theme::fg_faint())
                .child(format!("{}:", key)),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(theme::fg())
                .child(value.to_string()),
        )
}

/// Render a mini calibration indicator from calibration bucket data.
/// Shows a 5-char string like "▁▃▅▇█" representing how well-calibrated
/// each probability bucket is (closer to diagonal = better).
fn render_calibration_mini(cal: &CalibrationData) -> String {
    let buckets = [
        (cal.bucket_0_20, 0.10),   // ideal: 10% of 0-20 bucket resolve true
        (cal.bucket_20_40, 0.30),  // ideal: 30%
        (cal.bucket_40_60, 0.50),  // ideal: 50%
        (cal.bucket_60_80, 0.70),  // ideal: 70%
        (cal.bucket_80_100, 0.90), // ideal: 90%
    ];

    let bars = ["▁", "▂", "▃", "▅", "▇"];

    buckets
        .iter()
        .map(|(actual, ideal)| {
            let actual = actual.unwrap_or(*ideal);
            let error = (actual - ideal).abs();
            // Lower error = taller bar (better calibration)
            let idx = if error < 0.05 {
                4
            } else if error < 0.10 {
                3
            } else if error < 0.15 {
                2
            } else if error < 0.25 {
                1
            } else {
                0
            };
            bars[idx]
        })
        .collect::<Vec<_>>()
        .join("")
}

// ─── OAuth localhost callback handler ─────────────────────────────────────────

/// Accept a single HTTP request on the localhost listener, extract the token
/// from the query string, and return a success page to the browser.
async fn accept_oauth_callback(listener: &tokio::net::TcpListener) -> Result<String, String> {
    let (mut stream, _addr) = listener
        .accept()
        .await
        .map_err(|e| format!("Accept failed: {}", e))?;

    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .map_err(|e| format!("Read failed: {}", e))?;
    let request = String::from_utf8_lossy(&buf[..n]);

    // Parse the GET request line: "GET /callback?token=...&user_id=... HTTP/1.1"
    let first_line = request.lines().next().unwrap_or("");
    let path = first_line.split_whitespace().nth(1).unwrap_or("");

    // Extract token from query string
    let token = path
        .split('?')
        .nth(1)
        .and_then(|qs| {
            qs.split('&')
                .find(|p| p.starts_with("token="))
                .map(|p| p.trim_start_matches("token=").to_string())
        })
        .ok_or_else(|| "No token in callback URL".to_string())?;

    // URL-decode the token (basic: just handle %xx)
    let token = urlish_decode(&token);

    // Send a nice HTML response to the browser
    let html = r#"<!DOCTYPE html>
<html><head><title>Fermi Console</title>
<style>body{font-family:system-ui;background:#1f2430;color:#cbccc6;display:flex;justify-content:center;align-items:center;height:100vh;margin:0}
.card{text-align:center;padding:40px;border-radius:12px;background:#232834;border:1px solid #33374a}
h1{color:#73d0ff;margin:0 0 12px}p{color:#707a8c;margin:0}</style></head>
<body><div class="card"><h1>✓ Signed In</h1><p>You can close this tab and return to Fermi Console.</p></div></body></html>"#;

    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.flush().await;

    Ok(token)
}

/// Basic URL decode (handles %XX sequences).
fn urlish_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('%');
                result.push_str(&hex);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Render a GitHub-style ISO-8601 timestamp as a compact relative
/// phrase ("2h ago", "3d ago"). Falls back to the raw string when
/// parsing fails so we never crash on an unexpected format.
fn pretty_timestamp(iso: &str) -> String {
    if iso.is_empty() {
        return "just now".to_string();
    }
    match chrono::DateTime::parse_from_rfc3339(iso) {
        Ok(dt) => {
            let now = chrono::Utc::now();
            let diff = now.signed_duration_since(dt.with_timezone(&chrono::Utc));
            let secs = diff.num_seconds().max(0);
            if secs < 60 {
                "just now".to_string()
            } else if secs < 3600 {
                format!("{}m ago", secs / 60)
            } else if secs < 86_400 {
                format!("{}h ago", secs / 3600)
            } else if secs < 86_400 * 30 {
                format!("{}d ago", secs / 86_400)
            } else {
                dt.format("%Y-%m-%d").to_string()
            }
        }
        Err(_) => iso.to_string(),
    }
}

// ─── Entry Point ──────────────────────────────────────────────────────────

fn main() {
    env_logger::init();

    // Start a background Tokio runtime — reqwest needs this for HTTP.
    // GPUI has its own async executor, but reqwest's Client::builder()
    // and all HTTP operations require a Tokio reactor. We keep the
    // runtime alive for the lifetime of the app.
    let tokio_rt = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
    let _guard = tokio_rt.enter();

    // Create the API client — shared across the entire app
    let api_config = ApiConfig::default();
    let api = Arc::new(ApiClient::new(api_config));

    // Create local agent registry (same as MCP server)
    let registry = if let Ok(llm_executor) = LLMExecutor::from_env() {
        log::info!("Using LLM Executor (Anthropic API)");
        Arc::new(AgentRegistry::with_executor(Arc::new(llm_executor)))
    } else {
        log::warn!("No ANTHROPIC_API_KEY found — agents will use mock executor");
        Arc::new(AgentRegistry::new())
    };

    // Load agents from filesystem — search multiple candidate paths so it works
    // regardless of whether `cargo run` is invoked from the repo root, from
    // crates/fermi-console, or from a packaged binary location.
    let agents_loaded = if let Ok(dir) = std::env::var("AGENTS_DIR") {
        match registry.load_from_directory(&dir) {
            Ok(count) => {
                log::info!("Loaded {} agents from AGENTS_DIR={}", count, dir);
                true
            }
            Err(e) => {
                log::warn!("AGENTS_DIR={} failed: {}", dir, e);
                false
            }
        }
    } else {
        false
    };

    if !agents_loaded {
        let candidates = [
            "agents/curated",          // repo root
            "../../agents/curated",    // from crates/fermi-console
            "../../../agents/curated", // from target/debug
            "../agents/curated",       // one level deep
        ];
        let mut found = false;
        for candidate in &candidates {
            let path = std::path::Path::new(candidate);
            if path.is_dir() {
                match registry.load_from_directory(candidate) {
                    Ok(count) => {
                        log::info!("Loaded {} agents from {}", count, candidate);
                        found = true;
                        break;
                    }
                    Err(e) => {
                        log::warn!("Failed to load agents from {}: {}", candidate, e);
                    }
                }
            }
        }
        if !found {
            // Last resort: try relative to the executable's own location
            if let Ok(exe) = std::env::current_exe() {
                if let Some(exe_dir) = exe.parent() {
                    let from_exe = exe_dir.join("../../agents/curated");
                    if from_exe.is_dir() {
                        match registry.load_from_directory(&from_exe) {
                            Ok(count) => {
                                log::info!("Loaded {} agents from {:?}", count, from_exe);
                            }
                            Err(e) => {
                                log::warn!("Failed to load agents from {:?}: {}", from_exe, e);
                            }
                        }
                    } else {
                        log::warn!(
                            "No agents directory found. Set AGENTS_DIR or run from repo root. Searched: {:?} and {:?}",
                            candidates,
                            from_exe
                        );
                    }
                }
            }
        }
    }

    Application::new().run(move |cx: &mut App| {
        // Register keyboard shortcuts
        cx.bind_keys([
            KeyBinding::new("secondary-1", ShowDashboard, Some("FermiConsole")),
            KeyBinding::new("secondary-2", ShowPortfolio, Some("FermiConsole")),
            KeyBinding::new("secondary-3", ShowAgentFleet, Some("FermiConsole")),
            KeyBinding::new("secondary-4", ShowComposer, Some("FermiConsole")),
            KeyBinding::new("secondary-5", ShowLeaderboard, Some("FermiConsole")),
            KeyBinding::new("secondary-6", ShowTeams, Some("FermiConsole")),
            KeyBinding::new("secondary-n", NewForecast, Some("FermiConsole")),
            KeyBinding::new("secondary-q", Quit, None),
        ]);

        cx.on_action(|_: &Quit, cx| cx.quit());
        text_input::register_key_bindings(cx);

        // Register all keyboard shortcuts
        cx.bind_keys([
            KeyBinding::new(
                "secondary-enter",
                TriggerQuestionOrchestration,
                Some("FermiConsole"),
            ),
            KeyBinding::new("secondary-r", RunSimulation, Some("FermiConsole")),
            KeyBinding::new("secondary-p", PublishForecast, Some("FermiConsole")),
            KeyBinding::new("secondary-s", SaveForecast, Some("FermiConsole")),
            KeyBinding::new("secondary-o", ImportForecast, Some("FermiConsole")),
            KeyBinding::new("secondary-e", ToggleFplSource, Some("FermiConsole")),
            KeyBinding::new("secondary-m", MinimizeWindow, Some("FermiConsole")),
            KeyBinding::new("ctrl-shift-f", ToggleFullscreen, Some("FermiConsole")),
            // Discoverability: Ctrl+/ opens the shortcuts help modal.
            // Bound with and without shift so "Ctrl+?" — the shape
            // operators reach for on US layouts — also works.
            KeyBinding::new("secondary-/", ShowShortcuts, Some("FermiConsole")),
            KeyBinding::new("secondary-shift-/", ShowShortcuts, Some("FermiConsole")),
            // Esc dismisses the shortcuts modal. Scoped to the whole
            // console for now — there's no other Escape consumer at
            // this level, and the handler is a no-op when the modal
            // isn't showing.
            KeyBinding::new("escape", DismissShortcuts, Some("FermiConsole")),
        ]);

        // Driver arrow navigation (up/down arrow keys while in the Composer)
        cx.bind_keys([
            KeyBinding::new("up", cockpit::NavigateDriverUp, Some("FermiConsole")),
            KeyBinding::new("down", cockpit::NavigateDriverDown, Some("FermiConsole")),
        ]);

        // Set native application menu bar
        cx.set_menus(build_menus());

        let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
        let api_clone = api.clone();
        let registry_clone = registry.clone();

        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Fermi Console".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |_, cx| cx.new(|cx| FermiConsole::new(api_clone, registry_clone, cx)),
            )
            .unwrap();

        // Focus the window
        window
            .update(cx, |view, window, cx| {
                window.focus(&view.focus_handle(cx));
                cx.activate(true);
            })
            .unwrap();
    });
}
