use std::fmt;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self { Self { start, end } }
    pub fn join(self, other: Span) -> Self {
        Self {
            start: self.start,
            end: other.end,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Model,
    State,
    Fn,
    When,
    Must,
    Skip,
    Int,
    UInt,
    Bool,
    StringType,
    Address,
    Hex,
    Identifier(String),
    Integer(String),
    LeftBrace,
    RightBrace,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    Colon,
    Comma,
    Semicolon,
    Arrow,
    GreaterEqual,
    LessEqual,
    EqualEqual,
    BangEqual,
    Greater,
    Less,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eof,
}

impl TokenKind {
    pub fn name(&self) -> &'static str {
        match self {
            TokenKind::Model => "model",
            TokenKind::State => "state",
            TokenKind::Fn => "fn",
            TokenKind::When => "when",
            TokenKind::Must => "must",
            TokenKind::Skip => "skip",
            TokenKind::Int => "int",
            TokenKind::UInt => "uint",
            TokenKind::Bool => "bool",
            TokenKind::StringType => "string",
            TokenKind::Address => "address",
            TokenKind::Hex => "hex",
            TokenKind::Identifier(_) => "identifier",
            TokenKind::Integer(_) => "integer",
            TokenKind::LeftBrace => "{",
            TokenKind::RightBrace => "}",
            TokenKind::LeftParen => "(",
            TokenKind::RightParen => ")",
            TokenKind::LeftBracket => "[",
            TokenKind::RightBracket => "]",
            TokenKind::Colon => ":",
            TokenKind::Comma => ",",
            TokenKind::Semicolon => ";",
            TokenKind::Arrow => "->",
            TokenKind::GreaterEqual => ">=",
            TokenKind::LessEqual => "<=",
            TokenKind::EqualEqual => "==",
            TokenKind::BangEqual => "!=",
            TokenKind::Greater => ">",
            TokenKind::Less => "<",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            TokenKind::Eof => "end of file",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for LexError {}

pub fn lex(source: &str) -> Result<Vec<Token>, LexError> {
    let mut lexer = Lexer {
        source,
        bytes: source.as_bytes(),
        cursor: 0,
        tokens: Vec::new(),
    };
    lexer.lex()?;
    Ok(lexer.tokens)
}

struct Lexer<'src> {
    source: &'src str,
    bytes: &'src [u8],
    cursor: usize,
    tokens: Vec<Token>,
}

impl Lexer<'_> {
    fn lex(&mut self) -> Result<(), LexError> {
        while let Some(byte) = self.peek() {
            match byte {
                b' ' | b'\t' | b'\n' | b'\r' => {
                    self.cursor += 1;
                }
                b'/' if self.peek_next() == Some(b'/') => self.skip_line_comment(),
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.lex_identifier_or_keyword(),
                b'0'..=b'9' => self.lex_integer(),
                b'{' => self.push_single(TokenKind::LeftBrace),
                b'}' => self.push_single(TokenKind::RightBrace),
                b'(' => self.push_single(TokenKind::LeftParen),
                b')' => self.push_single(TokenKind::RightParen),
                b'[' => self.push_single(TokenKind::LeftBracket),
                b']' => self.push_single(TokenKind::RightBracket),
                b':' => self.push_single(TokenKind::Colon),
                b',' => self.push_single(TokenKind::Comma),
                b';' => self.push_single(TokenKind::Semicolon),
                b'+' => self.push_single(TokenKind::Plus),
                b'*' => self.push_single(TokenKind::Star),
                b'%' => self.push_single(TokenKind::Percent),
                b'-' if self.peek_next() == Some(b'>') => self.push_pair(TokenKind::Arrow),
                b'-' => self.push_single(TokenKind::Minus),
                b'>' if self.peek_next() == Some(b'=') => self.push_pair(TokenKind::GreaterEqual),
                b'>' => self.push_single(TokenKind::Greater),
                b'<' if self.peek_next() == Some(b'=') => self.push_pair(TokenKind::LessEqual),
                b'<' => self.push_single(TokenKind::Less),
                b'=' if self.peek_next() == Some(b'=') => self.push_pair(TokenKind::EqualEqual),
                b'!' if self.peek_next() == Some(b'=') => self.push_pair(TokenKind::BangEqual),
                b'/' => self.push_single(TokenKind::Slash),
                _ => {
                    let span = Span::new(self.cursor, self.cursor + 1);
                    return Err(LexError {
                        message: format!("unexpected character `{}`", byte as char),
                        span,
                    });
                }
            }
        }

        self.tokens.push(Token {
            kind: TokenKind::Eof,
            span: Span::new(self.source.len(), self.source.len()),
        });
        Ok(())
    }

    fn peek(&self) -> Option<u8> { self.bytes.get(self.cursor).copied() }

    fn peek_next(&self) -> Option<u8> { self.bytes.get(self.cursor + 1).copied() }

    fn skip_line_comment(&mut self) {
        self.cursor += 2;
        while let Some(byte) = self.peek() {
            self.cursor += 1;
            if byte == b'\n' {
                break;
            }
        }
    }

    fn lex_identifier_or_keyword(&mut self) {
        let start = self.cursor;
        while matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'0'..=b'9')
        ) {
            self.cursor += 1;
        }

        let text = &self.source[start..self.cursor];
        let kind = match text {
            "model" => TokenKind::Model,
            "state" => TokenKind::State,
            "fn" => TokenKind::Fn,
            "when" => TokenKind::When,
            "must" => TokenKind::Must,
            "skip" => TokenKind::Skip,
            "int" => TokenKind::Int,
            "uint" => TokenKind::UInt,
            "bool" => TokenKind::Bool,
            "string" => TokenKind::StringType,
            "address" => TokenKind::Address,
            "hex" => TokenKind::Hex,
            _ => TokenKind::Identifier(text.to_owned()),
        };
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.cursor),
        });
    }

    fn lex_integer(&mut self) {
        let start = self.cursor;
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.cursor += 1;
        }

        self.tokens.push(Token {
            kind: TokenKind::Integer(self.source[start..self.cursor].to_owned()),
            span: Span::new(start, self.cursor),
        });
    }

    fn push_single(&mut self, kind: TokenKind) {
        let start = self.cursor;
        self.cursor += 1;
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.cursor),
        });
    }

    fn push_pair(&mut self, kind: TokenKind) {
        let start = self.cursor;
        self.cursor += 2;
        self.tokens.push(Token {
            kind,
            span: Span::new(start, self.cursor),
        });
    }
}
