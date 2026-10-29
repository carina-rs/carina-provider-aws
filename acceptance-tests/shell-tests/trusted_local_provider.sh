#!/bin/bash
# Test local-provider trust setup without invoking carina or AWS.
#
# Usage:
#   bash acceptance-tests/shell-tests/trusted_local_provider.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HELPERS_SH="$SCRIPT_DIR/../shared/_helpers.sh"
WORK_DIR="$(mktemp -d)"
WORK_DIR="$(cd "$WORK_DIR" && pwd -P)"

cleanup() {
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

mkdir -p "$WORK_DIR/providers" "$WORK_DIR/relative"
AWS_WASM="$WORK_DIR/providers/aws.wasm"
AWSCC_WASM="$WORK_DIR/providers/awscc.wasm"
RELATIVE_AWS_WASM="$WORK_DIR/relative/aws.wasm"
OPERATOR_WASM="$WORK_DIR/providers/operator.wasm"
COLON_AWS_WASM="$WORK_DIR/providers/aws:local.wasm"
touch "$AWS_WASM" "$AWSCC_WASM" "$RELATIVE_AWS_WASM" "$OPERATOR_WASM" "$COLON_AWS_WASM"
MIXED_CRN="$WORK_DIR/mixed.crn"
printf '%s\n' 'provider aws {' '}' 'provider awscc {' '}' > "$MIXED_CRN"

read_exported_grant() {
    local working_dir="$1"
    local aws_wasm="$2"
    local awscc_wasm="$3"
    local operator_value="$4"
    local config_file="${5:-}"
    local aws_block
    local awscc_block

    (
        cd "$working_dir"
        CARINA_BIN=/bin/true
        _TEST_AWS_WASM="$aws_wasm"
        if [ -n "$awscc_wasm" ]; then
            _TEST_AWSCC_WASM="$awscc_wasm"
        else
            unset _TEST_AWSCC_WASM
        fi
        if [ -n "$operator_value" ]; then
            CARINA_TRUST_LOCAL_PROVIDER="$operator_value"
        else
            unset CARINA_TRUST_LOCAL_PROVIDER
        fi

        source "$HELPERS_SH"

        if [ -n "$config_file" ]; then
            injected_dir=""
            inject_provider_source "$config_file" injected_dir
            grep -F "source = \"file://$_TEST_AWS_WASM\"" "$injected_dir/main.crn" >/dev/null
            grep -F "source = \"file://$_TEST_AWSCC_WASM\"" "$injected_dir/main.crn" >/dev/null
            aws_block=$(sed -n '/^provider aws {/,/^}/p' "$injected_dir/main.crn")
            if ! grep -Fq "  version = \"$PROVIDER_VERSION\"" <<< "$aws_block"; then
                echo "provider aws block did not receive the aws provider version" >&2
                return 1
            fi
            awscc_block=$(sed -n '/^provider awscc {/,/^}/p' "$injected_dir/main.crn")
            if grep -q '^  version = ' <<< "$awscc_block"; then
                echo "provider awscc block unexpectedly received the aws provider version" >&2
                return 1
            fi
            rm -rf "$injected_dir"
        fi

        # A fresh process sees only exported variables, just like carina does.
        bash -c 'printf "%s\n" "$CARINA_TRUST_LOCAL_PROVIDER"'
    )
}

assert_equal() {
    local description="$1"
    local expected="$2"
    local actual="$3"

    if [ "$actual" != "$expected" ]; then
        echo "FAIL: $description" >&2
        echo "  expected: $expected" >&2
        echo "  actual:   $actual" >&2
        exit 1
    fi
}

actual="$(read_exported_grant "$WORK_DIR" "$AWS_WASM" "" "")"
assert_equal "single provider" "$AWS_WASM=aws" "$actual"

actual="$(read_exported_grant "$WORK_DIR" "$AWS_WASM" "$AWSCC_WASM" "" "$MIXED_CRN")"
assert_equal "two providers" "$AWS_WASM=aws:$AWSCC_WASM=aws" "$actual"

actual="$(read_exported_grant "$WORK_DIR" "relative/aws.wasm" "" "")"
assert_equal "relative provider path" "$RELATIVE_AWS_WASM=aws" "$actual"

operator_value="$OPERATOR_WASM=github"
actual="$(read_exported_grant "$WORK_DIR" "$AWS_WASM" "" "$operator_value")"
assert_equal "operator preset" "$operator_value:$AWS_WASM=aws" "$actual"

missing_output=""
missing_status=0
missing_output="$(read_exported_grant "$WORK_DIR" "providers/missing.wasm" "" "" 2>&1)" || missing_status=$?
if [ "$missing_status" -eq 0 ]; then
    echo "FAIL: missing provider path unexpectedly succeeded" >&2
    exit 1
fi
if [[ "$missing_output" != *"configured provider WASM does not exist (_TEST_AWS_WASM)"* ]]; then
    echo "FAIL: missing provider error was not clear" >&2
    echo "  output: $missing_output" >&2
    exit 1
fi

colon_output=""
colon_status=0
colon_output="$(read_exported_grant "$WORK_DIR" "$COLON_AWS_WASM" "" "" 2>&1)" || colon_status=$?
if [ "$colon_status" -eq 0 ]; then
    echo "FAIL: provider path containing ':' unexpectedly succeeded" >&2
    exit 1
fi
if [[ "$colon_output" != *"ERROR: _TEST_AWS_WASM provider WASM path contains ':' and cannot be used in CARINA_TRUST_LOCAL_PROVIDER:"* ]]; then
    echo "FAIL: provider path containing ':' did not name the configured variable" >&2
    echo "  output: $colon_output" >&2
    exit 1
fi

echo "PASS: trusted local provider grants"
