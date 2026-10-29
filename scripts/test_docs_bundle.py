#!/usr/bin/env python3

import gzip
import io
import json
import os
import runpy
import subprocess
import sys
import tarfile
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "docs-bundle.py"
DOCS_BUNDLE = runpy.run_path(str(SCRIPT))


def _octal(value, width):
    return f"{value:0{width - 1}o}\0".encode("ascii")


def tar_entry(
    name,
    data=b"",
    *,
    typeflag=b"0",
    linkname=b"",
    magic=b"ustar\0",
    version=b"00",
    declared_size=None,
):
    if isinstance(name, str):
        name = name.encode("utf-8")
    if len(name) > 100:
        raise ValueError("test tar entry name exceeds the ustar name field")
    if len(linkname) > 100:
        raise ValueError("test tar entry link name exceeds the ustar field")

    size = len(data) if declared_size is None else declared_size
    header = bytearray(512)
    header[0 : len(name)] = name
    header[100:108] = _octal(0o644, 8)
    header[108:116] = _octal(0, 8)
    header[116:124] = _octal(0, 8)
    header[124:136] = _octal(size, 12)
    header[136:148] = _octal(0, 12)
    header[148:156] = b"        "
    header[156:157] = typeflag
    header[157 : 157 + len(linkname)] = linkname
    header[257:263] = magic
    header[263:265] = version
    checksum = sum(header)
    header[148:156] = f"{checksum:06o}\0 ".encode("ascii")

    padding = b"\0" * ((-len(data)) % 512)
    return bytes(header) + data + padding


def pax_record(key, value):
    body = f"{key}={value}\n".encode("utf-8")
    length = len(body) + 2
    while True:
        record = str(length).encode("ascii") + b" " + body
        if len(record) == length:
            return record
        length = len(record)


def tar_stream(*entries):
    return b"".join(entries) + b"\0" * 1024


def write_bundle(path, *entries):
    path.write_bytes(gzip.compress(tar_stream(*entries), mtime=0))


NO_SERVICES_JSON = object()


def service_entry(
    *, service_id="core", title="Core", resources=None, data_sources=None, **extra
):
    entry = {
        "id": service_id,
        "title": title,
        "resources": ["alpha"] if resources is None else resources,
        "data_sources": ["beta"] if data_sources is None else data_sources,
    }
    entry.update(extra)
    return entry


def docs_entries(
    services=NO_SERVICES_JSON,
    *,
    resources=("alpha",),
    data_sources=("beta",),
):
    entries = [tar_entry("index.md", b"# Index\n")]
    entries.extend(
        tar_entry(f"resources/{slug}.md", f"# {slug}\n".encode())
        for slug in resources
    )
    entries.extend(
        tar_entry(f"data-sources/{slug}.md", f"# {slug}\n".encode())
        for slug in data_sources
    )
    if services is not NO_SERVICES_JSON:
        if not isinstance(services, bytes):
            services = json.dumps(services).encode("utf-8")
        entries.append(tar_entry("services.json", services))
    return entries


def entry_limit_entries(page_count):
    return [
        tar_entry("./", typeflag=b"5"),
        tar_entry("index.md", b"# Index\n"),
        tar_entry("resources/", typeflag=b"5"),
        *(
            tar_entry(f"resources/page_{index:04d}.md")
            for index in range(page_count)
        ),
    ]


def write_generated_page(path, title):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        f'---\ntitle: "{title}"\ndescription: "Fixture page"\n---\n\n'
        f"Documentation for {title}.\n",
        encoding="utf-8",
    )


def create_build_fixture(
    directory, *, reverse_creation=False, file_mode=0o644, mtime=0
):
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=True)
    source_root = directory / "generated-docs" / "aws"
    pages = [
        ("alpha/standalone.md", "aws.alpha.Standalone"),
        ("beta/shared.md", "aws.beta.Shared"),
        ("beta/shared_data_source.md", "aws.beta.Shared"),
        ("gamma/lookup_data_source.md", "aws.gamma.Lookup"),
    ]
    index_source = directory / "index.md"
    service_titles_source = directory / "service-titles.json"
    service_titles_source.write_text(
        json.dumps(
            {"alpha": "Alpha", "beta": "Beta", "gamma": "Gamma"},
            indent=2,
            sort_keys=True,
        )
        + "\n",
        encoding="utf-8",
    )

    if reverse_creation:
        index_source.write_text(
            "# AWS fixture\n\nFixture introduction.\n", encoding="utf-8"
        )
        pages.reverse()

    page_paths = []
    for relative, title in pages:
        page_path = source_root / relative
        page_paths.append(page_path)
        write_generated_page(page_path, title)

    if not reverse_creation:
        index_source.write_text(
            "# AWS fixture\n\nFixture introduction.\n", encoding="utf-8"
        )

    for fixture_file in [index_source, service_titles_source, *page_paths]:
        os.chmod(fixture_file, file_mode)
        os.utime(fixture_file, (mtime, mtime))

    return source_root, index_source, service_titles_source


class DocsBundleTests(unittest.TestCase):
    def run_script(self, *args):
        return subprocess.run(
            [sys.executable, str(SCRIPT), *map(str, args)],
            cwd=ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )

    def assert_check_rejects(self, entries, message):
        with tempfile.TemporaryDirectory() as directory:
            bundle = Path(directory) / "docs.tar.gz"
            write_bundle(bundle, *entries)
            result = self.run_script("check", bundle)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(message.lower(), result.stderr.lower())

    def assert_check_accepts(self, entries):
        with tempfile.TemporaryDirectory() as directory:
            bundle = Path(directory) / "docs.tar.gz"
            write_bundle(bundle, *entries)
            result = self.run_script("check", bundle)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def build_fixture(self, directory, filename="docs.tar.gz", **fixture_options):
        directory = Path(directory)
        source_root, index_source, service_titles_source = create_build_fixture(
            directory, **fixture_options
        )
        bundle = directory / filename
        with redirect_stdout(io.StringIO()):
            DOCS_BUNDLE["build_bundle"](
                bundle,
                source_root,
                index_source,
                service_titles_source,
            )
        return bundle

    def test_build_round_trip_passes_check(self):
        with tempfile.TemporaryDirectory() as directory:
            bundle = self.build_fixture(directory)
            check = self.run_script("check", bundle)
            self.assertEqual(check.returncode, 0, check.stdout + check.stderr)

            with tarfile.open(bundle, "r:gz") as archive:
                member = archive.extractfile("services.json")
                self.assertIsNotNone(member)
                services = json.load(member)

        self.assertEqual(
            services,
            {
                "services": [
                    {
                        "id": "alpha",
                        "title": "Alpha",
                        "resources": ["alpha_standalone"],
                        "data_sources": [],
                    },
                    {
                        "id": "beta",
                        "title": "Beta",
                        "resources": ["beta_shared"],
                        "data_sources": ["beta_shared"],
                    },
                    {
                        "id": "gamma",
                        "title": "Gamma",
                        "resources": [],
                        "data_sources": ["gamma_lookup"],
                    },
                ]
            },
        )

    def test_build_is_byte_reproducible(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            first_root = directory / "first"
            second_root = directory / "second"
            first = self.build_fixture(
                first_root,
                file_mode=0o600,
                mtime=1_000_000_000,
            )
            second = self.build_fixture(
                second_root,
                reverse_creation=True,
                file_mode=0o644,
                mtime=1_500_000_000,
            )

            first_page = first_root / "generated-docs/aws/alpha/standalone.md"
            second_page = second_root / "generated-docs/aws/alpha/standalone.md"
            self.assertEqual(first_page.stat().st_mode & 0o777, 0o600)
            self.assertEqual(second_page.stat().st_mode & 0o777, 0o644)
            self.assertNotEqual(
                first_page.stat().st_mtime_ns,
                second_page.stat().st_mtime_ns,
            )

            self.assertEqual(first.read_bytes(), second.read_bytes())

    def test_build_replaces_frontmatter_with_title_heading(self):
        with tempfile.TemporaryDirectory() as directory:
            bundle = self.build_fixture(directory)

            with tarfile.open(bundle, "r:gz") as archive:
                member = archive.extractfile("resources/alpha_standalone.md")
                self.assertIsNotNone(member)
                content = member.read().decode("utf-8")

        self.assertTrue(content.startswith("# aws.alpha.Standalone\n\n"))
        first_lines = "\n".join(content.splitlines()[:6])
        self.assertNotIn("---", first_lines)
        self.assertNotIn("title:", first_lines)

    def test_build_ships_the_same_heading_list_codegen_validated(self):
        validated = (
            "# aws.alpha.Example\n\n"
            "Fixture page.\n\n"
            "## Example\n\n"
            "```crn\nresource example \"fixture\" {}\n```\n\n"
            "## Argument Reference\n\n"
            "### `example`\n\n"
            "## Struct Definitions\n\n"
            "### Example\n"
        )
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            source_root = directory / "generated-docs" / "aws"
            source = source_root / "alpha" / "example.md"
            source.parent.mkdir(parents=True)
            source.write_text(
                '---\ntitle: "aws.alpha.Example"\n'
                'description: "Fixture page"\n---\n\n'
                + validated.split("\n", 1)[1].lstrip("\n"),
                encoding="utf-8",
            )
            index_source = directory / "index.md"
            index_source.write_text("# AWS fixture\n", encoding="utf-8")
            service_titles_source = directory / "service-titles.json"
            service_titles_source.write_text(
                json.dumps({"alpha": "Alpha"}),
                encoding="utf-8",
            )
            bundle = directory / "docs.tar.gz"
            with redirect_stdout(io.StringIO()):
                DOCS_BUNDLE["build_bundle"](
                    bundle,
                    source_root,
                    index_source,
                    service_titles_source,
                )

            with tarfile.open(bundle, "r:gz") as archive:
                member = archive.extractfile("resources/alpha_example.md")
                self.assertIsNotNone(member)
                shipped = member.read().decode("utf-8")

        heading_lines = lambda markdown: [
            line
            for line in markdown.splitlines()
            if line.startswith("#") and line.lstrip("#").startswith(" ")
        ]
        self.assertEqual(shipped, validated)
        self.assertEqual(heading_lines(shipped), heading_lines(validated))

    def test_build_splits_resources_and_data_sources_and_indexes_both(self):
        with tempfile.TemporaryDirectory() as directory:
            bundle = self.build_fixture(directory)
            with tarfile.open(bundle, "r:gz") as archive:
                self.assertEqual(
                    set(archive.getnames()),
                    {
                        "data-sources",
                        "data-sources/beta_shared.md",
                        "data-sources/gamma_lookup.md",
                        "index.md",
                        "resources",
                        "resources/alpha_standalone.md",
                        "resources/beta_shared.md",
                        "services.json",
                    },
                )
                member = archive.extractfile("index.md")
                self.assertIsNotNone(member)
                index = member.read().decode("utf-8")

        self.assertIn("## Resources", index)
        self.assertIn(
            "[aws.alpha.Standalone](resources/alpha_standalone.md)", index
        )
        self.assertIn("[aws.beta.Shared](resources/beta_shared.md)", index)
        self.assertIn("## Data Sources", index)
        self.assertIn("[aws.beta.Shared](data-sources/beta_shared.md)", index)
        self.assertIn("[aws.gamma.Lookup](data-sources/gamma_lookup.md)", index)

    def test_build_rejects_empty_generated_docs(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            source_root = directory / "generated-docs" / "aws"
            source_root.mkdir(parents=True)
            index_source = directory / "index.md"
            index_source.write_text("# AWS\n", encoding="utf-8")

            with self.assertRaisesRegex(
                DOCS_BUNDLE["BundleError"],
                "no resource or data-source documentation pages found",
            ):
                DOCS_BUNDLE["build_bundle"](
                    directory / "docs.tar.gz", source_root, index_source
                )

    def test_build_rejects_service_missing_from_titles(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            source_root, index_source, service_titles_source = create_build_fixture(
                directory
            )
            service_titles_source.write_text(
                json.dumps({"alpha": "Alpha", "beta": "Beta"}),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                DOCS_BUNDLE["BundleError"],
                "gamma.*no display title",
            ):
                DOCS_BUNDLE["build_bundle"](
                    directory / "docs.tar.gz",
                    source_root,
                    index_source,
                    service_titles_source,
                )

    def test_build_rejects_invalid_service_title(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            source_root, index_source, service_titles_source = create_build_fixture(
                directory
            )
            service_titles_source.write_text(
                json.dumps(
                    {
                        "alpha": "Alpha\u200b",
                        "beta": "Beta",
                        "gamma": "Gamma",
                    }
                ),
                encoding="utf-8",
            )

            with self.assertRaisesRegex(
                DOCS_BUNDLE["BundleError"],
                "alpha.*forbidden character U\\+200B",
            ):
                DOCS_BUNDLE["build_bundle"](
                    directory / "docs.tar.gz",
                    source_root,
                    index_source,
                    service_titles_source,
                )

    def test_index_omits_empty_sections(self):
        Page = DOCS_BUNDLE["Page"]
        cases = (
            (
                [Page("resources/s3_bucket.md", "aws.s3.Bucket", b"")],
                "## Resources",
                "## Data Sources",
            ),
            (
                [
                    Page(
                        "data-sources/s3_bucket.md",
                        "aws.s3.Bucket",
                        b"",
                    )
                ],
                "## Data Sources",
                "## Resources",
            ),
        )
        with tempfile.TemporaryDirectory() as directory:
            index_source = Path(directory) / "index.md"
            index_source.write_text("# AWS\n", encoding="utf-8")
            for pages, present, absent in cases:
                with self.subTest(present=present):
                    index = DOCS_BUNDLE["_build_index"](
                        pages, index_source
                    ).decode("utf-8")
                    self.assertIn(present, index)
                    self.assertNotIn(absent, index)

    def test_check_accepts_bundle_without_services_json(self):
        self.assert_check_accepts(docs_entries())

    def test_check_accepts_128_character_multibyte_service_title(self):
        services = {
            "services": [service_entry(title="界" * 128)],
        }
        self.assert_check_accepts(docs_entries(services))

    def test_check_accepts_128_byte_service_id(self):
        services = {
            "services": [service_entry(service_id="a" * 128)],
        }
        self.assert_check_accepts(docs_entries(services))

    def test_check_rejects_unknown_services_json_root_key(self):
        services = {
            "services": [service_entry()],
            "unknown": True,
        }
        self.assert_check_rejects(docs_entries(services), "unknown key 'unknown'")

    def test_check_rejects_unknown_service_entry_key(self):
        services = {
            "services": [service_entry(unknown=True)],
        }
        self.assert_check_rejects(docs_entries(services), "unknown key 'unknown'")

    def test_check_rejects_duplicate_services_json_keys(self):
        cases = (
            (
                b'{"services":[],"services":[]}',
                "duplicate key 'services'",
            ),
            (
                b'{"services":[{"id":"core","id":"other",'
                b'"title":"Core","resources":["alpha"],'
                b'"data_sources":["beta"]}]}',
                "duplicate key 'id'",
            ),
        )
        for services, message in cases:
            with self.subTest(message=message):
                self.assert_check_rejects(docs_entries(services), message)

    def test_check_rejects_empty_services(self):
        self.assert_check_rejects(
            docs_entries({"services": []}),
            "'services' must not be empty",
        )

    def test_check_rejects_bad_service_id(self):
        services = {
            "services": [service_entry(service_id="Bad-Id")],
        }
        self.assert_check_rejects(docs_entries(services), "invalid id")

    def test_check_rejects_129_byte_service_id(self):
        services = {
            "services": [service_entry(service_id="a" * 129)],
        }
        self.assert_check_rejects(docs_entries(services), "id exceeds 128 bytes")

    def test_check_rejects_empty_service_title(self):
        services = {"services": [service_entry(title="")]}
        self.assert_check_rejects(docs_entries(services), "title must not be empty")

    def test_check_rejects_129_character_service_title(self):
        services = {"services": [service_entry(title="x" * 129)]}
        self.assert_check_rejects(
            docs_entries(services),
            "title exceeds 128 Unicode code points",
        )

    def test_check_rejects_control_character_in_service_title(self):
        services = {"services": [service_entry(title="Core\nService")]}
        self.assert_check_rejects(
            docs_entries(services),
            "forbidden character U+000A",
        )

    def test_check_rejects_forbidden_format_characters_in_service_title(self):
        codepoints = (
            0x00AD,
            0x061C,
            0x180E,
            0x200B,
            0x200F,
            0x2028,
            0x202E,
            0x2060,
            0x206F,
            0xFEFF,
            0xFFF9,
            0xFFFB,
        )
        for codepoint in codepoints:
            with self.subTest(codepoint=f"U+{codepoint:04X}"):
                services = {
                    "services": [
                        service_entry(title=f"Core{chr(codepoint)}Service")
                    ]
                }
                self.assert_check_rejects(
                    docs_entries(services),
                    f"forbidden character U+{codepoint:04X}",
                )

    def test_check_rejects_duplicate_service_id(self):
        services = {
            "services": [
                service_entry(resources=["alpha"], data_sources=[]),
                service_entry(
                    title="Other",
                    resources=[],
                    data_sources=["beta"],
                ),
            ]
        }
        self.assert_check_rejects(docs_entries(services), "duplicates service id")

    def test_check_rejects_duplicate_service_title(self):
        services = {
            "services": [
                service_entry(resources=["alpha"], data_sources=[]),
                service_entry(
                    service_id="other",
                    resources=[],
                    data_sources=["beta"],
                ),
            ]
        }
        self.assert_check_rejects(docs_entries(services), "duplicates service title")

    def test_check_rejects_unknown_service_slug(self):
        services = {
            "services": [service_entry(resources=["missing"])],
        }
        self.assert_check_rejects(docs_entries(services), "unknown slug 'missing'")

    def test_check_rejects_service_slug_in_wrong_section(self):
        services = {
            "services": [
                service_entry(resources=["alpha", "beta"], data_sources=[])
            ],
        }
        self.assert_check_rejects(docs_entries(services), "wrong section")

    def test_check_rejects_page_assigned_twice(self):
        services = {
            "services": [service_entry(resources=["alpha", "alpha"])],
        }
        self.assert_check_rejects(docs_entries(services), "appears more than once")

    def test_check_rejects_unassigned_page(self):
        services = {"services": [service_entry()]}
        self.assert_check_rejects(
            docs_entries(services, resources=("alpha", "spare")),
            "page 'resources/spare.md' is not assigned",
        )

    def test_check_rejects_service_without_pages(self):
        services = {
            "services": [
                service_entry(),
                service_entry(
                    service_id="empty",
                    title="Empty",
                    resources=[],
                    data_sources=[],
                ),
            ]
        }
        self.assert_check_rejects(
            docs_entries(services),
            "must contain at least one page",
        )

    def test_check_accepts_pax_path_and_size_overrides(self):
        records = pax_record("path", "resources/pax_entry.md")
        records += pax_record("size", "6")
        entries = (
            tar_entry("pax-header", records, typeflag=b"x"),
            tar_entry("ignored", b"hello\n"),
            tar_entry("index.md", b"# Index\n"),
        )
        with tempfile.TemporaryDirectory() as directory:
            bundle = Path(directory) / "docs.tar.gz"
            write_bundle(bundle, *entries)
            result = self.run_script("check", bundle)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_check_entry_limit_ignores_empty_root(self):
        with tempfile.TemporaryDirectory() as directory:
            directory = Path(directory)
            for page_count, accepted in ((4094, True), (4095, False)):
                with self.subTest(real_entries=page_count + 2):
                    bundle = directory / f"{page_count}.tar.gz"
                    write_bundle(bundle, *entry_limit_entries(page_count))
                    result = self.run_script("check", bundle)
                    if accepted:
                        self.assertEqual(
                            result.returncode, 0, result.stdout + result.stderr
                        )
                    else:
                        self.assertNotEqual(
                            result.returncode, 0, result.stdout + result.stderr
                        )
                        self.assertIn(
                            "more than 4096 entries", result.stderr.lower()
                        )

    def test_check_counts_services_json_toward_entry_limit(self):
        entries = entry_limit_entries(4094)
        entries.append(tar_entry("services.json", b"{}"))
        self.assert_check_rejects(entries, "more than 4096 entries")

    def test_check_rejects_missing_index(self):
        self.assert_check_rejects(
            [tar_entry("resources/example.md", b"# Example\n")],
            "index.md is required",
        )

    def test_check_rejects_bad_slug(self):
        self.assert_check_rejects(
            [
                tar_entry("index.md", b"# Index\n"),
                tar_entry("resources/Bad-Slug.md", b"# Bad\n"),
            ],
            "invalid slug",
        )

    def test_check_rejects_extra_file(self):
        self.assert_check_rejects(
            [
                tar_entry("index.md", b"# Index\n"),
                tar_entry("README.md", b"extra\n"),
            ],
            "file is not allowed",
        )

    def test_check_rejects_extra_directory(self):
        self.assert_check_rejects(
            [
                tar_entry("index.md", b"# Index\n"),
                tar_entry("other/", typeflag=b"5"),
            ],
            "directory is not allowed",
        )

    def test_check_rejects_symlink(self):
        self.assert_check_rejects(
            [
                tar_entry("index.md", b"# Index\n"),
                tar_entry(
                    "resources/link.md",
                    typeflag=b"2",
                    linkname=b"../index.md",
                ),
            ],
            "symbolic link",
        )

    def test_check_rejects_gnu_long_name_entry(self):
        self.assert_check_rejects(
            [
                tar_entry("index.md", b"# Index\n"),
                tar_entry(
                    "././@LongLink",
                    b"resources/a_very_long_name.md\0",
                    typeflag=b"L",
                    magic=b"ustar ",
                    version=b" \0",
                ),
            ],
            "GNU long-name",
        )

    def test_check_rejects_pax_global_header(self):
        records = pax_record("comment", "not allowed")
        self.assert_check_rejects(
            [
                tar_entry("pax-global", records, typeflag=b"g"),
                tar_entry("index.md", b"# Index\n"),
            ],
            "pax global header",
        )

    def test_check_rejects_two_gzip_members(self):
        archive = tar_stream(tar_entry("index.md", b"# Index\n"))
        with tempfile.TemporaryDirectory() as directory:
            bundle = Path(directory) / "docs.tar.gz"
            bundle.write_bytes(
                gzip.compress(archive, mtime=0)
                + gzip.compress(archive, mtime=0)
            )
            result = self.run_script("check", bundle)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("exactly one gzip member", result.stderr.lower())

    def test_check_rejects_nonzero_data_after_end_marker(self):
        archive = tar_stream(tar_entry("index.md", b"# Index\n")) + b"\x01"
        with tempfile.TemporaryDirectory() as directory:
            bundle = Path(directory) / "docs.tar.gz"
            bundle.write_bytes(gzip.compress(archive, mtime=0))
            result = self.run_script("check", bundle)
        self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn(
            "non-zero data follows the tar end marker", result.stderr.lower()
        )

    def test_check_rejects_non_utf8_file(self):
        self.assert_check_rejects(
            [tar_entry("index.md", b"# Index\n\xff")],
            "not valid UTF-8",
        )

    def test_check_rejects_parent_path_component(self):
        self.assert_check_rejects(
            [
                tar_entry("index.md", b"# Index\n"),
                tar_entry("resources/../escape.md", b"# Escape\n"),
            ],
            "invalid path component",
        )

    def test_check_rejects_absolute_path(self):
        self.assert_check_rejects(
            [
                tar_entry("index.md", b"# Index\n"),
                tar_entry("/resources/escape.md", b"# Escape\n"),
            ],
            "absolute path",
        )

    def test_check_rejects_duplicate_path(self):
        self.assert_check_rejects(
            [
                tar_entry("index.md", b"# First\n"),
                tar_entry("./index.md", b"# Second\n"),
            ],
            "duplicate path",
        )

    def test_check_rejects_oversized_file(self):
        self.assert_check_rejects(
            [
                tar_entry("index.md", b"# Index\n"),
                tar_entry("resources/large.md", b"x" * (1024 * 1024 + 1)),
            ],
            "exceeds 1 MiB",
        )


if __name__ == "__main__":
    unittest.main()
