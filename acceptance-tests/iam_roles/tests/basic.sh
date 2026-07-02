#!/bin/bash
# Test: iam.Roles data source read with a pre-existing IAM role.
source "$(dirname "$0")/../../shared/_helpers.sh"

echo "Test: iam_roles data source"
echo ""

PRE_ROLE_NAME="carina-acc-test-iam-roles-preexisting"
CONSUMER_ROLE_NAME="carina-acc-test-iam-roles-consumer"
ASSUME_ROLE_POLICY='{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Principal":{"Service":"lambda.amazonaws.com"},"Action":"sts:AssumeRole"}]}'

WORK_DIR=$(mktemp -d)
ACTIVE_WORK_DIR="$WORK_DIR"

delete_role_if_exists() {
    local role_name="$1"
    local policy_arns policy_arn policy_names policy_name

    policy_arns=$(aws iam list-attached-role-policies \
        --role-name "$role_name" \
        --query 'AttachedPolicies[*].PolicyArn' \
        --output text 2>/dev/null || true)
    for policy_arn in $policy_arns; do
        [ -z "$policy_arn" ] && continue
        aws iam detach-role-policy --role-name "$role_name" --policy-arn "$policy_arn" 2>/dev/null || true
    done

    policy_names=$(aws iam list-role-policies \
        --role-name "$role_name" \
        --query 'PolicyNames[*]' \
        --output text 2>/dev/null || true)
    for policy_name in $policy_names; do
        [ -z "$policy_name" ] && continue
        aws iam delete-role-policy --role-name "$role_name" --policy-name "$policy_name" 2>/dev/null || true
    done

    aws iam delete-role --role-name "$role_name" 2>/dev/null || true
}

cleanup_pre_role() {
    delete_role_if_exists "$PRE_ROLE_NAME"
}
trap 'cleanup; cleanup_pre_role' EXIT

delete_role_if_exists "$PRE_ROLE_NAME"

printf "  %-50s" "setup: pre-create $PRE_ROLE_NAME"
if aws iam create-role \
    --role-name "$PRE_ROLE_NAME" \
    --assume-role-policy-document "$ASSUME_ROLE_POLICY" \
    --tags Key=Environment,Value=acceptance-test > /dev/null 2>&1 && \
    aws iam wait role-exists --role-name "$PRE_ROLE_NAME" > /dev/null 2>&1; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL"
    TEST_FAILED=$((TEST_FAILED + 1))
    finish_test
fi

cp "$SCRIPT_DIR/basic.crn" "$WORK_DIR/main.crn"
cd "$WORK_DIR"

prepare_work_dir "$WORK_DIR"
run_step "step0: init (resolve provider)" "$CARINA_BIN" init .
run_step "step1: apply (reads existing role)" "$CARINA_BIN" apply --auto-approve .

printf "  %-50s" "step2: plan-verify (no changes)"
PLAN_RC=0
PLAN_OUTPUT=$("$CARINA_BIN" plan --detailed-exitcode . 2>&1) || PLAN_RC=$?
if [ "$PLAN_RC" -eq 0 ]; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL (exit $PLAN_RC)"
    echo "$PLAN_OUTPUT"
    TEST_FAILED=$((TEST_FAILED + 1))
fi

# carina#3266 prunes data-source read artifact rows from state.resources;
# only the managed consumer role is persisted.
assert_state_resource_count "assert: 1 resource in state" "1" "$WORK_DIR"

printf "  %-50s" "assert: consumer.description = $PRE_ROLE_NAME"
ACTUAL=$(jq -r '.resources[] | select(.attributes.role_name == "'"$CONSUMER_ROLE_NAME"'") | .attributes.description' "$WORK_DIR/carina.state.json" 2>/dev/null)
if [ "$ACTUAL" = "$PRE_ROLE_NAME" ]; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL (got '$ACTUAL')"
    TEST_FAILED=$((TEST_FAILED + 1))
fi

run_step "step3: destroy (consumer role only)" "$CARINA_BIN" destroy --auto-approve .
ACTIVE_WORK_DIR=""
rm -rf "$WORK_DIR"

cleanup_pre_role

finish_test
