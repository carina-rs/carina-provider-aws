#!/usr/bin/env bash
# Generate aws provider boilerplate from ResourceDef metadata
#
# Usage (from project root):
#   ./carina-provider-aws/scripts/generate-provider.sh
#
# This script generates simple delete methods and no-op update methods
# from ResourceDef metadata.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Download models if needed
"$SCRIPT_DIR/download-smithy-models.sh"
MODEL_DIR="$("$SCRIPT_DIR/smithy-models-dir.sh")"

# Build and run codegen
cd "$PROJECT_ROOT"
cargo run -p carina-codegen-aws --bin smithy-codegen -- \
  --model-dir "$MODEL_DIR" \
  --output-dir "$SCRIPT_DIR/../carina-provider-aws/src" \
  --format provider

# Format
cargo fmt -p carina-provider-aws

echo ""
echo "Done! Generated carina-provider-aws/src/provider_generated.rs"
