#!/usr/bin/env python3
"""Regression checks for the current TreeWork CLI contract."""

from __future__ import annotations

import http.client
import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

from _paths import PLUGIN_ROOT

TW = PLUGIN_ROOT / "skills" / "treework" / "scripts" / "tw"


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def run(
    workspace: Path,
    build_dir: Path,
    *args: str,
    expect_ok: bool = True,
    extra_env: dict[str, str] | None = None,
) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    env["TREEWORK_BUILD_DIR"] = str(build_dir)
    if extra_env:
        env.update(extra_env)
    result = subprocess.run(
        [str(TW), *args],
        cwd=workspace,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
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


def load_json(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def project_state(workspace: Path) -> dict:
    return load_json(workspace / ".TreeWork" / "state" / "project.json")


def branch_state(workspace: Path, branch: str) -> dict:
    for item in load_json(workspace / ".TreeWork" / "state" / "branches.json")["branches"]:
        if item["path"] == branch:
            return item
    fail(f"missing branch state for {branch}")


def branch_paths(workspace: Path) -> set[str]:
    return {
        item["path"]
        for item in load_json(workspace / ".TreeWork" / "state" / "branches.json")["branches"]
    }


def graph_state(workspace: Path) -> dict:
    return load_json(workspace / ".TreeWork" / "state" / "graph.json")


def graph_projection(workspace: Path) -> dict:
    return load_json(workspace / ".TreeWork" / "out" / "graph.json")


def event_records(workspace: Path) -> list[dict]:
    path = workspace / ".TreeWork" / "events.jsonl"
    return [
        json.loads(line)
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]


def accepted_state_snapshot(workspace: Path) -> dict[str, bytes]:
    treework = workspace / ".TreeWork"
    snapshot: dict[str, bytes] = {}
    for root in [
        treework / "state",
        treework / "events.jsonl",
        treework / "history" / "checkpoints",
    ]:
        if root.is_file():
            snapshot[root.relative_to(treework).as_posix()] = root.read_bytes()
        elif root.is_dir():
            for path in sorted(item for item in root.rglob("*") if item.is_file()):
                snapshot[path.relative_to(treework).as_posix()] = path.read_bytes()
    return snapshot


def assert_typed_event(event: dict, event_type: str) -> None:
    if event.get("schema_version") != 1:
        fail(f"{event_type} did not use schema_version 1")
    if event.get("type") != event_type:
        fail(f"expected {event_type}, found {event.get('type')}")
    if not isinstance(event.get("tree_revision"), int) or not isinstance(event.get("data"), dict):
        fail(f"{event_type} is missing typed envelope fields")


def assert_no_pending_transaction(workspace: Path) -> None:
    if (workspace / ".TreeWork" / "state" / "pending-transaction.json").exists():
        fail("completed command left a transaction journal")
    if (workspace / ".TreeWork.pending-transaction-backup").exists():
        fail("completed command left a transaction backup")


def intended_control_manifest(journal: dict) -> set[str]:
    roots = journal.get("intended", {}).get("roots", [])
    control = next(
        (root for root in roots if root.get("relative_path") == ".TreeWork"),
        None,
    )
    if control is None or control.get("state", {}).get("kind") != "directory":
        fail("transaction intent has no complete .TreeWork directory result")
    paths = {
        entry["relative_path"]
        for entry in control["state"].get("entries", [])
    }
    forbidden = {
        path
        for path in paths
        if path == "out"
        or path.startswith("out/")
        or path == "state/project.json"
        or path == "state/project.tmp"
        or path.startswith("state/pending-transaction")
    }
    if forbidden:
        fail(f"transaction intent included excluded publication paths: {sorted(forbidden)}")
    return paths


def assert_full_init_scaffold(workspace: Path) -> None:
    tw_dir = workspace / ".TreeWork"
    required_files = [
        "PROJECT.md",
        "tree.yaml",
        "requirements.md",
        "assumptions.md",
        "references.md",
        "idea_inbox.md",
        "spec.md",
        "task_plan.md",
        "progress.md",
        "findings.md",
        "events.jsonl",
        "state/project.json",
        "state/branches.json",
        "state/graph.json",
    ]
    missing = [path for path in required_files if not (tw_dir / path).is_file()]
    if missing:
        fail(f"initialized publication is missing scaffold files: {missing}")
    if (tw_dir / "design.md").exists():
        fail("fresh init still created the deprecated design.md compatibility file")
    if (tw_dir / "open_questions.md").exists():
        fail("fresh init still created the retired open_questions.md file")
    for path in ["state", "branches", "out", "history/checkpoints"]:
        if not (tw_dir / path).is_dir():
            fail(f"initialized publication is missing scaffold directory {path}")
    initialized = event_records(workspace)
    snapshot_ref = initialized[-1].get("data", {}).get("snapshot_ref")
    if not snapshot_ref or not (tw_dir / snapshot_ref).is_file():
        fail("initialized publication is missing its genesis checkpoint")


def assert_branch_documents(workspace: Path, branch: str, custom_spec: str) -> None:
    branch_dir = workspace / ".TreeWork" / "branches" / branch
    missing = [
        name
        for name in [
            "task_plan.md",
            "progress.md",
            "findings.md",
            "verification.md",
        ]
        if not (branch_dir / name).is_file()
    ]
    if missing or not (workspace / ".TreeWork" / custom_spec).is_file():
        fail(
            f"published branch {branch} is missing documents {missing} "
            f"or custom Spec {custom_spec}"
        )


def write_tree(workspace: Path, source: str) -> None:
    path = workspace / ".TreeWork" / "tree.yaml"
    path.write_text(source.rstrip() + "\n", encoding="utf-8")


def complete_acceptance(workspace: Path, branch: str) -> None:
    path = workspace / ".TreeWork" / "branches" / branch / "task_plan.md"
    content = path.read_text(encoding="utf-8")
    if "- [ ]" not in content:
        fail(f"{branch} task plan has no acceptance checkbox")
    path.write_text(content.replace("- [ ]", "- [x]", 1), encoding="utf-8")


def server_request(
    workspace: Path,
    build_dir: Path,
    method: str,
    path: str,
    body: dict | None = None,
) -> tuple[int, str]:
    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    env["TREEWORK_BUILD_DIR"] = str(build_dir)
    proc = subprocess.Popen(
        [str(TW), "graph", "serve", "--port", "0", "--once"],
        cwd=workspace,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert proc.stdout is not None
    first = proc.stdout.readline()
    match = re.search(r"http://127\.0\.0\.1:(\d+)/project-map\.html", first)
    if not match:
        stdout, stderr = proc.communicate(timeout=10)
        fail(f"graph serve did not print a URL\nstdout:{first}{stdout}\nstderr:{stderr}")
    payload = json.dumps(body).encode() if body is not None else None
    headers = {"content-type": "application/json"} if body is not None else {}
    connection = http.client.HTTPConnection("127.0.0.1", int(match.group(1)), timeout=10)
    try:
        connection.request(method, path, body=payload, headers=headers)
        response = connection.getresponse()
        text = response.read().decode()
        status = response.status
    finally:
        connection.close()
    stdout, stderr = proc.communicate(timeout=10)
    if proc.returncode != 0:
        fail(f"graph serve failed\nstdout:{first}{stdout}\nstderr:{stderr}")
    return status, text


def assert_removed_commands(workspace: Path, build_dir: Path) -> None:
    help_text = run(workspace, build_dir, "--help").stdout
    for hidden in ["check", "sync", "graph"]:
        if re.search(rf"(?m)^\s+{re.escape(hidden)}\s", help_text):
            fail(f"internal command `{hidden}` remains in top-level help")
    for command in [
        "status",
        "context",
        "branch",
        "edge",
        "note",
        "block",
        "resume",
        "log",
        "replay",
        "vendor",
    ]:
        if re.search(rf"(?m)^\s+{re.escape(command)}\s", help_text):
            fail(f"removed command `{command}` remains in top-level help")
        result = run(workspace, build_dir, command, expect_ok=False)
        if "unrecognized subcommand" not in result.stderr:
            fail(f"removed command `{command}` did not fail as unrecognized")
    for command in ["next", "floor", "workspace", "runner", "team", "worker"]:
        result = run(workspace, build_dir, command, expect_ok=False)
        if "unrecognized subcommand" not in result.stderr:
            fail(f"retired command `{command}` did not fail as unrecognized")
    align_help = run(workspace, build_dir, "align", "--help").stdout
    if not re.search(r"(?m)^\s+end\s", align_help):
        fail("public align end is missing from Agent help")
    removed_accept = run(workspace, build_dir, "align", "accept", expect_ok=False)
    if "unrecognized subcommand" not in removed_accept.stderr:
        fail("retired align accept remains callable")


def check_alignment_workflow(temp_root: Path, build_dir: Path) -> None:
    workspace = temp_root / "alignment-workflow"
    workspace.mkdir()
    run(workspace, build_dir, "init")

    blocked_start = run(workspace, build_dir, "tree", "start", expect_ok=False)
    if "tw align end" not in blocked_start.stderr:
        fail("tree start did not explain the required Alignment Review boundary")

    run(workspace, build_dir, "align", "end")
    first_end = project_state(workspace)
    first_end_event = event_records(workspace)[-1]
    assert_typed_event(first_end_event, "alignment.accepted")
    if (
        first_end["stage"] != "build_tree"
        or first_end_event["data"]["stage"]["after"] != "build_tree"
    ):
        fail("first align end did not route a project without an accepted Tree to build_tree")
    pending_tree_check = run(workspace, build_dir, "check", "--brief")
    if "0 issue(s)" not in pending_tree_check.stdout:
        fail("first Tree pending state produced a false-positive check issue")

    before_repeated_end = (
        project_state(workspace),
        (workspace / ".TreeWork" / "events.jsonl").read_bytes(),
    )
    run(workspace, build_dir, "align", "end")
    after_repeated_end = (
        project_state(workspace),
        (workspace / ".TreeWork" / "events.jsonl").read_bytes(),
    )
    if after_repeated_end != before_repeated_end:
        fail("repeated align end outside Alignment changed state or emitted an event")

    run(workspace, build_dir, "tree", "start")
    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Alignment Workflow
  purpose: Exercise initial and returning Alignment.
  spec: spec.md
  children:
    - id: implementation
      title: Implementation
      purpose: Provide an accepted Tree for Alignment re-entry.
      spec: branches/implementation/spec.md
""",
    )
    first_apply = run(workspace, build_dir, "tree", "apply")
    if (
        "First Tree accepted" not in first_apply.stdout
        or "treework_project_map" not in first_apply.stdout
        or "Codex in-app browser" not in first_apply.stdout
    ):
        fail("first Tree apply did not emit the Project Map handoff")
    if project_state(workspace)["stage"] != "work_tree":
        fail("first Tree apply did not enter work_tree")

    project_path = workspace / ".TreeWork" / "state" / "project.json"
    accepted_project_bytes = project_path.read_bytes()
    malformed_project = json.loads(accepted_project_bytes)
    malformed_project["stage"] = "build_tree"
    malformed_project["tree_editing"] = None
    project_path.write_text(
        json.dumps(malformed_project, indent=2) + "\n",
        encoding="utf-8",
    )
    malformed_check = run(workspace, build_dir, "check", "--brief")
    if "stage is `build_tree` but no Tree Editing Session is open" not in malformed_check.stdout:
        fail("accepted-Tree build_tree state without an editing session was not diagnosed")
    project_path.write_bytes(accepted_project_bytes)

    run(workspace, build_dir, "align", "start")
    if project_state(workspace)["stage"] != "alignment":
        fail("align start did not reopen Alignment for an accepted Tree")
    blocked_update = run(workspace, build_dir, "tree", "update", expect_ok=False)
    if "requires stage `work_tree`" not in blocked_update.stderr:
        fail("Tree update did not remain closed while Alignment was active")

    run(workspace, build_dir, "align", "end")
    returning_end = project_state(workspace)
    returning_event = event_records(workspace)[-1]
    assert_typed_event(returning_event, "alignment.accepted")
    if (
        returning_end["stage"] != "work_tree"
        or returning_end["tree_editing"] is not None
        or returning_event["data"]["stage"]["after"] != "work_tree"
    ):
        fail("align end did not return an existing accepted Tree to work_tree")

    run(workspace, build_dir, "tree", "update")
    blocked_alignment = run(workspace, build_dir, "align", "start", expect_ok=False)
    if "Tree Editing Session is open" not in blocked_alignment.stderr:
        fail("align start did not protect an open Tree Editing Session")
    run(workspace, build_dir, "tree", "apply")
    if project_state(workspace)["stage"] != "work_tree":
        fail("Tree update after returning Alignment did not complete")


def check_graph_output_symlink_safety(temp_root: Path, build_dir: Path) -> None:
    workspace = temp_root / "graph-output-symlink"
    workspace.mkdir()
    run(workspace, build_dir, "init")
    treework = workspace / ".TreeWork"
    output = treework / "out"
    accepted_before = accepted_state_snapshot(workspace)

    shutil.rmtree(output)
    outside_output = temp_root / "outside-project-map-output"
    outside_output.mkdir()
    output_sentinel = outside_output / "sentinel.txt"
    output_sentinel.write_text("preserve output\n", encoding="utf-8")
    output.symlink_to(outside_output, target_is_directory=True)
    rejected_root = run(
        workspace,
        build_dir,
        "graph",
        "render",
        expect_ok=False,
    )
    if "refuses symlinked" not in rejected_root.stderr:
        fail(f"graph render returned an unclear output-root error: {rejected_root.stderr}")
    if (
        output_sentinel.read_text(encoding="utf-8") != "preserve output\n"
        or (outside_output / "graph.json").exists()
        or (outside_output / "project-map.html").exists()
    ):
        fail("graph render wrote through a symlinked output root")
    output.unlink()
    output.mkdir()

    outside_vendor = temp_root / "outside-project-map-vendor"
    outside_vendor.mkdir()
    vendor_sentinel = outside_vendor / "sentinel.txt"
    vendor_sentinel.write_text("preserve vendor\n", encoding="utf-8")
    (output / "vendor").symlink_to(outside_vendor, target_is_directory=True)
    rejected_vendor = run(
        workspace,
        build_dir,
        "graph",
        "render",
        expect_ok=False,
    )
    if "refuses symlinked" not in rejected_vendor.stderr:
        fail(f"graph render returned an unclear asset-path error: {rejected_vendor.stderr}")
    if vendor_sentinel.read_text(encoding="utf-8") != "preserve vendor\n":
        fail("graph render deleted through a symlinked asset path")
    if accepted_state_snapshot(workspace) != accepted_before:
        fail("rejected Project Map output changed accepted TreeWork state")


def check_primary_flow(temp_root: Path, build_dir: Path) -> None:
    workspace = temp_root / "workspace"
    workspace.mkdir()
    assert_removed_commands(workspace, build_dir)

    run(workspace, build_dir, "init")
    assert_full_init_scaffold(workspace)
    tw_dir = workspace / ".TreeWork"
    for required in [
        "PROJECT.md",
        "tree.yaml",
        "spec.md",
        "task_plan.md",
        "progress.md",
        "findings.md",
    ]:
        if not (tw_dir / required).is_file():
            fail(f"tw init did not create {required}")
    for removed in ["tree.md", "graph.md", "state/notes.json"]:
        if (tw_dir / removed).exists():
            fail(f"tw init still created removed artifact {removed}")
    initialized = event_records(workspace)
    if len(initialized) != 1:
        fail("fresh init did not publish exactly one event")
    assert_typed_event(initialized[0], "project.initialized")
    genesis_ref = initialized[0]["data"].get("snapshot_ref")
    if genesis_ref != "history/checkpoints/tree-r000000-e000001.json":
        fail("fresh init did not reference the required genesis checkpoint")
    genesis = load_json(tw_dir / genesis_ref)
    if (
        genesis.get("event_seq") != 1
        or genesis.get("tree_revision") != 0
        or genesis.get("tree") is not None
        or genesis.get("checkpoint_hash") != initialized[0]["data"].get("checkpoint_hash")
    ):
        fail("genesis checkpoint does not match project.initialized")
    before_repeat_init = (
        project_state(workspace),
        (tw_dir / "events.jsonl").read_bytes(),
        (tw_dir / genesis_ref).read_bytes(),
    )
    run(workspace, build_dir, "init")
    after_repeat_init = (
        project_state(workspace),
        (tw_dir / "events.jsonl").read_bytes(),
        (tw_dir / genesis_ref).read_bytes(),
    )
    if after_repeat_init != before_repeat_init:
        fail("repeated init was not a state-preserving no-op")

    run(workspace, build_dir, "align", "end")
    assert_typed_event(event_records(workspace)[-1], "alignment.accepted")
    run(workspace, build_dir, "tree", "start")
    assert_typed_event(event_records(workspace)[-1], "tree.editing_started")
    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Test Project
  purpose: Project-wide coordination and integration.
  spec: spec.md
  children:
    - id: alpha
      title: Alpha
      purpose: Exercise completion and recovery.
      spec: branches/alpha/spec.md
    - id: beta
      title: Beta
      purpose: Exercise abort behavior.
      depends_on:
        - alpha
""",
    )
    run(workspace, build_dir, "tree", "apply")
    applied_event = event_records(workspace)[-1]
    assert_typed_event(applied_event, "tree.applied")
    applied_checkpoint = load_json(tw_dir / applied_event["data"]["snapshot_ref"])
    if applied_checkpoint.get("checkpoint_hash") != applied_event["data"].get("checkpoint_hash"):
        fail("tree.applied checkpoint hash did not match its event")
    if project_state(workspace)["tree_revision"] != 1:
        fail("first declarative Tree apply did not create revision 1")
    if branch_paths(workspace) != {"root", "alpha", "beta"}:
        fail("first declarative Tree apply produced the wrong branch set")
    accepted_tree = load_json(tw_dir / "state" / "tree.json")
    if accepted_tree["revision"] != 1 or accepted_tree["root"] != "root":
        fail("accepted state/tree.json does not describe revision 1")
    beta_node = next(node for node in accepted_tree["nodes"] if node["id"] == "beta")
    if beta_node.get("depends_on") != ["alpha"]:
        fail("accepted Tree did not preserve depends_on")
    if not (tw_dir / "branches" / "alpha" / "spec.md").is_file():
        fail("Tree apply did not scaffold the referenced branch Spec")

    alpha_plan = (tw_dir / "branches" / "alpha" / "task_plan.md").read_text(encoding="utf-8")
    alpha_progress = (tw_dir / "branches" / "alpha" / "progress.md").read_text(encoding="utf-8")
    alpha_findings = (tw_dir / "branches" / "alpha" / "findings.md").read_text(encoding="utf-8")
    if "## Branch Intake Gate (" not in alpha_plan:
        fail("branch task plan lacks its intake heading explanation")
    if "## Recent Work (latest meaningful progress event" not in alpha_progress:
        fail("branch progress lacks its field explanation")
    if "unfinished work, impediments" not in alpha_progress or "handoff/return context" not in alpha_progress:
        fail("branch progress scaffold still uses the retired blocker/resume wording")
    if "## Decisions (" not in alpha_findings or "Implementation Notes" in alpha_findings:
        fail("branch findings template still uses the old structure")

    run(workspace, build_dir, "enter", "alpha", "--no-isolate")
    assert_typed_event(event_records(workspace)[-1], "branch.entered")
    run(workspace, build_dir, "pause", "--reason", "Waiting for a local decision")
    assert_typed_event(event_records(workspace)[-1], "branch.paused")
    paused = branch_state(workspace, "alpha")
    if paused["status"] != "paused" or paused.get("status_reason") != "Waiting for a local decision":
        fail("pause did not persist the paused state and reason")
    before_reason_change = project_state(workspace)["last_event_seq"]
    run(workspace, build_dir, "pause", "--reason", "Waiting for the revised decision")
    reason_event = event_records(workspace)[-1]
    assert_typed_event(reason_event, "branch.paused")
    if (
        project_state(workspace)["last_event_seq"] != before_reason_change + 1
        or reason_event["data"]["status"] != {"before": "paused", "after": "paused"}
        or reason_event["data"]["reason"]
        != {
            "before": "Waiting for a local decision",
            "after": "Waiting for the revised decision",
        }
    ):
        fail("pause reason change did not publish a paused-to-paused semantic event")
    before_same_reason = durable_treework_snapshot(workspace)
    run(workspace, build_dir, "pause", "--reason", "Waiting for the revised decision")
    if durable_treework_snapshot(workspace) != before_same_reason:
        fail("identical paused status and reason published an artificial event")
    before_recall = project_state(workspace)
    recall = json.loads(run(workspace, build_dir, "recall", "alpha", "--json").stdout)
    blocked_by_action = {
        item["action"]: item for item in recall.get("blocked_actions", [])
    }
    if (
        recall["branch"]["status"] != "paused"
        or recall["branch"].get("status_reason") != "Waiting for the revised decision"
        or recall.get("tree_revision") != 1
        or recall.get("publication_marker", {}).get("last_event_seq")
        != project_state(workspace)["last_event_seq"]
        or "spec" not in recall.get("docs", {})
        or not {"enter", "pause", "abort"}.issubset(
            set(recall.get("allowed_actions", []))
        )
        or "acceptance_incomplete"
        not in blocked_by_action.get("complete", {}).get("reason_codes", [])
        or "verification_not_verified"
        not in blocked_by_action.get("complete", {}).get("reason_codes", [])
        or project_state(workspace) != before_recall
    ):
        fail("recall did not expose a read-only committed action projection")
    run(workspace, build_dir, "enter", "alpha", "--no-isolate")
    assert_typed_event(event_records(workspace)[-1], "branch.entered")
    resumed = branch_state(workspace, "alpha")
    if resumed["status"] != "in_progress" or resumed.get("status_reason"):
        fail("enter did not return a paused branch to in_progress")
    before_repeated_enter = (
        project_state(workspace),
        (tw_dir / "state" / "branches.json").read_bytes(),
        (tw_dir / "events.jsonl").read_bytes(),
    )
    run(workspace, build_dir, "enter", "alpha", "--no-isolate")
    after_repeated_enter = (
        project_state(workspace),
        (tw_dir / "state" / "branches.json").read_bytes(),
        (tw_dir / "events.jsonl").read_bytes(),
    )
    if after_repeated_enter != before_repeated_enter:
        fail("repeated visible no-op enter published an artificial event")

    complete_acceptance(workspace, "alpha")
    run(
        workspace,
        build_dir,
        "verify",
        "--cmd",
        "primary lifecycle smoke",
        "--result",
        "passed",
        "--gap",
        "none",
    )
    assert_typed_event(event_records(workspace)[-1], "verification.recorded")
    before_repeat_verification = (
        project_state(workspace),
        (tw_dir / "state" / "branches.json").read_bytes(),
        (tw_dir / "branches" / "alpha" / "verification.md").read_bytes(),
        (tw_dir / "events.jsonl").read_bytes(),
    )
    run(
        workspace,
        build_dir,
        "verify",
        "--cmd",
        "primary lifecycle smoke",
        "--result",
        "passed",
        "--gap",
        "none",
    )
    after_repeat_verification = (
        project_state(workspace),
        (tw_dir / "state" / "branches.json").read_bytes(),
        (tw_dir / "branches" / "alpha" / "verification.md").read_bytes(),
        (tw_dir / "events.jsonl").read_bytes(),
    )
    if after_repeat_verification != before_repeat_verification:
        fail("equivalent verification published an artificial event")
    ready_recall = json.loads(run(workspace, build_dir, "recall", "alpha", "--json").stdout)
    if "complete" not in ready_recall.get("allowed_actions", []):
        fail("recall did not expose completion after all hard guards passed")
    run(workspace, build_dir, "complete")
    assert_typed_event(event_records(workspace)[-1], "branch.completed")
    if branch_state(workspace, "alpha")["status"] != "complete":
        fail("complete did not close alpha")

    run(workspace, build_dir, "enter", "beta", "--no-isolate")
    run(workspace, build_dir, "abort", "--reason", "No longer part of the accepted design")
    assert_typed_event(event_records(workspace)[-1], "branch.aborted")
    beta = branch_state(workspace, "beta")
    if beta["status"] != "aborted" or beta.get("status_reason") != "No longer part of the accepted design":
        fail("abort did not persist terminal status and reason")
    run(workspace, build_dir, "enter", "beta", "--no-isolate", expect_ok=False)

    before_graph_events = (tw_dir / "events.jsonl").read_bytes()
    run(workspace, build_dir, "graph", "render")
    if (tw_dir / "events.jsonl").read_bytes() != before_graph_events:
        fail("graph render emitted a workflow event")
    projection = graph_projection(workspace)
    if "notes" in projection or "edit_commands" in projection:
        fail("graph projection contains removed note or write-hint fields")
    node_statuses = {node["id"]: node["status"] for node in projection["nodes"]}
    if node_statuses.get("alpha") != "complete" or node_statuses.get("beta") != "aborted":
        fail("graph projection did not expose the accepted five-state lifecycle")

    html_status, html = server_request(workspace, build_dir, "GET", "/project-map.html")
    if (
        html_status != 200
        or 'id="root"' not in html
        or "./app.js" not in html
        or "./styles.css" not in html
    ):
        fail("Project Map server did not serve the production read-only panel")
    if (
        "treeworkWriteApi" in html
        or "/api/transaction" in html
        or "treeworkGraph" in html
        or "graphology" in html
        or "sigma" in html
    ):
        fail("Project Map HTML still exposes a retired or writable frontend")
    before_post = (tw_dir / "state" / "branches.json").read_text(encoding="utf-8")
    post_status, post_body = server_request(
        workspace,
        build_dir,
        "POST",
        "/api/transaction",
        {"action": "branch.move", "branch": "beta", "to": "alpha"},
    )
    if post_status != 405 or json.loads(post_body).get("ok") is not False:
        fail("Project Map mutation endpoint was not rejected")
    if (tw_dir / "state" / "branches.json").read_text(encoding="utf-8") != before_post:
        fail("rejected Project Map write changed branch state")
    before_sync_events = (tw_dir / "events.jsonl").read_bytes()
    run(workspace, build_dir, "sync")
    if (tw_dir / "events.jsonl").read_bytes() != before_sync_events:
        fail("generated-view sync emitted a workflow event")

    run(workspace, build_dir, "tree", "update")
    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Test Project
  purpose: Project-wide coordination and integration.
  spec: spec.md
  children:
    - id: alpha
      title: Alpha
      purpose: Exercise completion and recovery.
      spec: branches/alpha/spec.md
    - id: beta
      title: Beta
      purpose: Exercise abort behavior.
      depends_on:
        - alpha
    - id: gamma
      title: Gamma
      purpose: Exercise atomic apply rollback.
      spec: design/gamma.md
""",
    )
    before_state = project_state(workspace)
    before_events = (tw_dir / "events.jsonl").read_text(encoding="utf-8")
    before_tree = (tw_dir / "state" / "tree.json").read_text(encoding="utf-8")
    run(
        workspace,
        build_dir,
        "tree",
        "apply",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "tree-apply-after-event"},
    )
    if "gamma" in branch_paths(workspace):
        fail("failed tree apply did not roll back branch creation")
    if project_state(workspace) != before_state:
        fail("failed tree apply did not restore project state")
    if (tw_dir / "events.jsonl").read_text(encoding="utf-8") != before_events:
        fail("failed tree apply did not restore events")
    if (tw_dir / "state" / "tree.json").read_text(encoding="utf-8") != before_tree:
        fail("failed tree apply did not restore accepted Tree state")
    if (tw_dir / "design" / "gamma.md").exists():
        fail("failed tree apply left a newly scaffolded custom Spec behind")
    run(workspace, build_dir, "tree", "apply")
    if branch_state(workspace, "gamma")["parent"] != "root":
        fail("successful tree apply did not create gamma")
    if not (tw_dir / "design" / "gamma.md").is_file():
        fail("successful tree apply did not scaffold a custom Spec path")

    unchanged_revision = project_state(workspace)["tree_revision"]
    unchanged_event_seq = project_state(workspace)["last_event_seq"]
    run(workspace, build_dir, "tree", "update")
    unchanged_apply_output = run(workspace, build_dir, "tree", "apply")
    if "treework_project_map" in unchanged_apply_output.stdout:
        fail("later Tree apply repeated the first-Tree Project Map handoff")
    unchanged_project = project_state(workspace)
    unchanged_apply = event_records(workspace)[-1]
    assert_typed_event(unchanged_apply, "tree.applied")
    if (
        unchanged_project["tree_revision"] != unchanged_revision
        or unchanged_project["last_event_seq"] != unchanged_event_seq + 2
        or unchanged_apply["data"]["result"].get("topology_changed") is not False
        or unchanged_apply["data"].get("operations") != []
    ):
        fail("accepted no-change Apply did not remain a transaction at the same Tree revision")
    if not (tw_dir / unchanged_apply["data"]["snapshot_ref"]).is_file():
        fail("accepted no-change Apply did not write its checkpoint")

    run(workspace, build_dir, "tree", "update")
    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Test Project
  purpose: Project-wide coordination and integration.
  spec: spec.md
  children:
    - id: beta
      title: Beta
      purpose: Exercise abort behavior.
      children:
        - id: alpha
          title: Alpha
          purpose: Exercise completion and recovery.
          spec: branches/alpha/spec.md
    - id: gamma
      title: Gamma
      purpose: Exercise atomic apply rollback.
      spec: design/gamma.md
""",
    )
    protected = run(workspace, build_dir, "tree", "apply", expect_ok=False)
    if "protected branch" not in protected.stderr:
        fail("tree apply did not protect terminal branch topology")

    check = run(workspace, build_dir, "check", "--brief")
    if "TreeWork check:" not in check.stdout:
        fail("check did not produce validator output")


def check_legacy_state_read(temp_root: Path, build_dir: Path) -> None:
    workspace = temp_root / "legacy-state"
    workspace.mkdir()
    run(workspace, build_dir, "init")
    path = workspace / ".TreeWork" / "state" / "branches.json"
    state = load_json(path)
    state["branches"][0]["status"] = "blocked"
    state["branches"][0]["blocker"] = "Legacy wait reason"
    path.write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    recall = json.loads(run(workspace, build_dir, "recall", "root", "--json").stdout)
    if recall["branch"]["status"] != "paused" or recall["branch"].get("status_reason") != "Legacy wait reason":
        fail("legacy blocked state did not lazily normalize to paused")

    state["branches"][0]["status"] = "superseded"
    state["branches"][0]["blocker"] = "Legacy retirement"
    path.write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    recall = json.loads(run(workspace, build_dir, "recall", "root", "--json").stdout)
    if recall["branch"]["status"] != "aborted" or recall["branch"].get("status_reason") != "Legacy retirement":
        fail("legacy superseded state did not lazily normalize to aborted")


def check_project_index_state_migration(temp_root: Path, build_dir: Path) -> None:
    workspace = temp_root / "project-index-migration"
    workspace.mkdir()
    run(workspace, build_dir, "init")
    tw_dir = workspace / ".TreeWork"
    project_path = tw_dir / "state" / "project.json"
    project = load_json(project_path)
    project.update(
        {
            "stage": "work_tree",
            "current_branch": "legacy-child",
            "last_event_seq": 17,
            "tree_revision": 0,
            "project_index_hash": "legacy-index-hash",
            "last_sync": "unix:1700000000",
        }
    )
    project.pop("tree_hash", None)
    project_path.write_text(json.dumps(project, indent=2) + "\n", encoding="utf-8")
    legacy_events = event_records(workspace)
    for seq in range(2, 18):
        legacy_events.append(
            {
                "seq": seq,
                "time": f"unix:{1700000000 + seq}",
                "type": "branch.next_updated",
                "subject": "legacy-child",
                "message": f"Legacy event {seq}",
            }
        )
    (tw_dir / "events.jsonl").write_text(
        "".join(json.dumps(event, separators=(",", ":")) + "\n" for event in legacy_events),
        encoding="utf-8",
    )
    branches = {
        "branches": [
            {
                "path": "root",
                "parent": "",
                "title": "Legacy Project",
                "purpose": "Preserve the accepted legacy project.",
                "status": "in_progress",
                "verification_status": "unverified",
                "sync_status": "clean",
                "last_sync": "unix:1700000000",
            },
            {
                "path": "legacy-child",
                "parent": "root",
                "title": "Legacy Child",
                "purpose": "Preserve lifecycle and dependency state.",
                "status": "paused",
                "verification_status": "partial",
                "sync_status": "clean",
                "status_reason": "Migration fixture",
                "last_sync": "unix:1700000000",
            },
        ]
    }
    (tw_dir / "state" / "branches.json").write_text(
        json.dumps(branches, indent=2) + "\n", encoding="utf-8"
    )
    graph = {
        "edges": [
            {
                "id": "edge-1",
                "from": "root",
                "to": "legacy-child",
                "kind": "parent_of",
                "user_label": "root contains legacy-child",
                "interpreted_relation": "parent_of",
            }
        ]
    }
    (tw_dir / "state" / "graph.json").write_text(
        json.dumps(graph, indent=2) + "\n", encoding="utf-8"
    )
    (tw_dir / "state" / "project-index.json").write_text(
        '{"revision":1,"hash":"legacy","accepted_at":"unix:0","content":"legacy"}\n',
        encoding="utf-8",
    )
    legacy_docs = tw_dir / "branches" / "legacy-child"
    legacy_docs.mkdir(parents=True)
    preserved_plan = "# Task Plan\n\nBranch: legacy-child\n\n## Acceptance\n\n- [ ] Preserve me.\n"
    (legacy_docs / "task_plan.md").write_text(preserved_plan, encoding="utf-8")
    (legacy_docs / "progress.md").write_text(
        "# Progress\n\nBranch: legacy-child\n\nLegacy progress.\n", encoding="utf-8"
    )
    (legacy_docs / "findings.md").write_text(
        "# Findings\n\nBranch: legacy-child\n\nLegacy finding.\n", encoding="utf-8"
    )
    (legacy_docs / "verification.md").write_text(
        "# Verification\n\nBranch: legacy-child\n\nPartial.\n", encoding="utf-8"
    )
    (tw_dir / "tree.yaml").write_text(
        "operations:\n  - create: stale-proposal\n", encoding="utf-8"
    )

    run(workspace, build_dir, "tree", "update")
    mixed_events = event_records(workspace)
    if mixed_events[1].get("schema_version") is not None:
        fail("legacy event fixture unexpectedly became current-format")
    assert_typed_event(mixed_events[-1], "tree.editing_updated")
    migrated_project = project_state(workspace)
    if migrated_project["tree_revision"] != 1 or "tree_hash" not in migrated_project:
        fail("legacy migration did not establish accepted Tree revision and hash")
    if "project_index_hash" in migrated_project:
        fail("legacy project_index_hash remained active after migration")
    accepted = load_json(tw_dir / "state" / "tree.json")
    if [node["id"] for node in accepted["nodes"]] != ["root", "legacy-child"]:
        fail("legacy migration changed branch identity or order")
    generated = (tw_dir / "tree.yaml").read_text(encoding="utf-8")
    if "legacy-child" not in generated or "operations:" in generated:
        fail("legacy migration did not replace operation-oriented YAML")
    if not any((tw_dir / "archive").glob("tree.yaml.legacy*")):
        fail("legacy migration did not archive the old tree.yaml")
    if not (tw_dir / "archive" / "graph.pre-declarative.json").is_file():
        fail("legacy migration did not archive old graph state")
    if not (tw_dir / "archive" / "project-index.pre-declarative.json").is_file():
        fail("legacy migration did not archive the old Project Index snapshot")

    run(workspace, build_dir, "tree", "apply")
    migrated_branch = branch_state(workspace, "legacy-child")
    if (
        migrated_branch["status"] != "paused"
        or migrated_branch["verification_status"] != "partial"
        or migrated_branch.get("status_reason") != "Migration fixture"
    ):
        fail("legacy migration changed lifecycle or verification state")
    if (legacy_docs / "task_plan.md").read_text(encoding="utf-8") != preserved_plan:
        fail("legacy migration overwrote existing branch documents")
    if project_state(workspace)["last_event_seq"] != 19:
        fail("legacy migration did not preserve and advance the event sequence")


def check_tree_apply_validation(temp_root: Path, build_dir: Path) -> None:
    workspace = temp_root / "tree-apply-validation"
    workspace.mkdir()
    run(workspace, build_dir, "init")
    run(workspace, build_dir, "align", "end")
    run(workspace, build_dir, "tree", "start")
    tw_dir = workspace / ".TreeWork"
    before_invalid = project_state(workspace)
    before_invalid_events = (tw_dir / "events.jsonl").read_text(encoding="utf-8")
    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Validation
  purpose: Validate declarative Apply.
  status: pending
""",
    )
    invalid = run(workspace, build_dir, "tree", "apply", expect_ok=False)
    if "unknown field" not in invalid.stderr or ".TreeWork/tree.yaml:" not in invalid.stderr:
        fail("invalid YAML did not report an actionable source location")
    if project_state(workspace) != before_invalid:
        fail("invalid YAML changed project state")
    if (tw_dir / "events.jsonl").read_text(encoding="utf-8") != before_invalid_events:
        fail("invalid YAML changed the event stream")

    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Validation
  purpose: Validate declarative Apply.
  spec: spec.md
  children:
    - id: alpha
      title: Alpha
      purpose: Validate stale session handling.
    - id: beta
      title: Beta
      purpose: Validate omission handling.
      depends_on: [alpha]
""",
    )
    run(workspace, build_dir, "tree", "apply")
    alpha_plan_path = tw_dir / "branches" / "alpha" / "task_plan.md"
    scope_marker = (
        "## Scope (owned work and boundary; not progress notes or implementation history)"
        "\n\n-"
    )
    alpha_plan = alpha_plan_path.read_text(encoding="utf-8")
    if scope_marker not in alpha_plan:
        fail("branch task plan fixture is missing the Scope placeholder")
    alpha_plan = alpha_plan.replace(
        scope_marker,
        f"{scope_marker} Hand-authored scope.",
        1,
    )
    alpha_plan_path.write_text(alpha_plan, encoding="utf-8")

    run(workspace, build_dir, "tree", "update")
    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Validation
  purpose: Validate declarative Apply.
  spec: spec.md
  children:
    - id: alpha
      title: Alpha
      purpose: A revised Tree purpose must not overwrite Plan scope.
    - id: beta
      title: Beta
      purpose: Validate omission handling.
      depends_on: [alpha]
""",
    )
    run(workspace, build_dir, "tree", "apply")
    if "Hand-authored scope." not in alpha_plan_path.read_text(encoding="utf-8"):
        fail("Tree metadata update overwrote Agent-authored task_plan Scope")
    accepted_before_failures = (tw_dir / "state" / "tree.json").read_text(encoding="utf-8")

    run(workspace, build_dir, "tree", "update")
    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Validation
  purpose: Validate declarative Apply.
  spec: spec.md
""",
    )
    omission = run(workspace, build_dir, "tree", "apply", expect_ok=False)
    if "omission cannot delete history" not in omission.stderr:
        fail("Tree apply did not reject accepted-branch omission")
    if (tw_dir / "state" / "tree.json").read_text(encoding="utf-8") != accepted_before_failures:
        fail("omission failure changed accepted Tree state")

    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Validation
  purpose: Validate declarative Apply.
  spec: spec.md
  children:
    - id: alpha
      title: Alpha
      purpose: A revised Tree purpose must not overwrite Plan scope.
    - id: beta
      title: Beta
      purpose: Validate omission handling.
      depends_on: [alpha]
""",
    )
    before_source_race_events = (tw_dir / "events.jsonl").read_text(encoding="utf-8")
    source_race = run(
        workspace,
        build_dir,
        "tree",
        "apply",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "tree-apply-mutate-source"},
    )
    if "changed while Apply was preparing" not in source_race.stderr:
        fail("Tree apply did not detect source mutation during prepare")
    if (tw_dir / "events.jsonl").read_text(encoding="utf-8") != before_source_race_events:
        fail("source mutation failure changed the event stream")
    if (tw_dir / "state" / "tree.json").read_text(encoding="utf-8") != accepted_before_failures:
        fail("source mutation failure changed accepted Tree state")
    run(workspace, build_dir, "tree", "apply")

    run(workspace, build_dir, "tree", "update")
    accepted_before_stale = (tw_dir / "state" / "tree.json").read_text(encoding="utf-8")
    run(workspace, build_dir, "enter", "alpha", "--no-isolate")
    stale = run(workspace, build_dir, "tree", "apply", expect_ok=False)
    if "stale Tree Editing Session" not in stale.stderr:
        fail("Tree apply did not reject a stale event/state base")
    if (tw_dir / "state" / "tree.json").read_text(encoding="utf-8") != accepted_before_stale:
        fail("stale-session rejection changed accepted Tree state")


def durable_treework_snapshot(workspace: Path) -> dict[str, bytes]:
    tw_dir = workspace / ".TreeWork"
    snapshot: dict[str, bytes] = {}
    for path in sorted(tw_dir.rglob("*")):
        if not path.is_file():
            continue
        relative = path.relative_to(tw_dir).as_posix()
        if relative.startswith("out/") or relative == "state/pending-transaction.json":
            continue
        snapshot[relative] = path.read_bytes()
    return snapshot


def check_publication_recovery(temp_root: Path, build_dir: Path) -> None:
    for point in [
        "transaction-after-checkpoint",
        "transaction-after-accepted-state",
        "transaction-after-event",
        "transaction-after-durable-intent",
    ]:
        workspace = temp_root / f"fresh-{point}"
        workspace.mkdir()
        run(
            workspace,
            build_dir,
            "init",
            expect_ok=False,
            extra_env={"TREEWORK_TEST_FAILPOINT": point},
        )
        if (workspace / ".TreeWork").exists():
            fail(f"fresh init failure at {point} did not remove partial TreeWork state")
        if (workspace / ".TreeWork.pending-transaction-backup").exists():
            fail(f"fresh init failure at {point} left a backup")

    init_manifest_required = {
        "PROJECT.md",
        "tree.yaml",
        "requirements.md",
        "assumptions.md",
        "references.md",
        "idea_inbox.md",
        "spec.md",
        "task_plan.md",
        "progress.md",
        "findings.md",
        "events.jsonl",
        "state",
        "state/branches.json",
        "state/graph.json",
        "history",
        "history/checkpoints",
        "branches",
    }
    crash_rollback = temp_root / "fresh-crash-before-marker"
    crash_rollback.mkdir()
    run(
        crash_rollback,
        build_dir,
        "init",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "crash-transaction-after-durable-intent"},
    )
    init_journal = load_json(
        crash_rollback / ".TreeWork" / "state" / "pending-transaction.json"
    )
    init_manifest = intended_control_manifest(init_journal)
    if not init_manifest_required.issubset(init_manifest) or not any(
        path.startswith("history/checkpoints/tree-r000000-")
        for path in init_manifest
    ):
        fail("fresh init intent did not include the complete scaffold and genesis checkpoint")
    run(crash_rollback, build_dir, "check", "--brief", expect_ok=False)
    if (crash_rollback / ".TreeWork").exists():
        fail("fresh init pre-marker crash recovery left scaffold documents")

    crash_forward = temp_root / "fresh-crash-marker-forward"
    crash_forward.mkdir()
    run(
        crash_forward,
        build_dir,
        "init",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "crash-transaction-after-project-marker"},
    )
    forward_journal = load_json(
        crash_forward / ".TreeWork" / "state" / "pending-transaction.json"
    )
    if not init_manifest_required.issubset(intended_control_manifest(forward_journal)):
        fail("fresh init marker crash journal omitted scaffold documents")
    run(crash_forward, build_dir, "check", "--brief")
    assert_full_init_scaffold(crash_forward)
    assert_no_pending_transaction(crash_forward)

    marker_workspace = temp_root / "fresh-marker-forward"
    marker_workspace.mkdir()
    run(
        marker_workspace,
        build_dir,
        "init",
        extra_env={"TREEWORK_TEST_FAILPOINT": "transaction-after-project-marker"},
    )
    if project_state(marker_workspace)["last_event_seq"] != 1:
        fail("post-marker init failure did not finish forward")
    assert_typed_event(event_records(marker_workspace)[-1], "project.initialized")
    assert_full_init_scaffold(marker_workspace)
    assert_no_pending_transaction(marker_workspace)

    workspace = temp_root / "publication-recovery"
    workspace.mkdir()
    run(workspace, build_dir, "init")
    run(workspace, build_dir, "align", "end")
    run(workspace, build_dir, "tree", "start")
    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Publication Recovery
  purpose: Verify transaction publication recovery.
  spec: spec.md
  children:
    - id: alpha
      title: Alpha
      purpose: Accepted baseline.
      spec: branches/alpha/spec.md
""",
    )
    run(workspace, build_dir, "tree", "apply")
    write_tree(workspace, "legacy_tree: true")
    before_migration = durable_treework_snapshot(workspace)
    run(
        workspace,
        build_dir,
        "tree",
        "update",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "transaction-after-event"},
    )
    if durable_treework_snapshot(workspace) != before_migration:
        fail("pre-marker migration failure did not restore Tree and archive bytes")
    run(
        workspace,
        build_dir,
        "tree",
        "update",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "crash-transaction-after-project-marker"},
    )
    migration_journal = load_json(
        workspace / ".TreeWork" / "state" / "pending-transaction.json"
    )
    if not any(
        path.startswith("archive/") for path in intended_control_manifest(migration_journal)
    ):
        fail("migration publication intent omitted archive results")
    run(workspace, build_dir, "check", "--brief")
    if not any((workspace / ".TreeWork" / "archive").iterdir()):
        fail("marker-forward migration lost archived Tree artifacts")
    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Publication Recovery
  purpose: Verify transaction publication recovery.
  spec: spec.md
  children:
    - id: alpha
      title: Alpha
      purpose: Accepted baseline.
      spec: branches/alpha/spec.md
    - id: beta
      title: Beta
      purpose: Failure boundary candidate.
      spec: design/beta.md
""",
    )
    before = durable_treework_snapshot(workspace)
    for point in [
        "transaction-after-checkpoint",
        "transaction-after-accepted-state",
        "transaction-after-event",
        "transaction-after-durable-intent",
    ]:
        run(
            workspace,
            build_dir,
            "tree",
            "apply",
            expect_ok=False,
            extra_env={"TREEWORK_TEST_FAILPOINT": point},
        )
        if durable_treework_snapshot(workspace) != before:
            fail(f"Apply failure at {point} did not restore exact prior bytes")
        if (
            (workspace / ".TreeWork" / "branches" / "beta").exists()
            or (workspace / ".TreeWork" / "design" / "beta.md").exists()
        ):
            fail(f"Apply failure at {point} left new branch or custom Spec documents")
        assert_no_pending_transaction(workspace)

    before_marker_seq = project_state(workspace)["last_event_seq"]
    run(
        workspace,
        build_dir,
        "tree",
        "apply",
        extra_env={"TREEWORK_TEST_FAILPOINT": "transaction-after-project-marker"},
    )
    if (
        project_state(workspace)["last_event_seq"] != before_marker_seq + 1
        or "beta" not in branch_paths(workspace)
    ):
        fail("post-marker Apply failure did not finish the intended commit forward")
    marker_event = event_records(workspace)[-1]
    assert_typed_event(marker_event, "tree.applied")
    if not (workspace / ".TreeWork" / marker_event["data"]["snapshot_ref"]).is_file():
        fail("post-marker forward recovery lost its checkpoint")
    assert_branch_documents(workspace, "beta", "design/beta.md")
    assert_no_pending_transaction(workspace)

    run(workspace, build_dir, "tree", "update")
    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Publication Recovery
  purpose: Verify transaction publication recovery.
  spec: spec.md
  children:
    - id: alpha
      title: Alpha
      purpose: Accepted baseline.
      spec: branches/alpha/spec.md
    - id: beta
      title: Beta
      purpose: Failure boundary candidate.
      spec: design/beta.md
    - id: gamma
      title: Gamma
      purpose: Crash recovery candidate.
      spec: design/gamma.md
""",
    )
    before_crash = durable_treework_snapshot(workspace)
    run(
        workspace,
        build_dir,
        "tree",
        "apply",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "crash-transaction-after-event"},
    )
    if not (workspace / ".TreeWork" / "state" / "pending-transaction.json").is_file():
        fail("crash failpoint did not preserve the recovery journal")
    run(workspace, build_dir, "check", "--brief")
    if durable_treework_snapshot(workspace) != before_crash:
        fail("startup recovery did not exactly roll back a pre-marker crash")
    if (
        (workspace / ".TreeWork" / "branches" / "gamma").exists()
        or (workspace / ".TreeWork" / "design" / "gamma.md").exists()
    ):
        fail("pre-marker Apply crash left new branch or custom Spec documents")
    assert_no_pending_transaction(workspace)

    before_durable_seq = project_state(workspace)["last_event_seq"]
    run(
        workspace,
        build_dir,
        "tree",
        "apply",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "crash-transaction-after-durable-intent"},
    )
    journal = load_json(workspace / ".TreeWork" / "state" / "pending-transaction.json")
    if journal.get("pre_marker_durable") is not True or journal.get("intended") is None:
        fail("durable-intent crash did not persist its pre-marker durability proof")
    apply_manifest = intended_control_manifest(journal)
    required_apply_documents = {
        "branches/gamma",
        "branches/gamma/task_plan.md",
        "branches/gamma/progress.md",
        "branches/gamma/findings.md",
        "branches/gamma/verification.md",
        "branches/alpha/spec.md",
        "design/gamma.md",
        "events.jsonl",
        "state/branches.json",
        "state/graph.json",
        "state/tree.json",
    }
    if not required_apply_documents.issubset(apply_manifest) or not any(
        path.startswith("history/checkpoints/tree-r") for path in apply_manifest
    ):
        fail("Apply intent omitted branch documents, custom Spec, state, event, or checkpoint")
    if project_state(workspace)["last_event_seq"] != before_durable_seq:
        fail("durable-intent crash published project.json before its marker boundary")
    run(workspace, build_dir, "check", "--brief")
    if durable_treework_snapshot(workspace) != before_crash:
        fail("durable-intent crash did not exactly roll back before marker publication")
    if (
        (workspace / ".TreeWork" / "branches" / "gamma").exists()
        or (workspace / ".TreeWork" / "design" / "gamma.md").exists()
    ):
        fail("durable-intent rollback left new branch or custom Spec documents")
    assert_no_pending_transaction(workspace)

    before_crash_marker_seq = project_state(workspace)["last_event_seq"]
    run(
        workspace,
        build_dir,
        "tree",
        "apply",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "crash-transaction-after-project-marker"},
    )
    if not (workspace / ".TreeWork" / "state" / "pending-transaction.json").is_file():
        fail("post-marker crash did not leave marker-aware recovery evidence")
    marker_manifest = intended_control_manifest(
        load_json(workspace / ".TreeWork" / "state" / "pending-transaction.json")
    )
    if not required_apply_documents.issubset(marker_manifest):
        fail("post-marker Apply journal omitted transaction-owned documents")
    run(workspace, build_dir, "check", "--brief")
    if (
        project_state(workspace)["last_event_seq"] != before_crash_marker_seq + 1
        or "gamma" not in branch_paths(workspace)
    ):
        fail("startup recovery did not finish a complete marker commit forward")
    assert_typed_event(event_records(workspace)[-1], "tree.applied")
    assert_branch_documents(workspace, "gamma", "design/gamma.md")
    assert_no_pending_transaction(workspace)


def initialize_git_workspace(workspace: Path) -> None:
    subprocess.run(["git", "init"], cwd=workspace, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    subprocess.run(["git", "config", "user.email", "treework@example.local"], cwd=workspace, check=True)
    subprocess.run(["git", "config", "user.name", "TreeWork Test"], cwd=workspace, check=True)
    subprocess.run(
        ["git", "commit", "--allow-empty", "-m", "initial"],
        cwd=workspace,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )


def worktree_binding_path(worktree: Path) -> Path:
    git_dir = Path(
        subprocess.check_output(
            ["git", "rev-parse", "--path-format=absolute", "--git-dir"],
            cwd=worktree,
            text=True,
        ).strip()
    )
    return git_dir / "treework-branch.json"


def prepare_completion_worktree(
    workspace: Path,
    build_dir: Path,
    branch: str,
) -> tuple[Path, Path]:
    run(workspace, build_dir, "enter", branch)
    worktree = Path(branch_state(workspace, branch)["isolation"]["workspace_path"])
    if not worktree.is_dir():
        fail(f"enter did not create the completion worktree for {branch}")
    complete_acceptance(worktree, branch)
    run(
        worktree,
        build_dir,
        "verify",
        "--cmd",
        f"{branch} completion recovery",
        "--result",
        "passed",
        "--gap",
        "none",
    )
    subprocess.run(["git", "add", "."], cwd=worktree, check=True)
    subprocess.run(
        ["git", "commit", "-m", f"prepare {branch} completion"],
        cwd=worktree,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    binding = worktree_binding_path(worktree)
    if not binding.is_file():
        fail(f"{branch} completion fixture has no TreeWork binding")
    return worktree, binding


def check_worktree_binding(temp_root: Path, build_dir: Path) -> None:
    if not shutil.which("git"):
        return
    workspace = temp_root / "worktree-binding"
    workspace.mkdir()
    initialize_git_workspace(workspace)
    run(workspace, build_dir, "init")
    run(workspace, build_dir, "align", "end")
    run(workspace, build_dir, "tree", "start")
    write_tree(
        workspace,
        """version: 1
tree:
  id: root
  title: Worktree Test
  purpose: Exercise isolated workers.
  spec: spec.md
  children:
    - id: worker-a
      title: Worker A
      purpose: Independent worker branch A.
    - id: worker-b
      title: Worker B
      purpose: Independent worker branch B.
    - id: cleanup-remove
      title: Cleanup Remove
      purpose: Verify recoverable worktree removal.
    - id: cleanup-keep
      title: Cleanup Keep
      purpose: Verify recoverable binding removal.
    - id: cleanup-warning-remove
      title: Cleanup Warning Remove
      purpose: Verify committed removal warnings.
    - id: cleanup-warning-keep
      title: Cleanup Warning Keep
      purpose: Verify committed binding warnings.
    - id: cleanup-guard
      title: Cleanup Guard
      purpose: Verify untrusted workspace paths are never removed.
""",
    )
    run(workspace, build_dir, "tree", "apply")
    subprocess.run(["git", "add", "."], cwd=workspace, check=True)
    subprocess.run(
        ["git", "commit", "-m", "track TreeWork state"],
        cwd=workspace,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )

    before_enter_crash = (
        project_state(workspace),
        (workspace / ".TreeWork" / "state" / "branches.json").read_bytes(),
        (workspace / ".TreeWork" / "events.jsonl").read_bytes(),
    )
    failed_workspace = (
        workspace.parent / ".treework-worktrees" / workspace.name / "worker-a"
    )
    run(
        workspace,
        build_dir,
        "enter",
        "worker-a",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "crash-transaction-after-event"},
    )
    if not failed_workspace.is_dir():
        fail("enter crash fixture did not create its external worktree")
    run(workspace, build_dir, "check", "--brief")
    after_enter_crash = (
        project_state(workspace),
        (workspace / ".TreeWork" / "state" / "branches.json").read_bytes(),
        (workspace / ".TreeWork" / "events.jsonl").read_bytes(),
    )
    if after_enter_crash != before_enter_crash:
        fail("enter crash recovery did not restore exact accepted state")
    if failed_workspace.exists():
        fail("enter crash recovery did not clean the newly created worktree")
    branch_ref = subprocess.run(
        ["git", "show-ref", "--verify", "--quiet", "refs/heads/treework/worker-a"],
        cwd=workspace,
        check=False,
    )
    if branch_ref.returncode == 0:
        fail("enter crash recovery left the newly created Git branch")

    run(workspace, build_dir, "enter", "worker-a")
    run(workspace, build_dir, "enter", "worker-b")
    worker_a = Path(branch_state(workspace, "worker-a")["isolation"]["workspace_path"])
    worker_b = Path(branch_state(workspace, "worker-b")["isolation"]["workspace_path"])
    if not worker_a.is_dir() or not worker_b.is_dir():
        fail("enter did not create independent managed worktrees")

    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    env["TREEWORK_BUILD_DIR"] = str(build_dir)
    proc_a = subprocess.Popen(
        [str(TW), "pause", "--reason", "worker A parked"],
        cwd=worker_a,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    proc_b = subprocess.Popen(
        [str(TW), "abort", "--reason", "worker B cancelled"],
        cwd=worker_b,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    stdout_a, stderr_a = proc_a.communicate(timeout=30)
    stdout_b, stderr_b = proc_b.communicate(timeout=30)
    if proc_a.returncode != 0 or proc_b.returncode != 0:
        fail(
            "parallel bound transitions failed\n"
            f"worker-a stdout:\n{stdout_a}\nstderr:\n{stderr_a}\n"
            f"worker-b stdout:\n{stdout_b}\nstderr:\n{stderr_b}"
        )
    if branch_state(workspace, "worker-a")["status"] != "paused":
        fail("worker-a default transition targeted the wrong branch")
    if branch_state(workspace, "worker-b")["status"] != "aborted":
        fail("worker-b default transition targeted the wrong branch")
    if project_state(workspace)["current_branch"] != "worker-b":
        fail("branch-worktree transitions changed the control cursor")

    cross_recall = json.loads(
        run(worker_a, build_dir, "recall", "worker-b", "--json").stdout
    )
    blocked_by_action = {
        item["action"]: item for item in cross_recall.get("blocked_actions", [])
    }
    for action in ["enter", "pause", "abort", "complete"]:
        if "workspace_branch_mismatch" not in blocked_by_action.get(action, {}).get(
            "reason_codes", []
        ):
            fail(
                f"bound-worktree Recall did not reject cross-branch {action} eligibility"
            )

    events = workspace / ".TreeWork" / "events.jsonl"
    before_rejection = events.read_text(encoding="utf-8")
    rejected = run(worker_a, build_dir, "enter", "worker-b", "--no-isolate", expect_ok=False)
    if "worktree is bound to `worker-a`" not in rejected.stderr:
        fail("cross-branch worktree rejection did not explain its binding")
    if events.read_text(encoding="utf-8") != before_rejection:
        fail("rejected cross-branch command mutated events")

    structural = run(worker_a, build_dir, "tree", "update", expect_ok=False)
    if "run this from the control workspace" not in structural.stderr:
        fail("branch worktree was allowed to change global topology")

    git_dir = Path(
        subprocess.check_output(
            ["git", "rev-parse", "--path-format=absolute", "--git-dir"],
            cwd=worker_a,
            text=True,
        ).strip()
    )
    binding = git_dir / "treework-branch.json"
    backup = git_dir / "treework-branch.json.test-backup"
    binding.rename(backup)
    try:
        before_unbound = events.read_text(encoding="utf-8")
        result = run(worker_a, build_dir, "pause", expect_ok=False)
        if "has no branch binding" not in result.stderr:
            fail("unbound linked worktree did not fail closed")
        if events.read_text(encoding="utf-8") != before_unbound:
            fail("unbound linked worktree mutated events")
    finally:
        backup.rename(binding)

    remove_worktree, remove_binding = prepare_completion_worktree(
        workspace, build_dir, "cleanup-remove"
    )
    remove_progress = (
        remove_worktree / ".TreeWork" / "branches" / "cleanup-remove" / "progress.md"
    )
    before_remove_failure = (
        (workspace / ".TreeWork" / "state" / "project.json").read_bytes(),
        (workspace / ".TreeWork" / "state" / "branches.json").read_bytes(),
        (workspace / ".TreeWork" / "events.jsonl").read_bytes(),
        remove_progress.read_bytes(),
    )
    run(
        remove_worktree,
        build_dir,
        "complete",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "transaction-after-accepted-state"},
    )
    after_remove_failure = (
        (workspace / ".TreeWork" / "state" / "project.json").read_bytes(),
        (workspace / ".TreeWork" / "state" / "branches.json").read_bytes(),
        (workspace / ".TreeWork" / "events.jsonl").read_bytes(),
        remove_progress.read_bytes(),
    )
    if after_remove_failure != before_remove_failure:
        fail("failed completion remove path did not restore exact transaction bytes")
    if not remove_worktree.is_dir() or not remove_binding.is_file():
        fail("failed completion remove path changed the worktree or binding")
    if branch_state(workspace, "cleanup-remove")["isolation"]["managed_by_treework"] is not True:
        fail("failed completion remove path released accepted worktree management")
    assert_no_pending_transaction(workspace)
    run(remove_worktree, build_dir, "complete")
    completed_remove = branch_state(workspace, "cleanup-remove")
    if (
        completed_remove["status"] != "complete"
        or completed_remove["isolation"]["managed_by_treework"] is not False
        or "cleanup intent: remove worktree"
        not in completed_remove["isolation"]["last_status"]
    ):
        fail("successful completion remove path did not commit branch state")
    if remove_worktree.exists() or remove_binding.exists():
        fail("successful completion remove path did not remove its managed worktree")

    keep_worktree, keep_binding = prepare_completion_worktree(
        workspace, build_dir, "cleanup-keep"
    )
    keep_progress = (
        keep_worktree / ".TreeWork" / "branches" / "cleanup-keep" / "progress.md"
    )
    before_keep_failure = (
        (workspace / ".TreeWork" / "state" / "project.json").read_bytes(),
        (workspace / ".TreeWork" / "state" / "branches.json").read_bytes(),
        (workspace / ".TreeWork" / "events.jsonl").read_bytes(),
        keep_progress.read_bytes(),
    )
    run(
        keep_worktree,
        build_dir,
        "complete",
        "--keep-worktree",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "transaction-after-event"},
    )
    after_keep_failure = (
        (workspace / ".TreeWork" / "state" / "project.json").read_bytes(),
        (workspace / ".TreeWork" / "state" / "branches.json").read_bytes(),
        (workspace / ".TreeWork" / "events.jsonl").read_bytes(),
        keep_progress.read_bytes(),
    )
    if after_keep_failure != before_keep_failure:
        fail("failed --keep-worktree completion did not restore exact transaction bytes")
    if not keep_worktree.is_dir() or not keep_binding.is_file():
        fail("failed --keep-worktree completion changed the worktree or binding")
    if branch_state(workspace, "cleanup-keep")["isolation"]["managed_by_treework"] is not True:
        fail("failed --keep-worktree completion released accepted worktree management")
    assert_no_pending_transaction(workspace)
    run(keep_worktree, build_dir, "complete", "--keep-worktree")
    completed_keep = branch_state(workspace, "cleanup-keep")
    if (
        completed_keep["status"] != "complete"
        or completed_keep["isolation"]["managed_by_treework"] is not False
        or "cleanup intent: keep worktree and remove binding"
        not in completed_keep["isolation"]["last_status"]
    ):
        fail("successful --keep-worktree completion did not commit branch state")
    if not keep_worktree.is_dir() or keep_binding.exists():
        fail("successful --keep-worktree completion did not preserve only the worktree")

    warning_remove, warning_remove_binding = prepare_completion_worktree(
        workspace, build_dir, "cleanup-warning-remove"
    )
    warning = run(
        warning_remove,
        build_dir,
        "complete",
        extra_env={"TREEWORK_TEST_FAILPOINT": "completion-cleanup-before-remove"},
    )
    if "is complete, but isolation cleanup did not finish" not in warning.stderr:
        fail("post-commit worktree removal failure did not return a cleanup warning")
    if (
        branch_state(workspace, "cleanup-warning-remove")["status"] != "complete"
        or branch_state(workspace, "cleanup-warning-remove")["isolation"][
            "managed_by_treework"
        ]
        is not False
        or not warning_remove.is_dir()
        or not warning_remove_binding.is_file()
    ):
        fail("post-commit worktree removal warning had inconsistent state semantics")

    warning_keep, warning_keep_binding = prepare_completion_worktree(
        workspace, build_dir, "cleanup-warning-keep"
    )
    warning = run(
        warning_keep,
        build_dir,
        "complete",
        "--keep-worktree",
        extra_env={"TREEWORK_TEST_FAILPOINT": "completion-cleanup-before-unbind"},
    )
    if "is complete, but isolation cleanup did not finish" not in warning.stderr:
        fail("post-commit binding removal failure did not return a cleanup warning")
    if (
        branch_state(workspace, "cleanup-warning-keep")["status"] != "complete"
        or branch_state(workspace, "cleanup-warning-keep")["isolation"][
            "managed_by_treework"
        ]
        is not False
        or not warning_keep.is_dir()
        or not warning_keep_binding.is_file()
    ):
        fail("post-commit binding removal warning had inconsistent state semantics")

    guard_worktree, _guard_binding = prepare_completion_worktree(
        workspace, build_dir, "cleanup-guard"
    )
    victim = temp_root / "untrusted-workspace-path"
    victim.mkdir()
    sentinel = victim / "sentinel.txt"
    sentinel.write_text("must survive rollback and cleanup\n", encoding="utf-8")
    branches_path = workspace / ".TreeWork" / "state" / "branches.json"
    trusted_branches = branches_path.read_bytes()
    tampered_branches = load_json(branches_path)
    guard_state = next(
        item
        for item in tampered_branches["branches"]
        if item["path"] == "cleanup-guard"
    )
    guard_state["isolation"]["workspace_path"] = str(victim)
    branches_path.write_text(
        json.dumps(tampered_branches, indent=2) + "\n",
        encoding="utf-8",
    )
    before_guard_events = (workspace / ".TreeWork" / "events.jsonl").read_bytes()
    run(
        guard_worktree,
        build_dir,
        "pause",
        "--reason",
        "exercise rollback path guard",
        expect_ok=False,
        extra_env={"TREEWORK_TEST_FAILPOINT": "transaction-after-event"},
    )
    if sentinel.read_text(encoding="utf-8") != "must survive rollback and cleanup\n":
        fail("rollback followed an unverified accepted-state workspace_path")
    if (workspace / ".TreeWork" / "events.jsonl").read_bytes() != before_guard_events:
        fail("guarded rollback did not restore its event bytes")
    assert_no_pending_transaction(workspace)
    rejected_cleanup = run(guard_worktree, build_dir, "complete", expect_ok=False)
    if "is not a Git worktree" not in rejected_cleanup.stderr:
        fail("completion did not reject an unverified accepted-state workspace_path")
    if not victim.is_dir() or not sentinel.is_file() or not guard_worktree.is_dir():
        fail("rejected untrusted cleanup path changed an external directory or managed worktree")
    branches_path.write_bytes(trusted_branches)
    run(guard_worktree, build_dir, "complete")
    if guard_worktree.exists():
        fail("guard fixture could not complete through its verified managed worktree")


def main() -> None:
    if not TW.is_file():
        fail(f"missing tw wrapper at {TW}")
    temp_root = Path(tempfile.mkdtemp(prefix="treework-cli-regression-"))
    build_dir = temp_root / ".build"
    try:
        manifest = load_json(PLUGIN_ROOT / ".codex-plugin" / "plugin.json")
        expected_version = f"tw {manifest['version']}"
        version = run(temp_root, build_dir, "--version")
        if version.stdout.strip() != expected_version:
            fail("--version output changed")
        version_command = run(temp_root, build_dir, "version")
        if version_command.stdout.strip() != expected_version:
            fail("version command does not expose the plugin build version")
        check_alignment_workflow(temp_root, build_dir)
        check_graph_output_symlink_safety(temp_root, build_dir)
        check_primary_flow(temp_root, build_dir)
        check_legacy_state_read(temp_root, build_dir)
        check_project_index_state_migration(temp_root, build_dir)
        check_tree_apply_validation(temp_root, build_dir)
        check_publication_recovery(temp_root, build_dir)
        check_worktree_binding(temp_root, build_dir)
        print("ok: TreeWork current CLI regression")
    finally:
        shutil.rmtree(temp_root, ignore_errors=True)


if __name__ == "__main__":
    main()
