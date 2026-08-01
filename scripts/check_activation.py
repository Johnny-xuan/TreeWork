#!/usr/bin/env python3
"""Verify fresh-context visibility for the installed TreeWork plugin."""

from __future__ import annotations

import hashlib
import json
import os
import re
import signal
import stat
import subprocess
import tempfile
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

from _paths import REPOSITORY_ROOT

CODEX_HOME = Path(os.environ.get("CODEX_HOME", Path.home() / ".codex")).expanduser()
PLUGIN_NAME = "treework"
MARKETPLACE_NAME = "treework"
PLUGIN_SELECTOR = f"{PLUGIN_NAME}@{MARKETPLACE_NAME}"
COMMAND_TIMEOUT_SECONDS = 120.0
TERMINATION_GRACE_SECONDS = 2.0


class ActivationError(ValueError):
    pass


@dataclass(frozen=True)
class Candidate:
    version: str
    source_root: Path
    cache_root: Path


Runner = Callable[[list[str]], str]


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def ok(message: str) -> None:
    print(f"ok: {message}")


def terminate_process_group(
    process: subprocess.Popen[str],
    *,
    grace_seconds: float,
) -> str:
    def communicate_finished() -> bool:
        try:
            process.communicate(timeout=grace_seconds)
            return True
        except subprocess.TimeoutExpired:
            return False

    if os.name != "posix":
        if process.poll() is None:
            process.terminate()
        if communicate_finished():
            return "leader process; descendant cleanup is unavailable on this platform"
        if process.poll() is None:
            process.kill()
        if communicate_finished():
            return "leader process; descendant cleanup is unavailable on this platform"
        return "leader cleanup did not converge; descendant cleanup is unavailable on this platform"

    process_group = process.pid

    def signal_group(sig: signal.Signals) -> None:
        try:
            os.killpg(process_group, sig)
        except ProcessLookupError:
            pass

    def group_is_gone() -> bool:
        try:
            os.killpg(process_group, 0)
        except ProcessLookupError:
            return True
        return False

    def wait_for_group() -> bool:
        deadline = time.monotonic() + grace_seconds
        while time.monotonic() < deadline:
            if group_is_gone():
                return True
            time.sleep(min(0.02, grace_seconds))
        return group_is_gone()

    signal_group(signal.SIGTERM)
    pipes_closed = communicate_finished()
    group_gone = wait_for_group()
    if pipes_closed and group_gone:
        return "process group"

    signal_group(signal.SIGKILL)
    pipes_closed = communicate_finished()
    group_gone = wait_for_group()
    if pipes_closed and group_gone:
        return "process group"
    return "process-group cleanup did not converge"


def run(
    args: list[str],
    *,
    cwd: Path,
    timeout_seconds: float = COMMAND_TIMEOUT_SECONDS,
    termination_grace_seconds: float = TERMINATION_GRACE_SECONDS,
) -> str:
    try:
        process = subprocess.Popen(
            args,
            cwd=cwd,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
    except OSError as error:
        raise ActivationError(f"cannot start {' '.join(args)}: {error}") from error
    try:
        stdout, stderr = process.communicate(timeout=timeout_seconds)
    except subprocess.TimeoutExpired as error:
        cleanup = terminate_process_group(
            process,
            grace_seconds=termination_grace_seconds,
        )
        raise ActivationError(
            f"{' '.join(args)} timed out after {timeout_seconds:g} seconds; "
            f"terminated {cleanup}"
        ) from error
    if process.returncode != 0:
        raise ActivationError(
            f"{' '.join(args)} failed: {stderr.strip() or stdout.strip()}"
        )
    return stdout


def read_json(path: Path, label: str) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ActivationError(f"cannot read {label} at {path}: {error}") from error


def canonical_existing_path(raw: Any, label: str) -> Path:
    if not isinstance(raw, str) or not raw:
        raise ActivationError(f"{label} must be a non-empty path")
    try:
        return Path(raw).expanduser().resolve(strict=True)
    except OSError as error:
        raise ActivationError(f"{label} does not resolve: {raw}: {error}") from error


def validate_local_configuration(dev_root: Path) -> None:
    marketplace_path = dev_root / ".agents" / "plugins" / "marketplace.json"
    if not marketplace_path.is_file():
        raise ActivationError("missing treework marketplace file")
    marketplace = read_json(marketplace_path, "treework marketplace")
    if marketplace.get("name") != MARKETPLACE_NAME:
        raise ActivationError("marketplace name is not treework")
    entries = marketplace.get("plugins")
    if not isinstance(entries, list):
        raise ActivationError("marketplace plugins must be an array")
    entry = next((item for item in entries if item.get("name") == PLUGIN_NAME), None)
    if entry is None:
        raise ActivationError("treework entry missing from marketplace")
    source_path = entry.get("source", {}).get("path")
    if source_path != "./plugins/treework":
        raise ActivationError(
            "treework marketplace source must point at the tracked plugin"
        )


def validate_candidate(
    plugin_list: str,
    dev_root: Path,
    source_root: Path,
    code_home: Path,
) -> Candidate:
    try:
        payload = json.loads(plugin_list)
    except json.JSONDecodeError as exc:
        raise ActivationError(f"plugin list did not return valid JSON: {exc}") from exc
    if not isinstance(payload, dict):
        raise ActivationError("plugin list payload must be a JSON object")
    entries: list[dict[str, Any]] = []
    for group in ["installed", "available"]:
        values = payload.get(group)
        if not isinstance(values, list):
            raise ActivationError(f"plugin list `{group}` must be an array")
        entries.extend(item for item in values if isinstance(item, dict))
    targets = [item for item in entries if item.get("pluginId") == PLUGIN_SELECTOR]
    if len(targets) != 1:
        raise ActivationError(
            f"plugin list must contain exactly one {PLUGIN_SELECTOR} entry; found {len(targets)}"
        )
    target = targets[0]
    if (
        target.get("name") != PLUGIN_NAME
        or target.get("marketplaceName") != MARKETPLACE_NAME
        or target.get("installed") is not True
        or target.get("enabled") is not True
    ):
        raise ActivationError("candidate plugin is not installed and enabled")

    manifest = read_json(
        source_root / ".codex-plugin" / "plugin.json", "plugin source manifest"
    )
    version = manifest.get("version") if isinstance(manifest, dict) else None
    if not isinstance(version, str) or not version:
        raise ActivationError("plugin source manifest has no version")
    if target.get("version") != version:
        raise ActivationError(
            f"installed version {target.get('version')!r} does not match "
            f"plugin source version {version!r}"
        )

    marketplace_source = target.get("marketplaceSource")
    if marketplace_source is not None:
        if not isinstance(marketplace_source, dict):
            raise ActivationError("candidate marketplaceSource is malformed")
        if marketplace_source.get("sourceType") != "local":
            raise ActivationError("candidate marketplaceSource is not local")
        if canonical_existing_path(
            marketplace_source.get("source"), "candidate marketplaceSource"
        ) != dev_root.resolve(strict=True):
            raise ActivationError(
                "candidate marketplaceSource does not match this repository"
            )

    source = target.get("source")
    if not isinstance(source, dict) or source.get("source") != "local":
        raise ActivationError("candidate source is not local")
    if canonical_existing_path(
        source.get("path"), "candidate source.path"
    ) != source_root.resolve(strict=True):
        raise ActivationError("candidate source.path does not match this repository plugin")

    return Candidate(
        version=version,
        source_root=source_root.resolve(strict=True),
        cache_root=(
            code_home
            / "plugins"
            / "cache"
            / MARKETPLACE_NAME
            / PLUGIN_NAME
            / version
        ),
    )


def regular_file_manifest(root: Path) -> dict[str, str]:
    manifest: dict[str, str] = {}
    for path in sorted(root.rglob("*")):
        metadata = path.lstat()
        if stat.S_ISLNK(metadata.st_mode):
            raise ActivationError(f"candidate contains a symlink: {path.relative_to(root)}")
        if not stat.S_ISREG(metadata.st_mode):
            continue
        relative = path.relative_to(root).as_posix()
        digest = hashlib.sha256(relative.encode("utf-8") + b"\0" + path.read_bytes())
        manifest[relative] = digest.hexdigest()
    return manifest


def validate_cache_and_prompt(
    prompt: str,
    candidate: Candidate,
) -> Path:
    try:
        payload = json.loads(prompt)
    except json.JSONDecodeError as exc:
        raise ActivationError(f"prompt-input did not return valid JSON: {exc}") from exc
    if not isinstance(payload, list):
        raise ActivationError("prompt-input payload must be a JSON list")
    if "treework:treework" not in prompt:
        raise ActivationError("fresh prompt input does not expose the TreeWork skill")
    if "TreeWork" not in prompt:
        raise ActivationError("fresh prompt input does not expose the TreeWork plugin")

    expected_skill = (
        candidate.cache_root / "skills" / "treework" / "SKILL.md"
    )
    cache_base = candidate.cache_root.parents[1]
    expected_relative = expected_skill.relative_to(cache_base).as_posix()
    prompt_versions = set(
        re.findall(
            rf"{re.escape(PLUGIN_NAME)}/([^/\s`]+)/skills/{re.escape(PLUGIN_NAME)}/SKILL\.md",
            prompt,
        )
    )
    if prompt_versions != {candidate.version} or expected_relative not in prompt:
        raise ActivationError(
            "fresh prompt input does not point to the exact candidate cache version"
        )

    source_manifest = regular_file_manifest(candidate.source_root)
    if not source_manifest:
        raise ActivationError("candidate plugin source contains no regular files")
    for relative, expected_digest in source_manifest.items():
        cached = candidate.cache_root / relative
        try:
            metadata = cached.lstat()
        except OSError as error:
            raise ActivationError(f"candidate cache is missing {relative}: {error}") from error
        if not stat.S_ISREG(metadata.st_mode):
            raise ActivationError(f"candidate cache entry is not a regular file: {relative}")
        digest = hashlib.sha256(relative.encode("utf-8") + b"\0" + cached.read_bytes())
        if digest.hexdigest() != expected_digest:
            raise ActivationError(f"candidate cache bytes differ from source: {relative}")
    return expected_skill


def validate_hook_bundle(candidate: Candidate) -> None:
    hooks_path = candidate.cache_root / "hooks" / "hooks.json"
    if not hooks_path.is_file():
        raise ActivationError("installed plugin cache is missing hooks/hooks.json")
    hooks_text = hooks_path.read_text(encoding="utf-8")
    if "SessionStart" in hooks_text:
        raise ActivationError("installed hook bundle still contains the removed startup status hook")
    for event in ["PreToolUse", "Stop"]:
        if event not in hooks_text:
            raise ActivationError(f"installed hook bundle missing {event}")
    if "$PLUGIN_ROOT" not in hooks_text and "${PLUGIN_ROOT}" not in hooks_text:
        raise ActivationError("installed hook bundle does not use PLUGIN_ROOT")


def validate_cli_build_version(candidate: Candidate, runner: Runner) -> None:
    wrapper = (
        candidate.cache_root
        / "skills"
        / "treework"
        / "scripts"
        / "tw"
    )
    if not wrapper.is_file() or not os.access(wrapper, os.X_OK):
        raise ActivationError("installed plugin cache has no executable tw wrapper")
    with tempfile.TemporaryDirectory(prefix="treework-activation-cli-") as build_dir:
        reported = runner(
            [
                "env",
                f"TREEWORK_BUILD_DIR={build_dir}",
                str(wrapper),
                "version",
            ]
        ).strip()
    expected = f"tw {candidate.version}"
    if reported != expected:
        raise ActivationError(
            f"installed CLI reports {reported!r}; expected exact build {expected!r}"
        )


def attest_activation(
    dev_root: Path,
    code_home: Path,
    runner: Runner,
) -> Candidate:
    dev_root = dev_root.resolve(strict=True)
    source_root = (dev_root / "plugins" / PLUGIN_NAME).resolve(strict=True)
    validate_local_configuration(dev_root)
    plugin_list = runner(
        [
            "codex",
            "plugin",
            "list",
            "--marketplace",
            MARKETPLACE_NAME,
            "--json",
            "--available",
        ]
    )
    candidate = validate_candidate(plugin_list, dev_root, source_root, code_home)
    prompt = runner(
        [
            "codex",
            "debug",
            "prompt-input",
            "-c",
            "mcp_servers={}",
            "noop",
        ]
    )
    validate_cache_and_prompt(prompt, candidate)
    validate_hook_bundle(candidate)
    validate_cli_build_version(candidate, runner)
    return candidate


def main() -> None:
    try:
        candidate = attest_activation(
            REPOSITORY_ROOT,
            CODEX_HOME,
            lambda args: run(args, cwd=REPOSITORY_ROOT),
        )
    except ActivationError as error:
        fail(str(error))
    ok(
        "candidate marketplace, source, installed version, prompt cache, "
        "regular-file bytes, and CLI build version match"
    )
    ok(f"installed hook bundle present for {candidate.version}")
    print("note: hook trust/review is intentionally not changed by this check")


if __name__ == "__main__":
    main()
