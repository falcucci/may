use std::fs;
use std::path::PathBuf;
use std::process;

use clap::Args;
use verifier::SolverResult;

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
                    "Program verified: {} bounds checked, {} accepted, {} rejected, {} unknown, \
                     {} unsupported; {} transition goals proved, {} failed, {} unknown, {} \
                     unsupported.",
                    report.checked_bounds,
                    report.accepted_constraints(),
                    report.rejected_constraints(),
                    report.unknown_constraints(),
                    report.unsupported_constraints(),
                    report.proved_transition_goals(),
                    report.failed_transition_goals(),
                    report.unknown_transition_goals(),
                    report.unsupported_transition_goals()
                );

                for result in report
                    .transition_results
                    .iter()
                    .filter(|result| result.result == SolverResult::Rejected)
                {
                    println!(
                        "Counterexample for {} transition {} -> {}:",
                        result.function, result.from, result.to
                    );

                    for value in &result.counterexample {
                        println!("  {} = {}", value.name, value.value);
                    }
                }

                if report.rejected_constraints() > 0 || report.failed_transition_goals() > 0 {
                    process::exit(1);
                }
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
