#!/bin/bash
# Test state helpers against the v18 JSON emitted by `carina state show --json`.
#
# Usage:
#   bash acceptance-tests/shell-tests/state_helpers.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
HELPERS_SH="$SCRIPT_DIR/../shared/_helpers.sh"
TEST_DIR="$(mktemp -d)"
TEST_DIR="$(cd "$TEST_DIR" && pwd -P)"

cleanup_test_dir() {
    rm -rf "$TEST_DIR"
}
trap cleanup_test_dir EXIT

STATE_DIR="$TEST_DIR/state"
STATE_JSON="$TEST_DIR/state.json"
STUB_BIN="$TEST_DIR/carina"
AWS_WASM="$TEST_DIR/provider.wasm"
STDOUT_FILE="$TEST_DIR/helper.stdout"
STDERR_FILE="$TEST_DIR/helper.stderr"
mkdir -p "$STATE_DIR"
touch "$AWS_WASM"

cat > "$STATE_JSON" <<'JSON'
{
  "version": 18,
  "exports_version": 1,
  "serial": 1,
  "lineage": "state-helper-test",
  "carina_version": "0.6.0",
  "resources": [
    {
      "binding": "source",
      "resource_type": "s3.Bucket",
      "provider": "aws",
      "identity": "source-bucket",
      "current": {
        "kind": "applied",
        "identifier": "zeta-id",
        "attributes": {
          "bucket": "source-bucket",
          "tags": {"Environment": "test"}
        }
      }
    },
    {
      "binding": "consumer",
      "resource_type": "iam.Role",
      "provider": "aws",
      "identity": "consumer-role",
      "current": {
        "kind": "applied",
        "identifier": "alpha-id",
        "attributes": {
          "role_name": "consumer-role",
          "description": "from-state-v18"
        }
      }
    }
  ],
  "exports": {}
}
JSON

cat > "$STUB_BIN" <<'STUB'
#!/bin/bash
set -euo pipefail

if [ "${CARINA_TRUST_LOCAL_PROVIDER:-}" != "$EXPECTED_TRUST_GRANT" ]; then
    echo "missing trust grant: ${CARINA_TRUST_LOCAL_PROVIDER:-<unset>}" >&2
    exit 1
fi
if [ "$#" -ne 4 ] || [ "$1" != "state" ] || [ "$2" != "show" ] || \
   [ "$3" != "--json" ] || [ "$4" != "$EXPECTED_STATE_DIR" ]; then
    echo "unexpected arguments: $*" >&2
    exit 1
fi

echo "stub warning on stderr" >&2
if [ "${STUB_FAIL:-0}" = "1" ]; then
    echo "stub carina state failure" >&2
    exit 23
fi

cat "$EXPECTED_STATE_JSON"
STUB
chmod +x "$STUB_BIN"

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

assert_contains() {
    local description="$1"
    local actual="$2"
    local expected_fragment="$3"

    if [[ "$actual" != *"$expected_fragment"* ]]; then
        echo "FAIL: $description" >&2
        echo "  expected fragment: $expected_fragment" >&2
        echo "  actual:            $actual" >&2
        exit 1
    fi
}

assert_not_contains() {
    local description="$1"
    local actual="$2"
    local unexpected_fragment="$3"

    if [[ "$actual" == *"$unexpected_fragment"* ]]; then
        echo "FAIL: $description" >&2
        echo "  unexpected fragment: $unexpected_fragment" >&2
        echo "  actual:              $actual" >&2
        exit 1
    fi
}

assert_helper_failure() {
    local description="$1"
    local expected_fragment="$2"
    shift 2

    : > "$STDOUT_FILE"
    : > "$STDERR_FILE"
    if "$@" >"$STDOUT_FILE" 2>"$STDERR_FILE"; then
        echo "FAIL: $description unexpectedly succeeded" >&2
        exit 1
    fi

    assert_equal "$description keeps stdout empty" "" "$(<"$STDOUT_FILE")"
    assert_contains "$description reports the expected error" \
        "$(<"$STDERR_FILE")" "$expected_fragment"
}

(
    export CARINA_BIN="$STUB_BIN"
    export _TEST_AWS_WASM="$AWS_WASM"
    unset _TEST_AWSCC_WASM CARINA_TRUST_LOCAL_PROVIDER
    export EXPECTED_STATE_DIR="$STATE_DIR"
    export EXPECTED_STATE_JSON="$STATE_JSON"
    export EXPECTED_TRUST_GRANT="$AWS_WASM=aws"

    source "$HELPERS_SH"

    state_json="$(carina_state_json "$STATE_DIR" 2>"$STDERR_FILE")"
    version="$(jq -r '.version' <<< "$state_json")"
    assert_equal "state JSON version" "18" "$version"
    assert_contains "state JSON preserves warning on stderr" \
        "$(<"$STDERR_FILE")" "stub warning on stderr"

    identifiers="$(carina_state_identifiers "$STATE_DIR" 2>"$STDERR_FILE")"
    assert_equal "sorted v18 identifiers" $'alpha-id\nzeta-id' "$identifiers"
    assert_contains "identifier read preserves warning on stderr" \
        "$(<"$STDERR_FILE")" "stub warning on stderr"

    description="$(carina_state_query "$STATE_DIR" \
        '.resources[] | select(.binding == "consumer") | .current.attributes.description' \
        2>"$STDERR_FILE")"
    assert_equal "v18 current attribute" "from-state-v18" "$description"
    assert_contains "query preserves warning on stderr" \
        "$(<"$STDERR_FILE")" "stub warning on stderr"

    assert_helper_failure "multiple state values" \
        "returned 2 values (expected exactly one)" \
        carina_state_query "$STATE_DIR" '.resources[] | .current.kind'

    assert_helper_failure "missing state value" "state query returned no value" \
        carina_state_query "$STATE_DIR" \
        '.resources[] | select(.binding == "consumer") | .current.attributes.missing'

    EMPTY_ATTRIBUTE_JSON="$TEST_DIR/empty-attribute.json"
    jq '.resources[1].current.attributes.description = ""' \
        "$STATE_JSON" > "$EMPTY_ATTRIBUTE_JSON"
    export EXPECTED_STATE_JSON="$EMPTY_ATTRIBUTE_JSON"

    assert_helper_failure "empty state attribute" "state query returned no value" \
        carina_state_query "$STATE_DIR" \
        '.resources[] | select(.binding == "consumer") | .current.attributes.description'

    MISSING_IDENTIFIER_JSON="$TEST_DIR/missing-identifier.json"
    jq '.resources[0].current |= del(.identifier)' \
        "$STATE_JSON" > "$MISSING_IDENTIFIER_JSON"
    export EXPECTED_STATE_JSON="$MISSING_IDENTIFIER_JSON"

    assert_helper_failure "applied row missing identifier" "missing current.identifier" \
        carina_state_identifiers "$STATE_DIR"
    assert_contains "missing identifier names its binding" \
        "$(<"$STDERR_FILE")" 'binding "source"'

    NON_APPLIED_JSON="$TEST_DIR/non-applied.json"
    jq '
        .resources[0].current = {"kind": "none"}
        | .resources[1].current = {"kind": "create_in_flight"}
    ' "$STATE_JSON" > "$NON_APPLIED_JSON"
    export EXPECTED_STATE_JSON="$NON_APPLIED_JSON"

    assert_helper_failure "non-applied resource rows" \
        "state contains non-applied resource rows" \
        carina_state_identifiers "$STATE_DIR"
    assert_contains "none row names binding, identity, and kind" "$(<"$STDERR_FILE")" \
        'binding "source" identity "source-bucket" has current.kind "none"'
    assert_contains "create-in-flight row names binding, identity, and kind" "$(<"$STDERR_FILE")" \
        'binding "consumer" identity "consumer-role" has current.kind "create_in_flight"'
    assert_not_contains "non-applied rows are not reported as missing identifiers" \
        "$(<"$STDERR_FILE")" "missing current.identifier"

    EMPTY_STATE_JSON="$TEST_DIR/empty-state.json"
    jq '.resources = []' "$STATE_JSON" > "$EMPTY_STATE_JSON"
    export EXPECTED_STATE_JSON="$EMPTY_STATE_JSON"

    assert_helper_failure "empty state" "state contains no resources" \
        carina_state_identifiers "$STATE_DIR"

    export EXPECTED_STATE_JSON="$STATE_JSON"
    export STUB_FAIL=1
    assert_helper_failure "state JSON failure" "stub carina state failure" \
        carina_state_json "$STATE_DIR"
    assert_helper_failure "state query failure" "stub carina state failure" \
        carina_state_query "$STATE_DIR" '.version'
    assert_helper_failure "identifier failure" "stub carina state failure" \
        carina_state_identifiers "$STATE_DIR"
)

echo "PASS: state helpers"
