"""Fail early for unsupported engineering-script interpreters."""
import sys


def require_python(version=None):
    version = sys.version_info if version is None else version
    if tuple(version[:2]) < (3, 11):
        raise SystemExit(
            "edpcli engineering scripts require Python 3.11+; "
            "run with uv run --locked python (detected %s.%s)." % tuple(version[:2])
        )
