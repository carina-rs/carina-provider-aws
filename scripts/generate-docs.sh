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
SERVICE_TITLES_FILE="$PROJECT_ROOT/docs/registry/service-titles.json"
rm -rf "$DOCS_DIR"
mkdir -p "$DOCS_DIR"

# Look up a lowercase service code's canonical AWS display name.
service_display_name() {
    python3 - "$SERVICE_TITLES_FILE" "$1" <<'PY'
import json
import sys


class DuplicateJSONKeyError(ValueError):
    def __init__(self, key):
        super().__init__(key)
        self.key = key


def reject_duplicate_json_keys(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise DuplicateJSONKeyError(key)
        result[key] = value
    return result


path, service_id = sys.argv[1:]
try:
    with open(path, encoding="utf-8") as file:
        titles = json.load(file, object_pairs_hook=reject_duplicate_json_keys)
except DuplicateJSONKeyError as error:
    print(
        f"error: {path}: duplicate AWS service title key {error.key!r}",
        file=sys.stderr,
    )
    raise SystemExit(1)
except (OSError, UnicodeError, json.JSONDecodeError) as error:
    print(f"error: cannot read AWS service titles from {path}: {error}", file=sys.stderr)
    raise SystemExit(1)

if not isinstance(titles, dict):
    print(f"error: AWS service titles file is not a JSON object: {path}", file=sys.stderr)
    raise SystemExit(1)
if service_id not in titles:
    print(
        f"error: AWS service {service_id!r} has no display title in {path}",
        file=sys.stderr,
    )
    raise SystemExit(1)

title = titles[service_id]
if not isinstance(title, str):
    print(
        f"error: display title for AWS service {service_id!r} is not a string in {path}",
        file=sys.stderr,
    )
    raise SystemExit(1)
print(title)
PY
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
  --examples-dir "$EXAMPLES_DIR" \
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
    REFERENCE_KIND="resource"
    case "$(basename "$DOC_FILE")" in
        *_data_source.md) REFERENCE_KIND="data source" ;;
    esac
    FRONTMATTER_TMPFILE=$(mktemp)
    {
        echo "---"
        echo "title: \"$DSL_NAME\""
        echo "description: \"AWS $SERVICE_DISPLAY $RESOURCE_DISPLAY $REFERENCE_KIND reference\""
        echo "---"
        echo ""
        # Strip H1 line (Starlight renders frontmatter title as heading)
        sed '1{/^# /d;}' "$DOC_FILE"
    } > "$FRONTMATTER_TMPFILE"
    mv "$FRONTMATTER_TMPFILE" "$DOC_FILE"
done

echo ""
echo "Done! Generated documentation in $DOCS_DIR"
