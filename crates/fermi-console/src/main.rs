//! Fermi Console — MMOG-style forecasting command center
//!
//! Built on GPUI (Zed's GPU-accelerated UI framework).
//! This is the Phase 0 spike: prove the shell works, render the dashboard,
//! and establish the entity/view patterns we'll use throughout.

use gpui::prelude::*;
use gpui::*;

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
    // Mock data for the dashboard — will be replaced with real data from SQLite + API
    brier_score: f64,
    active_forecasts: u32,
    resolved_forecasts: u32,
    agents_online: u32,
    agents_total: u32,
    streak_days: u32,
    rank: u32,
    recent_activity: Vec<ActivityItem>,
}

#[derive(Clone)]
struct ActivityItem {
    icon: &'static str,
    text: String,
    time: String,
    color: u32,
}

impl FermiConsole {
    fn new(cx: &mut Context<Self>) -> Self {
        Self {
            active_panel: Panel::Dashboard,
            focus_handle: cx.focus_handle(),
            // Mock data — Phase 1 will pull from SQLite + API
            brier_score: 0.187,
            active_forecasts: 12,
            resolved_forecasts: 47,
            agents_online: 8,
            agents_total: 14,
            streak_days: 23,
            rank: 142,
            recent_activity: vec![
                ActivityItem {
                    icon: "✓",
                    text: "AMD forecast resolved — Brier 0.12".into(),
                    time: "2h ago".into(),
                    color: theme::GREEN,
                },
                ActivityItem {
                    icon: "⚙",
                    text: "macro_forecaster executed (Haiku, 1.2s)".into(),
                    time: "3h ago".into(),
                    color: theme::BLUE,
                },
                ActivityItem {
                    icon: "◈",
                    text: "New forecast: EU AI Act impact on startups".into(),
                    time: "5h ago".into(),
                    color: theme::CYAN,
                },
                ActivityItem {
                    icon: "⚑",
                    text: "Climbed to rank #142 (+3)".into(),
                    time: "1d ago".into(),
                    color: theme::GOLD,
                },
                ActivityItem {
                    icon: "✓",
                    text: "Tesla Q4 forecast resolved — Brier 0.31".into(),
                    time: "1d ago".into(),
                    color: theme::ORANGE,
                },
                ActivityItem {
                    icon: "⚙",
                    text: "sentiment_analyzer ran on 3 forecasts".into(),
                    time: "2d ago".into(),
                    color: theme::BLUE,
                },
            ],
        }
    }

    fn navigate(&mut self, panel: Panel, cx: &mut Context<Self>) {
        self.active_panel = panel;
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
                            .text_color(theme::fg_dim())
                            .child("● Connected to ABW"),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(theme::fg_dim())
                            .mt(px(2.0))
                            .child("v0.1.0 — Phase 0 Spike"),
                    ),
            )
    }

    fn render_nav_item(&self, panel: Panel, _cx: &Context<Self>) -> impl IntoElement {
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
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(theme::fg_dim())
                            .child(format!("🔥 {} day streak", self.streak_days)),
                    ),
            )
            .child(
                // Stats cards row
                div()
                    .flex()
                    .gap(px(16.0))
                    .child(self.render_stat_card(
                        "Brier Score",
                        &format!("{:.3}", self.brier_score),
                        "Lower is better",
                        theme::GREEN,
                    ))
                    .child(self.render_stat_card(
                        "Active Forecasts",
                        &self.active_forecasts.to_string(),
                        &format!("{} resolved", self.resolved_forecasts),
                        theme::CYAN,
                    ))
                    .child(self.render_stat_card(
                        "Agent Fleet",
                        &format!("{}/{}", self.agents_online, self.agents_total),
                        "agents online",
                        theme::BLUE,
                    ))
                    .child(self.render_stat_card(
                        "Global Rank",
                        &format!("#{}", self.rank),
                        "↑ 3 this week",
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
                    .child("Coming in Phase 1"),
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
                        other => self.render_placeholder(other).into_any_element(),
                    },
                ),
            )
    }
}

// ─── Entry Point ──────────────────────────────────────────────────────────────

fn main() {
    env_logger::init();

    Application::new().run(|cx: &mut App| {
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

        let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);

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
                |_, cx| cx.new(|cx| FermiConsole::new(cx)),
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
