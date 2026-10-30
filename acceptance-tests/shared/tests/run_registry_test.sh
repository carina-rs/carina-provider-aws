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
export STUB_STATE_LOG="$TEST_TMP/state.log"

cat > "$CARINA_BIN" <<'STUB'
#!/bin/bash
set -euo pipefail

command_name="${1:-}"
if [ "$command_name" = "state" ] && [ "${2:-}" = "show" ] && [ "${3:-}" = "--json" ]; then
    entry="${4:?missing entry path}"
    printf '%s\n' "$entry" >> "$STUB_STATE_LOG"
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

if [ "$command_name" = "init" ] || [ "$command_name" = "validate" ]; then
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

ln -s /usr/bin/true "$STUB_BIN_DIR/vela"

chmod +x "$STUB_BIN_DIR/aws" "$STUB_BIN_DIR/cargo"
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
    local run_id
    local host

    printf '%s\n' 'provider test {}' > "$source_crn"
    run_id="test-${TEST_COUNT}-${index}-99999999"
    run_dir="$(carina_registry_runs_root)/$run_id"
    host=$(hostname)
    mkdir -p "$run_dir"
    jq -n \
        --argjson schema "$CARINA_RUN_REGISTRY_SCHEMA" \
        --arg repo "$CARINA_ACCEPTANCE_REPO" \
        --arg run_id "$run_id" \
        --arg host "$host" \
        --argjson pid 99999999 \
        --arg created_at "2000-01-01T00:00:00Z" \
        '{schema:$schema,repo:$repo,run_id:$run_id,host:$host,pid:$pid,created_at:$created_at}' \
        > "$run_dir/run.json"
    carina_registry_register_entry "$run_dir" "$account" "$index" "$fixture" "$source_crn"
}

new_live_entry() {
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
account_mismatch_source="$TEST_TMP/account-mismatch.crn"
printf '%s\n' 'provider test {}' > "$account_mismatch_source"
if account_mismatch_output=$(
    export AWS_PROFILE="credential-account"
    export CARINA_ACCEPTANCE_ACCOUNT="recorded-account"
    export CARINA_ACCEPTANCE_HOME="$TEST_TMP/account-mismatch-registry"
    export SCRIPT_DIR="$TEST_TMP/custom-suite"
    source "$SHARED_DIR/_helpers.sh"
    register_acceptance_work_dir \
        "s3_bucket/account-mismatch.crn" "$account_mismatch_source" \
        account_mismatch_entry 2>&1
    ); then
    fail "custom-suite registration accepted conflicting registry and credential accounts"
fi
assert_contains "$account_mismatch_output" "CARINA_ACCEPTANCE_ACCOUNT"
assert_contains "$account_mismatch_output" "AWS_PROFILE"
assert_contains "$account_mismatch_output" "recorded-account"
assert_contains "$account_mismatch_output" "credential-account"
[ ! -e "$TEST_TMP/account-mismatch-registry/carina-provider-test/runs" ] || \
    fail "custom-suite mismatch created registry state before rejecting the accounts"
pass "custom suites reject a registry account that differs from AWS_PROFILE"

next_test
standalone_root="$TEST_TMP/standalone-runner"
mkdir -p "$standalone_root/acceptance-tests/shared" \
    "$standalone_root/acceptance-tests/sample" "$standalone_root/bin"
cp "$RUN_TESTS" "$standalone_root/acceptance-tests/run-tests.sh"
cp "$SHARED_DIR/_run_registry.sh" "$standalone_root/acceptance-tests/shared/_run_registry.sh"
cp "$SHARED_DIR/_provider_env.sh" "$standalone_root/acceptance-tests/shared/_provider_env.sh"
printf '%s\n' 'version = "0.0.0"' > "$standalone_root/Cargo.toml"
printf '%s\n' 'provider aws {' '}' \
    > "$standalone_root/acceptance-tests/sample/basic.crn"
standalone_git_log="$standalone_root/git.log"
: > "$standalone_git_log"
printf '%s\n' \
    '#!/bin/bash' \
    'printf "%s\n" "$*" >> "$STUB_GIT_LOG"' \
    'exit 91' \
    > "$standalone_root/bin/git"
chmod +x "$standalone_root/bin/git"
if ! standalone_validate_output=$(
    unset CARINA_ACCEPTANCE_REPO
    PATH="$standalone_root/bin:$PATH" STUB_GIT_LOG="$standalone_git_log" \
        /bin/bash "$standalone_root/acceptance-tests/run-tests.sh" validate sample/basic 2>&1
    ); then
    fail "validate outside a git checkout failed: $standalone_validate_output"
fi
[ ! -s "$standalone_git_log" ] || \
    fail "validate tried to resolve repository identity: $(cat "$standalone_git_log")"
assert_contains "$standalone_validate_output" "Results: 1 passed, 0 failed"
if standalone_registry_output=$(
    unset CARINA_ACCEPTANCE_REPO
    export PATH="$standalone_root/bin:$PATH"
    export STUB_GIT_LOG="$standalone_git_log"
    export PROJECT_ROOT="$standalone_root"
    source "$standalone_root/acceptance-tests/shared/_run_registry.sh"
    carina_registry_runs_root 2>&1
    ); then
    fail "registry lookup outside a git checkout unexpectedly succeeded"
fi
assert_contains "$standalone_registry_output" \
    "ERROR: cannot derive acceptance-test repository name; set CARINA_ACCEPTANCE_REPO"
pass "validate avoids repo resolution and stateful registry lookup reports its own error"

next_test
use_case_home "register-dotdot"
dotdot_source_crn="$TEST_TMP/dotdot-source.crn"
printf '%s\n' 'provider test {}' > "$dotdot_source_crn"
dotdot_run=$(carina_registry_create_run)
dotdot_runs_root=$(carina_registry_runs_root)
mkdir -p "$dotdot_runs_root/path-marker"
dotdot_register_path="$dotdot_runs_root/path-marker/../$(basename "$dotdot_run")"
if dotdot_register_output=$(carina_registry_register_entry \
    "$dotdot_register_path" "account-dotdot" 0 "s3_bucket/basic.crn" \
    "$dotdot_source_crn" 2>&1); then
    fail "registration accepted a run path containing a .. segment: $dotdot_register_output"
fi
assert_contains "$dotdot_register_output" ".."
pass "registration rejects run paths containing .. segments"

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
use_case_home "release-dotdot"
: > "$STUB_STATE_LOG"
dotdot_release_entry=$(new_entry "account-dotdot" "s3_bucket/basic.crn")
dotdot_release_account_dir=$(dirname "$dotdot_release_entry")
mkdir -p "$dotdot_release_account_dir/path-marker"
dotdot_release_path="$dotdot_release_account_dir/path-marker/../$(basename "$dotdot_release_entry")"
if dotdot_release_output=$(carina_registry_release_entry \
    "$dotdot_release_path" 2>&1); then
    fail "release accepted an entry path containing a .. segment"
fi
[ ! -s "$STUB_STATE_LOG" ] || \
    fail "release inspected state before rejecting a .. path segment"
[ -d "$dotdot_release_entry" ] || fail "release removed the entry reached through a .. path"
assert_contains "$dotdot_release_output" ".."
pass "release rejects entry paths containing .. segments before state inspection"

next_test
use_case_home "release-reuses-run"
reuse_source_crn="$TEST_TMP/reuse-source.crn"
printf '%s\n' 'provider test {}' > "$reuse_source_crn"
reuse_run="$(carina_registry_runs_root)/same-run-99999999"
mkdir -p "$reuse_run"
jq -n \
    --arg host "$(hostname)" \
    '{host:$host}' > "$reuse_run/run.json"
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
interleaved_run_a="$interleaved_runs_root/run-a-99999999"
interleaved_run_b="$interleaved_runs_root/run-b-99999999"
mkdir -p "$interleaved_run_a" "$interleaved_run_b/account-b"
jq -n --arg host "$(hostname)" '{host:$host}' > "$interleaved_run_a/run.json"
jq -n --arg host "$(hostname)" '{host:$host}' > "$interleaved_run_b/run.json"
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
use_case_home "live-run"
: > "$STUB_DESTROY_LOG"
live_entry=$(new_live_entry "account-live" "ec2_vpc/live.crn")
printf '%s\n' '{"schema_version":1,"state":"present","resources":[{"identity":"vpc"}]}' > "$live_entry/.stub-state.json"
live_output=$(carina_registry_cleanup --account "account-live") || \
    fail "cleanup failed instead of skipping a live run: $live_output"
[ -d "$live_entry" ] || fail "cleanup destroyed an entry owned by a live process"
[ ! -s "$STUB_DESTROY_LOG" ] || fail "cleanup invoked destroy for an entry owned by a live process"
assert_contains "$live_output" "in progress, skipped"
live_run_dir=$(dirname "$(dirname "$live_entry")")
live_host=$(hostname)
[ "$(jq -r '.host' "$live_run_dir/run.json")" = "$live_host" ] || \
    fail "run metadata did not record the creating host"
[ "$(jq -r '.host' "$live_entry/meta.json")" = "$live_host" ] || \
    fail "entry metadata did not record the creating host"
pass "cleanup skips entries whose creating process is still alive on this host"

next_test
use_case_home "live-run-before-filter"
: > "$STUB_DESTROY_LOG"
live_unmatched_entry=$(new_live_entry "account-live" "ec2_vpc/live-unmatched.crn")
rm -f "$live_unmatched_entry/meta.json"
live_unmatched_output=$(carina_registry_cleanup \
    --account "some-other-account" --filter "some-other-fixture") || \
    fail "cleanup failed while checking a live run before filters: $live_unmatched_output"
[ -d "$live_unmatched_entry" ] || fail "cleanup removed a live meta-less entry"
[ ! -s "$STUB_DESTROY_LOG" ] || fail "cleanup invoked destroy for a live meta-less entry"
assert_contains "$live_unmatched_output" "$live_unmatched_entry"
assert_contains "$live_unmatched_output" "in progress, skipped"
pass "cleanup performs liveness checks before account and fixture filters"

next_test
use_case_home "remote-run"
: > "$STUB_DESTROY_LOG"
remote_entry=$(new_entry "account-remote" "ec2_vpc/remote.crn")
printf '%s\n' '{"schema_version":1,"state":"present","resources":[{"identity":"vpc"}]}' > "$remote_entry/.stub-state.json"
remote_run_dir=$(dirname "$(dirname "$remote_entry")")
jq '.host = "remote.example.invalid"' "$remote_run_dir/run.json" \
    > "$remote_run_dir/run.json.tmp"
mv "$remote_run_dir/run.json.tmp" "$remote_run_dir/run.json"
jq '.host = "remote.example.invalid"' "$remote_entry/meta.json" \
    > "$remote_entry/meta.json.tmp"
mv "$remote_entry/meta.json.tmp" "$remote_entry/meta.json"
remote_output=$(carina_registry_cleanup --account "account-remote") || \
    fail "cleanup failed instead of skipping a run from another host: $remote_output"
[ -d "$remote_entry" ] || fail "cleanup destroyed an entry created on another host"
[ ! -s "$STUB_DESTROY_LOG" ] || fail "cleanup invoked destroy for an entry created on another host"
assert_contains "$remote_output" "remote.example.invalid"
assert_contains "$remote_output" "different host"
assert_contains "$remote_output" "skipped"
pass "cleanup never destroys entries created on another host"

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
use_case_home "unrelated-invalid-meta"
unrelated_invalid_entry=$(new_entry "account-filter" "s3_bucket/unrelated.crn")
printf '%s\n' \
    '{"schema":999,"account":"wrong-account","test":"s3_bucket/unrelated.crn"}' \
    > "$unrelated_invalid_entry/meta.json"
if ! unrelated_invalid_output=$(carina_registry_cleanup \
    --account "account-filter" --filter "ec2_vpc" 2>&1); then
    fail "filtered cleanup validated an unrelated entry: $unrelated_invalid_output"
fi
[ -d "$unrelated_invalid_entry" ] || fail "filtered cleanup removed an unrelated invalid entry"
pass "filtered cleanup ignores invalid metadata for an unrelated fixture"

next_test
use_case_home "filtered-orphaned-meta"
filtered_missing_meta_entry=$(new_entry "account-filter" "s3_bucket/missing.crn" 0)
rm -f "$filtered_missing_meta_entry/meta.json"
filtered_partial_meta_entry=$(new_entry "account-filter" "s3_bucket/partial.crn" 1)
printf '%s' '{"test":' > "$filtered_partial_meta_entry/meta.json"
if ! filtered_orphaned_output=$(carina_registry_cleanup \
    --account "account-filter" --filter "ec2_vpc" 2>&1); then
    fail "filtered cleanup failed on metadata that could not be matched: $filtered_orphaned_output"
fi
[ -d "$filtered_missing_meta_entry" ] || fail "filtered cleanup removed a meta-less entry"
[ -d "$filtered_partial_meta_entry" ] || fail "filtered cleanup removed a half-written entry"
assert_contains "$filtered_orphaned_output" "WARNING"
assert_contains "$filtered_orphaned_output" "$filtered_missing_meta_entry"
assert_contains "$filtered_orphaned_output" "$filtered_partial_meta_entry"
assert_contains "$filtered_orphaned_output" "cannot be matched"
pass "filtered cleanup warns without failing when metadata cannot be matched"

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
use_case_home "no-match"
no_match_entry=$(new_entry "account-match" "s3_bucket/basic.crn")
no_account_match_output=$(carina_registry_cleanup --account "account-other") || \
    fail "cleanup failed when no account matched"
[ "$no_account_match_output" = "No registry entries match" ] || \
    fail "unexpected nonmatching-account output: $no_account_match_output"
no_fixture_match_output=$(carina_registry_cleanup \
    --account "account-match" --filter "ec2_vpc") || \
    fail "cleanup failed when no fixture matched"
[ "$no_fixture_match_output" = "No registry entries match" ] || \
    fail "unexpected nonmatching-fixture output: $no_fixture_match_output"
[ -d "$no_match_entry" ] || fail "nonmatching cleanup removed the registry entry"
pass "cleanup distinguishes nonmatching entries from an empty registry"

next_test
use_case_home "empty"
empty_output=$(carina_registry_cleanup) || fail "empty cleanup failed"
[ "$empty_output" = "No registered entries" ] || fail "unexpected empty-registry output: $empty_output"
pass "empty cleanup reports no registered entries"

echo "1..$TEST_COUNT"
