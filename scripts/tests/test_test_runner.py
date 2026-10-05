#!/usr/bin/env python3
"""Behavioral regression tests for fast-gate selection; no Git mutations."""
import importlib.util
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

if __name__ == "__main__":
    unittest.main()
