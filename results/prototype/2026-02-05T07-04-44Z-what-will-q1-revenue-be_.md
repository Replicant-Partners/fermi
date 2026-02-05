# Forecast Results: What will Q1 revenue be?

**Generated:** 2026-02-05T07:04:44.766223465+00:00  
**Mean:** 21032.38 | **Median:** 20651.12 | **90% CI:** [13205.98, 30283.62]  
**Distribution:** ▁▃▄▆█ Symmetric ⬌  

---

## 📊 Distribution

![Distribution Histogram](charts/histogram.png)

<details>
<summary>📝 View Mermaid Source</summary>

```mermaid
%%{
  init: {
    'theme': 'dark',
    'themeVariables': {
      'darkMode': 'true',
      'background': '#1F2430',
      'primaryColor': '#5CCFE6',
      'primaryTextColor': '#CBCCC6',
      'primaryBorderColor': '#5C6773',
      'lineColor': '#FFCC66',
      'secondaryColor': '#BAE67E',
      'tertiaryColor': '#FFAE57',
      'textColor': '#CBCCC6',
      'fontSize': '14px',
      'xyChart': {
        'backgroundColor': '#1F2430',
        'titleColor': '#CBCCC6',
        'xAxisLabelColor': '#CBCCC6',
        'xAxisTitleColor': '#CBCCC6',
        'xAxisTickColor': '#CBCCC6',
        'xAxisLineColor': '#CBCCC6',
        'yAxisLabelColor': '#CBCCC6',
        'yAxisTitleColor': '#CBCCC6',
        'yAxisTickColor': '#CBCCC6',
        'yAxisLineColor': '#CBCCC6',
        'plotColorPalette': '#5CCFE6, #BAE67E, #FFCC66, #FFAE57'
      }
    }
  }
}%%
xychart-beta
  title "Result Distribution (n=10000)"
  x-axis "Value Range" [10030.3, 34727.5]
  y-axis "Relative Frequency" 0 --> 100
  bar [5, 15, 30, 50, 70, 85, 100, 85, 70, 50, 30, 15, 5]
```

</details>


### Statistics

| Metric | Value | Visualization |
|--------|-------|---------------|
| Mean | 21032.38 | |
| Median | 20651.12 | |
| Std Dev | 5091.09 | |
| Distribution | | ▁▂▃▄▆▆▇██▇▆▆▅▄▄▃▃▂▁▁ |
| P5 | 13205.98 | ├ |
| P25 | 17239.99 | ├ |
| P50 (Median) | 20651.12 | █ |
| P75 | 24410.27 | ┤ |
| P95 | 30283.62 | ┤ |
| Range | [10030.32, 34727.47] |    ┤  ├──█──┤   ├     |


---

## 🧠 Forecast Structure

![Forecast Structure](charts/mindmap.png)

<details>
<summary>📝 View Mermaid Source</summary>

```mermaid
%%{
  init: {
    'theme': 'base',
    'themeVariables': {
      'primaryColor': '#5CCFE6',
      'primaryTextColor': '#CBCCC6',
      'primaryBorderColor': '#5C6773',
      'lineColor': '#FFCC66',
      'secondaryColor': '#BAE67E',
      'tertiaryColor': '#FFAE57',
      'background': '#1F2430',
      'mainBkg': '#1F2430',
      'secondBkg': '#1F2430',
      'tertiaryBkg': '#1F2430',
      'textColor': '#CBCCC6',
      'border1': '#5C6773',
      'border2': '#5C6773',
      'arrowheadColor': '#FFCC66',
      'fontFamily': 'ui-monospace, "Cascadia Code", "Source Code Pro", Menlo, Consolas, "DejaVu Sans Mono", monospace',
      'fontSize': '14px'
    }
  }
}%%mindmap
  root((Forecast))
    Question
      "What will Q1 revenue be?"
    Drivers
      Continuous
        base_sales
      Binary
        success_multiplier
    Model
      Expression
```

</details>


---

## 🔄 Model Flow

![Model Flow](charts/flowchart.png)

<details>
<summary>📝 View Mermaid Source</summary>

```mermaid
%%{
  init: {
    'theme': 'base',
    'themeVariables': {
      'primaryColor': '#5CCFE6',
      'primaryTextColor': '#CBCCC6',
      'primaryBorderColor': '#5C6773',
      'lineColor': '#FFCC66',
      'secondaryColor': '#BAE67E',
      'tertiaryColor': '#FFAE57',
      'background': '#1F2430',
      'mainBkg': '#1F2430',
      'secondBkg': '#1F2430',
      'tertiaryBkg': '#1F2430',
      'textColor': '#CBCCC6',
      'border1': '#5C6773',
      'border2': '#5C6773',
      'arrowheadColor': '#FFCC66',
      'fontFamily': 'ui-monospace, "Cascadia Code", "Source Code Pro", Menlo, Consolas, "DejaVu Sans Mono", monospace',
      'fontSize': '14px'
    }
  }
}%%flowchart TD
    Start([Model Computation])
    D0["base_sales"]
    Start --> D0
    D1["success_multiplier"]
    Start --> D1
    Expr{{Expression}}
    D0 --> Expr
    D1 --> Expr
    Cond{Condition?}
    Expr --> Cond
    Cond -->|True| Then[Then Branch]
    Cond -->|False| Else[Else Branch]
    Then --> Result
    Else --> Result
    Result([Final Result])
```

</details>


---

## 🌊 Driver Impact Flow

![Driver Impact Flow](charts/sankey.png)

<details>
<summary>📝 View Mermaid Source</summary>

```mermaid
%%{
  init: {
    'theme': 'base',
    'themeVariables': {
      'primaryColor': '#5CCFE6',
      'primaryTextColor': '#CBCCC6',
      'primaryBorderColor': '#5C6773',
      'lineColor': '#FFCC66',
      'secondaryColor': '#BAE67E',
      'tertiaryColor': '#FFAE57',
      'background': '#1F2430',
      'mainBkg': '#1F2430',
      'secondBkg': '#1F2430',
      'tertiaryBkg': '#1F2430',
      'textColor': '#CBCCC6',
      'border1': '#5C6773',
      'border2': '#5C6773',
      'arrowheadColor': '#FFCC66',
      'fontFamily': 'ui-monospace, "Cascadia Code", "Source Code Pro", Menlo, Consolas, "DejaVu Sans Mono", monospace',
      'fontSize': '14px'
    }
  }
}%%%%{init: {"theme": "base", "themeVariables": {"fontSize": "16px"}}}%%
graph LR
    D0["base_sales"]
    D0 -->|33%| Model
    D1["success_multiplier"]
    D1 -->|34%| Model
    Model["Model<br/>Computation"]
    Model -->|Result| Output["Final<br/>Distribution"]
    classDef driverClass fill:#5CCFE6,stroke:#5C6773,stroke-width:2px,color:#1F2430
    classDef modelClass fill:#BAE67E,stroke:#5C6773,stroke-width:3px,color:#1F2430
    classDef outputClass fill:#FFCC66,stroke:#5C6773,stroke-width:3px,color:#1F2430
    class D0 driverClass
    class D1 driverClass
    class Model modelClass
    class Output outputClass
```

</details>


---

## 🌪️ Sensitivity Analysis

![Sensitivity Analysis](charts/tornado.png)

<details>
<summary>📝 View Mermaid Source</summary>

```mermaid
%%{
  init: {
    'theme': 'dark',
    'themeVariables': {
      'darkMode': 'true',
      'background': '#1F2430',
      'primaryColor': '#5CCFE6',
      'primaryTextColor': '#CBCCC6',
      'primaryBorderColor': '#5C6773',
      'lineColor': '#FFCC66',
      'secondaryColor': '#BAE67E',
      'tertiaryColor': '#FFAE57',
      'textColor': '#CBCCC6',
      'fontSize': '14px',
      'xyChart': {
        'backgroundColor': '#1F2430',
        'titleColor': '#CBCCC6',
        'xAxisLabelColor': '#CBCCC6',
        'xAxisTitleColor': '#CBCCC6',
        'xAxisTickColor': '#CBCCC6',
        'xAxisLineColor': '#CBCCC6',
        'yAxisLabelColor': '#CBCCC6',
        'yAxisTitleColor': '#CBCCC6',
        'yAxisTickColor': '#CBCCC6',
        'yAxisLineColor': '#CBCCC6',
        'plotColorPalette': '#5CCFE6, #BAE67E, #FFCC66, #FFAE57'
      }
    }
  }
}%%
xychart-beta
  title "Driver Sensitivity Analysis"
  x-axis ["base_sales", "success_multiplier"]
  y-axis "Impact Magnitude" 0 --> 100
  bar [60, 41]
```

</details>


### Sobol Sensitivity Indices

| Driver | First-Order S_i | Total-Order S_Ti | 95% CI | Std Error |
|--------|-----------------|------------------|--------|----------|
| base_sales | 0.329 | 0.602 | [0.545, 0.659] | 0.029 |
| success_multiplier | 0.336 | 0.413 | [0.355, 0.471] | 0.030 |

**Interpretation:**
- **First-Order (S_i):** Direct effect of the driver alone
- **Total-Order (S_Ti):** Total effect including interactions with other drivers
- **95% CI:** 95% confidence interval from bootstrap resampling
- Higher values indicate greater influence on the forecast outcome

---

## 📋 Drivers

### base_sales (Continuous)

**Distribution:** Triangular { p5: Number(10000.0), p50: Number(15000.0), p95: Number(25000.0) }

**Unit:** USD

**Rationale:** Based on historical Q4 data

---

### success_multiplier (Binary)

**Probability:** 65.00% [███████░░░] 65%

**Impact Multiplier:** 1.40x ↗ Positive

**Rationale:** Major client renewal pending

---

