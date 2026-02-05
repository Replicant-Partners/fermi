use tower_lsp::lsp_types::*;

/// Builder pattern for creating completion items with less boilerplate
pub struct CompletionBuilder {
    label: String,
    kind: CompletionItemKind,
    detail: Option<String>,
    documentation: Option<String>,
    insert_text: Option<String>,
    insert_text_format: Option<InsertTextFormat>,
    sort_text: Option<String>,
}

impl CompletionBuilder {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            kind: CompletionItemKind::TEXT,
            detail: None,
            documentation: None,
            insert_text: None,
            insert_text_format: None,
            sort_text: None,
        }
    }

    pub fn keyword(label: impl Into<String>) -> Self {
        Self::new(label).kind(CompletionItemKind::KEYWORD)
    }

    pub fn property(label: impl Into<String>) -> Self {
        Self::new(label).kind(CompletionItemKind::PROPERTY)
    }

    pub fn function(label: impl Into<String>) -> Self {
        Self::new(label).kind(CompletionItemKind::FUNCTION)
    }

    pub fn variable(label: impl Into<String>) -> Self {
        Self::new(label).kind(CompletionItemKind::VARIABLE)
    }

    pub fn operator(label: impl Into<String>) -> Self {
        Self::new(label).kind(CompletionItemKind::OPERATOR)
    }

    pub fn kind(mut self, kind: CompletionItemKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn docs(mut self, docs: impl Into<String>) -> Self {
        self.documentation = Some(docs.into());
        self
    }

    pub fn snippet(mut self, snippet: impl Into<String>) -> Self {
        self.insert_text = Some(snippet.into());
        self.insert_text_format = Some(InsertTextFormat::SNIPPET);
        self
    }

    pub fn sort(mut self, sort_key: impl Into<String>) -> Self {
        self.sort_text = Some(sort_key.into());
        self
    }

    pub fn build(self) -> CompletionItem {
        CompletionItem {
            label: self.label,
            kind: Some(self.kind),
            detail: self.detail,
            documentation: self.documentation.map(|d| Documentation::String(d)),
            insert_text: self.insert_text,
            insert_text_format: self.insert_text_format,
            sort_text: self.sort_text,
            ..Default::default()
        }
    }
}
