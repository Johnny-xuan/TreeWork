#!/usr/bin/env python3
"""Create a clean local package source for TreeWork."""

from __future__ import annotations

import shutil
import subprocess
import tarfile
import tempfile
from pathlib import Path

from _paths import DIST_ROOT, PLUGIN_ROOT, REPOSITORY_ROOT

PLUGIN_PREFIX = PLUGIN_ROOT.relative_to(REPOSITORY_ROOT).as_posix()
REQUIRED = [
    ".codex-plugin",
    ".mcp.json",
    "Cargo.toml",
    "Cargo.lock",
    "LICENSE",
    "assets",
    "crates",
    "hooks",
    "mcp",
    "scripts",
    "skills",
]


def git_output(*args: str) -> str:
    result = subprocess.run(
        ["git", *args],
        cwd=REPOSITORY_ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(
            f"git {' '.join(args)} failed with {result.returncode}: "
            f"{result.stderr.strip()}"
        )
    return result.stdout.strip()


def extract_archive(archive_path: Path, destination: Path) -> None:
    prefix = Path(PLUGIN_PREFIX)
    with tarfile.open(archive_path, mode="r") as archive:
        for member in archive.getmembers():
            relative = Path(member.name)
            if (
                relative.is_absolute()
                or not relative.parts
                or any(part in {"", ".", ".."} for part in relative.parts)
                or member.issym()
                or member.islnk()
            ):
                raise SystemExit(f"git archive contains unsafe path: {member.name}")
            if member.isdir() and (
                relative == prefix or prefix.is_relative_to(relative)
            ):
                continue
            try:
                packaged = relative.relative_to(prefix)
            except ValueError as error:
                raise SystemExit(
                    f"git archive contains path outside {PLUGIN_PREFIX}: {member.name}"
                ) from error
            if not packaged.parts:
                continue
            target = destination / packaged
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                target.chmod(member.mode & 0o777)
                continue
            if not member.isfile():
                raise SystemExit(f"git archive contains unsupported entry: {member.name}")
            source = archive.extractfile(member)
            if source is None:
                raise SystemExit(f"cannot read git archive entry: {member.name}")
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open("wb") as output:
                shutil.copyfileobj(source, output)
            target.chmod(member.mode & 0o777)


def validate_clean_package(package_root: Path) -> None:
    forbidden_parts = {
        ".TreeWork",
        ".agents",
        ".git",
        "dist",
        "node_modules",
        "project-map-ui",
        "prototypes",
        "target",
        "__pycache__",
    }
    for path in package_root.rglob("*"):
        relative = path.relative_to(package_root)
        if path.is_symlink():
            raise SystemExit(f"clean package contains symlink: {relative}")
        if any(part in forbidden_parts for part in relative.parts):
            raise SystemExit(f"clean package contains development path: {relative}")
        if path.is_file() and (
            path.suffix in {".map", ".pyc", ".tmp"} or path.name == ".DS_Store"
        ):
            raise SystemExit(f"clean package contains generated artifact: {relative}")
    for retired in [
        package_root / "assets" / "vendor" / "graphology.umd.min.js",
        package_root / "assets" / "vendor" / "sigma.min.js",
        package_root / "assets" / "vendor" / "manifest.json",
    ]:
        if retired.exists():
            raise SystemExit(f"clean package contains retired renderer asset: {retired}")


def main() -> None:
    repository = Path(git_output("rev-parse", "--show-toplevel")).resolve()
    if repository != REPOSITORY_ROOT.resolve():
        raise SystemExit(
            "package_plugin.py must run from the TreeWork repository"
        )
    source_commit = git_output("rev-parse", "HEAD")
    DIST_ROOT.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(
        prefix=".treework-package-", dir=DIST_ROOT.parent
    ) as temporary:
        temporary_root = Path(temporary)
        archive_path = temporary_root / "source.tar"
        candidate = temporary_root / "treework"
        candidate.mkdir()
        result = subprocess.run(
            [
                "git",
                "archive",
                "--format=tar",
                f"--output={archive_path}",
                source_commit,
                "--",
                PLUGIN_PREFIX,
            ],
            cwd=REPOSITORY_ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if result.returncode != 0:
            raise SystemExit(
                f"git archive {source_commit} failed with {result.returncode}: "
                f"{result.stderr.strip()}"
            )
        extract_archive(archive_path, candidate)
        for name in REQUIRED:
            if not (candidate / name).exists():
                raise SystemExit(f"commit {source_commit} is missing package entry: {name}")
        validate_clean_package(candidate)
        if DIST_ROOT.exists():
            shutil.rmtree(DIST_ROOT)
        shutil.move(str(candidate), str(DIST_ROOT))
    print(
        f"Packaged TreeWork from commit {source_commit} at {DIST_ROOT}"
    )


if __name__ == "__main__":
    main()
