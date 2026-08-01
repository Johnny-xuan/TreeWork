#!/usr/bin/env python3
"""Smoke-check the TreeWork MCP server over stdio JSON-RPC."""

from __future__ import annotations

import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.request
from pathlib import Path
from typing import Any

from _paths import PLUGIN_ROOT

SERVER = PLUGIN_ROOT / "scripts" / "start-mcp.sh"
TW = PLUGIN_ROOT / "skills" / "treework" / "scripts" / "tw"
MCP_MODULE = PLUGIN_ROOT / "mcp" / "treework_mcp.py"


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def ok(message: str) -> None:
    print(f"ok: {message}")


def run_tw(workspace: Path, build_dir: Path, *args: str) -> None:
    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    env["TREEWORK_BUILD_DIR"] = str(build_dir)
    result = subprocess.run(
        [str(TW), *args],
        cwd=workspace,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(f"tw {' '.join(args)} failed:\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}")


def accepted_state_snapshot(workspace: Path) -> dict[str, bytes]:
    tw_dir = workspace / ".TreeWork"
    roots = [
        tw_dir / "state",
        tw_dir / "events.jsonl",
        tw_dir / "history" / "checkpoints",
    ]
    snapshot: dict[str, bytes] = {}
    for root in roots:
        if root.is_file():
            snapshot[root.relative_to(tw_dir).as_posix()] = root.read_bytes()
        elif root.is_dir():
            for path in sorted(item for item in root.rglob("*") if item.is_file()):
                snapshot[path.relative_to(tw_dir).as_posix()] = path.read_bytes()
    return snapshot


def process_exists(process_id: int) -> bool:
    try:
        os.kill(process_id, 0)
    except ProcessLookupError:
        return False
    status = subprocess.run(
        ["ps", "-o", "stat=", "-p", str(process_id)],
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        check=False,
    ).stdout.strip()
    if not status or status.upper().startswith("Z"):
        return False
    return True


def wait_for_process_exit(process_id: int, timeout: float = 10.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if not process_exists(process_id):
            return
        time.sleep(0.05)
    fail(f"owned Project Map process {process_id} survived MCP host exit")


def load_mcp_module() -> Any:
    spec = importlib.util.spec_from_file_location(
        "treework_mcp_lifecycle_check",
        MCP_MODULE,
    )
    if spec is None or spec.loader is None:
        fail("unable to load MCP module for lifecycle checks")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def check_launcher_timeout_and_process_groups(temp_root: Path) -> None:
    module = load_mcp_module()
    timeout_env = module.PROJECT_MAP_STARTUP_TIMEOUT_ENV
    previous_timeout = os.environ.get(timeout_env)
    original_tw = module.TW
    try:
        os.environ.pop(timeout_env, None)
        if module.project_map_startup_timeout_seconds() != 300:
            fail("Project Map startup timeout default is not 300 seconds")
        os.environ[timeout_env] = "2"
        if module.project_map_startup_timeout_seconds() != 2:
            fail("Project Map startup timeout override was ignored")
        os.environ[timeout_env] = "0"
        try:
            module.project_map_startup_timeout_seconds()
        except module.ToolFailure:
            pass
        else:
            fail("Project Map startup timeout accepted a non-positive value")

        child_pid_file = temp_root / "timeout-child.pid"
        fake_tw = temp_root / "fake-timeout-tw"
        fake_tw.write_text(
            "#!/usr/bin/env python3\n"
            "import os\n"
            "import subprocess\n"
            "child = subprocess.Popen(['sleep', '300'])\n"
            "with open(os.environ['TREEWORK_TEST_CHILD_PID_FILE'], 'w', encoding='utf-8') as handle:\n"
            "    handle.write(str(child.pid))\n"
            "child.wait()\n",
            encoding="utf-8",
        )
        fake_tw.chmod(0o755)
        module.TW = fake_tw
        os.environ[timeout_env] = "2"
        os.environ["TREEWORK_TEST_CHILD_PID_FILE"] = str(child_pid_file)
        workspace = temp_root / "timeout-workspace"
        workspace.mkdir()
        timeout_error = ""
        try:
            module.start_project_map(
                workspace,
                module.workspace_identity(workspace),
            )
        except module.ToolFailure as error:
            timeout_error = str(error)
            if "timed out" not in str(error):
                fail(f"Project Map timeout error was unclear: {error}")
        else:
            fail("fake Project Map unexpectedly completed startup")
        if not child_pid_file.is_file():
            fail(
                "timeout fixture did not start its background child: "
                f"{timeout_error}; fake={fake_tw} tw={module.TW}"
            )
        wait_for_process_exit(int(child_pid_file.read_text(encoding="utf-8")))

        early_child_pid_file = temp_root / "early-exit-child.pid"
        leader = subprocess.Popen(
            [
                "/bin/sh",
                "-c",
                f"sleep 300 & printf '%s' \"$!\" > {early_child_pid_file!s}",
            ],
            start_new_session=True,
        )
        leader.wait(timeout=5)
        early_child_pid = int(early_child_pid_file.read_text(encoding="utf-8"))
        if not process_exists(early_child_pid):
            fail("leader early-exit fixture child did not remain alive")
        owned = module.OwnedProjectMap(
            workspace=workspace,
            identity=module.workspace_identity(workspace),
            process=leader,
            url="",
        )
        module.stop_owned_project_map(owned)
        wait_for_process_exit(early_child_pid)
        ok("mcp launcher timeout and leader-exit process-group cleanup")
    finally:
        module.TW = original_tw
        os.environ.pop("TREEWORK_TEST_CHILD_PID_FILE", None)
        if previous_timeout is None:
            os.environ.pop(timeout_env, None)
        else:
            os.environ[timeout_env] = previous_timeout


class McpClient:
    def __init__(self, build_dir: Path) -> None:
        env = os.environ.copy()
        env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
        env["TREEWORK_BUILD_DIR"] = str(build_dir)
        self.proc = subprocess.Popen(
            [str(SERVER)],
            cwd=PLUGIN_ROOT,
            env=env,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        self.next_id = 1

    def close(self) -> None:
        if self.proc.stdin:
            self.proc.stdin.close()
        try:
            self.proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.proc.kill()
            self.proc.wait(timeout=5)

    def request(self, method: str, params: dict[str, Any] | None = None) -> dict[str, Any]:
        if self.proc.stdin is None or self.proc.stdout is None:
            fail("MCP process pipes are not available")
        message_id = self.next_id
        self.next_id += 1
        payload: dict[str, Any] = {"jsonrpc": "2.0", "id": message_id, "method": method}
        if params is not None:
            payload["params"] = params
        self.proc.stdin.write(json.dumps(payload, separators=(",", ":")) + "\n")
        self.proc.stdin.flush()
        line = self.proc.stdout.readline()
        if not line:
            stderr = self.proc.stderr.read() if self.proc.stderr else ""
            fail(f"MCP server closed stdout unexpectedly. stderr:\n{stderr}")
        response = json.loads(line)
        if response.get("id") != message_id:
            fail(f"MCP response id mismatch: {response}")
        return response

    def notify(self, method: str, params: dict[str, Any] | None = None) -> None:
        if self.proc.stdin is None:
            fail("MCP process stdin is not available")
        payload: dict[str, Any] = {"jsonrpc": "2.0", "method": method}
        if params is not None:
            payload["params"] = params
        self.proc.stdin.write(json.dumps(payload, separators=(",", ":")) + "\n")
        self.proc.stdin.flush()


def assert_tool_result(response: dict[str, Any], tool_name: str) -> dict[str, Any]:
    result = response.get("result")
    if not isinstance(result, dict):
        fail(f"{tool_name} result is not an object: {response}")
    content = result.get("content")
    if not isinstance(content, list) or not content:
        fail(f"{tool_name} result missing text content")
    if result.get("isError"):
        fail(f"{tool_name} unexpectedly returned tool error: {content}")
    structured = result.get("structuredContent")
    if not isinstance(structured, dict):
        fail(f"{tool_name} result missing structuredContent")
    return structured


def write_tree(workspace: Path) -> None:
    path = workspace / ".TreeWork" / "tree.yaml"
    path.write_text(
        """version: 1
tree:
  id: root
  title: MCP Test
  purpose: Project-wide coordination and integration.
  spec: spec.md
  children:
    - id: mcp-ready
      title: MCP Ready
      purpose: Parent branch for MCP projection checks.
      children:
        - id: mcp-sample
          title: MCP Sample
          purpose: Branch used for MCP recall checks.
          spec: branches/mcp-sample/spec.md
          depends_on:
            - mcp-ready
""",
        encoding="utf-8",
    )


def prepare_workspace(workspace: Path, build_dir: Path) -> None:
    workspace.mkdir()
    run_tw(workspace, build_dir, "init")
    run_tw(workspace, build_dir, "align", "end")
    run_tw(workspace, build_dir, "tree", "start")
    write_tree(workspace)
    run_tw(workspace, build_dir, "tree", "apply")
    run_tw(workspace, build_dir, "enter", "mcp-sample", "--no-isolate")
    run_tw(workspace, build_dir, "check", "--brief")


def main() -> None:
    if not SERVER.is_file():
        fail("missing scripts/start-mcp.sh")
    if not os.access(SERVER, os.X_OK):
        fail("scripts/start-mcp.sh is not executable")
    mcp_manifest = PLUGIN_ROOT / ".mcp.json"
    try:
        manifest = json.loads(mcp_manifest.read_text(encoding="utf-8"))
    except FileNotFoundError:
        fail("missing .mcp.json")
    server_cfg = manifest.get("mcpServers", {}).get("treework")
    if not isinstance(server_cfg, dict):
        fail(".mcp.json must define mcpServers.treework")
    if server_cfg.get("command") != "bash" or server_cfg.get("args") != ["./scripts/start-mcp.sh"]:
        fail(".mcp.json treework command must use ./scripts/start-mcp.sh")
    ok("mcp manifest")

    temp_root = Path(tempfile.mkdtemp(prefix="treework-mcp-check-"))
    build_dir = temp_root / ".build"
    workspace = temp_root / "workspace"
    second_workspace = temp_root / "second-workspace"
    workspace_alias = temp_root / "workspace-alias"
    empty_workspace = temp_root / "empty"
    empty_workspace.mkdir()
    try:
        check_launcher_timeout_and_process_groups(temp_root)
        prepare_workspace(workspace, build_dir)
        prepare_workspace(second_workspace, build_dir)
        workspace_alias.symlink_to(workspace, target_is_directory=True)

        client = McpClient(build_dir)
        owned_processes: list[int] = []
        try:
            init = client.request(
                "initialize",
                {
                    "protocolVersion": "2025-11-25",
                    "capabilities": {},
                    "clientInfo": {"name": "treework-check", "version": "0"},
                },
            )
            init_result = init.get("result")
            if not isinstance(init_result, dict):
                fail("initialize result is not an object")
            if "read-only" not in init_result.get("instructions", ""):
                fail("initialize instructions must explain read-only scope")
            tools_cap = init_result.get("capabilities", {}).get("tools")
            if not isinstance(tools_cap, dict):
                fail("initialize result must advertise tools capability")
            client.notify("notifications/initialized")

            tool_list = client.request("tools/list")
            tools = tool_list.get("result", {}).get("tools")
            if not isinstance(tools, list):
                fail("tools/list did not return a tools array")
            names = [tool.get("name") for tool in tools]
            expected = [
                "treework_recall",
                "treework_project_map",
                "treework_check",
            ]
            if names != expected:
                fail(f"unexpected MCP tool list order: {names}")
            if "treework_graph" in names:
                fail("legacy treework_graph must not remain in the shipped MCP surface")
            ok("mcp initialize and tools/list")

            base_args = {"workspace": str(workspace)}
            branch = assert_tool_result(
                client.request(
                    "tools/call",
                    {"name": "treework_recall", "arguments": {**base_args, "branch": "mcp-sample"}},
                ),
                "treework_recall",
            )
            if "progress" not in branch.get("docs", {}):
                fail("treework_recall did not return branch docs")
            if "spec" not in branch.get("docs", {}):
                fail("treework_recall did not return the branch Spec")
            recall = branch.get("recall", {})
            if recall.get("recall_command") != "tw recall mcp-sample":
                fail("treework_recall did not return live recall command")
            if recall.get("parent", {}).get("path") != "mcp-ready":
                fail("treework_recall did not return branch parent")
            if not isinstance(recall.get("allowed_actions"), list):
                fail("treework_recall did not return action eligibility")
            if not isinstance(recall.get("blocked_actions"), list):
                fail("treework_recall did not return blocked action reasons")
            if not isinstance(recall.get("publication_marker"), dict):
                fail("treework_recall did not return the committed publication marker")
            before_launch = accepted_state_snapshot(workspace)
            launched = assert_tool_result(
                client.request(
                    "tools/call",
                    {"name": "treework_project_map", "arguments": base_args},
                ),
                "treework_project_map",
            )
            if launched.get("status") != "started" or launched.get("started") is not True:
                fail(f"first Project Map call did not start a process: {launched}")
            if launched.get("accepted_state_unchanged") is not True:
                fail("Project Map launcher did not attest accepted-state stability")
            if launched.get("accepted_state_hash_before") != launched.get(
                "accepted_state_hash_after"
            ):
                fail("Project Map launcher returned different accepted-state hashes")
            if accepted_state_snapshot(workspace) != before_launch:
                fail("Project Map launch changed accepted TreeWork state")
            url = launched.get("url")
            if not isinstance(url, str) or not url.startswith("http://127.0.0.1:"):
                fail(f"Project Map launcher returned an invalid URL: {url!r}")
            with urllib.request.urlopen(url, timeout=5) as response:
                body = response.read().decode("utf-8")
                if response.status != 200 or 'id="root"' not in body:
                    fail("Project Map launcher URL did not serve the production entrypoint")
            first_pid = launched.get("process_id")
            if not isinstance(first_pid, int) or not process_exists(first_pid):
                fail("Project Map launcher did not return a live owned process")
            owned_processes.append(first_pid)

            reused = assert_tool_result(
                client.request(
                    "tools/call",
                    {
                        "name": "treework_project_map",
                        "arguments": {"workspace": str(workspace_alias)},
                    },
                ),
                "treework_project_map",
            )
            if (
                reused.get("status") != "reused"
                or reused.get("reused") is not True
                or reused.get("url") != url
                or reused.get("process_id") != first_pid
                or reused.get("workspace") != str(workspace.resolve())
            ):
                fail(f"canonical workspace call did not reuse Project Map: {reused}")

            if sys.platform == "darwin":
                case_alias = workspace.with_name(workspace.name.upper())
                try:
                    alias_stat = case_alias.stat()
                    workspace_stat = workspace.stat()
                    same_identity = (
                        alias_stat.st_dev,
                        alias_stat.st_ino,
                    ) == (
                        workspace_stat.st_dev,
                        workspace_stat.st_ino,
                    )
                except FileNotFoundError:
                    same_identity = False
                if same_identity:
                    case_reused = assert_tool_result(
                        client.request(
                            "tools/call",
                            {
                                "name": "treework_project_map",
                                "arguments": {"workspace": str(case_alias)},
                            },
                        ),
                        "treework_project_map",
                    )
                    if (
                        case_reused.get("status") != "reused"
                        or case_reused.get("url") != url
                        or case_reused.get("process_id") != first_pid
                    ):
                        fail(
                            "case-aliased workspace did not reuse Project Map: "
                            f"{case_reused}"
                        )
                    ok("mcp macOS case-alias workspace identity reuse")
                else:
                    ok("mcp case-alias test skipped on case-sensitive volume")

            second = assert_tool_result(
                client.request(
                    "tools/call",
                    {
                        "name": "treework_project_map",
                        "arguments": {"workspace": str(second_workspace)},
                    },
                ),
                "treework_project_map",
            )
            second_pid = second.get("process_id")
            if (
                second.get("status") != "started"
                or second.get("url") == url
                or second_pid == first_pid
                or not isinstance(second_pid, int)
                or not process_exists(second_pid)
            ):
                fail(f"second workspace did not receive an isolated process: {second}")
            owned_processes.append(second_pid)
            ok("mcp Project Map launch, canonical reuse, and workspace isolation")

            check = assert_tool_result(
                client.request("tools/call", {"name": "treework_check", "arguments": base_args}),
                "treework_check",
            )
            if check.get("ok") is not True:
                fail("treework_check did not report ok")
            ok("mcp tool calls")

            missing = client.request(
                "tools/call",
                {"name": "treework_recall", "arguments": {"workspace": str(empty_workspace)}},
            )
            missing_result = missing.get("result", {})
            if not missing_result.get("isError"):
                fail("treework_recall should fail cleanly outside a TreeWork workspace")
            relative = client.request(
                "tools/call",
                {
                    "name": "treework_project_map",
                    "arguments": {"workspace": "relative/workspace"},
                },
            )
            if not relative.get("result", {}).get("isError"):
                fail("treework_project_map should reject a relative workspace")
            invalid = client.request(
                "tools/call",
                {
                    "name": "treework_project_map",
                    "arguments": {"workspace": str(empty_workspace)},
                },
            )
            if not invalid.get("result", {}).get("isError"):
                fail("treework_project_map should reject an uninitialized workspace")
            old_graph = client.request(
                "tools/call",
                {"name": "treework_graph", "arguments": base_args},
            )
            if old_graph.get("error", {}).get("code") != -32602:
                fail("treework_graph should be absent rather than aliased")
            ok("mcp launcher validation and removed legacy tool")
        finally:
            client.close()
        for process_id in owned_processes:
            wait_for_process_exit(process_id)
        ok("mcp host exit cleaned owned Project Map processes")
    finally:
        shutil.rmtree(temp_root, ignore_errors=True)


if __name__ == "__main__":
    main()
