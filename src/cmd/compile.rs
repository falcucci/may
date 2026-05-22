use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process;

use clap::Args;
use diagnostics::Paint;

use super::render_diagnostics;
use super::render_error;
use super::render_errors;

#[derive(Args)]
pub struct CompileCommand {
    /// May source file to compile.
    path: PathBuf,
}

impl CompileCommand {
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

        let report = match verifier::verify(&contract) {
            Ok(report) => report,
            Err(errors) => {
                render_errors(&self.path, &source, &errors);
                process::exit(1);
            }
        };

        let diagnostics = report.diagnostics();
        if !diagnostics.is_empty() {
            render_diagnostics(&self.path, &source, &diagnostics);
        }

        if !verification_all_clear(&report) {
            eprintln!("verification did not complete successfully; refusing to emit artifacts");
            process::exit(1);
        }

        let artifacts = match algorand::emit(&contract) {
            Ok(artifacts) => artifacts,
            Err(errors) => {
                render_errors(&self.path, &source, &errors);
                process::exit(1);
            }
        };

        let output_dir = build_dir_for(&self.path);
        if let Err(error) = fs::create_dir_all(&output_dir) {
            eprintln!("failed to create {}: {error}", output_dir.display());
            process::exit(1);
        }

        let approval_path = output_dir.join("approval.teal");
        if let Err(error) = fs::write(&approval_path, artifacts.approval_teal.as_bytes()) {
            eprintln!("failed to write {}: {error}", approval_path.display());
            process::exit(1);
        }

        let clear_path = output_dir.join("clear.teal");
        if let Err(error) = fs::write(&clear_path, artifacts.clear_teal.as_bytes()) {
            eprintln!("failed to write {}: {error}", clear_path.display());
            process::exit(1);
        }

        println!(
            "{}",
            "Successfully compiled Algorand artifacts.".green().bold()
        );
        println!("{} {}", "Approval:".cyan().bold(), approval_path.display());
        println!("{} {}", "Clear:".cyan().bold(), clear_path.display());
    }
}

fn build_dir_for(path: &Path) -> PathBuf {
    path.parent().unwrap_or_else(|| Path::new(".")).join("build")
}

fn verification_all_clear(report: &verifier::VerificationReport) -> bool {
    report.rejected_constraints() == 0
        && report.unknown_constraints() == 0
        && report.unsupported_constraints() == 0
        && report.rejected_constraint_blocks() == 0
        && report.unknown_constraint_blocks() == 0
        && report.unsupported_constraint_blocks() == 0
        && report.failed_transition_goals() == 0
        && report.unknown_transition_goals() == 0
        && report.unsupported_transition_goals() == 0
}
