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

# Map a lowercase service code to its canonical AWS display name.
# Naive uppercasing produces unreadable strings like "AWS LOGS log_group ..."
# in the frontmatter description.
service_display_name() {
    case "$1" in
        s3)            echo "S3" ;;
        ec2)           echo "EC2" ;;
        iam)           echo "IAM" ;;
        sts)           echo "STS" ;;
        sso)           echo "IAM Identity Center" ;;
        logs)          echo "CloudWatch Logs" ;;
        route53)       echo "Route 53" ;;
        identitystore) echo "Identity Store" ;;
        organizations) echo "Organizations" ;;
        *)             echo "$1" | tr '[:lower:]' '[:upper:]' ;;
    esac
}

# Download models if needed
"$SCRIPT_DIR/download-smithy-models.sh"

echo "Generating aws provider documentation..."
echo "Output directory: $DOCS_DIR"
echo ""

cd "$PROJECT_ROOT"
cargo run -p carina-codegen-aws --bin smithy-codegen -- \
  --model-dir "$SCRIPT_DIR/../carina-provider-aws/tests/fixtures/smithy" \
  --output-dir "$DOCS_DIR" \
  --format markdown

# Prepend Starlight frontmatter to all generated docs
for DOC_FILE in "$DOCS_DIR"/*/*.md; do
    [ -f "$DOC_FILE" ] || continue
    DSL_NAME=$(head -1 "$DOC_FILE" | sed 's/^# *//')
    SERVICE_DIR=$(basename "$(dirname "$DOC_FILE")")
    SERVICE_DISPLAY=$(service_display_name "$SERVICE_DIR")
    # Use the PascalCase resource name from the DSL title (e.g. "Bucket" from
    # "aws.s3.Bucket") so the description text matches the title casing.
    RESOURCE_DISPLAY=$(echo "$DSL_NAME" | awk -F'.' '{print $NF}')
    FRONTMATTER_TMPFILE=$(mktemp)
    {
        echo "---"
        echo "title: \"$DSL_NAME\""
        echo "description: \"AWS $SERVICE_DISPLAY $RESOURCE_DISPLAY resource reference\""
        echo "---"
        echo ""
        # Strip H1 line (Starlight renders frontmatter title as heading)
        sed '1{/^# /d;}' "$DOC_FILE"
    } > "$FRONTMATTER_TMPFILE"
    mv "$FRONTMATTER_TMPFILE" "$DOC_FILE"
done

# Insert examples into generated docs (after description, before Argument Reference)
for DOC_FILE in "$DOCS_DIR"/*/*.md; do
    SERVICE_DIR=$(basename "$(dirname "$DOC_FILE")")
    RESOURCE_NAME=$(basename "$DOC_FILE" .md)
    EXAMPLE_FILE="$EXAMPLES_DIR/${SERVICE_DIR}_${RESOURCE_NAME}/main.crn"
    if [ -f "$EXAMPLE_FILE" ]; then
        EXAMPLE_TMPFILE=$(mktemp)
        {
            echo "## Example"
            echo ""
            echo '```crn'
            # Strip provider block, leading comments, and leading blank lines
            sed -n '/^provider /,/^}/!p' "$EXAMPLE_FILE" | \
                sed '/^#/d' | \
                sed '/./,$!d'
            echo '```'
            echo ""
        } > "$EXAMPLE_TMPFILE"
        # Insert the example block before "## Argument Reference"
        MERGED_TMPFILE=$(mktemp)
        while IFS= read -r line || [ -n "$line" ]; do
            if [ "$line" = "## Argument Reference" ]; then
                cat "$EXAMPLE_TMPFILE"
            fi
            printf '%s\n' "$line"
        done < "$DOC_FILE" > "$MERGED_TMPFILE"
        mv "$MERGED_TMPFILE" "$DOC_FILE"
        rm -f "$EXAMPLE_TMPFILE"
    fi
done

echo ""
echo "Done! Generated documentation in $DOCS_DIR"
