use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug, Clone)]
struct DocumentState {
    text: String,
    drivers: HashMap<String, String>, // driver name -> distribution type
}

#[derive(Debug)]
struct Backend {
    client: Client,
    documents: Arc<RwLock<HashMap<String, DocumentState>>>,
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: "fermi-lsp".to_string(),
                version: Some("0.1.0".to_string()),
            }),
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                    DiagnosticOptions {
                        identifier: Some("fermi".to_string()),
                        inter_file_dependencies: false,
                        workspace_diagnostics: false,
                        work_done_progress_options: WorkDoneProgressOptions::default(),
                    },
                )),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: Some(vec![".".to_string(), " ".to_string()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Fermi LSP initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "File opened!")
            .await;
        self.on_change(params.text_document.uri, params.text_document.text)
            .await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next() {
            self.on_change(params.text_document.uri, change.text).await;
        }
    }

    async fn did_save(&self, _: DidSaveTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "File saved!")
            .await;
    }

    async fn did_close(&self, _: DidCloseTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "File closed!")
            .await;
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let completions = self.get_completions(params);
        Ok(Some(CompletionResponse::Array(completions)))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let hover_info = self.get_hover_info(params).await;
        Ok(hover_info)
    }
}

impl Backend {
    async fn on_change(&self, uri: Url, text: String) {
        // Parse the FPL code and get diagnostics
        let diagnostics = self.parse_and_diagnose(&text);

        // Extract driver information for hover support
        let drivers = self.extract_drivers(&text);

        // Store document state
        let mut docs = self.documents.write().await;
        docs.insert(
            uri.to_string(),
            DocumentState {
                text: text.clone(),
                drivers,
            },
        );

        // Publish diagnostics
        self.client
            .publish_diagnostics(uri, diagnostics, None)
            .await;
    }

    fn parse_and_diagnose(&self, text: &str) -> Vec<Diagnostic> {
        use fermi::{Lexer, Parser, SemanticAnalyzer};

        let mut diagnostics = Vec::new();

        // Lexical analysis
        let lexer = Lexer::new(text);
        let tokens_result = lexer.tokenize();

        // Handle lexer errors
        let tokens = match tokens_result {
            Ok(tokens) => tokens,
            Err(errors) => {
                // Convert lexer errors to diagnostics
                for error in errors {
                    let (line, column, message) = match error {
                        fermi::LexerError::UnterminatedString { line, column } => {
                            (line, column, "Unterminated string".to_string())
                        }
                        fermi::LexerError::InvalidNumber {
                            lexeme,
                            line,
                            column,
                        } => (line, column, format!("Invalid number: {}", lexeme)),
                        fermi::LexerError::InvalidProbability {
                            lexeme,
                            line,
                            column,
                        } => (line, column, format!("Invalid probability: {}", lexeme)),
                        fermi::LexerError::InvalidDate {
                            lexeme,
                            line,
                            column,
                        } => (line, column, format!("Invalid date: {}", lexeme)),
                        fermi::LexerError::UnexpectedCharacter { char, line, column } => {
                            (line, column, format!("Unexpected character: {}", char))
                        }
                        fermi::LexerError::InvalidEscape { char, line, column } => {
                            (line, column, format!("Invalid escape sequence: \\{}", char))
                        }
                    };

                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: (line - 1) as u32,
                                character: column as u32,
                            },
                            end: Position {
                                line: (line - 1) as u32,
                                character: (column + 1) as u32,
                            },
                        },
                        severity: Some(DiagnosticSeverity::ERROR),
                        code: Some(NumberOrString::String("E001".to_string())),
                        source: Some("fermi".to_string()),
                        message,
                        ..Default::default()
                    });
                }
                return diagnostics;
            }
        };

        // Syntax analysis
        let mut parser = Parser::new(tokens);
        match parser.parse() {
            Ok(program) => {
                // Semantic analysis
                let analyzer = SemanticAnalyzer::new();
                let analysis = analyzer.analyze(&program);

                if analysis.errors.is_empty() {
                    // Success - no errors
                    self.client
                        .log_message(MessageType::INFO, "Parse successful - no errors");
                } else {
                    // Semantic errors
                    for error in analysis.errors {
                        let message = match error {
                            fermi::SemanticError::UndefinedSymbol { name, message } => {
                                format!("Undefined symbol '{}': {}", name, message)
                            }
                            fermi::SemanticError::TypeMismatch {
                                expected,
                                found,
                                message,
                            } => {
                                format!(
                                    "Type mismatch: expected {:?}, found {:?}. {}",
                                    expected, found, message
                                )
                            }
                            fermi::SemanticError::DuplicateDefinition { name, message } => {
                                format!("Duplicate definition of '{}': {}", name, message)
                            }
                            fermi::SemanticError::ValidationError { rule, message } => {
                                format!("Validation error ({}): {}", rule, message)
                            }
                        };

                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position {
                                    line: 0,
                                    character: 0,
                                },
                                end: Position {
                                    line: 0,
                                    character: 0,
                                },
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: Some(NumberOrString::String("E003".to_string())),
                            source: Some("fermi".to_string()),
                            message,
                            ..Default::default()
                        });
                    }
                }
            }
            Err(error) => {
                // Parse error
                diagnostics.push(Diagnostic {
                    range: Range {
                        start: Position {
                            line: 0,
                            character: 0,
                        },
                        end: Position {
                            line: 0,
                            character: 0,
                        },
                    },
                    severity: Some(DiagnosticSeverity::ERROR),
                    code: Some(NumberOrString::String("E002".to_string())),
                    source: Some("fermi".to_string()),
                    message: error.to_string(),
                    ..Default::default()
                });
            }
        }

        diagnostics
    }

    fn get_completions(&self, _params: CompletionParams) -> Vec<CompletionItem> {
        let mut completions = Vec::new();

        // FPL keywords
        completions.push(CompletionItem {
            label: "question".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Define the forecast question".to_string()),
            insert_text: Some("question \"${1:What is your question?}\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });

        completions.push(CompletionItem {
            label: "driver".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Define a forecast driver variable".to_string()),
            insert_text: Some(
                "driver ${1:name} continuous {\n\tdistribution: ${2:triangular(${3:min}, ${4:likely}, ${5:max})}\n}".to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });

        completions.push(CompletionItem {
            label: "model".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Define the forecast model calculation".to_string()),
            insert_text: Some("model: ${1:expression}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });

        completions.push(CompletionItem {
            label: "simulate".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Run Monte Carlo simulation".to_string()),
            insert_text: Some("simulate ${1:10000} iterations".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });

        completions.push(CompletionItem {
            label: "evidence".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Document evidence for the forecast".to_string()),
            insert_text: Some(
                "evidence ${1:name} {\n\tsource: \"${2:source}\"\n\tsummary: \"${3:summary}\"\n}"
                    .to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });

        completions.push(CompletionItem {
            label: "agent".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Create an automated research agent".to_string()),
            insert_text: Some("agent ${1:name} {\n\tquery: \"${2:search query}\"\n\tschedule: every ${3:1} ${4:day}\n}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });

        // Driver types
        completions.push(CompletionItem {
            label: "continuous".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Continuous distribution driver type".to_string()),
            ..Default::default()
        });

        completions.push(CompletionItem {
            label: "binary".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Binary (yes/no) driver type".to_string()),
            ..Default::default()
        });

        completions.push(CompletionItem {
            label: "discrete".to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("Discrete values driver type".to_string()),
            ..Default::default()
        });

        // Driver properties
        let driver_props = vec![
            (
                "distribution",
                "distribution: ${1:triangular(${2:min}, ${3:likely}, ${4:max})}",
                "Probability distribution",
            ),
            (
                "probability",
                "probability: ${1:0.5}",
                "Probability value (0-1 or percentage)",
            ),
            ("unit", "unit: \"${1:units}\"", "Unit of measurement"),
            (
                "rationale",
                "rationale: \"${1:reasoning}\"",
                "Explanation of estimate",
            ),
            (
                "impact_multiplier",
                "impact_multiplier: ${1:1.0}",
                "Impact on final result",
            ),
        ];

        for (name, snippet, desc) in driver_props {
            completions.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(desc.to_string()),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }

        // Evidence properties
        let evidence_props = vec![
            ("source", "source: \"${1:source}\"", "Evidence source"),
            (
                "summary",
                "summary: \"${1:summary}\"",
                "Summary of evidence",
            ),
            ("relevance", "relevance: ${1:0.8}", "Relevance score (0-1)"),
            ("date", "date: ${1:2025-01-01}", "Date of evidence"),
        ];

        for (name, snippet, desc) in evidence_props {
            completions.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::PROPERTY),
                detail: Some(desc.to_string()),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }

        // Agent properties
        completions.push(CompletionItem {
            label: "query".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Search query for agent".to_string()),
            insert_text: Some("query: \"${1:search query}\"".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });

        completions.push(CompletionItem {
            label: "schedule".to_string(),
            kind: Some(CompletionItemKind::PROPERTY),
            detail: Some("Agent execution schedule".to_string()),
            insert_text: Some("schedule: every ${1:1} ${2:day}".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..Default::default()
        });

        // Distribution functions
        let distributions = vec![
            (
                "triangular",
                "triangular(${1:p5}, ${2:p50}, ${3:p95})",
                "Three-point distribution (p5, p50, p95)",
            ),
            (
                "normal",
                "normal(${1:mean}, ${2:stddev})",
                "Normal distribution (mean, standard deviation)",
            ),
            (
                "lognormal",
                "lognormal(${1:median}, ${2:sigma})",
                "Lognormal distribution (median, sigma)",
            ),
            (
                "uniform",
                "uniform(${1:low}, ${2:high})",
                "Uniform distribution (low, high)",
            ),
            (
                "beta",
                "beta(${1:alpha}, ${2:beta})",
                "Beta distribution (alpha, beta)",
            ),
        ];

        for (name, snippet, desc) in distributions {
            completions.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(desc.to_string()),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }

        // Math functions
        let functions = vec![
            ("sqrt", "sqrt(${1:x})", "Square root"),
            ("log", "log(${1:x})", "Natural logarithm"),
            ("exp", "exp(${1:x})", "Exponential (e^x)"),
            ("pow", "pow(${1:base}, ${2:exponent})", "Power function"),
            ("abs", "abs(${1:x})", "Absolute value"),
            ("min", "min(${1:a}, ${2:b})", "Minimum of two values"),
            ("max", "max(${1:a}, ${2:b})", "Maximum of two values"),
        ];

        for (name, snippet, desc) in functions {
            completions.push(CompletionItem {
                label: name.to_string(),
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(desc.to_string()),
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                ..Default::default()
            });
        }

        completions
    }

    async fn get_hover_info(&self, params: HoverParams) -> Option<Hover> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let doc = docs.get(&uri.to_string())?;

        // Get word at position
        let word = self.get_word_at_position(&doc.text, position)?;

        // Check if it's a distribution function
        let hover_text = match word.as_str() {
            "triangular" => Some("**triangular(p5, p50, p95)**\n\nThree-point distribution using 5th, 50th, and 95th percentiles.\n\n**Example:** `triangular(1000, 2000, 5000)`\n\nUseful for: expert estimates with min/likely/max values"),
            "normal" => Some("**normal(mean, stddev)**\n\nNormal (Gaussian) distribution.\n\n**Example:** `normal(100, 15)`\n\nUseful for: naturally occurring variations, measurement errors"),
            "lognormal" => Some("**lognormal(median, sigma)**\n\nLognormal distribution - for positive-only values with right skew.\n\n**Example:** `lognormal(1000, 0.5)`\n\nUseful for: prices, incomes, sizes"),
            "uniform" => Some("**uniform(low, high)**\n\nUniform distribution - all values equally likely.\n\n**Example:** `uniform(0, 100)`\n\nUseful for: complete uncertainty within range"),
            "beta" => Some("**beta(alpha, beta)**\n\nBeta distribution - bounded between 0 and 1.\n\n**Example:** `beta(2, 5)`\n\nUseful for: probabilities, percentages"),
            _ => {
                // Check if it's a driver
                if let Some(dist) = doc.drivers.get(&word) {
                    return Some(Hover {
                        contents: HoverContents::Markup(MarkupContent {
                            kind: MarkupKind::Markdown,
                            value: format!("**Driver:** `{}`\n\n**Distribution:** `{}`", word, dist),
                        }),
                        range: None,
                    });
                }
                None
            }
        };

        hover_text.map(|text| Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: text.to_string(),
            }),
            range: None,
        })
    }

    fn get_word_at_position(&self, text: &str, position: Position) -> Option<String> {
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

    fn extract_drivers(&self, text: &str) -> HashMap<String, String> {
        let mut drivers = HashMap::new();

        // Simple regex-like parsing for driver statements
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("driver ") {
                // Parse: driver <name> <distribution>(...)
                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                if parts.len() >= 3 {
                    let name = parts[1].to_string();
                    let dist_start = trimmed.find(parts[2]).unwrap_or(0);
                    let dist = trimmed[dist_start..].to_string();
                    drivers.insert(name, dist);
                }
            }
        }

        drivers
    }
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend {
        client,
        documents: Arc::new(RwLock::new(HashMap::new())),
    });
    Server::new(stdin, stdout, socket).serve(service).await;
}
