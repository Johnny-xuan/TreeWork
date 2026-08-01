#!/usr/bin/env python3
"""Prove the clean package is derived only from tracked HEAD content."""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

from _paths import DIST_ROOT, PLUGIN_ROOT, REPOSITORY_ROOT
from package_plugin import PLUGIN_PREFIX

MARKER = PLUGIN_ROOT / "scripts" / ".treework-untracked-package-marker"
MARKER_TEXT = "untracked working-tree content must not ship\n"


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def git_text(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["git", *args],
        cwd=REPOSITORY_ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def main() -> None:
    if MARKER.exists():
        fail(f"refusing to overwrite existing marker: {MARKER.name}")
    tracked = git_text(
        "ls-files", "--error-unmatch", str(MARKER.relative_to(REPOSITORY_ROOT))
    )
    if tracked.returncode == 0:
        fail("package exclusion marker unexpectedly exists in HEAD")
    source_commit_result = git_text("rev-parse", "HEAD")
    if source_commit_result.returncode != 0:
        fail(f"cannot resolve source commit: {source_commit_result.stderr.strip()}")
    source_commit = source_commit_result.stdout.strip()

    try:
        MARKER.write_text(MARKER_TEXT, encoding="utf-8")
        result = subprocess.run(
            [sys.executable, str(REPOSITORY_ROOT / "scripts" / "package_plugin.py")],
            cwd=REPOSITORY_ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if result.returncode != 0:
            fail(
                "commit-derived packaging failed\n"
                f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
            )
        if (DIST_ROOT / MARKER.relative_to(PLUGIN_ROOT)).exists():
            fail("untracked allowlisted marker entered the clean package")

        expected_result = git_text(
            "ls-tree",
            "-r",
            "--name-only",
            source_commit,
            "--",
            PLUGIN_PREFIX,
        )
        if expected_result.returncode != 0:
            fail(f"cannot list commit package files: {expected_result.stderr.strip()}")
        prefix = f"{PLUGIN_PREFIX}/"
        expected = {
            path[len(prefix) :]
            for path in expected_result.stdout.splitlines()
            if path.startswith(prefix)
        }
        actual = {
            path.relative_to(DIST_ROOT).as_posix()
            for path in DIST_ROOT.rglob("*")
            if path.is_file()
        }
        if actual != expected:
            fail(
                "package file set differs from allowlisted commit files: "
                f"missing={sorted(expected - actual)} extra={sorted(actual - expected)}"
            )

        for relative in sorted(expected):
            committed = subprocess.run(
                ["git", "show", f"{source_commit}:{PLUGIN_PREFIX}/{relative}"],
                cwd=REPOSITORY_ROOT,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            if committed.returncode != 0:
                fail(
                    f"cannot read {relative} from {source_commit}: "
                    f"{committed.stderr.decode(errors='replace').strip()}"
                )
            if (DIST_ROOT / relative).read_bytes() != committed.stdout:
                fail(f"packaged bytes differ from {source_commit}: {relative}")
        print(
            f"ok: clean package exactly matches {len(actual)} allowlisted files "
            f"from commit {source_commit}; untracked marker excluded"
        )
    finally:
        MARKER.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
