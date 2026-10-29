#!/bin/bash
# Multi-step acceptance tests for create_before_destroy
#
# Usage:
#   env AWS_PROFILE="<profile>" ./run.sh [filter]
#
# Tests:
#   ec2_vpc - Replacement is rejected for a conflicting resource
#   ec2_security_group - Replacement succeeds with a temporary unique name
#
# Filter (optional): substring to match test names (e.g. "ec2_vpc")
#
# This ec2_vpc case verifies that a Conflicting replacement is rejected at plan
# time because the replacement cannot safely coexist with the original.
# The ec2_security_group case verifies that a UniqueName resource can coexist
# with its original by using a temporary group_name during replacement.
#
# AWS authentication:
#   Set AWS_PROFILE to one of the carina-test-00X profiles. The script
#   resolves fresh STS credentials from that profile before each AWS-facing
#   carina invocation (via `aws configure export-credentials`), so long-running
#   replace/destroy cycles are not bound to a single SSO access-token
#   lifetime (see carina-rs/carina-provider-awscc#157). The first run
#   in a session needs `aws sso login --sso-session carina`.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
FILTER="${1:-}"

source "$SCRIPT_DIR/../shared/_helpers.sh"

TOTAL_PASSED=0
TOTAL_FAILED=0

# ── with_account_creds: resolve credentials for $AWS_PROFILE into env vars ──
# and exec the given command. See run-tests.sh for rationale.
with_account_creds() {
    local account="${AWS_PROFILE:-}"
    if [ -z "$account" ]; then
        echo "ERROR: AWS_PROFILE is not set; cannot resolve credentials" >&2
        return 1
    fi
    local creds
    if ! creds=$(aws configure export-credentials --profile "$account" --format env-no-export 2>&1); then
        echo "ERROR: aws configure export-credentials failed for $account: $creds" >&2
        return 1
    fi
    (
        set -a
        eval "$creds"
        set +a
        unset AWS_PROFILE
        "$@"
    )
}

# Track active work dir for signal cleanup
ACTIVE_WORK_DIR=""

signal_cleanup() {
    if [ -n "$ACTIVE_WORK_DIR" ] && [ -d "$ACTIVE_WORK_DIR" ]; then
        set +e
        echo ""
        echo "Interrupted. Cleaning up resources..."
        (cd "$ACTIVE_WORK_DIR" && with_account_creds "$CARINA_BIN" destroy --auto-approve . 2>&1)
        (cd "$ACTIVE_WORK_DIR" && with_account_creds "$CARINA_BIN" destroy --auto-approve . 2>&1)
        rm -rf "$ACTIVE_WORK_DIR"
        ACTIVE_WORK_DIR=""
    fi
    exit 1
}

trap signal_cleanup INT TERM

run_step() {
    local work_dir="$1"
    local description="$2"
    local command="$3"
    local extra_args="${4:-}"

    printf "  %-55s " "$description"

    local output
    if output=$(cd "$work_dir" && with_account_creds "$CARINA_BIN" $command $extra_args . 2>&1); then
        echo "OK"
        TOTAL_PASSED=$((TOTAL_PASSED + 1))
        return 0
    else
        echo "FAIL"
        echo "  ERROR: $output"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        return 1
    fi
}

run_expected_error() {
    local work_dir="$1"
    local description="$2"
    local command="$3"
    local expected_fragment="$4"
    local extra_args="${5:-}"

    printf "  %-55s " "$description"

    local output
    if output=$(cd "$work_dir" && with_account_creds "$CARINA_BIN" $command $extra_args . 2>&1); then
        echo "FAIL"
        echo "  ERROR: command unexpectedly succeeded; expected error containing:"
        echo "    $expected_fragment"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        return 1
    fi

    if grep -Fq "$expected_fragment" <<< "$output"; then
        echo "OK"
        TOTAL_PASSED=$((TOTAL_PASSED + 1))
        return 0
    fi

    echo "FAIL"
    echo "  ERROR: expected error containing:"
    echo "    $expected_fragment"
    echo "  Actual output:"
    echo "  $output"
    TOTAL_FAILED=$((TOTAL_FAILED + 1))
    return 1
}

# Extract all resource identifiers through `carina state show --json`.
# Args: work_dir
# Outputs: sorted identifiers (one per line); fails if any identifier is missing
get_identifiers() {
    local work_dir="$1"
    carina_state_identifiers "$work_dir"
}

# Assert that two identifier sets match the expected relationship
# Args: description ids_after_step1 ids_after_step2 expected("equal"|"different")
#
# For "different" mode, uses set-based comparison (comm -3) to check if at least
# one identifier differs between the two sets. This handles the case where most
# resources keep the same identifiers but only the replaced resource changes.
assert_identifiers() {
    local description="$1"
    local ids1="$2"
    local ids2="$3"
    local expected="$4"

    printf "  %-55s " "$description"

    if [ -z "$ids1" ] || [ -z "$ids2" ]; then
        echo "FAIL"
        echo "  ERROR: Could not extract identifiers (step1='$ids1', step2='$ids2')"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        return 1
    fi

    if [ "$expected" = "equal" ]; then
        if [ "$ids1" = "$ids2" ]; then
            echo "OK"
            TOTAL_PASSED=$((TOTAL_PASSED + 1))
            return 0
        else
            echo "FAIL"
            echo "  ERROR: Identifiers changed (expected same):"
            echo "    before: $ids1"
            echo "    after:  $ids2"
            TOTAL_FAILED=$((TOTAL_FAILED + 1))
            return 1
        fi
    else
        # Use comm -3 to find lines unique to either set (symmetric difference).
        # If there are any unique lines, at least one identifier changed.
        local diff_lines
        diff_lines=$(comm -3 <(echo "$ids1") <(echo "$ids2"))
        if [ -n "$diff_lines" ]; then
            echo "OK"
            TOTAL_PASSED=$((TOTAL_PASSED + 1))
            return 0
        else
            echo "FAIL"
            echo "  ERROR: Identifiers unchanged (expected at least one to differ): $ids1"
            TOTAL_FAILED=$((TOTAL_FAILED + 1))
            return 1
        fi
    fi
}

run_plan_expect_replace() {
    local work_dir="$1"
    local description="$2"

    printf "  %-55s " "$description"

    local output
    local rc=0
    output=$(cd "$work_dir" && with_account_creds "$CARINA_BIN" plan --detailed-exitcode . 2>&1) || rc=$?

    if [ $rc -ne 2 ]; then
        echo "FAIL"
        echo "  ERROR: expected replacement plan with exit code 2, got $rc:"
        echo "  $output"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        return 1
    fi

    if ! grep -Eq "must be replaced|[1-9][0-9]* to replace" <<< "$output"; then
        echo "FAIL"
        echo "  ERROR: plan exited with changes but did not show a replacement:"
        echo "  $output"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        return 1
    fi

    echo "OK"
    TOTAL_PASSED=$((TOTAL_PASSED + 1))
    return 0
}

assert_ec2_security_group_state() {
    local work_dir="$1"
    local description="$2"
    local base_group_name="$3"
    local expected_description="$4"

    printf "  %-55s " "$description"

    local state_json
    if ! state_json=$(carina_state_json "$work_dir"); then
        echo "FAIL"
        echo "  ERROR: could not read state"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        return 1
    fi

    local summary
    if ! summary=$(jq -r \
        --arg base_group_name "$base_group_name" \
        --arg expected_description "$expected_description" '
        [.resources[] | select(.resource_type == "ec2.SecurityGroup")] as $security_groups
        | ($base_group_name + "-") as $temporary_name_prefix
        | if (($security_groups | length) == 1
            and (($security_groups[0].current.attributes.group_name // "") | startswith($temporary_name_prefix))
            and ($security_groups[0].current.attributes.group_name != $base_group_name)
            and ($security_groups[0].current.attributes.description == $expected_description)) then
            "ok"
          else
            "count=\($security_groups | length) group_names=\($security_groups | map(.current.attributes.group_name // "<missing>") | join(",")) descriptions=\($security_groups | map(.current.attributes.description // "<missing>") | join(","))"
          end
    ' <<< "$state_json" 2>&1); then
        echo "FAIL"
        echo "  ERROR: could not inspect state:"
        echo "  $summary"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        return 1
    fi

    if [ "$summary" = "ok" ]; then
        echo "OK"
        TOTAL_PASSED=$((TOTAL_PASSED + 1))
        return 0
    fi

    echo "FAIL"
    echo "  ERROR: expected exactly 1 ec2.SecurityGroup with a temporary name based on $base_group_name and description '$expected_description', got $summary"
    TOTAL_FAILED=$((TOTAL_FAILED + 1))
    return 1
}

run_plan_verify() {
    local work_dir="$1"
    local description="$2"

    printf "  %-55s " "$description"

    local output
    local rc
    output=$(cd "$work_dir" && with_account_creds "$CARINA_BIN" plan --detailed-exitcode . 2>&1) || rc=$?
    rc=${rc:-0}

    if [ $rc -eq 2 ]; then
        echo "FAIL"
        echo "  ERROR: Post-apply plan detected changes (not idempotent):"
        echo "  $output"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        return 1
    elif [ $rc -ne 0 ]; then
        echo "FAIL"
        echo "  ERROR: $output"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        return 1
    fi

    echo "OK"
    TOTAL_PASSED=$((TOTAL_PASSED + 1))
    return 0
}

# destroy_work_dir: destroy resources in work_dir, retrying to handle dependencies
# Returns 0 if at least one destroy succeeded, 1 if ALL failed
# (Named distinctly from `cleanup` so the EXIT trap in _helpers.sh keeps using
# its own cleanup() — overriding it would call this with no args at exit and
# emit "No .crn files found in .".)
destroy_work_dir() {
    local work_dir="$1"
    local any_success=false

    # Disable set -e to ensure all destroy attempts run
    set +e
    echo "  Cleanup: destroying resources..."
    if (cd "$work_dir" && with_account_creds "$CARINA_BIN" destroy --auto-approve . 2>&1); then
        any_success=true
    fi
    # Retry: resources may have dependencies that prevent deletion on first pass
    if (cd "$work_dir" && with_account_creds "$CARINA_BIN" destroy --auto-approve . 2>&1); then
        any_success=true
    fi
    set -e

    if [ "$any_success" = false ]; then
        return 1
    fi
    return 0
}

# Swap the active .crn into work_dir/main.crn (with provider source injected).
# Args: source_crn target_work_dir
swap_crn() {
    local src="$1"
    local target="$2"
    local injected_dir
    inject_provider_source "$src" injected_dir || return 1
    cp "$injected_dir/main.crn" "$target/main.crn"
    rm -rf "$injected_dir"
}

# Run a single multi-step test
# Args: test_name step1_crn step2_crn description
run_test() {
    local test_name="$1"
    local step1="$2"
    local step2="$3"
    local desc="$4"
    local expected_step2_error="${5:-}"

    # Apply filter
    if [ -n "$FILTER" ] && [[ "$test_name" != *"$FILTER"* ]]; then
        return 0
    fi

    local work_dir
    work_dir=$(mktemp -d)

    # Register for signal cleanup
    ACTIVE_WORK_DIR="$work_dir"

    echo "$desc"
    echo ""

    # Load step1 and initialize (creates backend lock + installs providers)
    swap_crn "$step1" "$work_dir"
    if ! (cd "$work_dir" && with_account_creds "$CARINA_BIN" init . >/dev/null 2>&1); then
        echo "  step1: init                                             FAIL"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        return 1
    fi

    # Step 1: Apply initial config
    if ! run_step "$work_dir" "step1: apply initial" "apply" "--auto-approve"; then
        destroy_work_dir "$work_dir"
        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        return 1
    fi

    # Step 1b: Plan-verify initial state
    if ! run_plan_verify "$work_dir" "step1: plan-verify initial"; then
        destroy_work_dir "$work_dir"
        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        return 1
    fi

    # Capture identifiers after step 1
    local ids_after_step1
    if ! ids_after_step1=$(get_identifiers "$work_dir"); then
        echo "  assert: read identifiers after step 1                 FAIL"
        echo "  ERROR: could not read identifiers after step 1"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        destroy_work_dir "$work_dir"
        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        return 1
    fi

    # Swap in step2 config (providers are the same, lock already present)
    swap_crn "$step2" "$work_dir"

    if [ -n "$expected_step2_error" ]; then
        if ! run_expected_error "$work_dir" "step2: plan replace fails as expected" "plan" "$expected_step2_error"; then
            destroy_work_dir "$work_dir"
            rm -rf "$work_dir"
            ACTIVE_WORK_DIR=""
            return 1
        fi

        # Restore the applied config so destroy uses the same resource identities
        # as the initial apply.
        swap_crn "$step1" "$work_dir"

        if ! destroy_work_dir "$work_dir"; then
            echo "  WARNING: All destroy attempts failed. Preserving work dir for debugging:"
            echo "    $work_dir"
            TOTAL_FAILED=$((TOTAL_FAILED + 1))
            ACTIVE_WORK_DIR=""
            echo ""
            return 1
        fi

        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        echo ""
        return 0
    fi

    # Step 2a: Plan modified config and verify it is a replacement
    if ! run_plan_expect_replace "$work_dir" "step2: plan replace"; then
        destroy_work_dir "$work_dir"
        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        return 1
    fi

    # Step 2: Apply modified config (triggers create_before_destroy replacement)
    if ! run_step "$work_dir" "step2: apply replace (create_before_destroy)" "apply" "--auto-approve"; then
        destroy_work_dir "$work_dir"
        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        return 1
    fi

    # Capture identifiers after step 2
    local ids_after_step2
    if ! ids_after_step2=$(get_identifiers "$work_dir"); then
        echo "  assert: read identifiers after step 2                 FAIL"
        echo "  ERROR: could not read identifiers after step 2"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        destroy_work_dir "$work_dir"
        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        return 1
    fi

    # Assert identifiers changed (create_before_destroy should replace at least one resource)
    if ! assert_identifiers "assert: identifiers changed after replace" "$ids_after_step1" "$ids_after_step2" "different"; then
        destroy_work_dir "$work_dir"
        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        return 1
    fi

    # Assert the Security Group replacement kept its temporary unique name.
    if [ "$test_name" = "ec2_security_group" ] && ! assert_ec2_security_group_state \
        "$work_dir" \
        "assert: one SG with temporary name" \
        "carina-acc-test-cbd-sg" \
        "Carina create_before_destroy replacement security group"; then
        destroy_work_dir "$work_dir"
        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        return 1
    fi

    # Step 3: Plan-verify after replacement
    if ! run_plan_verify "$work_dir" "step3: plan-verify after replace"; then
        destroy_work_dir "$work_dir"
        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        return 1
    fi

    # Step 4: Destroy
    if ! destroy_work_dir "$work_dir"; then
        echo "  WARNING: All destroy attempts failed. Preserving work dir for debugging:"
        echo "    $work_dir"
        TOTAL_FAILED=$((TOTAL_FAILED + 1))
        ACTIVE_WORK_DIR=""
        echo ""
        return 1
    fi

    rm -rf "$work_dir"
    ACTIVE_WORK_DIR=""
    echo ""
}

echo "create_before_destroy multi-step acceptance tests (AWS)"
echo "════════════════════════════════════════"
echo ""

# Test 1: EC2 VPC - CBD fails for a Conflicting replacement
run_test "ec2_vpc" \
    "$SCRIPT_DIR/ec2_vpc_step1.crn" \
    "$SCRIPT_DIR/ec2_vpc_step2.crn" \
    "Test 1: EC2 VPC (conflicting replacement rejected)" \
    "resource type 'aws.ec2.Vpc' does not support create_before_destroy: the replacement cannot coexist with the original" || true

# Test 2: EC2 Security Group - CBD succeeds with a temporary unique name
# group_name is unique within a VPC, so the replacement uses a temporary name.
run_test "ec2_security_group" \
    "$SCRIPT_DIR/ec2_security_group_step1.crn" \
    "$SCRIPT_DIR/ec2_security_group_step2.crn" \
    "Test 2: EC2 Security Group (temporary unique name)" || true

echo "════════════════════════════════════════"
echo "Total: $TOTAL_PASSED passed, $TOTAL_FAILED failed"
echo "════════════════════════════════════════"

if [ $TOTAL_FAILED -gt 0 ]; then
    exit 1
fi
