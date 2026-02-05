# Test Document

This is a test markdown file with Mermaid diagrams.

## Flowchart Example

![diagram-2](charts/diagram-2.png)

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
    Start([Start]) --> Process[Process Data]
    Process --> Decision{Is Valid?}
    Decision -->|Yes| Success[Success]
    Decision -->|No| Error[Error]
    Success --> End([End])
    Error --> End

```

</details>

## Mind Map Example

![diagram-1](charts/diagram-1.png)

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
  root((Test Project))
    Features
      Authentication
      Dashboard
      Reports
    Tech Stack
      React
      Node.js
      PostgreSQL

```

</details>

## Chart Example

![diagram-0](charts/diagram-0.png)

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
  title "Sample Data"
  x-axis ["Q1", "Q2", "Q3", "Q4"]
  y-axis "Revenue" 0 --> 100
  bar [45, 60, 75, 90]

```

</details>

## Conclusion

All diagrams should be rendered!
