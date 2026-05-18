use clap::Subcommand;

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
