#!/usr/bin/env python3
"""Validate the TreeWork Pi package and load it through Pi when available."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path

from _paths import REPOSITORY_ROOT

PACKAGE = REPOSITORY_ROOT / "package.json"
ADAPTER = REPOSITORY_ROOT / "adapters" / "pi"
EXTENSION = ADAPTER / "index.ts"
SKILL = REPOSITORY_ROOT / "plugins" / "treework" / "skills" / "treework"


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def ok(message: str) -> None:
    print(f"ok: {message}")


def check_manifest() -> None:
    try:
        package = json.loads(PACKAGE.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError) as error:
        fail(f"invalid Pi package manifest: {error}")
    if package.get("name") != "treework-pi-adapter":
        fail("root package.json must identify treework-pi-adapter")
    pi = package.get("pi")
    if not isinstance(pi, dict):
        fail("root package.json must contain a pi manifest")
    if pi.get("extensions") != ["./adapters/pi/index.ts"]:
        fail("Pi manifest must load the focused adapter extension")
    if pi.get("skills") != ["./plugins/treework/skills/treework"]:
        fail("Pi manifest must reuse the shipped TreeWork skill")
    if package.get("license") != "MIT":
        fail("Pi adapter must retain the repository MIT license")
    plugin = json.loads(
        (REPOSITORY_ROOT / "plugins" / "treework" / ".codex-plugin" / "plugin.json").read_text(
            encoding="utf-8"
        )
    )
    if package.get("version") != plugin.get("version"):
        fail("Pi adapter and shared TreeWork runtime versions must match")
    ok("Pi package manifest")


def check_adapter_surface() -> None:
    required = [
        EXTENSION,
        ADAPTER / "core.mjs",
        ADAPTER / "mcp-client.mjs",
        ADAPTER / "README.md",
        ADAPTER / "tests" / "core.test.mjs",
        ADAPTER / "tests" / "mcp-client.test.mjs",
        SKILL / "SKILL.md",
    ]
    missing = [str(path.relative_to(REPOSITORY_ROOT)) for path in required if not path.is_file()]
    if missing:
        fail(f"Pi adapter is missing required files: {missing}")
    source = EXTENSION.read_text(encoding="utf-8")
    contracts = [
        'pi.on("tool_call"',
        'pi.on("agent_settled"',
        'pi.on("session_shutdown"',
        'name: "treework_tools"',
        'name: "treework_recall"',
        'name: "treework_project_map"',
        'name: "treework_check"',
        'pi.registerCommand("treework-enter"',
        'pi.registerCommand("treework-return"',
        "SessionManager.forkFrom",
    ]
    missing_contracts = [contract for contract in contracts if contract not in source]
    if missing_contracts:
        fail(f"Pi adapter is missing compatibility contracts: {missing_contracts}")
    executable_sources = "\n".join(
        path.read_text(encoding="utf-8")
        for path in ADAPTER.rglob("*")
        if path.is_file() and path.suffix in {".ts", ".mjs"}
    )
    if "auth.json" in executable_sources or "OPENAI_API_KEY" in executable_sources:
        fail("Pi adapter must not read Codex credentials or API keys")
    ok("Pi adapter compatibility surface")


def check_node_tests() -> None:
    result = subprocess.run(
        [
            "node",
            "--test",
            "adapters/pi/tests/core.test.mjs",
            "adapters/pi/tests/mcp-client.test.mjs",
        ],
        cwd=REPOSITORY_ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if result.returncode != 0:
        fail(f"Pi adapter Node tests failed:\n{result.stdout}")
    ok("Pi adapter guardrail and MCP integration tests")


def pi_executable() -> str | None:
    pi_bin = shutil.which("pi")
    if not pi_bin and os.environ.get("TREEWORK_REQUIRE_PI") == "1":
        fail("Pi executable is required but not available")
    return pi_bin


def run_pi_rpc_load(
    pi_bin: str,
    extra_args: list[str],
    env: dict[str, str],
) -> set[str]:
    result = subprocess.run(
        [pi_bin, "--mode", "rpc", "--no-session", *extra_args],
        cwd=REPOSITORY_ROOT,
        env=env,
        input='{"id":"commands","type":"get_commands"}\n',
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        timeout=60,
    )
    combined = f"{result.stdout}\n{result.stderr}".lower()
    if result.returncode != 0 or "failed to load extension" in combined:
        fail(f"Pi failed to load the adapter:\n{result.stdout}\n{result.stderr}")
    responses = []
    for line in result.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("type") == "response" and message.get("id") == "commands":
            responses.append(message)
    if len(responses) != 1 or not responses[0].get("success"):
        fail(f"Pi did not answer the adapter command probe:\n{result.stdout}\n{result.stderr}")
    return {
        command.get("name")
        for command in responses[0].get("data", {}).get("commands", [])
        if isinstance(command.get("name"), str)
    }


def check_pi_load(pi_bin: str | None) -> None:
    if not pi_bin:
        print("skip: Pi executable unavailable; structural and integration checks completed")
        return
    env = os.environ.copy()
    env["PI_OFFLINE"] = "1"
    commands = run_pi_rpc_load(
        pi_bin,
        ["--no-skills", "--no-extensions", "-e", str(EXTENSION), "--skill", str(SKILL)],
        env,
    )
    if not {"treework-enter", "treework-return", "skill:treework"} <= commands:
        fail(f"Pi adapter probe is missing commands or the shared skill: {sorted(commands)}")
    ok("Pi runtime loads adapter and shared TreeWork skill")


def package_source_matches(source: object, agent_dir: Path) -> bool:
    if not isinstance(source, str):
        return False
    target = REPOSITORY_ROOT.resolve()
    return any(
        (base / source).resolve() == target
        for base in (agent_dir, REPOSITORY_ROOT, Path.cwd())
    )


def check_pi_package_install(pi_bin: str | None) -> None:
    if not pi_bin:
        return
    with tempfile.TemporaryDirectory(prefix="treework-pi-package-") as temp_name:
        agent_dir = Path(temp_name) / "agent"
        env = os.environ.copy()
        env["PI_OFFLINE"] = "1"
        env["PI_CODING_AGENT_DIR"] = str(agent_dir)
        install = subprocess.run(
            [pi_bin, "install", str(REPOSITORY_ROOT)],
            cwd=REPOSITORY_ROOT,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=60,
        )
        if install.returncode != 0:
            fail(f"Pi local package install failed:\n{install.stdout}\n{install.stderr}")
        settings_path = agent_dir / "settings.json"
        settings = json.loads(settings_path.read_text(encoding="utf-8"))
        packages = settings.get("packages", [])
        recorded = any(package_source_matches(source, agent_dir) for source in packages)
        if not recorded:
            fail(f"Pi did not record the TreeWork package source: {packages}")
        commands = run_pi_rpc_load(pi_bin, [], env)
        if not {"treework-enter", "treework-return", "skill:treework"} <= commands:
            fail(f"installed Pi package is missing commands or the shared skill: {sorted(commands)}")
        if (agent_dir / "cache" / "treework").exists():
            fail("loading the Pi package eagerly created the TreeWork runtime cache")
        remove = subprocess.run(
            [pi_bin, "remove", str(REPOSITORY_ROOT)],
            cwd=REPOSITORY_ROOT,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=60,
        )
        if remove.returncode != 0:
            fail(f"Pi local package rollback failed:\n{remove.stdout}\n{remove.stderr}")
        settings = json.loads(settings_path.read_text(encoding="utf-8"))
        still_enabled = any(
            package_source_matches(source, agent_dir)
            for source in settings.get("packages", [])
        )
        if still_enabled:
            fail("Pi remove left the TreeWork package enabled")
    ok("Pi package installs, loads, and rolls back reversibly")


def main() -> None:
    check_manifest()
    check_adapter_surface()
    check_node_tests()
    pi_bin = pi_executable()
    check_pi_load(pi_bin)
    check_pi_package_install(pi_bin)


if __name__ == "__main__":
    main()
