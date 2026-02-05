/// Theme configuration for Mermaid charts
/// Based on Ayu Mirage color palette

pub struct ChartTheme {
    pub background: &'static str,
    pub foreground: &'static str,
    pub accent: &'static str,
    pub primary: &'static str,
    pub secondary: &'static str,
    pub tertiary: &'static str,
    pub muted: &'static str,
}

/// Ayu Mirage theme colors
pub const AYU_MIRAGE: ChartTheme = ChartTheme {
    background: "#1F2430", // Dark background
    foreground: "#CBCCC6", // Light text
    accent: "#FFCC66",     // Gold/amber accent
    primary: "#5CCFE6",    // Cyan-blue (main chart color)
    secondary: "#BAE67E",  // Muted green (secondary data)
    tertiary: "#FFAE57",   // Muted orange (tertiary data)
    muted: "#5C6773",      // Muted gray for borders/grid
};

/// Generate Mermaid theme configuration
pub fn generate_mermaid_theme_config(theme: &ChartTheme) -> String {
    format!(
        r#"%%{{
  init: {{
    'theme': 'base',
    'themeVariables': {{
      'primaryColor': '{}',
      'primaryTextColor': '{}',
      'primaryBorderColor': '{}',
      'lineColor': '{}',
      'secondaryColor': '{}',
      'tertiaryColor': '{}',
      'background': '{}',
      'mainBkg': '{}',
      'secondBkg': '{}',
      'tertiaryBkg': '{}',
      'textColor': '{}',
      'border1': '{}',
      'border2': '{}',
      'arrowheadColor': '{}',
      'fontFamily': 'ui-monospace, "Cascadia Code", "Source Code Pro", Menlo, Consolas, "DejaVu Sans Mono", monospace',
      'fontSize': '14px'
    }}
  }}
}}%%"#,
        theme.primary,    // primaryColor
        theme.foreground, // primaryTextColor
        theme.muted,      // primaryBorderColor
        theme.accent,     // lineColor
        theme.secondary,  // secondaryColor
        theme.tertiary,   // tertiaryColor
        theme.background, // background
        theme.background, // mainBkg
        theme.background, // secondBkg
        theme.background, // tertiaryBkg
        theme.foreground, // textColor
        theme.muted,      // border1
        theme.muted,      // border2
        theme.accent,     // arrowheadColor
    )
}

/// Generate XY Chart specific theme
pub fn generate_xychart_theme(theme: &ChartTheme) -> String {
    format!(
        r#"%%{{
  init: {{
    'theme': 'dark',
    'themeVariables': {{
      'darkMode': 'true',
      'background': '{}',
      'primaryColor': '{}',
      'primaryTextColor': '{}',
      'primaryBorderColor': '{}',
      'lineColor': '{}',
      'secondaryColor': '{}',
      'tertiaryColor': '{}',
      'textColor': '{}',
      'fontSize': '14px',
      'xyChart': {{
        'backgroundColor': '{}',
        'titleColor': '{}',
        'xAxisLabelColor': '{}',
        'xAxisTitleColor': '{}',
        'xAxisTickColor': '{}',
        'xAxisLineColor': '{}',
        'yAxisLabelColor': '{}',
        'yAxisTitleColor': '{}',
        'yAxisTickColor': '{}',
        'yAxisLineColor': '{}',
        'plotColorPalette': '{}, {}, {}, {}'
      }}
    }}
  }}
}}%%
"#,
        theme.background, // background
        theme.primary,    // primaryColor
        theme.foreground, // primaryTextColor
        theme.muted,      // primaryBorderColor
        theme.accent,     // lineColor
        theme.secondary,  // secondaryColor
        theme.tertiary,   // tertiaryColor
        theme.foreground, // textColor
        theme.background, // xyChart backgroundColor
        theme.foreground, // titleColor
        theme.foreground, // xAxisLabelColor (fixed spelling)
        theme.foreground, // xAxisTitleColor
        theme.foreground, // xAxisTickColor (changed to foreground for visibility)
        theme.foreground, // xAxisLineColor (changed to foreground for visibility)
        theme.foreground, // yAxisLabelColor (fixed spelling)
        theme.foreground, // yAxisTitleColor
        theme.foreground, // yAxisTickColor (changed to foreground for visibility)
        theme.foreground, // yAxisLineColor (changed to foreground for visibility)
        theme.primary,    // Plot colors (multi-color palette)
        theme.secondary,
        theme.accent,
        theme.tertiary,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_generation() {
        let config = generate_mermaid_theme_config(&AYU_MIRAGE);
        assert!(config.contains("#1F2430"));
        assert!(config.contains("#FFCC66"));
    }
}
