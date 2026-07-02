#!/bin/bash
# Runs iam_roles acceptance tests.
#
# Custom-runner directories are skipped by acceptance-tests/run-tests.sh,
# matching s3_bucket_data_source. Resolve credentials before invoking this
# suite; the Carina WASM provider cannot use AWS_PROFILE/SSO directly.
# Examples:
#   eval "$(aws configure export-credentials --profile carina-test-000 --format env)"
#   ./run.sh [filter]
#   with_account_creds "carina-test-000" ./run.sh [filter]
#
# The custom path pre-creates the data-source target outside Carina to avoid
# carina-rs/carina#3666, then runs init/apply/plan-verify/destroy.
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FILTER="${1:-}"

TOTAL_PASSED=0
TOTAL_FAILED=0
TESTS_RUN=0

echo "iam_roles acceptance tests"
echo "========================================"

for test_file in "$SCRIPT_DIR"/tests/*.sh; do
    test_name="$(basename "$test_file" .sh)"
    if [ -n "$FILTER" ] && ! echo "$test_name" | grep -q "$FILTER"; then
        continue
    fi
    echo ""
    TESTS_RUN=$((TESTS_RUN + 1))
    if bash "$test_file"; then
        TOTAL_PASSED=$((TOTAL_PASSED + 1))
    else
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
    fi
done

echo ""
echo "========================================"
echo "Tests run: $TESTS_RUN, $TOTAL_PASSED passed, $TOTAL_FAILED failed"
echo "========================================"

[ "$TOTAL_FAILED" -gt 0 ] && exit 1
