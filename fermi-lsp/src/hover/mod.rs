pub mod functions;
pub mod keywords;
pub mod properties;

use std::collections::HashMap;
use tower_lsp::lsp_types::*;

/// Get word at position in text
pub fn get_word_at_position(text: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;
    let col = position.character as usize;

    // Find word boundaries
    let start = line[..col]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1)
        .unwrap_or(0);
    let end = line[col..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| col + i)
        .unwrap_or(line.len());

    if start < end {
        Some(line[start..end].to_string())
    } else {
        None
    }
}

/// Get hover information for a word
pub fn get_hover_info(word: &str, drivers: &HashMap<String, String>) -> Option<Hover> {
    // Try each category for hover text
    let hover_text = keywords::get_keyword_hover(word)
        .or_else(|| functions::get_function_hover(word))
        .or_else(|| properties::get_property_hover(word));

    if let Some(text) = hover_text {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text,
            }),
            range: None,
        });
    }

    // Check if it's a driver
    if let Some(dist) = drivers.get(word) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!(
                    "**Driver:** `{}`\n\n**Type:** `{}`\n\nHover over the distribution function to see details.",
                    word, dist
                ),
            }),
            range: None,
        });
    }

    None
}
