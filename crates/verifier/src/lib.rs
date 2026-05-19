use std::fmt;

use parser::Span;
use semantics::Bounds;
use semantics::ContractDefinition;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub checked_bounds: usize,
    pub model_bounds: usize,
    pub state_bounds: usize,
    pub function_bounds: usize,
    pub results: Vec<BoundVerification>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundVerification {
    pub owner: BoundOwner,
    pub span: Span,
    pub outcome: VerificationOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundOwner {
    Model(String),
    State(String),
    Function(String),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum VerificationOutcome {
    StructuralOnly,
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
        verifier.verify_bounds(BoundOwner::Model(model.name.clone()), &model.bounds);
    }

    for state in &contract.states {
        verifier.verify_bounds(BoundOwner::State(state.name.clone()), &state.bounds);
    }

    for function in &contract.functions {
        verifier.verify_bounds(
            BoundOwner::Function(function.name.clone()),
            &function.bounds,
        );
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
    fn verify_bounds(&mut self, owner: BoundOwner, bounds: &Bounds) {
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

            self.report.checked_bounds += 1;
            match &owner {
                BoundOwner::Model(_) => self.report.model_bounds += 1,
                BoundOwner::State(_) => self.report.state_bounds += 1,
                BoundOwner::Function(_) => self.report.function_bounds += 1,
            }
            self.report.results.push(BoundVerification {
                owner: owner.clone(),
                span,
                outcome: VerificationOutcome::StructuralOnly,
            });
        }
    }
}

fn is_valid_span(span: Span) -> bool { span.start < span.end }

impl Default for VerificationReport {
    fn default() -> Self {
        Self {
            checked_bounds: 0,
            model_bounds: 0,
            state_bounds: 0,
            function_bounds: 0,
            results: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use parser::Span;
    use parser::ast::Expression;
    use parser::ast::IntegerLiteral;
    use semantics::Bounds;
    use semantics::ContractDefinition;
    use semantics::ModelDefinition;

    use super::BoundOwner;
    use super::VerificationOutcome;
    use super::verify;

    #[test]
    fn sees_model_state_and_function_bounds() {
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
        assert!(report.results.iter().any(|result| {
            result.owner == BoundOwner::Model("Counter".to_owned())
                && result.outcome == VerificationOutcome::StructuralOnly
        }));
        assert!(report.results.iter().any(|result| {
            result.owner == BoundOwner::State("Ready".to_owned())
                && result.outcome == VerificationOutcome::StructuralOnly
        }));
        assert!(report.results.iter().any(|result| {
            result.owner == BoundOwner::Function("increment".to_owned())
                && result.outcome == VerificationOutcome::StructuralOnly
        }));
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
}
