#!/bin/bash
# Persistent run registry for Carina acceptance tests.
#
# This file is shared by provider repositories. Keep it repo-agnostic and
# side-effect-free: only variable and function definitions belong here.
# Empty account and run directories are left in place by design. Cleanup walks
# run metadata and entry directories, so empty directories are harmless and
# retaining them avoids racing active runs between entry registrations.

CARINA_RUN_REGISTRY_SCHEMA=1

# carina_registry_process_start_time: Print a process's start time without the
# padding emitted by ps. A PID alone is not a stable process identity because
# the operating system can reuse it.
# Args: pid
carina_registry_process_start_time() {
    local pid="$1"
    local process_start_time

    process_start_time=$(ps -o lstart= -p "$pid" 2>/dev/null) || return 1
    process_start_time=$(printf '%s\n' "$process_start_time" | sed \
        -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')
    [ -n "$process_start_time" ] || return 1
    printf '%s\n' "$process_start_time"
}

# carina_registry_host: Print the host identity stored with each run.
carina_registry_host() {
    local host

    host=$(hostname) || return $?
    if [ -z "$host" ]; then
        echo "ERROR: cannot determine the acceptance run host" >&2
        return 1
    fi
    printf '%s\n' "$host"
}

# carina_registry_path_has_parent_segment: Return success when a path contains
# a component exactly equal to "..".
carina_registry_path_has_parent_segment() {
    case "/$1/" in
        */../*) return 0 ;;
    esac
    return 1
}

# carina_state_json: Read local state through Carina's supported interface.
# Reject any output whose schema_version is not 1 (carina#4106 contract).
# Args: work_dir
carina_state_json() {
    local state_json

    state_json=$("$CARINA_BIN" state show --json "$1") || return $?
    if ! jq -e '.schema_version == 1' <<< "$state_json" >/dev/null; then
        echo "ERROR: carina state JSON schema_version must be 1" >&2
        return 1
    fi

    printf '%s\n' "$state_json"
}

# carina_registry_repo_name: Print the repository name used by the registry.
# Callers may set CARINA_ACCEPTANCE_REPO. Otherwise derive it from the shared
# Git directory when this function is called.
carina_registry_repo_name() {
    local git_common_dir
    local repo_name

    if [ -n "${CARINA_ACCEPTANCE_REPO:-}" ]; then
        repo_name="$CARINA_ACCEPTANCE_REPO"
    else
        git_common_dir=$(git -C "${PROJECT_ROOT:-.}" rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || {
            echo "ERROR: cannot derive acceptance-test repository name; set CARINA_ACCEPTANCE_REPO" >&2
            return 1
        }
        repo_name=$(basename "$(dirname "$git_common_dir")")
    fi

    case "$repo_name" in
        ""|.|..|*/*)
            echo "ERROR: invalid acceptance-test repository name: $repo_name" >&2
            return 1
            ;;
    esac

    printf '%s\n' "$repo_name"
}

# carina_registry_root: Print this repository's persistent registry root.
carina_registry_root() {
    local acceptance_home
    local repo_name

    acceptance_home="${CARINA_ACCEPTANCE_HOME:-${HOME:?HOME is not set}/.cache/carina-acceptance}"
    repo_name=$(carina_registry_repo_name) || return $?
    printf '%s/%s\n' "${acceptance_home%/}" "$repo_name"
}

# carina_registry_runs_root: Print the directory containing persistent runs.
carina_registry_runs_root() {
    local registry_root

    registry_root=$(carina_registry_root) || return $?
    printf '%s/runs\n' "$registry_root"
}

# carina_registry_create_run: Create and print a persistent run directory.
# Run ids are a UTC timestamp followed by the current process id.
carina_registry_create_run() {
    local created_at
    local created_at_iso
    local host
    local process_start_time
    local repo_name
    local run_id
    local runs_root
    local run_dir
    local run_meta_tmp

    created_at=$(date -u '+%Y%m%dT%H%M%SZ') || return $?
    created_at_iso=$(date -u '+%Y-%m-%dT%H:%M:%SZ') || return $?
    host=$(carina_registry_host) || return $?
    if ! process_start_time=$(carina_registry_process_start_time "$$"); then
        echo "ERROR: cannot determine the acceptance run process start time" >&2
        return 1
    fi
    repo_name=$(carina_registry_repo_name) || return $?
    run_id="${created_at}-$$"
    runs_root=$(carina_registry_runs_root) || return $?
    run_dir="$runs_root/$run_id"

    mkdir -p "$run_dir" || return $?
    if [ ! -f "$run_dir/run.json" ]; then
        run_meta_tmp="$run_dir/run.json.tmp.$$"
        if ! jq -n \
            --argjson schema "$CARINA_RUN_REGISTRY_SCHEMA" \
            --arg repo "$repo_name" \
            --arg run_id "$run_id" \
            --arg host "$host" \
            --argjson pid "$$" \
            --arg process_start_time "$process_start_time" \
            --arg created_at "$created_at_iso" \
            '{schema:$schema,repo:$repo,run_id:$run_id,host:$host,pid:$pid,process_start_time:$process_start_time,created_at:$created_at}' \
            > "$run_meta_tmp" || ! mv "$run_meta_tmp" "$run_dir/run.json"; then
            rm -f "$run_meta_tmp"
            return 1
        fi
    fi
    printf '%s\n' "$run_dir"
}

# carina_registry_register_entry: Register one fixture and copy its main.crn.
# Args: run_dir account test_index fixture_path main_crn
carina_registry_register_entry() {
    if [ "$#" -ne 5 ]; then
        echo "ERROR: carina_registry_register_entry expects run_dir, account, test_index, fixture_path, and main_crn" >&2
        return 2
    fi

    local run_dir="$1"
    local account="$2"
    local test_index="$3"
    local fixture_path="$4"
    local main_crn="$5"
    local runs_root
    local run_id
    local repo_name
    local created_at
    local host
    local account_dir
    local entry_dir
    local meta_tmp

    runs_root=$(carina_registry_runs_root) || return $?
    if carina_registry_path_has_parent_segment "$run_dir"; then
        echo "ERROR: run directory contains a .. path segment: $run_dir" >&2
        return 1
    fi
    case "$run_dir" in
        "$runs_root"/*) ;;
        *)
            echo "ERROR: run directory is outside the registry: $run_dir" >&2
            return 1
            ;;
    esac
    if [ ! -d "$run_dir" ]; then
        echo "ERROR: run directory does not exist: $run_dir" >&2
        return 1
    fi
    case "$account" in
        ""|.|..|*/*)
            echo "ERROR: invalid registry account: $account" >&2
            return 1
            ;;
    esac
    case "$test_index" in
        ""|*[!0-9]*)
            echo "ERROR: registry test index must be a non-negative integer: $test_index" >&2
            return 1
            ;;
    esac
    if [ -z "$fixture_path" ]; then
        echo "ERROR: registry fixture path must not be empty" >&2
        return 1
    fi
    if [ ! -f "$main_crn" ]; then
        echo "ERROR: registry source main.crn does not exist: $main_crn" >&2
        return 1
    fi

    run_id=$(basename "$run_dir")
    repo_name=$(carina_registry_repo_name) || return $?
    created_at=$(date -u '+%Y-%m-%dT%H:%M:%SZ') || return $?
    if ! host=$(jq -er '.host | select(type == "string" and length > 0)' \
        "$run_dir/run.json" 2>/dev/null); then
        echo "ERROR: run metadata is missing or invalid: $run_dir/run.json" >&2
        return 1
    fi
    account_dir="$run_dir/$account"
    entry_dir="$account_dir/test_$test_index"

    mkdir -p "$account_dir" || return $?
    if ! mkdir "$entry_dir"; then
        echo "ERROR: registry entry already exists: $entry_dir" >&2
        return 1
    fi

    if ! cp "$main_crn" "$entry_dir/main.crn"; then
        rm -rf "$entry_dir"
        return 1
    fi

    meta_tmp="$entry_dir/meta.json.tmp"
    if ! jq -n \
        --argjson schema "$CARINA_RUN_REGISTRY_SCHEMA" \
        --arg repo "$repo_name" \
        --arg run_id "$run_id" \
        --arg account "$account" \
        --arg test "$fixture_path" \
        --arg host "$host" \
        --arg created_at "$created_at" \
        '{schema:$schema,repo:$repo,run_id:$run_id,account:$account,test:$test,host:$host,created_at:$created_at}' \
        > "$meta_tmp" || ! mv "$meta_tmp" "$entry_dir/meta.json"; then
        rm -rf "$entry_dir"
        return 1
    fi

    printf '%s\n' "$entry_dir"
}

# carina_registry_release_entry: Remove an entry only when supported state JSON
# reports absent state or present state with an empty resources array.
# Args: entry_dir
carina_registry_release_entry() {
    if [ "$#" -ne 1 ]; then
        echo "ERROR: carina_registry_release_entry expects exactly one entry directory" >&2
        return 2
    fi

    local entry_dir="$1"
    local runs_root
    local state_json

    runs_root=$(carina_registry_runs_root) || return $?
    if carina_registry_path_has_parent_segment "$entry_dir"; then
        echo "ERROR: refusing to release path containing a .. segment: $entry_dir" >&2
        return 1
    fi
    case "$entry_dir" in
        "$runs_root"/*/*/test_*) ;;
        *)
            echo "ERROR: refusing to release path outside the run registry: $entry_dir" >&2
            return 1
            ;;
    esac
    if [ ! -d "$entry_dir" ]; then
        echo "ERROR: registry entry does not exist: $entry_dir" >&2
        return 1
    fi

    if ! state_json=$(carina_state_json "$entry_dir"); then
        echo "ERROR: $entry_dir: carina_state_json failed; registry entry retained" >&2
        return 1
    fi
    if ! jq -e '
        (.state == "absent")
        or (.state == "present" and .resources == [])
    ' <<< "$state_json" >/dev/null; then
        echo "ERROR: $entry_dir: state is not absent or empty; registry entry retained" >&2
        return 1
    fi

    rm -rf "$entry_dir"
}

# carina_registry_account_selected: Return success when an account matches an
# optional list. With no list, every account is selected.
# Args: account [selected_account...]
carina_registry_account_selected() {
    local account="$1"
    shift
    local selected

    if [ "$#" -eq 0 ]; then
        return 0
    fi
    for selected in "$@"; do
        if [ "$account" = "$selected" ]; then
            return 0
        fi
    done
    return 1
}

# carina_registry_list_entries: Print registered entry directories, optionally
# restricted to the supplied account names.
# Args: [account...]
carina_registry_list_entries() {
    local runs_root
    local run_dir
    local account_dir
    local account
    local entry_dir

    runs_root=$(carina_registry_runs_root) || return $?
    [ -d "$runs_root" ] || return 0

    for run_dir in "$runs_root"/*; do
        [ -d "$run_dir" ] || continue
        for account_dir in "$run_dir"/*; do
            [ -d "$account_dir" ] || continue
            account=$(basename "$account_dir")
            carina_registry_account_selected "$account" "$@" || continue
            for entry_dir in "$account_dir"/test_*; do
                [ -d "$entry_dir" ] || continue
                printf '%s\n' "$entry_dir"
            done
        done
    done
}

# carina_registry_validate_meta: Validate entry metadata against its path and
# current repository. Errors always name the unusable entry.
# Args: entry_dir
carina_registry_validate_meta() {
    local entry_dir="$1"
    local meta_file="$entry_dir/meta.json"
    local repo_name
    local expected_run_id
    local expected_account
    local expected_host

    if [ ! -f "$meta_file" ]; then
        echo "ERROR: $entry_dir: missing meta.json" >&2
        return 1
    fi

    repo_name=$(carina_registry_repo_name) || return $?
    expected_run_id=$(basename "$(dirname "$(dirname "$entry_dir")")")
    expected_account=$(basename "$(dirname "$entry_dir")")
    if ! expected_host=$(jq -er '.host | select(type == "string" and length > 0)' \
        "$(dirname "$(dirname "$entry_dir")")/run.json" 2>/dev/null); then
        echo "ERROR: $entry_dir: invalid run.json" >&2
        return 1
    fi

    if ! jq -e \
        --argjson schema "$CARINA_RUN_REGISTRY_SCHEMA" \
        --arg repo "$repo_name" \
        --arg run_id "$expected_run_id" \
        --arg account "$expected_account" \
        --arg host "$expected_host" \
        '.schema == $schema
         and .repo == $repo
         and .run_id == $run_id
         and .account == $account
         and .host == $host
         and (.test | type == "string" and length > 0)
         and (.created_at | type == "string" and length > 0)' \
        "$meta_file" >/dev/null 2>&1; then
        echo "ERROR: $entry_dir: invalid meta.json" >&2
        return 1
    fi
}

# carina_registry_fixture_selected: Return success when a fixture path matches
# any optional substring filter. With no filters, every fixture is selected.
# Args: fixture_path [filter...]
carina_registry_fixture_selected() {
    local fixture_path="$1"
    shift
    local filter

    if [ "$#" -eq 0 ]; then
        return 0
    fi
    for filter in "$@"; do
        case "$fixture_path" in
            *"$filter"*) return 0 ;;
        esac
    done
    return 1
}

# carina_registry_cleanup: Destroy and state-check registered entries.
# Requires the caller to define with_account_creds(account, command...).
# Args: [--account account]... [--filter substring]...
carina_registry_cleanup() {
    local accounts=()
    local filters=()
    local entries=()
    local entry_dir
    local account
    local fixture_path
    local run_dir
    local run_host
    local run_pid
    local run_process_start_time
    local current_process_start_time
    local current_host
    local list_output
    local destroy_output
    local release_output
    local found=0
    local failed=0
    local skipped=0

    current_host=$(carina_registry_host) || return $?

    while [ "$#" -gt 0 ]; do
        case "$1" in
            --account)
                if [ -z "${2:-}" ]; then
                    echo "ERROR: --account requires a value" >&2
                    return 2
                fi
                accounts+=("$2")
                shift 2
                ;;
            --filter)
                if [ -z "${2:-}" ]; then
                    echo "ERROR: --filter requires a value" >&2
                    return 2
                fi
                filters+=("$2")
                shift 2
                ;;
            *)
                echo "ERROR: unknown registry cleanup argument: $1" >&2
                return 2
                ;;
        esac
    done

    if ! list_output=$(carina_registry_list_entries); then
        return 1
    fi
    while IFS= read -r entry_dir; do
        [ -n "$entry_dir" ] && entries+=("$entry_dir")
    done <<< "$list_output"

    for entry_dir in ${entries[@]+"${entries[@]}"}; do
        account=$(basename "$(dirname "$entry_dir")")
        if ! carina_registry_account_selected \
            "$account" ${accounts[@]+"${accounts[@]}"}; then
            continue
        fi

        run_dir=$(dirname "$(dirname "$entry_dir")")
        run_host=$(jq -r '.host // empty' "$run_dir/run.json" 2>/dev/null || true)
        if [ -n "$run_host" ] && [ "$run_host" != "$current_host" ]; then
            echo "$entry_dir: different host ($run_host), skipped"
            found=1
            skipped=$((skipped + 1))
            continue
        fi
        run_pid=$(jq -r \
            'if (.pid | type) == "number" then
                if .pid > 0 and .pid == (.pid | floor) then .pid else empty end
             else empty end' \
            "$run_dir/run.json" 2>/dev/null || true)
        run_process_start_time=$(jq -r \
            'if (.process_start_time | type) == "string" then .process_start_time else empty end' \
            "$run_dir/run.json" 2>/dev/null || true)
        case "$run_pid" in
            ""|*[!0-9]*) ;;
            *)
                if [ "$run_host" = "$current_host" ] && \
                   [ -n "$run_process_start_time" ] && \
                   kill -0 "$run_pid" 2>/dev/null && \
                   current_process_start_time=$(carina_registry_process_start_time "$run_pid") && \
                   [ "$current_process_start_time" = "$run_process_start_time" ]; then
                    echo "$entry_dir: in progress, skipped (PID $run_pid is alive with matching start time on $run_host)"
                    found=1
                    skipped=$((skipped + 1))
                    continue
                fi
                ;;
        esac

        if [ "${#filters[@]}" -ne 0 ]; then
            fixture_path=$(jq -r \
                'if (.test | type) == "string" and (.test | length) > 0 then .test else empty end' \
                "$entry_dir/meta.json" 2>/dev/null || true)
            if [ -z "$fixture_path" ]; then
                echo "WARNING: $entry_dir: skipped (cannot be matched against the filter: meta.json is missing or unreadable)" >&2
                found=1
                skipped=$((skipped + 1))
                continue
            fi
            if ! carina_registry_fixture_selected \
                "$fixture_path" ${filters[@]+"${filters[@]}"}; then
                continue
            fi
        fi

        if ! carina_registry_validate_meta "$entry_dir"; then
            found=1
            failed=$((failed + 1))
            continue
        fi

        account=$(jq -r '.account' "$entry_dir/meta.json")
        fixture_path=$(jq -r '.test' "$entry_dir/meta.json")
        found=1

        if [ ! -f "$entry_dir/main.crn" ]; then
            echo "ERROR: $entry_dir: missing main.crn" >&2
            failed=$((failed + 1))
            continue
        fi

        echo "Destroying $entry_dir ($fixture_path, $account)"
        if ! destroy_output=$(cd "$entry_dir" && \
            with_account_creds "$account" "$CARINA_BIN" destroy --auto-approve "$entry_dir" 2>&1); then
            echo "ERROR: $entry_dir: destroy failed; registry entry retained" >&2
            if [ -n "$destroy_output" ]; then
                echo "  $destroy_output" >&2
            fi
            failed=$((failed + 1))
            continue
        fi
        if grep -q "failed" <<< "$destroy_output"; then
            echo "ERROR: $entry_dir: destroy reported failure; registry entry retained" >&2
            if [ -n "$destroy_output" ]; then
                echo "  $destroy_output" >&2
            fi
            failed=$((failed + 1))
            continue
        fi

        if ! release_output=$(carina_registry_release_entry "$entry_dir" 2>&1); then
            echo "ERROR: $entry_dir: state check after destroy failed; registry entry retained" >&2
            if [ -n "$release_output" ]; then
                echo "  $release_output" >&2
            fi
            failed=$((failed + 1))
            continue
        fi
        echo "Destroyed and released: $entry_dir"
    done

    if [ "${#entries[@]}" -eq 0 ]; then
        echo "No registered entries"
        return 0
    fi
    if [ "$found" -eq 0 ]; then
        echo "No registry entries match"
        return 0
    fi
    if [ "$skipped" -ne 0 ]; then
        if [ "$failed" -ne 0 ]; then
            echo "Cleanup summary: $skipped selected entr$(if [ "$skipped" -eq 1 ]; then printf 'y'; else printf 'ies'; fi) skipped (exit status 1 because cleanup failures take precedence)" >&2
        else
            echo "Cleanup summary: $skipped selected entr$(if [ "$skipped" -eq 1 ]; then printf 'y'; else printf 'ies'; fi) skipped (exit status 3)" >&2
        fi
    fi
    if [ "$failed" -ne 0 ]; then
        echo "ERROR: cleanup failed for $failed registered entr$(if [ "$failed" -eq 1 ]; then printf 'y'; else printf 'ies'; fi)" >&2
        return 1
    fi
    if [ "$skipped" -ne 0 ]; then
        return 3
    fi

    return 0
}
