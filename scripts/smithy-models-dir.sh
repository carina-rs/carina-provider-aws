#!/usr/bin/env bash
# Resolve the model cache for the repository's pinned AWS Smithy revision.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REV_FILE="$PROJECT_ROOT/smithy-models.rev"

if [[ ! -f "$REV_FILE" ]]; then
    echo "ERROR: Missing Smithy model revision file: $REV_FILE" >&2
    exit 1
fi

REV="$(cat "$REV_FILE")"
if [[ ! "$REV" =~ ^[0-9a-fA-F]{40}$ ]]; then
    echo "ERROR: $REV_FILE must contain a full 40-character hex commit SHA of aws/api-models-aws" >&2
    exit 1
fi

printf '%s/carina-provider-aws/tests/fixtures/smithy/%s\n' "$PROJECT_ROOT" "$REV"
