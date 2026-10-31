#!/bin/bash
# Test state helpers against schema-v1 JSON emitted by `carina state show --json`.
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
ABSENT_STATE_JSON="$TEST_DIR/absent-state.json"
EMPTY_STATE_JSON="$TEST_DIR/empty-state.json"
SCHEMA_V2_JSON="$TEST_DIR/schema-v2.json"
STUB_BIN="$TEST_DIR/carina"
AWS_WASM="$TEST_DIR/provider.wasm"
STDOUT_FILE="$TEST_DIR/helper.stdout"
STDERR_FILE="$TEST_DIR/helper.stderr"
mkdir -p "$STATE_DIR"
touch "$AWS_WASM"

cat > "$STATE_JSON" <<'JSON'
{
  "schema_version": 1,
  "state": "present",
  "resources": [
    {
      "binding": "source",
      "identity": "source-bucket",
      "provider": "aws",
      "resource_type": "s3.Bucket",
      "protected": false,
      "current": {
        "kind": "applied",
        "identifier": "zeta-id",
        "attributes": {
          "bucket": "source-bucket",
          "tags": {"Environment": "test"}
        }
      },
      "deposed": []
    },
    {
      "binding": "consumer",
      "identity": "consumer-role",
      "provider": "aws",
      "resource_type": "iam.Role",
      "protected": false,
      "current": {
        "kind": "applied",
        "identifier": "alpha-id",
        "attributes": {
          "role_name": "consumer-role",
          "description": "from-schema-v1"
        }
      },
      "deposed": []
    }
  ]
}
JSON

jq '.state = "absent" | .resources = []' "$STATE_JSON" > "$ABSENT_STATE_JSON"
jq '.resources = []' "$STATE_JSON" > "$EMPTY_STATE_JSON"
jq '.schema_version = 2' "$STATE_JSON" > "$SCHEMA_V2_JSON"

cat > "$STUB_BIN" <<'STUB'
#!/bin/bash
set -euo pipefail

if [ "${CARINA_TRUST_LOCAL_PROVIDER:-}" != "$EXPECTED_TRUST_GRANT" ]; then
    echo "missing trust grant: ${CARINA_TRUST_LOCAL_PROVIDER:-<unset>}" >&2
    exit 1
fi
if [ "${CARINA_ACCEPT_EGRESS:-}" != "$EXPECTED_ACCEPT_EGRESS" ]; then
    echo "missing egress acceptance: ${CARINA_ACCEPT_EGRESS:-<unset>}" >&2
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
    unset _TEST_AWSCC_WASM CARINA_ACCEPT_EGRESS CARINA_TRUST_LOCAL_PROVIDER
    export EXPECTED_STATE_DIR="$STATE_DIR"
    export EXPECTED_STATE_JSON="$STATE_JSON"
    export EXPECTED_ACCEPT_EGRESS="file://$AWS_WASM"
    export EXPECTED_TRUST_GRANT="$AWS_WASM=aws"

    source "$HELPERS_SH"

    state_json="$(carina_state_json "$STATE_DIR" 2>"$STDERR_FILE")"
    assert_equal "state JSON schema version" "1" \
        "$(jq -r '.schema_version' <<< "$state_json")"
    assert_not_contains "state JSON stdout stays clean" "$state_json" "stub warning"
    assert_contains "state JSON preserves warning on stderr" \
        "$(<"$STDERR_FILE")" "stub warning on stderr"

    export EXPECTED_STATE_JSON="$SCHEMA_V2_JSON"
    assert_helper_failure "unsupported state schema" "schema_version must be 1" \
        carina_state_json "$STATE_DIR"
    export EXPECTED_STATE_JSON="$STATE_JSON"

    identifiers="$(carina_state_identifiers "$STATE_DIR" 2>"$STDERR_FILE")"
    assert_equal "sorted schema-v1 identifiers" $'alpha-id\nzeta-id' "$identifiers"
    assert_not_contains "identifier stdout stays clean" "$identifiers" "stub warning"
    assert_contains "identifier read preserves warning on stderr" \
        "$(<"$STDERR_FILE")" "stub warning on stderr"

    description="$(carina_state_query "$STATE_DIR" \
        '.resources[] | select(.binding == "consumer") | .current.attributes.description' \
        2>"$STDERR_FILE")"
    assert_equal "schema-v1 current attribute" "from-schema-v1" "$description"
    assert_not_contains "query stdout stays clean" "$description" "stub warning"
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
    jq '.resources[0].current.identifier = null' \
        "$STATE_JSON" > "$MISSING_IDENTIFIER_JSON"
    export EXPECTED_STATE_JSON="$MISSING_IDENTIFIER_JSON"

    assert_helper_failure "applied row missing identifier" "missing current.identifier" \
        carina_state_identifiers "$STATE_DIR"
    assert_contains "missing identifier names its binding" \
        "$(<"$STDERR_FILE")" 'binding "source"'

    NON_APPLIED_JSON="$TEST_DIR/non-applied.json"
    jq '
        .resources[0].current = {"kind": "none", "identifier": null, "attributes": {}}
        | .resources[1].current = {
            "kind": "create_in_flight",
            "identifier": "alpha-id",
            "attributes": {}
          }
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

    export EXPECTED_STATE_JSON="$EMPTY_STATE_JSON"

    assert_helper_failure "empty state" "state contains no resources" \
        carina_state_identifiers "$STATE_DIR"

    export EXPECTED_STATE_JSON="$STATE_JSON"
    export STUB_FAIL=1
    : > "$STDOUT_FILE"
    : > "$STDERR_FILE"
    state_json_rc=0
    carina_state_json "$STATE_DIR" > "$STDOUT_FILE" 2> "$STDERR_FILE" || state_json_rc=$?
    assert_equal "state JSON failure preserves exit code" "23" "$state_json_rc"
    assert_equal "state JSON failure keeps stdout empty" "" "$(<"$STDOUT_FILE")"
    assert_contains "state JSON failure reports the stub error" \
        "$(<"$STDERR_FILE")" "stub carina state failure"
    assert_helper_failure "state query failure" "stub carina state failure" \
        carina_state_query "$STATE_DIR" '.schema_version'
    assert_helper_failure "identifier failure" "stub carina state failure" \
        carina_state_identifiers "$STATE_DIR"

    unset STUB_FAIL
    export EXPECTED_STATE_JSON="$ABSENT_STATE_JSON"
    TEST_PASSED=0
    TEST_FAILED=0
    assert_state_resource_count "assert: strict zero-resource read" "0" "$STATE_DIR" \
        > "$STDOUT_FILE" 2> "$STDERR_FILE"
    assert_equal "absent-state resource count cannot pass" "0" "$TEST_PASSED"
    assert_equal "absent-state resource count records failure" "1" "$TEST_FAILED"
    assert_contains "absent-state resource count reports query failure" \
        "$(<"$STDOUT_FILE")" "FAIL (state query failed)"
    assert_contains "absent-state resource count explains rejection" \
        "$(<"$STDERR_FILE")" "state is absent"

    export EXPECTED_STATE_JSON="$EMPTY_STATE_JSON"
    TEST_PASSED=0
    TEST_FAILED=0
    assert_state_resource_count "assert: zero resources in real state" "0" "$STATE_DIR" \
        > "$STDOUT_FILE" 2> "$STDERR_FILE"
    assert_equal "real empty-state resource count passes" "1" "$TEST_PASSED"
    assert_equal "real empty-state resource count records no failure" "0" "$TEST_FAILED"
)

echo "PASS: state helpers"
