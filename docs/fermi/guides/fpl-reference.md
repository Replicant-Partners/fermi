# Fermi Quick Reference

## Commands at a Glance

### Execute Forecast
```bash
# Run a forecast
cargo run --release your_forecast.fpl

# Or with specific iterations
cargo run --release -- your_forecast.fpl --iterations 50000
```

### Generate Full Report
```bash
# Generate report with all visualizations
cargo run --release --example generate_report your_forecast.fpl

# Output: results/prototype/TIMESTAMP-question-slug.md
# Includes: histogram, mindmap, flowchart, sankey, tornado + sparklines
```

### Render Any Markdown
```bash
# Render any .md file with Mermaid diagrams
cargo run --release --example render_markdown your_file.md

# Custom output directory
cargo run --release --example render_markdown your_file.md ./output

# Works with ANY markdown file, not just FPL reports!
```

---

## Report Types

### FPL Forecast Report (Full)
- **Command:** `cargo run --release --example generate_report forecast.fpl`
- **Includes:**
  - 📊 Distribution histogram (themed)
  - 📈 Statistics table with sparklines
  - 🧠 Forecast structure (mindmap)
  - 🔄 Model flow (flowchart)
  - 🌊 Driver impact flow (sankey)
  - 🌪️ Sensitivity analysis (tornado)
  - 📋 Detailed driver specs
- **Requirements:** Valid FPL file, Mermaid CLI (mmdc)
- **Output:** `results/prototype/TIMESTAMP-question.md` + charts/

### Universal Markdown Renderer
- **Command:** `cargo run --release --example render_markdown any_file.md`
- **Includes:**
  - All Mermaid diagrams → PNG images
  - Ayu Mirage theme applied
  - Collapsible source code sections
- **Requirements:** Any `.md` file, Mermaid CLI (mmdc)
- **Output:** `rendered_output/filename-rendered.md` + charts/

---

## Zed Integration

### Slash Commands (in Zed)
```
/run-forecast          Execute current FPL file
/generate-report       Generate full report with charts
```

### LSP Commands (Future)
```
fermi.runForecast      Execute forecast via LSP
fermi.generateReport   Generate report via LSP
```

---

## File Structure

### FPL Forecast File
```fpl
// Comments supported!
question "Will X happen by Y?"

driver temperature continuous {
    display "Temperature"
    description "Daily temperature"
    distribution triangular { p5: 60, p50: 75, p95: 90 }
    unit "degrees"
    rationale "Based on historical data"
}

model temperature * 1.5
```

### Report Output
```
results/prototype/
├── 2026-02-05T06-18-09Z-will-x-happen.md
└── charts/
    ├── histogram.png
    ├── histogram.mmd
    ├── mindmap.png
    ├── mindmap.mmd
    ├── flowchart.png
    ├── flowchart.mmd
    ├── sankey.png
    ├── sankey.mmd
    ├── tornado.png
    ├── tornado.mmd
    └── puppeteer-config.json
```

---

## Theme Colors (Ayu Mirage)

```rust
Background:  #1F2430  (dark)
Foreground:  #CBCCC6  (light text)
Primary:     #5CCFE6  (cyan-blue)
Secondary:   #BAE67E  (muted green)
Accent:      #FFCC66  (gold)
Tertiary:    #FFAE57  (muted orange)
Muted:       #5C6773  (gray)
```

All charts automatically use these colors!

---

## Requirements

### For FPL Execution
- Rust/Cargo installed
- FPL file with valid syntax

### For Report Generation
```bash
# Install Mermaid CLI
npm install -g @mermaid-js/mermaid-cli

# Verify installation
mmdc --version
```

---

## Tips & Tricks

### Quick Test
```bash
# Run the example forecast
cargo run --release test_basic.fpl

# Generate example report
cargo run --release --example generate_report test_basic.fpl
```

### Batch Processing
```bash
# Process all FPL files
for file in forecasts/*.fpl; do
    cargo run --release --example generate_report "$file"
done

# Render all markdown files
for file in docs/*.md; do
    cargo run --release --example render_markdown "$file" rendered/
done
```

### Development Mode
```bash
# Faster iteration (debug build)
cargo run your_forecast.fpl

# Production (optimized)
cargo run --release your_forecast.fpl
```

### Check Syntax
```bash
# Run without execution (just parse)
cargo run --release your_forecast.fpl --check
```

---

## Troubleshooting

### "mmdc not found"
```bash
npm install -g @mermaid-js/mermaid-cli
```

### "Parse error"
- Check FPL syntax
- Comments must use `//`
- All statements need semicolons

### "Images not rendering"
- Verify `mmdc --version` works
- Check relative paths in markdown
- Ensure `charts/` directory exists

### "Build fails"
```bash
# Clean and rebuild
cargo clean
cargo build --release
```

---

## Session History

- **2026-02-05:** Theme integration (Ayu Mirage)
- **2026-02-05:** Phase 1 (LSP commands) + Phase 2 (Sankey, Tornado)
- **2026-02-05:** Universal markdown renderer

See `SESSION_*.md` files for detailed notes.

---

## Quick Links

- **Main docs:** `README.md`
- **Renderer guide:** `MARKDOWN_RENDERER.md`
- **Display panel design:** `DISPLAY_PANEL_DESIGN.md`
- **Session notes:** `SESSION_2026-02-05_*.md`
- **Examples:** `examples/` directory
- **Tests:** `tests/` directory

---

## Common Workflows

### 1. Create → Execute → Report
```bash
# 1. Write forecast
vim my_forecast.fpl

# 2. Test execution
cargo run --release my_forecast.fpl

# 3. Generate full report
cargo run --release --example generate_report my_forecast.fpl

# 4. View in Zed
zed results/prototype/latest-report.md
```

### 2. Document → Render → Share
```bash
# 1. Write docs with Mermaid
vim architecture.md

# 2. Render diagrams
cargo run --release --example render_markdown architecture.md

# 3. View result
zed rendered_output/architecture-rendered.md
```

### 3. Development → Test → Build
```bash
# 1. Make changes
vim src/executor.rs

# 2. Test
cargo test

# 3. Build release
cargo build --release

# 4. Run example
cargo run --release test_basic.fpl
```

---

**Pro Tip:** Bookmark this file for quick command reference!
