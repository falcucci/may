use std::fmt;

use parser::Span;
use parser::ast::BinaryOperator;
use parser::ast::Expression;
use semantics::ContractDefinition;
use semantics::FunctionDefinition;
use semantics::StateDefinition;
use semantics::Statement;
use semantics::Type;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlgorandArtifacts {
    pub approval_teal: String,
    pub clear_teal: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlgorandError {
    pub message: String,
    pub span: diagnostics::Span,
}

impl AlgorandError {
    fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span: span.into(),
        }
    }
}

impl fmt::Display for AlgorandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { write!(f, "{}", self.message) }
}

impl std::error::Error for AlgorandError {}

impl diagnostics::ToReport for AlgorandError {
    fn to_report(&self) -> diagnostics::Report {
        diagnostics::Report::emission(self.span, self.message.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TealInstruction {
    PragmaVersion(u64),
    Int(u64),
    ByteString(String),
    TxnaApplicationArgs(usize),
    Btoi,
    AppGlobalGet,
    AppGlobalPut,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Return,
}

impl fmt::Display for TealInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TealInstruction::PragmaVersion(version) => write!(f, "#pragma version {version}"),
            TealInstruction::Int(value) => write!(f, "int {value}"),
            TealInstruction::ByteString(value) => {
                write!(f, "byte \"{}\"", escape_teal_string(value))
            }
            TealInstruction::TxnaApplicationArgs(index) => {
                write!(f, "txna ApplicationArgs {index}")
            }
            TealInstruction::Btoi => write!(f, "btoi"),
            TealInstruction::AppGlobalGet => write!(f, "app_global_get"),
            TealInstruction::AppGlobalPut => write!(f, "app_global_put"),
            TealInstruction::Add => write!(f, "+"),
            TealInstruction::Subtract => write!(f, "-"),
            TealInstruction::Multiply => write!(f, "*"),
            TealInstruction::Divide => write!(f, "/"),
            TealInstruction::Modulo => write!(f, "%"),
            TealInstruction::Return => write!(f, "return"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TealProgram {
    instructions: Vec<TealInstruction>,
}

impl TealProgram {
    pub fn new(instructions: Vec<TealInstruction>) -> Self { Self { instructions } }

    pub fn always_approve() -> Self {
        Self::new(vec![
            TealInstruction::PragmaVersion(10),
            TealInstruction::Int(1),
            TealInstruction::Return,
        ])
    }

    pub fn render(&self) -> String {
        let mut output =
            self.instructions.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        output.push('\n');
        output
    }
}

#[derive(Debug, Default)]
pub struct AlgorandEmitter;

impl AlgorandEmitter {
    pub fn emit(
        &self,
        contract: &ContractDefinition,
    ) -> Result<AlgorandArtifacts, Vec<AlgorandError>> {
        let approval = match contract.functions.as_slice() {
            [] => TealProgram::always_approve(),
            [function] => self.emit_function(contract, function)?,
            [_, function, ..] => {
                return Err(vec![AlgorandError::new(
                    "Algorand emission supports one function for now",
                    function.span,
                )]);
            }
        };

        Ok(AlgorandArtifacts {
            approval_teal: approval.render(),
            clear_teal: TealProgram::always_approve().render(),
        })
    }

    fn emit_function(
        &self,
        contract: &ContractDefinition,
        function: &FunctionDefinition,
    ) -> Result<TealProgram, Vec<AlgorandError>> {
        let context = CompileContext::new(contract, function)?;
        context.emit_approval()
    }
}

pub fn emit(contract: &ContractDefinition) -> Result<AlgorandArtifacts, Vec<AlgorandError>> {
    AlgorandEmitter.emit(contract)
}

struct CompileContext<'a> {
    function: &'a FunctionDefinition,
    from_alias: &'a str,
    to_alias: &'a str,
    from_state: &'a StateDefinition,
    to_state: &'a StateDefinition,
}

impl<'a> CompileContext<'a> {
    fn new(
        contract: &'a ContractDefinition,
        function: &'a FunctionDefinition,
    ) -> Result<Self, Vec<AlgorandError>> {
        let mut errors = Vec::new();

        let Some(transition) = &function.transition else {
            return Err(vec![AlgorandError::new(
                "Algorand emission requires a state transition",
                function.span,
            )]);
        };

        let from_state = contract.states.iter().find(|state| state.name == transition.from);
        let to_state = contract.states.iter().find(|state| state.name == transition.to);

        let Some(from_state) = from_state else {
            return Err(vec![AlgorandError::new(
                format!("Algorand emission could not find state {}", transition.from),
                transition.span,
            )]);
        };

        let Some(to_state) = to_state else {
            return Err(vec![AlgorandError::new(
                format!("Algorand emission could not find state {}", transition.to),
                transition.span,
            )]);
        };

        let from_alias = match transition.from_alias.as_deref() {
            Some(alias) => alias,
            None => {
                errors.push(AlgorandError::new(
                    "Algorand emission requires an input state alias",
                    transition.span,
                ));
                ""
            }
        };

        let to_alias = match transition.to_alias.as_deref() {
            Some(alias) => alias,
            None => {
                errors.push(AlgorandError::new(
                    "Algorand emission requires an output state alias",
                    transition.span,
                ));
                ""
            }
        };

        for param in &function.params {
            if param.ty != Type::Int {
                errors.push(AlgorandError::new(
                    format!(
                        "Algorand emission only supports int parameters, got {}",
                        type_name(&param.ty)
                    ),
                    param.span,
                ));
            }
        }

        for field in &to_state.fields {
            if field.ty != Type::Int {
                errors.push(AlgorandError::new(
                    format!(
                        "Algorand global state only supports int field {} for now",
                        field.name
                    ),
                    field.span,
                ));
            }
        }

        if errors.is_empty() {
            Ok(Self {
                function,
                from_alias,
                to_alias,
                from_state,
                to_state,
            })
        } else {
            Err(errors)
        }
    }

    fn emit_approval(&self) -> Result<TealProgram, Vec<AlgorandError>> {
        let mut instructions = vec![TealInstruction::PragmaVersion(10)];
        let mut errors = Vec::new();
        let mut assigned_fields = Vec::<String>::new();

        for statement in &self.function.body {
            match statement {
                Statement::Skip { .. } => {}
                Statement::Assignment(assignment) => {
                    if assigned_fields.iter().any(|field| field == &assignment.target.field) {
                        errors.push(AlgorandError::new(
                            format!(
                                "Algorand emission does not support multiple writes to {}.{}",
                                assignment.target.alias, assignment.target.field
                            ),
                            assignment.target.span,
                        ));
                        continue;
                    }

                    if assignment.target.alias != self.to_alias {
                        errors.push(AlgorandError::new(
                            format!(
                                "Algorand emission can only write output alias {}",
                                self.to_alias
                            ),
                            assignment.target.span,
                        ));
                        continue;
                    }

                    if self.field_type(self.to_state, &assignment.target.field).is_none() {
                        errors.push(AlgorandError::new(
                            format!(
                                "Algorand emission could not find output field {}.{}",
                                assignment.target.alias, assignment.target.field
                            ),
                            assignment.target.span,
                        ));
                        continue;
                    }

                    assigned_fields.push(assignment.target.field.clone());
                    instructions.push(TealInstruction::ByteString(assignment.target.field.clone()));
                    if let Err(error) = self.emit_expression(&assignment.value, &mut instructions) {
                        errors.push(error);
                    }
                    instructions.push(TealInstruction::AppGlobalPut);
                }
            }
        }

        if assigned_fields.is_empty() {
            errors.push(AlgorandError::new(
                "Algorand emission requires one assignment body for now",
                self.function.span,
            ));
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        instructions.push(TealInstruction::Int(1));
        instructions.push(TealInstruction::Return);

        Ok(TealProgram::new(instructions))
    }

    fn emit_expression(
        &self,
        expression: &Expression,
        instructions: &mut Vec<TealInstruction>,
    ) -> Result<(), AlgorandError> {
        match expression {
            Expression::Identifier(identifier) => {
                let Some(index) =
                    self.function.params.iter().position(|param| param.name == identifier.text)
                else {
                    return Err(AlgorandError::new(
                        format!(
                            "Algorand emission could not lower identifier {}",
                            identifier.text
                        ),
                        identifier.span,
                    ));
                };

                instructions.push(TealInstruction::TxnaApplicationArgs(index));
                instructions.push(TealInstruction::Btoi);

                Ok(())
            }
            Expression::FieldAccess { base, field, span } => {
                if base.text != self.from_alias {
                    return Err(AlgorandError::new(
                        format!(
                            "Algorand emission can only read input alias {}",
                            self.from_alias
                        ),
                        base.span,
                    ));
                }

                if self.field_type(self.from_state, &field.text).is_none() {
                    return Err(AlgorandError::new(
                        format!(
                            "Algorand emission could not find input field {}.{}",
                            base.text, field.text
                        ),
                        *span,
                    ));
                }

                instructions.push(TealInstruction::ByteString(field.text.clone()));
                instructions.push(TealInstruction::AppGlobalGet);

                Ok(())
            }
            Expression::Integer(integer) => {
                let value = integer.text.parse::<u64>().map_err(|_| {
                    AlgorandError::new(
                        format!(
                            "Algorand emission could not lower integer literal {}",
                            integer.text
                        ),
                        integer.span,
                    )
                })?;

                instructions.push(TealInstruction::Int(value));

                Ok(())
            }
            Expression::Binary { lhs, op, rhs, span } => {
                self.emit_expression(lhs, instructions)?;
                self.emit_expression(rhs, instructions)?;

                match op {
                    BinaryOperator::Add => instructions.push(TealInstruction::Add),
                    BinaryOperator::Subtract => instructions.push(TealInstruction::Subtract),
                    BinaryOperator::Multiply => instructions.push(TealInstruction::Multiply),
                    BinaryOperator::Divide => instructions.push(TealInstruction::Divide),
                    BinaryOperator::Modulo => instructions.push(TealInstruction::Modulo),
                    BinaryOperator::Equal
                    | BinaryOperator::NotEqual
                    | BinaryOperator::Greater
                    | BinaryOperator::GreaterEqual
                    | BinaryOperator::Less
                    | BinaryOperator::LessEqual => {
                        return Err(AlgorandError::new(
                            format!(
                                "Algorand emission cannot lower {} in assignment values yet",
                                binary_operator_text(*op)
                            ),
                            *span,
                        ));
                    }
                }

                Ok(())
            }
        }
    }

    fn field_type<'b>(&self, state: &'b StateDefinition, field: &str) -> Option<&'b Type> {
        state
            .fields
            .iter()
            .find(|candidate| candidate.name == field)
            .map(|field| &field.ty)
    }
}

fn escape_teal_string(value: &str) -> String {
    value.chars().fold(String::new(), |mut escaped, ch| {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch => escaped.push(ch),
        }
        escaped
    })
}

fn binary_operator_text(op: BinaryOperator) -> &'static str {
    match op {
        BinaryOperator::Equal => "==",
        BinaryOperator::NotEqual => "!=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Modulo => "%",
    }
}

fn type_name(ty: &Type) -> String {
    match ty {
        Type::Int => "int".to_owned(),
        Type::UInt => "uint".to_owned(),
        Type::Bool => "bool".to_owned(),
        Type::String => "string".to_owned(),
        Type::Address => "address".to_owned(),
        Type::Hex => "hex".to_owned(),
        Type::Custom(name) => name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::TealProgram;
    use super::emit;

    #[test]
    fn renders_minimal_teal_program() {
        assert_eq!(
            TealProgram::always_approve().render(),
            "#pragma version 10\nint 1\nreturn\n"
        );
    }

    #[test]
    fn emits_approval_and_clear_artifacts() {
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

        let artifacts = emit(&contract).expect("contract should emit");

        assert_eq!(
            artifacts.approval_teal,
            "#pragma version 10\nint 1\nreturn\n"
        );
        assert_eq!(artifacts.clear_teal, "#pragma version 10\nint 1\nreturn\n");
    }

    #[test]
    fn emits_single_assignment_body() {
        let source = parser::parse_source(
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
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let artifacts = emit(&contract).expect("assignment body should emit");

        assert_eq!(
            artifacts.approval_teal,
            "#pragma version 10\nbyte \"value\"\nbyte \"value\"\napp_global_get\ntxna \
             ApplicationArgs 0\nbtoi\n+\napp_global_put\nint 1\nreturn\n"
        );
        assert_eq!(artifacts.clear_teal, "#pragma version 10\nint 1\nreturn\n");
    }

    #[test]
    fn rejects_multiple_functions_until_dispatch_exists() {
        let source = parser::parse_source(
            r#"
model Counter {
    value: int
}

state Ready(Counter) {}

fn increment(amount: int) when Ready as before -> Ready as after
{
    after.value = before.value + amount;
}

fn decrement(amount: int) when Ready as before -> Ready as after
{
    after.value = before.value - amount;
}
"#,
        )
        .expect("source should parse");
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let errors = emit(&contract).expect_err("multiple functions should not emit yet");

        assert!(
            errors.iter().any(|error| {
                error.message == "Algorand emission supports one function for now"
            })
        );
    }

    #[test]
    fn rejects_non_int_global_state_layout() {
        let source = parser::parse_source(
            r#"
model Counter {
    value: bool
}

state Ready(Counter) {}

fn set(enabled: bool) when Ready as before -> Ready as after
{
    after.value = enabled;
}
"#,
        )
        .expect("source should parse");
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let errors = emit(&contract).expect_err("bool state should not emit yet");

        assert!(errors.iter().any(|error| {
            error.message == "Algorand global state only supports int field value for now"
        }));
    }

    #[test]
    fn rejects_missing_assignment_body() {
        let source = parser::parse_source(
            r#"
model Counter {
    value: int
}

state Ready(Counter) {}

fn increment(amount: int) when Ready as before -> Ready as after
{
    skip;
}
"#,
        )
        .expect("source should parse");
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let errors = emit(&contract).expect_err("skip-only function should not emit yet");

        assert!(errors.iter().any(|error| {
            error.message == "Algorand emission requires one assignment body for now"
        }));
    }
}
