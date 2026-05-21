use std::collections::BTreeMap;
use std::collections::HashMap;
use std::fmt;

use parser::Span;
use parser::ast;
use semantics::Bounds;
use semantics::ContractDefinition;
use semantics::FieldDefinition;
use semantics::FunctionDefinition;
use semantics::ModelDefinition;
use semantics::ParameterDefinition;
use semantics::StateDefinition;
use semantics::StateTransitionDefinition;
use semantics::Type;
use z3::SatResult;
use z3::Solver;
use z3::ast::Bool;
use z3::ast::Int;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub checked_bounds: usize,
    pub model_bounds: usize,
    pub state_bounds: usize,
    pub function_bounds: usize,
    pub constraints: Vec<Constraint>,
    pub solver_results: Vec<ConstraintSolverResult>,
    pub transition_proofs: Vec<TransitionProof>,
    pub transition_results: Vec<TransitionProofResult>,
}

impl VerificationReport {
    pub fn accepted_constraints(&self) -> usize {
        self.solver_results
            .iter()
            .filter(|result| result.result == SolverResult::Accepted)
            .count()
    }

    pub fn rejected_constraints(&self) -> usize {
        self.solver_results
            .iter()
            .filter(|result| result.result == SolverResult::Rejected)
            .count()
    }

    pub fn unknown_constraints(&self) -> usize {
        self.solver_results
            .iter()
            .filter(|result| result.result == SolverResult::Unknown)
            .count()
    }

    pub fn unsupported_constraints(&self) -> usize {
        self.solver_results
            .iter()
            .filter(|result| matches!(result.result, SolverResult::Unsupported(_)))
            .count()
    }

    pub fn proved_transition_goals(&self) -> usize {
        self.transition_results
            .iter()
            .filter(|result| result.result == SolverResult::Accepted)
            .count()
    }

    pub fn failed_transition_goals(&self) -> usize {
        self.transition_results
            .iter()
            .filter(|result| result.result == SolverResult::Rejected)
            .count()
    }

    pub fn unknown_transition_goals(&self) -> usize {
        self.transition_results
            .iter()
            .filter(|result| result.result == SolverResult::Unknown)
            .count()
    }

    pub fn unsupported_transition_goals(&self) -> usize {
        self.transition_results
            .iter()
            .filter(|result| matches!(result.result, SolverResult::Unsupported(_)))
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint {
    pub owner: BoundOwner,
    pub expression: VerifiedExpression,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstraintSolverResult {
    pub owner: BoundOwner,
    pub span: Span,
    pub result: SolverResult,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionProof {
    pub function: String,
    pub from: String,
    pub to: String,
    pub assumptions: Vec<Constraint>,
    pub goal: Constraint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionProofResult {
    pub function: String,
    pub from: String,
    pub to: String,
    pub goal_owner: BoundOwner,
    pub span: Span,
    pub result: SolverResult,
    pub counterexample: Vec<CounterexampleValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterexampleValue {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionProofOutcome {
    pub result: SolverResult,
    pub counterexample: Vec<CounterexampleValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SolverResult {
    Accepted,
    Rejected,
    Unknown,
    Unsupported(String),
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

impl diagnostics::ToReport for VerificationError {
    fn to_report(&self) -> diagnostics::Report {
        diagnostics::Report::verification(self.span.into(), self.message.clone())
    }
}

pub trait SolverBackend {
    fn check_constraint(&self, constraint: &Constraint) -> SolverResult;
    fn prove_transition(&self, proof: &TransitionProof) -> TransitionProofOutcome;
}

#[derive(Debug, Default)]
pub struct StructuralBackend;

impl SolverBackend for StructuralBackend {
    fn check_constraint(&self, _constraint: &Constraint) -> SolverResult { SolverResult::Accepted }

    fn prove_transition(&self, _proof: &TransitionProof) -> TransitionProofOutcome {
        TransitionProofOutcome {
            result: SolverResult::Accepted,
            counterexample: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
pub struct Z3Backend;

impl SolverBackend for Z3Backend {
    fn check_constraint(&self, constraint: &Constraint) -> SolverResult {
        let Some(expression) = z3_bool(&constraint.expression) else {
            return SolverResult::Unsupported(
                "constraint is not supported by the Z3 backend yet".to_owned(),
            );
        };

        let solver = Solver::new();
        solver.assert(&expression);

        match solver.check() {
            SatResult::Sat => SolverResult::Accepted,
            SatResult::Unsat => SolverResult::Rejected,
            SatResult::Unknown => SolverResult::Unknown,
        }
    }

    fn prove_transition(&self, proof: &TransitionProof) -> TransitionProofOutcome {
        let solver = Solver::new();

        for assumption in &proof.assumptions {
            let Some(expression) = z3_bool(&assumption.expression) else {
                return TransitionProofOutcome {
                    result: SolverResult::Unsupported(
                        "transition assumption is not supported by the Z3 backend yet".to_owned(),
                    ),
                    counterexample: Vec::new(),
                };
            };

            solver.assert(&expression);
        }

        let Some(goal) = z3_bool(&proof.goal.expression) else {
            return TransitionProofOutcome {
                result: SolverResult::Unsupported(
                    "transition goal is not supported by the Z3 backend yet".to_owned(),
                ),
                counterexample: Vec::new(),
            };
        };

        solver.assert(&goal.not());

        match solver.check() {
            SatResult::Sat => TransitionProofOutcome {
                result: SolverResult::Rejected,
                counterexample: z3_counterexample(&solver, proof),
            },
            SatResult::Unsat => TransitionProofOutcome {
                result: SolverResult::Accepted,
                counterexample: Vec::new(),
            },
            SatResult::Unknown => TransitionProofOutcome {
                result: SolverResult::Unknown,
                counterexample: Vec::new(),
            },
        }
    }
}

pub fn verify(contract: &ContractDefinition) -> Result<VerificationReport, Vec<VerificationError>> {
    verify_with_backend(contract, &Z3Backend)
}

pub fn verify_with_backend(
    contract: &ContractDefinition,
    backend: &impl SolverBackend,
) -> Result<VerificationReport, Vec<VerificationError>> {
    let mut verifier = Verifier::default();
    let models = models_by_name(&contract.models);
    let states = states_by_name(&contract.states);

    for model in &contract.models {
        let scope = scope_from_fields(&model.fields);
        verifier.verify_bounds(BoundOwner::Model(model.name.clone()), &model.bounds, &scope);
    }

    for state in &contract.states {
        verifier.verify_state(state);
    }

    let state_fields = state_fields_by_name(&contract.states);

    for function in &contract.functions {
        verifier.verify_function(function, &state_fields);
        verifier.verify_transition_proofs(function, &models, &states, &state_fields);
    }

    if verifier.errors.is_empty() {
        let mut report = verifier.report;
        report.solver_results = report
            .constraints
            .iter()
            .map(|constraint| ConstraintSolverResult {
                owner: constraint.owner.clone(),
                span: constraint.span,
                result: backend.check_constraint(constraint),
            })
            .collect();
        report.transition_results = report
            .transition_proofs
            .iter()
            .map(|proof| {
                let outcome = backend.prove_transition(proof);

                TransitionProofResult {
                    function: proof.function.clone(),
                    from: proof.from.clone(),
                    to: proof.to.clone(),
                    goal_owner: proof.goal.owner.clone(),
                    span: proof.goal.span,
                    result: outcome.result,
                    counterexample: outcome.counterexample,
                }
            })
            .collect();

        Ok(report)
    } else {
        Err(verifier.errors)
    }
}

#[derive(Default)]
struct Verifier {
    report: VerificationReport,
    errors: Vec<VerificationError>,
}

#[derive(Debug, Clone, Default)]
struct VerifiedScope {
    values: HashMap<String, VerifiedValue>,
    fields: HashMap<String, HashMap<String, VerifiedType>>,
}

impl VerifiedScope {
    fn from_values(values: HashMap<String, VerifiedValue>) -> Self {
        Self {
            values,
            fields: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VerifiedValue {
    symbol: String,
    ty: VerifiedType,
}

impl Verifier {
    fn verify_state(&mut self, state: &StateDefinition) {
        let scope = scope_from_fields(&state.fields);
        self.verify_bounds(BoundOwner::State(state.name.clone()), &state.bounds, &scope);
    }

    fn verify_function(
        &mut self,
        function: &FunctionDefinition,
        state_fields: &HashMap<String, HashMap<String, VerifiedType>>,
    ) {
        let scope = scope_from_params(&function.params);
        let scope =
            scope_with_transition_aliases(scope, function.transition.as_ref(), state_fields);
        self.verify_bounds(
            BoundOwner::Function(function.name.clone()),
            &function.bounds,
            &scope,
        );
    }

    fn verify_transition_proofs(
        &mut self,
        function: &FunctionDefinition,
        models: &HashMap<String, &ModelDefinition>,
        states: &HashMap<String, &StateDefinition>,
        state_fields: &HashMap<String, HashMap<String, VerifiedType>>,
    ) {
        let Some(transition) = &function.transition else {
            return;
        };

        let (Some(from_alias), Some(to_alias)) = (
            transition.from_alias.as_deref(),
            transition.to_alias.as_deref(),
        ) else {
            return;
        };

        let Some(from_state) = states.get(&transition.from).copied() else {
            return;
        };

        let Some(to_state) = states.get(&transition.to).copied() else {
            return;
        };

        let mut assumptions = Vec::new();
        self.add_state_assumptions(from_alias, from_state, models, &mut assumptions);

        let function_scope = scope_with_transition_aliases(
            scope_from_params(&function.params),
            Some(transition),
            state_fields,
        );

        assumptions.extend(self.lower_constraints(
            BoundOwner::Function(function.name.clone()),
            &function.bounds,
            &function_scope,
        ));

        let goals = self.transition_goals(to_alias, to_state, models);
        for goal in goals {
            self.report.transition_proofs.push(TransitionProof {
                function: function.name.clone(),
                from: transition.from.clone(),
                to: transition.to.clone(),
                assumptions: assumptions.clone(),
                goal,
            });
        }
    }

    fn add_state_assumptions(
        &mut self,
        alias: &str,
        state: &StateDefinition,
        models: &HashMap<String, &ModelDefinition>,
        assumptions: &mut Vec<Constraint>,
    ) {
        if let Some(model_name) = &state.model {
            if let Some(model) = models.get(model_name).copied() {
                let scope = scope_from_fields_with_prefix(alias, &model.fields);
                assumptions.extend(self.lower_constraints(
                    BoundOwner::Model(model.name.clone()),
                    &model.bounds,
                    &scope,
                ));
            }
        }

        let scope = scope_from_fields_with_prefix(alias, &state.fields);
        assumptions.extend(self.lower_constraints(
            BoundOwner::State(state.name.clone()),
            &state.bounds,
            &scope,
        ));
    }

    fn transition_goals(
        &mut self,
        alias: &str,
        state: &StateDefinition,
        models: &HashMap<String, &ModelDefinition>,
    ) -> Vec<Constraint> {
        let mut goals = Vec::new();
        if let Some(model_name) = &state.model {
            if let Some(model) = models.get(model_name).copied() {
                let scope = scope_from_fields_with_prefix(alias, &model.fields);
                goals.extend(self.lower_constraints(
                    BoundOwner::Model(model.name.clone()),
                    &model.bounds,
                    &scope,
                ));
            }
        }

        let scope = scope_from_fields_with_prefix(alias, &state.fields);
        goals.extend(self.lower_constraints(
            BoundOwner::State(state.name.clone()),
            &state.bounds,
            &scope,
        ));

        goals
    }

    fn verify_bounds(&mut self, owner: BoundOwner, bounds: &Bounds, scope: &VerifiedScope) {
        let constraints = self.lower_constraints(owner.clone(), bounds, scope);
        for constraint in constraints {
            self.report.checked_bounds += 1;
            match &owner {
                BoundOwner::Model(_) => self.report.model_bounds += 1,
                BoundOwner::State(_) => self.report.state_bounds += 1,
                BoundOwner::Function(_) => self.report.function_bounds += 1,
            }
            self.report.constraints.push(constraint);
        }
    }

    fn lower_constraints(
        &mut self,
        owner: BoundOwner,
        bounds: &Bounds,
        scope: &VerifiedScope,
    ) -> Vec<Constraint> {
        let mut constraints = Vec::new();

        if !is_valid_span(bounds.span) {
            self.errors.push(VerificationError::new(
                "bound owner has an invalid span",
                bounds.span,
            ));
            return constraints;
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

            constraints.push(Constraint {
                owner: owner.clone(),
                span,
                expression,
            });
        }

        constraints
    }

    fn lower_expression(
        &mut self,
        expression: &ast::Expression,
        scope: &VerifiedScope,
    ) -> Option<VerifiedExpression> {
        match expression {
            ast::Expression::Identifier(identifier) => {
                let Some(value) = scope.values.get(&identifier.text) else {
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
                    name: value.symbol.clone(),
                    ty: value.ty.clone(),
                    span: identifier.span,
                })
            }
            ast::Expression::FieldAccess { base, field, span } => {
                let Some(fields) = scope.fields.get(&base.text) else {
                    self.errors.push(VerificationError::new(
                        format!(
                            "bound expression refers to unknown state alias {}",
                            base.text
                        ),
                        base.span,
                    ));
                    return None;
                };

                let Some(ty) = fields.get(&field.text) else {
                    self.errors.push(VerificationError::new(
                        format!(
                            "bound expression refers to unknown field {}.{}",
                            base.text, field.text
                        ),
                        field.span,
                    ));
                    return None;
                };

                Some(VerifiedExpression::Identifier {
                    name: format!("{}.{}", base.text, field.text),
                    ty: ty.clone(),
                    span: *span,
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

fn z3_bool(expression: &VerifiedExpression) -> Option<Bool> {
    match expression {
        VerifiedExpression::Binary { lhs, op, rhs, .. } => match op {
            VerifiedBinaryOperator::Equal => Some(z3_int(lhs)?.eq(&z3_int(rhs)?)),
            VerifiedBinaryOperator::NotEqual => Some(z3_int(lhs)?.eq(&z3_int(rhs)?).not()),
            VerifiedBinaryOperator::Greater => Some(z3_int(lhs)?.gt(&z3_int(rhs)?)),
            VerifiedBinaryOperator::GreaterEqual => Some(z3_int(lhs)?.ge(&z3_int(rhs)?)),
            VerifiedBinaryOperator::Less => Some(z3_int(lhs)?.lt(&z3_int(rhs)?)),
            VerifiedBinaryOperator::LessEqual => Some(z3_int(lhs)?.le(&z3_int(rhs)?)),
            VerifiedBinaryOperator::Add
            | VerifiedBinaryOperator::Subtract
            | VerifiedBinaryOperator::Multiply
            | VerifiedBinaryOperator::Divide
            | VerifiedBinaryOperator::Modulo => None,
        },
        VerifiedExpression::Identifier {
            name,
            ty: VerifiedType::Bool,
            ..
        } => Some(Bool::new_const(name.as_str())),
        VerifiedExpression::IntLiteral { .. } | VerifiedExpression::Identifier { .. } => None,
    }
}

fn z3_int(expression: &VerifiedExpression) -> Option<Int> {
    match expression {
        VerifiedExpression::IntLiteral { text, .. } => text.parse::<i64>().ok().map(Int::from_i64),
        VerifiedExpression::Identifier { name, ty, .. } if ty.is_numeric() => {
            Some(Int::new_const(name.as_str()))
        }
        VerifiedExpression::Binary { lhs, op, rhs, .. } => {
            let lhs = z3_int(lhs)?;
            let rhs = z3_int(rhs)?;

            match op {
                VerifiedBinaryOperator::Add => Some(Int::add(&[&lhs, &rhs])),
                VerifiedBinaryOperator::Subtract => Some(Int::sub(&[&lhs, &rhs])),
                VerifiedBinaryOperator::Multiply => Some(Int::mul(&[&lhs, &rhs])),
                VerifiedBinaryOperator::Divide => Some(lhs.div(&rhs)),
                VerifiedBinaryOperator::Modulo => Some(lhs.modulo(&rhs)),
                VerifiedBinaryOperator::Equal
                | VerifiedBinaryOperator::NotEqual
                | VerifiedBinaryOperator::Greater
                | VerifiedBinaryOperator::GreaterEqual
                | VerifiedBinaryOperator::Less
                | VerifiedBinaryOperator::LessEqual => None,
            }
        }
        VerifiedExpression::Identifier { .. } => None,
    }
}

fn z3_counterexample(solver: &Solver, proof: &TransitionProof) -> Vec<CounterexampleValue> {
    let Some(model) = solver.get_model() else {
        return Vec::new();
    };

    collect_proof_symbols(proof)
        .into_iter()
        .filter_map(|(name, ty)| {
            let value = match ty {
                VerifiedType::Int | VerifiedType::UInt | VerifiedType::IntegerLiteral => {
                    model.eval(&Int::new_const(name.as_str()), true).map(|value| value.to_string())
                }
                VerifiedType::Bool => {
                    model.eval(&Bool::new_const(name.as_str()), true).map(|value| value.to_string())
                }
                VerifiedType::String
                | VerifiedType::Address
                | VerifiedType::Hex
                | VerifiedType::Custom(_) => None,
            }?;

            Some(CounterexampleValue { name, value })
        })
        .collect()
}

fn collect_proof_symbols(proof: &TransitionProof) -> BTreeMap<String, VerifiedType> {
    let mut symbols = BTreeMap::new();
    for assumption in &proof.assumptions {
        collect_expression_symbols(&assumption.expression, &mut symbols);
    }

    collect_expression_symbols(&proof.goal.expression, &mut symbols);

    symbols
}

fn collect_expression_symbols(
    expression: &VerifiedExpression,
    symbols: &mut BTreeMap<String, VerifiedType>,
) {
    match expression {
        VerifiedExpression::Identifier { name, ty, .. } => {
            symbols.entry(name.clone()).or_insert_with(|| ty.clone());
        }
        VerifiedExpression::Binary { lhs, rhs, .. } => {
            collect_expression_symbols(lhs, symbols);
            collect_expression_symbols(rhs, symbols);
        }
        VerifiedExpression::IntLiteral { .. } => {}
    }
}

fn scope_from_fields(fields: &[FieldDefinition]) -> VerifiedScope {
    VerifiedScope::from_values(
        fields
            .iter()
            .map(|field| {
                (
                    field.name.clone(),
                    VerifiedValue {
                        symbol: field.name.clone(),
                        ty: VerifiedType::from(&field.ty),
                    },
                )
            })
            .collect(),
    )
}

fn scope_from_fields_with_prefix(prefix: &str, fields: &[FieldDefinition]) -> VerifiedScope {
    VerifiedScope::from_values(
        fields
            .iter()
            .map(|field| {
                (
                    field.name.clone(),
                    VerifiedValue {
                        symbol: format!("{}.{}", prefix, field.name),
                        ty: VerifiedType::from(&field.ty),
                    },
                )
            })
            .collect(),
    )
}

fn scope_from_params(params: &[ParameterDefinition]) -> VerifiedScope {
    VerifiedScope::from_values(
        params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    VerifiedValue {
                        symbol: param.name.clone(),
                        ty: VerifiedType::from(&param.ty),
                    },
                )
            })
            .collect(),
    )
}

fn scope_with_transition_aliases(
    mut scope: VerifiedScope,
    transition: Option<&StateTransitionDefinition>,
    state_fields: &HashMap<String, HashMap<String, VerifiedType>>,
) -> VerifiedScope {
    let Some(transition) = transition else {
        return scope;
    };

    add_transition_alias(
        &mut scope,
        transition.from_alias.as_deref(),
        &transition.from,
        state_fields,
    );
    add_transition_alias(
        &mut scope,
        transition.to_alias.as_deref(),
        &transition.to,
        state_fields,
    );

    scope
}

fn add_transition_alias(
    scope: &mut VerifiedScope,
    alias: Option<&str>,
    state: &str,
    state_fields: &HashMap<String, HashMap<String, VerifiedType>>,
) {
    let Some(alias) = alias else {
        return;
    };

    if let Some(fields) = state_fields.get(state) {
        scope.fields.insert(alias.to_owned(), fields.clone());
    }
}

fn state_fields_by_name(
    states: &[StateDefinition],
) -> HashMap<String, HashMap<String, VerifiedType>> {
    states
        .iter()
        .map(|state| (state.name.clone(), field_scope_from_fields(&state.fields)))
        .collect()
}

fn field_scope_from_fields(fields: &[FieldDefinition]) -> HashMap<String, VerifiedType> {
    fields
        .iter()
        .map(|field| (field.name.clone(), VerifiedType::from(&field.ty)))
        .collect()
}

fn models_by_name(models: &[ModelDefinition]) -> HashMap<String, &ModelDefinition> {
    models.iter().map(|model| (model.name.clone(), model)).collect()
}

fn states_by_name(states: &[StateDefinition]) -> HashMap<String, &StateDefinition> {
    states.iter().map(|state| (state.name.clone(), state)).collect()
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
            solver_results: Vec::new(),
            transition_proofs: Vec::new(),
            transition_results: Vec::new(),
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
    use super::SolverResult;
    use super::StructuralBackend;
    use super::VerifiedBinaryOperator;
    use super::VerifiedExpression;
    use super::VerifiedType;
    use super::verify;
    use super::verify_with_backend;

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
        assert_eq!(report.accepted_constraints(), 3);
        assert_eq!(report.rejected_constraints(), 0);
        assert_eq!(report.unknown_constraints(), 0);
        assert_eq!(report.unsupported_constraints(), 0);
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
    fn structural_backend_accepts_lowered_constraints() {
        let source = parser::parse_source(
            r#"
model Counter {
    value: int
    must [ value >= 0 ]
}
"#,
        )
        .expect("source should parse");
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let report = verify_with_backend(&contract, &StructuralBackend)
            .expect("contract should verify structurally");

        assert_eq!(report.checked_bounds, 1);
        assert_eq!(report.solver_results.len(), 1);
        assert_eq!(report.solver_results[0].result, SolverResult::Accepted);
    }

    #[test]
    fn lowers_transition_alias_fields_to_solver_symbols() {
        let source = parser::parse_source(
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
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let report = verify(&contract).expect("contract should lower for solving");

        assert_eq!(report.checked_bounds, 1);
        assert_eq!(report.accepted_constraints(), 1);
        assert!(expression_has_identifier(
            &report.constraints[0].expression,
            "after.value"
        ));
        assert!(expression_has_identifier(
            &report.constraints[0].expression,
            "before.value"
        ));
        assert_eq!(report.transition_proofs.len(), 0);
        assert_eq!(report.transition_results.len(), 0);
    }

    #[test]
    fn z3_backend_proves_transition_target_invariants() {
        let source = parser::parse_source(
            r#"
model Counter {
    value: int
    must [ value >= 0 ]
}

state Ready(Counter) {
    must [ value >= 0 ]
}

fn increment(amount: int) when Ready as before -> Ready as after
must [
    amount > 0,
    after.value == before.value + amount
]
{
    skip;
}
"#,
        )
        .expect("source should parse");
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let report = verify(&contract).expect("contract should lower for solving");

        assert_eq!(report.checked_bounds, 4);
        assert_eq!(report.accepted_constraints(), 4);
        assert_eq!(report.transition_proofs.len(), 2);
        assert_eq!(report.proved_transition_goals(), 2);
        assert_eq!(report.failed_transition_goals(), 0);
        assert_eq!(report.unsupported_transition_goals(), 0);
    }

    #[test]
    fn z3_backend_rejects_unproved_transition_target_invariants() {
        let source = parser::parse_source(
            r#"
model Counter {
    value: int
    must [ value >= 0 ]
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
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let report = verify(&contract).expect("contract should lower for solving");

        assert_eq!(report.checked_bounds, 2);
        assert_eq!(report.accepted_constraints(), 2);
        assert_eq!(report.transition_proofs.len(), 1);
        assert_eq!(report.proved_transition_goals(), 0);
        assert_eq!(report.failed_transition_goals(), 1);
        assert_eq!(report.transition_results[0].result, SolverResult::Rejected);
        assert!(counterexample_value(&report.transition_results[0], "amount").is_some());
        assert!(counterexample_value(&report.transition_results[0], "before.value").is_some());
        assert!(counterexample_value(&report.transition_results[0], "after.value").is_some());
    }

    #[test]
    fn z3_backend_rejects_unsatisfiable_integer_constraints() {
        let source = parser::parse_source(
            r#"
model Broken {
    must [ 0 > 1 ]
}
"#,
        )
        .expect("source should parse");
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let report = verify(&contract).expect("contract should lower for solving");

        assert_eq!(report.checked_bounds, 1);
        assert_eq!(report.accepted_constraints(), 0);
        assert_eq!(report.rejected_constraints(), 1);
        assert_eq!(report.solver_results[0].result, SolverResult::Rejected);
    }

    #[test]
    fn z3_backend_accepts_integer_arithmetic_constraints() {
        let source = parser::parse_source(
            r#"
model Counter {
    value: int
    must [
        value + 1 > 0,
        value - 1 < 10,
        value * 2 >= 0,
        value / 2 == 3,
        value % 2 == 0
    ]
}
"#,
        )
        .expect("source should parse");
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let report = verify(&contract).expect("contract should lower for solving");

        assert_eq!(report.checked_bounds, 5);
        assert_eq!(report.accepted_constraints(), 5);
        assert_eq!(report.rejected_constraints(), 0);
        assert_eq!(report.unsupported_constraints(), 0);
    }

    #[test]
    fn z3_backend_rejects_unsatisfiable_arithmetic_constraints() {
        let source = parser::parse_source(
            r#"
model Broken {
    must [ 1 + 1 == 3 ]
}
"#,
        )
        .expect("source should parse");
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let report = verify(&contract).expect("contract should lower for solving");

        assert_eq!(report.checked_bounds, 1);
        assert_eq!(report.accepted_constraints(), 0);
        assert_eq!(report.rejected_constraints(), 1);
        assert_eq!(report.unsupported_constraints(), 0);
    }

    #[test]
    fn z3_backend_rejects_unsatisfiable_transition_field_constraints() {
        let source = parser::parse_source(
            r#"
model Counter {
    value: int
}

state Ready(Counter) {}

fn increment() when Ready as before -> Ready as after
must [ before.value + 1 == before.value ]
{
    skip;
}
"#,
        )
        .expect("source should parse");
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let report = verify(&contract).expect("contract should lower for solving");

        assert_eq!(report.checked_bounds, 1);
        assert_eq!(report.accepted_constraints(), 0);
        assert_eq!(report.rejected_constraints(), 1);
        assert_eq!(report.unsupported_constraints(), 0);
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

    fn expression_has_identifier(expression: &VerifiedExpression, name: &str) -> bool {
        match expression {
            VerifiedExpression::Identifier { name: ident, .. } => ident == name,
            VerifiedExpression::Binary { lhs, rhs, .. } => {
                expression_has_identifier(lhs, name) || expression_has_identifier(rhs, name)
            }
            VerifiedExpression::IntLiteral { .. } => false,
        }
    }

    fn counterexample_value<'a>(
        result: &'a super::TransitionProofResult,
        name: &str,
    ) -> Option<&'a str> {
        result
            .counterexample
            .iter()
            .find(|value| value.name == name)
            .map(|value| value.value.as_str())
    }
}
