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

# carina#3266 prunes data-source read artifact rows from state.resources;
# only the managed bucket is persisted. Data-source values are proven
# end-to-end by tags on the managed bucket that consume existing.* outputs.
assert_state_resource_count "assert: 1 resource in state" "1" "$WORK_DIR"

MANAGED_PATH=".resources[] | select(.attributes.bucket == \"$NEW_BUCKET\")"

assert_state_value "assert: new_bucket present in state" \
    "$MANAGED_PATH | .attributes.bucket" \
    "$NEW_BUCKET" \
    "$WORK_DIR"
assert_state_value "assert: tag ExistingBucket = existing.bucket" \
    "$MANAGED_PATH | .attributes.tags.ExistingBucket" \
    "$PRE_BUCKET" \
    "$WORK_DIR"
assert_state_value "assert: tag ExistingRegion = existing.region" \
    "$MANAGED_PATH | .attributes.tags.ExistingRegion" \
    "$REGION" \
    "$WORK_DIR"
assert_state_value "assert: tag ExistingArn = existing.arn" \
    "$MANAGED_PATH | .attributes.tags.ExistingArn" \
    "arn:aws:s3:::$PRE_BUCKET" \
    "$WORK_DIR"
assert_state_value "assert: tag ExistingDomain = existing.bucket_domain_name" \
    "$MANAGED_PATH | .attributes.tags.ExistingDomain" \
    "$PRE_BUCKET.s3.amazonaws.com" \
    "$WORK_DIR"
assert_state_value "assert: tag ExistingRegionalName = existing.bucket_regional_domain_name" \
    "$MANAGED_PATH | .attributes.tags.ExistingRegionalName" \
    "$PRE_BUCKET.s3.$REGION.amazonaws.com" \
    "$WORK_DIR"
# Hosted zone for ap-northeast-1 per
# https://docs.aws.amazon.com/general/latest/gr/s3.html
assert_state_value "assert: tag ExistingHostedZoneId = existing.hosted_zone_id" \
    "$MANAGED_PATH | .attributes.tags.ExistingHostedZoneId" \
    "Z2M4EHUR26P7ZW" \
    "$WORK_DIR"

run_step "step2: destroy (only the Managed bucket)" "$CARINA_BIN" destroy --auto-approve .
ACTIVE_WORK_DIR=""
rm -rf "$WORK_DIR"

# pre-bucket cleanup is handled by the EXIT trap.

finish_test
