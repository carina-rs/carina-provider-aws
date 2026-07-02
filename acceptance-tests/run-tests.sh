#!/bin/bash
# Run acceptance tests for AWS provider
#
# Usage (from project root):
#   ./carina-provider-aws/acceptance-tests/run-tests.sh [options] [command] [filter...]
#
# Options:
#   --accounts START-END  Use only accounts in range (e.g., 0-3, 4-6, 7-9)
#                         Enables concurrent runs with non-overlapping account pools
#
# Commands:
#   validate   - Validate .crn files (default, no AWS credentials needed)
#   plan       - Run plan on .crn files (single account, needs AWS credentials)
#   apply      - Apply .crn files (single account, needs AWS credentials)
#   destroy    - Destroy resources created by .crn files (single account)
#   full       - Run apply+plan-verify+destroy per test, accounts in parallel
#   cleanup    - Run destroy across accounts in parallel (recover from stuck state)
#   deep-cleanup - Scan and remove orphaned AWS resources across accounts
#
# AWS authentication:
#   The runner selects credentials via AWS_PROFILE rather than wrapping in
#   aws-vault. Each carina-test-00X profile shares a single sso-session
#   (`carina`), so the AWS SDK can refresh the SSO access token on its own
#   during long-running operations (see carina-rs/carina-provider-awscc#157).
#
#   Before the first run in a session, log in once:
#     aws sso login --sso-session carina
#
# For validate, no AWS credentials are needed.
# For plan/apply/destroy, wrap with: with_account_creds "<profile>" ./run-tests.sh ...
# For full, AWS_PROFILE is set internally per account (carina-test-000..009).
#
# Filter (optional):
#   One or more substrings to match against test paths.
#   A test is included if it matches ANY of the provided filters (OR logic).
#   When no filter is provided, all tests are included.
#
# Examples:
#   ./run-tests.sh validate                          # validate all
#   ./run-tests.sh validate s3_bucket/basic          # validate specific test
#   ./run-tests.sh full                              # apply+plan-verify+destroy, 10 parallel accounts
#   ./run-tests.sh full ec2_vpc                      # apply+plan-verify+destroy VPC tests only
#   ./run-tests.sh full --accounts 0-3 iam_role      # use only accounts 000-003
#   ./run-tests.sh full --accounts 4-6 ec2_vpc       # concurrent run with accounts 004-006
#   ./run-tests.sh cleanup                           # destroy all matching tests across 10 accounts
#
# Performance:
#   WASM provider loading with debug builds is slow (JIT compilation ~4s per invocation).
#   Use release builds for significantly faster test runs:
#     cd ../carina && cargo build --release
#     export CARINA_BIN="$PWD/../carina/target/release/carina"

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Sibling-repo base for cross-provider lookups ──────────────────────
#
# Sibling provider repos (e.g. `carina-provider-awscc`) and the
# `carina` host binary live next to *this* repo in a flat carina-rs
# checkout. We resolve their base by walking up from the repo's
# canonical git directory rather than from `$PROJECT_ROOT/..`, because
# when the script is invoked from a `git worktree` worktree under
# `.worktrees/`, the parent of `$PROJECT_ROOT` is the `.worktrees/`
# directory — not the carina-rs root (carina-rs/carina-provider-aws#360).
#
# `git rev-parse --git-common-dir` returns the *main* repo's `.git`
# path even from inside a linked worktree, so `dirname` of that gives
# the repo root, and one more `dirname` gives the carina-rs root.
# `cd && pwd` canonicalizes both relative (`.git` from the main
# checkout) and absolute (full path from a worktree) forms into the
# same absolute path.
GIT_COMMON_DIR="$(cd "$(git -C "$PROJECT_ROOT" rev-parse --git-common-dir)" && pwd)"
REPO_ROOT="$(dirname "$GIT_COMMON_DIR")"
SIBLING_BASE="$(dirname "$REPO_ROOT")"

# ── Parse options ─────────────────────────────────────────────────────
ACCOUNT_START=""
ACCOUNT_END=""

# Parse --accounts before command
ARGS=()
while [ $# -gt 0 ]; do
    case "$1" in
        --accounts)
            if [ -z "${2:-}" ]; then
                echo "ERROR: --accounts requires a range argument (e.g., 0-3)"
                exit 1
            fi
            RANGE="$2"
            ACCOUNT_START="${RANGE%-*}"
            ACCOUNT_END="${RANGE#*-}"
            if ! echo "$ACCOUNT_START" | grep -q '^[0-9]\{1,\}$' || ! echo "$ACCOUNT_END" | grep -q '^[0-9]\{1,\}$'; then
                echo "ERROR: --accounts range must be START-END (e.g., 0-3)"
                exit 1
            fi
            if [ "$ACCOUNT_START" -gt "$ACCOUNT_END" ]; then
                echo "ERROR: --accounts START must be <= END (got $ACCOUNT_START-$ACCOUNT_END)"
                exit 1
            fi
            if [ "$ACCOUNT_START" -gt 9 ] || [ "$ACCOUNT_END" -gt 9 ]; then
                echo "ERROR: --accounts range must be within 0-9"
                exit 1
            fi
            shift 2
            ;;
        *)
            ARGS+=("$1")
            shift
            ;;
    esac
done

set -- ${ARGS[@]+"${ARGS[@]}"}

COMMAND="${1:-validate}"
shift || true
FILTERS=("$@")

# ── Build account list ────────────────────────────────────────────────
ALL_ACCOUNTS=(
    carina-test-000
    carina-test-001
    carina-test-002
    carina-test-003
    carina-test-004
    carina-test-005
    carina-test-006
    carina-test-007
    carina-test-008
    carina-test-009
)

if [ -n "$ACCOUNT_START" ]; then
    ACCOUNTS=()
    ACCOUNT_INDICES=()
    for i in $(seq "$ACCOUNT_START" "$ACCOUNT_END"); do
        ACCOUNTS+=("${ALL_ACCOUNTS[$i]}")
        ACCOUNT_INDICES+=("$i")
    done
else
    ACCOUNTS=("${ALL_ACCOUNTS[@]}")
    ACCOUNT_INDICES=()
    for i in $(seq 0 9); do
        ACCOUNT_INDICES+=("$i")
    done
fi
NUM_ACCOUNTS=${#ACCOUNTS[@]}

# ── Per-account lock files ────────────────────────────────────────────
# Each account gets its own lock to allow concurrent runs with
# non-overlapping account pools.
LOCK_BASE="/tmp/carina-acceptance-tests"
LOCKED_ACCOUNTS=()

acquire_account_lock() {
    local account_index="$1"
    local lock_dir="${LOCK_BASE}-${account_index}.lock"

    if mkdir "$lock_dir" 2>/dev/null; then
        echo $$ > "$lock_dir/pid"
        LOCKED_ACCOUNTS+=("$account_index")
        return 0
    fi

    # Lock exists - check if the holding process is still alive
    if [ -f "$lock_dir/pid" ]; then
        OLD_PID=$(cat "$lock_dir/pid" 2>/dev/null || echo "")
        if [ -n "$OLD_PID" ] && kill -0 "$OLD_PID" 2>/dev/null; then
            echo "ERROR: Account ${account_index} is already locked by another run-tests.sh instance (PID $OLD_PID)"
            return 1
        fi
    fi

    # Stale lock - reclaim
    echo "Removing stale lock for account ${account_index} (previous process no longer running)"
    rm -rf "$lock_dir"
    mkdir "$lock_dir" 2>/dev/null || {
        echo "ERROR: Failed to acquire lock for account ${account_index} (race condition). Retry."
        return 1
    }
    echo $$ > "$lock_dir/pid"
    LOCKED_ACCOUNTS+=("$account_index")
    return 0
}

release_account_locks() {
    for account_index in ${LOCKED_ACCOUNTS[@]+"${LOCKED_ACCOUNTS[@]}"}; do
        rm -rf "${LOCK_BASE}-${account_index}.lock"
    done
    LOCKED_ACCOUNTS=()
}

acquire_all_account_locks() {
    for account_index in "${ACCOUNT_INDICES[@]}"; do
        if ! acquire_account_lock "$account_index"; then
            # Release any locks we already acquired
            release_account_locks
            return 1
        fi
    done
    return 0
}

PIDS=()

if [ "$COMMAND" != "validate" ]; then
    if ! acquire_all_account_locks; then
        exit 1
    fi
    trap 'for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null; done; release_account_locks' EXIT
fi

# Validate command
case "$COMMAND" in
    validate|plan|apply|destroy|full|cleanup|deep-cleanup)
        ;;
    *)
        echo "ERROR: Unknown command '$COMMAND'"
        echo "Usage: $0 [--accounts START-END] [validate|plan|apply|destroy|full|cleanup|deep-cleanup] [filter...]"
        exit 1
        ;;
esac

# ── with_account_creds: resolve the profile's credentials into env vars ──
# and exec the given command. Uses `aws configure export-credentials` so the
# resulting AWS_ACCESS_KEY_ID / AWS_SESSION_TOKEN are fresh STS creds derived
# from the sso-session; the SSO access token in ~/.aws/sso/cache is refreshed
# automatically when it is close to expiring. Resolving creds before each
# carina invocation means every test starts with a new STS session, so a
# single long-running apply/destroy is not limited by the SSO access-token
# lifetime (see carina-rs/carina-provider-awscc#157).
#
# Usage: with_account_creds <account> <cmd> [args...]
with_account_creds() {
    local account="$1"
    shift
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

# ── deep_cleanup_account: scan AWS for orphaned resources ────────────
# Args: account_profile (e.g., "carina-test-000")
deep_cleanup_account() {
    local account="$1"
    local found=0

    # 1. Delete non-default VPCs and all dependencies
    local vpcs
    vpcs=$(with_account_creds "$account" aws ec2 describe-vpcs \
        --filters "Name=isDefault,Values=false" \
        --query 'Vpcs[*].VpcId' --output text 2>/dev/null)

    for vpc_id in $vpcs; do
        [ -z "$vpc_id" ] && continue
        found=$((found + 1))
        echo "  Cleaning VPC $vpc_id..."

        # Delete NAT Gateways (must be first, takes time)
        local nat_gws
        nat_gws=$(with_account_creds "$account" aws ec2 describe-nat-gateways \
            --filter "Name=vpc-id,Values=$vpc_id" "Name=state,Values=available,pending" \
            --query 'NatGateways[*].NatGatewayId' --output text 2>/dev/null)
        for nat_id in $nat_gws; do
            [ -z "$nat_id" ] && continue
            with_account_creds "$account" aws ec2 delete-nat-gateway --nat-gateway-id "$nat_id" 2>/dev/null || true
        done
        # Wait for NAT GWs to delete if any were found
        if [ -n "$nat_gws" ] && [ "$nat_gws" != "None" ]; then
            sleep 30
        fi

        # Delete VPC endpoints
        local endpoints
        endpoints=$(with_account_creds "$account" aws ec2 describe-vpc-endpoints \
            --filters "Name=vpc-id,Values=$vpc_id" \
            --query 'VpcEndpoints[*].VpcEndpointId' --output text 2>/dev/null)
        for ep_id in $endpoints; do
            [ -z "$ep_id" ] && continue
            with_account_creds "$account" aws ec2 delete-vpc-endpoints --vpc-endpoint-ids "$ep_id" 2>/dev/null || true
        done

        # Detach and delete internet gateways
        local igws
        igws=$(with_account_creds "$account" aws ec2 describe-internet-gateways \
            --filters "Name=attachment.vpc-id,Values=$vpc_id" \
            --query 'InternetGateways[*].InternetGatewayId' --output text 2>/dev/null)
        for igw_id in $igws; do
            [ -z "$igw_id" ] && continue
            with_account_creds "$account" aws ec2 detach-internet-gateway --internet-gateway-id "$igw_id" --vpc-id "$vpc_id" 2>/dev/null || true
            with_account_creds "$account" aws ec2 delete-internet-gateway --internet-gateway-id "$igw_id" 2>/dev/null || true
        done

        # Detach and delete VPN gateways
        local vpn_gws
        vpn_gws=$(with_account_creds "$account" aws ec2 describe-vpn-gateways \
            --filters "Name=attachment.vpc-id,Values=$vpc_id" \
            --query 'VpnGateways[*].VpnGatewayId' --output text 2>/dev/null)
        for vpn_id in $vpn_gws; do
            [ -z "$vpn_id" ] && continue
            with_account_creds "$account" aws ec2 detach-vpn-gateway --vpn-gateway-id "$vpn_id" --vpc-id "$vpc_id" 2>/dev/null || true
            with_account_creds "$account" aws ec2 delete-vpn-gateway --vpn-gateway-id "$vpn_id" 2>/dev/null || true
        done

        # Delete subnets
        local subnets
        subnets=$(with_account_creds "$account" aws ec2 describe-subnets \
            --filters "Name=vpc-id,Values=$vpc_id" \
            --query 'Subnets[*].SubnetId' --output text 2>/dev/null)
        for subnet_id in $subnets; do
            [ -z "$subnet_id" ] && continue
            with_account_creds "$account" aws ec2 delete-subnet --subnet-id "$subnet_id" 2>/dev/null || true
        done

        # Delete non-main route tables
        local rts
        rts=$(with_account_creds "$account" aws ec2 describe-route-tables \
            --filters "Name=vpc-id,Values=$vpc_id" \
            --query 'RouteTables[?Associations[0].Main!=`true`].RouteTableId' --output text 2>/dev/null)
        for rt_id in $rts; do
            [ -z "$rt_id" ] && continue
            with_account_creds "$account" aws ec2 delete-route-table --route-table-id "$rt_id" 2>/dev/null || true
        done

        # Delete non-default security groups
        local sgs
        sgs=$(with_account_creds "$account" aws ec2 describe-security-groups \
            --filters "Name=vpc-id,Values=$vpc_id" \
            --query 'SecurityGroups[?GroupName!=`default`].GroupId' --output text 2>/dev/null)
        for sg_id in $sgs; do
            [ -z "$sg_id" ] && continue
            with_account_creds "$account" aws ec2 delete-security-group --group-id "$sg_id" 2>/dev/null || true
        done

        # Delete VPC peering connections
        local peerings
        peerings=$(with_account_creds "$account" aws ec2 describe-vpc-peering-connections \
            --filters "Name=requester-vpc-info.vpc-id,Values=$vpc_id" \
            --query 'VpcPeeringConnections[*].VpcPeeringConnectionId' --output text 2>/dev/null)
        for peer_id in $peerings; do
            [ -z "$peer_id" ] && continue
            with_account_creds "$account" aws ec2 delete-vpc-peering-connection --vpc-peering-connection-id "$peer_id" 2>/dev/null || true
        done

        # Finally delete the VPC
        with_account_creds "$account" aws ec2 delete-vpc --vpc-id "$vpc_id" 2>/dev/null || true
    done

    # 2. Delete orphaned S3 buckets matching carina-acc-test*
    local buckets
    buckets=$(with_account_creds "$account" aws s3api list-buckets \
        --query 'Buckets[?starts_with(Name, `carina-acc-test`)].Name' --output text 2>/dev/null)
    for bucket in $buckets; do
        [ -z "$bucket" ] && continue
        found=$((found + 1))
        echo "  Cleaning S3 bucket $bucket..."
        with_account_creds "$account" aws s3 rb "s3://$bucket" --force 2>/dev/null || true
    done

    # 3. Delete orphaned IAM roles
    local roles
    roles=$(with_account_creds "$account" aws iam list-roles \
        --query 'Roles[?contains(RoleName, `acceptance-test`) || contains(RoleName, `carina-acc`)].RoleName' --output text 2>/dev/null)
    for role in $roles; do
        [ -z "$role" ] && continue
        found=$((found + 1))
        echo "  Cleaning IAM role $role..."
        # Detach all policies first
        local policies
        policies=$(with_account_creds "$account" aws iam list-attached-role-policies --role-name "$role" \
            --query 'AttachedPolicies[*].PolicyArn' --output text 2>/dev/null)
        for policy_arn in $policies; do
            [ -z "$policy_arn" ] && continue
            with_account_creds "$account" aws iam detach-role-policy --role-name "$role" --policy-arn "$policy_arn" 2>/dev/null || true
        done
        with_account_creds "$account" aws iam delete-role --role-name "$role" 2>/dev/null || true
    done

    # 4. Delete orphaned log groups
    local log_groups
    log_groups=$(with_account_creds "$account" aws logs describe-log-groups \
        --log-group-name-prefix "/acceptance-test/" \
        --query 'logGroups[*].logGroupName' --output text 2>/dev/null)
    for lg in $log_groups; do
        [ -z "$lg" ] && continue
        found=$((found + 1))
        echo "  Cleaning log group $lg..."
        with_account_creds "$account" aws logs delete-log-group --log-group-name "$lg" 2>/dev/null || true
    done

    # 5. Delete orphaned Route53 hosted zones from record set tests
    local route53_zone_name="carina-acc-test-route53-record-set.example.com."
    local route53_record_name="www.carina-acc-test-route53-record-set.example.com."
    local hosted_zones
    hosted_zones=$(with_account_creds "$account" aws route53 list-hosted-zones-by-name \
        --dns-name "$route53_zone_name" \
        --query "HostedZones[?Name=='$route53_zone_name'].Id" --output text 2>/dev/null)
    for zone_id in $hosted_zones; do
        if [ -z "$zone_id" ] || [ "$zone_id" = "None" ]; then
            continue
        fi
        found=$((found + 1))
        echo "  Cleaning Route53 hosted zone $zone_id..."
        with_account_creds "$account" aws route53 change-resource-record-sets \
            --hosted-zone-id "$zone_id" \
            --change-batch '{"Changes":[{"Action":"DELETE","ResourceRecordSet":{"Name":"'"$route53_record_name"'","Type":"A","TTL":60,"ResourceRecords":[{"Value":"192.0.2.10"}]}}]}' \
            2>/dev/null || true
        with_account_creds "$account" aws route53 delete-hosted-zone --id "$zone_id" 2>/dev/null || true
    done

    # 6. Delete orphaned ACM certificates from certificate tests
    local acm_cert_domain="acceptance-test.carina-rs.example.com"
    local certs
    certs=$(with_account_creds "$account" aws acm list-certificates \
        --region ap-northeast-1 \
        --certificate-statuses PENDING_VALIDATION ISSUED INACTIVE EXPIRED VALIDATION_TIMED_OUT REVOKED FAILED \
        --query "CertificateSummaryList[?DomainName=='$acm_cert_domain'].CertificateArn" --output text 2>/dev/null)
    for cert_arn in $certs; do
        if [ -z "$cert_arn" ] || [ "$cert_arn" = "None" ]; then
            continue
        fi
        found=$((found + 1))
        echo "  Cleaning ACM certificate $cert_arn..."
        with_account_creds "$account" aws acm delete-certificate \
            --region ap-northeast-1 \
            --certificate-arn "$cert_arn" 2>/dev/null || true
    done

    # 7. Delete transit gateways
    local tgws
    tgws=$(with_account_creds "$account" aws ec2 describe-transit-gateways \
        --filters "Name=state,Values=available,pending" \
        --query 'TransitGateways[*].TransitGatewayId' --output text 2>/dev/null)
    for tgw_id in $tgws; do
        [ -z "$tgw_id" ] && continue
        found=$((found + 1))
        echo "  Cleaning transit gateway $tgw_id..."
        # Delete attachments first
        local attachments
        attachments=$(with_account_creds "$account" aws ec2 describe-transit-gateway-attachments \
            --filters "Name=transit-gateway-id,Values=$tgw_id" "Name=state,Values=available" \
            --query 'TransitGatewayAttachments[*].TransitGatewayAttachmentId' --output text 2>/dev/null)
        for att_id in $attachments; do
            [ -z "$att_id" ] && continue
            with_account_creds "$account" aws ec2 delete-transit-gateway-vpc-attachment --transit-gateway-attachment-id "$att_id" 2>/dev/null || true
        done
        with_account_creds "$account" aws ec2 delete-transit-gateway --transit-gateway-id "$tgw_id" 2>/dev/null || true
    done

    # 8. Release Elastic IPs not associated with anything
    local eips
    eips=$(with_account_creds "$account" aws ec2 describe-addresses \
        --query 'Addresses[?AssociationId==null].AllocationId' --output text 2>/dev/null)
    for eip_id in $eips; do
        [ -z "$eip_id" ] && continue
        found=$((found + 1))
        echo "  Releasing Elastic IP $eip_id..."
        with_account_creds "$account" aws ec2 release-address --allocation-id "$eip_id" 2>/dev/null || true
    done

    echo "  $account: $found orphaned resources cleaned"
}

# Build provider binary
echo "Building provider binary..."
cargo build -p carina-provider-aws --target wasm32-wasip2 --release --quiet 2>/dev/null || cargo build -p carina-provider-aws --target wasm32-wasip2 --release

# Optimize WASM with vela (reduces binary size and load time)
if command -v vela &>/dev/null; then
    echo "Optimizing WASM with vela..."
    vela optimize "$PROJECT_ROOT/target/wasm32-wasip2/release/carina-provider-aws.wasm" -o "$PROJECT_ROOT/target/wasm32-wasip2/release/carina-provider-aws.wasm"
else
    echo "Note: vela not found, skipping WASM optimization. Install: cargo install --git https://github.com/mizzy/vela vela-cli"
fi
echo ""

# CARINA_BIN can be set externally (e.g., when running from the monorepo).
if [ -z "${CARINA_BIN:-}" ]; then
    # Prefer release build (WASM JIT is much faster with release)
    if [ -f "$SIBLING_BASE/carina/target/release/carina" ]; then
        CARINA_BIN="$SIBLING_BASE/carina/target/release/carina"
    elif [ -f "$SIBLING_BASE/carina/target/debug/carina" ]; then
        CARINA_BIN="$SIBLING_BASE/carina/target/debug/carina"
    elif command -v carina &>/dev/null; then
        CARINA_BIN="$(command -v carina)"
    else
        echo "ERROR: carina binary not found. Set CARINA_BIN or install carina."
        exit 1
    fi
fi

if [ ! -f "$CARINA_BIN" ]; then
    echo "ERROR: carina binary not found at $CARINA_BIN"
    exit 1
fi

# ── Provider source injection ────────────────────────────────────────
_TEST_AWS_WASM="${_TEST_AWS_WASM:-$PROJECT_ROOT/target/wasm32-wasip2/release/carina-provider-aws.wasm}"
_TEST_AWSCC_WASM="${_TEST_AWSCC_WASM:-$SIBLING_BASE/carina-provider-awscc/target/wasm32-wasip2/release/carina-provider-awscc.wasm}"

# Read provider version from workspace Cargo.toml to avoid hardcoding
PROVIDER_VERSION=$(grep '^version = ' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/version = "\(.*\)"/\1/')

# Validate that provider binaries are WASM components, not native binaries.
# Native binaries are no longer supported and will cause cryptic linker errors.
for bin_var in _TEST_AWS_WASM _TEST_AWSCC_WASM; do
    bin_path="${!bin_var}"
    if [ -n "$bin_path" ] && [ -f "$bin_path" ] && [[ "$bin_path" != *.wasm ]]; then
        echo "ERROR: $bin_var points to a non-WASM binary: $bin_path"
        echo "       Provider binaries must be .wasm components."
        echo "       Unset $bin_var to use the default WASM build, or point it to a .wasm file."
        exit 1
    fi
done

# inject_provider_source: Create a temp copy of a .crn file with source/version
# injected into provider blocks. Prints the temp file path.
# Args: original_crn_file
inject_provider_source() {
    local original="$1"
    local tmp_file
    tmp_file=$(mktemp).crn

    sed \
        -e '/^provider aws {/a\
  source = "file://'"$_TEST_AWS_WASM"'"\
  version = "'"$PROVIDER_VERSION"'"' \
        -e '/^provider awscc {/a\
  source = "file://'"$_TEST_AWSCC_WASM"'"\
  version = "'"$PROVIDER_VERSION"'"' \
        "$original" > "$tmp_file"

    echo "$tmp_file"
}

# Find test files
# matches_any_filter: returns 0 if rel_path matches any filter, or if no filters given
matches_any_filter() {
    local rel_path="$1"
    if [ ${#FILTERS[@]} -eq 0 ]; then
        return 0
    fi
    for f in "${FILTERS[@]}"; do
        if [[ "$rel_path" == *"$f"* ]]; then
            return 0
        fi
    done
    return 1
}

TESTS=()
while IFS= read -r -d '' file; do
    REL_PATH="${file#$SCRIPT_DIR/}"
    if ! matches_any_filter "$REL_PATH"; then
        continue
    fi
    # Skip .crn files in directories with a custom run.sh (multi-step tests)
    DIR_OF_FILE="$(dirname "$file")"
    if [ -f "$DIR_OF_FILE/run.sh" ]; then
        continue
    fi
    TESTS+=("$file")
done < <(find "$SCRIPT_DIR" -name "*.crn" -print0 | sort -z)

if [ ${#TESTS[@]} -eq 0 ]; then
    if [ ${#FILTERS[@]} -gt 0 ]; then
        echo "No test files found matching: ${FILTERS[*]}"
    else
        echo "No test files found"
    fi
    exit 0
fi

# ── deep-cleanup: scan and remove orphaned AWS resources ─────────────
if [ "$COMMAND" = "deep-cleanup" ]; then
    echo "Running deep cleanup to remove orphaned AWS resources across $NUM_ACCOUNTS accounts"
    echo ""

    # Pre-authenticate accounts
    echo "Pre-authenticating AWS accounts..."
    for SLOT in $(seq 0 $((NUM_ACCOUNTS - 1))); do
        ACCOUNT="${ACCOUNTS[$SLOT]}"
        echo "  Authenticating $ACCOUNT..."
        if ! with_account_creds "$ACCOUNT" aws sts get-caller-identity >/dev/null 2>&1; then
            echo "  WARNING: Failed to pre-authenticate $ACCOUNT"
        fi
    done
    echo "Pre-authentication complete."
    echo ""

    PIDS=()
    for SLOT in $(seq 0 $((NUM_ACCOUNTS - 1))); do
        ACCOUNT="${ACCOUNTS[$SLOT]}"
        (
            echo "── $ACCOUNT ──"
            deep_cleanup_account "$ACCOUNT"
        ) &
        PIDS+=($!)
        echo "  [$ACCOUNT] started (PID $!)"
    done

    echo ""
    echo "Waiting for all accounts to finish deep cleanup..."
    echo ""

    OVERALL_EXIT=0
    for PID in ${PIDS[@]+"${PIDS[@]}"}; do
        wait "$PID" || OVERALL_EXIT=1
    done

    echo "════════════════════════════════════════"
    echo "Deep cleanup complete."
    echo "════════════════════════════════════════"

    exit $OVERALL_EXIT
fi

# ── cleanup: destroy across accounts in parallel ─────────────────────
if [ "$COMMAND" = "cleanup" ]; then
    TOTAL=${#TESTS[@]}
    echo "Running cleanup (destroy) on $TOTAL test(s) across $NUM_ACCOUNTS accounts"
    echo ""

    WORK_DIR=$(mktemp -d)
    trap 'for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null; done; rm -rf '"$WORK_DIR"'; release_account_locks' EXIT

    # Pre-authenticate accounts
    echo "Pre-authenticating AWS accounts..."
    for SLOT in $(seq 0 $((NUM_ACCOUNTS - 1))); do
        ACCOUNT="${ACCOUNTS[$SLOT]}"
        echo "  Authenticating $ACCOUNT..."
        if ! with_account_creds "$ACCOUNT" aws sts get-caller-identity >/dev/null 2>&1; then
            echo "  WARNING: Failed to pre-authenticate $ACCOUNT"
        fi
    done
    echo "Pre-authentication complete."
    echo ""

    # Each account tries to destroy all tests (we don't know which account
    # created which resources, so every account attempts every test)
    PIDS=()
    for SLOT in $(seq 0 $((NUM_ACCOUNTS - 1))); do
        ACCOUNT="${ACCOUNTS[$SLOT]}"
        LOG_FILE="$WORK_DIR/slot_${SLOT}.log"
        STATE_DIR="$WORK_DIR/state_${SLOT}"
        mkdir -p "$STATE_DIR"

        (
            set +e
            DESTROYED=0
            SKIPPED=0

            TEST_INDEX=0
            for TEST_FILE in "${TESTS[@]}"; do
                REL_PATH="${TEST_FILE#$SCRIPT_DIR/}"
                INJECTED_FILE=$(inject_provider_source "$TEST_FILE")
                TEST_STATE_DIR="$STATE_DIR/test_${TEST_INDEX}"
                mkdir -p "$TEST_STATE_DIR"
                cp "$INJECTED_FILE" "$TEST_STATE_DIR/main.crn"
                rm -f "$INJECTED_FILE"
                TEST_INDEX=$((TEST_INDEX + 1))
                # Skip if no state file exists — nothing to destroy
                if [ ! -f "$TEST_STATE_DIR/carina.state.json" ]; then
                    SKIPPED=$((SKIPPED + 1))
                    continue
                fi
                echo "RUNNING destroy $REL_PATH"
                DESTROY_OUTPUT=$(cd "$TEST_STATE_DIR" && with_account_creds "$ACCOUNT" "$CARINA_BIN" destroy --auto-approve "$TEST_STATE_DIR" 2>&1)
                DESTROY_RC=$?
                if [ $DESTROY_RC -eq 0 ]; then
                    if echo "$DESTROY_OUTPUT" | grep -q "No resources to destroy"; then
                        SKIPPED=$((SKIPPED + 1))
                    else
                        echo "DESTROYED $REL_PATH"
                        DESTROYED=$((DESTROYED + 1))
                    fi
                else
                    echo "FAIL (destroy) $REL_PATH"
                    echo "  ERROR: $DESTROY_OUTPUT"
                fi
            done

            echo "---"
            echo "SUMMARY $ACCOUNT: $DESTROYED destroyed, $SKIPPED already clean"
        ) > "$LOG_FILE" 2>&1 &

        PIDS+=($!)
        echo "  [$ACCOUNT] started (PID $!)"
    done

    echo ""
    echo "Logs: $WORK_DIR/slot_*.log"
    echo "Waiting for all accounts to finish cleanup..."
    echo ""

    # Wait for all workers
    OVERALL_EXIT=0
    for PID in ${PIDS[@]+"${PIDS[@]}"}; do
        wait "$PID" || OVERALL_EXIT=1
    done

    # Print results per account
    for SLOT in $(seq 0 $((NUM_ACCOUNTS - 1))); do
        LOG_FILE="$WORK_DIR/slot_${SLOT}.log"
        if [ ! -f "$LOG_FILE" ]; then
            continue
        fi
        ACCOUNT="${ACCOUNTS[$SLOT]}"
        echo "── $ACCOUNT ──"
        cat "$LOG_FILE"
        echo ""
    done

    echo "════════════════════════════════════════"
    echo "Cleanup complete."
    echo "════════════════════════════════════════"

    exit $OVERALL_EXIT
fi

# ── full: parallel execution across selected accounts ─────────────────
if [ "$COMMAND" = "full" ]; then
    TOTAL=${#TESTS[@]}
    echo "Running full cycle (apply -> plan-verify -> destroy) on $TOTAL test(s) across $NUM_ACCOUNTS accounts"
    if [ -n "$ACCOUNT_START" ]; then
        echo "  Using accounts: ${ACCOUNTS[*]}"
    fi
    echo ""

    WORK_DIR=$(mktemp -d)

    # Signal handling for the main process: forward signals to workers
    PIDS=()
    cleanup_main() {
        echo ""
        echo "Interrupted! Signaling workers to clean up..."
        for PID in ${PIDS[@]+"${PIDS[@]}"}; do
            kill -TERM "$PID" 2>/dev/null || true
        done
        echo "Waiting for workers to finish cleanup..."
        for PID in ${PIDS[@]+"${PIDS[@]}"}; do
            wait "$PID" 2>/dev/null || true
        done
        echo "All workers finished."
        rm -rf "$WORK_DIR"
        release_account_locks
        exit 1
    }
    trap cleanup_main INT TERM

    # Clean up temp dir and release locks on normal exit
    trap 'for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null; done; rm -rf '"$WORK_DIR"'; release_account_locks' EXIT

    # Pre-authenticate accounts sequentially to avoid opening
    # multiple SSO browser tabs simultaneously
    echo "Pre-authenticating AWS accounts..."
    for SLOT in $(seq 0 $((NUM_ACCOUNTS - 1))); do
        ACCOUNT="${ACCOUNTS[$SLOT]}"
        echo "  Authenticating $ACCOUNT..."
        if ! with_account_creds "$ACCOUNT" aws sts get-caller-identity >/dev/null 2>&1; then
            echo "  WARNING: Failed to pre-authenticate $ACCOUNT"
        fi
    done
    echo "Pre-authentication complete."
    echo ""

    # Deep cleanup: remove orphaned resources before running tests
    echo "Running deep cleanup to remove orphaned resources..."
    for SLOT in $(seq 0 $((NUM_ACCOUNTS - 1))); do
        ACCOUNT="${ACCOUNTS[$SLOT]}"
        deep_cleanup_account "$ACCOUNT" &
    done
    wait
    echo ""

    # Distribute tests round-robin across accounts
    for i in "${!TESTS[@]}"; do
        SLOT=$((i % NUM_ACCOUNTS))
        echo "${TESTS[$i]}" >> "$WORK_DIR/slot_${SLOT}.list"
    done

    # Launch one worker per account
    for SLOT in $(seq 0 $((NUM_ACCOUNTS - 1))); do
        LIST_FILE="$WORK_DIR/slot_${SLOT}.list"
        if [ ! -f "$LIST_FILE" ]; then
            continue
        fi

        ACCOUNT="${ACCOUNTS[$SLOT]}"
        LOG_FILE="$WORK_DIR/slot_${SLOT}.log"
        SLOT_DIR="$WORK_DIR/state_${SLOT}"
        mkdir -p "$SLOT_DIR"

        (
            set +e
            PASSED=0
            FAILED=0
            CURRENT_STATE_DIR=""
            INTERRUPTED=0

            # Worker trap: attempt destroy for the current test on interruption
            worker_cleanup() {
                INTERRUPTED=1
                if [ -n "$CURRENT_STATE_DIR" ]; then
                    echo "INTERRUPTED - destroying resources for current test"
                    cd "$CURRENT_STATE_DIR" && with_account_creds "$ACCOUNT" "$CARINA_BIN" destroy --auto-approve "$CURRENT_STATE_DIR" 2>&1 || true
                fi
            }
            trap worker_cleanup INT TERM

            TEST_INDEX=0
            while IFS= read -r TEST_FILE; do
                if [ $INTERRUPTED -eq 1 ]; then
                    break
                fi

                REL_PATH="${TEST_FILE#$SCRIPT_DIR/}"
                INJECTED_FILE=$(inject_provider_source "$TEST_FILE")

                # Use a separate state directory per test to prevent
                # state file cross-contamination between tests (issue #537)
                CURRENT_STATE_DIR="$SLOT_DIR/test_${TEST_INDEX}"
                mkdir -p "$CURRENT_STATE_DIR"
                cp "$INJECTED_FILE" "$CURRENT_STATE_DIR/main.crn"
                rm -f "$INJECTED_FILE"
                TEST_INDEX=$((TEST_INDEX + 1))

                # Initialize backend lock and install providers before apply
                if ! INIT_OUTPUT=$("$CARINA_BIN" init "$CURRENT_STATE_DIR" 2>&1); then
                    echo "FAIL (init) $REL_PATH"
                    echo "  ERROR: $INIT_OUTPUT"
                    FAILED=$((FAILED + 1))
                    CURRENT_STATE_DIR=""
                    continue
                fi

                # Apply (run from state dir so each test has its own state file)
                echo "RUNNING apply $REL_PATH"
                APPLY_OUTPUT=$(cd "$CURRENT_STATE_DIR" && with_account_creds "$ACCOUNT" "$CARINA_BIN" apply --auto-approve "$CURRENT_STATE_DIR" 2>&1)
                APPLY_RC=$?
                if [ $INTERRUPTED -eq 1 ]; then
                    break
                fi
                if [ $APPLY_RC -ne 0 ] || echo "$APPLY_OUTPUT" | grep -q "failed"; then
                    echo "FAIL (apply) $REL_PATH"
                    echo "  ERROR: $APPLY_OUTPUT"
                    FAILED=$((FAILED + 1))
                    # Try to destroy whatever was partially created
                    cd "$CURRENT_STATE_DIR" && with_account_creds "$ACCOUNT" "$CARINA_BIN" destroy --auto-approve "$CURRENT_STATE_DIR" 2>&1 || true
                    CURRENT_STATE_DIR=""
                    continue
                fi

                # Post-apply plan verification (idempotency check)
                echo "RUNNING plan-verify $REL_PATH"
                PLAN_OUTPUT=$(cd "$CURRENT_STATE_DIR" && with_account_creds "$ACCOUNT" "$CARINA_BIN" plan --detailed-exitcode "$CURRENT_STATE_DIR" 2>&1)
                PLAN_RC=$?
                if [ $INTERRUPTED -eq 1 ]; then
                    break
                fi
                if [ $PLAN_RC -eq 2 ]; then
                    echo "FAIL (plan-verify) $REL_PATH"
                    echo "  ERROR: Post-apply plan detected changes (not idempotent):"
                    echo "  $PLAN_OUTPUT"
                    FAILED=$((FAILED + 1))
                    # Still destroy to clean up
                    cd "$CURRENT_STATE_DIR" && with_account_creds "$ACCOUNT" "$CARINA_BIN" destroy --auto-approve "$CURRENT_STATE_DIR" 2>&1 || true
                    CURRENT_STATE_DIR=""
                    continue
                elif [ $PLAN_RC -ne 0 ]; then
                    echo "FAIL (plan-verify) $REL_PATH"
                    echo "  ERROR: $PLAN_OUTPUT"
                    FAILED=$((FAILED + 1))
                    cd "$CURRENT_STATE_DIR" && with_account_creds "$ACCOUNT" "$CARINA_BIN" destroy --auto-approve "$CURRENT_STATE_DIR" 2>&1 || true
                    CURRENT_STATE_DIR=""
                    continue
                fi

                # Destroy
                echo "RUNNING destroy $REL_PATH"
                DESTROY_OUTPUT=$(cd "$CURRENT_STATE_DIR" && with_account_creds "$ACCOUNT" "$CARINA_BIN" destroy --auto-approve "$CURRENT_STATE_DIR" 2>&1)
                DESTROY_RC=$?
                if [ $DESTROY_RC -ne 0 ] || echo "$DESTROY_OUTPUT" | grep -q "failed"; then
                    echo "FAIL (destroy) $REL_PATH"
                    echo "  ERROR: $DESTROY_OUTPUT"
                    FAILED=$((FAILED + 1))
                    CURRENT_STATE_DIR=""
                    continue
                fi

                echo "OK   $REL_PATH"
                PASSED=$((PASSED + 1))
                CURRENT_STATE_DIR=""
            done < "$LIST_FILE"

            echo "---"
            echo "SUMMARY $ACCOUNT: $PASSED passed, $FAILED failed"
        ) > "$LOG_FILE" 2>&1 &

        PIDS+=($!)
        echo "  [$ACCOUNT] started (PID $!) - $(wc -l < "$LIST_FILE" | tr -d ' ') test(s)"
    done

    echo ""
    echo "Logs: $WORK_DIR/slot_*.log"
    echo "Waiting for all accounts to finish..."
    echo ""

    # Wait and collect results
    OVERALL_EXIT=0
    for SLOT in $(seq 0 $((NUM_ACCOUNTS - 1))); do
        LOG_FILE="$WORK_DIR/slot_${SLOT}.log"
        if [ ! -f "$LOG_FILE" ] && [ $SLOT -ge ${#PIDS[@]} ]; then
            continue
        fi
        PID_IDX=$SLOT
        if [ $PID_IDX -lt ${#PIDS[@]} ]; then
            wait "${PIDS[$PID_IDX]}" || OVERALL_EXIT=1
        fi
    done

    # Print results per account
    for SLOT in $(seq 0 $((NUM_ACCOUNTS - 1))); do
        LOG_FILE="$WORK_DIR/slot_${SLOT}.log"
        if [ ! -f "$LOG_FILE" ]; then
            continue
        fi
        ACCOUNT="${ACCOUNTS[$SLOT]}"
        echo "── $ACCOUNT ──"
        cat "$LOG_FILE"
        echo ""
    done

    # Aggregate
    TOTAL_PASSED=0
    TOTAL_FAILED=0
    for SLOT in $(seq 0 $((NUM_ACCOUNTS - 1))); do
        LOG_FILE="$WORK_DIR/slot_${SLOT}.log"
        if [ ! -f "$LOG_FILE" ]; then
            continue
        fi
        P=$(grep "^SUMMARY" "$LOG_FILE" | sed 's/.*: \([0-9]*\) passed.*/\1/' || echo 0)
        F=$(grep "^SUMMARY" "$LOG_FILE" | sed 's/.*, \([0-9]*\) failed/\1/' || echo 0)
        TOTAL_PASSED=$((TOTAL_PASSED + P))
        TOTAL_FAILED=$((TOTAL_FAILED + F))
    done

    echo "════════════════════════════════════════"
    echo "Total: $TOTAL_PASSED passed, $TOTAL_FAILED failed (of $TOTAL)"
    echo "════════════════════════════════════════"

    exit $OVERALL_EXIT
fi

# ── Single-command mode (validate/plan/apply/destroy) ────────────────
echo "Running '$COMMAND' on ${#TESTS[@]} test file(s):"
echo ""

# Use per-test isolated state directories for commands that touch state
# (plan/apply/destroy) to prevent cross-contamination between tests (issue #839)
if [ "$COMMAND" != "validate" ]; then
    WORK_DIR=$(mktemp -d)
    trap 'for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null; done; rm -rf '"$WORK_DIR"'; release_account_locks' EXIT
fi

PASSED=0
FAILED=0
ERRORS=()
TEST_INDEX=0

for TEST_FILE in "${TESTS[@]}"; do
    REL_PATH="${TEST_FILE#$SCRIPT_DIR/}"
    printf "  %-55s " "$REL_PATH"

    INJECTED_FILE=$(inject_provider_source "$TEST_FILE")

    AUTO_APPROVE=""
    if [ "$COMMAND" = "apply" ] || [ "$COMMAND" = "destroy" ]; then
        AUTO_APPROVE="--auto-approve"
    fi

    # Run from an isolated state directory for stateful commands
    if [ "$COMMAND" != "validate" ]; then
        STATE_DIR="$WORK_DIR/test_${TEST_INDEX}"
        mkdir -p "$STATE_DIR"
        cp "$INJECTED_FILE" "$STATE_DIR/main.crn"
        rm -f "$INJECTED_FILE"
        TEST_INDEX=$((TEST_INDEX + 1))
        TARGET_PATH="$STATE_DIR"

        # Initialize backend lock and install providers before the stateful command
        if ! INIT_OUTPUT=$("$CARINA_BIN" init "$STATE_DIR" 2>&1); then
            echo "FAIL (init)"
            ERRORS+=("$REL_PATH: $INIT_OUTPUT")
            FAILED=$((FAILED + 1))
            continue
        fi
    else
        # For validate, create a temp directory with the injected file
        VALIDATE_DIR=$(mktemp -d)
        cp "$INJECTED_FILE" "$VALIDATE_DIR/main.crn"
        rm -f "$INJECTED_FILE"
        TARGET_PATH="$VALIDATE_DIR"

        # `carina validate` loads the provider WASM to do schema-driven
        # validation, so the provider must be installed first. `carina init`
        # on a file:// source does not require AWS credentials.
        if ! INIT_OUTPUT=$("$CARINA_BIN" init "$VALIDATE_DIR" 2>&1); then
            echo "FAIL (init)"
            ERRORS+=("$REL_PATH: $INIT_OUTPUT")
            FAILED=$((FAILED + 1))
            rm -rf "$VALIDATE_DIR"
            continue
        fi
    fi

    RUN_PREFIX=""
    if [ "$COMMAND" != "validate" ]; then
        RUN_PREFIX="cd $STATE_DIR &&"
    fi

    if OUTPUT=$(eval "$RUN_PREFIX \"$CARINA_BIN\" \"$COMMAND\" $AUTO_APPROVE \"$TARGET_PATH\"" 2>&1); then
        if [ "$COMMAND" = "apply" ]; then
            # Post-apply plan verification (idempotency check)
            # Use || to capture non-zero exit codes without triggering set -e
            PLAN_RC=0
            PLAN_OUTPUT=$(eval "$RUN_PREFIX \"$CARINA_BIN\" plan --detailed-exitcode \"$TARGET_PATH\"" 2>&1) || PLAN_RC=$?
            if [ $PLAN_RC -eq 2 ]; then
                echo "FAIL (plan-verify)"
                ERRORS+=("$REL_PATH: Post-apply plan detected changes (not idempotent): $PLAN_OUTPUT")
                FAILED=$((FAILED + 1))
                continue
            elif [ $PLAN_RC -ne 0 ]; then
                echo "FAIL (plan-verify)"
                ERRORS+=("$REL_PATH: $PLAN_OUTPUT")
                FAILED=$((FAILED + 1))
                continue
            fi
        fi
        echo "OK"
        PASSED=$((PASSED + 1))
    else
        echo "FAIL"
        ERRORS+=("$REL_PATH: $OUTPUT")
        FAILED=$((FAILED + 1))
    fi
    if [ "$COMMAND" = "validate" ]; then
        rm -rf "$VALIDATE_DIR"
    fi
done

echo ""
echo "Results: $PASSED passed, $FAILED failed (total: $((PASSED + FAILED)))"

if [ ${#ERRORS[@]} -gt 0 ]; then
    echo ""
    echo "Failures:"
    for ERR in "${ERRORS[@]}"; do
        echo "  $ERR"
    done
    exit 1
fi
