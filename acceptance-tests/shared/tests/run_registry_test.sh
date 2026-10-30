#!/bin/bash

set -euo pipefail

TEST_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHARED_DIR="$(cd "$TEST_DIR/.." && pwd)"
RUN_TESTS="$SHARED_DIR/../run-tests.sh"
TEST_TMP="$(mktemp -d)"
trap 'rm -rf "$TEST_TMP"' EXIT

export CARINA_ACCEPTANCE_REPO="carina-provider-test"
export CARINA_BIN="$TEST_TMP/carina-stub"
export STUB_DESTROY_LOG="$TEST_TMP/destroy.log"
export STUB_CREDS_LOG="$TEST_TMP/creds.log"

cat > "$CARINA_BIN" <<'STUB'
#!/bin/bash
set -euo pipefail

command_name="${1:-}"
if [ "$command_name" = "state" ] && [ "${2:-}" = "show" ] && [ "${3:-}" = "--json" ]; then
    entry="${4:?missing entry path}"
    if [ -f "$entry/.stub-state-error" ]; then
        echo "stub state failure" >&2
        exit 17
    fi
    if [ -f "$entry/.stub-state.json" ]; then
        cat "$entry/.stub-state.json"
    else
        printf '%s\n' '{"schema_version":1,"state":"absent"}'
    fi
    exit 0
fi

if [ "$command_name" = "destroy" ]; then
    entry="${@: -1}"
    if [ -f "$entry/.stub-destroy-fail" ]; then
        echo "stub destroy failure" >&2
        exit 23
    fi
    printf '%s\n' "${STUB_ACCOUNT:-missing}" >> "$STUB_DESTROY_LOG"
    printf '%s\n' '{"schema_version":1,"state":"absent"}' > "$entry/.stub-state.json"
    exit 0
fi

echo "unexpected stub invocation: $*" >&2
exit 64
STUB
chmod +x "$CARINA_BIN"

STUB_BIN_DIR="$TEST_TMP/bin"
mkdir -p "$STUB_BIN_DIR"

cat > "$STUB_BIN_DIR/aws" <<'STUB'
#!/bin/bash
set -euo pipefail

if [ "${1:-}" = "configure" ] && [ "${2:-}" = "export-credentials" ] && \
   [ "${3:-}" = "--profile" ] && [ -n "${4:-}" ] && \
   [ "${5:-}" = "--format" ] && [ "${6:-}" = "env-no-export" ]; then
    printf '%s\n' "$4" >> "$STUB_CREDS_LOG"
    printf '%s\n' \
        'AWS_ACCESS_KEY_ID=stub-access-key' \
        'AWS_SECRET_ACCESS_KEY=stub-secret-key' \
        'AWS_SESSION_TOKEN=stub-session-token' \
        "STUB_ACCOUNT=$4"
    exit 0
fi

echo "unexpected aws stub invocation: $*" >&2
exit 64
STUB

cat > "$STUB_BIN_DIR/cargo" <<'STUB'
#!/bin/bash
exit 0
STUB

cat > "$STUB_BIN_DIR/vela" <<'STUB'
#!/bin/bash
exit 0
STUB

chmod +x "$STUB_BIN_DIR/aws" "$STUB_BIN_DIR/cargo" "$STUB_BIN_DIR/vela"
export PATH="$STUB_BIN_DIR:$PATH"

export _TEST_AWS_WASM="$TEST_TMP/carina-provider-aws.wasm"
: > "$_TEST_AWS_WASM"

# The cleanup implementation calls this hook. Keeping it in the test avoids
# loading AWS credentials or invoking the AWS CLI.
with_account_creds() {
    local account="$1"
    shift
    STUB_ACCOUNT="$account" "$@"
}

source "$SHARED_DIR/_run_registry.sh"

TEST_COUNT=0

fail() {
    echo "not ok $TEST_COUNT - $*" >&2
    exit 1
}

pass() {
    echo "ok $TEST_COUNT - $*"
}

next_test() {
    TEST_COUNT=$((TEST_COUNT + 1))
}

assert_contains() {
    local haystack="$1"
    local needle="$2"
    case "$haystack" in
        *"$needle"*) ;;
        *) fail "expected output to contain '$needle'; got: $haystack" ;;
    esac
}

use_case_home() {
    local name="$1"
    export CARINA_ACCEPTANCE_HOME="$TEST_TMP/$name"
    mkdir -p "$CARINA_ACCEPTANCE_HOME"
}

new_entry() {
    local account="$1"
    local fixture="$2"
    local index="${3:-0}"
    local source_crn="$TEST_TMP/source.crn"
    local run_dir

    printf '%s\n' 'provider test {}' > "$source_crn"
    run_dir=$(carina_registry_create_run)
    carina_registry_register_entry "$run_dir" "$account" "$index" "$fixture" "$source_crn"
}

next_test
registry_main="$TEST_TMP/carina-provider-worktree-test"
registry_worktree="$TEST_TMP/registry-linked-worktree"
git init -q -b main "$registry_main"
git -C "$registry_main" \
    -c user.name="Acceptance Test" \
    -c user.email="acceptance-test@example.invalid" \
    commit --allow-empty -qm "initial commit"
git -C "$registry_main" worktree add -q --detach "$registry_worktree" HEAD
main_repo_name=$(
    unset CARINA_ACCEPTANCE_REPO
    PROJECT_ROOT="$registry_main"
    carina_registry_repo_name
)
worktree_repo_name=$(
    unset CARINA_ACCEPTANCE_REPO
    PROJECT_ROOT="$registry_worktree"
    carina_registry_repo_name
)
[ "$main_repo_name" = "$(basename "$registry_main")" ] || \
    fail "main checkout derived the wrong repository name: $main_repo_name"
[ "$worktree_repo_name" = "$main_repo_name" ] || \
    fail "worktree derived '$worktree_repo_name' instead of '$main_repo_name'"
pass "repository name is identical from the main checkout and a worktree"

next_test
use_case_home "release"
absent_entry=$(new_entry "account-a" "s3_bucket/basic.crn" 0)
carina_registry_release_entry "$absent_entry" || fail "absent state was not released"
[ ! -e "$absent_entry" ] || fail "absent entry still exists"

empty_entry=$(new_entry "account-a" "s3_bucket/basic.crn" 1)
printf '%s\n' '{"schema_version":1,"state":"present","resources":[]}' > "$empty_entry/.stub-state.json"
carina_registry_release_entry "$empty_entry" || fail "empty present state was not released"
[ ! -e "$empty_entry" ] || fail "empty entry still exists"

nonempty_entry=$(new_entry "account-a" "s3_bucket/basic.crn" 2)
printf '%s\n' '{"schema_version":1,"state":"present","resources":[{"identity":"kept"}]}' > "$nonempty_entry/.stub-state.json"
if carina_registry_release_entry "$nonempty_entry" >"$TEST_TMP/nonempty.out" 2>&1; then
    fail "non-empty state was released"
fi
[ -d "$nonempty_entry" ] || fail "non-empty entry was removed"
assert_contains "$(cat "$TEST_TMP/nonempty.out")" "$nonempty_entry"
pass "release accepts only absent or empty state"

next_test
use_case_home "release-reuses-run"
reuse_source_crn="$TEST_TMP/reuse-source.crn"
printf '%s\n' 'provider test {}' > "$reuse_source_crn"
reuse_run="$(carina_registry_runs_root)/same-run"
mkdir -p "$reuse_run"
if ! reuse_entry_a=$(carina_registry_register_entry \
    "$reuse_run" "account-reuse" 0 "sqs_queue/basic.crn" "$reuse_source_crn"); then
    fail "could not register the first entry in a reusable run"
fi
carina_registry_release_entry "$reuse_entry_a" || \
    fail "could not release the first entry in a reusable run"
if ! reuse_entry_b=$(carina_registry_register_entry \
    "$reuse_run" "account-reuse" 1 "logs_log_group/basic.crn" "$reuse_source_crn"); then
    fail "could not register a second entry after releasing the first entry in the same run"
fi
[ -d "$reuse_entry_b" ] || fail "second entry in the reused run does not exist"
pass "release leaves a run available for its next entry"

next_test
use_case_home "release-interleaved-runs"
interleaved_source_crn="$TEST_TMP/interleaved-source.crn"
printf '%s\n' 'provider test {}' > "$interleaved_source_crn"
interleaved_runs_root=$(carina_registry_runs_root)
interleaved_run_a="$interleaved_runs_root/run-a"
interleaved_run_b="$interleaved_runs_root/run-b"
mkdir -p "$interleaved_run_a" "$interleaved_run_b/account-b"
if ! interleaved_entry_a=$(carina_registry_register_entry \
    "$interleaved_run_a" "account-a" 0 "sqs_queue/basic.crn" "$interleaved_source_crn"); then
    fail "could not register an entry in the first interleaved run"
fi
carina_registry_release_entry "$interleaved_entry_a" || \
    fail "could not release the entry in the first interleaved run"
if ! interleaved_entry_b=$(carina_registry_register_entry \
    "$interleaved_run_b" "account-b" 0 "logs_log_group/basic.crn" "$interleaved_source_crn"); then
    fail "release in one run removed another active run"
fi
[ -d "$interleaved_entry_b" ] || fail "entry in the second interleaved run does not exist"
pass "release leaves other active runs available while they are empty"

next_test
use_case_home "cleanup-success"
: > "$STUB_DESTROY_LOG"
cleanup_entry=$(new_entry "account-b" "ec2_vpc/basic.crn")
printf '%s\n' '{"schema_version":1,"state":"present","resources":[{"identity":"vpc"}]}' > "$cleanup_entry/.stub-state.json"
filtered_entry=$(new_entry "account-b" "s3_bucket/basic.crn" 1)
printf '%s\n' '{"schema_version":1,"state":"present","resources":[{"identity":"bucket"}]}' > "$filtered_entry/.stub-state.json"
cleanup_output=$(carina_registry_cleanup \
    --account "account-b" --filter "ec2_vpc") || fail "cleanup failed: $cleanup_output"
[ ! -e "$cleanup_entry" ] || fail "cleanup did not release the entry"
[ -d "$filtered_entry" ] || fail "cleanup ignored the fixture filter"
[ "$(cat "$STUB_DESTROY_LOG")" = "account-b" ] || fail "cleanup used the wrong account"
assert_contains "$cleanup_output" "$cleanup_entry"
pass "cleanup filters fixtures, uses the registered account, and releases the entry"

next_test
use_case_home "run-tests-cleanup-no-filter"
: > "$STUB_DESTROY_LOG"
: > "$STUB_CREDS_LOG"
runner_first_entry=$(new_entry "carina-test-009" "ec2_vpc/basic.crn" 0)
printf '%s\n' '{"schema_version":1,"state":"present","resources":[{"identity":"vpc"}]}' > "$runner_first_entry/.stub-state.json"
runner_second_entry=$(new_entry "carina-test-009" "s3_bucket/basic.crn" 1)
printf '%s\n' '{"schema_version":1,"state":"present","resources":[{"identity":"bucket"}]}' > "$runner_second_entry/.stub-state.json"
if ! runner_output=$(/bin/bash "$RUN_TESTS" --accounts 9-9 cleanup 2>&1); then
    fail "run-tests.sh cleanup without filters failed: $runner_output"
fi
[ ! -e "$runner_first_entry" ] || fail "unfiltered runner cleanup retained the first entry"
[ ! -e "$runner_second_entry" ] || fail "unfiltered runner cleanup retained the second entry"
[ "$(cat "$STUB_DESTROY_LOG")" = $'carina-test-009\ncarina-test-009' ] || \
    fail "unfiltered runner cleanup used the wrong account: $(cat "$STUB_DESTROY_LOG")"
[ "$(cat "$STUB_CREDS_LOG")" = $'carina-test-009\ncarina-test-009' ] || \
    fail "unfiltered runner cleanup did not use stub credentials: $(cat "$STUB_CREDS_LOG")"
pass "run-tests.sh cleanup without filters works under /bin/bash"

next_test
use_case_home "run-tests-cleanup-filter"
: > "$STUB_DESTROY_LOG"
: > "$STUB_CREDS_LOG"
runner_matching_entry=$(new_entry "carina-test-009" "ec2_vpc/basic.crn" 0)
printf '%s\n' '{"schema_version":1,"state":"present","resources":[{"identity":"vpc"}]}' > "$runner_matching_entry/.stub-state.json"
runner_filtered_entry=$(new_entry "carina-test-009" "s3_bucket/basic.crn" 1)
printf '%s\n' '{"schema_version":1,"state":"present","resources":[{"identity":"bucket"}]}' > "$runner_filtered_entry/.stub-state.json"
if ! runner_output=$(/bin/bash "$RUN_TESTS" --accounts 9-9 cleanup ec2_vpc 2>&1); then
    fail "run-tests.sh cleanup with a filter failed: $runner_output"
fi
[ ! -e "$runner_matching_entry" ] || fail "filtered runner cleanup retained the matching entry"
[ -d "$runner_filtered_entry" ] || fail "filtered runner cleanup removed the non-matching entry"
[ "$(cat "$STUB_DESTROY_LOG")" = "carina-test-009" ] || \
    fail "filtered runner cleanup used the wrong account: $(cat "$STUB_DESTROY_LOG")"
[ "$(cat "$STUB_CREDS_LOG")" = "carina-test-009" ] || \
    fail "filtered runner cleanup did not use stub credentials: $(cat "$STUB_CREDS_LOG")"
pass "run-tests.sh cleanup forwards a filter under /bin/bash"

next_test
use_case_home "missing-meta"
missing_meta_entry=$(new_entry "account-c" "iam_role/basic.crn")
rm -f "$missing_meta_entry/meta.json"
if missing_meta_output=$(carina_registry_cleanup --account "account-c" 2>&1); then
    fail "cleanup accepted an entry with missing meta.json"
fi
assert_contains "$missing_meta_output" "$missing_meta_entry"
assert_contains "$missing_meta_output" "meta.json"
pass "cleanup rejects and names an entry with missing metadata"

next_test
use_case_home "invalid-meta"
invalid_meta_entry=$(new_entry "account-c" "iam_role/invalid.crn")
printf '%s\n' '{"schema":999}' > "$invalid_meta_entry/meta.json"
if invalid_meta_output=$(carina_registry_cleanup --account "account-c" 2>&1); then
    fail "cleanup accepted an entry with invalid meta.json"
fi
assert_contains "$invalid_meta_output" "$invalid_meta_entry"
assert_contains "$invalid_meta_output" "invalid meta.json"
pass "cleanup rejects and names an entry with invalid metadata"

next_test
use_case_home "missing-main"
missing_main_entry=$(new_entry "account-d" "sqs_queue/basic.crn")
rm -f "$missing_main_entry/main.crn"
if missing_main_output=$(carina_registry_cleanup --account "account-d" 2>&1); then
    fail "cleanup accepted an entry with missing main.crn"
fi
assert_contains "$missing_main_output" "$missing_main_entry"
assert_contains "$missing_main_output" "main.crn"
pass "cleanup rejects and names an entry with missing configuration"

next_test
use_case_home "destroy-failure"
destroy_failure_entry=$(new_entry "account-e" "ec2_subnet/basic.crn")
printf '%s\n' '{"schema_version":1,"state":"present","resources":[{"identity":"subnet"}]}' > "$destroy_failure_entry/.stub-state.json"
: > "$destroy_failure_entry/.stub-destroy-fail"
if destroy_failure_output=$(carina_registry_cleanup --account "account-e" 2>&1); then
    fail "cleanup ignored a destroy failure"
fi
assert_contains "$destroy_failure_output" "$destroy_failure_entry"
assert_contains "$destroy_failure_output" "destroy"
[ -d "$destroy_failure_entry" ] || fail "destroy failure entry was removed"
pass "cleanup rejects and names an entry when destroy fails"

next_test
use_case_home "state-failure"
state_failure_entry=$(new_entry "account-f" "ec2_route/basic.crn")
printf '%s\n' '{"schema_version":1,"state":"present","resources":[{"identity":"route"}]}' > "$state_failure_entry/.stub-state.json"
: > "$state_failure_entry/.stub-state-error"
if state_failure_output=$(carina_registry_cleanup --account "account-f" 2>&1); then
    fail "cleanup ignored a state inspection failure"
fi
assert_contains "$state_failure_output" "$state_failure_entry"
assert_contains "$state_failure_output" "state check"
[ -d "$state_failure_entry" ] || fail "state failure entry was removed"
pass "cleanup retains and names an entry when state inspection fails"

next_test
use_case_home "empty"
empty_output=$(carina_registry_cleanup) || fail "empty cleanup failed"
[ "$empty_output" = "No registered entries" ] || fail "unexpected empty-registry output: $empty_output"
pass "empty cleanup reports no registered entries"

echo "1..$TEST_COUNT"
