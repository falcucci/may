use std::fmt;

use crate::ast::BinaryOperator;
use crate::ast::ConstraintBlock;
use crate::ast::Declaration;
use crate::ast::Expression;
use crate::ast::Field;
use crate::ast::FunctionDeclaration;
use crate::ast::Identifier;
use crate::ast::IntegerLiteral;
use crate::ast::ModelDeclaration;
use crate::ast::ModelItem;
use crate::ast::Parameter;
use crate::ast::Source;
use crate::ast::StateDeclaration;
use crate::ast::StateTransition;
use crate::ast::Statement;
use crate::ast::TypeName;
use crate::lexer::Span;
use crate::lexer::Token;
use crate::lexer::TokenKind;
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
    let tokens = lex(source).map_err(|error| ParseError {
        message: error.message,
        span: error.span,
    })?;
    Parser { tokens, cursor: 0 }.parse_source()
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    fn parse_source(&mut self) -> Result<Source, ParseError> {
        let mut declarations = Vec::new();

        while !self.at(&TokenKind::Eof) {
            declarations.push(self.parse_declaration()?);
        }

        Ok(Source { declarations })
    }

    fn parse_declaration(&mut self) -> Result<Declaration, ParseError> {
        if self.at(&TokenKind::Model) {
            return self.parse_model().map(Declaration::Model);
        }
        if self.at(&TokenKind::State) {
            return self.parse_state().map(Declaration::State);
        }
        if self.at(&TokenKind::Fn) {
            return self.parse_function().map(Declaration::Function);
        }

        Err(self.error_here("expected a declaration"))
    }

    fn parse_model(&mut self) -> Result<ModelDeclaration, ParseError> {
        let start = self.expect(&TokenKind::Model)?.span;
        let name = self.expect_identifier()?;
        self.expect(&TokenKind::LeftBrace)?;

        let mut items = Vec::new();
        while !self.at(&TokenKind::RightBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error_here("expected `}` to close model declaration"));
            }

            if self.at(&TokenKind::Must) {
                items.push(ModelItem::Constraint(self.parse_constraint_block()?));
            } else {
                items.push(ModelItem::Field(self.parse_field()?));
            }
        }

        let end = self.expect(&TokenKind::RightBrace)?.span;
        Ok(ModelDeclaration {
            name,
            items,
            span: start.join(end),
        })
    }

    fn parse_state(&mut self) -> Result<StateDeclaration, ParseError> {
        let start = self.expect(&TokenKind::State)?.span;
        let name = self.expect_identifier()?;
        let model = if self.at(&TokenKind::LeftParen) {
            self.advance();
            let model = self.expect_identifier()?;
            self.expect(&TokenKind::RightParen)?;
            Some(model)
        } else {
            None
        };
        self.expect(&TokenKind::LeftBrace)?;

        let mut constraints = Vec::new();
        while !self.at(&TokenKind::RightBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error_here("expected `}` to close state declaration"));
            }
            constraints.push(self.parse_constraint_block()?);
        }

        let end = self.expect(&TokenKind::RightBrace)?.span;
        Ok(StateDeclaration {
            name,
            model,
            constraints,
            span: start.join(end),
        })
    }

    fn parse_function(&mut self) -> Result<FunctionDeclaration, ParseError> {
        let start = self.expect(&TokenKind::Fn)?.span;
        let name = self.expect_identifier()?;
        self.expect(&TokenKind::LeftParen)?;
        let params = self.parse_parameters()?;
        self.expect(&TokenKind::RightParen)?;
        let transition = if self.at(&TokenKind::When) {
            Some(self.parse_transition()?)
        } else {
            None
        };
        self.expect(&TokenKind::LeftBrace)?;

        let mut body = Vec::new();
        while !self.at(&TokenKind::RightBrace) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error_here("expected `}` to close function declaration"));
            }
            body.push(self.parse_statement()?);
        }

        let end = self.expect(&TokenKind::RightBrace)?.span;
        Ok(FunctionDeclaration {
            name,
            params,
            transition,
            body,
            span: start.join(end),
        })
    }

    fn parse_parameters(&mut self) -> Result<Vec<Parameter>, ParseError> {
        let mut params = Vec::new();
        if self.at(&TokenKind::RightParen) {
            return Ok(params);
        }

        loop {
            let name = self.expect_identifier()?;
            self.expect(&TokenKind::Colon)?;
            let ty = self.parse_type_name()?;
            let span = name.span.join(ty.name.span);
            params.push(Parameter { name, ty, span });

            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }

        Ok(params)
    }

    fn parse_transition(&mut self) -> Result<StateTransition, ParseError> {
        let start = self.expect(&TokenKind::When)?.span;
        let from = self.expect_identifier()?;
        self.expect(&TokenKind::Arrow)?;
        let to = self.expect_identifier()?;
        let span = start.join(to.span);
        Ok(StateTransition { from, to, span })
    }

    fn parse_field(&mut self) -> Result<Field, ParseError> {
        let name = self.expect_identifier()?;
        self.expect(&TokenKind::Colon)?;
        let ty = self.parse_type_name()?;
        let span = name.span.join(ty.name.span);
        self.eat(&TokenKind::Comma);
        self.eat(&TokenKind::Semicolon);
        Ok(Field { name, ty, span })
    }

    fn parse_type_name(&mut self) -> Result<TypeName, ParseError> {
        if let Some(identifier) = self.eat_identifier() {
            return Ok(TypeName { name: identifier });
        }

        let token = self.advance();
        let text = match &token.kind {
            TokenKind::Int => "int",
            TokenKind::UInt => "uint",
            TokenKind::Bool => "bool",
            TokenKind::StringType => "string",
            TokenKind::Address => "address",
            TokenKind::Hex => "hex",
            _ => {
                return Err(ParseError {
                    message: "expected a type name".to_owned(),
                    span: token.span,
                });
            }
        };

        Ok(TypeName {
            name: Identifier {
                text: text.to_owned(),
                span: token.span,
            },
        })
    }

    fn parse_constraint_block(&mut self) -> Result<ConstraintBlock, ParseError> {
        let start = self.expect(&TokenKind::Must)?.span;
        self.expect(&TokenKind::LeftBracket)?;

        let mut expressions = Vec::new();
        while !self.at(&TokenKind::RightBracket) {
            if self.at(&TokenKind::Eof) {
                return Err(self.error_here("expected `]` to close constraint block"));
            }

            expressions.push(self.parse_expression()?);
            self.eat(&TokenKind::Comma);
            self.eat(&TokenKind::Semicolon);
        }

        let end = self.expect(&TokenKind::RightBracket)?.span;
        Ok(ConstraintBlock {
            expressions,
            span: start.join(end),
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        if self.at(&TokenKind::Skip) {
            let start = self.expect(&TokenKind::Skip)?.span;
            let semicolon = self.expect(&TokenKind::Semicolon)?.span;
            return Ok(Statement::Skip {
                span: start.join(semicolon),
            });
        }

        Err(self.error_here("expected a statement"))
    }

    fn parse_expression(&mut self) -> Result<Expression, ParseError> { self.parse_comparison() }

    fn parse_comparison(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_additive()?;

        while let Some(op) = self.eat_comparison_operator() {
            let rhs = self.parse_additive()?;
            let span = expression.span().join(rhs.span());
            expression = Expression::Binary {
                lhs: Box::new(expression),
                op,
                rhs: Box::new(rhs),
                span,
            };
        }

        Ok(expression)
    }

    fn parse_additive(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_multiplicative()?;

        while let Some(op) = self.eat_additive_operator() {
            let rhs = self.parse_multiplicative()?;
            let span = expression.span().join(rhs.span());
            expression = Expression::Binary {
                lhs: Box::new(expression),
                op,
                rhs: Box::new(rhs),
                span,
            };
        }

        Ok(expression)
    }

    fn parse_multiplicative(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_primary()?;

        while let Some(op) = self.eat_multiplicative_operator() {
            let rhs = self.parse_primary()?;
            let span = expression.span().join(rhs.span());
            expression = Expression::Binary {
                lhs: Box::new(expression),
                op,
                rhs: Box::new(rhs),
                span,
            };
        }

        Ok(expression)
    }

    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        if let Some(identifier) = self.eat_identifier() {
            return Ok(Expression::Identifier(identifier));
        }

        if let TokenKind::Integer(text) = self.current().kind.clone() {
            let span = self.advance().span;
            return Ok(Expression::Integer(IntegerLiteral { text, span }));
        }

        if self.eat(&TokenKind::LeftParen) {
            let expression = self.parse_expression()?;
            self.expect(&TokenKind::RightParen)?;
            return Ok(expression);
        }

        Err(self.error_here("expected an expression"))
    }

    fn eat_comparison_operator(&mut self) -> Option<BinaryOperator> {
        let op = if self.eat(&TokenKind::GreaterEqual) {
            BinaryOperator::GreaterEqual
        } else if self.eat(&TokenKind::LessEqual) {
            BinaryOperator::LessEqual
        } else if self.eat(&TokenKind::EqualEqual) {
            BinaryOperator::Equal
        } else if self.eat(&TokenKind::BangEqual) {
            BinaryOperator::NotEqual
        } else if self.eat(&TokenKind::Greater) {
            BinaryOperator::Greater
        } else if self.eat(&TokenKind::Less) {
            BinaryOperator::Less
        } else {
            return None;
        };
        Some(op)
    }

    fn eat_additive_operator(&mut self) -> Option<BinaryOperator> {
        let op = if self.eat(&TokenKind::Plus) {
            BinaryOperator::Add
        } else if self.eat(&TokenKind::Minus) {
            BinaryOperator::Subtract
        } else {
            return None;
        };
        Some(op)
    }

    fn eat_multiplicative_operator(&mut self) -> Option<BinaryOperator> {
        let op = if self.eat(&TokenKind::Star) {
            BinaryOperator::Multiply
        } else if self.eat(&TokenKind::Slash) {
            BinaryOperator::Divide
        } else if self.eat(&TokenKind::Percent) {
            BinaryOperator::Modulo
        } else {
            return None;
        };
        Some(op)
    }

    fn eat_identifier(&mut self) -> Option<Identifier> {
        match self.current().kind.clone() {
            TokenKind::Identifier(text) => {
                let span = self.advance().span;
                Some(Identifier { text, span })
            }
            _ => None,
        }
    }

    fn expect_identifier(&mut self) -> Result<Identifier, ParseError> {
        self.eat_identifier().ok_or_else(|| self.error_here("expected an identifier"))
    }

    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.at(kind) {
            self.advance();
            return true;
        }
        false
    }

    fn expect(&mut self, kind: &TokenKind) -> Result<Token, ParseError> {
        if self.at(kind) {
            return Ok(self.advance());
        }

        Err(ParseError {
            message: format!("expected `{}`", kind.name()),
            span: self.current().span,
        })
    }

    fn at(&self, kind: &TokenKind) -> bool { discriminant_eq(&self.current().kind, kind) }

    fn current(&self) -> &Token {
        self.tokens
            .get(self.cursor)
            .expect("parser cursor should never move past EOF token")
    }

    fn advance(&mut self) -> Token {
        let token = self.current().clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.cursor += 1;
        }
        token
    }

    fn error_here(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            message: message.into(),
            span: self.current().span,
        }
    }
}

fn discriminant_eq(lhs: &TokenKind, rhs: &TokenKind) -> bool {
    std::mem::discriminant(lhs) == std::mem::discriminant(rhs)
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
