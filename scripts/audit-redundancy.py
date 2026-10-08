#!/usr/bin/env python3
"""Read-only whole-repository audit; review candidates before deletion.

One command executes every registered rule against every tracked/non-ignored file.
Rust evidence comes from syn ASTs, including all cfg branches, not text grep.
"""
from __future__ import annotations
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent))
from python_runtime import require_python
require_python()
import argparse
from collections import Counter, defaultdict
import hashlib
import json
import re
import subprocess
from urllib.parse import unquote, urlsplit

ROOT = Path(__file__).resolve().parents[1]
RULES = ROOT / "scripts/audit/redundancy-rules.json"


def command(args, *, root=ROOT, input_text=None):
    result = subprocess.run(
        args, cwd=root, input=input_text, capture_output=True, text=True,
        encoding="utf-8", errors="strict", timeout=600,
    )
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}): {' '.join(args)}\n{result.stderr}")
    return result.stdout


def repository_files(root):
    result = command(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"], root=root)
    return sorted({name for name in result.split('\0') if name and (root / name).is_file()})


def cargo_targets(root):
    metadata = json.loads(command(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], root=root))
    targets = [target for package in metadata['packages'] if package['id'] in metadata['workspace_members'] for target in package['targets']]
    roots = [Path(t['src_path']).relative_to(root).as_posix() for t in targets]
    tests = {t['name'] for t in targets if 'test' in t['kind']}
    return sorted(roots), tests


def rust_facts(root, files, roots):
    payload = json.dumps({'files': [p for p in files if p.endswith('.rs')], 'roots': roots})
    return json.loads(command(['cargo', 'run', '--locked', '--quiet', '--example', 'audit-redundancy'], root=root, input_text=payload))


def text_files(root, files):
    texts = {}
    for name in files:
        try:
            data = (root / name).read_bytes()
            if b'\0' not in data:
                texts[name] = data.decode('utf-8')
        except UnicodeError:
            pass
    return texts


def evaluate(root, files, facts, texts, tests, rules):
    """All rules consume the same complete snapshot; no changed-file filtering."""
    findings = []
    refs = defaultdict(list)
    for ref in facts['references']:
        refs[ref['name']].append(ref)
    declarations = Counter(f['name'] for f in facts['functions'])

    def add(rule, path, subject, evidence, certainty='candidate'):
        findings.append(dict(rule=rule['id'], path=path, subject=subject, evidence=evidence, certainty=certainty))

    for rule in rules:
        kind = rule['kind']
        if kind == 'unregistered_rust':
            for path in files:
                if path.endswith('.rs') and path not in facts['registered']:
                    add(rule, path, 'Rust source', 'Absent from every Cargo root/module graph, with all cfg branches traversed.')
        elif kind == 'missing_module':
            for path in facts['missing_modules']:
                add(rule, path, 'mod declaration', 'Declared Rust module file does not exist.', 'confirmed')
        elif kind in {'unreferenced_api', 'test_only_api', 'ignored_parameter', 'forwarding_api'}:
            for fn in facts['functions']:
                if not fn['path'].startswith('src/') or fn['test_only'] or fn['trait_impl']:
                    continue
                subject = '::'.join(p for p in (fn['owner'], fn['name']) if p)
                uses = refs[fn['name']]
                production = [r for r in uses if r['path'].startswith('src/') and not r['test_only']]
                test_uses = [r for r in uses if r['test_only']]
                other = [r for r in uses if not r['path'].startswith('src/') and not r['test_only']]
                if kind == 'unreferenced_api' and fn['public'] and not uses:
                    add(rule, fn['path'], subject, 'No AST path, method-call or macro-token reference; imports/re-exports excluded.')
                elif kind == 'test_only_api' and fn['public'] and test_uses and not production and not other:
                    paths = sorted({r['path'] for r in test_uses})
                    add(rule, fn['path'], subject, 'Only test consumers: ' + ', '.join(paths))
                elif kind == 'ignored_parameter' and fn['ignored_parameters']:
                    add(rule, fn['path'], subject, 'Unread parameters: ' + ', '.join(fn['ignored_parameters']))
                elif kind == 'forwarding_api' and fn['public'] and fn['forwards_to']:
                    add(rule, fn['path'], subject, 'Single-call body forwards to ' + fn['forwards_to'])
                # Same-name references conservatively retain methods; never infer symbol resolution.
                if declarations[fn['name']] > 1:
                    for finding in findings[-1:]:
                        if finding['rule'] == rule['id'] and finding['path'] == fn['path'] and finding['subject'] == subject:
                            finding['evidence'] += '; overloaded/shared name: usage classification is conservative.'
        elif kind == 'declaration_consumers':
            for declaration in facts.get('declared_symbols', []):
                if not declaration['path'].startswith('src/') or declaration['test_only'] or not declaration.get('public', False):
                    continue
                uses = refs[declaration['name']]
                if not uses:
                    add(rule, declaration['path'], declaration['name'], 'No AST consumer of this public declaration; exports, derive/macro behavior or external contracts require review.')
                elif all(r['test_only'] for r in uses):
                    add(rule, declaration['path'], declaration['name'], 'Only test consumers of declaration: ' + ', '.join(sorted({r['path'] for r in uses})))
        elif kind == 'test_only_module':
            modules = defaultdict(list)
            for fn in facts['functions']:
                if fn['path'].startswith('src/') and not fn['test_only']:
                    modules[fn['path']].append(fn)
            declarations_by_path = defaultdict(list)
            for declaration in facts.get('declared_symbols', []):
                if declaration['path'].startswith('src/') and not declaration['test_only']:
                    declarations_by_path[declaration['path']].append(declaration)
            for path in modules.keys() | declarations_by_path.keys():
                functions = modules[path]
                declarations_in_module = declarations_by_path[path]
                if path in facts.get('reexport_files', []):
                    continue
                if not any(d.get('public', False) for d in functions + declarations_in_module) or any(fn['trait_impl'] for fn in functions):
                    continue
                names = {d['name'] for d in functions + declarations_in_module}
                consumers = [r for name in names for r in refs[name] if r['path'] != path or r['test_only']]
                if any(not r['test_only'] for r in consumers) or not any(r['test_only'] for r in consumers):
                    continue
                add(rule, path, 'test-only consumers', 'No external production/example/tool consumer of module declarations; external or inline test consumers: ' + ', '.join(sorted({r['path'] for r in consumers})))
        elif kind == 'unreferenced_script':
            for path in files:
                if not path.startswith('scripts/') or '/tests/' in path or not path.endswith(('.py', '.sh', '.ps1')):
                    continue
                name = Path(path).name
                if not any(name in text for other, text in texts.items() if other != path):
                    add(rule, path, name, 'No filename reference in other repository text; may be a manual entrypoint.')
        elif kind == 'broken_document_link':
            for path, text in texts.items():
                if not path.endswith('.md'):
                    continue
                text = re.sub(r'```.*?```|~~~.*?~~~', '', text, flags=re.S)
                targets = re.findall(r'\[[^\]\n]*\]\(\s*(<[^>]+>|[^\s)]+)', text)
                targets += re.findall(r'^\s*\[[^\]]+\]:\s*(<[^>]+>|[^\s]+)', text, flags=re.M)
                for target in sorted(set(targets)):
                    target = target.strip('<>')
                    parsed = urlsplit(target)
                    if parsed.scheme or parsed.netloc or not parsed.path:
                        continue
                    resolved = (root / path).parent / unquote(parsed.path)
                    if not resolved.exists():
                        add(rule, path, target, 'Local Markdown target does not exist.', 'confirmed')
        elif kind == 'obsolete_test_target':
            for path, text in texts.items():
                if path.endswith('.md'):
                    for target in sorted(set(re.findall(r'\bcargo\s+test[^\n`]*?--test\s+([\w-]+)', text))):
                        if target not in tests:
                            add(rule, path, target, 'No current Cargo test target with this name.', 'confirmed')
        elif kind == 'duplicate_document':
            hashes = defaultdict(list)
            for path, text in texts.items():
                if path.endswith('.md') and len(text) >= 200:
                    hashes[hashlib.sha256(text.encode()).hexdigest()].append(path)
            for paths in hashes.values():
                if len(paths) > 1:
                    add(rule, paths[0], 'identical document', ', '.join(paths), 'candidate')
        elif kind == 'duplicate_native_file_identity':
            symbol = rule['symbol']
            # Direct native API invocations only, not imports, documentation or
            # references to the canonical platform adapter. This is a REVIEW
            # candidate, not a reason to delete working Windows code.
            calling_modules = [path for path, source in texts.items()
                if path.startswith('src/') and path.endswith('.rs')
                and re.search(r'\b'+re.escape(symbol)+r'\s*\(', source)]
            if len(calling_modules) > 1:
                for path in sorted(calling_modules):
                    add(rule, path, symbol,
                        'Direct Win32 file-identity reader also found in: '
                        + ', '.join(other for other in sorted(calling_modules) if other != path))
        elif kind == 'retired_symbol':
            retired = set(rule['symbols'])
            for declaration in facts['functions'] + facts.get('declared_symbols', []):
                if declaration['path'].startswith('src/') and declaration['name'] in retired:
                    add(rule, declaration['path'], declaration['name'], 'Declaration matches the confirmed retirement ledger.', 'confirmed')
            for ref in facts['references']:
                if ref['name'] in retired and ref['path'].startswith(('src/', 'tests/', 'examples/')):
                    add(rule, ref['path'], ref['name'], 'AST use matches the confirmed retirement ledger.', 'confirmed')
        else:
            raise ValueError(f"Unimplemented redundancy rule: {kind}")
    unique = {json.dumps(f, sort_keys=True): f for f in findings}
    return sorted(unique.values(), key=lambda f: (f['rule'], f['path'], f['subject']))


def markdown(report):
    lines = ['# 全仓冗余扫描', '', f"扫描 {report['file_count']} 个文件、{report['rust_file_count']} 份 Rust 源码，执行 {len(report['rules'])} 条规则。", '',
             '候选需要核对产品职责、公开安全能力、协议兼容和手动工具用途。工具只写审计报告，不删除源码。', '',
             'AST 包含全部 cfg 分支；宏仅统计标识符、不展开。按名称保守统计引用，不能证明跨模块同名符号、别名或运行时注册的完整调用关系。', '']
    for rule in report['rules']:
        findings = [f for f in report['findings'] if f['rule'] == rule['id']]
        lines += [f"## {rule['id']}：{rule['description']}（{len(findings)}）", '']
        for finding in findings:
            lines += [f"- **{finding['certainty']}** `{finding['path']}` — `{finding['subject']}`：{finding['evidence']}"]
        if not findings:
            lines += ['未发现匹配。']
        lines += ['']
    return '\n'.join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'target/redundancy-audit')
    parser.add_argument('--check', action='store_true', help='Return 1 for confirmed findings; review candidates do not fail.')
    args = parser.parse_args()
    files = repository_files(ROOT)
    roots, tests = cargo_targets(ROOT)
    print(f'[audit] 全仓 {len(files)} 个文件；读取所有 Rust AST 与 Cargo 模块注册', flush=True)
    facts = rust_facts(ROOT, files, roots)
    rules = json.loads(RULES.read_text(encoding="utf-8"))
    if rules['schema_version'] != 1 or len({r['id'] for r in rules['rules']}) != len(rules['rules']):
        raise ValueError('Invalid rule schema or duplicate rule id')
    report = dict(schema_version=1, file_count=len(files), rust_file_count=sum(p.endswith('.rs') for p in files),
                  rules=rules['rules'], findings=evaluate(ROOT, files, facts, text_files(ROOT, files), tests, rules['rules']))
    args.output.mkdir(parents=True, exist_ok=True)
    (args.output / 'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding="utf-8")
    (args.output / 'report.md').write_text(markdown(report) + '\n', encoding="utf-8")
    counts = Counter(f['rule'] for f in report['findings'])
    for rule in report['rules']:
        print(f"[audit] {rule['id']}: {counts[rule['id']]}")
    confirmed = sum(f['certainty'] == 'confirmed' for f in report['findings'])
    print(f"[audit] 确认问题 {confirmed}；待核查候选 {len(report['findings']) - confirmed}")
    print(f"[audit] 报告: {args.output / 'report.md'}")
    return 1 if args.check and confirmed else 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (RuntimeError, ValueError, subprocess.TimeoutExpired) as error:
        print(f'[audit] {error}', file=sys.stderr)
        sys.exit(2)
