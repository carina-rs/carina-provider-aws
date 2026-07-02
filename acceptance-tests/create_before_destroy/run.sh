#!/bin/bash
# Multi-step acceptance tests for create_before_destroy
#
# Usage:
#   env AWS_PROFILE="<profile>" ./run.sh [filter]
#
# Tests:
#   ec2_vpc - Replacement rejected when no unique_name_attribute exists
#
# Filter (optional): substring to match test names (e.g. "ec2_vpc")
#
# This ec2_vpc case pins core's current MissingNameAttributeError contract.
# Name-free CBD semantics are tracked in carina-rs/carina#3667; positive CBD
# replacement coverage lives in the iam_role steps of this suite.
#
# AWS authentication:
#   Set AWS_PROFILE to one of the carina-test-00X profiles. The script
#   resolves fresh STS credentials from that profile before each carina
#   invocation (via `aws configure export-credentials`), so long-running
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

# Extract all resource identifiers from carina.state.json as a sorted newline-separated string.
# Args: work_dir
# Outputs: sorted identifiers (one per line), or empty if none found
get_identifiers() {
    local work_dir="$1"
    jq -r '.resources[].identifier // empty' "$work_dir/carina.state.json" 2>/dev/null | sort || true
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
    injected_dir=$(inject_provider_source "$src")
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
    ids_after_step1=$(get_identifiers "$work_dir")

    # Swap in step2 config (providers are the same, lock already present)
    swap_crn "$step2" "$work_dir"

    if [ -n "$expected_step2_error" ]; then
        if ! run_expected_error "$work_dir" "step2: plan replace fails as expected" "plan" "$expected_step2_error"; then
            destroy_work_dir "$work_dir"
            rm -rf "$work_dir"
            ACTIVE_WORK_DIR=""
            return 1
        fi

        # Restore the applied config so destroy uses the same resource identity
        # that created the initial VPC.
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

    # Step 2: Apply modified config (triggers create_before_destroy replacement)
    if ! run_step "$work_dir" "step2: apply replace (create_before_destroy)" "apply" "--auto-approve"; then
        destroy_work_dir "$work_dir"
        rm -rf "$work_dir"
        ACTIVE_WORK_DIR=""
        return 1
    fi

    # Capture identifiers after step 2
    local ids_after_step2
    ids_after_step2=$(get_identifiers "$work_dir")

    # Assert identifiers changed (create_before_destroy should replace at least one resource)
    if ! assert_identifiers "assert: identifiers changed after replace" "$ids_after_step1" "$ids_after_step2" "different"; then
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

# Test 1: EC2 VPC - CBD rejects schemas without unique_name_attribute
# ec2.Vpc has no unique_name_attribute; cidr_block is create-only.
run_test "ec2_vpc" \
    "$SCRIPT_DIR/ec2_vpc_step1.crn" \
    "$SCRIPT_DIR/ec2_vpc_step2.crn" \
    "Test 1: EC2 VPC (missing unique_name_attribute error)" \
    "has no unique_name_attribute"

echo "════════════════════════════════════════"
echo "Total: $TOTAL_PASSED passed, $TOTAL_FAILED failed"
echo "════════════════════════════════════════"

if [ $TOTAL_FAILED -gt 0 ]; then
    exit 1
fi
