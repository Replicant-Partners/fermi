# ADR-009: Right Sidebar Results Panel

**Date:** 2026-02-04  
**Status:** Accepted  
**Deciders:** ilabra, Claude  
**Related:** Module 2 Q2.5

## Context

After executing a forecast, users need to see detailed results: distribution charts, statistics, confidence intervals, and historical comparisons. The location of this results panel significantly impacts workflow efficiency.

**User's Requirement:** "lets start with right side bar"

**The Options:**
- A) Bottom panel (like terminal, familiar)
- B) Right sidebar (keeps editor visible)
- C) Floating window (movable, dismissible)
- D) Inline expansion (results appear in editor itself)

**Design Goals:**
1. **Code + Results Visible:** User should see forecast code and results simultaneously
2. **Maximize Vertical Space:** Forecasts are typically 10-30 lines tall, benefit from vertical editor space
3. **Natural Reading Flow:** Left-to-right: code → execution → results
4. **Zed-Native Feel:** Follow Zed's UI patterns and conventions

## Decision

We will implement a **right sidebar results panel** as the primary results display location.

**Panel Characteristics:**
- **Default Width:** 40% of window width (400-600px typical)
- **Resizable:** User can drag divider to adjust width
- **Collapsible:** Cmd+B to toggle visibility (standard Zed sidebar binding)
- **Persistent:** State preserved across sessions (width, visibility, scroll position)
- **Multi-Section:** Tabbed interface for different result types

## Consequences

### Positive

1. **Code Always Visible:** Editor remains in view while reviewing results (unlike bottom panel)
2. **Natural Flow:** Read code left, see results right (matches reading direction)
3. **Vertical Space Efficient:** Forecasts are vertically compact, horizontal layout is natural
4. **Multiple Results:** Can show charts, stats, history in vertical scroll without squashing editor
5. **Zed Convention:** Right sidebar is standard Zed pattern (e.g., project diagnostics, LSP panels)
6. **Wide Screens Friendly:** Modern monitors are 16:9 or wider, horizontal split is efficient

### Negative

1. **Narrow Screens:** On laptops <13", space feels cramped with sidebar open
2. **Wide Charts:** Some visualizations (tornado charts, calibration plots) benefit from width
3. **Context Switching:** Eyes travel further (left to right) compared to bottom panel (top to bottom)
4. **Split Focus:** Divides attention horizontally instead of vertically

### Neutral

1. **Customization:** Users can adjust width, collapse when not needed
2. **Multi-Monitor:** Users with multiple monitors might prefer floating window (future enhancement)

## Alternatives Considered

### A. Bottom Panel (Like Terminal)
**Pros:** Familiar pattern, full-width charts, vertical reading flow  
**Cons:** Squashes editor vertically, forecasts often need more vertical than horizontal space  
**Rejected Because:** Forecasts are vertically compact - losing vertical space hurts more than horizontal

**Why Right Sidebar Wins:** Consider typical forecast:
```fpl
forecast "Q4 Revenue" {          // Line 1
    driver revenue triangular()  // Line 2
    driver costs normal()        // Line 3
    estimate revenue - costs     // Line 4
}                                // Line 5 - only 5 lines tall!
```
Bottom panel would compress 5-line forecast + results into narrow strip. Right sidebar preserves full vertical space for code.

### C. Floating Window
**Pros:** Maximum flexibility, movable to second monitor, doesn't affect editor layout  
**Cons:** Window management complexity, inconsistent with Zed UI patterns, easy to lose/hide  
**Rejected Because:** Breaks Zed's integrated IDE experience, adds cognitive load (where did results go?)

**Potential Future:** Add "Pop Out" button in sidebar to create floating window for multi-monitor users

### D. Inline Expansion
**Pros:** Results appear exactly where forecast is, no eye travel  
**Cons:** Expands file vertically, scrolling nightmare, hard to compare multiple forecasts  
**Rejected Because:** Disrupts code structure, makes file navigation difficult

## Implementation Notes

### Phase 1: Basic Sidebar (Week 1-2)

**Panel Registration:**
```rust
// In fermi-lsp Zed extension
impl PanelProvider for FermiExtension {
    fn panels(&self) -> Vec<Panel> {
        vec![
            Panel {
                id: "fermi-results".into(),
                title: "Forecast Results".into(),
                position: PanelPosition::Right,
                default_width: Dimension::Percent(40),
                collapsible: true,
                keybinding: Some("cmd-b".into()),
            }
        ]
    }
    
    fn render_panel(&self, panel_id: &str) -> Element {
        match panel_id {
            "fermi-results" => self.render_results_panel(),
            _ => Element::Empty,
        }
    }
}
```

**Basic Layout:**
```
┌─────────────────────────────┬─────────────────────────┐
│ Editor (60%)                │ Results Panel (40%)     │
│                             │                         │
│ forecast "Q4 Revenue" {     │ ┌─────────────────────┐ │
│   driver revenue            │ │  Distribution       │ │
│     triangular(500,1200)    │ │  ▁▃▅▇▆▄▂           │ │
│                             │ │  p50: 1200          │ │
│   estimate revenue - costs  │ │  p10-p90: 800-1800  │ │
│ }                           │ └─────────────────────┘ │
│                             │                         │
│                             │ ┌─────────────────────┐ │
│                             │ │  Statistics         │ │
│                             │ │  Mean: 1200         │ │
│                             │ │  Std: 450           │ │
│                             │ └─────────────────────┘ │
└─────────────────────────────┴─────────────────────────┘
```

### Phase 2: Tabbed Interface (Week 3)

**Tab Structure:**
```
┌─────────────────────────────────────────┐
│ [Distribution] [Statistics] [History]   │  ← Tabs
├─────────────────────────────────────────┤
│                                         │
│   Content for active tab                │
│                                         │
└─────────────────────────────────────────┘
```

**Tab Implementation:**
```rust
enum ResultsTab {
    Distribution,
    Statistics,
    History,
    Agents,
}

struct ResultsPanel {
    active_tab: ResultsTab,
    forecast_results: Option<ForecastResult>,
}

impl ResultsPanel {
    fn render(&self) -> Element {
        div()
            .child(self.render_tab_bar())
            .child(self.render_tab_content())
    }
    
    fn render_tab_content(&self) -> Element {
        match self.active_tab {
            ResultsTab::Distribution => self.render_distribution_tab(),
            ResultsTab::Statistics => self.render_statistics_tab(),
            ResultsTab::History => self.render_history_tab(),
            ResultsTab::Agents => self.render_agents_tab(),
        }
    }
    
    fn render_distribution_tab(&self) -> Element {
        if let Some(results) = &self.forecast_results {
            div()
                .child(render_histogram(&results.samples))
                .child(render_percentiles(&results.percentiles))
                .child(render_confidence_interval(&results))
        } else {
            div().child("No results yet. Press Cmd+Enter to run forecast.")
        }
    }
}
```

### Phase 3: Advanced Features (Week 4-5)

**1. Panel State Persistence:**
```rust
struct PanelState {
    width: f32,
    visible: bool,
    active_tab: ResultsTab,
    scroll_position: f32,
}

impl PanelState {
    fn save(&self) {
        let state_json = serde_json::to_string(self).unwrap();
        save_to_workspace_state("fermi.results_panel", &state_json);
    }
    
    fn load() -> Self {
        if let Some(state_json) = load_from_workspace_state("fermi.results_panel") {
            serde_json::from_str(&state_json).unwrap_or_default()
        } else {
            Self::default()
        }
    }
}
```

**2. Multi-Forecast Support:**
```rust
// When file has multiple forecasts, show selector
fn render_forecast_selector(&self) -> Element {
    let forecasts = self.get_all_forecasts();
    
    select()
        .options(forecasts.iter().map(|f| (f.id, f.title)))
        .on_change(|selected_id| {
            self.load_results_for_forecast(selected_id);
        })
}
```

**3. Quick Actions:**
```rust
fn render_action_bar(&self) -> Element {
    div()
        .child(button("Run Again").on_click(|| self.rerun_forecast()))
        .child(button("Export CSV").on_click(|| self.export_results()))
        .child(button("Share").on_click(|| self.share_forecast()))
        .child(button("Pop Out").on_click(|| self.create_floating_window()))
}
```

### Panel Content Sections

**Distribution Tab:**
```
┌─────────────────────────────────────────┐
│ Distribution                            │
├─────────────────────────────────────────┤
│                                         │
│     ▁▃▅▇▆▄▂                            │
│    ╱───────╲                           │
│   ╱         ╲                          │
│  ╱           ╲                         │
│ ─────────────────                      │
│ 500    1200   2500                     │
│                                         │
│ ┌─────────────────────────────────┐   │
│ │ Percentiles                     │   │
│ │ p10:  800                       │   │
│ │ p25:  950                       │   │
│ │ p50:  1200  ← median            │   │
│ │ p75:  1450                      │   │
│ │ p90:  1800                      │   │
│ └─────────────────────────────────┘   │
└─────────────────────────────────────────┘
```

**Statistics Tab:**
```
┌─────────────────────────────────────────┐
│ Statistics                              │
├─────────────────────────────────────────┤
│                                         │
│ Central Tendency                        │
│   Mean:     1,205                       │
│   Median:   1,200                       │
│   Mode:     1,200 (triangular peak)     │
│                                         │
│ Dispersion                              │
│   Std Dev:  450                         │
│   Variance: 202,500                     │
│   Range:    500 - 2500                  │
│   IQR:      500 (p75 - p25)            │
│                                         │
│ Shape                                   │
│   Skewness: 0.05 (nearly symmetric)     │
│   Kurtosis: -0.8 (platykurtic)         │
│                                         │
│ Execution                               │
│   Iterations: 50,000                    │
│   Duration:   234 ms                    │
│   Location:   Local                     │
└─────────────────────────────────────────┘
```

**History Tab:**
```
┌─────────────────────────────────────────┐
│ History                                 │
├─────────────────────────────────────────┤
│                                         │
│ Forecast Evolution                      │
│                                         │
│ 1800 ┤                          ●       │
│      │                        ╱         │
│ 1500 ┤                      ●           │
│      │                    ╱             │
│ 1200 ┤        ●─────●───●              │
│      │      ╱                           │
│  900 ┤    ●                             │
│      │  ╱                               │
│  600 ┤●                                 │
│      └─────────────────────────────────│
│       v1   v2   v3   v4   v5   v6      │
│                                         │
│ Recent Executions                       │
│ ┌───────────────────────────────────┐  │
│ │ v6  1,800  2 min ago  +50%       │  │
│ │ v5  1,500  1 hr ago   +25%       │  │
│ │ v4  1,200  3 hrs ago  (baseline) │  │
│ └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
```

**Agents Tab:**
```
┌─────────────────────────────────────────┐
│ Agent Activity                          │
├─────────────────────────────────────────┤
│                                         │
│ ┌───────────────────────────────────┐  │
│ │ 🦊 Market Research                │  │
│ │ Status: ✓ Completed               │  │
│ │ Duration: 2.3s                    │  │
│ │ Cost: $0.04                       │  │
│ │                                   │  │
│ │ Query: "Current GPU market size"  │  │
│ │                                   │  │
│ │ [View Full Response]              │  │
│ └───────────────────────────────────┘  │
│                                         │
│ ┌───────────────────────────────────┐  │
│ │ 🐉 Data Analyst                   │  │
│ │ Status: ⏳ Running...             │  │
│ │ Duration: 15.2s                   │  │
│ │                                   │  │
│ │ [Cancel]                          │  │
│ └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
```

## Configuration

**User Settings:**
```json
{
  "fermi": {
    "results_panel": {
      "position": "right",  // "right" | "bottom" | "floating"
      "default_width": 0.4,  // 40% of window
      "default_visible": true,
      "default_tab": "distribution",
      "tabs": {
        "distribution": true,
        "statistics": true,
        "history": true,
        "agents": true
      },
      "auto_open": true,  // Open panel on first execution
      "auto_switch": true,  // Switch to panel when execution completes
      "preserve_scroll": true
    }
  }
}
```

## Responsive Behavior

**Width Breakpoints:**
```rust
fn compute_panel_layout(window_width: f32, panel_width_percent: f32) -> PanelLayout {
    let panel_width = window_width * panel_width_percent;
    
    match window_width {
        w if w < 1024 => {
            // Narrow screen: Suggest bottom panel or hide by default
            PanelLayout::Bottom { height: 300 }
        }
        w if w < 1440 => {
            // Standard laptop: Right panel, smaller default width
            PanelLayout::Right { width: (window_width * 0.3).min(400.0) }
        }
        _ => {
            // Wide screen: Right panel, comfortable width
            PanelLayout::Right { width: (window_width * 0.4).min(600.0) }
        }
    }
}
```

## References

- Module 2 Q2.5: Results Panel Location
- Zed panel system: https://zed.dev/docs/extensions/panels
- Vertical vs horizontal space for forecasting workflows
- User interface patterns for data analysis tools (RStudio, Jupyter)

## Testing Strategy

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_panel_opens_on_execution() {
        let editor = create_test_editor();
        let panel = ResultsPanel::new();
        
        assert!(!panel.is_visible());
        
        editor.execute_forecast();
        
        assert!(panel.is_visible());
        assert_eq!(panel.active_tab, ResultsTab::Distribution);
    }
    
    #[test]
    fn test_panel_state_persistence() {
        let mut panel = ResultsPanel::new();
        panel.set_width(500.0);
        panel.set_visible(false);
        panel.set_active_tab(ResultsTab::History);
        
        panel.save_state();
        
        let restored = ResultsPanel::load_state();
        assert_eq!(restored.width, 500.0);
        assert!(!restored.visible);
        assert_eq!(restored.active_tab, ResultsTab::History);
    }
    
    #[test]
    fn test_multi_forecast_switching() {
        let panel = ResultsPanel::new();
        
        let forecast1 = create_test_forecast("Revenue");
        let forecast2 = create_test_forecast("Costs");
        
        panel.load_results(&forecast1);
        assert_eq!(panel.current_forecast_id, forecast1.id);
        
        panel.load_results(&forecast2);
        assert_eq!(panel.current_forecast_id, forecast2.id);
    }
}
```

## Success Metrics

- **Panel Usage:** >85% of executions result in panel view (indicates usefulness)
- **Width Adjustment:** <20% of users change default width (indicates good default)
- **Tab Distribution:** Track which tabs are most viewed (optimize for popular tabs)
- **Performance:** Panel rendering <16ms (60 FPS for smooth interaction)
- **User Satisfaction:** >4/5 rating for results panel layout

## Future Enhancements

1. **Bottom Panel Option:** Add configuration to switch to bottom panel for user preference
2. **Pop Out Window:** Button to open results in floating window (for multi-monitor)
3. **Side-by-Side Comparison:** Compare results from two different forecast versions
4. **Custom Layouts:** Let users arrange tabs, add/remove sections
5. **Shareable Results:** Export panel as PNG/PDF for sharing forecast results
6. **Panel Sync:** When collaborating, sync panel state between team members
