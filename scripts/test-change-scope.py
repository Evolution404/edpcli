#!/usr/bin/env python3
"""Regression checks for paths, including deleted/renamed paths without filesystem lookup."""
import unittest
from change_scope import classify

class ScopeTests(unittest.TestCase):
    def test_safety_inputs(self):
        for path in ("tests/fixtures/protocol/mode1/gold.bin", "backup/current.edpb"):
            self.assertTrue(classify([path])["rust"])
            self.assertTrue(classify([path])["protocol"])
        for path in ("build.rs", ".github/workflows/release.yml", "new/tool.py", "tool"):
            self.assertTrue(classify([path])["rust"])
        self.assertTrue(classify([".github/workflows/release.yml"])["deps"])

    def test_documentation_and_empty(self):
        self.assertFalse(classify([])["rust"])
        self.assertFalse(classify(["docs/user/usage.md"])["rust"])

    def test_rename_delete(self):
        self.assertTrue(classify(["tests/fixtures/protocol/old.bin", "docs/new.bin"])["protocol"])

if __name__ == "__main__":
    unittest.main()
