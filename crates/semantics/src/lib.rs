use std::collections::HashMap;
use std::fmt;

use parser::Span;
use parser::ast::BinaryOperator;
use parser::ast::ConstraintBlock;
use parser::ast::Declaration;
use parser::ast::Expression;
use parser::ast::Field;
use parser::ast::Identifier;
use parser::ast::ModelItem;
use parser::ast::Parameter;
use parser::ast::Source;
use parser::ast::TypeName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDefinition {
    pub models: Vec<ModelDefinition>,
    pub states: Vec<StateDefinition>,
    pub functions: Vec<FunctionDefinition>,
}

impl ContractDefinition {
    pub fn declaration_count(&self) -> usize {
        self.models.len() + self.states.len() + self.functions.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelDefinition {
    pub name: String,
    pub fields: Vec<FieldDefinition>,
    pub bounds: Bounds,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateDefinition {
    pub name: String,
    pub model: Option<String>,
    pub fields: Vec<FieldDefinition>,
    pub bounds: Bounds,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDefinition {
    pub name: String,
    pub params: Vec<ParameterDefinition>,
    pub transition: Option<StateTransitionDefinition>,
    pub bounds: Bounds,
    pub body: Vec<parser::ast::Statement>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDefinition {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterDefinition {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTransitionDefinition {
    pub from: String,
    pub to: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bounds {
    pub span: Span,
    pub expressions: Vec<Expression>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Int,
    UInt,
    Bool,
    String,
    Address,
    Hex,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationInfo {
    pub name: String,
    pub kind: DeclarationKind,
    pub span: Span,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum DeclarationKind {
    Model,
    State,
    Function,
}

impl fmt::Display for DeclarationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeclarationKind::Model => write!(f, "model"),
            DeclarationKind::State => write!(f, "state"),
            DeclarationKind::Function => write!(f, "function"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticError {
    pub message: String,
    pub span: Span,
}

impl SemanticError {
    fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }
}

impl fmt::Display for SemanticError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at bytes {}..{}",
            self.message, self.span.start, self.span.end
        )
    }
}

impl std::error::Error for SemanticError {}

pub fn check(source: &Source) -> Result<ContractDefinition, Vec<SemanticError>> {
    let mut checker = Checker::default();
    checker.collect_declarations(source);
    checker.collect_model_fields(source);
    checker.check_declarations(source);

    if checker.errors.is_empty() {
        Ok(checker.build_contract(source))
    } else {
        Err(checker.errors)
    }
}

#[derive(Default)]
struct Checker {
    symbols: HashMap<String, DeclarationInfo>,
    model_fields: HashMap<String, HashMap<String, SemanticType>>,
    errors: Vec<SemanticError>,
}

impl Checker {
    fn collect_declarations(&mut self, source: &Source) {
        for declaration in &source.declarations {
            let info = declaration_info(declaration);

            if let Some(previous) = self.symbols.get(&info.name) {
                self.errors.push(SemanticError::new(
                    format!("{} is already declared as a {}", info.name, previous.kind),
                    info.span,
                ));
                continue;
            }

            self.symbols.insert(info.name.clone(), info);
        }
    }

    fn collect_model_fields(&mut self, source: &Source) {
        for declaration in &source.declarations {
            let Declaration::Model(model) = declaration else {
                continue;
            };

            if self.model_fields.contains_key(&model.name.text) {
                continue;
            }

            let mut fields = HashMap::<String, SemanticType>::new();

            for item in &model.items {
                let ModelItem::Field(field) = item else {
                    continue;
                };

                self.check_duplicate_field(&model.name, field, &fields);
                self.check_type(&field.ty);

                if !fields.contains_key(&field.name.text) {
                    fields.insert(field.name.text.clone(), self.resolve_type(&field.ty));
                }
            }

            self.model_fields.insert(model.name.text.clone(), fields);
        }
    }
    fn check_declarations(&mut self, source: &Source) {
        for declaration in &source.declarations {
            match declaration {
                Declaration::Model(model) => {
                    if let Some(scope) = self.model_fields.get(&model.name.text).cloned() {
                        for item in &model.items {
                            if let ModelItem::Constraint(block) = item {
                                self.check_must_block(&model.name, &scope, block);
                            }
                        }
                    }
                }
                Declaration::State(state) => {
                    let mut scope = HashMap::new();

                    if let Some(model) = &state.model {
                        self.check_model_reference(&state.name, model);
                        if let Some(model_fields) = self.model_fields.get(&model.text) {
                            scope = model_fields.clone();
                        }
                    }

                    for block in &state.constraints {
                        self.check_must_block(&state.name, &scope, block);
                    }
                }
                Declaration::Function(function) => {
                    let mut scope = HashMap::new();

                    for param in &function.params {
                        self.check_type(&param.ty);
                        self.add_parameter_to_scope(&function.name, param, &mut scope);
                    }

                    if let Some(transition) = &function.transition {
                        self.check_state_reference(&function.name, &transition.from);
                        self.check_state_reference(&function.name, &transition.to);
                    }

                    for block in &function.constraints {
                        self.check_must_block(&function.name, &scope, block);
                    }
                }
            }
        }
    }

    fn build_contract(&self, source: &Source) -> ContractDefinition {
        let mut contract = ContractDefinition {
            models: Vec::new(),
            states: Vec::new(),
            functions: Vec::new(),
        };

        for declaration in &source.declarations {
            match declaration {
                Declaration::Model(model) => contract.models.push(ModelDefinition {
                    name: model.name.text.clone(),
                    fields: self.model_field_definitions(model),
                    bounds: bounds_from_blocks(
                        model.span,
                        model.items.iter().filter_map(|item| match item {
                            ModelItem::Constraint(block) => Some(block),
                            ModelItem::Field(_) => None,
                        }),
                    ),
                    span: model.span,
                }),
                Declaration::State(state) => contract.states.push(StateDefinition {
                    name: state.name.text.clone(),
                    model: state.model.as_ref().map(|model| model.text.clone()),
                    fields: state
                        .model
                        .as_ref()
                        .map(|model| self.model_fields_for_state(source, &model.text))
                        .unwrap_or_default(),
                    bounds: bounds_from_blocks(state.span, state.constraints.iter()),
                    span: state.span,
                }),
                Declaration::Function(function) => contract.functions.push(FunctionDefinition {
                    name: function.name.text.clone(),
                    params: function
                        .params
                        .iter()
                        .map(|param| ParameterDefinition {
                            name: param.name.text.clone(),
                            ty: self.type_definition(&param.ty),
                            span: param.span,
                        })
                        .collect(),
                    transition: function.transition.as_ref().map(|transition| {
                        StateTransitionDefinition {
                            from: transition.from.text.clone(),
                            to: transition.to.text.clone(),
                            span: transition.span,
                        }
                    }),
                    bounds: bounds_from_blocks(function.span, function.constraints.iter()),
                    body: function.body.clone(),
                    span: function.span,
                }),
            }
        }

        contract
    }

    fn model_fields_for_state(&self, source: &Source, model_name: &str) -> Vec<FieldDefinition> {
        source
            .declarations
            .iter()
            .find_map(|declaration| match declaration {
                Declaration::Model(model) if model.name.text == model_name => {
                    Some(self.model_field_definitions(model))
                }
                _ => None,
            })
            .unwrap_or_default()
    }

    fn model_field_definitions(
        &self,
        model: &parser::ast::ModelDeclaration,
    ) -> Vec<FieldDefinition> {
        model
            .items
            .iter()
            .filter_map(|item| match item {
                ModelItem::Field(field) => Some(FieldDefinition {
                    name: field.name.text.clone(),
                    ty: self.type_definition(&field.ty),
                    span: field.span,
                }),
                ModelItem::Constraint(_) => None,
            })
            .collect()
    }

    fn check_duplicate_field(
        &mut self,
        model_name: &Identifier,
        field: &Field,
        fields: &HashMap<String, SemanticType>,
    ) {
        if fields.contains_key(&field.name.text) {
            self.errors.push(SemanticError::new(
                format!(
                    "{} already has a field named {}",
                    model_name.text, field.name.text
                ),
                field.name.span,
            ));
        }
    }

    fn check_model_reference(&mut self, state_name: &Identifier, model_name: &Identifier) {
        match self.symbols.get(&model_name.text) {
            Some(info) if info.kind == DeclarationKind::Model => {}
            Some(info) => self.errors.push(SemanticError::new(
                format!(
                    "{} uses {} as a model, but it is a {}",
                    state_name.text, model_name.text, info.kind
                ),
                model_name.span,
            )),
            None => self.errors.push(SemanticError::new(
                format!(
                    "{} refers to unknown model {}",
                    state_name.text, model_name.text
                ),
                model_name.span,
            )),
        }
    }

    fn check_state_reference(&mut self, function_name: &Identifier, state_name: &Identifier) {
        match self.symbols.get(&state_name.text) {
            Some(info) if info.kind == DeclarationKind::State => {}
            Some(info) => self.errors.push(SemanticError::new(
                format!(
                    "{} uses {} as a state, but it is a {}",
                    function_name.text, state_name.text, info.kind
                ),
                state_name.span,
            )),
            None => self.errors.push(SemanticError::new(
                format!(
                    "{} refers to unknown state {}",
                    function_name.text, state_name.text
                ),
                state_name.span,
            )),
        }
    }

    fn check_type(&mut self, ty: &TypeName) {
        if is_primitive_type(&ty.name.text) {
            return;
        }

        match self.symbols.get(&ty.name.text) {
            Some(info) if matches!(info.kind, DeclarationKind::Model | DeclarationKind::State) => {}
            Some(info) => self.errors.push(SemanticError::new(
                format!("{} is a {}, not a type", ty.name.text, info.kind),
                ty.name.span,
            )),
            None => self.errors.push(SemanticError::new(
                format!("unknown type {}", ty.name.text),
                ty.name.span,
            )),
        }
    }
    fn add_parameter_to_scope(
        &mut self,
        function_name: &Identifier,
        param: &Parameter,
        scope: &mut HashMap<String, SemanticType>,
    ) {
        if scope.contains_key(&param.name.text) {
            self.errors.push(SemanticError::new(
                format!(
                    "{} already has a parameter named {}",
                    function_name.text, param.name.text
                ),
                param.name.span,
            ));
            return;
        }

        scope.insert(param.name.text.clone(), self.resolve_type(&param.ty));
    }

    fn check_must_block(
        &mut self,
        owner_name: &Identifier,
        scope: &HashMap<String, SemanticType>,
        block: &ConstraintBlock,
    ) {
        for expression in &block.expressions {
            let ty = self.infer_expression(owner_name, scope, expression);

            if ty == SemanticType::Unknown {
                continue;
            }

            if ty != SemanticType::Bool {
                self.errors.push(SemanticError::new(
                    format!("{} must expression must resolve to bool", owner_name.text),
                    expression.span(),
                ));
            }
        }
    }

    fn infer_expression(
        &mut self,
        owner_name: &Identifier,
        scope: &HashMap<String, SemanticType>,
        expression: &Expression,
    ) -> SemanticType {
        match expression {
            Expression::Identifier(identifier) => match scope.get(&identifier.text) {
                Some(ty) => ty.clone(),
                None => {
                    self.errors.push(SemanticError::new(
                        format!(
                            "{} must block refers to unknown identifier {}",
                            owner_name.text, identifier.text
                        ),
                        identifier.span,
                    ));
                    SemanticType::Unknown
                }
            },
            Expression::Integer(_) => SemanticType::IntegerLiteral,
            Expression::Binary { lhs, op, rhs, span } => {
                let lhs_ty = self.infer_expression(owner_name, scope, lhs);
                let rhs_ty = self.infer_expression(owner_name, scope, rhs);

                if lhs_ty == SemanticType::Unknown || rhs_ty == SemanticType::Unknown {
                    return SemanticType::Unknown;
                }

                self.infer_binary_expression(*op, &lhs_ty, &rhs_ty, *span)
            }
        }
    }

    fn infer_binary_expression(
        &mut self,
        op: BinaryOperator,
        lhs_ty: &SemanticType,
        rhs_ty: &SemanticType,
        span: Span,
    ) -> SemanticType {
        match op {
            BinaryOperator::Add
            | BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Modulo => {
                if lhs_ty.is_numeric() && rhs_ty.is_numeric() {
                    SemanticType::merged_numeric(lhs_ty, rhs_ty)
                } else {
                    self.errors.push(SemanticError::new(
                        format!(
                            "operator {} expects numeric operands",
                            binary_operator_text(op)
                        ),
                        span,
                    ));
                    SemanticType::Unknown
                }
            }
            BinaryOperator::Greater
            | BinaryOperator::GreaterEqual
            | BinaryOperator::Less
            | BinaryOperator::LessEqual => {
                if lhs_ty.is_numeric() && rhs_ty.is_numeric() {
                    SemanticType::Bool
                } else {
                    self.errors.push(SemanticError::new(
                        format!(
                            "operator {} expects numeric operands",
                            binary_operator_text(op)
                        ),
                        span,
                    ));
                    SemanticType::Unknown
                }
            }
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                if lhs_ty.is_compatible_with(rhs_ty) {
                    SemanticType::Bool
                } else {
                    self.errors.push(SemanticError::new(
                        format!(
                            "operator {} expects compatible operands",
                            binary_operator_text(op)
                        ),
                        span,
                    ));
                    SemanticType::Unknown
                }
            }
        }
    }

    fn resolve_type(&self, ty: &TypeName) -> SemanticType {
        match ty.name.text.as_str() {
            "int" => SemanticType::Int,
            "uint" => SemanticType::UInt,
            "bool" => SemanticType::Bool,
            "string" => SemanticType::String,
            "address" => SemanticType::Address,
            "hex" => SemanticType::Hex,
            name => match self.symbols.get(name) {
                Some(info)
                    if matches!(info.kind, DeclarationKind::Model | DeclarationKind::State) =>
                {
                    SemanticType::Custom(name.to_owned())
                }
                _ => SemanticType::Unknown,
            },
        }
    }

    fn type_definition(&self, ty: &TypeName) -> Type {
        self.resolve_type(ty)
            .to_type()
            .unwrap_or_else(|| Type::Custom(ty.name.text.clone()))
    }
}

fn declaration_info(declaration: &Declaration) -> DeclarationInfo {
    match declaration {
        Declaration::Model(model) => DeclarationInfo {
            name: model.name.text.clone(),
            kind: DeclarationKind::Model,
            span: model.name.span,
        },
        Declaration::State(state) => DeclarationInfo {
            name: state.name.text.clone(),
            kind: DeclarationKind::State,
            span: state.name.span,
        },
        Declaration::Function(function) => DeclarationInfo {
            name: function.name.text.clone(),
            kind: DeclarationKind::Function,
            span: function.name.span,
        },
    }
}

fn is_primitive_type(name: &str) -> bool {
    matches!(name, "int" | "uint" | "bool" | "string" | "address" | "hex")
}

fn bounds_from_blocks<'a>(
    owner_span: Span,
    blocks: impl IntoIterator<Item = &'a ConstraintBlock>,
) -> Bounds {
    Bounds {
        span: owner_span,
        expressions: blocks
            .into_iter()
            .flat_map(|block| block.expressions.iter().cloned())
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SemanticType {
    Int,
    UInt,
    Bool,
    String,
    Address,
    Hex,
    Custom(String),
    IntegerLiteral,
    Unknown,
}

impl SemanticType {
    fn to_type(&self) -> Option<Type> {
        match self {
            SemanticType::Int => Some(Type::Int),
            SemanticType::UInt => Some(Type::UInt),
            SemanticType::Bool => Some(Type::Bool),
            SemanticType::String => Some(Type::String),
            SemanticType::Address => Some(Type::Address),
            SemanticType::Hex => Some(Type::Hex),
            SemanticType::Custom(name) => Some(Type::Custom(name.clone())),
            SemanticType::IntegerLiteral | SemanticType::Unknown => None,
        }
    }

    fn is_numeric(&self) -> bool {
        matches!(
            self,
            SemanticType::Int | SemanticType::UInt | SemanticType::IntegerLiteral
        )
    }

    fn is_compatible_with(&self, other: &Self) -> bool {
        self == other || (self.is_numeric() && other.is_numeric())
    }

    fn merged_numeric(lhs: &Self, rhs: &Self) -> Self {
        if lhs == &SemanticType::Int || rhs == &SemanticType::Int {
            SemanticType::Int
        } else if lhs == &SemanticType::UInt || rhs == &SemanticType::UInt {
            SemanticType::UInt
        } else {
            SemanticType::IntegerLiteral
        }
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

#[cfg(test)]
mod tests {
    use parser::parse_source;

    use super::Type;
    use super::check;

    fn semantic_errors(source: &str) -> Vec<String> {
        let source = parse_source(source).expect("source should parse");
        check(&source)
            .expect_err("source should be semantically invalid")
            .into_iter()
            .map(|error| error.message)
            .collect()
    }

    #[test]
    fn accepts_counter_subset() {
        let source = parse_source(
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

        let definition = check(&source).expect("source should be semantically valid");
        assert_eq!(definition.declaration_count(), 3);
        assert_eq!(definition.models.len(), 1);
        assert_eq!(definition.states.len(), 1);
        assert_eq!(definition.functions.len(), 1);

        let counter = &definition.models[0];
        assert_eq!(counter.name, "Counter");
        assert_eq!(counter.fields.len(), 1);
        assert_eq!(counter.fields[0].name, "value");
        assert_eq!(counter.fields[0].ty, Type::Int);
        assert_eq!(counter.bounds.expressions.len(), 1);

        let ready = &definition.states[0];
        assert_eq!(ready.name, "Ready");
        assert_eq!(ready.model.as_deref(), Some("Counter"));
        assert_eq!(ready.fields.len(), 1);
        assert_eq!(ready.fields[0].name, "value");
        assert_eq!(ready.bounds.expressions.len(), 1);

        let increment = &definition.functions[0];
        assert_eq!(increment.name, "increment");
        assert_eq!(increment.params.len(), 1);
        assert_eq!(increment.params[0].name, "amount");
        assert_eq!(increment.params[0].ty, Type::Int);
        assert_eq!(
            increment.transition.as_ref().map(|transition| transition.from.as_str()),
            Some("Ready")
        );
        assert_eq!(
            increment.transition.as_ref().map(|transition| transition.to.as_str()),
            Some("Ready")
        );
        assert_eq!(increment.bounds.expressions.len(), 1);
        assert_eq!(increment.body.len(), 1);
    }

    #[test]
    fn rejects_duplicate_top_level_names() {
        let errors = semantic_errors(
            r#"
model Counter {}
state Counter {}
"#,
        );

        assert!(errors.iter().any(|error| error == "Counter is already declared as a model"));
    }

    #[test]
    fn rejects_unknown_state_model() {
        let errors = semantic_errors("state Ready(Counter) {}");

        assert!(errors.iter().any(|error| error == "Ready refers to unknown model Counter"));
    }

    #[test]
    fn rejects_non_model_state_model() {
        let errors = semantic_errors(
            r#"
state Counter {}
state Ready(Counter) {}
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error == "Ready uses Counter as a model, but it is a state")
        );
    }

    #[test]
    fn rejects_unknown_transition_state() {
        let errors = semantic_errors(
            r#"
fn increment(amount: int) when Ready -> Ready {
    skip;
}
"#,
        );

        assert!(errors.iter().any(|error| error == "increment refers to unknown state Ready"));
    }

    #[test]
    fn rejects_duplicate_model_fields() {
        let errors = semantic_errors(
            r#"
model Counter {
    value: int
    value: int
}
"#,
        );

        assert!(errors.iter().any(|error| error == "Counter already has a field named value"));
    }

    #[test]
    fn rejects_unknown_types() {
        let errors = semantic_errors(
            r#"
model Counter {
    value: Money
}
"#,
        );

        assert!(errors.iter().any(|error| error == "unknown type Money"));
    }

    #[test]
    fn accepts_forward_declared_custom_types() {
        let source = parse_source(
            r#"
model Account {
    wallet: Wallet
}

state Wallet {}
"#,
        )
        .expect("source should parse");

        check(&source).expect("source should be semantically valid");
    }
    #[test]
    fn rejects_unknown_model_must_identifiers() {
        let errors = semantic_errors(
            r#"
model Counter {
    value: int
    must [ missing >= 0 ]
}
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error == "Counter must block refers to unknown identifier missing")
        );
    }

    #[test]
    fn rejects_non_boolean_model_must_expressions() {
        let errors = semantic_errors(
            r#"
model Counter {
    value: int
    must [ value ]
}
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error == "Counter must expression must resolve to bool")
        );
    }

    #[test]
    fn checks_state_must_expressions_against_model_fields() {
        let errors = semantic_errors(
            r#"
model Counter {
    value: int
}

state Ready(Counter) {
    must [ missing >= 0 ]
}
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error == "Ready must block refers to unknown identifier missing")
        );
    }

    #[test]
    fn rejects_state_must_fields_without_a_model_scope() {
        let errors = semantic_errors(
            r#"
state Ready {
    must [ value >= 0 ]
}
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error == "Ready must block refers to unknown identifier value")
        );
    }

    #[test]
    fn rejects_invalid_must_operator_operands() {
        let errors = semantic_errors(
            r#"
model Counter {
    value: string
    must [ value + 1 >= 0 ]
}
"#,
        );

        assert!(errors.iter().any(|error| error == "operator + expects numeric operands"));
    }

    #[test]
    fn checks_function_must_expressions_against_parameters() {
        let errors = semantic_errors(
            r#"
fn increment(amount: int) must [ missing > 0 ] {
    skip;
}
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error == "increment must block refers to unknown identifier missing")
        );
    }

    #[test]
    fn rejects_non_boolean_function_must_expressions() {
        let errors = semantic_errors(
            r#"
fn increment(amount: int) must [ amount ] {
    skip;
}
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error == "increment must expression must resolve to bool")
        );
    }

    #[test]
    fn rejects_duplicate_function_parameters() {
        let errors = semantic_errors(
            r#"
fn increment(amount: int, amount: int) must [ amount > 0 ] {
    skip;
}
"#,
        );

        assert!(
            errors
                .iter()
                .any(|error| error == "increment already has a parameter named amount")
        );
    }
}
