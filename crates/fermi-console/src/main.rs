//! Fermi Console — MMOG-style forecasting command center
//!
//! Built on GPUI (Zed's GPU-accelerated UI framework).
//! Sprint 2: real API integration, portfolio panel with live data.

mod api;
mod cockpit;
mod composer;
mod text_input;

use api::client::{ApiClient, ApiConfig, ApiError, Forecast, ForecastQuery, MyStats, Portfolio};
use cockpit::CockpitState;
use composer::ComposerState;
use gpui::prelude::*;
use gpui::*;
use std::sync::Arc;

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
            Panel::Dashboard => "⌘1",
            Panel::Portfolio => "⌘2",
            Panel::AgentFleet => "⌘3",
            Panel::Composer => "⌘4",
            Panel::Leaderboard => "⌘5",
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

    // Research Cockpit state (OODA loop workspace)
    cockpit: Option<CockpitState>,
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
        let mut console = Self {
            active_panel: Panel::Dashboard,
            focus_handle: cx.focus_handle(),
            api,
            connected: false,
            user_display_name: None,
            api_key_input: String::new(),
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
        };

        // Try to load API key from environment
        if let Ok(key) = std::env::var("FERMI_API_KEY").or_else(|_| std::env::var("ABW_API_KEY")) {
            console.api_key_input = key;
            console.try_connect(cx);
        }

        console
    }

    // ── API connection ────────────────────────────────────────────────

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
                        this.user_display_name = me.display_name.clone();
                        log::info!("Connected as: {:?}", me.display_name);
                        this.fetch_all_data(cx);
                    })
                    .ok();
                }
                Err(e) => {
                    log::error!("Auth failed: {}", e);
                    api.clear_api_key().await;
                    this.update(cx, |this, _cx| {
                        this.connected = false;
                        this.user_display_name = None;
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
        // Create cockpit on first visit to Composer
        if panel == Panel::Composer && self.cockpit.is_none() {
            self.cockpit = Some(CockpitState::new(self.api.clone(), &mut **cx));
        }
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
                // Logo / title
                div()
                    .px(px(16.0))
                    .py(px(20.0))
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
                            .mt(px(4.0))
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
                            .child("v0.1.0 — Sprint 2"),
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

    fn render_dashboard(&self) -> impl IntoElement {
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
                            .text_color(theme::gold())
                            .child("Set FERMI_API_KEY or ABW_API_KEY to connect")
                            .into_any_element()
                    }),
            )
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

    // ── Portfolio Panel ───────────────────────────────────────────────────

    fn render_portfolio(&self) -> impl IntoElement {
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
                        ))
                    })
                    // Draft forecasts section
                    .when(!self.draft_forecasts.is_empty(), |el| {
                        el.child(self.render_forecast_section(
                            "Drafts",
                            &self.draft_forecasts,
                            theme::FG_DIM,
                        ))
                    })
                    // Resolved forecasts section
                    .when(!self.resolved_forecasts.is_empty(), |el| {
                        el.child(self.render_forecast_section(
                            "Resolved",
                            &self.resolved_forecasts,
                            theme::GREEN,
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
                    .children(forecasts.iter().map(|f| self.render_forecast_row(f))),
            )
    }

    fn render_forecast_row(&self, forecast: &Forecast) -> impl IntoElement {
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

        div()
            .id(SharedString::from(format!("forecast-{}", forecast.id)))
            .flex()
            .items_center()
            .gap(px(12.0))
            .px(px(16.0))
            .py(px(10.0))
            .border_b_1()
            .border_color(theme::fg_faint())
            .cursor_pointer()
            .hover(|style| style.bg(theme::bg_hover()))
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
                                el.child(forecast.domain.as_deref().unwrap_or("").to_string())
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
    }

    // ── Placeholder Panels ────────────────────────────────────────────────

    fn render_placeholder(&self, panel: Panel) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(16.0))
            .child(
                div()
                    .text_size(px(48.0))
                    .text_color(theme::fg_faint())
                    .child(panel.icon()),
            )
            .child(
                div()
                    .text_size(px(20.0))
                    .text_color(theme::fg_dim())
                    .child(panel.label()),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(theme::fg_faint())
                    .child("Coming next sprint"),
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
            .flex()
            .size_full()
            .bg(theme::bg())
            .text_color(theme::fg())
            .font_family("Berkeley Mono, JetBrains Mono, Menlo, monospace")
            .child(
                // Sidebar
                self.render_sidebar(cx),
            )
            .child(
                // Main content area
                div().flex().flex_col().flex_grow().overflow_hidden().child(
                    match self.active_panel {
                        Panel::Dashboard => self.render_dashboard().into_any_element(),
                        Panel::Portfolio => self.render_portfolio().into_any_element(),
                        Panel::Composer => {
                            if let Some(ref cockpit_state) = self.cockpit {
                                cockpit::render_cockpit(cockpit_state, cx).into_any_element()
                            } else {
                                // Shouldn't happen — navigate() creates it
                                composer::render_composer(&self.composer).into_any_element()
                            }
                        }
                        other => self.render_placeholder(other).into_any_element(),
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

// ─── Entry Point ──────────────────────────────────────────────────────────────

fn main() {
    env_logger::init();

    // Create the API client — shared across the entire app
    let api_config = ApiConfig::default();
    let api = Arc::new(ApiClient::new(api_config));

    Application::new().run(move |cx: &mut App| {
        // Register keyboard shortcuts
        cx.bind_keys([
            KeyBinding::new("cmd-1", ShowDashboard, Some("FermiConsole")),
            KeyBinding::new("cmd-2", ShowPortfolio, Some("FermiConsole")),
            KeyBinding::new("cmd-3", ShowAgentFleet, Some("FermiConsole")),
            KeyBinding::new("cmd-4", ShowComposer, Some("FermiConsole")),
            KeyBinding::new("cmd-5", ShowLeaderboard, Some("FermiConsole")),
            KeyBinding::new("cmd-n", NewForecast, Some("FermiConsole")),
            KeyBinding::new("cmd-q", Quit, None),
        ]);

        cx.on_action(|_: &Quit, cx| cx.quit());
        text_input::register_key_bindings(cx);

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
