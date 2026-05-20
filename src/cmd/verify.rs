use std::fs;
use std::path::PathBuf;
use std::process;

use clap::Args;

#[derive(Args)]
pub struct VerifyCommand {
    /// May source file to verify.
    path: PathBuf,
}

impl VerifyCommand {
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

        let contract = match semantics::check(&program) {
            Ok(contract) => contract,
            Err(errors) => {
                for error in errors {
                    eprintln!("semantic error: {error}");
                }
                process::exit(1);
            }
        };

        match verifier::verify(&contract) {
            Ok(report) => {
                println!(
                    "Program is structurally verified: {} bounds checked.",
                    report.checked_bounds
                );
            }
            Err(errors) => {
                for error in errors {
                    eprintln!("verification error: {error}");
                }
                process::exit(1);
            }
        }
    }
}
