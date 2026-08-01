#!/usr/bin/env python3
"""Runtime checks for TreeWork plugin hooks."""

from __future__ import annotations

import json
import os
import shutil
import stat
import subprocess
import tempfile
from pathlib import Path
from typing import Any

from _paths import PLUGIN_ROOT

HOOKS_DIR = PLUGIN_ROOT / "hooks"
TW = PLUGIN_ROOT / "skills" / "treework" / "scripts" / "tw"

EXPECTED_EVENTS = {
    "PreToolUse": "pre_tool_use.sh",
    "Stop": "stop.sh",
}


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def ok(message: str) -> None:
    print(f"ok: {message}")


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        fail(f"missing {path.relative_to(PLUGIN_ROOT)}")
    except json.JSONDecodeError as exc:
        fail(f"{path.relative_to(PLUGIN_ROOT)} is not valid JSON: {exc}")


def hook_env(build_dir: Path) -> dict[str, str]:
    env = os.environ.copy()
    env["PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    env["TREEWORK_BUILD_DIR"] = str(build_dir)
    return env


def run_hook(
    script_name: str,
    cwd: Path,
    build_dir: Path,
    *,
    stdin: str = "",
) -> subprocess.CompletedProcess[str]:
    script = HOOKS_DIR / script_name
    result = subprocess.run(
        [str(script)],
        cwd=cwd,
        env=hook_env(build_dir),
        input=stdin,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(
            f"{script_name} failed in {cwd} with {result.returncode}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def run_tw(workspace: Path, build_dir: Path, *args: str) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        [str(TW), *args],
        cwd=workspace,
        env=hook_env(build_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(
            f"tw {' '.join(args)} failed in {workspace}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def require_json_line(output: str, label: str) -> dict[str, Any]:
    lines = [line for line in output.splitlines() if line.strip()]
    if len(lines) != 1:
        fail(f"{label} should emit exactly one JSON line, got {len(lines)} line(s): {output!r}")
    try:
        value = json.loads(lines[0])
    except json.JSONDecodeError as exc:
        fail(f"{label} emitted invalid JSON: {exc}: {lines[0]}")
    if not isinstance(value, dict):
        fail(f"{label} JSON output must be an object")
    return value


def check_manifest() -> None:
    hooks = load_json(HOOKS_DIR / "hooks.json")
    if not isinstance(hooks, dict) or not isinstance(hooks.get("hooks"), dict):
        fail("hooks/hooks.json must contain a hooks object")
    configured = hooks["hooks"]
    if "SessionStart" in configured or (HOOKS_DIR / "session_start.sh").exists():
        fail("duplicate startup status hook should not be packaged")
    for event, script_name in EXPECTED_EVENTS.items():
        entries = configured.get(event)
        if not isinstance(entries, list) or not entries:
            fail(f"hooks/hooks.json missing {event}")
        encoded = json.dumps(entries)
        expected_command = f"$PLUGIN_ROOT/hooks/{script_name}"
        if expected_command not in encoded:
            fail(f"{event} should call {expected_command}")
        script = HOOKS_DIR / script_name
        if not script.is_file():
            fail(f"missing hook script {script.relative_to(PLUGIN_ROOT)}")
        if not (script.stat().st_mode & stat.S_IXUSR):
            fail(f"hook script is not executable: {script.relative_to(PLUGIN_ROOT)}")
    ok("hook manifest and executables")


def make_workspace(root: Path, build_dir: Path, name: str) -> Path:
    workspace = root / name
    workspace.mkdir()
    run_tw(workspace, build_dir, "init")
    return workspace


def make_dirty(workspace: Path) -> None:
    branches_path = workspace / ".TreeWork" / "state" / "branches.json"
    state = load_json(branches_path)
    for branch in state["branches"]:
        if branch["path"] == "root":
            branch["status"] = "complete"
            branch["verification_status"] = "unverified"
            break
    branches_path.write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")


def check_no_treework_workspace(root: Path, build_dir: Path) -> None:
    workspace = root / "plain"
    workspace.mkdir()
    for script_name in EXPECTED_EVENTS.values():
        result = run_hook(script_name, workspace, build_dir, stdin='{"tool_name":"Bash"}')
        if result.stdout.strip():
            fail(f"{script_name} should stay quiet outside .TreeWork workspaces")
    ok("hooks stay quiet outside TreeWork workspaces")


def check_clean_workspace(root: Path, build_dir: Path) -> None:
    workspace = make_workspace(root, build_dir, "clean")

    unrelated = run_hook("pre_tool_use.sh", workspace, build_dir, stdin='{"tool_name":"Bash","command":"echo ok"}')
    if unrelated.stdout.strip():
        fail("pre_tool_use.sh should stay quiet for unrelated input")

    guarded = run_hook(
        "pre_tool_use.sh",
        workspace,
        build_dir,
        stdin='{"tool_name":"apply_patch","file":".TreeWork/state/project.json"}',
    )
    guarded_json = require_json_line(guarded.stdout, "pre_tool_use.sh guardrail")
    if "hookSpecificOutput" not in guarded_json:
        fail("pre_tool_use.sh guardrail output missing hookSpecificOutput")

    stop = run_hook("stop.sh", workspace, build_dir)
    if stop.stdout.strip():
        fail("stop.sh should stay quiet when tw check passes")

    ok("clean TreeWork hook behavior")


def check_first_tree_pending_workspace(root: Path, build_dir: Path) -> None:
    workspace = make_workspace(root, build_dir, "first-tree-pending")
    run_tw(workspace, build_dir, "align", "end")

    check_result = run_tw(workspace, build_dir, "check", "--brief")
    if "0 issue(s)" not in check_result.stdout:
        fail("first Tree pending state produced a false-positive check issue")

    stop = run_hook("stop.sh", workspace, build_dir)
    if stop.stdout.strip():
        fail("stop.sh warned between first align end and tree start")

    ok("first Tree pending state stays quiet")


def check_dirty_workspace(root: Path, build_dir: Path) -> None:
    workspace = make_workspace(root, build_dir, "dirty")
    make_dirty(workspace)

    check_result = run_tw(workspace, build_dir, "check", "--brief")
    if "0 issue(s)" in check_result.stdout:
        fail("dirty workspace did not produce TreeWork check issues")

    stop = run_hook("stop.sh", workspace, build_dir)
    stop_json = require_json_line(stop.stdout, "stop.sh dirty check")
    if stop_json.get("continue") is not True or "systemMessage" not in stop_json:
        fail("stop.sh dirty output must request continue with a systemMessage")

    ok("dirty TreeWork hook warnings")


def main() -> None:
    if not TW.is_file():
        fail(f"missing tw wrapper at {TW}")
    check_manifest()

    temp_root = Path(tempfile.mkdtemp(prefix="treework-hooks-"))
    build_dir = temp_root / ".build"
    try:
        check_no_treework_workspace(temp_root, build_dir)
        check_clean_workspace(temp_root, build_dir)
        check_first_tree_pending_workspace(temp_root, build_dir)
        check_dirty_workspace(temp_root, build_dir)
    finally:
        shutil.rmtree(temp_root, ignore_errors=True)


if __name__ == "__main__":
    main()
