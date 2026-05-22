use std::fs;
use std::path::PathBuf;
use std::process;

use clap::Args;
use diagnostics::Paint;
use verifier::SolverResult;

use super::render_diagnostics;
use super::render_error;
use super::render_errors;

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
                render_error(&self.path, &source, &error);
                process::exit(1);
            }
        };

        let contract = match semantics::check(&program) {
            Ok(contract) => contract,
            Err(errors) => {
                render_errors(&self.path, &source, &errors);
                process::exit(1);
            }
        };

        match verifier::verify(&contract) {
            Ok(report) => {
                println!(
                    "{} {} bounds checked, {} accepted, {} rejected, {} unknown, {} unsupported; \
                     {} must blocks checked, {} accepted, {} rejected, {} unknown, {} \
                     unsupported; {} transition goals proved, {} failed, {} unknown, {} \
                     unsupported.",
                    "Program verified:".green().bold(),
                    report.checked_bounds,
                    report.accepted_constraints(),
                    report.rejected_constraints(),
                    report.unknown_constraints(),
                    report.unsupported_constraints(),
                    report.constraint_blocks.len(),
                    report.accepted_constraint_blocks(),
                    report.rejected_constraint_blocks(),
                    report.unknown_constraint_blocks(),
                    report.unsupported_constraint_blocks(),
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

                let diagnostics = report.diagnostics();
                if !diagnostics.is_empty() {
                    render_diagnostics(&self.path, &source, &diagnostics);
                }

                if report.rejected_constraints() > 0
                    || report.rejected_constraint_blocks() > 0
                    || report.failed_transition_goals() > 0
                {
                    process::exit(1);
                }
            }
            Err(errors) => {
                render_errors(&self.path, &source, &errors);
                process::exit(1);
            }
        }
    }
}
