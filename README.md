# May

May is an experimental programming language for writing contracts around
explicit state, not hidden mutation.

The goal is simple: make the important rules part of the language itself. Data
models, states, transitions, and invariants should be written in syntax the
compiler can parse, check and eventually verify.

_This is not a usable contract language yet. Right now the project only has the
start of the front end: parsing and basic semantic checks._

    model Counter {
        value: int
        must [ value >= 0 ]
    }

    state Ready(Counter) {
        must [ value >= 0 ]
    }

    fn increment(amount: int) when Ready -> Ready
    must [ amount > 0 ]
    {
        skip;
    }

Run the checks:

    cargo run -- check examples/counter.may
