#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd -P)"
PROVIDER_WASM="$PROJECT_ROOT/target/wasm32-wasip2/release/carina-provider-aws.wasm"
CARINA_BIN="${CARINA_BIN:-carina}"

if [ ! -f "$PROVIDER_WASM" ]; then
    echo "ERROR: release provider WASM not found: $PROVIDER_WASM" >&2
    echo "Build it with: cargo build -p carina-provider-aws --target wasm32-wasip2 --release" >&2
    exit 1
fi

if ! CARINA_BIN_PATH="$(command -v "$CARINA_BIN")"; then
    echo "ERROR: carina binary not found: $CARINA_BIN" >&2
    echo "Set CARINA_BIN to the carina executable or install carina in PATH." >&2
    exit 1
fi
if [[ "$CARINA_BIN_PATH" != /* ]]; then
    CARINA_BIN_PATH="$(cd "$(dirname "$CARINA_BIN_PATH")" && pwd -P)/$(basename "$CARINA_BIN_PATH")"
fi

_TEST_AWS_WASM="$PROVIDER_WASM"
_TEST_AWSCC_WASM=""
source "$PROJECT_ROOT/acceptance-tests/shared/_provider_env.sh"

TEMP_ROOT="$(mktemp -d)"
cleanup() {
    rm -rf "$TEMP_ROOT"
}
trap cleanup EXIT

shopt -s nullglob
example_dirs=("$PROJECT_ROOT"/examples/*/)
if [ "${#example_dirs[@]}" -eq 0 ]; then
    echo "ERROR: no example directories found under $PROJECT_ROOT/examples" >&2
    exit 1
fi

passed=0
failed=0

for example_dir in "${example_dirs[@]}"; do
    example_name="$(basename "$example_dir")"
    work_dir="$TEMP_ROOT/$example_name"
    preparation_error=""

    mkdir -p "$work_dir"

    if ! cp -R "${example_dir}." "$work_dir"; then
        preparation_error="failed to copy example directory"
    else
        crn_files=("$work_dir"/*.crn)

        if [ "${#crn_files[@]}" -eq 0 ]; then
            preparation_error="no .crn files found"
        else
            for crn_file in "${crn_files[@]}"; do
                if ! grep -q '^provider aws {' "$crn_file"; then
                    continue
                fi

                if ! tmp_file="$(mktemp "${crn_file}.XXXXXX")"; then
                    preparation_error="failed to create temporary file for $(basename "$crn_file")"
                    break
                fi

                if ! _inject_provider_source_filter < "$crn_file" > "$tmp_file" ||
                    ! mv "$tmp_file" "$crn_file"; then
                    preparation_error="failed to inject provider source into $(basename "$crn_file")"
                    break
                fi
            done
        fi
    fi

    if [ -n "$preparation_error" ]; then
        echo "FAIL $example_name"
        echo "  $preparation_error"
        failed=$((failed + 1))
        continue
    fi

    if output=$(
        {
            cd "$work_dir" &&
                "$CARINA_BIN_PATH" init . &&
                "$CARINA_BIN_PATH" validate .
        } 2>&1
    ); then
        echo "PASS $example_name"
        passed=$((passed + 1))
    else
        echo "FAIL $example_name"
        printf '%s\n' "$output" | sed 's/^/  /'
        failed=$((failed + 1))
    fi
done

echo
echo "Validated $((passed + failed)) example directories: $passed passed, $failed failed."

if [ "$failed" -ne 0 ]; then
    exit 1
fi
