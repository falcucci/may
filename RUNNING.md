Running May

You'll need Rust and Cargo. The repository pins the Rust toolchain, so a
normal Cargo command should pick the right compiler if rustup is installed.

The verifier uses Z3 through the Rust bindings. If the build cannot find Z3,
install it with your package manager. On macOS that is normally:

    brew install z3

Start with the help output if you want the CLI surface:

    cargo run -- help

To check that the program parses and passes the semantic pass:

    cargo run -- check examples/counter.may

To ask the verifier to prove the `must` bounds and transition:

    cargo run -- verify examples/counter.may

And to emit Algorand artifacts:

    cargo run -- compile examples/counter.may

Compile runs the same front end and verifier first. If verification fails, it
does not write artifacts.

When it succeeds, it writes the target output next to the source file under:

    examples/build/algorand/

That directory contains:

    approval.teal
    clear.teal
    application.json

The generated TEAL is deliberately simple. It dispatches on the first
application argument, updates global state for the supported assignment
subset and rejects calls it does not understand.
