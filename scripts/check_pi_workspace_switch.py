#!/usr/bin/env python3
"""Exercise Pi's real session handoff across a TreeWork managed worktree."""

from __future__ import annotations

import json
import os
import queue
import re
import shutil
import subprocess
import tempfile
import threading
import time
import uuid
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from _paths import PLUGIN_ROOT, REPOSITORY_ROOT

TW = PLUGIN_ROOT / "skills" / "treework" / "scripts" / "tw"
EXTENSION = REPOSITORY_ROOT / "adapters" / "pi" / "index.ts"


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def run(command: list[str], cwd: Path, env: dict[str, str] | None = None) -> str:
    result = subprocess.run(
        command,
        cwd=cwd,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        timeout=180,
    )
    if result.returncode != 0:
        fail(
            f"command failed ({' '.join(command)}):\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return f"{result.stdout}\n{result.stderr}"


def prepare_treework(control: Path, build_dir: Path) -> Path:
    control.mkdir()
    run(["git", "init", "-q", "-b", "main"], control)
    run(["git", "config", "user.name", "TreeWork Pi Test"], control)
    run(["git", "config", "user.email", "treework-pi@example.invalid"], control)
    (control / "src").mkdir()
    (control / "src" / "baseline.txt").write_text("baseline\n", encoding="utf-8")
    run(["git", "add", "src/baseline.txt"], control)
    run(["git", "commit", "-q", "-m", "baseline"], control)

    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    env["TREEWORK_BUILD_DIR"] = str(build_dir)
    run([str(TW), "init"], control, env)
    run([str(TW), "align", "end"], control, env)
    run([str(TW), "tree", "start"], control, env)
    (control / ".TreeWork" / "tree.yaml").write_text(
        """version: 1
tree:
  id: root
  title: Pi Adapter Test
  purpose: Verify TreeWork workspace handoff in Pi.
  spec: spec.md
  children:
    - id: adapter
      title: Pi Adapter
      purpose: Exercise a managed TreeWork branch worktree.
      spec: branches/adapter/spec.md
""",
        encoding="utf-8",
    )
    run([str(TW), "tree", "apply"], control, env)
    run(["git", "add", ".TreeWork"], control)
    run(["git", "commit", "-q", "-m", "accept TreeWork test tree"], control)
    preview = run([str(TW), "enter", "adapter", "--dry-run"], control, env)
    match = re.search(r"^\s*workspace:\s*(.+?)\s*$", preview, re.MULTILINE)
    if not match:
        fail(f"tw enter --dry-run did not report a workspace:\n{preview}")
    return Path(match.group(1)).resolve()


class RpcClient:
    def __init__(self, process: subprocess.Popen[str]) -> None:
        self.process = process
        self.lines: queue.Queue[str] = queue.Queue()
        self.stderr: list[str] = []
        self.events: list[str] = []
        self.reader = threading.Thread(target=self._read_stdout, daemon=True)
        self.err_reader = threading.Thread(target=self._read_stderr, daemon=True)
        self.reader.start()
        self.err_reader.start()

    def _read_stdout(self) -> None:
        assert self.process.stdout is not None
        for line in self.process.stdout:
            stripped = line.rstrip("\n")
            self.events.append(stripped)
            self.lines.put(stripped)

    def _read_stderr(self) -> None:
        assert self.process.stderr is not None
        for line in self.process.stderr:
            self.stderr.append(line.rstrip("\n"))

    def request(self, request: dict[str, Any], timeout: float = 30) -> dict[str, Any]:
        assert self.process.stdin is not None
        request_id = request.setdefault("id", f"req-{time.monotonic_ns()}")
        self.process.stdin.write(json.dumps(request, separators=(",", ":")) + "\n")
        self.process.stdin.flush()
        deadline = time.monotonic() + timeout
        observed: list[str] = []
        while time.monotonic() < deadline:
            try:
                raw = self.lines.get(timeout=max(0.01, deadline - time.monotonic()))
            except queue.Empty:
                break
            observed.append(raw)
            try:
                message = json.loads(raw)
            except json.JSONDecodeError:
                continue
            if message.get("type") == "response" and message.get("id") == request_id:
                if not message.get("success"):
                    fail(f"Pi RPC request failed: {message}\nobserved={observed}\nstderr={self.stderr}")
                return message
        fail(
            f"timed out waiting for Pi RPC response to {request}\n"
            f"observed={observed}\nstderr={self.stderr}"
        )
        raise AssertionError("unreachable")


def session_header(session_file: str) -> dict[str, Any]:
    with Path(session_file).open(encoding="utf-8") as handle:
        return json.loads(handle.readline())


def write_source_session(path: Path, cwd: Path) -> None:
    path.write_text(
        json.dumps(
            {
                "type": "session",
                "version": 3,
                "id": str(uuid.uuid4()),
                "timestamp": datetime.now(timezone.utc).isoformat(),
                "cwd": str(cwd.resolve()),
            },
            separators=(",", ":"),
        )
        + "\n",
        encoding="utf-8",
    )


def branch_record(control: Path, branch: str) -> dict[str, Any]:
    state = json.loads((control / ".TreeWork" / "state" / "branches.json").read_text())
    return next(item for item in state["branches"] if item["path"] == branch)


def wait_for_session(
    client: RpcClient,
    expected_cwd: Path,
    previous_file: str,
    timeout: float = 20,
) -> tuple[dict[str, Any], dict[str, Any]]:
    deadline = time.monotonic() + timeout
    last_state: dict[str, Any] = {}
    last_header: dict[str, Any] | None = None
    while time.monotonic() < deadline:
        state = client.request({"type": "get_state"})["data"]
        last_state = state
        session_file = state.get("sessionFile")
        if session_file and Path(session_file).is_file():
            header = session_header(session_file)
            last_header = header
            if session_file != previous_file and Path(header.get("cwd", "")).resolve() == expected_cwd.resolve():
                return state, header
        time.sleep(0.1)
    fail(
        f"Pi did not finish switching to {expected_cwd}; previous={previous_file}; "
        f"last state={last_state}; last header={last_header}; "
        f"first events={client.events[:30]}; last events={client.events[-20:]}; "
        f"stderr={client.stderr}"
    )
    raise AssertionError("unreachable")


def main() -> None:
    pi_bin = shutil.which("pi")
    if not pi_bin:
        if os.environ.get("TREEWORK_REQUIRE_PI") == "1":
            fail("Pi executable is required but unavailable")
        print("skip: Pi executable unavailable; workspace-switch integration was not run")
        return

    with tempfile.TemporaryDirectory(prefix="treework-pi-switch-") as temp_name:
        temp = Path(temp_name)
        control = temp / "control"
        branch = prepare_treework(control, temp / "treework-build")
        source_cwd = control / "src"
        session_dir = temp / "sessions"
        session_dir.mkdir()
        source_file = session_dir / "source.jsonl"
        write_source_session(source_file, source_cwd)
        env = os.environ.copy()
        env["PI_OFFLINE"] = "1"
        env["PI_CODING_AGENT_DIR"] = str(temp / "pi-agent")
        process = subprocess.Popen(
            [
                pi_bin,
                "--mode",
                "rpc",
                "--session-dir",
                str(session_dir),
                "--session",
                str(source_file),
                "--no-skills",
                "--no-extensions",
                "-e",
                str(EXTENSION),
            ],
            cwd=source_cwd,
            env=env,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=1,
        )
        client = RpcClient(process)
        try:
            commands = client.request({"type": "get_commands"})
            names = {item.get("name") for item in commands.get("data", {}).get("commands", [])}
            if not {
                "treework-adapter",
                "treework-enter",
                "treework-return",
            }.issubset(names):
                fail(f"Pi did not register TreeWork commands: {sorted(str(name) for name in names)}")

            marker = "treework-pi-history-marker"
            client.request({"type": "bash", "command": f"printf '{marker}\\n'"})
            initial = client.request({"type": "get_state"})["data"]["sessionFile"]
            if not Path(initial).is_file():
                fail(f"Pi did not persist the source session: {initial}")

            client.request(
                {"type": "prompt", "message": "/treework-enter adapter --no-resume"},
                timeout=60,
            )
            branch_state, branch_header = wait_for_session(client, branch, initial, timeout=60)
            if Path(branch_header.get("parentSession", "")).resolve() != Path(initial).resolve():
                fail("Pi branch session did not retain the source session as its parent")
            branch_messages = client.request({"type": "get_messages"})
            if marker not in json.dumps(branch_messages, ensure_ascii=False):
                fail("Pi branch session did not preserve conversation history")

            adapter = branch_record(control, "adapter")
            if adapter["status"] != "in_progress":
                fail(f"deferred Pi Enter did not commit the branch transition: {adapter}")

            client.request(
                {"type": "prompt", "message": "/treework-return --no-resume"},
                timeout=60,
            )
            control_state, control_header = wait_for_session(
                client, control, branch_state["sessionFile"]
            )
            if Path(control_header.get("parentSession", "")).resolve() != Path(
                branch_state["sessionFile"]
            ).resolve():
                fail("Pi return session did not retain the branch session as its parent")
            returned_messages = client.request({"type": "get_messages"})
            if marker not in json.dumps(returned_messages, ensure_ascii=False):
                fail("Pi return session did not preserve conversation history")
        finally:
            if process.stdin:
                process.stdin.close()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.terminate()
                process.wait(timeout=10)
        if process.returncode not in (0, None):
            fail(f"Pi RPC process exited {process.returncode}: {client.stderr}")

        tw_env = os.environ.copy()
        tw_env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
        tw_env["TREEWORK_BUILD_DIR"] = str(temp / "treework-build")
        run([str(TW), "pause", "--reason", "prepare cancellation test"], control, tw_env)
        events_path = control / ".TreeWork" / "events.jsonl"
        events_before = len(events_path.read_text(encoding="utf-8").splitlines())
        cancel_extension = temp / "cancel-switch.ts"
        cancel_extension.write_text(
            'export default function (pi) { pi.on("session_before_switch", () => ({ cancel: true })); }\n',
            encoding="utf-8",
        )
        cancel_sessions = temp / "cancel-sessions"
        cancel_sessions.mkdir()
        cancel_source = cancel_sessions / "source.jsonl"
        write_source_session(cancel_source, control)
        cancel_process = subprocess.Popen(
            [
                pi_bin,
                "--mode",
                "rpc",
                "--session-dir",
                str(cancel_sessions),
                "--session",
                str(cancel_source),
                "--no-skills",
                "--no-extensions",
                "-e",
                str(EXTENSION),
                "-e",
                str(cancel_extension),
            ],
            cwd=control,
            env=env,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=1,
        )
        cancel_client = RpcClient(cancel_process)
        try:
            marker = "treework-pi-cancel-marker"
            cancel_client.request({"type": "bash", "command": f"printf '{marker}\\n'"})
            cancel_client.request(
                {"type": "prompt", "message": "/treework-enter adapter --no-resume"},
                timeout=60,
            )
            deadline = time.monotonic() + 60
            cancelled_branch: dict[str, Any] = {}
            while time.monotonic() < deadline:
                cancelled_branch = branch_record(control, "adapter")
                event_count = len(events_path.read_text(encoding="utf-8").splitlines())
                if (
                    event_count >= events_before + 2
                    and cancelled_branch.get("status") == "paused"
                    and "Pi workspace handoff failed" in cancelled_branch.get("status_reason", "")
                ):
                    break
                time.sleep(0.1)
            else:
                fail(
                    "cancelled Pi switch did not recover Enter to paused state: "
                    f"branch={cancelled_branch} events={event_count - events_before}"
                )
            state = cancel_client.request({"type": "get_state"})["data"]
            if Path(state["sessionFile"]).resolve() != cancel_source.resolve():
                fail("cancelled TreeWork switch unexpectedly replaced the source Pi session")
            session_files = sorted(cancel_sessions.glob("*.jsonl"))
            if session_files != [cancel_source]:
                fail(f"cancelled TreeWork switch left an orphan Pi session: {session_files}")
        finally:
            if cancel_process.stdin:
                cancel_process.stdin.close()
            try:
                cancel_process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                cancel_process.terminate()
                cancel_process.wait(timeout=10)
        if cancel_process.returncode not in (0, None):
            fail(f"Pi cancellation RPC process exited {cancel_process.returncode}: {cancel_client.stderr}")

    print(
        "ok: Pi conversation round-trips through a TreeWork managed worktree and "
        "cancelled handoffs recover to paused"
    )


if __name__ == "__main__":
    main()
