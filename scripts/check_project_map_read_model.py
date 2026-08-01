#!/usr/bin/env python3
"""Runtime smoke for the coherent Project Map read model and server."""

from __future__ import annotations

import http.client
import json
import os
import re
import selectors
import shutil
import socket
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any, Callable

from _paths import PLUGIN_ROOT

TW = PLUGIN_ROOT / "skills" / "treework" / "scripts" / "tw"
SERVER_URL = re.compile(r"http://127\.0\.0\.1:(\d+)/project-map\.html")


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def environment(build_dir: Path) -> dict[str, str]:
    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    env["TREEWORK_BUILD_DIR"] = str(build_dir)
    return env


def run_tw(
    workspace: Path,
    build_dir: Path,
    *args: str,
    expect_ok: bool = True,
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        [str(TW), *args],
        cwd=workspace,
        env=environment(build_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=180,
        check=False,
    )
    if expect_ok and result.returncode != 0:
        fail(
            f"tw {' '.join(args)} failed with {result.returncode}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    if not expect_ok and result.returncode == 0:
        fail(f"tw {' '.join(args)} unexpectedly succeeded")
    return result


def start_server(
    workspace: Path, build_dir: Path
) -> tuple[subprocess.Popen[str], int]:
    process = subprocess.Popen(
        [str(TW), "graph", "serve", "--port", "0"],
        cwd=workspace,
        env=environment(build_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdout is not None
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    try:
        if not selector.select(timeout=120):
            stop_server(process)
            fail("graph serve did not report a URL within 120 seconds")
        first_line = process.stdout.readline()
    finally:
        selector.close()
    match = SERVER_URL.search(first_line)
    if not match:
        process.terminate()
        try:
            stdout, stderr = process.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            stdout, stderr = process.communicate(timeout=10)
        fail(
            "graph serve did not print its localhost URL\n"
            f"stdout:\n{first_line}{stdout}\nstderr:\n{stderr}"
        )
    return process, int(match.group(1))


def stop_server(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.communicate(timeout=10)
    except subprocess.TimeoutExpired:
        process.kill()
        process.communicate(timeout=10)


def request(
    port: int,
    method: str,
    path: str,
    body: bytes | None = None,
    headers: dict[str, str] | None = None,
) -> tuple[int, dict[str, str], bytes]:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=10)
    try:
        connection.request(method, path, body=body, headers=headers or {})
        response = connection.getresponse()
        payload = response.read()
        return response.status, dict(response.getheaders()), payload
    finally:
        connection.close()


def request_json(port: int, method: str, path: str) -> tuple[int, dict[str, Any]]:
    status, _, payload = request(port, method, path)
    try:
        value = json.loads(payload)
    except json.JSONDecodeError as error:
        fail(f"{method} {path} returned invalid JSON: {error}\n{payload!r}")
    if not isinstance(value, dict):
        fail(f"{method} {path} did not return a JSON object")
    return status, value


def wait_for_json(
    port: int,
    path: str,
    predicate: Callable[[dict[str, Any]], bool],
    description: str,
) -> dict[str, Any]:
    deadline = time.monotonic() + 10
    last_status = 0
    last_value: dict[str, Any] = {}
    while time.monotonic() < deadline:
        last_status, last_value = request_json(port, "GET", path)
        if last_status == 200 and predicate(last_value):
            return last_value
        time.sleep(0.05)
    fail(
        f"timed out waiting for {description}; "
        f"last status={last_status}, body={json.dumps(last_value, sort_keys=True)}"
    )


def state_snapshot(workspace: Path) -> dict[str, bytes]:
    treework = workspace / ".TreeWork"
    paths = [
        treework / "state" / "project.json",
        treework / "state" / "branches.json",
        treework / "state" / "graph.json",
        treework / "events.jsonl",
    ]
    tree = treework / "state" / "tree.json"
    if tree.exists():
        paths.append(tree)
    return {
        str(path.relative_to(workspace)): path.read_bytes()
        for path in paths
    }


def assert_section_shape(detail: dict[str, Any]) -> None:
    expected = {
        "task_plan": {
            "scope",
            "acceptance",
            "local_steps",
            "out_of_scope",
            "dependencies",
            "branch_intake_gate",
        },
        "progress": {"current_reality", "recent_work", "open_issues", "exit_notes"},
        "findings": {
            "decisions",
            "interface_or_contract_effects",
            "risks_and_unknowns",
        },
        "verification": {"status", "evidence", "coverage_gap"},
    }
    for section, keys in expected.items():
        value = detail.get(section)
        if not isinstance(value, dict) or set(value) != keys:
            fail(f"branch detail has an invalid {section} shape: {value!r}")


def wait_for_narrative_invalidation(
    port: int, mutate: Callable[[], None]
) -> dict[str, Any]:
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=12)
    try:
        connection.request("GET", "/api/project-map/events")
        response = connection.getresponse()
        if response.status != 200:
            fail(f"SSE endpoint returned {response.status}")
        content_type = response.getheader("content-type", "")
        if not content_type.startswith("text/event-stream"):
            fail(f"SSE endpoint returned unexpected content type {content_type!r}")
        mutate()

        event_name = ""
        data_lines: list[str] = []
        while True:
            raw_line = response.readline()
            if not raw_line:
                fail("SSE stream closed before narrative invalidation")
            line = raw_line.decode("utf-8").rstrip("\r\n")
            if not line:
                if event_name == "invalidate" and data_lines:
                    value = json.loads("\n".join(data_lines))
                    if "narrative" in value.get("changes", []):
                        return value
                event_name = ""
                data_lines = []
            elif line.startswith("event:"):
                event_name = line.partition(":")[2].strip()
            elif line.startswith("data:"):
                data_lines.append(line.partition(":")[2].strip())
    except (TimeoutError, socket.timeout):
        fail("timed out waiting for SSE narrative invalidation")
    finally:
        connection.close()


def write_accepted_tree(workspace: Path) -> None:
    (workspace / ".TreeWork" / "tree.yaml").write_text(
        """version: 1
tree:
  id: root
  title: Runtime Smoke
  purpose: Validate the production Project Map read model.
  spec: spec.md
  children:
    - id: alpha
      title: Alpha
      purpose: Exercise accepted current-state and Inspector routes.
      spec: branches/alpha/spec.md
""",
        encoding="utf-8",
    )


def check_uninitialized(workspace: Path, build_dir: Path) -> None:
    result = run_tw(
        workspace,
        build_dir,
        "graph",
        "serve",
        "--port",
        "0",
        "--once",
        expect_ok=False,
    )
    if "TreeWork is not initialized" not in result.stderr:
        fail(
            "uninitialized graph serve did not return the TreeWork initialization error\n"
            f"stderr:\n{result.stderr}"
        )
    if (workspace / ".TreeWork.lock").exists():
        fail("uninitialized graph serve left the TreeWork lock behind")


def check_runtime(workspace: Path, build_dir: Path) -> None:
    run_tw(workspace, build_dir, "init")
    before_server = state_snapshot(workspace)
    server, port = start_server(workspace, build_dir)
    try:
        if (workspace / ".TreeWork.lock").exists():
            fail("graph serve retained the TreeWork lock while listening")
        if state_snapshot(workspace) != before_server:
            fail("graph serve compatibility preparation changed accepted state")

        html_status, _, html = request(port, "GET", "/project-map.html")
        if (
            html_status != 200
            or b'id="root"' not in html
            or b"./app.js" not in html
            or b"./styles.css" not in html
        ):
            fail("fresh revision-zero graph serve did not render the production panel")
        if b"treeworkGraph" in html or b"graphology" in html or b"sigma" in html:
            fail("production Project Map HTML still wires the retired graph frontend")

        app_status, app_headers, app = request(port, "GET", "/app.js")
        if app_status != 200 or b"/api/project-map" not in app:
            fail("production Project Map JavaScript was not served")
        if not any(
            key.lower() == "content-type"
            and value.startswith("application/javascript")
            for key, value in app_headers.items()
        ):
            fail(f"Project Map JavaScript has invalid headers: {app_headers!r}")

        css_status, css_headers, css = request(port, "GET", "/styles.css")
        if css_status != 200 or b"./vendor/fonts/" not in css:
            fail("production Project Map CSS or local font wiring was not served")
        if not any(
            key.lower() == "content-type" and value.startswith("text/css")
            for key, value in css_headers.items()
        ):
            fail(f"Project Map CSS has invalid headers: {css_headers!r}")

        font_status, font_headers, font = request(
            port,
            "GET",
            "/vendor/fonts/fraunces-latin-500-normal.woff2",
        )
        if font_status != 200 or not font:
            fail("locally bundled Fraunces font was not served")
        if not any(
            key.lower() == "content-type" and value == "font/woff2"
            for key, value in font_headers.items()
        ):
            fail(f"Project Map font has invalid headers: {font_headers!r}")

        if os.name == "posix":
            output = workspace / ".TreeWork" / "out"
            graph = output / "graph.json"
            outside = workspace.parent / "outside-project-map-assets"
            outside.mkdir()
            sentinel = b"treework-post-start-outside-sentinel"
            outside_graph = outside / "graph.json"
            outside_graph.write_bytes(sentinel)
            before_swaps = state_snapshot(workspace)

            graph_body = graph.read_bytes()
            graph.unlink()
            graph.symlink_to(outside_graph)
            try:
                for path in ["/api/graph", "/graph.json"]:
                    status, _, payload = request(port, "GET", path)
                    if status == 200 or sentinel in payload:
                        fail(
                            f"post-start graph symlink was served by {path}: "
                            f"status={status} payload={payload!r}"
                        )
            finally:
                graph.unlink()
                graph.write_bytes(graph_body)

            parked_output = workspace / ".TreeWork" / "out.started-root"
            for name in ["project-map.html", "app.js", "styles.css"]:
                (outside / name).write_bytes(sentinel)
            output.rename(parked_output)
            output.symlink_to(outside, target_is_directory=True)
            try:
                for path in [
                    "/project-map.html",
                    "/app.js",
                    "/styles.css",
                    "/api/graph",
                    "/graph.json",
                ]:
                    status, _, payload = request(port, "GET", path)
                    if status == 200 or sentinel in payload:
                        fail(
                            f"post-start output-root symlink was served by {path}: "
                            f"status={status} payload={payload!r}"
                        )
            finally:
                output.unlink()
                parked_output.rename(output)

            if outside_graph.read_bytes() != sentinel:
                fail("post-start containment check modified the external sentinel")
            if state_snapshot(workspace) != before_swaps:
                fail("post-start containment check changed accepted state")

        current_status, current = request_json(port, "GET", "/api/project-map")
        if current_status != 200:
            fail(f"revision-zero current endpoint returned {current_status}")
        if (
            current.get("tree_revision") != 0
            or current.get("project", {}).get("topology_source") != "bootstrap"
            or [node.get("id") for node in current.get("nodes", [])] != ["root"]
            or current.get("dependencies") != []
        ):
            fail(f"revision-zero projection is invalid: {json.dumps(current, sort_keys=True)}")
        first_narrative_revision = current.get("narrative_revision")

        branch_status, root_detail = request_json(
            port, "GET", "/api/project-map/branch?id=root"
        )
        if branch_status != 200 or root_detail.get("branch", {}).get("id") != "root":
            fail(f"root branch detail is invalid: {root_detail!r}")
        assert_section_shape(root_detail)

        genesis_status, genesis = request_json(
            port, "GET", "/api/project-map/replay"
        )
        if (
            genesis_status != 200
            or genesis.get("reconstruction", {}).get("status") != "available"
            or genesis.get("meta", {}).get("checkpoint_event_seq") != 1
            or [
                node.get("id")
                for node in (genesis.get("state") or {}).get("nodes", [])
            ]
            != ["root"]
            or [item.get("seq") for item in genesis.get("transactions", [])] != [1]
        ):
            fail(f"genesis Replay projection is invalid: {genesis!r}")

        progress = workspace / ".TreeWork" / "progress.md"

        def mutate_progress() -> None:
            progress.write_text(
                progress.read_text(encoding="utf-8")
                + "\nProject Map SSE narrative smoke.\n",
                encoding="utf-8",
            )

        invalidation = wait_for_narrative_invalidation(port, mutate_progress)
        if invalidation.get("changes") != ["narrative"]:
            fail(f"unexpected SSE invalidation categories: {invalidation!r}")
        if invalidation.get("narrative_revision") == first_narrative_revision:
            fail("SSE narrative invalidation did not advance narrative_revision")
        wait_for_json(
            port,
            "/api/project-map/branch?id=root",
            lambda value: value.get("narrative_revision")
            == invalidation.get("narrative_revision"),
            "root Inspector narrative refresh",
        )

        before_rejected_requests = state_snapshot(workspace)
        post_status, post_body = request_json(port, "POST", "/api/project-map")
        if post_status != 405 or post_body.get("ok") is not False:
            fail(f"POST current endpoint was not rejected: {post_status} {post_body!r}")
        head_status, _, _ = request(port, "HEAD", "/api/project-map")
        if head_status != 405:
            fail(f"HEAD current endpoint was not rejected: {head_status}")
        foreign_host_status, _, _ = request(
            port,
            "GET",
            "/api/project-map",
            headers={"Host": "treework.example"},
        )
        if foreign_host_status != 403:
            fail(
                "foreign Host current endpoint was not rejected: "
                f"{foreign_host_status}"
            )
        foreign_origin_status, _, _ = request(
            port,
            "GET",
            "/api/project-map",
            headers={"Origin": "https://treework.example"},
        )
        if foreign_origin_status != 403:
            fail(
                "foreign Origin current endpoint was not rejected: "
                f"{foreign_origin_status}"
            )
        local_origin_status, _, _ = request(
            port,
            "GET",
            "/api/project-map",
            headers={
                "Host": f"localhost:{port}",
                "Origin": f"http://localhost:{port}",
            },
        )
        if local_origin_status != 200:
            fail(f"localhost Origin current endpoint returned {local_origin_status}")
        if state_snapshot(workspace) != before_rejected_requests:
            fail("rejected Project Map requests changed accepted state")

        run_tw(workspace, build_dir, "align", "end")
        run_tw(workspace, build_dir, "tree", "start")
        write_accepted_tree(workspace)
        run_tw(workspace, build_dir, "tree", "apply")

        accepted = wait_for_json(
            port,
            "/api/project-map",
            lambda value: value.get("tree_revision") == 1
            and value.get("project", {}).get("topology_source") == "accepted",
            "accepted revision-one projection",
        )
        if [node.get("id") for node in accepted.get("nodes", [])] != ["root", "alpha"]:
            fail(f"accepted current endpoint has unexpected nodes: {accepted!r}")
        alpha = next(node for node in accepted["nodes"] if node["id"] == "alpha")
        if alpha.get("readiness") != "ready":
            fail(f"accepted dependency readiness is invalid: {alpha!r}")

        alpha_status, alpha_detail = request_json(
            port, "GET", "/api/project-map/branch?id=alpha"
        )
        if alpha_status != 200 or alpha_detail.get("branch", {}).get("id") != "alpha":
            fail(f"alpha branch detail is invalid: {alpha_detail!r}")
        assert_section_shape(alpha_detail)

        accepted_marker = json.loads(
            (workspace / ".TreeWork" / "state" / "project.json").read_text(
                encoding="utf-8"
            )
        )
        apply_seq = accepted_marker["last_event_seq"]
        replay_status, replay = request_json(
            port, "GET", f"/api/project-map/replay?at={apply_seq}"
        )
        if (
            replay_status != 200
            or replay.get("reconstruction", {}).get("status") != "available"
            or replay.get("meta", {}).get("checkpoint_event_seq") != apply_seq
            or [
                node.get("id")
                for node in (replay.get("state") or {}).get("nodes", [])
            ]
            != ["root", "alpha"]
        ):
            fail(f"accepted Replay projection is invalid: {replay!r}")

        filtered_status, filtered = request_json(
            port, "GET", f"/api/project-map/replay?at={apply_seq}&branch=alpha"
        )
        if (
            filtered_status != 200
            or len((filtered.get("state") or {}).get("nodes", [])) != 2
            or [item.get("type") for item in filtered.get("transactions", [])]
            != ["tree.applied"]
        ):
            fail(f"branch-filtered Replay changed global state: {filtered!r}")

        empty_status, empty_timeline = request_json(
            port,
            "GET",
            f"/api/project-map/replay?at={apply_seq}&after={apply_seq}",
        )
        if empty_status != 200 or empty_timeline.get("transactions") != []:
            fail(f"exclusive Replay `after` is invalid: {empty_timeline!r}")

        invalid_status, invalid = request_json(
            port, "GET", f"/api/project-map/replay?at={apply_seq + 1}"
        )
        if invalid_status != 400 or invalid.get("ok") is not False:
            fail(f"invalid Replay range was not rejected: {invalid_status} {invalid!r}")
        missing_status, missing = request_json(
            port, "GET", "/api/project-map/replay?branch=missing"
        )
        if missing_status != 404 or missing.get("ok") is not False:
            fail(f"unknown Replay branch was not rejected: {missing_status} {missing!r}")

        run_tw(workspace, build_dir, "tree", "update")
        run_tw(workspace, build_dir, "tree", "apply")
        no_change_marker = json.loads(
            (workspace / ".TreeWork" / "state" / "project.json").read_text(
                encoding="utf-8"
            )
        )
        no_change_seq = no_change_marker["last_event_seq"]
        no_change_status, no_change = request_json(
            port,
            "GET",
            f"/api/project-map/replay?at={no_change_seq}&after={apply_seq}",
        )
        transactions = no_change.get("transactions", [])
        no_change_apply = transactions[-1] if transactions else {}
        if (
            no_change_status != 200
            or no_change.get("reconstruction", {}).get("status") != "available"
            or no_change.get("meta", {}).get("checkpoint_event_seq") != no_change_seq
            or no_change_apply.get("type") != "tree.applied"
            or no_change_apply.get("replayable") is not True
            or no_change_apply.get("changes", {})
            .get("result", {})
            .get("topology_changed")
            is not False
        ):
            fail(f"no-change Apply is not a Replay transaction: {no_change!r}")
    finally:
        stop_server(server)


def main() -> None:
    temp_root = Path(tempfile.mkdtemp(prefix="treework-project-map-read-model-"))
    build_dir = temp_root / ".build"
    uninitialized = temp_root / "uninitialized"
    workspace = temp_root / "workspace"
    uninitialized.mkdir()
    workspace.mkdir()
    try:
        run_tw(uninitialized, build_dir, "--version")
        check_uninitialized(uninitialized, build_dir)
        check_runtime(workspace, build_dir)
        print("ok: coherent Project Map read model runtime")
    finally:
        shutil.rmtree(temp_root, ignore_errors=True)


if __name__ == "__main__":
    main()
