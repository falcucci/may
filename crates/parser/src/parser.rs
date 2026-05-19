use std::fmt;

use lalrpop_util::ParseError as LalrpopParseError;

use crate::ast::Source;
use crate::lexer::LexError;
use crate::lexer::Span;
use crate::lexer::Token;
use crate::lexer::lex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for ParseError {}

pub fn parse_source(source: &str) -> Result<Source, ParseError> {
    let tokens = lex(source).map_err(ParseError::from)?;

    crate::may::SourceParser::new()
        .parse(tokens.into_iter())
        .map_err(ParseError::from)
}

impl From<LexError> for ParseError {
    fn from(error: LexError) -> Self {
        Self {
            message: error.message,
            span: error.span,
        }
    }
}

impl From<LalrpopParseError<usize, Token, LexError>> for ParseError {
    fn from(error: LalrpopParseError<usize, Token, LexError>) -> Self {
        match error {
            LalrpopParseError::InvalidToken { location } => Self {
                message: "invalid token".to_owned(),
                span: Span::new(location, location),
            },
            LalrpopParseError::UnrecognizedEof { location, expected } => Self {
                message: expected_message("unexpected end of file", &expected),
                span: Span::new(location, location),
            },
            LalrpopParseError::UnrecognizedToken { token, expected } => Self {
                message: expected_message(&format!("unexpected `{}`", token.1), &expected),
                span: Span::new(token.0, token.2),
            },
            LalrpopParseError::ExtraToken { token } => Self {
                message: format!("extra token `{}`", token.1),
                span: Span::new(token.0, token.2),
            },
            LalrpopParseError::User { error } => ParseError::from(error),
        }
    }
}

fn expected_message(prefix: &str, expected: &[String]) -> String {
    if expected.is_empty() {
        return prefix.to_owned();
    }

    format!("{prefix}; expected {}", expected.join(", "))
}

#[cfg(test)]
mod tests {
    use crate::ast::BinaryOperator;
    use crate::ast::Declaration;
    use crate::ast::Expression;
    use crate::ast::ModelItem;
    use crate::ast::Statement;
    use crate::parse_source;

    const COUNTER: &str = r#"
model Counter {
    value: int

    must [
        value >= 0
    ]
}

state Ready(Counter) {
    must [
        value >= 0
    ]
}

fn increment(amount: int) when Ready -> Ready {
    skip;
}
"#;

    #[test]
    fn parses_counter_subset() {
        let source = parse_source(COUNTER).expect("source should parse");

        assert_eq!(source.declarations.len(), 3);

        let Declaration::Model(model) = &source.declarations[0] else {
            panic!("expected model declaration");
        };
        assert_eq!(model.name.text, "Counter");
        assert_eq!(model.items.len(), 2);
        assert!(matches!(&model.items[0], ModelItem::Field(field) if field.name.text == "value"));
        assert!(matches!(&model.items[1], ModelItem::Constraint(_)));

        let Declaration::State(state) = &source.declarations[1] else {
            panic!("expected state declaration");
        };
        assert_eq!(state.name.text, "Ready");
        assert_eq!(
            state.model.as_ref().map(|name| name.text.as_str()),
            Some("Counter")
        );
        assert_eq!(state.constraints.len(), 1);

        let Declaration::Function(function) = &source.declarations[2] else {
            panic!("expected function declaration");
        };
        assert_eq!(function.name.text, "increment");
        assert_eq!(function.params.len(), 1);
        assert_eq!(function.params[0].name.text, "amount");
        assert_eq!(
            function.transition.as_ref().map(|transition| transition.from.text.as_str()),
            Some("Ready")
        );
        assert_eq!(
            function.transition.as_ref().map(|transition| transition.to.text.as_str()),
            Some("Ready")
        );
        assert!(matches!(function.body.as_slice(), [Statement::Skip { .. }]));
    }

    #[test]
    fn parses_binary_expression_precedence() {
        let source = parse_source(
            r#"
model Counter {
    value: int
    must [
        value + 1 >= 2 * 3
    ]
}
"#,
        )
        .expect("source should parse");

        let Declaration::Model(model) = &source.declarations[0] else {
            panic!("expected model declaration");
        };
        let ModelItem::Constraint(block) = &model.items[1] else {
            panic!("expected constraint block");
        };
        let Expression::Binary { op, .. } = &block.expressions[0] else {
            panic!("expected binary expression");
        };
        assert_eq!(*op, BinaryOperator::GreaterEqual);
    }
}
