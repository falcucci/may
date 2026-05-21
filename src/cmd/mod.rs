use std::path::Path;

use clap::Subcommand;
use diagnostics::Report;
use diagnostics::ToReport;

use self::check::CheckCommand;
use self::compile::CompileCommand;
use self::new::NewCommand;
use self::verify::VerifyCommand;

mod check;
mod compile;
mod new;
mod verify;

#[derive(Subcommand)]
pub enum Commands {
    New(NewCommand),
    Check(CheckCommand),
    Verify(VerifyCommand),
    Compile(CompileCommand),
}

impl Commands {
    pub fn run(&self) {
        match self {
            Commands::New(cmd) => cmd.run(),
            Commands::Check(cmd) => cmd.run(),
            Commands::Verify(cmd) => cmd.run(),
            Commands::Compile(cmd) => cmd.run(),
        }
    }
}

fn render_diagnostics(path: &Path, source: &str, reports: &[Report]) {
    let file_name = path.display().to_string();

    if let Err(error) = diagnostics::render_reports(&file_name, source, reports) {
        eprintln!(
            "failed to render diagnostics for {}: {error}",
            path.display()
        );
    }
}

fn render_error<E: ToReport>(path: &Path, source: &str, error: &E) {
    render_diagnostics(path, source, &[error.to_report()]);
}

fn render_errors<E: ToReport>(path: &Path, source: &str, errors: &[E]) {
    let reports = errors.iter().map(|error| error.to_report()).collect::<Vec<_>>();
    render_diagnostics(path, source, &reports);
}
