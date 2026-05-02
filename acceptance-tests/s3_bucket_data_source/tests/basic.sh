#!/bin/bash
# Test: s3.Bucket dual-registration (Managed + DataSource in one apply)
source "$(dirname "$0")/../../shared/_helpers.sh"

echo "Test: s3_bucket_data_source dual apply"
echo ""

REGION="ap-northeast-1"
SUFFIX="$(uuidgen | tr -d - | tr 'A-Z' 'a-z' | head -c 8)"
NEW_BUCKET="carina-acc-test-aws-s3-ds-new-${SUFFIX}"
PRE_BUCKET="carina-acc-test-aws-s3-ds-pre-${SUFFIX}"
export NEW_BUCKET PRE_BUCKET

WORK_DIR=$(mktemp -d)
ACTIVE_WORK_DIR="$WORK_DIR"

# Pre-create the bucket Carina will `read`. Cleanup on EXIT (in addition
# to the helper-trap which destroys Carina-managed resources).
cleanup_pre_bucket() {
    aws s3api delete-bucket --bucket "$PRE_BUCKET" --region "$REGION" 2>/dev/null || true
}
trap 'cleanup_pre_bucket; cleanup' EXIT

printf "  %-50s" "setup: pre-create $PRE_BUCKET"
if aws s3api create-bucket \
    --bucket "$PRE_BUCKET" \
    --region "$REGION" \
    --create-bucket-configuration "LocationConstraint=$REGION" > /dev/null 2>&1; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL"
    TEST_FAILED=$((TEST_FAILED + 1))
    finish_test
fi

# Substitute env vars into the .crn template.
envsubst < "$SCRIPT_DIR/basic.crn" > "$WORK_DIR/main.crn"
cd "$WORK_DIR"

# `carina init` resolves `source = "file://..."` into a staged provider
# under `.carina/providers/`. The release CLI requires it before any
# apply/plan/destroy.
prepare_work_dir "$WORK_DIR"
run_step "step0: init (resolve provider)" "$CARINA_BIN" init .

run_step "step1: apply (creates new + reads existing)" "$CARINA_BIN" apply --auto-approve .

# State should hold both resources: new_bucket (Managed) + existing (DataSource).
assert_state_resource_count "assert: 2 resources in state" "2" "$WORK_DIR"

# Locate the data source entry by binding name and verify the computed
# attributes (arn, region, bucket_domain_name, bucket_regional_domain_name,
# hosted_zone_id) match what the lookup table promises.
DS_PATH=".resources[] | select(.binding == \"existing\")"

printf "  %-50s" "assert: existing.bucket = $PRE_BUCKET"
ACTUAL=$(jq -r "$DS_PATH | .attributes.bucket" "$WORK_DIR/carina.state.json" 2>/dev/null)
if [ "$ACTUAL" = "$PRE_BUCKET" ]; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL (got '$ACTUAL')"
    TEST_FAILED=$((TEST_FAILED + 1))
fi

printf "  %-50s" "assert: existing.region = $REGION"
ACTUAL=$(jq -r "$DS_PATH | .attributes.region" "$WORK_DIR/carina.state.json" 2>/dev/null)
if [ "$ACTUAL" = "$REGION" ]; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL (got '$ACTUAL')"
    TEST_FAILED=$((TEST_FAILED + 1))
fi

printf "  %-50s" "assert: existing.arn formatted"
ACTUAL=$(jq -r "$DS_PATH | .attributes.arn" "$WORK_DIR/carina.state.json" 2>/dev/null)
if [ "$ACTUAL" = "arn:aws:s3:::$PRE_BUCKET" ]; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL (got '$ACTUAL')"
    TEST_FAILED=$((TEST_FAILED + 1))
fi

printf "  %-50s" "assert: existing.bucket_domain_name"
ACTUAL=$(jq -r "$DS_PATH | .attributes.bucket_domain_name" "$WORK_DIR/carina.state.json" 2>/dev/null)
if [ "$ACTUAL" = "$PRE_BUCKET.s3.amazonaws.com" ]; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL (got '$ACTUAL')"
    TEST_FAILED=$((TEST_FAILED + 1))
fi

printf "  %-50s" "assert: existing.bucket_regional_domain_name"
ACTUAL=$(jq -r "$DS_PATH | .attributes.bucket_regional_domain_name" "$WORK_DIR/carina.state.json" 2>/dev/null)
if [ "$ACTUAL" = "$PRE_BUCKET.s3.$REGION.amazonaws.com" ]; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL (got '$ACTUAL')"
    TEST_FAILED=$((TEST_FAILED + 1))
fi

# Hosted zone for ap-northeast-1 per
# https://docs.aws.amazon.com/general/latest/gr/s3.html
printf "  %-50s" "assert: existing.hosted_zone_id (ap-northeast-1)"
ACTUAL=$(jq -r "$DS_PATH | .attributes.hosted_zone_id" "$WORK_DIR/carina.state.json" 2>/dev/null)
if [ "$ACTUAL" = "Z2M4EHUR26P7ZW" ]; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL (got '$ACTUAL')"
    TEST_FAILED=$((TEST_FAILED + 1))
fi

# Verify the Managed-side bucket is also recorded.
printf "  %-50s" "assert: new_bucket present in state"
ACTUAL=$(jq -r ".resources[] | select(.binding == \"new_bucket\") | .attributes.bucket" "$WORK_DIR/carina.state.json" 2>/dev/null)
if [ "$ACTUAL" = "$NEW_BUCKET" ]; then
    echo "OK"
    TEST_PASSED=$((TEST_PASSED + 1))
else
    echo "FAIL (got '$ACTUAL')"
    TEST_FAILED=$((TEST_FAILED + 1))
fi

run_step "step2: destroy (only the Managed bucket)" "$CARINA_BIN" destroy --auto-approve .
ACTIVE_WORK_DIR=""
rm -rf "$WORK_DIR"

# pre-bucket cleanup is handled by the EXIT trap.

finish_test
