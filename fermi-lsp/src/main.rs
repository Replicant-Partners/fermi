mod completions;
mod hover;

use serde::{Deserialize, Serialize};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ForecastResult {
    success: bool,
    error: Option<String>,
    output: Option<String>,
}

// CompletionContext is now in the completions module

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
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["fermi.runForecast".to_string()],
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                code_lens_provider: Some(CodeLensOptions {
                    resolve_provider: Some(false),
                }),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
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

    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri;

        // Add a "Run Forecast" code lens at the top of the file
        let code_lens = CodeLens {
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
            command: Some(Command {
                title: "▶ Run Forecast".to_string(),
                command: "fermi.runForecast".to_string(),
                arguments: Some(vec![serde_json::Value::String(uri.to_string())]),
            }),
            data: None,
        };

        Ok(Some(vec![code_lens]))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        let mut actions = Vec::new();

        // Check if there are diagnostics in this range that we can fix
        for diagnostic in &params.context.diagnostics {
            // Add evidence block action
            if diagnostic.message.contains("missing evidence")
                || diagnostic.message.contains("Consider adding evidence")
            {
                let action = CodeAction {
                    title: "Add evidence block".to_string(),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diagnostic.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some([(
                            uri.clone(),
                            vec![TextEdit {
                                range: Range {
                                    start: Position {
                                        line: range.end.line + 1,
                                        character: 0,
                                    },
                                    end: Position {
                                        line: range.end.line + 1,
                                        character: 0,
                                    },
                                },
                                new_text: "\nevidence source_name {\n    source: \"Source citation\"\n    summary: \"Brief summary of the evidence\"\n    relevance: 0.8\n    date: 2026-01-01\n}\n".to_string(),
                            }],
                        )]
                        .into_iter()
                        .collect()),
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                actions.push(CodeActionOrCommand::CodeAction(action));
            }
        }

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        match params.command.as_str() {
            "fermi.runForecast" => {
                // Get the document URI from arguments
                if let Some(args) = params.arguments.first() {
                    if let Some(uri_str) = args.as_str() {
                        let result = self.run_forecast(uri_str).await;
                        return Ok(Some(serde_json::to_value(result).unwrap()));
                    }
                }

                self.client
                    .log_message(
                        MessageType::ERROR,
                        "No document URI provided for forecast execution",
                    )
                    .await;

                Ok(None)
            }
            "fermi.generateReport" => {
                // Get the document URI from arguments
                if let Some(args) = params.arguments.first() {
                    if let Some(uri_str) = args.as_str() {
                        let result = self.generate_report(uri_str).await;
                        return Ok(Some(serde_json::to_value(result).unwrap()));
                    }
                }

                self.client
                    .log_message(
                        MessageType::ERROR,
                        "No document URI provided for report generation",
                    )
                    .await;

                Ok(None)
            }
            _ => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Unknown command: {}", params.command),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}

impl Backend {
    async fn run_forecast(&self, uri_str: &str) -> ForecastResult {
        // Get the document
        let docs = self.documents.read().await;
        let doc = match docs.get(uri_str) {
            Some(d) => d,
            None => {
                return ForecastResult {
                    success: false,
                    error: Some("Document not found".to_string()),
                    output: None,
                };
            }
        };

        // Log execution start
        self.client
            .log_message(MessageType::INFO, "Executing Fermi forecast...")
            .await;

        // Execute the FPL code
        use fermi::{Executor, Lexer, Parser, SemanticAnalyzer};

        // Lexical analysis
        let lexer = Lexer::new(&doc.text);
        let tokens = match lexer.tokenize() {
            Ok(t) => t,
            Err(errors) => {
                let error_msg = format!("Lexer errors: {:?}", errors);
                self.client
                    .log_message(MessageType::ERROR, &error_msg)
                    .await;
                return ForecastResult {
                    success: false,
                    error: Some(error_msg),
                    output: None,
                };
            }
        };

        // Syntax analysis
        let parser = Parser::new(tokens);
        let program = match parser.parse() {
            Ok(p) => p,
            Err(error) => {
                let error_msg = format!("Parse error: {}", error);
                self.client
                    .log_message(MessageType::ERROR, &error_msg)
                    .await;
                return ForecastResult {
                    success: false,
                    error: Some(error_msg),
                    output: None,
                };
            }
        };

        // Semantic analysis
        let analyzer = SemanticAnalyzer::new();
        let analysis = analyzer.analyze(&program);

        if !analysis.errors.is_empty() {
            let error_msg = format!("Semantic errors: {:?}", analysis.errors);
            self.client
                .log_message(MessageType::ERROR, &error_msg)
                .await;
            return ForecastResult {
                success: false,
                error: Some(error_msg),
                output: None,
            };
        }

        // Execute - default to 10,000 iterations
        let mut executor = Executor::new(10_000);
        match executor.execute(&program) {
            Ok(result) => {
                // Format a nice output summary
                let summary = format!(
                    "Forecast Results ({} iterations):\n\
                     Mean: {:.2}\n\
                     Median: {:.2}\n\
                     Std Dev: {:.2}\n\
                     95% CI: [{:.2}, {:.2}]\n\
                     90% CI: [{:.2}, {:.2}]\n\
                     50% CI: [{:.2}, {:.2}]\n\
                     Min: {:.2}\n\
                     Max: {:.2}",
                    result.iterations,
                    result.mean,
                    result.median,
                    result.std_dev,
                    result.p5,
                    result.p95,
                    result.p5,
                    result.p95,
                    result.p25,
                    result.p75,
                    result.min,
                    result.max
                );

                self.client
                    .log_message(MessageType::INFO, "Forecast executed successfully!")
                    .await;
                self.client
                    .show_message(
                        MessageType::INFO,
                        &format!(
                            "Forecast complete! Mean: {:.2}, Median: {:.2}",
                            result.mean, result.median
                        ),
                    )
                    .await;

                ForecastResult {
                    success: true,
                    error: None,
                    output: Some(summary),
                }
            }
            Err(error) => {
                let error_msg = format!("Execution error: {}", error);
                self.client
                    .log_message(MessageType::ERROR, &error_msg)
                    .await;
                self.client
                    .show_message(MessageType::ERROR, &error_msg)
                    .await;

                ForecastResult {
                    success: false,
                    error: Some(error_msg),
                    output: None,
                }
            }
        }
    }

    async fn generate_report(&self, uri_str: &str) -> ForecastResult {
        // Get the document
        let docs = self.documents.read().await;
        let doc = match docs.get(uri_str) {
            Some(d) => d,
            None => {
                return ForecastResult {
                    success: false,
                    error: Some("Document not found".to_string()),
                    output: None,
                };
            }
        };

        // Log start
        self.client
            .log_message(MessageType::INFO, "Generating report...")
            .await;

        // Execute the FPL code (same as run_forecast)
        use fermi::{Executor, Lexer, Parser, SemanticAnalyzer};

        // Lexical analysis
        let lexer = Lexer::new(&doc.text);
        let tokens = match lexer.tokenize() {
            Ok(t) => t,
            Err(errors) => {
                let error_msg = format!("Lexer errors: {:?}", errors);
                self.client
                    .log_message(MessageType::ERROR, &error_msg)
                    .await;
                return ForecastResult {
                    success: false,
                    error: Some(error_msg),
                    output: None,
                };
            }
        };

        // Syntax analysis
        let parser = Parser::new(tokens);
        let program = match parser.parse() {
            Ok(p) => p,
            Err(error) => {
                let error_msg = format!("Parse error: {}", error);
                self.client
                    .log_message(MessageType::ERROR, &error_msg)
                    .await;
                return ForecastResult {
                    success: false,
                    error: Some(error_msg),
                    output: None,
                };
            }
        };

        // Semantic analysis
        let analyzer = SemanticAnalyzer::new();
        let analysis = analyzer.analyze(&program);

        if !analysis.errors.is_empty() {
            let error_msg = format!("Semantic errors: {:?}", analysis.errors);
            self.client
                .log_message(MessageType::ERROR, &error_msg)
                .await;
            return ForecastResult {
                success: false,
                error: Some(error_msg),
                output: None,
            };
        }

        // Execute - default to 10,000 iterations
        let mut executor = Executor::new(10_000);
        let result = match executor.execute(&program) {
            Ok(r) => r,
            Err(error) => {
                let error_msg = format!("Execution error: {}", error);
                self.client
                    .log_message(MessageType::ERROR, &error_msg)
                    .await;
                self.client
                    .show_message(MessageType::ERROR, &error_msg)
                    .await;
                return ForecastResult {
                    success: false,
                    error: Some(error_msg),
                    output: None,
                };
            }
        };

        // Generate report
        use fermi::generate_report;
        use std::path::PathBuf;

        let output_dir = PathBuf::from("results/prototype");
        if let Err(e) = std::fs::create_dir_all(&output_dir) {
            let error_msg = format!("Failed to create output directory: {}", e);
            self.client
                .log_message(MessageType::ERROR, &error_msg)
                .await;
            return ForecastResult {
                success: false,
                error: Some(error_msg),
                output: None,
            };
        }

        // Generate report and convert result to strings immediately for Send safety
        let report_result = match generate_report(&program, &result, &output_dir) {
            Ok(path) => Ok(path),
            Err(e) => Err(e.to_string()),
        };

        match report_result {
            Ok(report_path) => {
                self.client
                    .log_message(MessageType::INFO, "Report generated successfully!")
                    .await;
                self.client
                    .show_message(
                        MessageType::INFO,
                        &format!("Report generated: {}", report_path),
                    )
                    .await;

                ForecastResult {
                    success: true,
                    error: None,
                    output: Some(format!("Report saved to: {}", report_path)),
                }
            }
            Err(error_string) => {
                let error_msg = format!("Report generation failed: {}", error_string);
                self.client
                    .log_message(MessageType::ERROR, &error_msg)
                    .await;
                self.client
                    .show_message(MessageType::ERROR, &error_msg)
                    .await;

                ForecastResult {
                    success: false,
                    error: Some(error_msg),
                    output: None,
                }
            }
        }
    }

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
        let parser = Parser::new(tokens);
        match parser.parse() {
            Ok(program) => {
                // Semantic analysis
                let analyzer = SemanticAnalyzer::new();
                let analysis = analyzer.analyze(&program);

                if analysis.errors.is_empty() {
                    // Success - no errors (log message is async but we don't need to wait)
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

    fn get_completions(&self, params: CompletionParams) -> Vec<CompletionItem> {
        // Extract parameters
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        // Get document text and driver names
        let (text, driver_names) = if let Ok(docs) = self.documents.try_read() {
            if let Some(doc) = docs.get(&uri.to_string()) {
                (doc.text.clone(), doc.drivers.clone())
            } else {
                (String::new(), HashMap::new())
            }
        } else {
            (String::new(), HashMap::new())
        };

        // Analyze context
        let context = completions::CompletionContext::analyze(&text, position);

        // Get completions from the completions module
        completions::get_completions(&context, &driver_names)
    }

    // Removed: Large get_completions implementation is now in completions module
    // Removed: get_completion_context helper is now in completions module
    // Removed: get_driver_names helper - integrated above

    /*
    OLD IMPLEMENTATION REMOVED - NOW IN COMPLETIONS MODULE
    fn get_completions(&self, params: CompletionParams) -> Vec<CompletionItem> {
        ...458 lines removed...
    }
    */

    // DUMMY BLOCK TO BE REPLACED - START

    async fn get_hover_info(&self, params: HoverParams) -> Option<Hover> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        // Log hover request for debugging
        self.client
            .log_message(
                MessageType::INFO,
                &format!("Hover request at {}:{}", position.line, position.character),
            )
            .await;

        // Get document
        let docs = self.documents.read().await;
        let doc = match docs.get(&uri.to_string()) {
            Some(d) => d,
            None => {
                self.client
                    .log_message(MessageType::WARNING, "Document not found for hover")
                    .await;
                return None;
            }
        };

        // Get word at position
        let word = match hover::get_word_at_position(&doc.text, position) {
            Some(w) => {
                self.client
                    .log_message(MessageType::INFO, &format!("Hover word: '{}'", w))
                    .await;
                w
            }
            None => {
                self.client
                    .log_message(MessageType::WARNING, "No word found at position")
                    .await;
                return None;
            }
        };

        // Get hover info from hover module
        let result = hover::get_hover_info(&word, &doc.drivers);

        if result.is_none() {
            self.client
                .log_message(MessageType::INFO, &format!("No hover info for '{}'", word))
                .await;
        }

        result
    }

    // OLD IMPLEMENTATION REMOVED - Replaced with call to hover module
    /*
    async fn get_hover_info(&self, params: HoverParams) -> Option<Hover> {
        ...90+ lines of match statements removed...
    }
    */

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
