#!/bin/bash
# Resolve the local provider WASM paths used by the acceptance harness, grant
# those exact files AWS-family trust, and accept their declared egress for every
# carina subprocess.
#
# Callers must set PROJECT_ROOT and define resolve_sibling_base before sourcing
# this file. The top-level runner sources it after building the AWS provider;
# custom suites source it through _helpers.sh after their provider builds exist.

_TEST_AWS_WASM="${_TEST_AWS_WASM:-$PROJECT_ROOT/target/wasm32-wasip2/release/carina-provider-aws.wasm}"
_TEST_AWSCC_WASM="${_TEST_AWSCC_WASM:-}"
_TEST_OPERATOR_TRUST_LOCAL_PROVIDER="${CARINA_TRUST_LOCAL_PROVIDER:-}"
_TEST_OPERATOR_ACCEPT_EGRESS="${CARINA_ACCEPT_EGRESS:-}"

PROVIDER_VERSION=$(grep '^version = ' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/version = "\(.*\)"/\1/')

resolve_provider_wasm_path() {
    local variable_name="$1"
    local configured_path="${!variable_name:-}"
    local provider_dir

    if [ -z "$configured_path" ]; then
        return 0
    fi

    if [[ "$configured_path" != /* ]]; then
        configured_path="$(pwd -P)/$configured_path"
    fi

    if [ ! -e "$configured_path" ]; then
        echo "ERROR: configured provider WASM does not exist ($variable_name): $configured_path" >&2
        return 1
    fi
    if [ ! -f "$configured_path" ]; then
        echo "ERROR: configured provider WASM is not a file ($variable_name): $configured_path" >&2
        return 1
    fi
    if [[ "$configured_path" == *:* ]]; then
        echo "ERROR: $variable_name provider WASM path contains ':' and cannot be used in CARINA_TRUST_LOCAL_PROVIDER: $configured_path" >&2
        return 1
    fi
    if [[ "$configured_path" == *,* ]]; then
        echo "ERROR: $variable_name provider WASM path contains ',' and cannot be used in CARINA_ACCEPT_EGRESS: $configured_path" >&2
        return 1
    fi
    if [[ "$configured_path" != *.wasm ]]; then
        echo "ERROR: $variable_name points to a non-WASM binary: $configured_path" >&2
        echo "       Provider binaries must be .wasm components." >&2
        return 1
    fi

    provider_dir="$(cd "$(dirname "$configured_path")" && pwd -P)"
    configured_path="$provider_dir/$(basename "$configured_path")"
    printf -v "$variable_name" '%s' "$configured_path"
}

export_local_provider_trust() {
    local variable_name
    local configured_path
    local accepted_sources=""
    local grants=""

    for variable_name in _TEST_AWS_WASM _TEST_AWSCC_WASM; do
        resolve_provider_wasm_path "$variable_name" || return 1
    done

    for variable_name in _TEST_AWS_WASM _TEST_AWSCC_WASM; do
        configured_path="${!variable_name:-}"
        if [ -n "$configured_path" ]; then
            grants="${grants:+$grants:}$configured_path=aws"
            accepted_sources="${accepted_sources:+$accepted_sources,}file://$configured_path"
        fi
    done

    CARINA_TRUST_LOCAL_PROVIDER="${_TEST_OPERATOR_TRUST_LOCAL_PROVIDER}${_TEST_OPERATOR_TRUST_LOCAL_PROVIDER:+:}$grants"
    CARINA_ACCEPT_EGRESS="${_TEST_OPERATOR_ACCEPT_EGRESS}${_TEST_OPERATOR_ACCEPT_EGRESS:+,}$accepted_sources"
    export CARINA_ACCEPT_EGRESS CARINA_TRUST_LOCAL_PROVIDER
}

ensure_awscc_wasm() {
    if [ -z "${_TEST_AWSCC_WASM:-}" ]; then
        _TEST_AWSCC_WASM="${SIBLING_BASE:-$(resolve_sibling_base)}/carina-provider-awscc/target/wasm32-wasip2/release/carina-provider-awscc.wasm"
    fi
    export_local_provider_trust
}

_inject_provider_source_filter() {
    sed \
        -e '/^provider aws {/a\
  source = "file://'"$_TEST_AWS_WASM"'"\
  version = "'"$PROVIDER_VERSION"'"' \
        -e '/^provider awscc {/a\
  source = "file://'"$_TEST_AWSCC_WASM"'"'
}

# inject_provider_source: Create a temporary directory containing a .crn file
# with local provider sources injected and assign it to a caller variable.
# PROVIDER_VERSION describes this repository's aws provider, so the sibling
# awscc provider gets a source override only.
# Args: original_crn_file output_variable
inject_provider_source() {
    local original="$1"
    local output_variable="$2"
    local tmp_dir

    if grep -q '^provider awscc {' "$original" 2>/dev/null; then
        ensure_awscc_wasm || return 1
    fi

    tmp_dir=$(mktemp -d)

    _inject_provider_source_filter < "$original" > "$tmp_dir/main.crn"

    printf -v "$output_variable" '%s' "$tmp_dir"
}

export_local_provider_trust || {
    return 1 2>/dev/null || exit 1
}
