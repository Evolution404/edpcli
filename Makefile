.PHONY: install audit

install:
	@./scripts/install.sh

audit:
	@python3 scripts/audit-redundancy.py --check
