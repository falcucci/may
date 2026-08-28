use std::collections::BTreeMap;
use std::fmt;

use parser::Span;
use parser::ast::BinaryOperator;
use parser::ast::Expression;
use semantics::ContractDefinition;
use semantics::FunctionDefinition;
use semantics::StateDefinition;
use semantics::Statement;
use semantics::Type;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlgorandArtifacts {
    pub approval_teal: String,
    pub clear_teal: String,
    pub application_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AlgorandManifest {
    pub contract: String,
    pub global_schema: StateSchema,
    pub local_schema: StateSchema,
    pub global_state: Vec<GlobalStateEntry>,
    pub functions: Vec<FunctionEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateSchema {
    pub num_uints: usize,
    pub num_byte_slices: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GlobalStateEntry {
    pub field: String,
    pub key: String,
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FunctionEntry {
    pub name: String,
    pub selector: String,
    pub params: Vec<FunctionParamEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FunctionParamEntry {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub application_arg: usize,
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
    IntEnum(&'static str),
    ByteString(String),
    TxnApplicationId,
    TxnOnCompletion,
    TxnNumAppArgs,
    TxnaApplicationArgs(usize),
    Btoi,
    AppGlobalGet,
    AppGlobalPut,
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    GreaterThan,
    BranchIfNotZero(String),
    Label(String),
    Return,
}

impl fmt::Display for TealInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TealInstruction::PragmaVersion(version) => write!(f, "#pragma version {version}"),
            TealInstruction::Int(value) => write!(f, "int {value}"),
            TealInstruction::IntEnum(value) => write!(f, "int {value}"),
            TealInstruction::ByteString(value) => {
                write!(f, "byte \"{}\"", escape_teal_string(value))
            }
            TealInstruction::TxnApplicationId => write!(f, "txn ApplicationID"),
            TealInstruction::TxnOnCompletion => write!(f, "txn OnCompletion"),
            TealInstruction::TxnNumAppArgs => write!(f, "txn NumAppArgs"),
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
            TealInstruction::Equal => write!(f, "=="),
            TealInstruction::GreaterThan => write!(f, ">"),
            TealInstruction::BranchIfNotZero(label) => write!(f, "bnz {label}"),
            TealInstruction::Label(label) => write!(f, "{label}:"),
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
        let approval = if contract.functions.is_empty() && global_state_entries(contract).is_empty()
        {
            TealProgram::always_approve()
        } else {
            self.emit_application(contract)?
        };

        Ok(AlgorandArtifacts {
            approval_teal: approval.render(),
            clear_teal: TealProgram::always_approve().render(),
            application_json: render_manifest(&build_manifest(contract)),
        })
    }

    fn emit_application(
        &self,
        contract: &ContractDefinition,
    ) -> Result<TealProgram, Vec<AlgorandError>> {
        let mut contexts = Vec::new();
        let mut errors = Vec::new();

        for function in &contract.functions {
            match CompileContext::new(contract, function) {
                Ok(context) => contexts.push(context),
                Err(mut function_errors) => errors.append(&mut function_errors),
            }
        }

        let mut bodies = Vec::new();
        for context in &contexts {
            let mut body = Vec::new();
            match context.emit_body(&mut body) {
                Ok(()) => {
                    let label = function_label(&context.function.name);
                    bodies.push((
                        label,
                        function_body_label(&context.function.name),
                        context.function.params.len() + 1,
                        body,
                    ));
                }
                Err(mut body_errors) => errors.append(&mut body_errors),
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        let mut instructions = vec![TealInstruction::PragmaVersion(10)];

        emit_lifecycle_gate(&mut instructions);
        emit_create_branch(contract, &mut instructions);
        emit_noop_branch(&mut instructions);

        for context in &contexts {
            instructions.push(TealInstruction::TxnaApplicationArgs(0));
            instructions.push(TealInstruction::ByteString(context.function.name.clone()));
            instructions.push(TealInstruction::Equal);
            instructions.push(TealInstruction::BranchIfNotZero(function_label(
                &context.function.name,
            )));
        }

        instructions.push(TealInstruction::Int(0));
        instructions.push(TealInstruction::Return);

        for (label, body_label, expected_arg_count, body) in bodies {
            instructions.push(TealInstruction::Label(label));
            instructions.push(TealInstruction::TxnNumAppArgs);
            instructions.push(TealInstruction::Int(expected_arg_count as u64));
            instructions.push(TealInstruction::Equal);
            instructions.push(TealInstruction::BranchIfNotZero(body_label.clone()));
            instructions.push(TealInstruction::Int(0));
            instructions.push(TealInstruction::Return);
            instructions.push(TealInstruction::Label(body_label));
            instructions.extend(body);
        }

        Ok(TealProgram::new(instructions))
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

    fn emit_body(&self, instructions: &mut Vec<TealInstruction>) -> Result<(), Vec<AlgorandError>> {
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
                    if let Err(error) = self.emit_expression(&assignment.value, instructions) {
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

        Ok(())
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

                instructions.push(TealInstruction::TxnaApplicationArgs(index + 1));
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

fn emit_lifecycle_gate(instructions: &mut Vec<TealInstruction>) {
    instructions.push(TealInstruction::TxnApplicationId);
    instructions.push(TealInstruction::Int(0));
    instructions.push(TealInstruction::Equal);
    instructions.push(TealInstruction::BranchIfNotZero(create_label()));
    instructions.push(TealInstruction::TxnOnCompletion);
    instructions.push(TealInstruction::IntEnum("NoOp"));
    instructions.push(TealInstruction::Equal);
    instructions.push(TealInstruction::BranchIfNotZero(noop_label()));
    instructions.push(TealInstruction::Int(0));
    instructions.push(TealInstruction::Return);
}

fn emit_create_branch(contract: &ContractDefinition, instructions: &mut Vec<TealInstruction>) {
    instructions.push(TealInstruction::Label(create_label()));

    for field in global_state_entries(contract) {
        instructions.push(TealInstruction::ByteString(field.key));
        match field.ty.as_str() {
            "uint" => instructions.push(TealInstruction::Int(0)),
            "bytes" => instructions.push(TealInstruction::ByteString(String::new())),
            _ => unreachable!("schema type names are closed"),
        }
        instructions.push(TealInstruction::AppGlobalPut);
    }

    instructions.push(TealInstruction::Int(1));
    instructions.push(TealInstruction::Return);
}

fn emit_noop_branch(instructions: &mut Vec<TealInstruction>) {
    instructions.push(TealInstruction::Label(noop_label()));
    instructions.push(TealInstruction::TxnNumAppArgs);
    instructions.push(TealInstruction::Int(0));
    instructions.push(TealInstruction::GreaterThan);
    instructions.push(TealInstruction::BranchIfNotZero(dispatch_label()));
    instructions.push(TealInstruction::Int(0));
    instructions.push(TealInstruction::Return);
    instructions.push(TealInstruction::Label(dispatch_label()));
}

fn create_label() -> String { "may_create".to_owned() }

fn noop_label() -> String { "may_noop".to_owned() }

fn dispatch_label() -> String { "may_dispatch".to_owned() }

fn function_label(name: &str) -> String { format!("may_fn_{name}") }

fn function_body_label(name: &str) -> String { format!("may_fn_{name}_body") }

fn build_manifest(contract: &ContractDefinition) -> AlgorandManifest {
    let global_state = global_state_entries(contract);

    AlgorandManifest {
        contract: "MayContract".to_owned(),
        global_schema: StateSchema {
            num_uints: global_state.iter().filter(|entry| entry.ty == "uint").count(),
            num_byte_slices: global_state.iter().filter(|entry| entry.ty == "bytes").count(),
        },
        local_schema: StateSchema {
            num_uints: 0,
            num_byte_slices: 0,
        },
        global_state,
        functions: contract.functions.iter().map(function_entry).collect(),
    }
}

fn global_state_entries(contract: &ContractDefinition) -> Vec<GlobalStateEntry> {
    let mut fields = BTreeMap::<String, Type>::new();

    for state in &contract.states {
        for field in &state.fields {
            fields.entry(field.name.clone()).or_insert_with(|| field.ty.clone());
        }
    }

    fields
        .into_iter()
        .map(|(field, ty)| GlobalStateEntry {
            key: field.clone(),
            field,
            ty: schema_type_name(&ty).to_owned(),
        })
        .collect()
}

fn function_entry(function: &FunctionDefinition) -> FunctionEntry {
    FunctionEntry {
        name: function.name.clone(),
        selector: function.name.clone(),
        params: function
            .params
            .iter()
            .enumerate()
            .map(|(index, param)| FunctionParamEntry {
                name: param.name.clone(),
                ty: type_name(&param.ty),
                application_arg: index + 1,
            })
            .collect(),
    }
}

fn render_manifest(manifest: &AlgorandManifest) -> String {
    let mut json =
        serde_json::to_string_pretty(manifest).expect("Algorand manifest should serialize");
    json.push('\n');
    json
}

fn schema_type_name(ty: &Type) -> &'static str {
    match ty {
        Type::Int | Type::UInt | Type::Bool => "uint",
        Type::String | Type::Address | Type::Hex | Type::Custom(_) => "bytes",
    }
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
mod fixtures;

#[cfg(test)]
mod tests {
    use super::AlgorandArtifacts;
    use super::TealProgram;
    use super::emit;
    use super::fixtures::*;

    fn emit_counter_increment() -> AlgorandArtifacts {
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

        emit(&contract).expect("counter should emit")
    }

    #[test]
    fn renders_minimal_teal_program() {
        assert_eq!(TealProgram::always_approve().render(), ALWAYS_APPROVE_TEAL);
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

        assert_eq!(artifacts.approval_teal, ALWAYS_APPROVE_TEAL);
        assert_eq!(artifacts.clear_teal, ALWAYS_APPROVE_TEAL);
    }

    #[test]
    fn emits_single_assignment_body() {
        let artifacts = emit_counter_increment();

        assert_eq!(artifacts.approval_teal, SINGLE_ASSIGNMENT_APPROVAL_TEAL);
        assert_eq!(artifacts.clear_teal, ALWAYS_APPROVE_TEAL);
        assert_eq!(artifacts.application_json, COUNTER_APPLICATION_JSON);
    }

    #[test]
    fn emits_two_function_dispatch() {
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

        let artifacts = emit(&contract).expect("multiple functions should emit");

        assert_eq!(artifacts.approval_teal, TWO_FUNCTION_DISPATCH_APPROVAL_TEAL);
    }

    #[test]
    fn create_initializes_global_state() {
        let artifacts = emit_counter_increment();

        assert!(artifacts.approval_teal.contains(concat!(
            "\nmay_create:\n",
            "byte \"value\"\n",
            "int 0\n",
            "app_global_put\n",
            "int 1\n",
            "return\n",
        )));
    }

    #[test]
    fn noop_calls_require_application_args_before_dispatch() {
        let artifacts = emit_counter_increment();

        assert!(artifacts.approval_teal.contains(concat!(
            "\nmay_noop:\n",
            "txn NumAppArgs\n",
            "int 0\n",
            ">\n",
            "bnz may_dispatch\n",
            "int 0\n",
            "return\n",
            "may_dispatch:\n",
        )));
    }

    #[test]
    fn non_noop_calls_are_rejected() {
        let artifacts = emit_counter_increment();

        assert!(artifacts.approval_teal.contains(concat!(
            "\ntxn OnCompletion\n",
            "int NoOp\n",
            "==\n",
            "bnz may_noop\n",
            "int 0\n",
            "return\n",
            "may_create:\n",
        )));
    }

    #[test]
    fn function_dispatch_checks_exact_argument_count() {
        let artifacts = emit_counter_increment();

        assert!(artifacts.approval_teal.contains(concat!(
            "\nmay_fn_increment:\n",
            "txn NumAppArgs\n",
            "int 2\n",
            "==\n",
            "bnz may_fn_increment_body\n",
            "int 0\n",
            "return\n",
            "may_fn_increment_body:\n",
        )));
    }

    #[test]
    fn dispatch_falls_through_to_reject_unknown_selectors() {
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
"#,
        )
        .expect("source should parse");
        let contract = semantics::check(&source).expect("source should be semantically valid");

        let artifacts = emit(&contract).expect("function should emit");

        assert!(artifacts.approval_teal.contains("\nint 0\nreturn\nmay_fn_increment:\n"));
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
