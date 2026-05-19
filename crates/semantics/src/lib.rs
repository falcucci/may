use std::collections::HashMap;
use std::fmt;

use parser::Span;
use parser::ast::Declaration;
use parser::ast::Field;
use parser::ast::Identifier;
use parser::ast::ModelItem;
use parser::ast::Source;
use parser::ast::TypeName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractDefinition {
    pub declarations: Vec<DeclarationInfo>,
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
    checker.check_declarations(source);

    if checker.errors.is_empty() {
        Ok(ContractDefinition {
            declarations: checker
                .declaration_order
                .iter()
                .filter_map(|name| checker.symbols.get(name))
                .cloned()
                .collect(),
        })
    } else {
        Err(checker.errors)
    }
}

#[derive(Default)]
struct Checker {
    symbols: HashMap<String, DeclarationInfo>,
    declaration_order: Vec<String>,
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

            self.declaration_order.push(info.name.clone());
            self.symbols.insert(info.name.clone(), info);
        }
    }

    fn check_declarations(&mut self, source: &Source) {
        for declaration in &source.declarations {
            match declaration {
                Declaration::Model(model) => self.check_model_fields(&model.name, &model.items),
                Declaration::State(state) => {
                    if let Some(model) = &state.model {
                        self.check_model_reference(&state.name, model);
                    }
                }
                Declaration::Function(function) => {
                    for param in &function.params {
                        self.check_type(&param.ty);
                    }

                    if let Some(transition) = &function.transition {
                        self.check_state_reference(&function.name, &transition.from);
                        self.check_state_reference(&function.name, &transition.to);
                    }
                }
            }
        }
    }

    fn check_model_fields(&mut self, model_name: &Identifier, items: &[ModelItem]) {
        let mut fields = HashMap::<String, Span>::new();

        for item in items {
            let ModelItem::Field(field) = item else {
                continue;
            };

            self.check_duplicate_field(model_name, field, &mut fields);
            self.check_type(&field.ty);
        }
    }

    fn check_duplicate_field(
        &mut self,
        model_name: &Identifier,
        field: &Field,
        fields: &mut HashMap<String, Span>,
    ) {
        if fields.insert(field.name.text.clone(), field.name.span).is_some() {
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

#[cfg(test)]
mod tests {
    use parser::parse_source;

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

fn increment(amount: int) when Ready -> Ready {
    skip;
}
"#,
        )
        .expect("source should parse");

        let definition = check(&source).expect("source should be semantically valid");
        assert_eq!(definition.declarations.len(), 3);
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
}
