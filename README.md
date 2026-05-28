# May

May is an experimental programming language for writing contracts around
explicit state, not hidden mutation.

The goal is simple: make the important rules part of the language itself. Data
models, states, transitions, and invariants should be written in syntax the
compiler can parse, check and eventually verify.

_This is not a usable contract language yet. Right now the project has a small
front end and the first verifier path._

    model Counter {
        value: int
        must [ value >= 0 ]
    }

    state Ready(Counter) {
        must [ value >= 0 ]
    }

    fn increment(amount: int) when Ready as before -> Ready as after
    must [
        amount > 0,
        after.value == before.value + amount
    ]
    {
        after.value = before.value + amount;
    }

Run the checks:

    cargo run -- check examples/counter.may
    cargo run -- verify examples/counter.may
