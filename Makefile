.PHONY: install audit

install:
	@./scripts/install.sh

audit:
	@uv run --locked python scripts/audit-redundancy.py --check
