use std::fmt;

use semantics::ContractDefinition;

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
    Return,
}

impl fmt::Display for TealInstruction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TealInstruction::PragmaVersion(version) => write!(f, "#pragma version {version}"),
            TealInstruction::Int(value) => write!(f, "int {value}"),
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
        _contract: &ContractDefinition,
    ) -> Result<AlgorandArtifacts, Vec<AlgorandError>> {
        Ok(AlgorandArtifacts {
            approval_teal: TealProgram::always_approve().render(),
            clear_teal: TealProgram::always_approve().render(),
        })
    }
}

pub fn emit(contract: &ContractDefinition) -> Result<AlgorandArtifacts, Vec<AlgorandError>> {
    AlgorandEmitter.emit(contract)
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
}
