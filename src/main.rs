use clap::Parser;
use cmd::Commands;

mod cmd;

#[derive(Parser)]
#[command(author, version, about, subcommand_required = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn main() {
    let cli = Cli::parse();
    cli.command.run();
}
