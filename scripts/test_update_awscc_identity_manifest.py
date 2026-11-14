"""Exercise the refresh gate offline, without creating or modifying Git repositories."""
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[1]
UPDATER = ROOT / "scripts/update-awscc-identity-manifest.py"
COMMIT = "a" * 40


class ManifestRefreshTests(unittest.TestCase):
    def setUp(self):
        (ROOT / "target").mkdir(exist_ok=True)
        temporary = tempfile.TemporaryDirectory(dir=ROOT / "target", prefix="manifest-gate-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        scripts = self.root / "aws/scripts"
        scripts.mkdir(parents=True)
        self.script = scripts / UPDATER.name
        shutil.copy2(UPDATER, self.script)
        self.source = self.root / "awscc"
        self.source.mkdir()
        (self.source / "Cargo.toml").write_text('''[workspace.dependencies]
carina-core = { git = "https://github.com/carina-rs/carina", rev = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb" }
carina-aws-types = { git = "https://github.com/carina-rs/carina-provider-aws", rev = "cccccccccccccccccccccccccccccccccccccccc" }
''')
        for name in ["Cargo.lock", "carina-provider-awscc/src/partition.rs", *(
            "carina-provider-awscc/src/schemas/" + name + ".rs"
            for name in ["config", "catalog", "awscc_types", "pattern", "scheduler", "string_identity"]
        )]:
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("")
        binary = self.root / "bin"
        binary.mkdir()
        self.trace = self.root / "commands.jsonl"
        git = binary / "git"
        git.write_text(textwrap.dedent(f'''\
            #!{sys.executable}
            import json, os, sys
            args = sys.argv[3:]
            with open(os.environ["MANIFEST_TEST_TRACE"], "a") as trace:
                trace.write(json.dumps(["git", *args]) + "\\n")
            if args == ["rev-parse", "HEAD"]:
                print({COMMIT!r})
            elif args == ["merge-base", "--is-ancestor", {COMMIT!r}, "origin/main"]:
                sys.exit(int(os.environ["MANIFEST_TEST_ANCESTRY"]))
            elif args[:2] == ["status", "--porcelain"]:
                pass
            else:
                raise SystemExit("Unexpected Git command: " + repr(args))
        '''))
        git.chmod(0o755)
        cargo = binary / "cargo"
        cargo.write_text(textwrap.dedent(f'''\
            #!{sys.executable}
            import json, os, sys
            assert os.environ["CARGO_NET_OFFLINE"] == "true"
            with open(os.environ["MANIFEST_TEST_TRACE"], "a") as trace:
                trace.write(json.dumps(["cargo", *sys.argv[1:]]) + "\\n")
            print("offline Cargo sentinel", file=sys.stderr)
            sys.exit(41)
        '''))
        cargo.chmod(0o755)
        self.environment = dict(os.environ, PATH=str(binary) + os.pathsep + os.environ["PATH"],
                                MANIFEST_TEST_TRACE=str(self.trace), MANIFEST_TEST_ANCESTRY="1")

    def run_updater(self, *arguments):
        result = subprocess.run([sys.executable, str(self.script), *map(str, arguments)],
                                env=self.environment, capture_output=True, text=True, timeout=30)
        calls = [json.loads(line) for line in self.trace.read_text().splitlines()] if self.trace.exists() else []
        return result, calls

    def test_unpublished_commit_is_rejected_before_cargo_or_scratch_writes(self):
        result, calls = self.run_updater(self.source)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("not reachable from origin/main", result.stderr)
        self.assertFalse(any(call[0] == "cargo" for call in calls))
        self.assertFalse((self.root / "aws/target").exists())

    def test_git_ancestry_errors_fail_closed(self):
        self.environment["MANIFEST_TEST_ANCESTRY"] = "128"
        result, calls = self.run_updater(self.source)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("origin/main", result.stderr)
        self.assertFalse(any(call[0] == "cargo" for call in calls))

    def test_published_ancestor_is_allowed(self):
        self.environment["MANIFEST_TEST_ANCESTRY"] = "0"
        result, calls = self.run_updater(self.source)
        self.assertIn(["git", "merge-base", "--is-ancestor", COMMIT, "origin/main"], calls)
        self.assertIn("offline Cargo sentinel", result.stderr)

    def test_explicit_allow_unpublished_bypasses_only_the_ancestry_gate(self):
        result, calls = self.run_updater("--allow-unpublished", self.source)
        self.assertIn("offline Cargo sentinel", result.stderr)
        self.assertIn(["git", "rev-parse", "HEAD"], calls)
        self.assertTrue(any(call[0] == "git" and call[1:3] == ["status", "--porcelain"] for call in calls))
        self.assertFalse(any("merge-base" in call for call in calls))

    def test_updater_is_executable(self):
        self.assertTrue(UPDATER.stat().st_mode & stat.S_IXUSR)


if __name__ == "__main__":
    unittest.main()
