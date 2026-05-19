use std::fs;
use std::path::PathBuf;
use std::process;

use clap::Args;

#[derive(Args)]
pub struct CheckCommand {
    /// May source file to parse.
    path: PathBuf,
}

impl CheckCommand {
    pub fn run(&self) {
        let source = match fs::read_to_string(&self.path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!("failed to read {}: {error}", self.path.display());
                process::exit(1);
            }
        };

        let program = match parser::parse_source(&source) {
            Ok(program) => program,
            Err(error) => {
                eprintln!("parse error: {error}");
                process::exit(1);
            }
        };

        match semantics::check(&program) {
            Ok(definition) => {
                println!(
                    "Program is semantically valid: {} declarations.",
                    definition.declaration_count()
                );
            }
            Err(errors) => {
                for error in errors {
                    eprintln!("semantic error: {error}");
                }
                process::exit(1);
            }
        };
    }
}
