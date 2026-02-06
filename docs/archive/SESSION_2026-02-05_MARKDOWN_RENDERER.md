# Session Notes: Universal Markdown Renderer
**Date:** 2026-02-05  
**Focus:** Created standalone markdown renderer for any .md file with Mermaid diagrams

---

## Summary

Built a universal markdown renderer that can take **any markdown file** (not just FPL reports) and render all Mermaid diagrams to PNG images with your Ayu Mirage theme applied.

---

## What We Built

### Standalone Tool: `render_markdown`

**Purpose:** Render any markdown file with Mermaid diagrams to images

**Usage:**
```bash
# Basic usage
cargo run --release --example render_markdown input.md

# Custom output directory
cargo run --release --example render_markdown input.md ./output
```

**Features:**
✨ Works with any `.md` file (universal, not FPL-specific)  
🎨 Auto-applies Ayu Mirage theme to all diagrams  
🖼️ Generates PNG images via Mermaid CLI  
📝 Preserves original Mermaid source in collapsible sections  
🔍 Detects chart types and applies appropriate theming  
⚡ Fast rendering (~1-2 seconds per diagram)

---

## How It Works

1. **Input:** Any markdown file with Mermaid code blocks
2. **Extract:** Regex finds all ` ```mermaid ... ``` ` blocks
3. **Theme:** Applies Ayu Mirage theme config
4. **Render:** Uses `mmdc` (Mermaid CLI) to generate PNGs
5. **Replace:** Substitutes code blocks with image references
6. **Output:** New markdown file with rendered images

### Example Flow

**Input (`test.md`):**
```markdown
# My Document

```mermaid
flowchart TD
    A --> B
```
```

**Processing:**
- Extract: `flowchart TD\n    A --> B`
- Add theme: `%%{init: {...}}%%flowchart TD...`
- Generate: `charts/diagram-0.png`
- Replace with: `![diagram-0](charts/diagram-0.png)`

**Output (`rendered_output/test-rendered.md`):**
```markdown
# My Document

![diagram-0](charts/diagram-0.png)

<details>
<summary>📝 View Mermaid Source</summary>

```mermaid
%%{init: {...theme...}}%%
flowchart TD
    A --> B
```

</details>
```

---

## Technical Implementation

### File: `examples/render_markdown.rs` (~170 lines)

**Main function:**
- Parses CLI arguments (input file, optional output dir)
- Validates input file exists
- Creates output directory structure
- Calls `render_mermaid_diagrams()`
- Writes rendered markdown

**Key function: `render_mermaid_diagrams()`**
```rust
fn render_mermaid_diagrams(
    markdown: &str,
    charts_dir: &Path,
) -> Result<String, Box<dyn std::error::Error>>
```

**Algorithm:**
1. Check if `mmdc` is available
2. Find all Mermaid blocks with regex: `` r"```mermaid\n([\s\S]*?)```" ``
3. Store diagram info: `Vec<(start_pos, end_pos, code)>`
4. Process in **reverse order** (preserves string indices)
5. For each diagram:
   - Apply theme if not already themed
   - Generate image with `fermi::report::mermaid::generate_image()`
   - Create markdown with image + collapsible source
   - Replace original code block

**Helper function: `apply_theme_to_mermaid()`**
```rust
fn apply_theme_to_mermaid(mermaid_code: &str) -> String
```

Detects chart type and applies appropriate theme:
- `xychart` → `generate_xychart_theme()` (dark theme for charts)
- Others → `generate_mermaid_theme_config()` (base theme)

---

## Dependency Added

**File: `Cargo.toml`**
```toml
# For regex in examples
regex = "1.10"
```

Needed for extracting Mermaid code blocks from markdown.

---

## Test Results

### Test File Created: `test_render.md`

Contains 3 diagrams:
1. Flowchart (TD orientation)
2. Mind map
3. XY chart (bar chart)

### Test Execution

```bash
$ cargo run --release --example render_markdown test_render.md

📖 Rendering markdown file: test_render.md
📁 Output directory: rendered_output

🎨 Found 3 Mermaid diagram(s), rendering...
  ✓ Rendered: diagram-0.png
  ✓ Rendered: diagram-1.png
  ✓ Rendered: diagram-2.png

✅ Rendering complete!
📄 Output file: rendered_output/test_render-rendered.md
🖼️  Charts saved to: rendered_output

💡 Open the rendered markdown file in Zed or your favorite markdown viewer!
```

### Output Files

```
rendered_output/
├── test_render-rendered.md
└── charts/
    ├── diagram-0.png (16K) - XY chart
    ├── diagram-0.mmd
    ├── diagram-1.png (25K) - Mind map
    ├── diagram-1.mmd
    ├── diagram-2.png (20K) - Flowchart
    ├── diagram-2.mmd
    └── puppeteer-config.json
```

---

## Path Fix

**Issue:** Initially created `charts/charts/` subdirectory (nested)

**Root Cause:** `generate_image()` creates its own `charts/` subdirectory

**Fix:**
```rust
// Before
let charts_dir = output_dir.join("charts");
fs::create_dir_all(&charts_dir)?;

// After
let charts_dir = output_dir.clone(); // Let generate_image create charts/
```

---

## Use Cases

### 1. Documentation Rendering
```bash
cargo run --release --example render_markdown README.md
```

### 2. Any Markdown File
```bash
cargo run --release --example render_markdown notes/architecture.md
cargo run --release --example render_markdown blog/post.md
cargo run --release --example render_markdown docs/api.md
```

### 3. Batch Processing
```bash
for file in docs/*.md; do
    cargo run --release --example render_markdown "$file" rendered/
done
```

### 4. Re-render FPL Reports
```bash
# Generate report (includes forecast execution)
cargo run --release --example generate_report forecast.fpl

# Later: re-render just the markdown (no execution)
cargo run --release --example render_markdown results/prototype/my-report.md
```

---

## Theme Application

### XY Charts (Histograms, Bar Charts)
```rust
%%{
  init: {
    'theme': 'dark',
    'themeVariables': {
      'darkMode': 'true',
      'background': '#1F2430',
      'xyChart': {
        'backgroundColor': '#1F2430',
        'titleColor': '#CBCCC6',
        'xAxisLabelColor': '#CBCCC6',
        // ... readable axis colors
        'plotColorPalette': '#5CCFE6, #BAE67E, #FFCC66, #FFAE57'
      }
    }
  }
}%%
```

### Other Diagrams (Flowcharts, Mind Maps, etc.)
```rust
%%{
  init: {
    'theme': 'base',
    'themeVariables': {
      'primaryColor': '#5CCFE6',
      'secondaryColor': '#BAE67E',
      'tertiaryColor': '#FFAE57',
      'background': '#1F2430',
      'textColor': '#CBCCC6',
      // ... other Ayu Mirage colors
    }
  }
}%%
```

---

## Error Handling

### No Mermaid CLI
```
⚠️  Warning: Mermaid CLI (mmdc) not found.
   Install with: npm install -g @mermaid-js/mermaid-cli
   Mermaid diagrams will remain as code blocks.
```
Returns original markdown unchanged.

### No Diagrams Found
```
ℹ️  No Mermaid diagrams found in the markdown file.
```
Returns original markdown unchanged.

### Rendering Failure
```
✗ Failed to render diagram 2: Parse error on line 5
```
Leaves original code block in place, continues with other diagrams.

---

## Answer to User's Question

**Question:** "how can i pass any MD file to our little renderer?"

**Answer:**
```bash
# Any markdown file with Mermaid diagrams:
cargo run --release --example render_markdown YOUR_FILE.md

# Examples:
cargo run --release --example render_markdown notes.md
cargo run --release --example render_markdown ~/Documents/architecture.md
cargo run --release --example render_markdown ../other-project/README.md

# With custom output:
cargo run --release --example render_markdown input.md ./my_output_dir
```

The renderer is **completely universal** - it doesn't care about FPL at all. It just:
1. Finds Mermaid code blocks
2. Renders them to images
3. Replaces them in the markdown

Works with any `.md` file from anywhere!

---

## Files Created

1. `examples/render_markdown.rs` (~170 lines) - Standalone renderer
2. `MARKDOWN_RENDERER.md` (comprehensive usage guide)
3. `SESSION_2026-02-05_MARKDOWN_RENDERER.md` (this file)
4. `test_render.md` (test file with 3 diagrams)

---

## Files Modified

1. `Cargo.toml` (+3 lines) - Added `regex = "1.10"` dependency

---

## Integration with Existing System

The renderer **reuses** existing infrastructure:
- `fermi::report::mermaid::generate_image()` - PNG generation
- `fermi::report::mermaid::is_mmdc_available()` - CLI detection
- `fermi::report::theme::AYU_MIRAGE` - Color palette
- `fermi::report::theme::generate_xychart_theme()` - Chart theming
- `fermi::report::theme::generate_mermaid_theme_config()` - General theming

No duplication - just a new entry point for the same rendering pipeline!

---

## Next Steps (Optional)

Future enhancements could include:
- Watch mode (auto-regenerate on file changes)
- Batch mode (process multiple files at once)
- PDF export
- SVG output option
- Parallel rendering
- Progress bars for multiple files
- Custom theme selection via CLI

But for now, it's a simple, working tool that does exactly what you need!

---

## Summary

✅ **Built:** Universal markdown renderer  
✅ **Works with:** Any `.md` file with Mermaid diagrams  
✅ **Applies:** Ayu Mirage theme automatically  
✅ **Preserves:** Original Mermaid source code  
✅ **Usage:** `cargo run --release --example render_markdown input.md`  
✅ **Documentation:** Complete guide in MARKDOWN_RENDERER.md  

Your "little renderer" is now ready to handle any markdown file you throw at it! 🎉
