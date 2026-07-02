#!/bin/bash
# Test: sts_caller_identity data source read with a managed consumer role.
source "$(dirname "$0")/../../shared/_helpers.sh"

echo "Test: sts_caller_identity data source"
echo ""

CONSUMER_ROLE_NAME="carina-acc-test-sts-caller-consumer"

WORK_DIR=$(mktemp -d)
ACTIVE_WORK_DIR="$WORK_DIR"
cp "$SCRIPT_DIR/basic.crn" "$WORK_DIR/main.crn"
cd "$WORK_DIR"

prepare_work_dir "$WORK_DIR"
run_step "step0: init (resolve provider)" "$CARINA_BIN" init .
run_step "step1: apply (read data source)" "$CARINA_BIN" apply --auto-approve .

# carina#3266 prunes data-source read artifact rows from state.resources;
# only the managed consumer role is persisted.
assert_state_resource_count "assert: 1 resource in state" "1" "$WORK_DIR"

# account_id should be a 12-digit number persisted through the consumer role.
printf "  %-50s" "assert: consumer.description is 12 digits"
ACCOUNT_ID=$(jq -r '.resources[] | select(.attributes.role_name == "'"$CONSUMER_ROLE_NAME"'") | .attributes.description' "$WORK_DIR/carina.state.json" 2>/dev/null)
if echo "$ACCOUNT_ID" | grep -qE '^[0-9]{12}$'; then
    echo "OK ($ACCOUNT_ID)"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL (got '$ACCOUNT_ID')"
    TEST_FAILED=$((TEST_FAILED + 1))
fi

# arn should start with arn:aws and be persisted through the consumer role tag.
printf "  %-50s" "assert: consumer CallerArn starts with arn:aws"
ARN=$(jq -r '.resources[] | select(.attributes.role_name == "'"$CONSUMER_ROLE_NAME"'") | .attributes.tags.CallerArn' "$WORK_DIR/carina.state.json" 2>/dev/null)
if echo "$ARN" | grep -q '^arn:aws'; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL (got '$ARN')"
    TEST_FAILED=$((TEST_FAILED + 1))
fi

run_step "step3: destroy (consumer role only)" "$CARINA_BIN" destroy --auto-approve .
ACTIVE_WORK_DIR=""
rm -rf "$WORK_DIR"

finish_test
