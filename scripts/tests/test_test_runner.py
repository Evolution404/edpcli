#!/usr/bin/env python3
"""Behavioral regression tests for fast-gate selection; no Git mutations."""
import importlib.util
import os
import tempfile
import time
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("edpcli_test_runner", ROOT / "scripts/test-full.py")
runner = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = runner
spec.loader.exec_module(runner)

class GateTests(unittest.TestCase):
    def test_untracked_changes_are_included_even_with_tracked_edits(self):
        def git(command):
            paths = "src/tui/backup_metadata.rs\0" if command[1] == "ls-files" else "README.md\0"
            return subprocess.CompletedProcess(command, 0, paths, "")
        with patch.object(runner, "run_text", side_effect=git):
            self.assertEqual(runner.changed_paths(), ["README.md", "src/tui/backup_metadata.rs"])
            self.assertIn("tui_suite", runner.suites_for_paths(runner.changed_paths()))

    def test_common_helpers_and_unknown_tests_run_every_suite(self):
        for path in ["tests/common/mod.rs", "tests/support/fixtures.rs", "tests/new_behavior.rs", "tests/tui_suite.rs"]:
            with self.subTest(path=path):
                self.assertEqual(runner.suites_for_paths([path]), set(runner.ALL_SUITES))

    def test_clean_worktree_uses_latest_commit(self):
        def git(command):
            paths = "tests/edpb.rs\0" if command[1] == "diff-tree" else ""
            return subprocess.CompletedProcess(command, 0, paths, "")
        with patch.object(runner, "run_text", side_effect=git):
            self.assertEqual(runner.changed_paths(), ["tests/edpb.rs"])

    def test_document_scope_matches_ci_and_local(self):
        import change_scope
        for path in ["docs/protocol/EDP_PROTOCOL_REVERSE_ENGINEERING.md", "docs/protocol/FIELD_GUIDE.md", "audit/protocol/gold.bin"]:
            with self.subTest(path=path):
                self.assertTrue(change_scope.classify([path])["protocol"])
                self.assertIn("protocol_suite", runner.suites_for_paths([path]))
        self.assertIn("tui_suite", runner.suites_for_paths(["docs/ui/TUI.md"]))
        self.assertIn("backup_suite", runner.suites_for_paths(["docs/backup/EDPB.md"]))
        self.assertFalse(change_scope.classify(["README.md"])["protocol"])

    def test_hil_never_enters_ordinary_suite_selection(self):
        self.assertFalse(runner.suites_for_paths(["src/new.rs"]) & runner.HIL_TARGETS)

class DeadlineTests(unittest.TestCase):
    def test_budget_contract_matches_documentation_and_ci(self):
        from test_gate import BUDGETS
        doc = (ROOT / "docs/architecture/ARCHITECTURE.md").read_text()
        for key, value in BUDGETS.items():
            self.assertIn(f"| `{key}` | {value} |", doc)
        ci = (ROOT / ".github/workflows/ci.yml").read_text()
        self.assertIn(f'EDPCLI_TEST_MAX_SECONDS: "{BUDGETS["ci_max_seconds"]}"', ci)

    def test_doctest_timeout_remains_typed(self):
        timeout = subprocess.CompletedProcess([], 124, "", "[TIMEOUT] phase=doctest")
        with patch.object(runner.subprocess, "run", return_value=timeout):
            result = runner.run_doctests({})
        self.assertTrue(result.timed_out)
        self.assertEqual(result.returncode, 124)

    def test_forced_deadline_stops_silent_compile_and_doctest_trees(self):
        from test_gate import run_watchdog
        for phase in ["compile", "doctest"]:
            with self.subTest(phase=phase), tempfile.TemporaryDirectory() as root:
                pid_file = Path(root) / "child.pid"
                child = "import time; time.sleep(60)"
                parent = "import os, pathlib, subprocess, sys, time; p=subprocess.Popen([sys.executable, '-c', " + repr(child) + "]); pathlib.Path(sys.argv[1]).write_text(str(p.pid)); os.close(1); time.sleep(60)"
                started = time.monotonic()
                deadline = 1 if os.name == "posix" else 3
                result = run_watchdog([sys.executable, "-c", parent, str(pid_file)], deadline, phase)
                self.assertEqual(result, 124)
                self.assertLess(time.monotonic() - started, deadline + 3)
                self.assertTrue(pid_file.exists(), "fixture must start before timeout")
                pid = int(pid_file.read_text())
                if os.name == "posix":
                    status = subprocess.run(["ps", "-p", str(pid), "-o", "stat="], capture_output=True, text=True).stdout.strip()
                    self.assertTrue(not status or status.startswith("Z"), status)
                else:
                    # taskkill /T is synchronous; verify the child no longer exists.
                    listing = subprocess.run(["tasklist", "/FI", f"PID eq {pid}", "/FO", "CSV", "/NH"], capture_output=True, text=True).stdout
                    self.assertNotIn(f'"{pid}"', listing)

if __name__ == "__main__":
    unittest.main()
