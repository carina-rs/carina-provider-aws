#!/usr/bin/env python3
"""Build and validate the documentation archive published with a release."""

import argparse
import codecs
import gzip
import io
import json
import os
import re
import sys
import tarfile
import tempfile
import zlib
from dataclasses import dataclass
from pathlib import Path


MIB = 1024 * 1024
MAX_COMPRESSED_SIZE = 16 * MIB
MAX_ENTRIES = 4096
MAX_FILE_SIZE = 1 * MIB
MAX_FILES_SIZE = 64 * MIB
MAX_TAR_SIZE = 80 * MIB

ROOT = Path(__file__).resolve().parents[1]
GENERATED_DOCS = ROOT / "generated-docs" / "aws"
INDEX_SOURCE = ROOT / "docs" / "registry" / "index.md"

SLUG_RE = re.compile(r"[a-z0-9][a-z0-9_]*\Z")


class BundleError(Exception):
    """A clear, user-facing build error."""


@dataclass(frozen=True)
class Page:
    archive_path: str
    title: str
    content: bytes


def _validate_source_slug(slug, description):
    if not SLUG_RE.fullmatch(slug):
        raise BundleError(
            f"invalid {description} slug {slug!r}: expected [a-z0-9][a-z0-9_]*"
        )
    if len(slug.encode("utf-8")) > 128:
        raise BundleError(f"{description} slug {slug!r} exceeds 128 bytes")


def _frontmatter_title_and_body(path):
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError as error:
        raise BundleError(
            f"{path}: generated documentation is not UTF-8: {error}"
        ) from error

    lines = text.splitlines(keepends=True)
    if not lines or lines[0].rstrip("\r\n") != "---":
        raise BundleError(f"{path}: expected YAML frontmatter starting with '---'")

    closing = None
    for index, line in enumerate(lines[1:], start=1):
        if line.rstrip("\r\n") == "---":
            closing = index
            break
    if closing is None:
        raise BundleError(f"{path}: YAML frontmatter has no closing '---'")

    title_values = []
    for line in lines[1:closing]:
        match = re.fullmatch(r"title:\s*(.*?)\s*(?:\r?\n)?", line)
        if match:
            title_values.append(match.group(1))
    if len(title_values) != 1:
        raise BundleError(f"{path}: YAML frontmatter must contain exactly one title")

    raw_title = title_values[0]
    try:
        title = json.loads(raw_title)
    except json.JSONDecodeError as error:
        raise BundleError(f"{path}: invalid title in YAML frontmatter: {error}") from error
    if not isinstance(title, str) or not title or "\n" in title or "\r" in title:
        raise BundleError(f"{path}: YAML frontmatter title must be a non-empty string")

    body = "".join(lines[closing + 1 :]).lstrip("\r\n")
    return title, f"# {title}\n\n{body}".encode("utf-8")


def _collect_pages(source_root=GENERATED_DOCS):
    if not source_root.is_dir():
        raise BundleError(f"generated documentation directory does not exist: {source_root}")

    destinations = {}
    for source in sorted(source_root.rglob("*.md")):
        relative = source.relative_to(source_root)
        if len(relative.parts) != 2:
            raise BundleError(
                f"{source}: expected generated-docs/aws/<service>/<name>.md"
            )

        service = relative.parts[0]
        stem = source.stem
        is_data_source = stem.endswith("_data_source")
        name = stem[: -len("_data_source")] if is_data_source else stem
        _validate_source_slug(service, "service")
        _validate_source_slug(name, "document name")

        slug = f"{service}_{name}"
        _validate_source_slug(slug, "bundle")
        section = "data-sources" if is_data_source else "resources"
        archive_path = f"{section}/{slug}.md"
        if archive_path in destinations:
            previous = destinations[archive_path][0]
            raise BundleError(
                f"two sources map to {archive_path}: {previous} and {source}"
            )

        title, content = _frontmatter_title_and_body(source)
        destinations[archive_path] = (source, Page(archive_path, title, content))

    if not destinations:
        raise BundleError(
            "no resource or data-source documentation pages found in "
            f"{source_root}"
        )

    return [value[1] for value in destinations.values()]


def _build_index(pages, index_source=INDEX_SOURCE):
    try:
        introduction = index_source.read_text(encoding="utf-8")
    except FileNotFoundError as error:
        raise BundleError(
            f"provider documentation index does not exist: {index_source}"
        ) from error
    except UnicodeDecodeError as error:
        raise BundleError(
            f"{index_source}: provider index is not UTF-8: {error}"
        ) from error

    sections = []
    for heading, prefix in (
        ("Resources", "resources/"),
        ("Data Sources", "data-sources/"),
    ):
        section_pages = sorted(
            (page for page in pages if page.archive_path.startswith(prefix)),
            key=lambda page: page.archive_path,
        )
        if not section_pages:
            continue
        lines = [f"## {heading}", ""]
        lines.extend(
            f"- [{page.title}]({page.archive_path})" for page in section_pages
        )
        sections.append("\n".join(lines).rstrip())

    return (introduction.rstrip() + "\n\n" + "\n\n".join(sections) + "\n").encode(
        "utf-8"
    )


def _tar_info(name, *, size=0, directory=False):
    info = tarfile.TarInfo(name)
    info.type = tarfile.DIRTYPE if directory else tarfile.REGTYPE
    info.size = 0 if directory else size
    info.mode = 0o755 if directory else 0o644
    info.mtime = 0
    info.uid = 0
    info.gid = 0
    info.uname = ""
    info.gname = ""
    return info


def build_bundle(output, source_root=GENERATED_DOCS, index_source=INDEX_SOURCE):
    pages = _collect_pages(source_root)
    index = _build_index(pages, index_source)

    entries = [("index.md", False, index)]
    for directory in ("resources", "data-sources"):
        if any(page.archive_path.startswith(directory + "/") for page in pages):
            entries.append((directory, True, b""))
    entries.extend((page.archive_path, False, page.content) for page in pages)
    entries.sort(key=lambda entry: entry[0])

    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="wb", dir=output.parent, prefix=f".{output.name}.", delete=False
        ) as raw:
            temporary = Path(raw.name)
            with gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
                with tarfile.open(
                    mode="w", fileobj=compressed, format=tarfile.PAX_FORMAT
                ) as archive:
                    for name, directory, content in entries:
                        info = _tar_info(
                            name, size=len(content), directory=directory
                        )
                        archive.addfile(
                            info, None if directory else io.BytesIO(content)
                        )
        os.replace(temporary, output)
        temporary = None
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)

    print(f"built {output} ({len(pages) + 1} files)")


def _decompress_gzip(path, errors):
    try:
        compressed_size = path.stat().st_size
        if compressed_size > MAX_COMPRESSED_SIZE:
            errors.append(
                f"compressed archive is {compressed_size} bytes; limit is 16 MiB"
            )
            return None
        compressed = path.read_bytes()
    except OSError as error:
        errors.append(f"cannot read bundle: {error}")
        return None

    output = io.BytesIO()
    decompressor = zlib.decompressobj(16 + zlib.MAX_WBITS)
    decompressed_size = 0
    trailing = b""
    pending = compressed

    try:
        while pending:
            allowance = MAX_TAR_SIZE - decompressed_size + 1
            produced = decompressor.decompress(pending, allowance)
            output.write(produced)
            decompressed_size += len(produced)
            if decompressed_size > MAX_TAR_SIZE:
                errors.append("decompressed tar stream exceeds 80 MiB")
                output.close()
                return None

            if decompressor.eof:
                trailing = decompressor.unused_data
                break
            pending = decompressor.unconsumed_tail
    except zlib.error as error:
        errors.append(f"invalid gzip member: {error}")
        output.close()
        return None

    if not decompressor.eof:
        errors.append("bundle is not one complete gzip member")
        output.close()
        return None
    if trailing:
        if trailing.startswith(b"\x1f\x8b"):
            errors.append("archive must contain exactly one gzip member")
        else:
            errors.append("data follows the single gzip member")
        output.close()
        return None

    output.seek(0)
    return output


def _parse_octal(field, description, errors):
    stripped = field.rstrip(b"\0 ").lstrip(b" ")
    if not stripped:
        return 0
    if any(byte not in b"01234567" for byte in stripped):
        errors.append(f"invalid octal {description} in tar header")
        return None
    return int(stripped, 8)


def _header_path(header):
    name = header[0:100].split(b"\0", 1)[0]
    prefix = header[345:500].split(b"\0", 1)[0]
    return prefix + b"/" + name if prefix else name


def _path_text(path):
    return path.decode("utf-8", errors="backslashreplace")


def _parse_pax_records(data, errors):
    records = {}
    offset = 0
    while offset < len(data):
        space = data.find(b" ", offset)
        if space < 0 or not data[offset:space].isdigit():
            errors.append("malformed pax extended-header record length")
            return records
        length = int(data[offset:space])
        end = offset + length
        if length <= space - offset + 2 or end > len(data):
            errors.append("malformed pax extended-header record length")
            return records
        record = data[space + 1 : end]
        if not record.endswith(b"\n") or b"=" not in record[:-1]:
            errors.append("malformed pax extended-header record")
            return records
        key, value = record[:-1].split(b"=", 1)
        try:
            key_text = key.decode("ascii")
        except UnicodeDecodeError:
            errors.append("pax extended-header key is not ASCII")
        else:
            records[key_text] = value
        offset = end
    return records


def _normalise_path(path, directory, errors):
    original = path
    if path.startswith(b"./"):
        path = path[2:]
    if directory and path.endswith(b"/"):
        path = path[:-1]
    if not path:
        return None

    shown = _path_text(original)
    if path.startswith(b"/"):
        errors.append(f"absolute path is not allowed: {shown!r}")
    components = path.split(b"/")
    if any(component in (b"", b".", b"..") for component in components):
        errors.append(f"invalid path component in {shown!r}")
    if b"\\" in path:
        errors.append(f"path must use '/' separators: {shown!r}")
    return path


def _read_payload(stream, size, *, collect, validate_utf8, label, errors):
    remaining = size
    chunks = [] if collect else None
    decoder = codecs.getincrementaldecoder("utf-8")() if validate_utf8 else None
    utf8_failed = False

    while remaining:
        chunk = stream.read(min(64 * 1024, remaining))
        if not chunk:
            errors.append(f"tar entry {label!r} is truncated")
            return None, False
        remaining -= len(chunk)
        if chunks is not None:
            chunks.append(chunk)
        if decoder is not None and not utf8_failed:
            try:
                decoder.decode(chunk, final=False)
            except UnicodeDecodeError:
                errors.append(f"file {label!r} is not valid UTF-8")
                utf8_failed = True

    if decoder is not None and not utf8_failed:
        try:
            decoder.decode(b"", final=True)
        except UnicodeDecodeError:
            errors.append(f"file {label!r} is not valid UTF-8")

    padding_size = (-size) % 512
    padding = stream.read(padding_size)
    if len(padding) != padding_size:
        errors.append(f"tar entry {label!r} has truncated padding")
        return None, False
    if any(padding):
        errors.append(f"tar entry {label!r} has non-zero padding")
    return b"".join(chunks) if chunks is not None else None, True


def _entry_type_error(typeflag):
    names = {
        b"1": "hard link",
        b"2": "symbolic link",
        b"3": "character device",
        b"4": "block device",
        b"6": "FIFO",
        b"L": "GNU long-name entry",
        b"K": "GNU long-link entry",
    }
    return names.get(typeflag, f"tar entry type {typeflag!r}") + " is not allowed"


def _validate_file_path(path, errors):
    if path == b"index.md":
        return
    parts = path.split(b"/")
    if (
        len(parts) != 2
        or parts[0] not in (b"resources", b"data-sources")
        or not parts[1].endswith(b".md")
    ):
        errors.append(f"file is not allowed: {_path_text(path)!r}")
        return

    slug_bytes = parts[1][:-3]
    if len(slug_bytes) > 128:
        errors.append(f"slug in {_path_text(path)!r} exceeds 128 bytes")
    try:
        slug = slug_bytes.decode("utf-8")
    except UnicodeDecodeError:
        slug = ""
    if not SLUG_RE.fullmatch(slug):
        errors.append(
            f"invalid slug in {_path_text(path)!r}: expected [a-z0-9][a-z0-9_]*"
        )


def _validate_tar(stream, errors):
    seen_paths = set()
    index_present = False
    entries = 0
    files_size = 0
    entry_limit_reported = False
    files_limit_reported = False
    pending_pax = None
    found_end = False
    header_number = 0

    while True:
        header = stream.read(512)
        if not header:
            break
        if len(header) != 512:
            errors.append("decompressed tar stream ends with a partial header block")
            break

        if not any(header):
            second = stream.read(512)
            if len(second) != 512:
                errors.append("tar end marker is truncated; expected two zero blocks")
                break
            if any(second):
                errors.append("tar end marker must contain two consecutive zero blocks")
                stream.seek(-512, os.SEEK_CUR)
                continue
            found_end = True
            if pending_pax is not None:
                errors.append("pax extended header is not followed by an archive entry")
            while True:
                trailing = stream.read(64 * 1024)
                if not trailing:
                    break
                if any(trailing):
                    errors.append("non-zero data follows the tar end marker")
                    break
            break

        header_number += 1
        label = f"header {header_number}"

        stored_checksum = _parse_octal(header[148:156], "checksum", errors)
        checksum_header = bytearray(header)
        checksum_header[148:156] = b"        "
        if stored_checksum is not None and stored_checksum != sum(checksum_header):
            errors.append(f"tar {label} has an invalid checksum")

        signature = header[257:265]
        if signature == b"ustar  \0":
            errors.append(f"tar {label} uses the GNU tar header format")
        elif signature != b"ustar\x00" + b"00":
            errors.append(f"tar {label} is not POSIX ustar or pax")

        typeflag = header[156:157]
        header_size = _parse_octal(header[124:136], "size", errors)
        if header_size is None:
            errors.append(f"cannot locate the payload for tar {label}")
            break

        raw_path = _header_path(header)
        if typeflag == b"x":
            pax_data, complete = _read_payload(
                stream,
                header_size,
                collect=True,
                validate_utf8=False,
                label=_path_text(raw_path),
                errors=errors,
            )
            if not complete:
                break
            if pending_pax is None:
                pending_pax = {}
            pending_pax.update(_parse_pax_records(pax_data, errors))
            continue

        pax_overrides = pending_pax or {}
        effective_path = pax_overrides.get("path", raw_path)
        effective_size = header_size
        if "size" in pax_overrides:
            pax_size = pax_overrides["size"]
            if not pax_size.isdigit():
                errors.append("pax size override must be a non-negative decimal integer")
            else:
                effective_size = int(pax_size)
                if effective_size > MAX_TAR_SIZE:
                    errors.append("pax size override exceeds the 80 MiB tar limit")
        pending_pax = None

        if typeflag == b"g":
            errors.append("pax global header is not allowed")

        regular = typeflag in (b"", b"\0", b"0")
        directory = typeflag == b"5"
        if not regular and not directory and typeflag != b"g":
            errors.append(_entry_type_error(typeflag))

        path = _normalise_path(effective_path, directory, errors)
        path_label = _path_text(effective_path)
        if path is not None:
            if typeflag != b"g":
                entries += 1
                if entries > MAX_ENTRIES and not entry_limit_reported:
                    errors.append("archive contains more than 4096 entries")
                    entry_limit_reported = True

            if path in seen_paths:
                errors.append(f"duplicate path: {_path_text(path)!r}")
            else:
                seen_paths.add(path)

            if directory:
                if path not in (b"resources", b"data-sources"):
                    errors.append(f"directory is not allowed: {_path_text(path)!r}")
            elif regular:
                _validate_file_path(path, errors)
                if path == b"index.md":
                    index_present = True

        if regular:
            files_size += effective_size
            if effective_size > MAX_FILE_SIZE:
                errors.append(f"file {path_label!r} exceeds 1 MiB")
            if files_size > MAX_FILES_SIZE and not files_limit_reported:
                errors.append("regular files total more than 64 MiB")
                files_limit_reported = True

        _, complete = _read_payload(
            stream,
            effective_size,
            collect=False,
            validate_utf8=regular,
            label=path_label,
            errors=errors,
        )
        if not complete:
            break

    if not found_end:
        errors.append("tar archive is missing its two-block end marker")
    if not index_present:
        errors.append("index.md is required at the archive root")


def check_bundle(bundle):
    bundle = Path(bundle)
    errors = []
    stream = _decompress_gzip(bundle, errors)
    if stream is not None:
        try:
            _validate_tar(stream, errors)
        finally:
            stream.close()

    if errors:
        for error in errors:
            print(f"error: {error}", file=sys.stderr)
        return False

    print(f"checked {bundle}: valid docs bundle")
    return True


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    build_parser = subparsers.add_parser("build", help="build the docs bundle")
    build_parser.add_argument("output", type=Path)
    check_parser = subparsers.add_parser("check", help="validate a docs bundle")
    check_parser.add_argument("bundle", type=Path)
    args = parser.parse_args(argv)

    try:
        if args.command == "build":
            build_bundle(args.output)
            return 0
        return 0 if check_bundle(args.bundle) else 1
    except (BundleError, OSError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
