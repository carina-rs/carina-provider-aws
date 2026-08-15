#!/bin/bash
# Test: managed s3.Bucket read-only outputs
source "$(dirname "$0")/../../shared/_helpers.sh"

echo "Test: s3_bucket_outputs managed read-only outputs"
echo ""

REGION="ap-northeast-1"
SUFFIX="$(uuidgen | tr -d - | tr 'A-Z' 'a-z' | head -c 8)"
SOURCE_BUCKET="carina-acc-test-aws-s3-source-${SUFFIX}"
CONSUMER_BUCKET="carina-acc-test-aws-s3-consumer-${SUFFIX}"
export SOURCE_BUCKET CONSUMER_BUCKET

WORK_DIR=$(mktemp -d)
ACTIVE_WORK_DIR="$WORK_DIR"
trap cleanup EXIT

envsubst < "$SCRIPT_DIR/basic.crn" > "$WORK_DIR/main.crn"
cd "$WORK_DIR"

prepare_work_dir "$WORK_DIR"
run_step "step0: init (resolve provider)" "$CARINA_BIN" init .
run_step "step1: apply (create source + consumer)" "$CARINA_BIN" apply --auto-approve .

assert_state_resource_count "assert: 2 resources in state" "2" "$WORK_DIR"

SOURCE_PATH=".resources[] | select(.attributes.bucket == \"$SOURCE_BUCKET\")"
CONSUMER_PATH=".resources[] | select(.attributes.bucket == \"$CONSUMER_BUCKET\")"

assert_state_value "assert: source arn" \
    "$SOURCE_PATH | .attributes.arn" \
    "arn:aws:s3:::$SOURCE_BUCKET" \
    "$WORK_DIR"
assert_state_value "assert: source region" \
    "$SOURCE_PATH | .attributes.region" \
    "$REGION" \
    "$WORK_DIR"
assert_state_value "assert: source domain name" \
    "$SOURCE_PATH | .attributes.bucket_domain_name" \
    "$SOURCE_BUCKET.s3.amazonaws.com" \
    "$WORK_DIR"
assert_state_value "assert: source regional domain name" \
    "$SOURCE_PATH | .attributes.bucket_regional_domain_name" \
    "$SOURCE_BUCKET.s3.$REGION.amazonaws.com" \
    "$WORK_DIR"
assert_state_value "assert: source hosted zone id" \
    "$SOURCE_PATH | .attributes.hosted_zone_id" \
    "Z2M4EHUR26P7ZW" \
    "$WORK_DIR"

assert_state_value "assert: consumer tag SourceArn" \
    "$CONSUMER_PATH | .attributes.tags.SourceArn" \
    "arn:aws:s3:::$SOURCE_BUCKET" \
    "$WORK_DIR"
assert_state_value "assert: consumer tag SourceRegion" \
    "$CONSUMER_PATH | .attributes.tags.SourceRegion" \
    "$REGION" \
    "$WORK_DIR"
assert_state_value "assert: consumer tag SourceDomain" \
    "$CONSUMER_PATH | .attributes.tags.SourceDomain" \
    "$SOURCE_BUCKET.s3.amazonaws.com" \
    "$WORK_DIR"
assert_state_value "assert: consumer tag SourceRegionalDomain" \
    "$CONSUMER_PATH | .attributes.tags.SourceRegionalDomain" \
    "$SOURCE_BUCKET.s3.$REGION.amazonaws.com" \
    "$WORK_DIR"
assert_state_value "assert: consumer tag SourceHostedZoneId" \
    "$CONSUMER_PATH | .attributes.tags.SourceHostedZoneId" \
    "Z2M4EHUR26P7ZW" \
    "$WORK_DIR"

run_step "step2: destroy" "$CARINA_BIN" destroy --auto-approve .
ACTIVE_WORK_DIR=""
rm -rf "$WORK_DIR"

finish_test
