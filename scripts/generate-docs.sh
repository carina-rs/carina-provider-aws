#!/bin/bash
# Generate aws provider documentation from Smithy models
#
# Usage (from project root):
#   ./carina-provider-aws/scripts/generate-docs.sh
#
# This script generates markdown documentation from Smithy model JSON files.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

DOCS_DIR="$PROJECT_ROOT/generated-docs/aws"
EXAMPLES_DIR="$PROJECT_ROOT/examples"
rm -rf "$DOCS_DIR"
mkdir -p "$DOCS_DIR"

# Download models if needed
"$SCRIPT_DIR/download-smithy-models.sh"

echo "Generating aws provider documentation..."
echo "Output directory: $DOCS_DIR"
echo ""

cd "$PROJECT_ROOT"
cargo run -p carina-codegen-aws --bin smithy-codegen -- \
  --model-dir "$SCRIPT_DIR/../carina-provider-aws/tests/fixtures/smithy" \
  --output-dir "$DOCS_DIR" \
  --examples-dir "$EXAMPLES_DIR" \
  --format markdown

# Prepend Starlight frontmatter to all generated docs
for DOC_FILE in "$DOCS_DIR"/*/*.md; do
    [ -f "$DOC_FILE" ] || continue
    DSL_NAME=$(head -1 "$DOC_FILE" | sed 's/^# *//')
    FRONTMATTER_TMPFILE=$(mktemp)
    {
        echo "---"
        echo "title: \"$DSL_NAME\""
        echo "---"
        echo ""
        # Strip H1 line (Starlight renders frontmatter title as heading)
        sed '1{/^# /d;}' "$DOC_FILE"
    } > "$FRONTMATTER_TMPFILE"
    mv "$FRONTMATTER_TMPFILE" "$DOC_FILE"
done

echo ""
echo "Done! Generated documentation in $DOCS_DIR"
