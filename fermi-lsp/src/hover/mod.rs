pub mod functions;
pub mod keywords;
pub mod properties;

use std::collections::HashMap;
use tower_lsp::lsp_types::*;

/// Get word at position in text
pub fn get_word_at_position(text: &str, position: Position) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let line = lines.get(position.line as usize)?;

    // Handle UTF-8 properly by converting to char indices
    let chars: Vec<char> = line.chars().collect();
    let col = position.character as usize;

    // Bounds check
    if col > chars.len() {
        return None;
    }

    // If cursor is on whitespace or special char, try to find word nearby
    let mut search_col = col;
    if search_col < chars.len() {
        let ch = chars[search_col];
        if !ch.is_alphanumeric() && ch != '_' {
            // Move back to find a word
            if search_col > 0 {
                search_col -= 1;
            } else {
                return None;
            }
        }
    } else if search_col > 0 {
        search_col = chars.len() - 1;
    }

    // Find word boundaries
    let mut start = search_col;
    while start > 0 {
        let ch = chars[start - 1];
        if !ch.is_alphanumeric() && ch != '_' {
            break;
        }
        start -= 1;
    }

    let mut end = search_col;
    while end < chars.len() {
        let ch = chars[end];
        if !ch.is_alphanumeric() && ch != '_' {
            break;
        }
        end += 1;
    }

    if start < end && end <= chars.len() {
        let word: String = chars[start..end].iter().collect();
        if !word.is_empty() {
            return Some(word);
        }
    }

    None
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
