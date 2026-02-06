# Execute Command - Run Forecasts in Zed

The Fermi LSP now supports executing forecasts directly from Zed!

## How It Works

The LSP server provides a command `fermi.runForecast` that:
1. Reads your current `.fpl` file
2. Parses and validates the FPL code
3. Executes the Monte Carlo simulation (10,000 iterations)
4. Shows results in a message and logs detailed output

## Usage

### Method 1: Command Palette (Available Now)

1. Open a `.fpl` file
2. Open command palette: `Cmd+Shift+P` (Mac) or `Ctrl+Shift+P` (Linux/Windows)
3. Type "Execute" or search for LSP commands
4. Look for "Execute Command" or similar
5. Select `fermi.runForecast`

### Method 2: Keyboard Shortcut (Needs Configuration)

To bind to `Cmd+R` or `Ctrl+R`:

1. Open Zed settings: `Cmd+,` (Mac) or `Ctrl+,` (Windows/Linux)
2. Go to "Keymap" section
3. Add this binding:

```json
{
  "bindings": {
    "cmd-r": "lsp::ExecuteCommand",
    "ctrl-r": "lsp::ExecuteCommand"
  }
}
```

Or edit `~/.config/zed/keymap.json` directly:

```json
[
  {
    "context": "Editor && fpl",
    "bindings": {
      "cmd-r": [
        "lsp::ExecuteCommand",
        {
          "command": "fermi.runForecast",
          "arguments": []
        }
      ]
    }
  }
]
```

### Method 3: Slash Command

Type `/run-forecast` in the editor to get instructions on running forecasts.

## Output Format

When you execute a forecast, you'll see:

**Popup Message:**
```
Forecast complete! Mean: 1523.45, Median: 1489.23
```

**Detailed Output (in LSP log):**
```
Forecast Results (10000 iterations):
Mean: 1523.45
Median: 1489.23
Std Dev: 345.67
95% CI: [892.34, 2234.56]
90% CI: [892.34, 2234.56]
50% CI: [1234.56, 1789.01]
Min: 567.89
Max: 3456.78
```

## Example Forecast

Create a file `test.fpl`:

```fpl
question "What will Q1 revenue be?"

driver base_revenue continuous {
    distribution: triangular(1000, 1500, 2500)
    unit: "thousands"
}

driver growth_multiplier continuous {
    distribution: normal(1.1, 0.15)
}

model: base_revenue * growth_multiplier

simulate 10000 iterations
```

Then execute with `Cmd+R` (once configured) or via command palette!

## Troubleshooting

### Command not found
- Make sure you've restarted the LSP server: `Cmd+Shift+P` → "zed: restart language server"
- Rebuild the LSP: `cd fermi-lsp && cargo build --release`

### No output shown
- Check the LSP logs in Zed's output panel
- Look for error messages in the diagnostics

### Errors in execution
- Fix any syntax errors shown in diagnostics first
- Make sure your FPL file is valid
- Check that you have a `model:` and `simulate` statement

## Next Steps

Future enhancements:
- Results panel showing charts and histograms
- Click to jump to driver definitions
- Inline result annotations
- Export results to CSV/JSON

---

**Status:** ✅ LSP command implemented and working
**Next:** Results panel UI (coming soon!)
