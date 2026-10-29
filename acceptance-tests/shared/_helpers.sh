#!/bin/bash
# Common helpers for acceptance tests
#
# Usage: source this file from test scripts:
#   source "$(dirname "$0")/../../shared/_helpers.sh"

set -e

HELPERS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Derive SCRIPT_DIR from the caller's location (two levels up from shared/)
# Each test script is at <suite>/tests/<script>.sh, so SCRIPT_DIR is <suite>/
if [ -z "$SCRIPT_DIR" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[1]}")/.." && pwd)"
fi
PROJECT_ROOT="$(cd "$HELPERS_DIR/../.." && pwd)"

# See run-tests.sh for the rationale: `$PROJECT_ROOT/..` breaks when
# the script is invoked from a `git worktree` worktree
# (carina-rs/carina-provider-aws#360). Resolve the carina-rs sibling
# base via the canonical .git directory so worktree and main checkouts
# both produce the same path.
resolve_sibling_base() {
    local git_common_dir
    git_common_dir="$(cd "$PROJECT_ROOT" && cd "$(git rev-parse --git-common-dir)" && pwd)"
    dirname "$(dirname "$git_common_dir")"
}

if [ -z "${CARINA_BIN:-}" ]; then
    SIBLING_BASE="$(resolve_sibling_base)"
    if [ -f "$SIBLING_BASE/carina/target/debug/carina" ]; then
        CARINA_BIN="$SIBLING_BASE/carina/target/debug/carina"
    elif command -v carina &>/dev/null; then
        CARINA_BIN="$(command -v carina)"
    else
        echo "ERROR: carina binary not found. Set CARINA_BIN or install carina."
        exit 1
    fi
fi

# ── Provider source injection and local-provider trust ───────────────
# Use WASM provider binaries (carina CLI only supports WASM providers)
# Build with: cargo build -p carina-provider-aws --target wasm32-wasip2 --release
source "$HELPERS_DIR/_provider_env.sh" || exit 1

# inject_provider_source_dir: Inject local provider metadata into all .crn files in a directory (in-place).
# The aws block gets source/version; awscc gets source only.
# Idempotent: skips files that already contain a 'source =' line.
# Args: directory
inject_provider_source_dir() {
    local dir="$1"
    local crn_file
    local tmp_file

    if grep -R -q '^provider awscc {' "$dir" 2>/dev/null; then
        ensure_awscc_wasm || return 1
    fi

    while IFS= read -r -d '' crn_file; do
        if grep -q '^[[:space:]]*source[[:space:]]*=' "$crn_file" 2>/dev/null; then
            continue
        fi

        tmp_file=$(mktemp "$crn_file.tmp.XXXXXX") || return 1
        if ! cp -p "$crn_file" "$tmp_file" ||
           ! _inject_provider_source_filter < "$crn_file" > "$tmp_file" ||
           ! mv "$tmp_file" "$crn_file"; then
            rm -f "$tmp_file"
            return 1
        fi
    done < <(find "$dir" -name '*.crn' -print0)
}

TEST_PASSED=0
TEST_FAILED=0

# carina_work_dir_initialized: Return 0 when the work dir already has init artifacts.
# Args: work_dir
carina_work_dir_initialized() {
    local work_dir="$1"

    [ -d "$work_dir/.carina/providers" ] || [ -f "$work_dir/carina-backend.lock" ]
}

# prepare_work_dir: Inject provider source and ensure the work directory is initialized.
# Call this after copying .crn files to the work dir but before running carina commands.
# Args: work_dir
prepare_work_dir() {
    if [ "$#" -ne 1 ]; then
        echo "ERROR: prepare_work_dir expects exactly one work directory" >&2
        return 2
    fi

    local work_dir="$1"

    inject_provider_source_dir "$work_dir" || return 1

    if carina_work_dir_initialized "$work_dir"; then
        return 0
    fi

    (cd "$work_dir" && "$CARINA_BIN" init . >/dev/null)
}

ACTIVE_WORK_DIR=""
cleanup() {
    if [ -n "$ACTIVE_WORK_DIR" ] && [ -d "$ACTIVE_WORK_DIR" ] && \
       compgen -G "$ACTIVE_WORK_DIR/*.crn" > /dev/null; then
        echo "  Cleanup: destroying resources..."
        cd "$ACTIVE_WORK_DIR"
        "$CARINA_BIN" destroy --auto-approve . 2>/dev/null || true
        "$CARINA_BIN" destroy --auto-approve . 2>/dev/null || true
        rm -f carina.state.json carina-backend.lock carina-providers.lock
    fi
}
trap cleanup EXIT

run_step() {
    local description="$1"
    shift
    # Ensure provider source is injected into .crn files in the current directory
    prepare_work_dir "$(pwd)"
    printf "  %-50s" "$description"
    if "$@" > /dev/null 2>&1; then
        echo "OK"
        TEST_PASSED=$((TEST_PASSED + 1))
    else
        echo "FAIL"
        TEST_FAILED=$((TEST_FAILED + 1))
    fi
}

# carina_state_json: Read local state through carina's supported interface.
# Args: work_dir
carina_state_json() {
    "$CARINA_BIN" state show --json "$1"
}

# carina_state_query: Extract exactly one non-empty value from local state.
# Args: work_dir jq_query
carina_state_query() {
    local work_dir="$1"
    local jq_query="$2"
    local state_json

    state_json=$(carina_state_json "$work_dir") || return $?

    jq -r --arg state_query "$jq_query" "
        [$jq_query] as \$values
        | if (\$values | length) == 0
            or any(\$values[]; . == null or (type == \"string\" and length == 0)) then
            error(\"state query returned no value: \" + \$state_query)
          elif (\$values | length) != 1 then
            error(\"state query returned \" + ((\$values | length) | tostring)
                + \" values (expected exactly one): \" + \$state_query)
          else
            \$values[0]
          end
    " <<< "$state_json"
}

# carina_state_identifiers: Print every current v18 resource identifier, sorted.
# Args: work_dir
carina_state_identifiers() {
    local state_json
    local identifiers
    state_json=$(carina_state_json "$1") || return $?
    if ! identifiers=$(jq -r '
        .resources as $resources
        | if ($resources | length) == 0 then
            error("state contains no resources")
          else .
          end
        | [$resources[]
            | select(.current.kind != "applied")
            | "binding \((.binding // "<missing>") | tojson) identity \((.identity // "<missing>") | tojson) has current.kind \((.current.kind // "<missing>") | tojson)"
          ] as $non_applied
        | if ($non_applied | length) != 0 then
            error("state contains non-applied resource rows: " + ($non_applied | join("; ")))
          else .
          end
        | [$resources[]
            | select((.current.identifier | type) != "string" or .current.identifier == "")
            | "binding \((.binding // "<missing>") | tojson) identity \((.identity // "<missing>") | tojson)"
          ] as $missing_identifiers
        | if ($missing_identifiers | length) != 0 then
            error("state applied resource is missing current.identifier: "
                + ($missing_identifiers | join("; ")))
          else .
          end
        | $resources[].current.identifier
    ' <<< "$state_json"); then
        return 1
    fi

    printf '%s\n' "$identifiers" | LC_ALL=C sort
}

assert_state_value() {
    local description="$1"
    local jq_query="$2"
    local expected="$3"
    local work_dir="$4"

    printf "  %-50s" "$description"
    local actual
    if ! actual=$(carina_state_query "$work_dir" "$jq_query"); then
        echo "FAIL (state query failed)"
        TEST_FAILED=$((TEST_FAILED + 1))
        return 0
    fi
    if [ "$actual" = "$expected" ]; then
        echo "OK"
        TEST_PASSED=$((TEST_PASSED + 1))
    else
        echo "FAIL (expected '$expected', got '$actual')"
        TEST_FAILED=$((TEST_FAILED + 1))
    fi
}

assert_state_resource_count() {
    local description="$1"
    local expected="$2"
    local work_dir="$3"

    printf "  %-50s" "$description"
    local actual
    if ! actual=$(carina_state_query "$work_dir" '.resources | length'); then
        echo "FAIL (state query failed)"
        TEST_FAILED=$((TEST_FAILED + 1))
        return 0
    fi
    if [ "$actual" = "$expected" ]; then
        echo "OK"
        TEST_PASSED=$((TEST_PASSED + 1))
    else
        echo "FAIL (expected '$expected', got '$actual')"
        TEST_FAILED=$((TEST_FAILED + 1))
    fi
}

# Print test results and exit with appropriate code
finish_test() {
    echo ""
    echo "  Results: $TEST_PASSED passed, $TEST_FAILED failed"
    if [ "$TEST_FAILED" -gt 0 ]; then
        exit 1
    fi
}
