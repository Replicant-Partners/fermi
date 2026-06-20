//! Fermi Console — MMOG-style forecasting command center
//!
//! Built on GPUI (Zed's GPU-accelerated UI framework).
//! Sprint 2: real API integration, portfolio panel with live data.

mod api;
mod charts;
mod cockpit;
mod composer;
mod text_input;

use api::client::{
    ApiClient, ApiConfig, CalibrationData, CreatePortfolioRequest, Forecast,
    ForecastQuery, LeaderboardEntry, LeaderboardQuery, MyStats,
    PatchPortfolioRequest, Portfolio, PortfolioForecast, PortfolioStats,
};
use std::collections::{HashMap, HashSet};
use cockpit::CockpitState;
use composer::ComposerState;
use fermi::agent_backend::{
    agent_card::AgentCard, llm_executor::LLMExecutor, registry::AgentRegistry,
};
use gpui::prelude::*;
use gpui::*;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use text_input::TextInput;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

// ─── Menu builder ─────────────────────────────────────────────────────────────

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
        // ── Window menu ───────────────────────────────────────────
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize              Ctrl+M", MinimizeWindow),
                MenuItem::action("Zoom", ZoomWindow),
                MenuItem::action("Toggle Fullscreen     Ctrl+Shift+F", ToggleFullscreen),
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
        }
    }

    fn icon(&self) -> &'static str {
        match self {
            Panel::Dashboard => "⌂",
            Panel::Portfolio => "◈",
            Panel::AgentFleet => "⚙",
            Panel::Composer => "✎",
            Panel::Leaderboard => "⚑",
        }
    }

    fn shortcut_hint(&self) -> &'static str {
        match self {
            Panel::Dashboard => "Ctrl+1",
            Panel::Portfolio => "Ctrl+2",
            Panel::AgentFleet => "Ctrl+3",
            Panel::Composer => "Ctrl+4",
            Panel::Leaderboard => "Ctrl+5",
        }
    }

    fn all() -> &'static [Panel] {
        &[
            Panel::Dashboard,
            Panel::Portfolio,
            Panel::AgentFleet,
            Panel::Composer,
            Panel::Leaderboard,
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
    portfolio_stats_cache: HashMap<String, PortfolioStats>,
    portfolio_forecasts: HashMap<String, Vec<PortfolioForecast>>,
    portfolio_forecasts_loading: HashSet<String>,
    portfolio_rename_id: Option<String>,
    portfolio_rename_input: Entity<TextInput>,
    portfolio_confirm_delete_id: Option<String>,
    /// Sort mode for the portfolio detail's forecast rows.
    portfolio_sort_mode: PortfolioSortMode,
    /// Free-text filter for the portfolio detail. The entity owns the
    /// source-of-truth string; render reads it live with
    /// `portfolio_filter_input.read(cx).text()`. Matches case-insensitively
    /// against question_text + tags.
    portfolio_filter_input: Entity<TextInput>,

    // Commit sheet (shown on ⌘P before publishing)
    commit_sheet_showing: bool,
    commit_sheet_visibility: String,
    commit_sheet_question: String,
    commit_sheet_probability: f64,

    // Resolve sheet (record actual outcome of an active forecast)
    resolve_sheet_showing: bool,
    resolve_forecast_id: Option<String>,
    resolve_forecast_question: String,
    resolve_outcome: Option<bool>,
    resolve_loading: bool,
    resolve_error: Option<String>,

    // Toast notification (auto-dismiss after 3 s)
    // (message, icon, color)
    toast: Option<(String, &'static str, u32)>,
}

#[derive(Clone)]
struct ActivityItem {
    icon: &'static str,
    text: String,
    time: String,
    color: u32,
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
            portfolio_stats_cache: HashMap::new(),
            portfolio_forecasts: HashMap::new(),
            portfolio_forecasts_loading: HashSet::new(),
            portfolio_rename_id: None,
            portfolio_rename_input,
            portfolio_confirm_delete_id: None,
            portfolio_sort_mode: PortfolioSortMode::RecentActivity,
            portfolio_filter_input,
            commit_sheet_showing: false,
            commit_sheet_visibility: "private".into(),
            commit_sheet_question: String::new(),
            commit_sheet_probability: 0.5,
            resolve_sheet_showing: false,
            resolve_forecast_id: None,
            resolve_forecast_question: String::new(),
            resolve_outcome: None,
            resolve_loading: false,
            resolve_error: None,
            toast: None,
        };

        // Try to load API key from environment (fallback for dev)
        if let Ok(key) = std::env::var("FERMI_API_KEY").or_else(|_| std::env::var("ABW_API_KEY")) {
            console.api_key_input = key;
            console.try_connect(cx);
        }

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
            let auth_url = format!("{}/auth/{}?redirect={}", base_url, provider, callback_url);
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
                                this.user_display_name = me.display_name.clone();
                                log::info!("[oauth] Connected as: {:?}", me.display_name);
                                this.fetch_all_data(cx);
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
    fn show_toast(&mut self, message: impl Into<String>, icon: &'static str, color: u32, cx: &mut Context<Self>) {
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
                        this.user_display_name = me.display_name.clone();
                        log::info!("Connected as: {:?}", me.display_name);
                        this.fetch_all_data(cx);
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
            let result = tokio::spawn(async move {
                api.list_forecast_workspaces().await
            })
            .await;

            match result {
                Ok(Ok(resp)) => {
                    let workspaces = resp
                        .get("workspaces")
                        .and_then(|v| v.as_array())
                        .cloned()
                        .unwrap_or_default();
                    log::info!("[workspaces] Fetched {} fermi_forecast workspaces", workspaces.len());

                    // Parse workspace metadata from the list response — no per-workspace
                    // HTTP calls. Params are extracted from workspace name pattern.
                    let mut forecasts: Vec<WorkspaceForecast> = Vec::new();
                    for ws in &workspaces {
                        let ws_id = ws.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let name = ws.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let created = ws.get("created_at").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let forecast_id = ws.get("forecast_id").and_then(|v| v.as_str()).map(String::from);

                        // Parse team info from workspace name pattern:
                        // "Team Prior — Argentina (ARG)" or "Tournament Path — Group B"
                        let (team_name, team_id, group, program_type, elo) = if name.starts_with("Team Prior") {
                            // Extract team name from "Team Prior — Name (ID)"
                            let after_dash = name.strip_prefix("Team Prior — ").unwrap_or(&name);
                            let tn = after_dash.split(" (").next().unwrap_or(after_dash).to_string();
                            let tid = after_dash.split('(').nth(1)
                                .and_then(|s| s.strip_suffix(')'))
                                .map(|s| s.to_string());
                            (Some(tn), tid, None, Some("TEAM_PRIOR".to_string()), None)
                        } else if name.starts_with("Tournament Path") {
                            let grp = name.strip_prefix("Tournament Path — Group ")
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
                        let deduped: Vec<WorkspaceForecast> = forecasts.into_iter()
                            .filter(|wf| seen.insert(wf.workspace_name.clone()))
                            .collect();
                        this.workspace_forecasts = deduped;
                        this.workspace_forecasts_loading = false;
                        cx.notify();
                    }).ok();
                }
                Ok(Err(e)) => {
                    log::error!("[workspaces] Failed to fetch: {}", e);
                    this.update(cx, |this, cx| {
                        this.workspace_forecasts_loading = false;
                        cx.notify();
                    }).ok();
                }
                Err(e) => {
                    log::error!("[workspaces] Task error: {}", e);
                    this.update(cx, |this, cx| {
                        this.workspace_forecasts_loading = false;
                        cx.notify();
                    }).ok();
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

    fn fetch_forecasts(&mut self, cx: &mut Context<Self>) {
        self.forecasts_loading = true;
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
                    // Build activity feed from resolved forecasts
                    this.recent_activity = resp
                        .forecasts
                        .iter()
                        .take(8)
                        .map(|f| {
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
                            ActivityItem {
                                icon: "✓",
                                text: format!(
                                    "{} — {} (Brier {:.2})",
                                    truncate(&f.question_text, 40),
                                    outcome,
                                    brier,
                                ),
                                time: f
                                    .resolved_at
                                    .as_deref()
                                    .and_then(|s| s.split('T').next())
                                    .unwrap_or("?")
                                    .to_string(),
                                color,
                            }
                        })
                        .collect();
                    this.resolved_forecasts = resp.forecasts;
                }
                if let Ok(resp) = draft_res {
                    this.draft_forecasts = resp.forecasts;
                }
                this.forecasts_loading = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn fetch_portfolios(&mut self, cx: &mut Context<Self>) {
        self.portfolios_loading = true;
        let api = self.api.clone();

        cx.spawn(async move |this, cx| match api.list_portfolios().await {
            Ok(resp) => {
                this.update(cx, |this, cx| {
                    this.portfolios = resp.portfolios;
                    this.portfolios_loading = false;
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
            // Observe cockpit changes to drain toast notifications.
            cx.observe(&cockpit_entity, |this, cockpit_ref, cx| {
                let toasts: Vec<String> = cockpit_ref.update(cx, |state, _| {
                    std::mem::take(&mut state.pending_toasts)
                });
                for msg in toasts {
                    this.show_toast(msg, "✓", theme::GREEN, cx);
                }
            }).detach();
            self.cockpit = Some(cockpit_entity);
        }
        // Refresh data when switching to agent fleet or leaderboard
        if changed && self.connected {
            match panel {
                Panel::AgentFleet => self.fetch_agents(cx),
                Panel::Leaderboard => self.fetch_leaderboard(cx),
                _ => {}
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
            self.commit_sheet_showing = true;
            cx.notify();
        }
    }

    /// Called when the user confirms in the commit sheet.
    fn do_commit_forecast(&mut self, cx: &mut Context<Self>) {
        self.commit_sheet_showing = false;
        let visibility = self.commit_sheet_visibility.clone();

        if let Some(ref cockpit) = self.cockpit {
            let cockpit = cockpit.clone();
            cockpit.update(cx, |cockpit, cx| {
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
                cockpit.right_tab = match cockpit.right_tab {
                    crate::cockpit::RightTab::Edit => crate::cockpit::RightTab::Fpl,
                    crate::cockpit::RightTab::Fpl => crate::cockpit::RightTab::Wiki,
                    crate::cockpit::RightTab::Wiki => crate::cockpit::RightTab::Schedules,
                    crate::cockpit::RightTab::Schedules => crate::cockpit::RightTab::Trajectory,
                    crate::cockpit::RightTab::Trajectory => crate::cockpit::RightTab::Edit,
                };
                if cockpit.right_tab == crate::cockpit::RightTab::Trajectory {
                    cockpit.load_timeline(cx);
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
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .mt(px(2.0))
                            // Pulled from the crate's compile-time version so a
                            // Cargo.toml bump suffices — no stringly-typed drift
                            // between the cargo manifest and the footer label.
                            .child(format!("v{} — BayesOps", env!("CARGO_PKG_VERSION"))),
                    ),
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

    // ── Dashboard Panel ───────────────────────────────────────────────────

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
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .child(format!("🔥 {} active days (30d)", days_30d))
                            .into_any_element()
                    } else {
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::fg_faint())
                            .child("Sign in to sync forecasts and use agents")
                            .into_any_element()
                    }),
            )
            // ── Sign-in card (when not connected) ─────────────────
            .when(!self.connected, |el| el.child(self.render_sign_in_card(cx)))
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
            .child(
                // Activity feed
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
                            .px(px(16.0))
                            .py(px(12.0))
                            .border_b_1()
                            .border_color(theme::fg_faint())
                            .text_size(px(14.0))
                            .text_color(theme::fg())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Recent Activity"),
                    )
                    .child(
                        div().flex().flex_col().p(px(8.0)).gap(px(2.0)).children(
                            self.recent_activity
                                .iter()
                                .map(|item| self.render_activity_item(item)),
                        ),
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

    fn render_activity_item(&self, item: &ActivityItem) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(12.0))
            .py(px(8.0))
            .rounded(px(4.0))
            .hover(|style| style.bg(theme::bg_hover()))
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
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme::fg_dim())
                    .child(item.time.clone()),
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
                        return Err("Insufficient credits — top up your ABW balance to search Polymarket".into());
                    }
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        return Err(format!("Server error {}: {}", status.as_u16(), body.chars().take(120).collect::<String>()));
                    }

                    let bytes = resp.bytes().await.map_err(|e| format!("Failed to read body: {}", e))?;
                    let body = String::from_utf8_lossy(&bytes);
                    log::debug!(
                        "[polymarket] Raw response ({} bytes): {}",
                        bytes.len(),
                        body.chars().take(500).collect::<String>()
                    );
                    let data: serde_json::Value = serde_json::from_slice(&bytes)
                        .map_err(|e| format!("Bad response ({}): {}", e, body.chars().take(120).collect::<String>()))?;
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
        let title = self.portfolio_create_input.read(cx).text().trim().to_string();
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

        cx.spawn(async move |this, cx| {
            match api.portfolio_stats(&pid).await {
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
            }
        })
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
        self.portfolio_forecasts_loading.insert(portfolio_id.clone());
        let api = self.api.clone();
        let pid = portfolio_id.clone();

        cx.spawn(async move |this, cx| {
            match api.list_portfolio_forecasts(&pid).await {
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
            }
        })
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

        cx.spawn(async move |this, cx| {
            match api.delete_portfolio(&portfolio_id).await {
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
            }
        })
        .detach();
    }

    fn rename_portfolio(&mut self, portfolio_id: String, new_title: String, cx: &mut Context<Self>) {
        let api = self.api.clone();
        let pid = portfolio_id.clone();
        let title = new_title.clone();

        cx.spawn(async move |this, cx| {
            let req = PatchPortfolioRequest { title: Some(title), description: None };
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
                    this.update(cx, |this, cx| {
                        this.resolve_sheet_showing = false;
                        this.resolve_forecast_id = None;
                        this.resolve_loading = false;
                        // Move forecast from active to resolved in local state
                        if let Some(pos) = this.active_forecasts.iter().position(|f| f.id == forecast_id) {
                            let mut f = this.active_forecasts.remove(pos);
                            f.status = "resolved".into();
                            f.brier_score = Some(resp.brier_score);
                            f.actual_outcome = Some(resp.actual_outcome);
                            this.resolved_forecasts.insert(0, f);
                        }
                        // Invalidate portfolio caches so Brier scores refresh
                        this.portfolio_forecasts.clear();
                        this.portfolio_stats_cache.clear();
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
        let wf = self.workspace_forecasts.iter().find(|w| w.workspace_id == ws_id).cloned();
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
                    wf.as_ref().map(|w| w.workspace_name.as_str()).unwrap_or(&ws_id),
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
                let result = tokio::spawn(async move {
                    api.get_forecast(&fid).await
                }).await;

                match result {
                    Ok(Ok(forecast)) => {
                        let Some(fpl_text) = forecast.fpl_source else {
                            log::warn!("[workspace-open] forecast {} has no fpl_source", forecast.id);
                            return;
                        };
                        // Parse on the runtime thread; mutate cockpit on the UI thread.
                        let parsed = ::fermi::lexer::Lexer::new(&fpl_text)
                            .tokenize()
                            .ok()
                            .and_then(|tokens| ::fermi::parser::Parser::new(tokens).parse().ok());

                        cockpit_handle.update(cx, |cockpit, cx| {
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
                                    cockpit.predicted_probability = forecast.predicted_probability;
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
                                        text: "FPL parse failed — showing raw source only.".into(),
                                    });
                                }
                            }
                            cx.notify();
                        }).ok();
                    }
                    Ok(Err(e)) => {
                        log::error!("[workspace-open] get_forecast failed: {}", e);
                    }
                    Err(e) => {
                        log::error!("[workspace-open] task join error: {}", e);
                    }
                }
            }).detach();
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

                    cockpit_handle.update(cx, |cockpit, cx| {
                        // Wire the question text — even if FPL parse fails
                        // the user at least sees the question.
                        cockpit.question_input.update(cx, |input, cx| {
                            input.set_text(&q_text, cx);
                        });
                        cockpit.predicted_probability = prob;
                        cockpit.workspace_id = ws_id;
                        // If we have a workspace, fetch its params output
                        // so the next Ctrl+R can bind per-team scalars
                        // (elo_current, gdp_per_capita_log, …) and any
                        // BayesOps-fitted distributions (`<driver>_fitted`)
                        // into the Executor.
                        if cockpit.workspace_id.is_some() {
                            cockpit.load_workspace_params(cx);
                        }

                        // ── Polymarket hydration ────────────────────────
                        // metadata.polymarket shape is what
                        // polymarket::link_handler wrote. The pm_market_price
                        // etc. fields on the cockpit are what the right-side
                        // panel reads to render the crowd-vs-fermi delta.
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
                            }
                        }

                        if let Some(fpl) = fpl_text.as_ref() {
                            cockpit.cached_fpl = fpl.clone();
                        }

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

        // Create cockpit if needed
        if self.cockpit.is_none() {
            let api = self.api.clone();
            self.cockpit = Some(cx.new(|cx| CockpitState::new(api, self.registry.clone(), cx)));
        }

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
                    // ── Left: portfolio list ──────────────────────────
                    .child(
                        div()
                            .w(px(220.0))
                            .flex_shrink_0()
                            .flex()
                            .flex_col()
                            .border_r_1()
                            .border_color(theme::fg_faint())
                            .when(self.portfolios.is_empty(), |el| {
                                el.child(
                                    div()
                                        .px(px(14.0))
                                        .py(px(12.0))
                                        .text_size(px(11.0))
                                        .text_color(theme::fg_faint())
                                        .child("No portfolios yet."),
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
                    // ── Right: selected portfolio detail ──────────────
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_grow()
                            .when(selected.is_none(), |el| {
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
                                            }),
                                    )
                                    // Stats + calibration curve (when stats fetched)
                                    .when(self.portfolio_stats_cache.contains_key(&pid), |el| {
                                        let stats = self.portfolio_stats_cache.get(&pid).unwrap().clone();
                                        el.child(render_portfolio_stats_panel(&stats))
                                    })
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
                                        let mut filtered: Vec<PortfolioForecast> = forecasts
                                            .into_iter()
                                            .filter(|f| {
                                                if lc_filter.is_empty() { return true; }
                                                let q_match = f.question_text.to_lowercase().contains(&lc_filter);
                                                let tag_match = f
                                                    .tags
                                                    .as_ref()
                                                    .map(|t| t.iter().any(|tag| tag.to_lowercase().contains(&lc_filter)))
                                                    .unwrap_or(false);
                                                q_match || tag_match
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

                                        let el = el.child(toolbar);

                                        el.children(filtered.into_iter().map(|f| {
                                            let fid = f.id.clone();
                                            let pid_rm = pid.clone();
                                            let prob_val = f.predicted_probability.unwrap_or(0.0);
                                            let prob_pct = (prob_val * 100.0).round() as u32;
                                            let prob_color = if prob_pct >= 70 { theme::CYAN }
                                                else if prob_pct >= 40 { theme::BLUE }
                                                else { theme::FG_DIM };
                                            let status_color = match f.status.as_str() {
                                                "active" => theme::CYAN,
                                                "resolved" => theme::GREEN,
                                                _ => theme::FG_DIM,
                                            };
                                            let brier_str = f.brier_score
                                                .map(|b| format!("{:.3}", b))
                                                .unwrap_or_default();

                                            let recent_str = f
                                                .updated_at
                                                .as_deref()
                                                .map(|t| format_relative_time(t))
                                                .unwrap_or_else(|| "—".into());
                                            let pm_str = f
                                                .pm_market_price
                                                .map(|p| format!("crowd {:.0}%", p * 100.0));
                                            let delta_str = f.pm_divergence_pp.map(|d| {
                                                let sign = if d >= 0.0 { "+" } else { "" };
                                                format!("Δ {}{:.1}pp", sign, d)
                                            });
                                            // Hsla (not the u32 const) so it slots straight
                                                // into .text_color without another rgb() hop.
                                                let delta_color = match f.pm_divergence_pp {
                                                Some(d) if d.abs() >= 10.0 => theme::gold(),
                                                Some(d) if d.abs() >= 3.0 => theme::cyan(),
                                                Some(_) => theme::fg_dim(),
                                                None => theme::fg_faint(),
                                            };
                                            let movement_str = f.n_recent_updates.and_then(|n| {
                                                if n > 0 { Some(format!("{}× 7d", n)) } else { None }
                                            });

                                            let fid_click = fid.clone();
                                            div()
                                                .id(SharedString::from(format!("pf-row-{}", fid)))
                                                .px(px(14.0))
                                                .py(px(7.0))
                                                .border_b_1()
                                                .border_color(theme::fg_faint())
                                                .flex()
                                                .items_center()
                                                .gap(px(8.0))
                                                .cursor_pointer()
                                                .hover(|s| s.bg(theme::bg_hover()))
                                                .on_click(cx.listener(move |this, _event, _window, cx| {
                                                    this.open_forecast(&fid_click, cx);
                                                }))
                                                // Question text (wider truncate so team prior
                                                // names fit comfortably alongside the new
                                                // status fields).
                                                .child(
                                                    div()
                                                        .flex_grow()
                                                        .overflow_hidden()
                                                        .text_size(px(11.0))
                                                        .text_color(theme::fg())
                                                        .child(truncate(&f.question_text, 60)),
                                                )
                                                // Recent activity (relative time)
                                                .child(
                                                    div()
                                                        .text_size(px(10.0))
                                                        .text_color(theme::fg_faint())
                                                        .child(recent_str),
                                                )
                                                // Movement chip (count in last 7 days)
                                                .when(movement_str.is_some(), move |el| {
                                                    el.child(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .text_color(rgb(theme::BLUE))
                                                            .child(movement_str.unwrap()),
                                                    )
                                                })
                                                // Fermi probability pill
                                                .child(
                                                    div()
                                                        .px(px(6.0))
                                                        .py(px(2.0))
                                                        .rounded(px(4.0))
                                                        .bg(theme::bg_hover())
                                                        .text_size(px(10.0))
                                                        .text_color(rgb(prob_color))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .child(format!("{}%", prob_pct)),
                                                )
                                                // Polymarket crowd pill (when linked)
                                                .when(pm_str.is_some(), move |el| {
                                                    el.child(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .text_color(theme::fg_dim())
                                                            .child(pm_str.unwrap()),
                                                    )
                                                })
                                                // PM delta (color-graded: gold for big gaps)
                                                .when(delta_str.is_some(), move |el| {
                                                    el.child(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .text_color(delta_color)
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .child(delta_str.unwrap()),
                                                    )
                                                })
                                                // Status badge (compact)
                                                .child(
                                                    div()
                                                        .text_size(px(10.0))
                                                        .text_color(rgb(status_color))
                                                        .child(f.status.clone()),
                                                )
                                                // Brier score (resolved only)
                                                .when(!brier_str.is_empty(), move |el| {
                                                    el.child(
                                                        div()
                                                            .text_size(px(10.0))
                                                            .text_color(theme::fg_dim())
                                                            .child(brier_str),
                                                    )
                                                })
                                                // Remove button — explicit ✕ stays at the
                                                // far right with its own click handler so the
                                                // row's whole-row click can drill into the cockpit.
                                                .child(
                                                    div()
                                                        .id(SharedString::from(format!("rm-pf-{}", fid)))
                                                        .text_size(px(11.0))
                                                        .text_color(theme::fg_faint())
                                                        .cursor_pointer()
                                                        .hover(|s| s.text_color(theme::red()))
                                                        .on_click(cx.listener(move |this, _event, _window, cx| {
                                                            this.remove_from_portfolio(pid_rm.clone(), fid.clone(), cx);
                                                        }))
                                                        .child("×"),
                                                )
                                        }))
                                    })
                            }),
                    ),
            )
    }

    fn render_forecast_portfolio_row(
        &self,
        forecast_id: &str,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let fid = forecast_id.to_string();
        div()
            .px(px(24.0))
            .py(px(8.0))
            .border_t_1()
            .border_color(theme::fg_faint())
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .child("Add to portfolio:"),
            )
            .when(!self.portfolios.is_empty(), |el| {
                el.children(self.portfolios.iter().map(|p| {
                    let pid = p.id.clone();
                    let fid2 = fid.clone();
                    let label = truncate(&p.title, 18);
                    // Check if this forecast is already in this portfolio (from cache)
                    let already_in = self.portfolio_forecasts
                        .get(&p.id)
                        .map(|fs| fs.iter().any(|f| f.id == fid))
                        .unwrap_or(false);

                    if already_in {
                        div()
                            .id(SharedString::from(format!("already-in-{}-{}", pid, fid)))
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(theme::fg_faint())
                            .text_size(px(10.0))
                            .text_color(theme::fg_faint())
                            .child(format!("✓ {}", label))
                            .into_any_element()
                    } else {
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
                            .into_any_element()
                    }
                }))
            })
            .when(self.portfolios.is_empty(), |el| {
                el.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(theme::fg_faint())
                        .child("Create a portfolio above first"),
                )
            })
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
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
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
                                        .hover(|s| s.bg(rgb(theme::BG_HOVER)).border_color(rgb(theme::CYAN)))
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
                                let accent = if result.as_deref().map(|r| r.starts_with("✓")).unwrap_or(false) {
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
                                        .when(!loading, |s| {
                                            s.hover(|s| s.bg(rgb(theme::BG_HOVER)))
                                        })
                                        .on_click(cx.listener(|this, _event, _window, cx| {
                                            this.check_pm_resolutions(cx);
                                        }))
                                        .child(label),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .child(format!(
                                "{} active · {} resolved · {} drafts",
                                self.active_forecasts.len(),
                                self.resolved_forecasts.len(),
                                self.draft_forecasts.len(),
                            )),
                    ),
            )
            // ── Polymarket Search Panel ───────────────────────────────
            .when(self.pm_show_search, |el| {
                el.child(
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
                                .child(
                                    div()
                                        .flex_grow()
                                        .child(self.pm_search_input.clone()),
                                )
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
                                .child("Search active prediction markets. Select one to import as a Fermi forecast."),
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
                        .when(self.pm_search_error.is_some() && !self.pm_search_loading, |el| {
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
                        })
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
                                    .children(self.pm_search_results.iter().enumerate().map(|(i, result)| {
                                        let question_str = result.get("question")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("Unknown")
                                            .to_string();
                                        let question_display = question_str.clone();
                                        let event_title = result.get("event_title")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        let price_pct = result.get("market_price_pct")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("?")
                                            .to_string();
                                        let vol_fmt = result.get("volume_24h_fmt")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        let liq_fmt = result.get("liquidity_fmt")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        let confidence = result.get("confidence_signal")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("Low")
                                            .to_string();
                                        let pm_event_id = result.get("pm_event_id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        let pm_market_id = result.get("pm_market_id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("")
                                            .to_string();
                                        let market_price = result.get("market_price")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0);
                                        let end_date = result.get("end_date")
                                            .and_then(|v| v.as_str())
                                            .map(|s| s[..10.min(s.len())].to_string())
                                            .unwrap_or_default();
                                        let change_1w = result.get("price_change_1w")
                                            .and_then(|v| v.as_f64());
                                        let volume_24h_raw = result.get("volume_24h")
                                            .and_then(|v| v.as_f64());
                                        let liquidity_raw = result.get("liquidity")
                                            .and_then(|v| v.as_f64());

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
                                            .hover(|s| s.border_color(rgb(theme::PURPLE)).bg(rgb(theme::BG_HOVER)))
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
                                                                .child(format!("{}{:.1}pp", arrow, c * 100.0)),
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
                                                    .when(!event_title.is_empty() && event_title != question_display, |el| {
                                                        el.child(
                                                            div()
                                                                .text_size(px(9.0))
                                                                .text_color(theme::fg_faint())
                                                                .child(event_title),
                                                        )
                                                    })
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
                                    })),
                            )
                        })
                        // Empty state
                        .when(
                            !self.pm_search_loading && self.pm_search_results.is_empty() && !self.pm_show_search,
                            |el| {
                                el.child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(theme::fg_dim())
                                        .child("Search for a Polymarket question to import into Fermi."),
                                )
                            },
                        ),
                )
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
            // ── Named Portfolios ──────────────────────────────────────
            .when(self.connected, |el| {
                el.child(self.render_portfolios_section(cx))
            })
            .when(self.connected && !self.forecasts_loading, |el| {
                el
                    // Live forecasts (committed, Brier-scored)
                    .when(!self.active_forecasts.is_empty(), |el| {
                        el.child(self.render_forecast_section(
                            "Live",
                            "committed · Brier-scored",
                            &self.active_forecasts,
                            theme::CYAN,
                            cx,
                        ))
                    })
                    // Draft forecasts (saved, not committed)
                    .when(!self.draft_forecasts.is_empty(), |el| {
                        el.child(self.render_forecast_section(
                            "Drafts",
                            "saved · not committed",
                            &self.draft_forecasts,
                            theme::FG_DIM,
                            cx,
                        ))
                    })
                    // Resolved forecasts
                    .when(!self.resolved_forecasts.is_empty(), |el| {
                        el.child(self.render_forecast_section(
                            "Resolved",
                            "",
                            &self.resolved_forecasts,
                            theme::GREEN,
                            cx,
                        ))
                    })
                    // Workspace forecasts (from ABW fermi_forecast app)
                    .when(self.connected, |el| {
                        let count = self.workspace_forecasts.len();
                        let collapsed = self.workspace_section_collapsed;

                        // Group by program type
                        let team_priors: Vec<&WorkspaceForecast> = self.workspace_forecasts.iter()
                            .filter(|wf| wf.program_type.as_deref() == Some("TEAM_PRIOR"))
                            .collect();
                        let tournament_paths: Vec<&WorkspaceForecast> = self.workspace_forecasts.iter()
                            .filter(|wf| wf.program_type.as_deref() == Some("TOURNAMENT_PATH"))
                            .collect();
                        let other: Vec<&WorkspaceForecast> = self.workspace_forecasts.iter()
                            .filter(|wf| wf.program_type.as_deref() != Some("TEAM_PRIOR")
                                && wf.program_type.as_deref() != Some("TOURNAMENT_PATH"))
                            .collect();

                        el.child(
                            div()
                                .flex()
                                .flex_col()
                                .bg(theme::bg_elevated())
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(theme::fg_faint())
                                // Collapsible header
                                .child(
                                    div()
                                        .id("ws-section-header")
                                        .px(px(16.0))
                                        .py(px(10.0))
                                        .border_b_1()
                                        .border_color(theme::fg_faint())
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme::bg_hover()))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.workspace_section_collapsed = !this.workspace_section_collapsed;
                                            cx.notify();
                                        }))
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(theme::fg_faint())
                                                .child(if collapsed { "▶" } else { "▼" }),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .text_color(theme::fg())
                                                .font_weight(FontWeight::BOLD)
                                                .child(if self.workspace_forecasts_loading {
                                                    "Workspaces (loading…)".to_string()
                                                } else {
                                                    format!("Workspaces ({})", count)
                                                }),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(theme::fg_faint())
                                                .child(format!(
                                                    "{} teams · {} groups",
                                                    team_priors.len(),
                                                    tournament_paths.len(),
                                                )),
                                        ),
                                )
                                // Content (hidden when collapsed)
                                .when(!collapsed && !team_priors.is_empty(), |el| {
                                    el.child(
                                        div()
                                            .px(px(16.0))
                                            .py(px(4.0))
                                            .text_size(px(9.0))
                                            .text_color(theme::fg_faint())
                                            .child(format!("Team Priors ({})", team_priors.len())),
                                    )
                                })
                                .when(!collapsed, |el| {
                                    el.children(team_priors.iter().chain(tournament_paths.iter()).chain(other.iter()).map(|wf| {
                                    let ws_id = wf.workspace_id.clone();
                                    let display_name = wf.team_name.as_deref()
                                        .unwrap_or(&wf.workspace_name);
                                    let group_label = wf.group.as_ref()
                                        .map(|g| format!("Group {}", g))
                                        .unwrap_or_default();
                                    let prob_label = wf.probability
                                        .map(|p| format!("{:.1}%", p * 100.0))
                                        .unwrap_or_else(|| "—".to_string());
                                    let elo_label = wf.elo
                                        .map(|e| format!("Elo {:.0}", e))
                                        .unwrap_or_default();
                                    let ptype = wf.program_type.as_deref().unwrap_or("FORECAST");

                                    div()
                                        .id(ElementId::Name(format!("ws-{}", ws_id).into()))
                                        .px(px(16.0))
                                        .py(px(8.0))
                                        .border_b_1()
                                        .border_color(theme::bg_hover())
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme::bg_hover()))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.open_workspace_forecast(&ws_id, cx);
                                        }))
                                        .flex()
                                        .items_center()
                                        .gap(px(12.0))
                                        // Probability
                                        .child(
                                            div()
                                                .w(px(55.0))
                                                .text_size(px(14.0))
                                                .text_color(theme::cyan())
                                                .font_weight(FontWeight::BOLD)
                                                .child(prob_label),
                                        )
                                        // Team name
                                        .child(
                                            div()
                                                .flex_grow()
                                                .min_w(px(0.0))
                                                .text_size(px(12.0))
                                                .text_color(theme::fg())
                                                .child(display_name.to_string()),
                                        )
                                        // Group + Elo
                                        .child(
                                            div()
                                                .flex()
                                                .gap(px(8.0))
                                                .text_size(px(9.0))
                                                .text_color(theme::fg_dim())
                                                .when(!group_label.is_empty(), |el| {
                                                    el.child(group_label)
                                                })
                                                .when(!elo_label.is_empty(), |el| {
                                                    el.child(elo_label)
                                                }),
                                        )
                                        // Type badge
                                        .child(
                                            div()
                                                .text_size(px(8.0))
                                                .text_color(theme::fg_faint())
                                                .px(px(4.0))
                                                .py(px(1.0))
                                                .rounded(px(2.0))
                                                .bg(theme::bg())
                                                .child(ptype.to_string()),
                                        )
                                }))
                                })  // close .when(!collapsed)
                        )   // close el.child (the section div)
                    })  // close .when(self.connected)
                    // Local forecasts (saved to disk)
                    .when(!self.local_forecasts.is_empty(), |el| {
                        el.child(
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
                                                .text_color(rgb(theme::GOLD))
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .child(format!(
                                                    "Lab ({})",
                                                    self.local_forecasts.len()
                                                )),
                                        )
                                        .child(
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(theme::fg_faint())
                                                .child("on disk · not synced"),
                                        ),
                                )
                                .child(div().flex().flex_col().children(
                                    self.local_forecasts.iter().map(|forecast| {
                                        {
                                            let path =
                                                format!("forecasts/{}.fpl", forecast.filename);
                                            div()
                                                .id(SharedString::from(format!(
                                                    "local-forecast-{}",
                                                    forecast.filename
                                                )))
                                                .cursor_pointer()
                                                .on_click(cx.listener(
                                                    move |this, _event, _window, cx| {
                                                        // Load forecast and switch to composer
                                                        if this.cockpit.is_none() {
                                                            let api = this.api.clone();
                                                            this.cockpit = Some(cx.new(|cx| {
                                                                CockpitState::new(
                                                                    api,
                                                                    this.registry.clone(),
                                                                    cx,
                                                                )
                                                            }));
                                                        }
                                                        if let Some(ref cockpit) = this.cockpit {
                                                            let cockpit = cockpit.clone();
                                                            let p = path.clone();
                                                            cockpit.update(cx, |cockpit, cx| {
                                                                cockpit.load_forecast(&p, cx);
                                                            });
                                                        }
                                                        this.active_panel = Panel::Composer;
                                                        cx.notify();
                                                    },
                                                ))
                                                .flex()
                                                .items_center()
                                                .gap(px(12.0))
                                                .px(px(16.0))
                                                .py(px(10.0))
                                                .border_b_1()
                                                .border_color(theme::fg_faint())
                                                .hover(|s| s.bg(theme::bg_hover()))
                                                // Inside view probability
                                                .child(
                                                    div()
                                                        .flex()
                                                        .flex_col()
                                                        .items_center()
                                                        .w(px(70.0))
                                                        .child(
                                                            div()
                                                                .text_size(px(16.0))
                                                                .text_color(rgb(theme::CYAN))
                                                                .font_weight(FontWeight::BOLD)
                                                                .child(format!(
                                                                    "{:.2}%",
                                                                    forecast.probability * 100.0
                                                                )),
                                                        )
                                                        .when(forecast.base_rate > 0.0, |el| {
                                                            let div_pp = (forecast.probability
                                                                - forecast.base_rate)
                                                                * 100.0;
                                                            let div_color = if div_pp > 0.0 {
                                                                theme::GREEN
                                                            } else {
                                                                theme::RED
                                                            };
                                                            el.child(
                                                                div()
                                                                    .text_size(px(9.0))
                                                                    .text_color(rgb(div_color))
                                                                    .child(format!(
                                                                        "vs {:.1}%",
                                                                        forecast.base_rate * 100.0
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
                                                                .text_size(px(13.0))
                                                                .text_color(theme::fg())
                                                                .child(forecast.question.clone()),
                                                        )
                                                        .child(
                                                            div()
                                                                .flex()
                                                                .gap(px(8.0))
                                                                .text_size(px(10.0))
                                                                .text_color(theme::fg_faint())
                                                                .child(format!(
                                                                    "v{}",
                                                                    forecast.version
                                                                ))
                                                                .child(format!(
                                                                    "{} drivers",
                                                                    forecast.driver_count
                                                                ))
                                                                .child(format!(
                                                                    "{} evidence",
                                                                    forecast.evidence_count
                                                                ))
                                                                .child(forecast.timestamp.clone()),
                                                        ),
                                                )
                                                // Confidence badge
                                                .when(forecast.confidence > 0.0, |el| {
                                                    let (label, color) =
                                                        if forecast.confidence > 0.7 {
                                                            ("High", theme::GREEN)
                                                        } else if forecast.confidence > 0.4 {
                                                            ("Med", theme::GOLD)
                                                        } else {
                                                            ("Low", theme::RED)
                                                        };
                                                    el.child(
                                                        div()
                                                            .text_size(px(9.0))
                                                            .text_color(rgb(color))
                                                            .px(px(5.0))
                                                            .py(px(2.0))
                                                            .rounded(px(3.0))
                                                            .bg(theme::bg())
                                                            .child(label),
                                                    )
                                                })
                                                 // Mini inside/outside view comparison sparkline
                                                .when(forecast.version_probs.len() > 1, |el| {
                                                    let history: Vec<crate::charts::IndexPoint> =
                                                        forecast
                                                            .version_probs
                                                            .iter()
                                                            .enumerate()
                                                            .map(|(i, &p)| {
                                                                crate::charts::IndexPoint {
                                                                    label: format!("v{}", i + 1),
                                                                    inside_view: p * 100.0,
                                                                    outside_view: forecast
                                                                        .base_rate
                                                                        * 100.0,
                                                                    crowd_price: None,
                                                                }
                                                            })
                                                            .collect();
                                                    let chart_w = 100u32;
                                                    let chart_h = 28u32;
                                                    let rgb_buf = crate::charts::render_index_chart(
                                                        &history,
                                                        history.len() - 1,
                                                        chart_w,
                                                        chart_h,
                                                    );
                                                    let render_img =
                                                        crate::charts::rgb_to_render_image(
                                                            &rgb_buf, chart_w, chart_h,
                                                        );
                                                    // Divergence label: show how far inside is from outside
                                                    let latest = forecast.version_probs.last().copied().unwrap_or(0.0);
                                                    let base = forecast.base_rate;
                                                    let divergence_pp = ((latest - base) * 100.0).round() as i32;
                                                    let div_label = if divergence_pp.abs() < 2 {
                                                        "≈ base rate".to_string()
                                                    } else if divergence_pp > 0 {
                                                        format!("+{}pp vs base", divergence_pp)
                                                    } else {
                                                        format!("{}pp vs base", divergence_pp)
                                                    };
                                                    let div_color = if divergence_pp.abs() < 5 {
                                                        theme::FG_FAINT
                                                    } else if divergence_pp.abs() < 15 {
                                                        theme::GOLD
                                                    } else {
                                                        theme::RED
                                                    };
                                                    el.child(
                                                        div()
                                                            .flex()
                                                            .flex_col()
                                                            .gap(px(2.0))
                                                            .child(
                                                                gpui::img(gpui::ImageSource::Render(render_img))
                                                                    .w(gpui::px(chart_w as f32))
                                                                    .h(gpui::px(chart_h as f32)),
                                                            )
                                                            .child(
                                                                div()
                                                                    .flex()
                                                                    .items_center()
                                                                    .gap(px(4.0))
                                                                    .child(
                                                                        div()
                                                                            .text_size(px(8.0))
                                                                            .text_color(theme::cyan())
                                                                            .child("in"),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .text_size(px(8.0))
                                                                            .text_color(rgb(theme::FG_FAINT))
                                                                            .child("↔"),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .text_size(px(8.0))
                                                                            .text_color(theme::gold())
                                                                            .child("out"),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .text_size(px(8.0))
                                                                            .text_color(rgb(div_color))
                                                                            .min_w(px(0.0))
                                                                            .child(div_label),
                                                                    ),
                                                            )
                                                    )
                                                })
                                        }
                                    }),
                                )),
                        )
                    })
                    // Empty state
                    .when(
                        self.active_forecasts.is_empty()
                            && self.draft_forecasts.is_empty()
                            && self.resolved_forecasts.is_empty(),
                        |el| {
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
                                            .child("No forecasts yet"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(12.0))
                                            .text_color(theme::fg_faint())
                                            .child("Create your first forecast with ⌘N"),
                                    ),
                            )
                        },
                    )
            })
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
                    // Visibility badge (Live forecasts only)
                    .when(forecast.status == "active", |el| {
                        let (icon, label, color) = match forecast.visibility.as_str() {
                            "public" => ("🌐", "public", theme::CYAN),
                            "team" => ("👥", "team", theme::BLUE),
                            _ => ("🔒", "private", theme::FG_DIM),
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
        let fermi_agents: Vec<_> = cards
            .iter()
            .filter(|c| c.metadata.tags.iter().any(|t| t == "fermi-orchestra"))
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
            // ── Agent cards ───────────────────────────────────────────
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .p(px(16.0))
                    .children(fermi_agents.iter().map(|card| {
                        let agent_id = &card.agent_id;
                        let run = agent_runs.iter().find(|r| r.agent_name == *agent_id);
                        let drivers = assigned_map.get(agent_id).cloned().unwrap_or_default();
                        render_fleet_agent_row(card, run, &drivers)
                    }))
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
                    }),
            )
    }

    // ── Leaderboard Panel ─────────────────────────────────────────────────

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
                                            .child(
                                                div()
                                                    .text_size(px(18.0))
                                                    .child("🔒"),
                                            )
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
                                                    .child("only you"),
                                            )
                                    })
                                    // Team option
                                    .child({
                                        let is_sel = selected_vis == "team";
                                        div()
                                            .id("vis-team")
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap(px(4.0))
                                            .flex_1()
                                            .p(px(12.0))
                                            .rounded(px(8.0))
                                            .border_1()
                                            .border_color(if is_sel {
                                                rgb(theme::BLUE)
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
                                                this.commit_sheet_visibility = "team".into();
                                                cx.notify();
                                            }))
                                            .child(
                                                div()
                                                    .text_size(px(18.0))
                                                    .child("👥"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(11.0))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .text_color(rgb(theme::FG))
                                                    .child("Team"),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(9.0))
                                                    .text_color(theme::fg_faint())
                                                    .child("team Brier"),
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
                                            .child(
                                                div()
                                                    .text_size(px(18.0))
                                                    .child("🌐"),
                                            )
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
                                    .hover(|s| s.bg(rgb(theme::BG_HOVER)).text_color(rgb(theme::FG)))
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

    fn render_resolve_sheet(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let question = truncate(&self.resolve_forecast_question, 80);
        let selected = self.resolve_outcome;
        let has_selection = selected.is_some();

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
                                    .child("Record the actual outcome. This locks in your Brier score."),
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
                                            .child(
                                                div()
                                                    .text_size(px(24.0))
                                                    .child("✓"),
                                            )
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
                                            .child(
                                                div()
                                                    .text_size(px(24.0))
                                                    .child("✗"),
                                            )
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
                                .child(
                                    self.resolve_error
                                        .as_deref()
                                        .unwrap_or("")
                                        .to_string(),
                                ),
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
                                        el.bg(theme::bg_hover())
                                            .text_color(theme::fg_faint())
                                    })
                                    .child(if self.resolve_loading {
                                        "Resolving…"
                                    } else {
                                        "Confirm"
                                    }),
                            ),
                    ),
            )
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

        div()
            .key_context("FermiConsole")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::on_show_dashboard))
            .on_action(cx.listener(Self::on_show_portfolio))
            .on_action(cx.listener(Self::on_show_agent_fleet))
            .on_action(cx.listener(Self::on_show_composer))
            .on_action(cx.listener(Self::on_show_leaderboard))
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
                // Main content area
                div().flex().flex_col().flex_grow().overflow_hidden().child(
                    match self.active_panel {
                        Panel::Dashboard => self.render_dashboard(cx).into_any_element(),
                        Panel::Portfolio => self.render_portfolio(cx).into_any_element(),
                        Panel::AgentFleet => self.render_agent_fleet_panel(cx).into_any_element(),
                        Panel::Composer => {
                            if let Some(ref cockpit_entity) = self.cockpit {
                                cockpit::render_cockpit(cockpit_entity).into_any_element()
                            } else {
                                // Shouldn't happen — navigate() creates it
                                composer::render_composer(&self.composer).into_any_element()
                            }
                        }
                        Panel::Leaderboard => self.render_leaderboard_panel().into_any_element(),
                    },
                ),
            )
            // Commit sheet overlay (⌘P)
            .children(commit_overlay)
            // Resolve sheet overlay
            .children(resolve_overlay)
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
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(color))
                                    .child(icon),
                            )
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

/// Render an RFC3339 timestamp as a compact "now / 5m / 3h / 2d / 4w / 8mo / 2y"
/// relative string for portfolio rows. Falls back to "—" on parse failure
/// rather than poisoning the whole list with a panic.
fn format_relative_time(rfc3339: &str) -> String {
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
                        let finding = run
                            .and_then(|r| r.latest_finding.as_deref())
                            .unwrap_or("");
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
                .when(
                    run.and_then(|r| r.credits_charged).is_some(),
                    |el| {
                        let c = run.and_then(|r| r.credits_charged).unwrap_or(0.0);
                        el.child(
                            div()
                                .text_size(px(9.0))
                                .text_color(theme::fg_faint())
                                .child(format!("⚡ {:.1} cr", c)),
                        )
                    },
                )
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
        .when(
            cal_buckets.iter().any(|(_, v, _)| v.is_some()),
            |el| {
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
                        }))
                )
            },
        )
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
                                    .child(truncate(
                                        r.question_text.as_deref().unwrap_or("?"),
                                        52,
                                    )),
                            )
                            .when(r.brier_score.is_some(), |el| {
                                el.child(
                                    div()
                                        .text_size(px(10.0))
                                        .text_color(theme::fg_faint())
                                        .child(format!(
                                            "{:.3}",
                                            r.brier_score.unwrap_or(0.0)
                                        )),
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

// ─── Entry Point ──────────────────────────────────────────────────────────────

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
