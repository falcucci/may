# Counter Example

The smallest useful May shape is a model, a state and a fn that says how the
state moves.

The source file is examples/counter.may.

```may
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
```

The model says that value is an integer and must never be negative.

The state repeats that bound for Ready.

The fn says it moves from one Ready state to another one. Its body says how
the state changes. Its must block says what has to be true around that
change.

Those two views should describe the same transition.
