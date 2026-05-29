#!/bin/bash
# Guard against the `carina-rs/carina-provider-aws#390` bug class:
# hand-written `AttributeType::string_enum(...)` calls in
# `carina-aws-types/src/lib.rs` that ship with an empty `dsl_aliases`
# argument (`vec![]`).
#
# `AwsNormalizer::api_canonicalize_recursive` relies on the alias table
# to rewrite DSL spellings (e.g. `aes256`) to the AWS API canonical
# (`AES256`) before the wire call. A StringEnum whose alias table is
# empty silently forwards the raw alias spelling to the AWS SDK and
# triggers `MalformedXML` / `Unknown(...)` errors at apply time —
# unit tests pass, acceptance tests fail against real AWS.
#
# The fix is to construct hand-written StringEnums via the
# `string_enum_with_dsl_aliases(name, values, identity)` helper in
# `carina-aws-types/src/lib.rs`, which derives the alias table from
# `values` so "empty by accident" is impossible by construction.
#
# Why this script exists at all: the helper is a *convention*, not a
# type-system gate — the underlying `AttributeType::string_enum`
# constructor in carina-core still accepts an empty alias vec. This
# script enforces the convention at CI time. See the PR that closes
# #390 for the full reasoning.
#
# The script only checks `carina-aws-types/src/lib.rs`. The two
# string_enum sites outside that file (`factory.rs` region,
# `schemas/types.rs` cloudfront_hosted_zone_id) are explicitly
# reviewed: region builds non-empty aliases dynamically, and
# cloudfront_hosted_zone_id uses a single hand-curated alias
# (`Z2FDTNDATAQYW2 → global`) that the derivation rule cannot produce.
#
# Exit 0 = OK, non-zero = found a problem (message explains the fix).
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="$REPO_ROOT/carina-aws-types/src/lib.rs"

if [ ! -f "$TARGET" ]; then
    echo "check-string-enum-aliases: ERROR: $TARGET not found" >&2
    exit 2
fi

# Allow-list: line ranges in $TARGET where a raw `AttributeType::string_enum`
# call with `vec![]` or a dynamic alias source is intentional.
#
# Currently:
#   - `condition_type` (IAM condition operator map key): builds a
#     dynamic alias table from `all_condition_operator_snake_forms()` /
#     `dsl_enum_value` and may emit an empty filtered vec. The site
#     uses the raw constructor because the alias source is not the
#     `values` slice.
#
# Add new exceptions sparingly. Each one is a bug-class admission.
#
# The pattern is anchored with `\(` so sibling names like
# `fn condition_type_v2` / `fn condition_type_helper` do NOT inherit the
# exception by accident.
ALLOWLIST_PATTERN='fn condition_type\('

# Find every `AttributeType::string_enum(` call in $TARGET and report any
# whose closing `)` appears with `vec![]` as the last positional arg.
#
# Approach: pull out the rough 12-line window starting at each
# `AttributeType::string_enum(` line, then look for the `vec![],?\s*\)`
# closing shape. Cross-check against the allow-list by walking up to
# the nearest `fn <name>(` declaration above the call.

violations=0
report=""

while IFS=: read -r lineno _; do
    [ -z "$lineno" ] && continue

    # Window: 15 lines starting at the call site is enough for every
    # current call in this file (longest is `s3_transition_storage_class`,
    # 11 lines). The 4-line headroom absorbs future enums with one or two
    # extra values without needing to revisit this script.
    window=$(sed -n "${lineno},$((lineno + 14))p" "$TARGET")

    # The call must close within the window. Reconstruct the body of
    # the `string_enum(...)` call by joining lines until the paren
    # depth that opened at the `string_enum(` returns to zero, then
    # inspect the joined body. This catches both multi-line layouts
    # (where `vec![]` sits on its own line) AND single-line layouts
    # (where rustfmt collapses the call onto one line) — round-4
    # review flagged the single-line case as a prior false-negative.
    #
    # An "empty-aliases" call is one whose **last positional argument**
    # — the substring after the final top-level `,` and before the
    # closing `)` — is exactly `vec![]`. The depth tracker is needed
    # to ignore commas / closing parens inside nested calls like
    # `string_enum_identity("X", Some("Y"))`.
    if printf '%s\n' "$window" | awk '
        BEGIN { depth = 0; body = ""; found_close = 0; started = 0 }
        {
            line = $0
            # On the first line of the window, skip past anything
            # before the literal `AttributeType::string_enum(` so we
            # start paren tracking at the right `(`. Without this, a
            # call inlined into a `fn name() -> AttributeType { ... }`
            # body would trip the `(` of the fn header first.
            if (!started) {
                pos = index(line, "AttributeType::string_enum(")
                if (pos == 0) next
                line = substr(line, pos + length("AttributeType::string_enum"))
                started = 1
            }
            for (i = 1; i <= length(line); i++) {
                c = substr(line, i, 1)
                if (c == "(") {
                    depth++
                    if (depth == 1) {
                        # Skip the opening `(` itself in the body.
                        continue
                    }
                } else if (c == ")") {
                    if (depth == 1) {
                        found_close = 1
                        # Body now holds the args of the `string_enum`
                        # call. The last positional arg is whatever
                        # comes after the final top-level `,` once
                        # any trailing-comma + whitespace is stripped.
                        # See END block.
                        exit 0
                    }
                    depth--
                }
                if (depth >= 1) {
                    body = body c
                }
            }
            body = body " "  # newline → space
        }
        END {
            if (!found_close) exit 1
            # Strip trailing whitespace.
            sub(/[ \t]+$/, "", body)
            # Strip a single trailing top-level comma (rustfmt always
            # emits one on multi-line calls).
            sub(/,$/, "", body)
            sub(/[ \t]+$/, "", body)
            # If the body now ends with `vec![]`, the last positional
            # argument was empty-aliases. We do not need to find the
            # exact last comma position: nested calls like
            # `dsl_aliases_for(&[...])` end with `)`, and
            # `string_enum_identity(...)` ends with `))`, so an
            # endswith-`vec![]` check is sufficient for any current
            # carina-aws-types StringEnum signature.
            if (length(body) >= 6 && substr(body, length(body) - 5) == "vec![]") {
                exit 0
            }
            exit 1
        }
    '; then
        # Walk back to find the enclosing `fn <name>(` to check the
        # allow-list. Look at the 100 lines before the call site.
        ctx_start=$((lineno > 100 ? lineno - 100 : 1))
        enclosing_fn=$(sed -n "${ctx_start},${lineno}p" "$TARGET" |
            grep -E '^(pub )?fn [a-z_][a-z0-9_]*' |
            tail -1)
        if printf '%s' "$enclosing_fn" | grep -qE "$ALLOWLIST_PATTERN"; then
            continue
        fi
        violations=$((violations + 1))
        report+="  $TARGET:$lineno (in ${enclosing_fn:-<unknown fn>})"$'\n'
    fi
done < <(grep -n 'AttributeType::string_enum(' "$TARGET")

if [ "$violations" -gt 0 ]; then
    echo "check-string-enum-aliases: FAIL: found $violations call site(s) with empty dsl_aliases:" >&2
    printf '%s' "$report" >&2
    echo "" >&2
    echo "Use string_enum_with_dsl_aliases(name, values, identity) instead." >&2
    echo "It derives the alias table from values via dsl_aliases_for, making" >&2
    echo "the carina-rs/carina-provider-aws#390 bug class impossible by construction." >&2
    echo "If the call really needs a dynamic alias source, add it to the" >&2
    echo "ALLOWLIST_PATTERN in this script with a short justification." >&2
    exit 1
fi

echo "check-string-enum-aliases OK: 0 hand-written StringEnums in carina-aws-types with empty dsl_aliases."
