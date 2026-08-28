set shell := ["bash", "-euo", "pipefail", "-c"]

source := "examples/counter.may"
approval_teal := "examples/build/algorand/approval.teal"
clear_teal := "examples/build/algorand/clear.teal"

default:
    just --list

check file=source:
    cargo run -- check {{file}}

verify file=source:
    cargo run -- verify {{file}}

compile file=source:
    cargo run -- compile {{file}}

test:
    cargo test --workspace

algorand-localnet:
    algokit localnet start

algorand-accounts:
    algokit goal account list

algorand-teal:
    cargo run -- compile {{source}}
    algokit goal clerk compile {{approval_teal}}
    algokit goal clerk compile {{clear_teal}}

algorand-counter-smoke creator="" amount="5":
    #!/usr/bin/env bash
    set -euo pipefail

    creator="{{creator}}"

    cargo run -- compile {{source}}
    algokit localnet start

    if [ -z "$creator" ] || [ "$creator" = "ADDRESS" ]; then
        accounts_output="$(algokit goal account list)"
        printf '%s\n' "$accounts_output"
        creator="$(
            printf '%s\n' "$accounts_output" \
                | sed -n 's/.*\([A-Z2-7]\{58\}\).*/\1/p' \
                | sed -n '1p'
        )"
    fi

    if [ -z "$creator" ]; then
        printf 'failed to choose a LocalNet creator account\n' >&2
        exit 1
    fi

    printf 'Using creator: %s\n' "$creator"

    algokit goal clerk compile {{approval_teal}}
    algokit goal clerk compile {{clear_teal}}

    create_status=0
    create_output="$(
        algokit goal app create \
            --creator "$creator" \
            --approval-prog "{{approval_teal}}" \
            --clear-prog "{{clear_teal}}" \
            --global-byteslices 0 \
            --global-ints 1 \
            --local-byteslices 0 \
            --local-ints 0 \
            2>&1
    )" || create_status=$?
    printf '%s\n' "$create_output"

    if [ "$create_status" -ne 0 ]; then
        exit "$create_status"
    fi

    app_id="$(
        printf '%s\n' "$create_output" \
            | sed -n 's/.*app index \([0-9][0-9]*\).*/\1/p' \
            | tail -n 1
    )"

    if [ -z "$app_id" ]; then
        printf 'failed to read app id from goal output\n' >&2
        exit 1
    fi

    algokit goal app call \
        --app-id "$app_id" \
        --from "$creator" \
        --app-arg "str:increment" \
        --app-arg "int:{{amount}}"

    algokit goal app read \
        --app-id "$app_id" \
        --guess-format \
        --global \
        --from "$creator"
