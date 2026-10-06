#!/usr/bin/env python3
"""Behavioral regressions for build facts, release admission and atomic install."""
from __future__ import annotations
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/ci"))
from build_config import matrices, platforms, toolchain


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts/ci" / name)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ReleaseTests(unittest.TestCase):
    def test_ci_identity_and_latest_run(self):
        verifier = load_script("verify-release-ci.py")
        good = dict(databaseId=1, headSha="a" * 40, headBranch="main", event="push", status="completed", conclusion="success")
        self.assertIsNone(verifier.select_run([{**good, "headSha": "b" * 40}, {**good, "headBranch": "feature"}, {**good, "event": "pull_request"}], "a" * 40))
        newest = {**good, "databaseId": 2, "conclusion": "cancelled"}
        self.assertEqual(verifier.select_run([good, newest], "a" * 40), newest)

    def test_ci_command_fails_closed(self):
        with tempfile.TemporaryDirectory() as directory:
            fake = Path(directory) / ("gh.cmd" if os.name == "nt" else "gh")
            if os.name == "nt":
                self.skipTest("POSIX CLI shim; selection contract is platform independent")
            fake.write_text('#!/bin/sh\nprintf "%s" "$FAKE_RUNS"\n', encoding="utf-8")
            fake.chmod(0o755)
            good = dict(databaseId=1, headSha="a" * 40, headBranch="main", event="push", status="completed", conclusion="success")
            cases = [([], False), ([good], True), ([{**good, "headSha": "b" * 40}], False), ([{**good, "status": "in_progress"}], False), ([{**good, "conclusion": "failure"}], False), ([{**good, "conclusion": "cancelled"}], False)]
            for runs, success in cases:
                result = subprocess.run([sys.executable, str(ROOT / "scripts/ci/verify-release-ci.py"), "--commit", "a" * 40, "--repo", "example/repo", "--timeout", "0"], env={**os.environ, "PATH": directory + os.pathsep + os.environ["PATH"], "FAKE_RUNS": json.dumps(runs)}, capture_output=True)
                self.assertEqual(result.returncode == 0, success, result.stderr)

    def test_platforms_have_complete_native_and_merge_coverage(self):
        facts = matrices()
        self.assertEqual(len(platforms()), 7)
        self.assertEqual(len(facts["primary"]["include"]), 3)
        self.assertEqual(len(facts["secondary"]["include"]), 3)
        for system in ("linux", "windows"):
            self.assertEqual({p["arch"] for p in facts[system]["include"]}, {"arm64", "x86_64"})

    def test_manifest_rejects_incomplete_or_unverified_assets(self):
        version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
        prefix = f"edpcli-v{version}-"
        commit = "a" * 40
        with tempfile.TemporaryDirectory() as directory:
            assets = Path(directory)
            archives = []
            for value in platforms().values():
                path = assets / f"{prefix}{value['label']}.{value['archive']}"
                path.write_bytes(b"test archive")
                (assets / (path.name + ".sha256")).write_text(hashlib.sha256(path.read_bytes()).hexdigest() + "  " + path.name + "\n", encoding="utf-8")
                archives.append(path)
            (assets / (prefix + "Cargo.lock")).write_bytes((ROOT / "Cargo.lock").read_bytes())
            (assets / (prefix + "rust-toolchain.txt")).write_text(f"rustc {toolchain()} (test)\n", encoding="utf-8")
            metadata = {"workspace_members": ["root"], "packages": [{"id": "root", "name": "edpcli", "version": version}]}
            (assets / (prefix + "cargo-metadata.json")).write_text(json.dumps(metadata), encoding="utf-8")
            sbom = {"metadata": {"component": {"name": "edpcli", "version": version}, "properties": [{"name": "git:commit", "value": commit}, {"name": "build:rust-toolchain", "value": toolchain()}]}}
            sbom_path = assets / (prefix + "sbom.cdx.json")
            sbom_path.write_text(json.dumps(sbom), encoding="utf-8")
            output = assets / (prefix + "release-manifest.json")
            command = [sys.executable, str(ROOT / "scripts/ci/generate-release-manifest.py"), "--assets-dir", directory, "--output", str(output), "--tag", "v" + version, "--commit", commit]
            def check(success):
                result = subprocess.run(command, capture_output=True)
                self.assertEqual(result.returncode == 0, success, result.stderr)
            check(True)
            first = output.read_bytes()
            check(True)
            self.assertEqual(output.read_bytes(), first)
            self.assertEqual(len(json.loads(first)["assets"]), 18)
            path = archives[0]
            path.unlink(); check(False); path.write_bytes(b"test archive")
            path.write_bytes(b"corrupt"); check(False); path.write_bytes(b"test archive")
            extra = assets / "unexpected"; extra.write_bytes(b"extra"); check(False); extra.unlink()
            if os.name == "posix":
                extra.symlink_to(output.name); check(False); extra.unlink()
            sbom["metadata"]["properties"][0]["value"] = "b" * 40
            sbom_path.write_text(json.dumps(sbom), encoding="utf-8"); check(False)
            sbom["metadata"]["properties"][0]["value"] = commit
            sbom_path.write_text(json.dumps(sbom), encoding="utf-8")
            command[command.index("--tag") + 1] = "v0.0.0"; check(False)


@unittest.skipUnless(os.name == "posix", "POSIX installer")
class InstallTests(unittest.TestCase):
    def test_success_and_failure_preserve_previous_install(self):
        for mode in ("success", "broken", "shadow", "rollback", "locked", "first-failure"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                home = Path(directory)
                target = home / ".local/bin/edpcli"; target.parent.mkdir(parents=True)
                original = b"#!/bin/sh\necho old\n"
                if mode != "first-failure":
                    target.write_bytes(original); target.chmod(0o755)
                source = home / "source"
                source.write_text("#!/bin/sh\n" + ("exit 1\n" if mode == "broken" else "echo new\n"), encoding="utf-8")
                source.chmod(0o755)
                shim_dir = home / "shims"; shim_dir.mkdir()
                shim = shim_dir / "zsh"
                shim.write_text('''#!/bin/sh
counter="$HOME/counter"
n=0
[ ! -f "$counter" ] || n=$(cat "$counter")
n=$((n+1)); printf '%s' "$n" > "$counter"
if [ "$MODE" = shadow ] || { [ "$MODE" = rollback ] && [ "$n" -gt 1 ]; } || { [ "$MODE" = first-failure ] && [ "$n" -gt 1 ]; }; then
  printf '/other/edpcli\\n'
else printf '%s/.local/bin/edpcli\\n' "$HOME"; fi
''', encoding="utf-8")
                shim.chmod(0o755)
                if mode == "locked": (target.parent / ".edpcli-install.lock").mkdir()
                result = subprocess.run(["sh", str(ROOT / "scripts/install-local.sh"), str(source)], env={**os.environ, "HOME": directory, "PATH": str(shim_dir) + os.pathsep + os.environ["PATH"], "MODE": mode}, capture_output=True)
                self.assertEqual(result.returncode == 0, mode == "success", result.stderr)
                if mode == "first-failure": self.assertFalse(target.exists())
                else: self.assertEqual(target.read_bytes(), source.read_bytes() if mode == "success" else original)
                self.assertFalse(list(target.parent.glob(".edpcli-candidate.*")))
                self.assertFalse(list(target.parent.glob(".edpcli-previous.*")))


class BuildTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.binary = Path(cls.temporary.name) / ("build.exe" if os.name == "nt" else "build")
        subprocess.run(["rustc", "--edition=" + tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["edition"], str(ROOT / "build.rs"), "-o", str(cls.binary)], check=True, capture_output=True, cwd=ROOT)

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def probe(self, directory):
        return subprocess.check_output([str(self.binary)], cwd=directory, env={**os.environ, "SOURCE_DATE_EPOCH": "1700000000"}, text=True)

    def test_clone_worktree_packed_refs_and_source_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory); repo = base / "repo"; repo.mkdir()
            def git(*args):
                return subprocess.check_output(["git", "-c", "user.name=test", "-c", "user.email=test@example.invalid", *args], cwd=repo, stderr=subprocess.DEVNULL, text=True).strip()
            git("init", "-b", "main")
            (repo / "source.rs").write_text("source", encoding="utf-8")
            (repo / "guide.md").write_text("guide", encoding="utf-8")
            (repo / " guide.md").write_text("leading-space filename", encoding="utf-8")
            git("add", "."); git("commit", "-m", "initial")
            clean = self.probe(repo)
            self.assertNotIn("+dirty", clean)
            self.assertIn("cargo:rerun-if-changed=guide.md", clean)
            self.assertIn("cargo:rerun-if-changed= guide.md", clean)
            self.assertIn("2023-11-14T22:13:20Z", clean)
            (repo / "guide.md").write_text("changed", encoding="utf-8")
            self.assertIn("+dirty", self.probe(repo))
            (repo / "guide.md").unlink()
            deleted = self.probe(repo)
            self.assertIn("+dirty", deleted)
            self.assertIn("cargo:rerun-if-changed=guide.md", deleted)
            (repo / "guide.md").write_text("guide", encoding="utf-8")
            self.assertNotIn("+dirty", self.probe(repo))
            git("reset", "--hard", "HEAD")
            git("pack-refs", "--all", "--prune")
            packed = self.probe(repo)
            self.assertIn("cargo:rerun-if-changed=" + git("rev-parse", "--git-path", "packed-refs"), packed)
            worktree = base / "worktree"
            git("worktree", "add", "--detach", str(worktree), "HEAD")
            watched = subprocess.check_output(["git", "rev-parse", "--git-path", "HEAD"], cwd=worktree, text=True).strip()
            self.assertIn("cargo:rerun-if-changed=" + watched, self.probe(worktree))
            archive = base / "archive"; archive.mkdir()
            self.assertIn("EDPCLI_BUILD_GIT=unknown", self.probe(archive))


if __name__ == "__main__":
    unittest.main()
