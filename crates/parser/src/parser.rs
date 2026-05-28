use std::fmt;

use lalrpop_util::ParseError as LalrpopParseError;

use crate::ast::Source;
use crate::lexer::LexError;
use crate::lexer::Span;
use crate::lexer::Token;
use crate::lexer::lex;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    Lexer,
    Parser,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
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

impl diagnostics::ToReport for ParseError {
    fn to_report(&self) -> diagnostics::Report {
        match self.kind {
            ParseErrorKind::Lexer => {
                diagnostics::Report::lexer(self.span.into(), self.message.clone())
            }
            ParseErrorKind::Parser => {
                diagnostics::Report::parser(self.span.into(), self.message.clone())
            }
        }
    }
}

pub fn parse_source(source: &str) -> Result<Source, ParseError> {
    let tokens = lex(source).map_err(ParseError::from)?;

    crate::may::SourceParser::new()
        .parse(tokens.into_iter())
        .map_err(ParseError::from)
}

impl From<LexError> for ParseError {
    fn from(error: LexError) -> Self {
        Self {
            kind: ParseErrorKind::Lexer,
            message: error.message,
            span: error.span,
        }
    }
}

impl From<LalrpopParseError<usize, Token, LexError>> for ParseError {
    fn from(error: LalrpopParseError<usize, Token, LexError>) -> Self {
        match error {
            LalrpopParseError::InvalidToken { location } => Self {
                kind: ParseErrorKind::Parser,
                message: "invalid token".to_owned(),
                span: Span::new(location, location),
            },
            LalrpopParseError::UnrecognizedEof { location, expected } => Self {
                kind: ParseErrorKind::Parser,
                message: expected_message("unexpected end of file", &expected),
                span: Span::new(location, location),
            },
            LalrpopParseError::UnrecognizedToken { token, expected } => Self {
                kind: ParseErrorKind::Parser,
                message: expected_message(&format!("unexpected `{}`", token.1), &expected),
                span: Span::new(token.0, token.2),
            },
            LalrpopParseError::ExtraToken { token } => Self {
                kind: ParseErrorKind::Parser,
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

fn increment(amount: int) when Ready -> Ready
must [ amount > 0 ]
{
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
        assert!(
            function
                .transition
                .as_ref()
                .and_then(|transition| transition.from_alias.as_ref())
                .is_none()
        );
        assert_eq!(
            function.transition.as_ref().map(|transition| transition.to.text.as_str()),
            Some("Ready")
        );
        assert!(
            function
                .transition
                .as_ref()
                .and_then(|transition| transition.to_alias.as_ref())
                .is_none()
        );
        assert_eq!(function.constraints.len(), 1);
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

    #[test]
    fn parses_transition_aliases_and_field_access() {
        let source = parse_source(
            r#"
model Counter {
    value: int
}

state Ready(Counter) {}

fn increment(amount: int) when Ready as before -> Ready as after
must [ after.value == before.value + amount ]
{
    skip;
}
"#,
        )
        .expect("source should parse");

        let Declaration::Function(function) = &source.declarations[2] else {
            panic!("expected function declaration");
        };
        let transition = function.transition.as_ref().expect("expected state transition");
        assert_eq!(
            transition.from_alias.as_ref().map(|alias| alias.text.as_str()),
            Some("before")
        );
        assert_eq!(
            transition.to_alias.as_ref().map(|alias| alias.text.as_str()),
            Some("after")
        );

        let Expression::Binary { lhs, rhs, op, .. } = &function.constraints[0].expressions[0]
        else {
            panic!("expected equality expression");
        };
        assert_eq!(*op, BinaryOperator::Equal);
        assert!(matches!(
            lhs.as_ref(),
            Expression::FieldAccess { base, field, .. }
                if base.text == "after" && field.text == "value"
        ));
        assert!(matches!(
            rhs.as_ref(),
            Expression::Binary {
                lhs,
                op: BinaryOperator::Add,
                ..
            } if matches!(
                lhs.as_ref(),
                Expression::FieldAccess { base, field, .. }
                    if base.text == "before" && field.text == "value"
            )
        ));
    }

    #[test]
    fn parses_assignment_statements() {
        let source = parse_source(
            r#"
model Counter {
    value: int
}

state Ready(Counter) {}

fn increment(amount: int) when Ready as before -> Ready as after
must [ after.value == before.value + amount ]
{
    after.value = before.value + amount;
}
"#,
        )
        .expect("source should parse");

        let Declaration::Function(function) = &source.declarations[2] else {
            panic!("expected function declaration");
        };

        let [Statement::Assignment(assignment)] = function.body.as_slice() else {
            panic!("expected assignment statement");
        };

        assert_eq!(assignment.target.base.text, "after");
        assert_eq!(assignment.target.field.text, "value");
        assert!(matches!(
            &assignment.value,
            Expression::Binary {
                op: BinaryOperator::Add,
                ..
            }
        ));
    }
}
