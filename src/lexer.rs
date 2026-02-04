/// FPL Lexer/Tokenizer
///
/// Transforms raw FPL source code into a stream of tokens for the parser.
/// Handles keywords, literals, operators, and provides rich error messages.

use std::fmt;

/// Token types in the FPL language
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Keywords
    Question,
    Driver,
    Evidence,
    Agent,
    Model,
    Simulate,
    Continuous,
    Binary,
    Triangular,
    Normal,
    Lognormal,
    Uniform,
    Beta,
    If,
    Then,
    Else,
    Schedule,
    Every,

    // Literals
    String(String),
    Number(f64),
    Probability(f64),     // 0.5p or 75%
    Date(String),         // YYYY-MM-DD format
    Boolean(bool),

    // Identifiers
    Identifier(String),

    // Operators
    Plus,                 // +
    Minus,                // -
    Star,                 // *
    Slash,                // /
    Percent,              // %
    Caret,                // ^

    // Comparison
    Equals,               // =
    DoubleEquals,         // ==
    NotEquals,            // !=
    Greater,              // >
    Less,                 // <
    GreaterEqual,         // >=
    LessEqual,            // <=

    // Logical
    And,                  // and
    Or,                   // or
    Not,                  // not

    // Delimiters
    LBrace,               // {
    RBrace,               // }
    LParen,               // (
    RParen,               // )
    LBracket,             // [
    RBracket,             // ]
    Comma,                // ,
    Colon,                // :
    Semicolon,            // ;
    Arrow,                // ->

    // Special
    Newline,
    Comment(String),
    Whitespace,
    EOF,
}

/// Token with location information
#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub lexeme: String,
    pub line: usize,
    pub column: usize,
    pub position: usize,
}

impl Token {
    pub fn new(token_type: TokenType, lexeme: String, line: usize, column: usize, position: usize) -> Self {
        Token {
            token_type,
            lexeme,
            line,
            column,
            position,
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} '{}' at {}:{}", self.token_type, self.lexeme, self.line, self.column)
    }
}

/// Lexer error types
#[derive(Debug, Clone)]
pub enum LexerError {
    UnterminatedString { line: usize, column: usize },
    InvalidNumber { lexeme: String, line: usize, column: usize },
    InvalidProbability { lexeme: String, line: usize, column: usize },
    InvalidDate { lexeme: String, line: usize, column: usize },
    UnexpectedCharacter { char: char, line: usize, column: usize },
    InvalidEscape { char: char, line: usize, column: usize },
}

impl fmt::Display for LexerError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LexerError::UnterminatedString { line, column } => {
                write!(f, "Unterminated string at {}:{}", line, column)
            }
            LexerError::InvalidNumber { lexeme, line, column } => {
                write!(f, "Invalid number '{}' at {}:{}", lexeme, line, column)
            }
            LexerError::InvalidProbability { lexeme, line, column } => {
                write!(f, "Invalid probability '{}' at {}:{}. Use format like 0.5p or 75%", lexeme, line, column)
            }
            LexerError::InvalidDate { lexeme, line, column } => {
                write!(f, "Invalid date '{}' at {}:{}. Use YYYY-MM-DD format", lexeme, line, column)
            }
            LexerError::UnexpectedCharacter { char, line, column } => {
                write!(f, "Unexpected character '{}' at {}:{}", char, line, column)
            }
            LexerError::InvalidEscape { char, line, column } => {
                write!(f, "Invalid escape sequence '\\{}' at {}:{}", char, line, column)
            }
        }
    }
}

/// Lexer for FPL
pub struct Lexer {
    source: Vec<char>,
    tokens: Vec<Token>,
    errors: Vec<LexerError>,

    // Position tracking
    current: usize,
    line: usize,
    column: usize,
    line_start: usize,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            tokens: Vec::new(),
            errors: Vec::new(),
            current: 0,
            line: 1,
            column: 1,
            line_start: 0,
        }
    }

    /// Tokenize the entire source
    pub fn tokenize(mut self) -> Result<Vec<Token>, Vec<LexerError>> {
        while !self.is_at_end() {
            self.scan_token();
        }

        // Add EOF token
        self.add_token(TokenType::EOF, "".to_string());

        if self.errors.is_empty() {
            Ok(self.tokens)
        } else {
            Err(self.errors)
        }
    }

    /// Scan a single token
    fn scan_token(&mut self) {
        let c = self.advance();

        match c {
            // Whitespace (ignore except newline)
            ' ' | '\r' | '\t' => {
                // Skip whitespace
            }

            '\n' => {
                self.line += 1;
                self.column = 1;
                self.line_start = self.current;
            }

            // Comments
            '#' => {
                self.scan_comment();
            }

            // Single-character tokens
            '{' => self.add_token_here(TokenType::LBrace, c.to_string()),
            '}' => self.add_token_here(TokenType::RBrace, c.to_string()),
            '(' => self.add_token_here(TokenType::LParen, c.to_string()),
            ')' => self.add_token_here(TokenType::RParen, c.to_string()),
            '[' => self.add_token_here(TokenType::LBracket, c.to_string()),
            ']' => self.add_token_here(TokenType::RBracket, c.to_string()),
            ',' => self.add_token_here(TokenType::Comma, c.to_string()),
            ':' => self.add_token_here(TokenType::Colon, c.to_string()),
            ';' => self.add_token_here(TokenType::Semicolon, c.to_string()),
            '+' => self.add_token_here(TokenType::Plus, c.to_string()),
            '*' => self.add_token_here(TokenType::Star, c.to_string()),
            '/' => self.add_token_here(TokenType::Slash, c.to_string()),
            '%' => self.add_token_here(TokenType::Percent, c.to_string()),
            '^' => self.add_token_here(TokenType::Caret, c.to_string()),

            // Two-character tokens
            '-' => {
                if self.match_char('>') {
                    self.add_token_here(TokenType::Arrow, "->".to_string());
                } else {
                    self.add_token_here(TokenType::Minus, c.to_string());
                }
            }

            '=' => {
                if self.match_char('=') {
                    self.add_token_here(TokenType::DoubleEquals, "==".to_string());
                } else {
                    self.add_token_here(TokenType::Equals, c.to_string());
                }
            }

            '!' => {
                if self.match_char('=') {
                    self.add_token_here(TokenType::NotEquals, "!=".to_string());
                } else {
                    self.error(LexerError::UnexpectedCharacter {
                        char: c,
                        line: self.line,
                        column: self.column - 1,
                    });
                }
            }

            '>' => {
                if self.match_char('=') {
                    self.add_token_here(TokenType::GreaterEqual, ">=".to_string());
                } else {
                    self.add_token_here(TokenType::Greater, c.to_string());
                }
            }

            '<' => {
                if self.match_char('=') {
                    self.add_token_here(TokenType::LessEqual, "<=".to_string());
                } else {
                    self.add_token_here(TokenType::Less, c.to_string());
                }
            }

            // String literals
            '"' => self.scan_string(),

            // Numbers
            '0'..='9' => self.scan_number(),

            // Identifiers and keywords
            'a'..='z' | 'A'..='Z' | '_' => self.scan_identifier(),

            // Unexpected character
            _ => {
                self.error(LexerError::UnexpectedCharacter {
                    char: c,
                    line: self.line,
                    column: self.column - 1,
                });
            }
        }
    }

    /// Scan a comment (from # to end of line)
    fn scan_comment(&mut self) {
        let start = self.current;

        while !self.is_at_end() && self.peek() != '\n' {
            self.advance();
        }

        let comment: String = self.source[start..self.current].iter().collect();
        // Comments are typically ignored, but we can preserve them for tooling
        // self.add_token(TokenType::Comment(comment), format!("#{}", comment));
    }

    /// Scan a string literal
    fn scan_string(&mut self) {
        let start_line = self.line;
        let start_column = self.column - 1;
        let mut string = String::new();

        while !self.is_at_end() && self.peek() != '"' {
            if self.peek() == '\n' {
                self.line += 1;
                self.column = 0;
                self.line_start = self.current + 1;
            }

            if self.peek() == '\\' {
                self.advance(); // consume backslash
                if !self.is_at_end() {
                    let escaped = self.advance();
                    match escaped {
                        'n' => string.push('\n'),
                        't' => string.push('\t'),
                        'r' => string.push('\r'),
                        '\\' => string.push('\\'),
                        '"' => string.push('"'),
                        _ => {
                            self.error(LexerError::InvalidEscape {
                                char: escaped,
                                line: self.line,
                                column: self.column - 1,
                            });
                            string.push(escaped);
                        }
                    }
                }
            } else {
                string.push(self.advance());
            }
        }

        if self.is_at_end() {
            self.error(LexerError::UnterminatedString {
                line: start_line,
                column: start_column,
            });
            return;
        }

        // Consume closing "
        self.advance();

        let lexeme = format!("\"{}\"", string);
        self.add_token(TokenType::String(string), lexeme);
    }

    /// Scan a number (integer, float, probability, or date)
    fn scan_number(&mut self) {
        let start = self.current - 1;

        // Consume digits
        while !self.is_at_end() && self.peek().is_ascii_digit() {
            self.advance();
        }

        // Check for decimal point
        if !self.is_at_end() && self.peek() == '.' && self.peek_next().map_or(false, |c| c.is_ascii_digit()) {
            self.advance(); // consume '.'

            while !self.is_at_end() && self.peek().is_ascii_digit() {
                self.advance();
            }
        }

        // Check for scientific notation
        if !self.is_at_end() && (self.peek() == 'e' || self.peek() == 'E') {
            self.advance(); // consume 'e' or 'E'

            if !self.is_at_end() && (self.peek() == '+' || self.peek() == '-') {
                self.advance(); // consume sign
            }

            while !self.is_at_end() && self.peek().is_ascii_digit() {
                self.advance();
            }
        }

        let lexeme: String = self.source[start..self.current].iter().collect();

        // Check for probability suffix (p or %)
        if !self.is_at_end() {
            if self.peek() == 'p' {
                self.advance();
                self.scan_probability_p(lexeme, start);
                return;
            } else if self.peek() == '%' {
                self.advance();
                self.scan_probability_percent(lexeme, start);
                return;
            }
        }

        // Check for date format (YYYY-MM-DD)
        if !self.is_at_end() && self.peek() == '-' && lexeme.len() == 4 {
            let date_str = self.try_scan_date(start);
            if let Some(date) = date_str {
                self.add_token(TokenType::Date(date.clone()), date);
                return;
            }
        }

        // Regular number
        match lexeme.parse::<f64>() {
            Ok(value) => {
                self.add_token(TokenType::Number(value), lexeme);
            }
            Err(_) => {
                self.error(LexerError::InvalidNumber {
                    lexeme: lexeme.clone(),
                    line: self.line,
                    column: self.column - lexeme.len(),
                });
            }
        }
    }

    /// Scan probability with 'p' suffix (e.g., 0.5p)
    fn scan_probability_p(&mut self, lexeme: String, start: usize) {
        match lexeme.parse::<f64>() {
            Ok(value) => {
                if value < 0.0 || value > 1.0 {
                    self.error(LexerError::InvalidProbability {
                        lexeme: format!("{}p", lexeme),
                        line: self.line,
                        column: self.column - lexeme.len() - 1,
                    });
                } else {
                    self.add_token(TokenType::Probability(value), format!("{}p", lexeme));
                }
            }
            Err(_) => {
                self.error(LexerError::InvalidProbability {
                    lexeme: format!("{}p", lexeme),
                    line: self.line,
                    column: self.column - lexeme.len() - 1,
                });
            }
        }
    }

    /// Scan probability with '%' suffix (e.g., 75%)
    fn scan_probability_percent(&mut self, lexeme: String, start: usize) {
        match lexeme.parse::<f64>() {
            Ok(value) => {
                if value < 0.0 || value > 100.0 {
                    self.error(LexerError::InvalidProbability {
                        lexeme: format!("{}%", lexeme),
                        line: self.line,
                        column: self.column - lexeme.len() - 1,
                    });
                } else {
                    let prob = value / 100.0;
                    self.add_token(TokenType::Probability(prob), format!("{}%", lexeme));
                }
            }
            Err(_) => {
                self.error(LexerError::InvalidProbability {
                    lexeme: format!("{}%", lexeme),
                    line: self.line,
                    column: self.column - lexeme.len() - 1,
                });
            }
        }
    }

    /// Try to scan a date in YYYY-MM-DD format
    fn try_scan_date(&mut self, start: usize) -> Option<String> {
        let saved_current = self.current;
        let saved_column = self.column;

        // Try to match YYYY-MM-DD
        if !self.match_char('-') {
            return None;
        }

        // MM
        if !self.is_digit(self.peek()) || !self.is_digit(self.peek_next().unwrap_or('x')) {
            self.current = saved_current;
            self.column = saved_column;
            return None;
        }
        self.advance();
        self.advance();

        if !self.match_char('-') {
            self.current = saved_current;
            self.column = saved_column;
            return None;
        }

        // DD
        if !self.is_digit(self.peek()) || !self.is_digit(self.peek_next().unwrap_or('x')) {
            self.current = saved_current;
            self.column = saved_column;
            return None;
        }
        self.advance();
        self.advance();

        let date_str: String = self.source[start..self.current].iter().collect();

        // Basic validation
        if self.is_valid_date(&date_str) {
            Some(date_str)
        } else {
            self.current = saved_current;
            self.column = saved_column;
            None
        }
    }

    /// Basic date validation
    fn is_valid_date(&self, date: &str) -> bool {
        let parts: Vec<&str> = date.split('-').collect();
        if parts.len() != 3 {
            return false;
        }

        if let (Ok(year), Ok(month), Ok(day)) = (
            parts[0].parse::<u32>(),
            parts[1].parse::<u32>(),
            parts[2].parse::<u32>(),
        ) {
            year >= 1900 && year <= 2100 && month >= 1 && month <= 12 && day >= 1 && day <= 31
        } else {
            false
        }
    }

    /// Scan an identifier or keyword
    fn scan_identifier(&mut self) {
        let start = self.current - 1;

        while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_') {
            self.advance();
        }

        let lexeme: String = self.source[start..self.current].iter().collect();

        // Check if it's a keyword
        let token_type = match lexeme.as_str() {
            // Statement keywords
            "question" => TokenType::Question,
            "driver" => TokenType::Driver,
            "evidence" => TokenType::Evidence,
            "agent" => TokenType::Agent,
            "model" => TokenType::Model,
            "simulate" => TokenType::Simulate,

            // Driver types
            "continuous" => TokenType::Continuous,
            "binary" => TokenType::Binary,

            // Distribution types
            "triangular" => TokenType::Triangular,
            "normal" => TokenType::Normal,
            "lognormal" => TokenType::Lognormal,
            "uniform" => TokenType::Uniform,
            "beta" => TokenType::Beta,

            // Control flow
            "if" => TokenType::If,
            "then" => TokenType::Then,
            "else" => TokenType::Else,

            // Scheduling
            "schedule" => TokenType::Schedule,
            "every" => TokenType::Every,

            // Logical operators
            "and" => TokenType::And,
            "or" => TokenType::Or,
            "not" => TokenType::Not,

            // Boolean literals
            "true" => TokenType::Boolean(true),
            "false" => TokenType::Boolean(false),

            // Otherwise it's an identifier
            _ => TokenType::Identifier(lexeme.clone()),
        };

        self.add_token(token_type, lexeme);
    }

    // Helper methods

    fn is_at_end(&self) -> bool {
        self.current >= self.source.len()
    }

    fn advance(&mut self) -> char {
        let c = self.source[self.current];
        self.current += 1;
        self.column += 1;
        c
    }

    fn peek(&self) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            self.source[self.current]
        }
    }

    fn peek_next(&self) -> Option<char> {
        if self.current + 1 >= self.source.len() {
            None
        } else {
            Some(self.source[self.current + 1])
        }
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.is_at_end() || self.peek() != expected {
            false
        } else {
            self.advance();
            true
        }
    }

    fn is_digit(&self, c: char) -> bool {
        c.is_ascii_digit()
    }

    fn add_token(&mut self, token_type: TokenType, lexeme: String) {
        let lexeme_len = lexeme.len();
        let token = Token::new(
            token_type,
            lexeme,
            self.line,
            self.column - lexeme_len,
            self.current - lexeme_len,
        );
        self.tokens.push(token);
    }

    fn add_token_here(&mut self, token_type: TokenType, lexeme: String) {
        let token = Token::new(
            token_type,
            lexeme.clone(),
            self.line,
            self.column - lexeme.len(),
            self.current - lexeme.len(),
        );
        self.tokens.push(token);
    }

    fn error(&mut self, error: LexerError) {
        self.errors.push(error);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keywords() {
        let source = "question driver evidence agent model simulate";
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens.len(), 7); // 6 keywords + EOF
        assert!(matches!(tokens[0].token_type, TokenType::Question));
        assert!(matches!(tokens[1].token_type, TokenType::Driver));
        assert!(matches!(tokens[5].token_type, TokenType::Simulate));
    }

    #[test]
    fn test_numbers() {
        let source = "42 3.14 1.5e10 0.001";
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens.len(), 5); // 4 numbers + EOF
        assert!(matches!(tokens[0].token_type, TokenType::Number(42.0)));
        assert!(matches!(tokens[1].token_type, TokenType::Number(3.14)));
    }

    #[test]
    fn test_probability() {
        let source = "0.5p 75% 0.95p";
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens.len(), 4); // 3 probabilities + EOF
        assert!(matches!(tokens[0].token_type, TokenType::Probability(0.5)));
        assert!(matches!(tokens[1].token_type, TokenType::Probability(0.75)));
    }

    #[test]
    fn test_date() {
        let source = "2026-12-31 2025-01-15";
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens.len(), 3); // 2 dates + EOF
        assert!(matches!(tokens[0].token_type, TokenType::Date(_)));
    }

    #[test]
    fn test_string() {
        let source = r#""Hello, World!" "Line 1\nLine 2""#;
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens.len(), 3); // 2 strings + EOF
        if let TokenType::String(s) = &tokens[0].token_type {
            assert_eq!(s, "Hello, World!");
        }
    }

    #[test]
    fn test_operators() {
        let source = "+ - * / = == != > < >= <= -> and or not";
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert!(tokens.len() > 10);
        assert!(matches!(tokens[0].token_type, TokenType::Plus));
        assert!(matches!(tokens[5].token_type, TokenType::DoubleEquals));
    }

    #[test]
    fn test_identifiers() {
        let source = "market_size growth_rate user_count";
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        assert_eq!(tokens.len(), 4); // 3 identifiers + EOF
        if let TokenType::Identifier(name) = &tokens[0].token_type {
            assert_eq!(name, "market_size");
        }
    }

    #[test]
    fn test_comment() {
        let source = "driver market_size # This is a comment\ntriangular";
        let lexer = Lexer::new(source);
        let tokens = lexer.tokenize().unwrap();

        // Comments are ignored
        assert_eq!(tokens.len(), 3); // driver, market_size, triangular, EOF
    }

    #[test]
    fn test_error_unterminated_string() {
        let source = r#""unterminated"#;
        let lexer = Lexer::new(source);
        let result = lexer.tokenize();

        assert!(result.is_err());
        if let Err(errors) = result {
            assert_eq!(errors.len(), 1);
            assert!(matches!(errors[0], LexerError::UnterminatedString { .. }));
        }
    }

    #[test]
    fn test_error_invalid_probability() {
        let source = "1.5p 150%"; // Out of range
        let lexer = Lexer::new(source);
        let result = lexer.tokenize();

        assert!(result.is_err());
    }

    #[test]
    fn test_complete_forecast() {
        let source = r#"
question "Will AMD reach $200 by 2026-12-31?"

driver market_size continuous {
    distribution: triangular(500, 1200, 2500)
    unit: "millions USD"
}

driver growth_rate continuous {
    distribution: normal(0.25, 0.05)
    unit: "ratio"
}

evidence market_report {
    source: "Gartner Research 2025"
    relevance: 0.9p
    date: 2025-09-15
}

agent research {
    query: "AMD market size projections"
    schedule: every 1 week
}

model: market_size * (1 + growth_rate)

simulate 10000 iterations
"#;

        let lexer = Lexer::new(source);
        let result = lexer.tokenize();

        assert!(result.is_ok());
        let tokens = result.unwrap();

        // Verify we got a reasonable number of tokens
        assert!(tokens.len() > 50);

        // Verify some key tokens
        assert!(matches!(tokens[0].token_type, TokenType::Question));

        // Find the driver keyword
        let driver_pos = tokens.iter().position(|t| matches!(t.token_type, TokenType::Driver));
        assert!(driver_pos.is_some());

        // Find the simulate keyword
        let simulate_pos = tokens.iter().position(|t| matches!(t.token_type, TokenType::Simulate));
        assert!(simulate_pos.is_some());
    }
}
