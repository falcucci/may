use std::fs;
use std::path::PathBuf;
use std::process;

use clap::Args;
use diagnostics::Paint;

use super::render_error;
use super::render_errors;

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
                render_error(&self.path, &source, &error);
                process::exit(1);
            }
        };

        match semantics::check(&program) {
            Ok(definition) => {
                println!(
                    "{} {} declarations.",
                    "Program is semantically valid:".green().bold(),
                    definition.declaration_count()
                );
            }
            Err(errors) => {
                render_errors(&self.path, &source, &errors);
                process::exit(1);
            }
        };
    }
}
