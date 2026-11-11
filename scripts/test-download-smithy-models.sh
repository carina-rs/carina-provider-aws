#!/usr/bin/env bash
# Exercise revision validation and cache isolation with an offline curl fixture.
set -euo pipefail
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
python3 - "$repo_root" <<'PY'
from pathlib import Path
import json
import os
import shutil
import subprocess
import sys
import tempfile
import textwrap
import unittest

REPO = Path(sys.argv.pop(1)).resolve()
BASH = shutil.which("bash")


class SmithyModelDownloadTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="carina-smithy-models-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name) / "repository with spaces"
        self.scripts = self.root / "scripts"
        self.scripts.mkdir(parents=True)
        for name in ("download-smithy-models.sh", "smithy-models-dir.sh"):
            shutil.copy2(REPO / "scripts" / name, self.scripts / name)
        self.pin = self.root / "smithy-models.rev"
        shutil.copy2(REPO / "smithy-models.rev", self.pin)
        self.revision = self.pin.read_text().strip()
        self.cache = self.root / "carina-provider-aws/tests/fixtures/smithy"
        self.trace = self.root / "curl.jsonl"
        binary = self.root / "bin"
        binary.mkdir()
        curl = binary / "curl"
        curl.write_text(textwrap.dedent("""\
            #!/usr/bin/env python3
            from pathlib import Path
            import json
            import os
            import sys

            arguments = sys.argv[1:]
            with open(os.environ["SMITHY_TEST_CURL_TRACE"], "a") as trace:
                trace.write(json.dumps(arguments) + "\\n")
            output = Path(arguments[arguments.index("-o") + 1])
            output.write_text("incomplete" if os.environ.get("SMITHY_TEST_CURL_FAIL")
                              else arguments[1])
            sys.exit(23 if os.environ.get("SMITHY_TEST_CURL_FAIL") else 0)
        """))
        curl.chmod(0o755)
        self.environment = dict(os.environ, PATH=f"{binary}{os.pathsep}{os.environ['PATH']}",
                                SMITHY_TEST_CURL_TRACE=str(self.trace))

    def run_script(self, name="download-smithy-models.sh", expected_status=0):
        result = subprocess.run([BASH, str(self.scripts / name)], cwd=self.root.parent,
                                env=self.environment, capture_output=True, text=True,
                                timeout=30)
        self.assertEqual(result.returncode, expected_status, result.stdout + result.stderr)
        return result

    def requests(self):
        return [json.loads(line) for line in self.trace.read_text().splitlines()]

    def test_missing_revision_fails_before_network_or_cache_creation(self):
        self.pin.unlink()
        result = self.run_script(expected_status=1)
        self.assertIn("Missing Smithy model revision file", result.stderr)
        self.assertIn("smithy-models.rev", result.stderr)
        self.assertFalse(self.trace.exists())
        self.assertFalse(self.cache.exists())

    def test_malformed_revisions_fail_before_network_or_cache_creation(self):
        for value in ("", "main\n", "a" * 39, "a" * 41, "g" * 40,
                      f" {self.revision}\n", f"{self.revision} \n",
                      f"{self.revision}\n{self.revision}\n"):
            with self.subTest(value=value):
                self.pin.write_text(value)
                result = self.run_script(expected_status=1)
                self.assertIn("smithy-models.rev must contain a full 40-character hex commit SHA",
                              result.stderr)
                self.assertFalse(self.trace.exists())
                self.assertFalse(self.cache.exists())

    def test_path_helper_accepts_full_hex_with_or_without_final_newline(self):
        for revision in (self.revision, self.revision.upper()):
            for ending in ("", "\n"):
                with self.subTest(revision=revision, ending=ending):
                    self.pin.write_text(revision + ending)
                    result = self.run_script("smithy-models-dir.sh")
                    self.assertEqual(result.stdout, f"{self.cache / revision}\n")
        self.assertFalse(self.cache.exists())
        self.assertFalse(self.trace.exists())

    def test_downloads_use_pinned_urls_and_ignore_the_legacy_flat_cache(self):
        self.cache.mkdir(parents=True)
        legacy = self.cache / "ec2.json"
        legacy.write_text("stale model from main")
        self.run_script()
        requests = self.requests()
        self.assertEqual(len(requests), 11)
        for arguments in requests:
            self.assertTrue(arguments[1].startswith(
                f"https://raw.githubusercontent.com/aws/api-models-aws/{self.revision}/models/"))
        models = list((self.cache / self.revision).glob("*.json"))
        self.assertEqual(len(models), 11)
        self.assertEqual(legacy.read_text(), "stale model from main")
        self.assertIn(f"/{self.revision}/models/ec2/",
                      (self.cache / self.revision / "ec2.json").read_text())

    def test_second_download_reuses_the_same_revision(self):
        self.run_script()
        requests = self.requests()
        result = self.run_script()
        self.assertEqual(self.requests(), requests)
        self.assertNotIn("Downloading ", result.stdout)

    def test_changed_pin_uses_a_separate_cache(self):
        self.run_script()
        old_model = self.cache / self.revision / "ec2.json"
        old_contents = old_model.read_text()
        other = ("0" if self.revision[0] != "0" else "1") + self.revision[1:]
        self.pin.write_text(other + "\n")
        result = self.run_script("smithy-models-dir.sh")
        self.assertEqual(result.stdout, f"{self.cache / other}\n")
        self.run_script()
        self.assertEqual(len(self.requests()), 22)
        for arguments in self.requests()[11:]:
            self.assertIn(f"/{other}/models/", arguments[1])
        self.assertIn(f"/{other}/models/", (self.cache / other / "ec2.json").read_text())
        self.assertEqual(old_model.read_text(), old_contents)

    def test_failed_download_cannot_be_reused_as_a_cached_model(self):
        self.environment["SMITHY_TEST_CURL_FAIL"] = "1"
        self.run_script(expected_status=23)
        self.assertEqual(list((self.cache / self.revision).iterdir()), [])
        del self.environment["SMITHY_TEST_CURL_FAIL"]
        self.run_script()
        self.assertEqual(len(self.requests()), 12)
        self.assertIn(f"/{self.revision}/models/", (self.cache / self.revision / "ec2.json").read_text())


unittest.main()
PY
