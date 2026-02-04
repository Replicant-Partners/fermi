use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug)]
struct Backend {
    client: Client,
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
}

impl Backend {
    async fn on_change(&self, uri: Url, text: String) {
        // Parse the FPL code and get diagnostics
        let diagnostics = self.parse_and_diagnose(&text);

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
                        fermi::LexerError::InvalidNumber { lexeme, line, column } => {
                            (line, column, format!("Invalid number: {}", lexeme))
                        }
                        fermi::LexerError::InvalidProbability { lexeme, line, column } => {
                            (line, column, format!("Invalid probability: {}", lexeme))
                        }
                        fermi::LexerError::InvalidDate { lexeme, line, column } => {
                            (line, column, format!("Invalid date: {}", lexeme))
                        }
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
                    self.client.log_message(
                        MessageType::INFO,
                        "Parse successful - no errors",
                    );
                } else {
                    // Semantic errors
                    for error in analysis.errors {
                        let message = match error {
                            fermi::SemanticError::UndefinedSymbol { name, message } => {
                                format!("Undefined symbol '{}': {}", name, message)
                            }
                            fermi::SemanticError::TypeMismatch { expected, found, message } => {
                                format!("Type mismatch: expected {:?}, found {:?}. {}", expected, found, message)
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
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(|client| Backend { client });
    Server::new(stdin, stdout, socket).serve(service).await;
}
