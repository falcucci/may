use std::collections::HashMap;
use std::fmt;

use parser::Span;
use parser::ast;
use semantics::Bounds;
use semantics::ContractDefinition;
use semantics::FieldDefinition;
use semantics::FunctionDefinition;
use semantics::ParameterDefinition;
use semantics::StateDefinition;
use semantics::Type;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub checked_bounds: usize,
    pub model_bounds: usize,
    pub state_bounds: usize,
    pub function_bounds: usize,
    pub constraints: Vec<Constraint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint {
    pub owner: BoundOwner,
    pub expression: VerifiedExpression,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifiedExpression {
    IntLiteral {
        text: String,
        span: Span,
    },
    Identifier {
        name: String,
        ty: VerifiedType,
        span: Span,
    },
    Binary {
        lhs: Box<VerifiedExpression>,
        op: VerifiedBinaryOperator,
        rhs: Box<VerifiedExpression>,
        ty: VerifiedType,
        span: Span,
    },
}

impl VerifiedExpression {
    pub fn ty(&self) -> VerifiedType {
        match self {
            VerifiedExpression::IntLiteral { .. } => VerifiedType::IntegerLiteral,
            VerifiedExpression::Identifier { ty, .. } | VerifiedExpression::Binary { ty, .. } => {
                ty.clone()
            }
        }
    }

    pub fn span(&self) -> Span {
        match self {
            VerifiedExpression::IntLiteral { span, .. }
            | VerifiedExpression::Identifier { span, .. }
            | VerifiedExpression::Binary { span, .. } => *span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifiedType {
    Int,
    UInt,
    Bool,
    String,
    Address,
    Hex,
    Custom(String),
    IntegerLiteral,
}

impl VerifiedType {
    fn is_numeric(&self) -> bool {
        matches!(
            self,
            VerifiedType::Int | VerifiedType::UInt | VerifiedType::IntegerLiteral
        )
    }

    fn is_compatible_with(&self, other: &Self) -> bool {
        self == other || (self.is_numeric() && other.is_numeric())
    }

    fn merged_numeric(lhs: &Self, rhs: &Self) -> Self {
        if lhs == &VerifiedType::Int || rhs == &VerifiedType::Int {
            VerifiedType::Int
        } else if lhs == &VerifiedType::UInt || rhs == &VerifiedType::UInt {
            VerifiedType::UInt
        } else {
            VerifiedType::IntegerLiteral
        }
    }
}

impl From<&Type> for VerifiedType {
    fn from(value: &Type) -> Self {
        match value {
            Type::Int => VerifiedType::Int,
            Type::UInt => VerifiedType::UInt,
            Type::Bool => VerifiedType::Bool,
            Type::String => VerifiedType::String,
            Type::Address => VerifiedType::Address,
            Type::Hex => VerifiedType::Hex,
            Type::Custom(name) => VerifiedType::Custom(name.clone()),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VerifiedBinaryOperator {
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
}

impl From<ast::BinaryOperator> for VerifiedBinaryOperator {
    fn from(value: ast::BinaryOperator) -> Self {
        match value {
            ast::BinaryOperator::Equal => VerifiedBinaryOperator::Equal,
            ast::BinaryOperator::NotEqual => VerifiedBinaryOperator::NotEqual,
            ast::BinaryOperator::Greater => VerifiedBinaryOperator::Greater,
            ast::BinaryOperator::GreaterEqual => VerifiedBinaryOperator::GreaterEqual,
            ast::BinaryOperator::Less => VerifiedBinaryOperator::Less,
            ast::BinaryOperator::LessEqual => VerifiedBinaryOperator::LessEqual,
            ast::BinaryOperator::Add => VerifiedBinaryOperator::Add,
            ast::BinaryOperator::Subtract => VerifiedBinaryOperator::Subtract,
            ast::BinaryOperator::Multiply => VerifiedBinaryOperator::Multiply,
            ast::BinaryOperator::Divide => VerifiedBinaryOperator::Divide,
            ast::BinaryOperator::Modulo => VerifiedBinaryOperator::Modulo,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundOwner {
    Model(String),
    State(String),
    Function(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationError {
    pub message: String,
    pub span: Span,
}

impl VerificationError {
    fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

impl fmt::Display for VerificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for VerificationError {}

pub fn verify(contract: &ContractDefinition) -> Result<VerificationReport, Vec<VerificationError>> {
    let mut verifier = Verifier::default();

    for model in &contract.models {
        let scope = scope_from_fields(&model.fields);
        verifier.verify_bounds(BoundOwner::Model(model.name.clone()), &model.bounds, &scope);
    }

    for state in &contract.states {
        verifier.verify_state(state);
    }

    for function in &contract.functions {
        verifier.verify_function(function);
    }

    if verifier.errors.is_empty() {
        Ok(verifier.report)
    } else {
        Err(verifier.errors)
    }
}

#[derive(Default)]
struct Verifier {
    report: VerificationReport,
    errors: Vec<VerificationError>,
}

impl Verifier {
    fn verify_state(&mut self, state: &StateDefinition) {
        let scope = scope_from_fields(&state.fields);
        self.verify_bounds(BoundOwner::State(state.name.clone()), &state.bounds, &scope);
    }

    fn verify_function(&mut self, function: &FunctionDefinition) {
        let scope = scope_from_params(&function.params);
        self.verify_bounds(
            BoundOwner::Function(function.name.clone()),
            &function.bounds,
            &scope,
        );
    }

    fn verify_bounds(
        &mut self,
        owner: BoundOwner,
        bounds: &Bounds,
        scope: &HashMap<String, VerifiedType>,
    ) {
        if !is_valid_span(bounds.span) {
            self.errors.push(VerificationError::new(
                "bound owner has an invalid span",
                bounds.span,
            ));
            return;
        }

        for expression in &bounds.expressions {
            let span = expression.span();
            if !is_valid_span(span) {
                self.errors.push(VerificationError::new(
                    "bound expression has an invalid span",
                    span,
                ));
                continue;
            }

            let Some(expression) = self.lower_expression(expression, scope) else {
                continue;
            };

            if expression.ty() != VerifiedType::Bool {
                self.errors.push(VerificationError::new(
                    "bound expression must lower to bool",
                    expression.span(),
                ));
                continue;
            }

            self.report.checked_bounds += 1;
            match &owner {
                BoundOwner::Model(_) => self.report.model_bounds += 1,
                BoundOwner::State(_) => self.report.state_bounds += 1,
                BoundOwner::Function(_) => self.report.function_bounds += 1,
            }
            self.report.constraints.push(Constraint {
                owner: owner.clone(),
                span,
                expression,
            });
        }
    }

    fn lower_expression(
        &mut self,
        expression: &ast::Expression,
        scope: &HashMap<String, VerifiedType>,
    ) -> Option<VerifiedExpression> {
        match expression {
            ast::Expression::Identifier(identifier) => {
                let Some(ty) = scope.get(&identifier.text) else {
                    self.errors.push(VerificationError::new(
                        format!(
                            "bound expression refers to unknown identifier {}",
                            identifier.text
                        ),
                        identifier.span,
                    ));
                    return None;
                };

                Some(VerifiedExpression::Identifier {
                    name: identifier.text.clone(),
                    ty: ty.clone(),
                    span: identifier.span,
                })
            }
            ast::Expression::Integer(integer) => Some(VerifiedExpression::IntLiteral {
                text: integer.text.clone(),
                span: integer.span,
            }),
            ast::Expression::Binary { lhs, op, rhs, span } => {
                let lhs = self.lower_expression(lhs, scope)?;
                let rhs = self.lower_expression(rhs, scope)?;
                let lhs_ty = lhs.ty();
                let rhs_ty = rhs.ty();
                let Some(ty) = self.lower_binary_type(*op, &lhs_ty, &rhs_ty, *span) else {
                    return None;
                };

                Some(VerifiedExpression::Binary {
                    lhs: Box::new(lhs),
                    op: (*op).into(),
                    rhs: Box::new(rhs),
                    ty,
                    span: *span,
                })
            }
        }
    }

    fn lower_binary_type(
        &mut self,
        op: ast::BinaryOperator,
        lhs_ty: &VerifiedType,
        rhs_ty: &VerifiedType,
        span: Span,
    ) -> Option<VerifiedType> {
        match op {
            ast::BinaryOperator::Add
            | ast::BinaryOperator::Subtract
            | ast::BinaryOperator::Multiply
            | ast::BinaryOperator::Divide
            | ast::BinaryOperator::Modulo => {
                if lhs_ty.is_numeric() && rhs_ty.is_numeric() {
                    Some(VerifiedType::merged_numeric(lhs_ty, rhs_ty))
                } else {
                    self.errors.push(VerificationError::new(
                        format!(
                            "operator {} expects numeric operands",
                            binary_operator_text(op)
                        ),
                        span,
                    ));
                    None
                }
            }
            ast::BinaryOperator::Greater
            | ast::BinaryOperator::GreaterEqual
            | ast::BinaryOperator::Less
            | ast::BinaryOperator::LessEqual => {
                if lhs_ty.is_numeric() && rhs_ty.is_numeric() {
                    Some(VerifiedType::Bool)
                } else {
                    self.errors.push(VerificationError::new(
                        format!(
                            "operator {} expects numeric operands",
                            binary_operator_text(op)
                        ),
                        span,
                    ));
                    None
                }
            }
            ast::BinaryOperator::Equal | ast::BinaryOperator::NotEqual => {
                if lhs_ty.is_compatible_with(rhs_ty) {
                    Some(VerifiedType::Bool)
                } else {
                    self.errors.push(VerificationError::new(
                        format!(
                            "operator {} expects compatible operands",
                            binary_operator_text(op)
                        ),
                        span,
                    ));
                    None
                }
            }
        }
    }
}

fn scope_from_fields(fields: &[FieldDefinition]) -> HashMap<String, VerifiedType> {
    fields
        .iter()
        .map(|field| (field.name.clone(), VerifiedType::from(&field.ty)))
        .collect()
}

fn scope_from_params(params: &[ParameterDefinition]) -> HashMap<String, VerifiedType> {
    params
        .iter()
        .map(|param| (param.name.clone(), VerifiedType::from(&param.ty)))
        .collect()
}

fn is_valid_span(span: Span) -> bool { span.start < span.end }

fn binary_operator_text(op: ast::BinaryOperator) -> &'static str {
    match op {
        ast::BinaryOperator::Equal => "==",
        ast::BinaryOperator::NotEqual => "!=",
        ast::BinaryOperator::Greater => ">",
        ast::BinaryOperator::GreaterEqual => ">=",
        ast::BinaryOperator::Less => "<",
        ast::BinaryOperator::LessEqual => "<=",
        ast::BinaryOperator::Add => "+",
        ast::BinaryOperator::Subtract => "-",
        ast::BinaryOperator::Multiply => "*",
        ast::BinaryOperator::Divide => "/",
        ast::BinaryOperator::Modulo => "%",
    }
}

impl Default for VerificationReport {
    fn default() -> Self {
        Self {
            checked_bounds: 0,
            model_bounds: 0,
            state_bounds: 0,
            function_bounds: 0,
            constraints: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use parser::Span;
    use parser::ast::Expression;
    use parser::ast::Identifier;
    use parser::ast::IntegerLiteral;
    use semantics::Bounds;
    use semantics::ContractDefinition;
    use semantics::ModelDefinition;

    use super::BoundOwner;
    use super::VerifiedBinaryOperator;
    use super::VerifiedExpression;
    use super::VerifiedType;
    use super::verify;

    #[test]
    fn lowers_model_state_and_function_bounds() {
        let source = parser::parse_source(
            r#"
model Counter {
    value: int
    must [ value >= 0 ]
}

state Ready(Counter) {
    must [ value >= 0 ]
}

fn increment(amount: int) when Ready -> Ready
must [ amount > 0 ]
{
    skip;
}
"#,
        )
        .expect("source should parse");
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let report = verify(&contract).expect("contract should verify structurally");

        assert_eq!(report.checked_bounds, 3);
        assert_eq!(report.model_bounds, 1);
        assert_eq!(report.state_bounds, 1);
        assert_eq!(report.function_bounds, 1);
        assert_has_identifier_constraint(
            &report,
            BoundOwner::Model("Counter".to_owned()),
            "value",
            VerifiedType::Int,
        );
        assert_has_identifier_constraint(
            &report,
            BoundOwner::State("Ready".to_owned()),
            "value",
            VerifiedType::Int,
        );
        assert_has_identifier_constraint(
            &report,
            BoundOwner::Function("increment".to_owned()),
            "amount",
            VerifiedType::Int,
        );
    }

    #[test]
    fn rejects_malformed_bound_expression_spans() {
        let contract = ContractDefinition {
            models: vec![ModelDefinition {
                name: "Broken".to_owned(),
                fields: Vec::new(),
                bounds: Bounds {
                    span: Span::new(1, 2),
                    expressions: vec![Expression::Integer(IntegerLiteral {
                        text: "0".to_owned(),
                        span: Span::new(3, 3),
                    })],
                },
                span: Span::new(1, 2),
            }],
            states: Vec::new(),
            functions: Vec::new(),
        };

        let errors = verify(&contract).expect_err("contract should fail structural verification");

        assert!(
            errors
                .iter()
                .any(|error| error.message == "bound expression has an invalid span")
        );
    }

    #[test]
    fn rejects_unknown_internal_identifiers() {
        let contract = ContractDefinition {
            models: vec![ModelDefinition {
                name: "Broken".to_owned(),
                fields: Vec::new(),
                bounds: Bounds {
                    span: Span::new(1, 2),
                    expressions: vec![Expression::Identifier(Identifier {
                        text: "missing".to_owned(),
                        span: Span::new(3, 10),
                    })],
                },
                span: Span::new(1, 2),
            }],
            states: Vec::new(),
            functions: Vec::new(),
        };

        let errors = verify(&contract).expect_err("contract should fail structural verification");

        assert!(
            errors
                .iter()
                .any(|error| error.message
                    == "bound expression refers to unknown identifier missing")
        );
    }

    fn assert_has_identifier_constraint(
        report: &super::VerificationReport,
        owner: BoundOwner,
        name: &str,
        ty: VerifiedType,
    ) {
        assert!(report.constraints.iter().any(|constraint| {
            constraint.owner == owner
                && matches!(
                    &constraint.expression,
                    VerifiedExpression::Binary {
                        lhs,
                        op: VerifiedBinaryOperator::GreaterEqual | VerifiedBinaryOperator::Greater,
                        ty: VerifiedType::Bool,
                        ..
                    } if matches!(
                        lhs.as_ref(),
                        VerifiedExpression::Identifier {
                            name: ident,
                            ty: ident_ty,
                            ..
                        } if ident == name && ident_ty == &ty
                    )
                )
        }));
    }
}
