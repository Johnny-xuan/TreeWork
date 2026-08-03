#!/usr/bin/env python3
"""Read-only TreeWork MCP server.

The server intentionally implements a small subset of MCP over stdio without
external dependencies. It only exposes read/check tools; TreeWork state
mutation remains owned by the Rust `tw` transaction core.
"""

from __future__ import annotations

import atexit
import hashlib
import json
import math
import os
import queue
import re
import signal
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any


PROTOCOL_VERSION = "2025-11-25"
PLUGIN_ROOT = Path(__file__).resolve().parents[1]
PLUGIN_VERSION = str(
    json.loads(
        (PLUGIN_ROOT / ".codex-plugin" / "plugin.json").read_text(encoding="utf-8")
    )["version"]
)
TW = PLUGIN_ROOT / "skills" / "treework" / "scripts" / "tw"
INSTRUCTIONS = (
    "TreeWork MCP is read-only. Use it to inspect live branch "
    "recall/documents and check output. Launch Project Map after the first "
    "accepted Tree or when the user explicitly asks to open it, and pass an "
    "absolute workspace path. The "
    "launcher may refresh disposable panel assets but never changes accepted "
    "TreeWork state. Branch transitions must still go through the bundled tw "
    "transaction CLI."
)
PROJECT_MAP_URL = re.compile(
    r"^Serving TreeWork Project Map at "
    r"(http://127\.0\.0\.1:(?P<port>\d+)/project-map\.html)\s*$"
)
PROJECT_MAP_STARTUP_TIMEOUT_ENV = "TREEWORK_PROJECT_MAP_STARTUP_TIMEOUT_SECONDS"
PROJECT_MAP_STARTUP_TIMEOUT_DEFAULT_SECONDS = 300.0
PROJECT_MAP_PROBE_TIMEOUT_SECONDS = 3.0


class ToolFailure(Exception):
    """Tool-level failure returned as an MCP tool error, not a protocol error."""


@dataclass
class OwnedProjectMap:
    workspace: Path
    identity: tuple[int, int]
    process: subprocess.Popen[str]
    url: str
    output: list[str] = field(default_factory=list)
    output_queue: queue.Queue[str | None] = field(default_factory=queue.Queue)
    reader: threading.Thread | None = None


PROJECT_MAPS: dict[tuple[int, int], OwnedProjectMap] = {}
PROJECT_MAPS_LOCK = threading.RLock()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def read_text(path: Path, max_chars: int = 12000) -> str:
    if not path.is_file():
        return ""
    text = path.read_text(encoding="utf-8", errors="replace")
    if len(text) > max_chars:
        return text[:max_chars] + f"\n\n[truncated to {max_chars} chars]"
    return text


def resolve_workspace(arguments: dict[str, Any] | None) -> Path:
    raw = ""
    if arguments:
        value = arguments.get("workspace")
        if isinstance(value, str):
            raw = value.strip()
    raw = raw or os.environ.get("TREEWORK_WORKSPACE", "").strip() or os.getcwd()
    return Path(raw).expanduser().resolve()


def resolve_absolute_workspace(arguments: dict[str, Any] | None) -> Path:
    raw = arguments.get("workspace") if arguments else None
    if not isinstance(raw, str) or not raw.strip():
        raise ToolFailure("treework_project_map requires an absolute workspace path")
    requested = Path(raw.strip()).expanduser()
    if not requested.is_absolute():
        raise ToolFailure("treework_project_map workspace must be an absolute path")
    try:
        workspace = requested.resolve(strict=True)
    except FileNotFoundError as exc:
        raise ToolFailure(f"TreeWork workspace does not exist: {requested}") from exc
    if not workspace.is_dir():
        raise ToolFailure(f"TreeWork workspace is not a directory: {workspace}")
    treework_dir(workspace)
    return workspace


def workspace_identity(workspace: Path) -> tuple[int, int]:
    try:
        metadata = workspace.stat()
    except OSError as exc:
        raise ToolFailure(f"Unable to identify TreeWork workspace {workspace}: {exc}") from exc
    return (metadata.st_dev, metadata.st_ino)


def project_map_startup_timeout_seconds() -> float:
    raw = os.environ.get(PROJECT_MAP_STARTUP_TIMEOUT_ENV, "").strip()
    if not raw:
        return PROJECT_MAP_STARTUP_TIMEOUT_DEFAULT_SECONDS
    try:
        timeout = float(raw)
    except ValueError as exc:
        raise ToolFailure(
            f"{PROJECT_MAP_STARTUP_TIMEOUT_ENV} must be a positive number of seconds"
        ) from exc
    if not math.isfinite(timeout) or timeout <= 0:
        raise ToolFailure(
            f"{PROJECT_MAP_STARTUP_TIMEOUT_ENV} must be a positive finite number of seconds"
        )
    return timeout


def treework_dir(workspace: Path) -> Path:
    tw_dir = workspace / ".TreeWork"
    if not tw_dir.is_dir():
        raise ToolFailure(f"{workspace} does not contain a .TreeWork directory")
    return tw_dir


def accepted_state_hash(workspace: Path) -> str:
    tw_dir = treework_dir(workspace)
    roots = [
        tw_dir / "state",
        tw_dir / "events.jsonl",
        tw_dir / "history" / "checkpoints",
    ]
    files: list[Path] = []
    for root in roots:
        if root.is_file():
            files.append(root)
        elif root.is_dir():
            files.extend(path for path in root.rglob("*") if path.is_file())
    digest = hashlib.sha256()
    for path in sorted(files, key=lambda item: item.relative_to(tw_dir).as_posix()):
        relative = path.relative_to(tw_dir).as_posix().encode("utf-8")
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        content = path.read_bytes()
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return f"sha256:{digest.hexdigest()}"


def load_project(workspace: Path) -> dict[str, Any]:
    return load_json(treework_dir(workspace) / "state" / "project.json")


def load_branches(workspace: Path) -> list[dict[str, Any]]:
    state = load_json(treework_dir(workspace) / "state" / "branches.json")
    branches = state.get("branches", [])
    if not isinstance(branches, list):
        raise ToolFailure("branches.json does not contain a branches array")
    normalized = []
    for item in branches:
        if not isinstance(item, dict):
            continue
        branch = dict(item)
        legacy_reason = branch.pop("blocker", "")
        if not branch.get("status_reason") and isinstance(legacy_reason, str):
            branch["status_reason"] = legacy_reason
        if branch.get("status") == "blocked":
            branch["status"] = "paused"
            branch["status_reason"] = branch.get("status_reason") or "Migrated from legacy blocked state."
        elif branch.get("status") == "superseded":
            branch["status"] = "aborted"
            branch["status_reason"] = branch.get("status_reason") or "Migrated from legacy superseded state."
        normalized.append(branch)
    return normalized


def load_edges(workspace: Path) -> list[dict[str, Any]]:
    state = load_json(treework_dir(workspace) / "state" / "graph.json")
    edges = state.get("edges", [])
    if not isinstance(edges, list):
        raise ToolFailure("graph.json does not contain an edges array")
    return [item for item in edges if isinstance(item, dict)]


def branch_by_path(workspace: Path) -> dict[str, dict[str, Any]]:
    return {branch.get("path", ""): branch for branch in load_branches(workspace)}


def pick_branch(workspace: Path, arguments: dict[str, Any] | None) -> str:
    if arguments:
        raw = arguments.get("branch")
        if isinstance(raw, str) and raw.strip():
            return raw.strip()
    current = load_project(workspace).get("current_branch")
    return current if isinstance(current, str) and current else "root"


def related_edges(edges: list[dict[str, Any]], branch: str) -> list[dict[str, Any]]:
    return [
        edge
        for edge in edges
        if edge.get("from") == branch or edge.get("to") == branch
    ]


def text_result(text: str, structured: dict[str, Any] | None = None, *, is_error: bool = False) -> dict[str, Any]:
    result: dict[str, Any] = {
        "content": [{"type": "text", "text": text}],
        "isError": is_error,
    }
    if structured is not None:
        result["structuredContent"] = structured
    return result


def tool_recall(arguments: dict[str, Any] | None) -> dict[str, Any]:
    workspace = resolve_workspace(arguments)
    branch = pick_branch(workspace, arguments)
    max_chars = 12000
    if arguments and isinstance(arguments.get("max_chars"), int):
        max_chars = max(1000, min(arguments["max_chars"], 50000))
    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    result = subprocess.run(
        [str(TW), "recall", branch, "--json"],
        cwd=workspace,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        timeout=60,
    )
    if result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip()
        raise ToolFailure(detail or f"tw recall exited {result.returncode}")
    try:
        projection = json.loads(result.stdout)
    except json.JSONDecodeError as error:
        raise ToolFailure(f"tw recall returned invalid JSON: {error}") from error

    docs = projection.get("docs", {})
    if not isinstance(docs, dict):
        raise ToolFailure("tw recall returned an invalid document projection")
    docs = {
        name: (
            content
            if isinstance(content, str) and len(content) <= max_chars
            else content[:max_chars] + f"\n\n[truncated to {max_chars} chars]"
            if isinstance(content, str)
            else ""
        )
        for name, content in docs.items()
    }
    current = projection.get("branch", {})
    parent = projection.get("parent")
    children = projection.get("children", [])
    rel_edges = projection.get("related_edges", [])
    recall = {
        "branch": current,
        "parent": parent,
        "children": children,
        "related_edges": rel_edges,
        "isolation": projection.get("isolation", {}),
        "tree_revision": projection.get("tree_revision"),
        "publication_marker": projection.get("publication_marker", {}),
        "allowed_actions": projection.get("allowed_actions", []),
        "blocked_actions": projection.get("blocked_actions", []),
        "recall_command": f"tw recall {branch}",
    }
    text_parts = [
        f"Branch: {branch}",
        f"Status: {current.get('status', '')} / {current.get('verification_status', '')}",
        f"Recall command: tw recall {branch}",
        f"Children: {len(children)}",
        f"Related edges: {len(rel_edges)}",
        "Allowed actions: " + ", ".join(recall["allowed_actions"] or ["none"]),
    ]
    for name, content in docs.items():
        if content:
            text_parts.append(f"\n## {name}.md\n{content}")
    return text_result(
        "\n".join(text_parts),
        {
            "workspace": str(workspace),
            "branch": current,
            "docs": docs,
            "recall": recall,
        },
    )


def read_process_output(owned: OwnedProjectMap) -> None:
    assert owned.process.stdout is not None
    for line in owned.process.stdout:
        owned.output.append(line)
        owned.output_queue.put(line)
    owned.output_queue.put(None)


def process_output(owned: OwnedProjectMap) -> str:
    return "".join(owned.output)[-12000:].strip()


def process_group_has_live_members(process_group: int) -> bool:
    try:
        result = subprocess.run(
            ["ps", "-axo", "pgid=,stat="],
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=2,
            check=False,
        )
        if result.returncode == 0:
            for line in result.stdout.splitlines():
                fields = line.split()
                if len(fields) >= 2 and fields[0] == str(process_group):
                    if not fields[1].upper().startswith("Z"):
                        return True
            return False
    except (OSError, subprocess.TimeoutExpired):
        pass
    try:
        os.killpg(process_group, 0)
    except ProcessLookupError:
        return False
    return True


def stop_owned_project_map(owned: OwnedProjectMap) -> None:
    process_group = owned.process.pid
    try:
        os.killpg(process_group, signal.SIGTERM)
    except ProcessLookupError:
        pass

    if owned.process.poll() is None:
        try:
            owned.process.wait(timeout=1)
        except subprocess.TimeoutExpired:
            pass

    deadline = time.monotonic() + 8
    while time.monotonic() < deadline:
        if not process_group_has_live_members(process_group):
            break
        time.sleep(0.05)
    else:
        try:
            os.killpg(process_group, signal.SIGKILL)
        except ProcessLookupError:
            pass

    if owned.process.poll() is None:
        try:
            owned.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            owned.process.kill()
            owned.process.wait(timeout=5)


def cleanup_project_maps() -> None:
    with PROJECT_MAPS_LOCK:
        owned_processes = list(PROJECT_MAPS.values())
        PROJECT_MAPS.clear()
    for owned in owned_processes:
        stop_owned_project_map(owned)


def probe_project_map(url: str) -> bool:
    try:
        with urllib.request.urlopen(
            url,
            timeout=PROJECT_MAP_PROBE_TIMEOUT_SECONDS,
        ) as response:
            return response.status == 200
    except (OSError, urllib.error.URLError):
        return False


def start_project_map(
    workspace: Path,
    identity: tuple[int, int],
) -> OwnedProjectMap:
    startup_timeout = project_map_startup_timeout_seconds()
    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    process = subprocess.Popen(
        [str(TW), "graph", "serve", "--port", "0"],
        cwd=workspace,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        start_new_session=True,
        bufsize=1,
    )
    owned = OwnedProjectMap(
        workspace=workspace,
        identity=identity,
        process=process,
        url="",
    )
    owned.reader = threading.Thread(
        target=read_process_output,
        args=(owned,),
        name=f"treework-project-map-{process.pid}",
        daemon=True,
    )
    owned.reader.start()
    deadline = time.monotonic() + startup_timeout
    try:
        while time.monotonic() < deadline:
            remaining = max(0.01, deadline - time.monotonic())
            try:
                line = owned.output_queue.get(timeout=remaining)
            except queue.Empty:
                break
            if line is None:
                code = process.poll()
                raise ToolFailure(
                    "Project Map process exited before startup completed"
                    f" (exit {code}).\n{process_output(owned)}"
                )
            match = PROJECT_MAP_URL.match(line)
            if match:
                owned.url = match.group(1)
                if not probe_project_map(owned.url):
                    raise ToolFailure(
                        "Project Map reported a URL but did not become reachable.\n"
                        f"{process_output(owned)}"
                    )
                return owned
        raise ToolFailure(
            "Project Map startup timed out before reporting a localhost URL.\n"
            f"{process_output(owned)}"
        )
    except Exception:
        stop_owned_project_map(owned)
        raise


def tool_project_map(arguments: dict[str, Any] | None) -> dict[str, Any]:
    workspace = resolve_absolute_workspace(arguments)
    identity = workspace_identity(workspace)
    before_hash = accepted_state_hash(workspace)
    with PROJECT_MAPS_LOCK:
        owned = PROJECT_MAPS.get(identity)
        reused = bool(
            owned
            and owned.process.poll() is None
            and probe_project_map(owned.url)
        )
        if owned and not reused:
            PROJECT_MAPS.pop(identity, None)
            stop_owned_project_map(owned)
            owned = None
        if owned is None:
            owned = start_project_map(workspace, identity)
            PROJECT_MAPS[identity] = owned
    after_hash = accepted_state_hash(workspace)
    if before_hash != after_hash:
        with PROJECT_MAPS_LOCK:
            PROJECT_MAPS.pop(identity, None)
        stop_owned_project_map(owned)
        raise ToolFailure(
            "Project Map startup changed accepted TreeWork state; the owned "
            "process was stopped"
        )
    status = "reused" if reused else "started"
    structured = {
        "workspace": str(owned.workspace),
        "url": owned.url,
        "status": status,
        "started": not reused,
        "reused": reused,
        "process_id": owned.process.pid,
        "accepted_state_hash_before": before_hash,
        "accepted_state_hash_after": after_hash,
        "accepted_state_unchanged": True,
    }
    return text_result(
        "\n".join(
            [
                f"Project Map {status} for {owned.workspace}",
                f"URL: {owned.url}",
                "Accepted TreeWork state: unchanged",
            ]
        ),
        structured,
    )


def tool_check(arguments: dict[str, Any] | None) -> dict[str, Any]:
    workspace = resolve_workspace(arguments)
    treework_dir(workspace)
    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    result = subprocess.run(
        [str(TW), "check", "--brief"],
        cwd=workspace,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        timeout=60,
    )
    ok = result.returncode == 0
    text = result.stdout.strip() or result.stderr.strip() or f"tw check exited {result.returncode}"
    return text_result(
        text,
        {
            "workspace": str(workspace),
            "ok": ok,
            "returncode": result.returncode,
            "stdout": result.stdout,
            "stderr": result.stderr,
        },
        is_error=not ok,
    )


TOOLS = [
    {
        "name": "treework_recall",
        "title": "TreeWork Branch Recall",
        "description": "Recover a branch from one committed projection, including relationships, documents, isolation, action eligibility, blockers, and publication revision.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": {"type": "string"},
                "branch": {"type": "string", "description": "Branch path. Defaults to current branch."},
                "max_chars": {
                    "type": "integer",
                    "minimum": 1000,
                    "maximum": 50000,
                    "description": "Maximum characters per Markdown document.",
                },
            },
            "additionalProperties": False,
        },
    },
    {
        "name": "treework_project_map",
        "title": "Open TreeWork Project Map",
        "description": "Start or reuse the read-only localhost Project Map after the first accepted Tree or when the user explicitly asks to open it.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": {
                    "type": "string",
                    "description": "Absolute path to an initialized TreeWork workspace.",
                }
            },
            "required": ["workspace"],
            "additionalProperties": False,
        },
    },
    {
        "name": "treework_check",
        "title": "TreeWork Check",
        "description": "Run read-only `tw check --brief` in the target workspace and return validator output.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "workspace": {"type": "string"}
            },
            "additionalProperties": False,
        },
    },
]

TOOL_HANDLERS = {
    "treework_recall": tool_recall,
    "treework_project_map": tool_project_map,
    "treework_check": tool_check,
}


def jsonrpc_result(message_id: Any, result: Any) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": message_id, "result": result}


def jsonrpc_error(message_id: Any, code: int, message: str) -> dict[str, Any]:
    return {"jsonrpc": "2.0", "id": message_id, "error": {"code": code, "message": message}}


def write_message(message: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(message, separators=(",", ":"), ensure_ascii=False) + "\n")
    sys.stdout.flush()


def handle_request(message: dict[str, Any]) -> dict[str, Any] | None:
    message_id = message.get("id")
    method = message.get("method")
    params = message.get("params")
    is_request = "id" in message

    if not isinstance(method, str):
        return jsonrpc_error(message_id, -32600, "Invalid Request: missing method") if is_request else None
    if method == "notifications/initialized":
        return None
    if not is_request:
        return None

    if method == "initialize":
        requested_version = PROTOCOL_VERSION
        if isinstance(params, dict) and isinstance(params.get("protocolVersion"), str):
            requested_version = params["protocolVersion"]
        return jsonrpc_result(
            message_id,
            {
                "protocolVersion": requested_version or PROTOCOL_VERSION,
                "capabilities": {"tools": {"listChanged": False}},
                "serverInfo": {
                    "name": "treework",
                    "version": PLUGIN_VERSION,
                },
                "instructions": INSTRUCTIONS,
            },
        )
    if method == "ping":
        return jsonrpc_result(message_id, {})
    if method == "tools/list":
        return jsonrpc_result(message_id, {"tools": TOOLS})
    if method == "tools/call":
        if not isinstance(params, dict):
            return jsonrpc_error(message_id, -32602, "tools/call params must be an object")
        name = params.get("name")
        arguments = params.get("arguments") or {}
        if not isinstance(name, str) or name not in TOOL_HANDLERS:
            return jsonrpc_error(message_id, -32602, f"unknown tool: {name}")
        if not isinstance(arguments, dict):
            return jsonrpc_error(message_id, -32602, "tool arguments must be an object")
        try:
            return jsonrpc_result(message_id, TOOL_HANDLERS[name](arguments))
        except ToolFailure as exc:
            return jsonrpc_result(message_id, text_result(str(exc), {"error": str(exc)}, is_error=True))
        except Exception as exc:  # noqa: BLE001 - keep protocol alive and report as tool error.
            print(f"treework MCP tool {name} failed: {exc}", file=sys.stderr)
            return jsonrpc_result(
                message_id,
                text_result(f"TreeWork MCP tool failed: {exc}", {"error": str(exc)}, is_error=True),
            )

    return jsonrpc_error(message_id, -32601, f"Method not found: {method}")


def main() -> None:
    try:
        for raw_line in sys.stdin:
            line = raw_line.strip()
            if not line:
                continue
            try:
                message = json.loads(line)
            except json.JSONDecodeError as exc:
                write_message(jsonrpc_error(None, -32700, f"Parse error: {exc}"))
                continue
            if not isinstance(message, dict):
                write_message(jsonrpc_error(None, -32600, "Invalid Request: message must be an object"))
                continue
            response = handle_request(message)
            if response is not None:
                write_message(response)
    finally:
        cleanup_project_maps()


atexit.register(cleanup_project_maps)


if __name__ == "__main__":
    main()
