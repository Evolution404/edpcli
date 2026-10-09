.PHONY: install audit version

install:
	@./scripts/install.sh

audit:
	@uv run --locked python scripts/audit-redundancy.py --check

version:
	@cargo metadata --no-deps --locked --format-version 1 | python3 -c 'import json,sys;data=json.load(sys.stdin);print("源码版本:", next(p["version"] for p in data["packages"] if p["name"]=="edpcli"))'
	@git status --short --branch
	@git log -1 --oneline
