use std::fmt;

use logos::Logos;

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

impl From<Span> for diagnostics::Span {
    fn from(value: Span) -> Self { Self::new(value.start, value.end) }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LogosError {
    #[default]
    InvalidToken,
}

#[derive(Logos, Debug, Clone, PartialEq, Eq)]
#[logos(skip r"[ \t\n\r\f]+")]
#[logos(skip r"//[^\n]*?")]
#[logos(error = LogosError)]
pub enum Token {
    #[token("model")]
    Model,
    #[token("state")]
    State,
    #[token("fn")]
    Fn,
    #[token("when")]
    When,
    #[token("as")]
    As,
    #[token("must")]
    Must,
    #[token("skip")]
    Skip,
    #[token("int")]
    Int,
    #[token("uint")]
    UInt,
    #[token("bool")]
    Bool,
    #[token("string")]
    StringType,
    #[token("address")]
    Address,
    #[token("hex")]
    Hex,
    #[regex("[_a-zA-Z][_0-9a-zA-Z]*", |lexer| lexer.slice().to_owned())]
    Identifier(String),
    #[regex("[0-9]+", |lexer| lexer.slice().to_owned())]
    Integer(String),
    #[token("{")]
    LeftBrace,
    #[token("}")]
    RightBrace,
    #[token("(")]
    LeftParen,
    #[token(")")]
    RightParen,
    #[token("[")]
    LeftBracket,
    #[token("]")]
    RightBracket,
    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token(";")]
    Semicolon,
    #[token(".")]
    Dot,
    #[token("->")]
    Arrow,
    #[token(">=")]
    GreaterEqual,
    #[token("<=")]
    LessEqual,
    #[token("==")]
    EqualEqual,
    #[token("!=")]
    BangEqual,
    #[token("=")]
    Equal,
    #[token(">")]
    Greater,
    #[token("<")]
    Less,
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
}

impl Token {
    pub fn name(&self) -> &'static str {
        match self {
            Token::Model => "model",
            Token::State => "state",
            Token::Fn => "fn",
            Token::When => "when",
            Token::As => "as",
            Token::Must => "must",
            Token::Skip => "skip",
            Token::Int => "int",
            Token::UInt => "uint",
            Token::Bool => "bool",
            Token::StringType => "string",
            Token::Address => "address",
            Token::Hex => "hex",
            Token::Identifier(_) => "identifier",
            Token::Integer(_) => "integer",
            Token::LeftBrace => "{",
            Token::RightBrace => "}",
            Token::LeftParen => "(",
            Token::RightParen => ")",
            Token::LeftBracket => "[",
            Token::RightBracket => "]",
            Token::Colon => ":",
            Token::Comma => ",",
            Token::Semicolon => ";",
            Token::Dot => ".",
            Token::Arrow => "->",
            Token::GreaterEqual => ">=",
            Token::LessEqual => "<=",
            Token::EqualEqual => "==",
            Token::BangEqual => "!=",
            Token::Equal => "=",
            Token::Greater => ">",
            Token::Less => "<",
            Token::Plus => "+",
            Token::Minus => "-",
            Token::Star => "*",
            Token::Slash => "/",
            Token::Percent => "%",
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Identifier(text) | Token::Integer(text) => write!(f, "{text}"),
            token => write!(f, "{}", token.name()),
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

impl diagnostics::ToReport for LexError {
    fn to_report(&self) -> diagnostics::Report {
        diagnostics::Report::lexer(self.span.into(), self.message.clone())
    }
}

pub type SpannedToken = (usize, Token, usize);

pub fn lex(source: &str) -> Result<Vec<SpannedToken>, LexError> {
    let mut tokens = Vec::new();

    for (token, span) in Token::lexer(source).spanned() {
        let token = token.map_err(|_| LexError {
            message: "unexpected token".to_owned(),
            span: Span::new(span.start, span.end),
        })?;
        tokens.push((span.start, token, span.end));
    }

    Ok(tokens)
}
