#!/usr/bin/env python3
"""Require successful main push CI for precisely the release commit."""
import argparse
import json
import subprocess
import time


def select_run(runs: list[dict], commit: str) -> dict | None:
    matching = [run for run in runs if run.get("headSha") == commit and run.get("event") == "push" and run.get("headBranch") == "main"]
    return max(matching, key=lambda run: run["databaseId"], default=None)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--commit", required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--timeout", type=int, default=900)
    args = parser.parse_args()
    deadline = time.monotonic() + args.timeout
    while True:
        raw = subprocess.check_output([
            "gh", "run", "list", "--repo", args.repo, "--workflow", "ci.yml",
            "--commit", args.commit, "--branch", "main", "--event", "push", "--limit", "100",
            "--json", "databaseId,headSha,headBranch,event,status,conclusion",
        ], text=True, timeout=60)
        run = select_run(json.loads(raw), args.commit)
        if run and run["status"] == "completed":
            if run["conclusion"] != "success":
                raise SystemExit(f"main CI {run['databaseId']} failed: {run['conclusion']}")
            print(f"same-commit main CI passed: {run['databaseId']}")
            return
        if time.monotonic() >= deadline:
            raise SystemExit("No successful completed same-commit main CI before timeout")
        time.sleep(min(15, max(0, deadline - time.monotonic())))


if __name__ == "__main__":
    main()
