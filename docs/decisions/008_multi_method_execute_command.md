# ADR-008: Multi-Method Execute Command UX (Keyboard + Palette + Auto-Execute)

**Date:** 2026-02-04  
**Status:** Accepted  
**Deciders:** ilabra, Claude  
**Related:** Module 2 Q2.4

## Context

Running a forecast is the most frequent action in the Fermi IDE. Users need quick, intuitive ways to execute forecasts without leaving their flow state.

**User's Requirement:** "All of the above" - support multiple execution methods

**The Options:**
- A) Keyboard shortcut (e.g., Cmd+R)
- B) Command palette ("Fermi: Run Forecast")
- C) Auto-execute on save (like formatters)
- D) All of the above

**Design Goals:**
1. **Fast Iteration:** Execute → review → adjust → execute cycle should be <2 seconds
2. **Discoverability:** New users should find execution obvious
3. **Flexibility:** Power users can choose their preferred method
4. **No Surprise Execution:** Don't run expensive forecasts accidentally

## Decision

We will implement **all three execution methods**, with smart defaults and configuration:

### 1. Keyboard Shortcut (Primary Method)
**Default Binding:** `Cmd+Enter` (macOS) / `Ctrl+Enter` (Linux/Windows)

**Behavior:**
- Executes the forecast where cursor is located
- If cursor not in forecast block, executes most recently edited forecast
- Shows execution status in status bar
- Non-blocking: user can continue editing during execution

**Why Cmd+Enter?**
- Common in data tools (Jupyter, RStudio, Observable)
- Different from Cmd+R (refresh) to avoid confusion
- Easy to press with one hand

### 2. Command Palette (Discovery Method)
**Command:** "Fermi: Run Forecast"

**Behavior:**
- Available via Cmd+Shift+P → type "run"
- Shows all forecasts in current file as options
- Useful when file has multiple forecasts
- Same as keyboard shortcut when only one forecast exists

**Additional Commands:**
- "Fermi: Run Forecast (Background)" - for long-running forecasts
- "Fermi: Run All Forecasts" - execute all in file sequentially
- "Fermi: Cancel Execution" - stop running forecast

### 3. Auto-Execute on Save (Productivity Method)
**Default:** Disabled (opt-in)

**Behavior:**
- When enabled, executes forecast automatically on file save
- Only for "quick" forecasts (<100K iterations by default)
- Skips execution if forecast unchanged since last run (uses content hash)
- Shows subtle status indicator during auto-execution

**Smart Throttling:**
- Debounce 500ms after last save (don't execute on every auto-save)
- Skip auto-execute if manual execution is running
- Queue only one pending auto-execution (don't stack multiple runs)

## Consequences

### Positive

1. **Fast for Power Users:** Cmd+Enter feels instant, muscle memory develops quickly
2. **Discoverable for New Users:** Command palette makes execution obvious
3. **Productivity Boost:** Auto-execute eliminates manual trigger for quick iterations
4. **Flexibility:** Users choose method that fits their workflow
5. **Standard Conventions:** Follows patterns from Jupyter, RStudio, Observable

### Negative

1. **Configuration Complexity:** Three methods mean more settings to understand
2. **Auto-Execute Risk:** Users might accidentally run expensive forecasts
3. **Keyboard Shortcut Conflicts:** Cmd+Enter might conflict with Zed or user bindings
4. **Status Indication:** Need clear feedback about which method triggered execution
5. **Testing Burden:** Must test all three execution paths

### Neutral

1. **Method Preference:** Different users will prefer different methods (need telemetry)
2. **Learning Curve:** New users might be confused about when to use each method

## Alternatives Considered

### Single Method Only (Keyboard or Palette)
**Pros:** Simpler implementation, clearer UX, less configuration  
**Cons:** Different users have different preferences, limits workflow optimization  
**Rejected Because:** User explicitly wanted "all of the above" - flexibility is valuable

### Click Button to Execute
**Pros:** Very discoverable, visual, familiar to Jupyter users  
**Cons:** Requires UI chrome, slower than keyboard, takes screen space  
**Rejected Because:** Breaks keyboard-focused IDE flow, but could add as 4th method later

### Auto-Execute on Keystroke (Like Copilot)
**Pros:** Instant feedback, no explicit trigger needed  
**Cons:** Too aggressive, wastes computation, distracting during typing  
**Rejected Because:** Forecasting execution is heavier than completion inference

## Implementation Notes

### Phase 1: Keyboard Shortcut (Week 1)

**Zed Extension Registration:**
```rust
// In fermi-lsp extension
impl CommandProvider for FermiExtension {
    fn commands(&self) -> Vec<Command> {
        vec![
            Command {
                name: "fermi::run_forecast".into(),
                description: "Run the current forecast".into(),
                keybinding: Some(Keybinding {
                    key: "cmd-enter".into(),
                    context: "Editor && fpl_file".into(),
                }),
            },
        ]
    }
    
    fn handle_command(&mut self, command: &str, ctx: &CommandContext) {
        match command {
            "fermi::run_forecast" => self.execute_forecast(ctx),
            _ => {}
        }
    }
}
```

**Execution Logic:**
```rust
fn execute_forecast(&self, ctx: &CommandContext) {
    // Find forecast at cursor
    let cursor_pos = ctx.cursor_position();
    let forecast = self.find_forecast_at_position(cursor_pos)
        .or_else(|| self.get_most_recently_edited_forecast());
    
    match forecast {
        Some(f) => {
            // Show status
            ctx.status_bar.set_message("⚡ Running forecast...", MessageType::Info);
            
            // Execute (async)
            let execution_id = self.start_execution(f);
            
            // Store for result retrieval
            self.pending_executions.insert(execution_id, f.id);
        }
        None => {
            ctx.status_bar.set_message("⚠️ No forecast found", MessageType::Warning);
        }
    }
}
```

### Phase 2: Command Palette (Week 2)

**Command Registration:**
```rust
vec![
    Command {
        name: "fermi::run_forecast".into(),
        description: "Run the current forecast".into(),
        keybinding: Some("cmd-enter".into()),
    },
    Command {
        name: "fermi::run_forecast_background".into(),
        description: "Run forecast in background (for long executions)".into(),
        keybinding: None,
    },
    Command {
        name: "fermi::run_all_forecasts".into(),
        description: "Run all forecasts in current file".into(),
        keybinding: Some("cmd-shift-enter".into()),
    },
    Command {
        name: "fermi::cancel_execution".into(),
        description: "Cancel running forecast".into(),
        keybinding: Some("cmd-.".into()),
    },
]
```

**Multi-Forecast Picker:**
```rust
fn execute_forecast_with_picker(&self, ctx: &CommandContext) {
    let forecasts = self.get_all_forecasts();
    
    if forecasts.is_empty() {
        ctx.show_error("No forecasts found in file");
        return;
    }
    
    if forecasts.len() == 1 {
        // Just run it
        self.execute_forecast(&forecasts[0]);
    } else {
        // Show picker
        ctx.show_picker(PickerOptions {
            items: forecasts.iter().map(|f| PickerItem {
                label: f.title.clone(),
                detail: format!("{} drivers, {} iterations", 
                    f.drivers.len(), 
                    f.iterations
                ),
            }).collect(),
            on_select: |index| {
                self.execute_forecast(&forecasts[index]);
            },
        });
    }
}
```

### Phase 3: Auto-Execute on Save (Week 3)

**Configuration:**
```json
{
  "fermi": {
    "execution": {
      "auto_execute": {
        "enabled": false,  // Opt-in
        "max_iterations": 100000,  // Only for quick forecasts
        "debounce_ms": 500,  // Wait 500ms after save
        "skip_if_unchanged": true,  // Hash-based deduplication
        "show_notification": false  // Silent or show toast
      }
    }
  }
}
```

**Auto-Execute Implementation:**
```rust
struct AutoExecutor {
    debounce_timer: Option<TimerId>,
    last_executed_hash: HashMap<ForecastId, u64>,
    config: AutoExecuteConfig,
}

impl AutoExecutor {
    fn on_file_saved(&mut self, forecast: &Forecast) {
        if !self.config.enabled {
            return;
        }
        
        // Check iteration limit
        if forecast.iterations > self.config.max_iterations {
            return; // Too expensive for auto-execute
        }
        
        // Check if forecast changed
        let content_hash = self.hash_forecast(forecast);
        if let Some(&last_hash) = self.last_executed_hash.get(&forecast.id) {
            if last_hash == content_hash {
                return; // Unchanged, skip execution
            }
        }
        
        // Debounce
        if let Some(timer) = self.debounce_timer {
            timer.cancel();
        }
        
        self.debounce_timer = Some(set_timeout(
            Duration::from_millis(self.config.debounce_ms),
            move || {
                self.execute_forecast(forecast);
                self.last_executed_hash.insert(forecast.id, content_hash);
                
                if self.config.show_notification {
                    show_toast("✓ Forecast auto-executed", ToastType::Success);
                }
            }
        ));
    }
    
    fn hash_forecast(&self, forecast: &Forecast) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        forecast.drivers.hash(&mut hasher);
        forecast.estimate.hash(&mut hasher);
        forecast.iterations.hash(&mut hasher);
        hasher.finish()
    }
}
```

### Status Indication

**Status Bar Messages:**
```
⚡ Running forecast "Q4 Revenue"... (Cmd+Enter)
⚙️ Auto-executing forecast... (on save)
✓ Forecast completed in 234ms
⚠️ Execution failed: Division by zero
🚫 Execution cancelled by user
```

**Visual Indicators:**
```rust
// Subtle inline indicator during execution
fn show_execution_status(forecast: &Forecast, status: ExecutionStatus) {
    let indicator = match status {
        ExecutionStatus::Running => "⚡",
        ExecutionStatus::Success => "✓",
        ExecutionStatus::Failed => "⚠️",
        ExecutionStatus::Cancelled => "🚫",
    };
    
    show_inlay_hint(
        forecast.title_position,
        format!("{} {}", indicator, status.message()),
        InlayHintKind::Ephemeral,
    );
}
```

## User Experience Examples

### Example 1: New User Discovery
```
User: Opens Fermi IDE for first time
System: Shows welcome panel with "Try Cmd+Enter to run forecast"
User: Types forecast, presses Cmd+Enter
System: Shows "⚡ Running forecast..." in status bar
System: Shows results in right panel after 200ms
User: Adjusts parameters, presses Cmd+Enter again
System: Updates results instantly
```

### Example 2: Power User Flow
```
User: Enables auto-execute in settings
User: Types forecast, saves file (Cmd+S)
System: Waits 500ms (debounce)
System: Auto-executes silently
System: Updates results panel
User: Tweaks driver, saves again
System: Auto-executes again (detects change via hash)
User: Sees updated results immediately
```

### Example 3: Command Palette User
```
User: Presses Cmd+Shift+P
User: Types "run"
System: Shows "Fermi: Run Forecast" command
User: Presses Enter
System: Shows picker with all forecasts in file
User: Selects "Q4 Revenue Forecast"
System: Executes selected forecast
```

## Configuration Examples

### Minimal Config (Default)
```json
{
  "fermi": {
    "execution": {
      "keybinding": "cmd-enter",
      "show_status": true
    }
  }
}
```

### Power User Config
```json
{
  "fermi": {
    "execution": {
      "keybinding": "cmd-r",  // Custom binding
      "auto_execute": {
        "enabled": true,
        "max_iterations": 50000,
        "debounce_ms": 300,
        "skip_if_unchanged": true,
        "show_notification": false
      },
      "show_status": true,
      "status_position": "inline"  // Show next to forecast title
    }
  }
}
```

## References

- Module 2 Q2.4: Execute Command UX
- Jupyter notebook: Shift+Enter to run cell
- RStudio: Cmd+Enter to run code
- Observable: Auto-execute on edit
- Zed command system: https://zed.dev/docs/extensions/commands

## Testing Strategy

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_keyboard_shortcut_execution() {
        let editor = create_test_editor();
        editor.open_file("test.fpl");
        
        // Simulate Cmd+Enter
        editor.send_command("fermi::run_forecast");
        
        // Should trigger execution
        assert!(editor.execution_manager.has_pending_execution());
    }
    
    #[test]
    fn test_auto_execute_skip_unchanged() {
        let auto_executor = AutoExecutor::new(AutoExecuteConfig::default());
        let forecast = create_test_forecast();
        
        // First save - should execute
        auto_executor.on_file_saved(&forecast);
        assert_eq!(auto_executor.execution_count(), 1);
        
        // Second save (unchanged) - should skip
        auto_executor.on_file_saved(&forecast);
        assert_eq!(auto_executor.execution_count(), 1); // Still 1
        
        // Modify forecast
        forecast.drivers[0].distribution = Distribution::Normal(100, 20);
        
        // Third save (changed) - should execute
        auto_executor.on_file_saved(&forecast);
        assert_eq!(auto_executor.execution_count(), 2);
    }
    
    #[test]
    fn test_auto_execute_respects_iteration_limit() {
        let config = AutoExecuteConfig {
            enabled: true,
            max_iterations: 100_000,
            ..Default::default()
        };
        let auto_executor = AutoExecutor::new(config);
        
        // Small forecast - should auto-execute
        let small = create_forecast_with_iterations(50_000);
        auto_executor.on_file_saved(&small);
        assert_eq!(auto_executor.execution_count(), 1);
        
        // Large forecast - should skip
        let large = create_forecast_with_iterations(1_000_000);
        auto_executor.on_file_saved(&large);
        assert_eq!(auto_executor.execution_count(), 1); // Still 1
    }
}
```

## Success Metrics

- **Keyboard Shortcut Usage:** >70% of executions via Cmd+Enter (indicates discoverability)
- **Auto-Execute Adoption:** >30% of users enable auto-execute after 1 week
- **Execution Latency:** <100ms from command to status indication
- **Wasted Executions:** <5% of auto-executions are duplicate (hash-based skip working)
- **User Satisfaction:** >4.5/5 rating for execution UX in user surveys

## Future Enhancements

1. **Context Menu:** Right-click forecast → "Run This Forecast"
2. **Run Configuration:** Save named execution configurations (iterations, agents, etc.)
3. **Partial Execution:** Run only specific drivers for testing
4. **Execution History:** Quick-access list of recent executions with re-run button
5. **Collaborative Execution:** "Run on Save" triggers for all team members viewing file
