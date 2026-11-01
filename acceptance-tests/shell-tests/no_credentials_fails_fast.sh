#!/bin/bash
# Verify that a real provider WASM fails promptly when no AWS credentials exist.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd -P)"
AWS_WASM="$PROJECT_ROOT/target/wasm32-wasip2/release/carina-provider-aws.wasm"
CARINA_BIN="${CARINA_BIN:-carina}"

if [[ "$CARINA_BIN" == */* ]]; then
    CARINA_DIR="$(cd "$(dirname "$CARINA_BIN")" && pwd -P)"
    CARINA_EXE="$CARINA_DIR/$(basename "$CARINA_BIN")"
else
    CARINA_EXE="$(command -v "$CARINA_BIN" || true)"
    CARINA_DIR="$(dirname "$CARINA_EXE")"
fi

if [ ! -x "$CARINA_EXE" ]; then
    echo "ERROR: carina binary not found or not executable: $CARINA_BIN" >&2
    exit 1
fi
if [ ! -f "$AWS_WASM" ]; then
    echo "ERROR: release provider WASM not found: $AWS_WASM" >&2
    exit 1
fi

PROVIDER_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$PROJECT_ROOT/Cargo.toml" | head -1)
if [ -z "$PROVIDER_VERSION" ]; then
    echo "ERROR: provider version not found in Cargo.toml" >&2
    exit 1
fi

WORK_DIR="$(mktemp -d)"
WORK_DIR="$(cd "$WORK_DIR" && pwd -P)"
TEST_HOME="$WORK_DIR/home"
CASE_DIR="$WORK_DIR/case"
mkdir -p "$TEST_HOME" "$CASE_DIR"

cleanup() {
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT

cat > "$CASE_DIR/main.crn" <<EOF
provider aws {
  source = "file://$AWS_WASM"
  version = "$PROVIDER_VERSION"
  region = aws.Region.ap_northeast_1
}

aws.ec2.Vpc {
  cidr_block = '10.0.0.0/16'
}
EOF

run_clean() {
    # Disable the host's IMDS source so this case has no credentials on EC2 too.
    env -i \
        HOME="$TEST_HOME" \
        PATH="$CARINA_DIR:/usr/bin:/bin" \
        AWS_EC2_METADATA_DISABLED=true \
        CARINA_TRUST_LOCAL_PROVIDER="$AWS_WASM=aws" \
        CARINA_ACCEPT_EGRESS="file://$AWS_WASM" \
        "$@"
}

(
    cd "$CASE_DIR"
    run_clean "$CARINA_EXE" init . >/dev/null
)

start=$(date +%s)
set +e
output=$(
    cd "$CASE_DIR"
    run_clean perl -e 'alarm shift; exec @ARGV' 60 \
        "$CARINA_EXE" apply --auto-approve . 2>&1
)
status=$?
set -e
elapsed=$(($(date +%s) - start))

if [ "$status" -eq 142 ] || [ "$status" -eq 14 ]; then
    echo "FAIL: carina apply was killed by the 60-second alarm" >&2
    echo "$output" >&2
    exit 1
fi
if [ "$status" -eq 0 ]; then
    echo "FAIL: carina apply unexpectedly succeeded without credentials" >&2
    echo "$output" >&2
    exit 1
fi
if [ "$elapsed" -ge 60 ]; then
    echo "FAIL: carina apply did not fail promptly (${elapsed}s)" >&2
    echo "$output" >&2
    exit 1
fi
if ! grep -Eiq 'no credentials|credentials were not found|CredentialsNotLoaded|credentials not loaded|credential provider was not enabled' <<< "$output"; then
    echo "FAIL: carina apply did not report a credentials failure" >&2
    echo "$output" >&2
    exit 1
fi
if grep -Fq 'Retrying' <<< "$output"; then
    echo "FAIL: carina apply retried a non-transient credentials failure" >&2
    echo "$output" >&2
    exit 1
fi

echo "PASS: missing credentials failed without retries in ${elapsed}s"
