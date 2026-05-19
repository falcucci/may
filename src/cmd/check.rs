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

        match parser::parse_source(&source) {
            Ok(program) => {
                println!(
                    "Program parsed successfully: {} declarations.",
                    program.declarations.len()
                );
            }
            Err(error) => {
                eprintln!("parse error: {error}");
                process::exit(1);
            }
        }
    }
}
