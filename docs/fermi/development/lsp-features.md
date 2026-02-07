# 🚀 Fermi LSP Autocomplete - Complete Feature Guide

## Quick Start

The Fermi LSP now has **comprehensive autocomplete** with over **80+ intelligent suggestions**! Here's what you can do:

## ✨ Context-Aware Completions

The autocomplete is smart - it knows where you are and suggests only relevant items.

### 📍 Top Level (Empty Line)
Press `Ctrl+Space` (or your trigger key) on an empty line:

**You'll see:**
```
question   - Define the forecast question
driver     - Define a driver variable
model      - Define the forecast model
simulate   - Run Monte Carlo simulation
evidence   - Document evidence
agent      - Create an automated research agent
```

**Try it:** Type `qu` + Tab
```fpl
question "What is your forecast question?"
```

---

### 🎯 Driver Definition
When you type `driver name ` (with space after name):

**You'll see:**
```
continuous - Continuous probability distribution
binary     - Binary outcome (yes/no)
discrete   - Discrete values with probabilities
```

**Try it:** Type `driver revenue con` + Tab
```fpl
driver revenue continuous {
    distribution: triangular(min, likely, max)
    unit: "units"
    rationale: "reasoning"
}
```

---

### 📦 Inside Driver Blocks
Place cursor inside `{}` of a driver and press `Ctrl+Space`:

**You'll see:**
```
distribution       - Probability distribution function
probability        - Probability value (for binary)
unit              - Unit of measurement
rationale         - Explanation of estimate
impact_multiplier - Impact on model (for binary)
min               - Minimum value
max               - Maximum value
values            - List of values (for discrete)
weights           - Probability weights (for discrete)
```

**Each property has a snippet!** Type `dis` + Tab:
```fpl
distribution: triangular(p5, p50, p95)
```

---

### 📊 Distribution Functions
Type distribution names anywhere you need a probability distribution:

**Available distributions:**
| Function | Best For | Example |
|----------|----------|---------|
| `triangular(p5, p50, p95)` | Expert estimates, min/likely/max | `triangular(100, 200, 500)` |
| `normal(mean, stddev)` | Natural variations, symmetric | `normal(100, 15)` |
| `lognormal(median, sigma)` | Prices, incomes (positive only) | `lognormal(50000, 0.5)` |
| `uniform(low, high)` | Complete uncertainty | `uniform(0, 100)` |
| `beta(alpha, beta)` | Probabilities, percentages [0-1] | `beta(2, 5)` |
| `exponential(lambda)` | Wait times, time to failure | `exponential(0.5)` |

**Try it:** Type `tri` + Tab inside a distribution property:
```fpl
distribution: triangular(p5, p50, p95)
```

**Hover over any distribution** to see detailed documentation!

---

### 🧮 Math Functions
Use these in your `model:` expression:

**Basic Math:**
- `sqrt(x)` - Square root
- `abs(x)` - Absolute value
- `pow(base, exp)` - Power function
- `round(x)` - Round to integer
- `floor(x)` - Round down
- `ceil(x)` - Round up

**Logarithms:**
- `log(x)` - Natural log (base e)
- `log10(x)` - Base-10 log
- `exp(x)` - Exponential (e^x)

**Min/Max:**
- `min(a, b)` - Minimum value
- `max(a, b)` - Maximum value

**Trigonometry:**
- `sin(x)`, `cos(x)`, `tan(x)` - Trig functions (radians)

**Try it in model:**
```fpl
model: sqrt(base_value) * log10(multiplier) + round(adjustment)
```

---

### 🔀 Control Flow
Create conditional logic in your model:

**if-then-else:**
```fpl
model: base * (if major_deal then 1.5 else 1.0)
```

**Try it:** Type `if` + Tab:
```fpl
if condition then true_value else false_value
```

---

### 🎲 Operators
All standard operators with autocomplete:

**Arithmetic:** `+`, `-`, `*`, `/`, `^`, `%`
**Comparison:** `==`, `!=`, `<`, `>`, `<=`, `>=`
**Logical:** `and`, `or`, `not`

**Example:**
```fpl
model: (revenue > 100000) and (costs < 50000)
```

---

### 📝 Evidence Blocks
Inside an evidence block, autocomplete suggests:

**Properties:**
```
source     - Citation or source
summary    - Brief summary
relevance  - Relevance score (0-1)
date       - Date (YYYY-MM-DD)
url        - URL link
strength   - Quality score (0-1)
```

**Try it:** Type `evi` + Tab at top level:
```fpl
evidence name {
    source: "source"
    summary: "summary"
    relevance: 0.8
    date: 2026-01-01
}
```

---

### 🤖 Agent Blocks
Inside an agent block:

**Properties:**
```
query      - Search query string
schedule   - Execution schedule (every N unit)
```

**Try it:** Type `age` + Tab:
```fpl
agent name {
    query: "search query"
    schedule: every 1 day
}
```

**Time units available:** `day`, `week`, `month`, `year`

---

### 🎯 Driver Name Completions
The autocomplete learns your driver names!

**Example:**
```fpl
driver base_price continuous {
    distribution: triangular(10, 20, 30)
}

driver volume continuous {
    distribution: normal(1000, 100)
}

driver growth_rate continuous {
    distribution: triangular(0.05, 0.15, 0.30)
}

# Now in your model, type "ba" and you'll see "base_price" suggested!
model: base_price * volume * (1 + growth_rate)
```

---

## 💡 Pro Tips

### 1. Trigger Autocomplete
- **Automatic**: Type `.` or space (configured as triggers)
- **Manual**: Press `Ctrl+Space` anytime

### 2. Navigate Snippets
When a snippet is inserted:
- Press `Tab` to jump to next placeholder
- Press `Shift+Tab` to go back
- Type to replace placeholder text

### 3. Hover for Help
Hover your mouse over any:
- Distribution function → See full documentation
- Math function → See formula and example
- Driver name → See its distribution type
- Keyword → See usage information

### 4. Smart Filtering
The autocomplete filters as you type:
- Type `tri` → Shows "triangular" first
- Type `norm` → Shows "normal" first
- Type `log` → Shows both "log" and "log10"

### 5. Context Matters
Don't see what you need? Check your context:
- Inside `{}` of driver? You'll only see driver properties
- At top level? You'll see top-level keywords
- In model line? You'll see math functions and driver names

---

## 🎨 Visual Guide

```fpl
# 1. Start here - press Ctrl+Space
|

# You see: question, driver, model, simulate, evidence, agent

question "What will Q1 revenue be?"

# 2. Define drivers - context-aware completion
driver base_sales continuous {
    # 3. Inside here - press Ctrl+Space
    |
    # You see: distribution, probability, unit, rationale, etc.
    
    distribution: triangular(10000, 15000, 25000)
    #             ^^^ Type "tri" + Tab for this snippet
    
    unit: "USD"
    rationale: "Based on historical Q4 data"
}

driver success_multiplier binary {
    probability: 0.65p
    impact_multiplier: 1.4
    rationale: "Major client renewal pending"
}

# 4. Use math functions and driver names
model: base_sales * (if success_multiplier then 1.4 else 1.0)
#      ^^^^^^^^^^     ^^^ Control flow          ^^^^^^^^^^^ Driver name
#      Driver name                              autocompletes!
#      autocompletes!

simulate 10000 iterations
```

---

## 📊 Completion Statistics

**Total Items: 80+**
- 6 Top-level keywords
- 3 Driver types
- 9 Driver properties
- 6 Evidence properties  
- 2 Agent properties
- 6 Distribution functions
- 14 Math functions
- 3 Control flow keywords
- 8 Time units
- 15 Operators
- Dynamic driver names (from your document)

---

## 🐛 Troubleshooting

### Autocomplete not working?
1. **Check LSP server is running**
   - Look for "Fermi LSP initialized!" in logs
   
2. **Rebuild the server**
   ```bash
   cd fermi-lsp && cargo build --release
   ```

3. **Restart Zed/Editor**
   - Close and reopen your editor
   
4. **Check file extension**
   - File must be `.fpl` extension

### Seeing wrong suggestions?
- Check your cursor position
- Context determines what's shown
- Try manual trigger: `Ctrl+Space`

### Missing driver name completions?
- Driver must be defined above model line
- Driver definition must be valid syntax
- Try saving the file first

---

## 🎯 Quick Reference Card

| Context | Trigger | Result |
|---------|---------|--------|
| Empty line | `qu` + Tab | `question "..."` |
| Empty line | `dr` + Tab | Full driver block |
| After `driver name ` | `co` + Tab | `continuous` |
| Inside driver `{}` | `dis` + Tab | `distribution: triangular(...)` |
| Inside model line | Driver name | Autocompletes variable |
| Inside model line | `sqrt` + Tab | `sqrt(x)` |
| Inside model line | `if` + Tab | `if ... then ... else ...` |
| Empty line | `evi` + Tab | Full evidence block |
| Empty line | `age` + Tab | Full agent block |

---

## 🎓 Learning Path

**Beginner:**
1. Create question with `qu` + Tab
2. Add a driver with `dr` + Tab
3. Fill in distribution with `tri` + Tab
4. Add model line referencing driver
5. Add simulate with `sim` + Tab

**Intermediate:**
6. Add binary driver for conditionals
7. Use if-then-else in model
8. Add multiple drivers and reference them
9. Use math functions like sqrt, log
10. Add evidence blocks

**Advanced:**
11. Use complex expressions with operators
12. Combine multiple distributions
13. Create discrete drivers with values/weights
14. Use all math functions: log10, floor, ceil, etc.
15. Set up agents with schedules

---

## 🚀 Next Steps

1. **Try the test file**: Open `autocomplete_test.fpl`
2. **Read full changes**: See `AUTOCOMPLETE_IMPROVEMENTS.md`
3. **Experiment**: Create your own forecast!
4. **Give feedback**: Found an issue or have ideas?

---

**Enjoy your enhanced Fermi forecasting experience! 🎉**
