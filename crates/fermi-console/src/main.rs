//! Fermi Console — MMOG-style forecasting command center
//!
//! Built on GPUI (Zed's GPU-accelerated UI framework).
//! Sprint 2: real API integration, portfolio panel with live data.

mod api;
mod cockpit;
mod composer;
mod text_input;

use api::client::{
    ApiClient, ApiConfig, ApiError, CalibrationData, Forecast, ForecastQuery, LeaderboardEntry,
    LeaderboardQuery, LeaderboardResponse, MyStats, Portfolio,
};
use cockpit::CockpitState;
use composer::ComposerState;
use gpui::prelude::*;
use gpui::*;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use text_input::TextInput;

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

struct FermiConsole {
    active_panel: Panel,
    focus_handle: FocusHandle,

    // API client (shared, thread-safe)
    api: Arc<ApiClient>,

    // Connection state
    connected: bool,
    user_display_name: Option<String>,
    api_key_input: String,

    // Sign-in UI (in-app token entry)
    sign_in_token_input: Entity<TextInput>,
    sign_in_error: Option<String>,
    sign_in_loading: bool,

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
}

#[derive(Clone)]
struct ActivityItem {
    icon: &'static str,
    text: String,
    time: String,
    color: u32,
}

impl FermiConsole {
    fn new(api: Arc<ApiClient>, cx: &mut Context<Self>) -> Self {
        let sign_in_token_input = cx.new(|cx| {
            TextInput::new(cx)
                .with_placeholder("Paste your ABW token or API key")
                .with_label("Sign In")
                .with_large(true)
        });

        let mut console = Self {
            active_panel: Panel::Dashboard,
            focus_handle: cx.focus_handle(),
            api,
            connected: false,
            user_display_name: None,
            api_key_input: String::new(),
            sign_in_token_input,
            sign_in_error: None,
            sign_in_loading: false,
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
                Panel::Portfolio => self.fetch_forecasts(cx),
                _ => {}
            }
        }
        // Create cockpit Entity on first visit to Composer
        if panel == Panel::Composer && self.cockpit.is_none() {
            let api = self.api.clone();
            self.cockpit = Some(cx.new(|cx| CockpitState::new(api, cx)));
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
                cockpit.run_simulation(cx);
            });
            cx.notify();
        }
    }

    /// ⌘P — Publish forecast to the API for Brier tracking.
    /// After the cockpit fires the publish, we schedule a delayed
    /// refresh of portfolio + stats so the sidebar data updates.
    fn on_publish_forecast(
        &mut self,
        _: &PublishForecast,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ref cockpit) = self.cockpit {
            let cockpit = cockpit.clone();
            cockpit.update(cx, |cockpit, cx| {
                cockpit.publish_forecast(cx);
            });

            // Refresh portfolio and stats after a short delay to let
            // the publish POST complete on the server side.
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
                cockpit.show_fpl_source = !cockpit.show_fpl_source;
                if cockpit.show_fpl_source {
                    cockpit.refresh_fpl_cache(cx);
                }
                cx.notify();
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
    fn on_reset_cockpit(&mut self, _: &ResetCockpit, _window: &mut Window, cx: &mut Context<Self>) {
        let api = self.api.clone();
        self.cockpit = Some(cx.new(|cx| CockpitState::new(api, cx)));
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
                            .child("v0.2.0 — Sprint 3"),
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
            .gap(px(12.0))
            .p(px(20.0))
            .bg(theme::bg_elevated())
            .rounded(px(8.0))
            .border_1()
            .border_color(theme::fg_faint())
            .max_w(px(480.0))
            .child(
                div()
                    .text_size(px(16.0))
                    .text_color(theme::cyan())
                    .font_weight(FontWeight::BOLD)
                    .child("Sign In to Agent Bestiary World"),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(theme::fg_dim())
                    .child("Enter your ABW API key or session token to connect. You can get one from agent-bestiary.world/settings."),
            )
            // Token input field
            .child(self.sign_in_token_input.clone())
            // Sign in button
            .child(
                div()
                    .flex()
                    .gap(px(12.0))
                    .items_center()
                    .child(
                        div()
                            .id("sign-in-btn")
                            .px(px(20.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .bg(rgb(theme::CYAN))
                            .text_color(rgb(theme::BG_DEEP))
                            .text_size(px(13.0))
                            .font_weight(FontWeight::BOLD)
                            .cursor_pointer()
                            .hover(|s| s.opacity(0.85))
                            .on_click(cx.listener(|this, _event, _window, cx| {
                                this.sign_in_from_ui(cx);
                            }))
                            .child(if self.sign_in_loading {
                                "Connecting…"
                            } else {
                                "Sign In"
                            }),
                    )
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
                    }),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(theme::fg_faint())
                    .child("You can also use the app offline — Ctrl+4 to open the Composer and create local forecasts."),
            )
    }

    // ── Portfolio Panel ───────────────────────────────────────────────────

    fn render_portfolio(&self, cx: &Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
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
                            .text_size(px(22.0))
                            .text_color(theme::fg())
                            .font_weight(FontWeight::BOLD)
                            .child("Portfolio"),
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
            .when(self.connected && !self.forecasts_loading, |el| {
                el
                    // Active forecasts section
                    .when(!self.active_forecasts.is_empty(), |el| {
                        el.child(self.render_forecast_section(
                            "Active Forecasts",
                            &self.active_forecasts,
                            theme::CYAN,
                            cx,
                        ))
                    })
                    // Draft forecasts section
                    .when(!self.draft_forecasts.is_empty(), |el| {
                        el.child(self.render_forecast_section(
                            "Drafts",
                            &self.draft_forecasts,
                            theme::FG_DIM,
                            cx,
                        ))
                    })
                    // Resolved forecasts section
                    .when(!self.resolved_forecasts.is_empty(), |el| {
                        el.child(self.render_forecast_section(
                            "Resolved",
                            &self.resolved_forecasts,
                            theme::GREEN,
                            cx,
                        ))
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
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(rgb(accent))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(format!("{} ({})", title, forecasts.len())),
                    ),
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
            .when(is_selected, |el| el.child(render_forecast_detail(forecast)))
    }

    // ── Agent Fleet Panel ─────────────────────────────────────────────────

    fn render_agent_fleet_panel(&self) -> impl IntoElement {
        div()
            .id("agent-fleet-panel")
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
                            .text_color(theme::cyan())
                            .font_weight(FontWeight::BOLD)
                            .child("⚙ Agent Fleet"),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .child(format!("{} agents available", self.agent_cards.len())),
                    )
                    .when(self.agents_loading, |el| {
                        el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(theme::gold())
                                .child("⟳ Loading…"),
                        )
                    }),
            )
            // Agent grid
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(12.0))
                    .p(px(16.0))
                    .children(
                        self.agent_cards
                            .iter()
                            .map(|card| self.render_agent_card(card)),
                    )
                    .when(self.agent_cards.is_empty() && !self.agents_loading, |el| {
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
                                        .child("No agents found"),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.0))
                                        .text_color(theme::fg_faint())
                                        .mt(px(4.0))
                                        .child("Connect to the API to browse available agents"),
                                ),
                        )
                    }),
            )
    }

    fn render_agent_card(&self, card: &JsonValue) -> impl IntoElement {
        let agent_id = card
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let agent_type = card
            .get("agent_type")
            .and_then(|v| v.as_str())
            .unwrap_or("research");
        let description = card
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("No description");
        let tier = card
            .get("tier")
            .and_then(|v| v.as_str())
            .unwrap_or("standard");
        let model = card.get("model").and_then(|v| v.as_str()).unwrap_or("—");
        let tags: Vec<&str> = card
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|t| t.as_str()).collect())
            .unwrap_or_default();

        let tier_color = match tier {
            "premium" => theme::GOLD,
            "standard" => theme::CYAN,
            "free" => theme::GREEN,
            _ => theme::FG_DIM,
        };

        let type_icon = match agent_type {
            "research" => "🔍",
            "creative" => "✨",
            "system" => "⚙",
            "coherence" => "🧠",
            "game" => "🎮",
            _ => "●",
        };

        div()
            .w(px(280.0))
            .bg(theme::bg_elevated())
            .border_1()
            .border_color(theme::fg_faint())
            .rounded(px(6.0))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .hover(|s| s.border_color(rgb(tier_color)))
            .cursor_pointer()
            // Header: icon + name + tier badge
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().text_size(px(16.0)).child(type_icon.to_string()))
                    .child(
                        div()
                            .flex_grow()
                            .text_size(px(13.0))
                            .text_color(theme::fg())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(agent_id.to_string()),
                    )
                    .child(
                        div()
                            .text_size(px(9.0))
                            .text_color(rgb(tier_color))
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(3.0))
                            .bg(theme::bg_active())
                            .child(tier.to_string()),
                    ),
            )
            // Description
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(theme::fg_dim())
                    .child(truncate(description, 80)),
            )
            // Model
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .text_size(px(10.0))
                    .child(div().text_color(theme::fg_faint()).child("model:"))
                    .child(div().text_color(theme::fg_dim()).child(model.to_string())),
            )
            // Tags
            .when(!tags.is_empty(), |el| {
                el.child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(4.0))
                        .children(tags.iter().map(|tag| {
                            div()
                                .text_size(px(9.0))
                                .text_color(theme::fg_faint())
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(2.0))
                                .bg(theme::bg())
                                .child(tag.to_string())
                        })),
                )
            })
            // Performance stats (if available)
            .when(card.get("performance").is_some(), |el| {
                let perf = card.get("performance").unwrap();
                let avg_time = perf
                    .get("avg_execution_time_ms")
                    .and_then(|v| v.as_u64())
                    .map(|t| format!("{}ms", t))
                    .unwrap_or_else(|| "—".into());
                let total_runs = perf
                    .get("total_executions")
                    .and_then(|v| v.as_u64())
                    .map(|n| format!("{} runs", n))
                    .unwrap_or_else(|| "—".into());

                el.child(
                    div()
                        .flex()
                        .gap(px(12.0))
                        .mt(px(4.0))
                        .pt(px(4.0))
                        .border_t_1()
                        .border_color(theme::fg_faint())
                        .text_size(px(10.0))
                        .text_color(theme::fg_faint())
                        .child(avg_time)
                        .child(total_runs),
                )
            })
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
}

impl Focusable for FermiConsole {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FermiConsole {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .on_action(cx.listener(Self::on_toggle_fpl_source))
            .on_action(cx.listener(Self::on_minimize_window))
            .on_action(cx.listener(Self::on_zoom_window))
            .on_action(cx.listener(Self::on_toggle_fullscreen))
            .on_action(cx.listener(Self::on_reset_cockpit))
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
                        Panel::AgentFleet => self.render_agent_fleet_panel().into_any_element(),
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
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
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
            KeyBinding::new("secondary-e", ToggleFplSource, Some("FermiConsole")),
            KeyBinding::new("secondary-m", MinimizeWindow, Some("FermiConsole")),
            KeyBinding::new("ctrl-shift-f", ToggleFullscreen, Some("FermiConsole")),
        ]);

        // Set native application menu bar
        cx.set_menus(build_menus());

        let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
        let api_clone = api.clone();

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
                move |_, cx| cx.new(|cx| FermiConsole::new(api_clone, cx)),
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
