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
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts/ci"))
from build_config import matrices, platforms, toolchain


def load_script(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts/ci" / name)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


_audit_spec = importlib.util.spec_from_file_location("redundancy_audit", ROOT / "scripts/audit-redundancy.py")
redundancy_audit = importlib.util.module_from_spec(_audit_spec)
_audit_spec.loader.exec_module(redundancy_audit)


class RedundancyAuditTests(unittest.TestCase):
    def test_subprocess_encoding_is_utf8_on_non_utf8_windows_locale(self):
        # Windows runners otherwise decode child UTF-8 using the legacy code page.
        completed = subprocess.CompletedProcess(["example"], 0, "中文检测通过", "")
        with patch.object(redundancy_audit.subprocess, "run", return_value=completed) as run:
            self.assertEqual(redundancy_audit.command(["example"]), "中文检测通过")
        self.assertEqual(run.call_args.kwargs["encoding"], "utf-8")
        self.assertEqual(run.call_args.kwargs["errors"], "strict")
        self.assertTrue(run.call_args.kwargs["text"])

    def findings(self, root, files, facts, texts, tests=()):
        rules = json.loads((ROOT / "scripts/audit/redundancy-rules.json").read_text())["rules"]
        return redundancy_audit.evaluate(root, files, facts, texts, set(tests), rules)

    def test_all_modules_are_scanned_and_candidates_are_not_deletion_proofs(self):
        facts = dict(registered=["src/lib.rs"], missing_modules=[], references=[
            dict(path="tests/b.rs", name="test_helper", test_only=True),
            dict(path="src/main.rs", name="current", test_only=False),
        ], functions=[dict(path=path, name=name, owner="", public=True, test_only=False,
                           trait_impl=False, ignored_parameters=ignored, forwards_to=forward)
                      for path, name, ignored, forward in [
                          ("src/a.rs", "unused_a", [], None),
                          ("src/b.rs", "unused_b", ["_old_flag"], "current"),
                          ("src/c.rs", "test_helper", [], None),
                          ("src/current.rs", "current", [], None),
                      ]])
        found = self.findings(ROOT, ["src/lib.rs", "src/a.rs", "src/b.rs", "src/orphan.rs"], facts, {})
        unused = {f['subject'] for f in found if f['rule'] == 'unreferenced_api'}
        self.assertEqual(unused, {'unused_a', 'unused_b'})
        self.assertTrue(any(f['rule'] == 'test_only_api' and f['subject'] == 'test_helper' for f in found))
        self.assertTrue(any(f['rule'] == 'ignored_parameter' for f in found))
        self.assertTrue(any(f['rule'] == 'forwarding_api' for f in found))
        self.assertTrue(any(f['rule'] == 'unregistered_rust' and f['path'] == 'src/orphan.rs' for f in found))
        self.assertTrue(all(f['certainty'] == 'candidate' for f in found))

    def test_document_links_test_targets_duplicates_and_script_consumers(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); (root / 'docs').mkdir(); (root / 'docs/exists file.md').touch()
            document = '[ok](<exists%20file.md>) [bad](missing.md) [web](https://example.org) [anchor](#title)\n'
            document += '```sh\ncargo test --test retired_suite\n```\n' + 'text ' * 50
            texts = {'docs/a.md': document, 'docs/b.md': document,
                     'scripts/manual.py': 'print(1)', 'scripts/active.py': 'print(2)',
                     'README.md': 'python3 scripts/active.py'}
            found = self.findings(root, list(texts), dict(functions=[], references=[], registered=[], missing_modules=[]), texts, ['repository_suite'])
            broken = [f for f in found if f['rule'] == 'broken_document_link']
            self.assertEqual({f['subject'] for f in broken}, {'missing.md'})
            self.assertEqual(len(broken), 2)
            self.assertTrue(any(f['rule'] == 'obsolete_test_target' for f in found))
            self.assertEqual(len([f for f in found if f['rule'] == 'duplicate_document']), 1)
            self.assertEqual({f['path'] for f in found if f['rule'] == 'unreferenced_script'}, {'scripts/manual.py'})

    def test_test_only_module_excludes_live_types_and_trait_dispatch(self):
        functions = [dict(path=path, name=name, owner='', public=True, test_only=False,
                          trait_impl=trait, ignored_parameters=[], forwards_to=None)
                     for path, name, trait in [('src/old.rs', 'old', False), ('src/live.rs', 'live', False), ('src/trait.rs', 'callback', True), ('src/barrel.rs', 'facade', False), ('src/inline_old.rs', 'inline_old', False)]]
        references = [dict(path='tests/test.rs', name=name, test_only=True) for name in ['old', 'live', 'callback', 'facade', 'OldType']]
        references += [dict(path='src/main.rs', name='LiveType', test_only=False), dict(path='src/inline_old.rs', name='inline_old', test_only=True)]
        facts = dict(functions=functions, references=references, registered=[], missing_modules=[],
                     declared_symbols=[dict(path='src/live.rs', name='LiveType', test_only=False, public=True), dict(path='src/type_only.rs', name='OldType', test_only=False, public=True)], reexport_files=['src/barrel.rs'])
        found = self.findings(ROOT, [], facts, {})
        self.assertEqual({f['path'] for f in found if f['rule'] == 'test_only_module'}, {'src/old.rs', 'src/type_only.rs', 'src/inline_old.rs'})

    def test_declaration_consumers_scan_all_public_types_and_constants(self):
        declarations = [dict(path=path, name=name, public=public, test_only=test)
                        for path, name, public, test in [
                            ('src/a.rs', 'OldType', True, False),
                            ('src/b.rs', 'OldConstant', True, False),
                            ('src/c.rs', 'TestOnlyType', True, False),
                            ('src/d.rs', 'LiveType', True, False),
                            ('src/private.rs', 'Private', False, False),
                            ('src/test.rs', 'TestFixture', True, True)]]
        facts = dict(functions=[], registered=[], missing_modules=[], declared_symbols=declarations,
                     references=[dict(path='tests/a.rs', name='TestOnlyType', test_only=True),
                                 dict(path='src/main.rs', name='LiveType', test_only=False)])
        findings = self.findings(ROOT, [], facts, {})
        matches = [f for f in findings if f['rule'] == 'declaration_consumers']
        self.assertEqual({f['subject'] for f in matches}, {'OldType', 'OldConstant', 'TestOnlyType'})
        self.assertTrue(all(f['certainty'] == 'candidate' for f in matches))

    def test_new_unknown_rule_fails_instead_of_silently_skipping(self):
        with self.assertRaisesRegex(ValueError, 'Unimplemented'):
            redundancy_audit.evaluate(ROOT, [], dict(functions=[], references=[], registered=[], missing_modules=[]), {}, set(), [{'id': 'new', 'kind': 'new'}])

    def test_removed_test_convenience_apis_cannot_recur_in_production(self):
        names = ["plan_format_targets", "begin_write_wizard", "scan_disks", "format_partition_on_disk"]
        facts = dict(functions=[], registered=[], missing_modules=[], references=[],
                     declared_symbols=[dict(path="src/example.rs", name=name, test_only=False) for name in names])
        found = self.findings(ROOT, [], facts, {})
        self.assertEqual({f['subject'] for f in found if f['rule'] == 'retired_symbol'}, set(names))
        self.assertTrue(all(f['certainty'] == 'confirmed' for f in found))

    def test_retired_symbols_and_missing_modules_are_confirmed(self):
        facts = dict(functions=[], registered=[], missing_modules=['src/missing.rs'],
                     declared_symbols=[dict(path='src/c.rs', name='ProvisionValidator', test_only=False)],
                     references=[dict(path='src/b.rs', name='generate_image', test_only=False)])
        found = self.findings(ROOT, [], facts, {})
        self.assertEqual({f['rule'] for f in found}, {'missing_module', 'retired_symbol'})
        self.assertTrue(any(f['subject'] == 'ProvisionValidator' for f in found))
        self.assertTrue(all(f['certainty'] == 'confirmed' for f in found))


class PythonToolingTests(unittest.TestCase):
    def test_unsupported_interpreter_fails_before_tooling_runs(self):
        from python_runtime import require_python
        for version in [(3, 9), (3, 10)]:
            with self.assertRaisesRegex(SystemExit, r"require Python 3.11\+"):
                require_python(version)
        for version in [(3, 11), (3, 14)]:
            require_python(version)

    def test_ci_runs_whole_repository_audit_for_every_change(self):
        ci = (ROOT / ".github/workflows/ci.yml").read_text()
        job = ci.split("  repository-audit:\n", 1)[1].split("  protocol-audit:\n", 1)[0]
        self.assertNotIn("if: needs.changes", job)
        self.assertIn("uv run --locked python scripts/audit-redundancy.py --check", job)
        self.assertIn("if: always()", job)
        self.assertIn("path: target/redundancy-audit/", job)

    def test_uv_project_contract_is_pinned_and_portable(self):
        project = tomllib.loads((ROOT / "pyproject.toml").read_text(encoding="utf-8"))
        self.assertEqual(project["project"]["requires-python"], ">=3.11")
        self.assertEqual(project["tool"]["uv"]["python-preference"], "only-managed")
        self.assertFalse(project["tool"]["uv"]["package"])
        self.assertEqual(project["tool"]["uv"]["required-version"], ">=0.12.23,<0.13")
        self.assertEqual((ROOT / ".python-version").read_text(encoding="utf-8").strip(), "3.14.5")
        deps = {item.split(">=", 1)[0] for item in project["dependency-groups"]["protocol"]}
        self.assertEqual(deps, {"capstone", "pefile", "unicorn"})
        lock = (ROOT / "uv.lock").read_text(encoding="utf-8")
        self.assertIn('registry = "https://pypi.org/simple"', lock)
        self.assertNotIn("mirrors.aliyun.com", lock)

    def test_repository_entrypoints_use_locked_uv(self):
        checked = [
            ROOT / "Makefile",
            ROOT / "AGENTS.md",
            ROOT / "README.md",
            ROOT / "scripts/test-fast.sh",
            ROOT / "scripts/audit/README.md",
            ROOT / "audit/protocol/README.md",
            ROOT / "docs/development/PYTHON_TOOLING.md",
        ]
        checked += list((ROOT / ".github/workflows").glob("*.yml"))
        for path in checked:
            text = path.read_text(encoding="utf-8")
            self.assertNotIn("uv run --locked uv run", text, path)
            for line in text.splitlines():
                stripped = line.strip()
                if "scripts/" not in stripped:
                    continue
                self.assertNotRegex(stripped, r"(^|[|;&(]\s*)python3?\s+scripts/", path)

    def test_every_workflow_python_job_sets_up_uv(self):
        for path in (ROOT / ".github/workflows").glob("*.yml"):
            lines = path.read_text(encoding="utf-8").splitlines()
            starts = [i for i, line in enumerate(lines) if line.startswith("  ") and not line.startswith("    ") and line.endswith(":")]
            for index, start in enumerate(starts):
                end = starts[index + 1] if index + 1 < len(starts) else len(lines)
                block = "\n".join(lines[start:end])
                if "uv run --locked python" in block or "uv run --python " in block:
                    self.assertIn("uses: ./.github/actions/setup-python-tooling", block, f"{path}:{lines[start]}")
        action = (ROOT / ".github/actions/setup-python-tooling/action.yml").read_text(encoding="utf-8")
        self.assertIn("astral-sh/setup-uv@c771a70e6277c0a99b617c7a806ffedaca235ff9 # v9.0.0", action)
        ci = (ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        python_job = ci.split("  python-tooling:\n", 1)[1].split("  supply-chain:\n", 1)[0]
        self.assertNotIn("matrix.python", python_job)
        self.assertNotIn("strategy:", python_job)
        self.assertIn("uv run --locked python", python_job)



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
