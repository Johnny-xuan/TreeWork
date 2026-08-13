#!/usr/bin/env python3
"""Build both independently installable TreeWork release assets from HEAD."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import stat
import subprocess
import tarfile
import tempfile
import zipfile
from pathlib import Path

from _paths import REPOSITORY_ROOT


RELEASE_ROOT = REPOSITORY_ROOT / "dist" / "releases"
EDITIONS = {
    "Coding-Agents": {
        "sources": [
            (Path("plugins/treework"), Path("plugins/treework")),
        ],
        "files": [
            (
                Path(".agents/plugins/marketplace.json"),
                Path(".agents/plugins/marketplace.json"),
            ),
        ],
        "directory": "treework-coding-agents",
        "required": {
            ".agents/plugins/marketplace.json",
            "plugins/treework/.codex-plugin/plugin.json",
            "plugins/treework/.mcp.json",
            "plugins/treework/skills/treework/SKILL.md",
        },
    },
    "Manual": {
        "sources": [(Path("skills/treework-manual"), Path("."))],
        "files": [],
        "directory": "treework-manual",
        "required": {"SKILL.md"},
    },
}


def git_bytes(*args: str) -> bytes:
    result = subprocess.run(
        ["git", *args],
        cwd=REPOSITORY_ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        raise SystemExit(
            f"git {' '.join(args)} failed with {result.returncode}: "
            f"{result.stderr.decode(errors='replace').strip()}"
        )
    return result.stdout


def committed_file(commit: str, path: Path) -> bytes:
    return git_bytes("show", f"{commit}:{path.as_posix()}")


def toml_package_version(raw: bytes, name: str) -> str | None:
    package = raw.decode().split("[package]", 1)
    if len(package) != 2:
        return None
    match = re.search(r'^version\s*=\s*"([^"]+)"', package[1], re.MULTILINE)
    return match.group(1) if match else None


def lock_package_version(raw: bytes, package_name: str) -> str | None:
    pattern = re.compile(
        rf'\[\[package\]\]\s+name\s*=\s*"{re.escape(package_name)}"\s+'
        r'version\s*=\s*"([^"]+)"',
        re.MULTILINE,
    )
    matches = pattern.findall(raw.decode())
    return matches[0] if len(matches) == 1 else None


def source_version(commit: str) -> str:
    raw = committed_file(commit, Path("plugins/treework/.codex-plugin/plugin.json"))
    try:
        manifest = json.loads(raw)
        version = manifest["version"]
    except (json.JSONDecodeError, KeyError, TypeError) as error:
        raise SystemExit("committed plugin manifest has no valid version") from error
    if not isinstance(version, str) or not version:
        raise SystemExit("committed plugin version must be a non-empty string")

    versions = {
        "plugin manifest": version,
        "treework-cli": toml_package_version(
            committed_file(
                commit, Path("plugins/treework/crates/treework-cli/Cargo.toml")
            ),
            "treework-cli",
        ),
        "Cargo.lock treework-cli": lock_package_version(
            committed_file(commit, Path("plugins/treework/Cargo.lock")),
            "treework-cli",
        ),
        "Project Map": json.loads(
            committed_file(commit, Path("project-map-ui/package.json"))
        ).get("version"),
    }
    mismatched = {
        name: value for name, value in versions.items() if value != version
    }
    if mismatched:
        raise SystemExit(
            f"committed release versions do not match {version}: {mismatched}"
        )
    return version


def extract_commit_path(commit: str, source: Path, destination: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="treework-release-archive-") as temporary:
        archive_path = Path(temporary) / "source.tar"
        result = subprocess.run(
            [
                "git",
                "archive",
                "--format=tar",
                f"--output={archive_path}",
                commit,
                "--",
                source.as_posix(),
            ],
            cwd=REPOSITORY_ROOT,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if result.returncode != 0:
            raise SystemExit(
                f"cannot archive {source} from {commit}: "
                f"{result.stderr.decode(errors='replace').strip()}"
            )

        with tarfile.open(archive_path, mode="r") as archive:
            for member in archive.getmembers():
                archived = Path(member.name)
                if (
                    archived.is_absolute()
                    or not archived.parts
                    or any(part in {"", ".", ".."} for part in archived.parts)
                    or member.issym()
                    or member.islnk()
                ):
                    raise SystemExit(f"unsafe release path: {member.name}")
                if member.isdir() and (
                    archived == source or source.is_relative_to(archived)
                ):
                    continue
                try:
                    relative = archived.relative_to(source)
                except ValueError as error:
                    raise SystemExit(
                        f"release archive escaped {source}: {member.name}"
                    ) from error
                if not relative.parts:
                    continue
                target = destination / relative
                if member.isdir():
                    target.mkdir(parents=True, exist_ok=True)
                    target.chmod(member.mode & 0o777)
                    continue
                if not member.isfile():
                    raise SystemExit(f"unsupported release entry: {member.name}")
                stream = archive.extractfile(member)
                if stream is None:
                    raise SystemExit(f"cannot read release entry: {member.name}")
                target.parent.mkdir(parents=True, exist_ok=True)
                with target.open("wb") as output:
                    shutil.copyfileobj(stream, output)
                target.chmod(member.mode & 0o777)


def validate_edition(name: str, root: Path, required: set[str]) -> None:
    actual = {
        path.relative_to(root).as_posix()
        for path in root.rglob("*")
        if path.is_file()
    }
    missing = required - actual
    if missing:
        raise SystemExit(f"{name} release is missing {sorted(missing)}")
    if any(path.is_symlink() for path in root.rglob("*")):
        raise SystemExit(f"{name} release contains a symlink")
    if name == "Manual" and actual != {"SKILL.md"}:
        raise SystemExit(
            "Manual release must remain a self-contained single-file Skill; "
            f"found {sorted(actual)}"
        )


def build_candidate(
    commit: str,
    candidate: Path,
    sources: list[tuple[Path, Path]],
    files: list[tuple[Path, Path]],
) -> None:
    candidate.mkdir()
    for source, relative_destination in sources:
        destination = candidate / relative_destination
        destination.mkdir(parents=True, exist_ok=True)
        extract_commit_path(commit, source, destination)
    for source, relative_destination in files:
        destination = candidate / relative_destination
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(committed_file(commit, source))


def zip_info(name: str, mode: int) -> zipfile.ZipInfo:
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.create_system = 3
    info.external_attr = (stat.S_IFREG | mode) << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    return info


def write_reproducible_zip(
    source: Path, archive_path: Path, extras: dict[str, bytes] | None = None
) -> None:
    with zipfile.ZipFile(
        archive_path, mode="w", compression=zipfile.ZIP_DEFLATED, compresslevel=9
    ) as archive:
        for name, content in sorted((extras or {}).items()):
            archive.writestr(zip_info(name, 0o644), content, compresslevel=9)
        for path in sorted(source.rglob("*"), key=lambda item: item.as_posix()):
            if not path.is_file():
                continue
            relative = path.relative_to(source.parent).as_posix()
            mode = stat.S_IMODE(path.stat().st_mode)
            archive.writestr(
                zip_info(relative, mode), path.read_bytes(), compresslevel=9
            )


def verify_zip(
    archive_path: Path,
    directory: str,
    source: Path,
    extras: dict[str, bytes] | None = None,
) -> None:
    expected = {
        f"{directory}/{path.relative_to(source).as_posix()}": path.read_bytes()
        for path in source.rglob("*")
        if path.is_file()
    }
    expected.update(extras or {})
    with zipfile.ZipFile(archive_path) as archive:
        names = {name for name in archive.namelist() if not name.endswith("/")}
        if names != set(expected):
            raise SystemExit(
                f"{archive_path.name} file set differs: "
                f"missing={sorted(set(expected) - names)} "
                f"extra={sorted(names - set(expected))}"
            )
        for name, content in expected.items():
            if archive.read(name) != content:
                raise SystemExit(f"{archive_path.name} differs at {name}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--tag",
        help="Require the release tag to equal v<plugin version>.",
    )
    args = parser.parse_args()

    repository = Path(git_bytes("rev-parse", "--show-toplevel").decode().strip())
    if repository.resolve() != REPOSITORY_ROOT.resolve():
        raise SystemExit("run package_release_assets.py from the TreeWork repository")
    commit = git_bytes("rev-parse", "HEAD").decode().strip()
    version = source_version(commit)
    license_bytes = committed_file(commit, Path("LICENSE"))
    expected_tag = f"v{version}"
    if args.tag and args.tag != expected_tag:
        raise SystemExit(
            f"release tag {args.tag!r} does not match committed version {expected_tag!r}"
        )

    if RELEASE_ROOT.exists():
        shutil.rmtree(RELEASE_ROOT)
    RELEASE_ROOT.mkdir(parents=True)

    with tempfile.TemporaryDirectory(
        prefix=".treework-release-", dir=RELEASE_ROOT.parent
    ) as temporary:
        temporary_root = Path(temporary)
        built: list[Path] = []
        for name, config in EDITIONS.items():
            directory = str(config["directory"])
            candidate = temporary_root / directory
            build_candidate(
                commit,
                candidate,
                config["sources"],
                config["files"],
            )
            validate_edition(name, candidate, config["required"])
            archive_path = RELEASE_ROOT / f"TreeWork-{name}-v{version}.zip"
            extras = {"LICENSE": license_bytes}
            write_reproducible_zip(candidate, archive_path, extras)
            verify_zip(archive_path, directory, candidate, extras)
            built.append(archive_path)

    for archive_path in built:
        digest = hashlib.sha256(archive_path.read_bytes()).hexdigest()
        print(f"{archive_path.name}  sha256:{digest}")
    print(f"Packaged both TreeWork editions from commit {commit}")


if __name__ == "__main__":
    main()
