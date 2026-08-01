#!/usr/bin/env python3
"""Installed-product acceptance for the packaged Project Map."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

from _paths import PLUGIN_ROOT, REPOSITORY_ROOT
from check_mcp import (
    McpClient,
    accepted_state_snapshot,
    assert_tool_result,
    wait_for_process_exit,
)


TW = PLUGIN_ROOT / "skills" / "treework" / "scripts" / "tw"
BUNDLED_RUNTIME = (
    Path.home()
    / ".cache"
    / "codex-runtimes"
    / "codex-primary-runtime"
    / "dependencies"
)
NODE = Path(
    os.environ.get("TREEWORK_NODE", BUNDLED_RUNTIME / "node" / "bin" / "node")
)
NODE_MODULES = Path(
    os.environ.get(
        "TREEWORK_NODE_PATH",
        BUNDLED_RUNTIME / "node" / "node_modules",
    )
)


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def run_tw(
    workspace: Path,
    build_dir: Path,
    *args: str,
) -> subprocess.CompletedProcess[str]:
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
        timeout=180,
        check=False,
    )
    if result.returncode != 0:
        fail(
            f"tw {' '.join(args)} failed with {result.returncode}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def load_json(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path, value: Any) -> None:
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def plugin_identity() -> dict[str, str]:
    manifest = load_json(PLUGIN_ROOT / ".codex-plugin" / "plugin.json")
    source_mode = (REPOSITORY_ROOT / ".git").exists()
    identity = {
        "distribution": "tracked-plugin" if source_mode else "clean-package",
        "name": str(manifest.get("name", "")),
        "version": str(manifest.get("version", "")),
    }
    if source_mode:
        result = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=REPOSITORY_ROOT,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
        if result.returncode != 0:
            fail(f"cannot identify source revision: {result.stderr.strip()}")
        identity["revision"] = result.stdout.strip()
    return identity


def request_json(url: str) -> tuple[int, dict[str, Any]]:
    try:
        with urllib.request.urlopen(url, timeout=8) as response:
            return response.status, json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        return error.code, json.loads(error.read().decode("utf-8"))


def fresh_tree() -> str:
    return """version: 1
tree:
  id: root
  title: Installed Project Map
  purpose: Verify the packaged three-view product.
  spec: spec.md
  children:
    - id: accepted-base
      title: Accepted Base
      purpose: Provide a satisfied prerequisite.
      spec: branches/accepted-base/spec.md
    - id: installed-map
      title: Installed Map
      purpose: Exercise Map, Dependency, Replay, and Inspector.
      spec: branches/installed-map/spec.md
      depends_on:
        - accepted-base
    - id: downstream
      title: Downstream
      purpose: Show packaged downstream impact.
      depends_on:
        - installed-map
"""


def complete_branch(workspace: Path, build_dir: Path, branch: str) -> None:
    run_tw(workspace, build_dir, "enter", branch, "--no-isolate")
    plan = workspace / ".TreeWork" / "branches" / branch / "task_plan.md"
    plan.write_text(
        plan.read_text(encoding="utf-8").replace("- [ ]", "- [x]"),
        encoding="utf-8",
    )
    run_tw(
        workspace,
        build_dir,
        "verify",
        "--cmd",
        "installed package prerequisite",
        "--result",
        "passed",
        "--gap",
        "none",
    )
    run_tw(workspace, build_dir, "complete")


def prepare_fresh(workspace: Path, build_dir: Path) -> None:
    workspace.mkdir()
    subprocess.run(
        ["git", "init", "-q"],
        cwd=workspace,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    run_tw(workspace, build_dir, "init")
    run_tw(workspace, build_dir, "align", "end")
    run_tw(workspace, build_dir, "tree", "start")
    (workspace / ".TreeWork" / "tree.yaml").write_text(
        fresh_tree(),
        encoding="utf-8",
    )
    run_tw(workspace, build_dir, "tree", "apply")
    complete_branch(workspace, build_dir, "accepted-base")
    run_tw(workspace, build_dir, "enter", "installed-map", "--no-isolate")
    run_tw(
        workspace,
        build_dir,
        "verify",
        "--cmd",
        "installed package live verification",
        "--result",
        "partial",
        "--gap",
        "browser acceptance pending",
    )


def branch_state(
    branch_id: str,
    parent: str,
    title: str,
    purpose: str,
    status: str,
    verification: str,
    reason: str = "",
) -> dict[str, Any]:
    return {
        "path": branch_id,
        "parent": parent,
        "title": title,
        "purpose": purpose,
        "status": status,
        "verification_status": verification,
        "sync_status": "clean",
        "status_reason": reason,
        "last_sync": "unix:1700000000",
    }


def prepare_legacy(workspace: Path, build_dir: Path) -> dict[str, Any]:
    workspace.mkdir()
    run_tw(workspace, build_dir, "init")
    tw_dir = workspace / ".TreeWork"
    project_path = tw_dir / "state" / "project.json"
    project = load_json(project_path)
    project.update(
        {
            "stage": "work_tree",
            "current_branch": "legacy-child",
            "last_event_seq": 3,
            "tree_revision": 0,
            "project_index_hash": "legacy-index-hash",
            "last_sync": "unix:1700000000",
        }
    )
    project.pop("tree_hash", None)
    write_json(project_path, project)
    events = [
        json.loads(line)
        for line in (tw_dir / "events.jsonl").read_text(encoding="utf-8").splitlines()
        if line
    ]
    events.extend(
        [
            {
                "seq": 2,
                "time": "unix:1700000002",
                "type": "branch.next_updated",
                "subject": "legacy-child",
                "message": "Legacy routing event",
            },
            {
                "seq": 3,
                "time": "unix:1700000003",
                "type": "branch.status_changed",
                "subject": "legacy-child",
                "message": "Legacy lifecycle event",
            },
        ]
    )
    (tw_dir / "events.jsonl").write_text(
        "".join(
            json.dumps(event, separators=(",", ":"), ensure_ascii=False) + "\n"
            for event in events
        ),
        encoding="utf-8",
    )
    write_json(
        tw_dir / "state" / "branches.json",
        {
            "branches": [
                branch_state(
                    "root",
                    "",
                    "Legacy Project",
                    "Preserve the migrated project.",
                    "in_progress",
                    "unverified",
                ),
                branch_state(
                    "legacy-base",
                    "root",
                    "Legacy Base",
                    "Preserve a completed prerequisite.",
                    "complete",
                    "verified",
                ),
                branch_state(
                    "legacy-child",
                    "root",
                    "Legacy Child",
                    "Preserve identity, documents, and dependency state.",
                    "paused",
                    "partial",
                    "Migration fixture",
                ),
            ]
        },
    )
    write_json(
        tw_dir / "state" / "graph.json",
        {
            "edges": [
                {
                    "id": "edge-parent-base",
                    "from": "root",
                    "to": "legacy-base",
                    "kind": "parent_of",
                    "user_label": "root contains legacy-base",
                    "interpreted_relation": "parent_of",
                },
                {
                    "id": "edge-parent-child",
                    "from": "root",
                    "to": "legacy-child",
                    "kind": "parent_of",
                    "user_label": "root contains legacy-child",
                    "interpreted_relation": "parent_of",
                },
                {
                    "id": "edge-dependency",
                    "from": "legacy-child",
                    "to": "legacy-base",
                    "kind": "depends_on",
                    "user_label": "legacy-child depends on legacy-base",
                    "interpreted_relation": "depends_on",
                },
            ]
        },
    )
    (tw_dir / "state" / "project-index.json").write_text(
        '{"revision":1,"hash":"legacy","accepted_at":"unix:0","content":"legacy"}\n',
        encoding="utf-8",
    )
    child_docs = tw_dir / "branches" / "legacy-child"
    child_docs.mkdir(parents=True)
    preserved_plan = (
        "# Task Plan\n\nBranch: legacy-child\n\n"
        "## Scope\n\nPreserve legacy scope.\n\n"
        "## Acceptance\n\n- [ ] Preserve identity and documents.\n\n"
        "## Dependencies\n\nLegacy external prerequisite.\n"
    )
    (child_docs / "task_plan.md").write_text(preserved_plan, encoding="utf-8")
    (child_docs / "progress.md").write_text(
        "# Progress\n\nBranch: legacy-child\n\n"
        "## Current Reality\n\nLegacy progress remains visible.\n",
        encoding="utf-8",
    )
    (child_docs / "findings.md").write_text(
        "# Findings\n\nBranch: legacy-child\n\n"
        "## Decisions\n\nPreserve the accepted identity.\n",
        encoding="utf-8",
    )
    (child_docs / "verification.md").write_text(
        "# Verification\n\nBranch: legacy-child\n\n"
        "Status: partial\n\n## Evidence\n\n- Legacy evidence.\n",
        encoding="utf-8",
    )
    (tw_dir / "tree.yaml").write_text(
        "operations:\n  - create: stale-proposal\n",
        encoding="utf-8",
    )

    run_tw(workspace, build_dir, "tree", "update")
    run_tw(workspace, build_dir, "tree", "apply")
    if (child_docs / "task_plan.md").read_text(encoding="utf-8") != preserved_plan:
        fail("legacy migration overwrote branch documents")
    accepted = load_json(tw_dir / "state" / "tree.json")
    ids = [node["id"] for node in accepted["nodes"]]
    if ids != ["root", "legacy-base", "legacy-child"]:
        fail(f"legacy migration changed branch identity or order: {ids}")
    child = next(node for node in accepted["nodes"] if node["id"] == "legacy-child")
    if child["depends_on"] != ["legacy-base"]:
        fail("legacy migration lost dependency topology")
    migrated_project = load_json(project_path)
    if (
        migrated_project.get("last_event_seq") != 5
        or migrated_project.get("tree_revision") != 1
        or "tree_hash" not in migrated_project
    ):
        fail(f"legacy migration published unexpected revisions: {migrated_project}")
    return {
        "branch_ids": ids,
        "last_event_seq": migrated_project["last_event_seq"],
        "tree_revision": migrated_project["tree_revision"],
    }


def browser_acceptance(
    url: str,
    workspace: Path,
    build_dir: Path,
    temp_root: Path,
) -> None:
    script = temp_root / "installed-project-map-browser.js"
    script.write_text(
        r"""
const { chromium } = require('playwright');
const { execFileSync } = require('child_process');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

async function launchBrowser() {
  try {
    return await chromium.launch({ channel: 'chrome', headless: true, timeout: 15000 });
  } catch (_error) {
    return chromium.launch({ headless: true, timeout: 15000 });
  }
}

(async () => {
  const browser = await launchBrowser();
  const context = await browser.newContext({ viewport: { width: 1280, height: 800 } });
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  const externalRequests = [];
  page.on('request', (request) => {
    const target = new URL(request.url());
    if (target.hostname !== '127.0.0.1') externalRequests.push(request.url());
  });
  await page.goto(process.env.TREEWORK_PROJECT_MAP_URL, { waitUntil: 'domcontentloaded' });
  await page.locator('[data-node-id="installed-map"]').waitFor();
  assert(await page.locator('.branch-node').count() === 4, 'Map did not render the complete fresh Tree');
  await page.locator('[data-node-id="installed-map"]').click();
  await page.locator('#branchInspector').waitFor();
  assert((await page.locator('#branchInspector').innerText()).includes('Installed Map'), 'Inspector did not show branch detail');

  await page.getByRole('button', { name: 'Dependency', exact: true }).click();
  await page.locator('[data-node-id="installed-map"][data-node-role="focus"]').waitFor();
  assert(await page.locator('[data-edge="accepted-base:installed-map"][data-satisfied="true"]').count() === 1, 'Dependency prerequisite was not satisfied');
  assert(await page.locator('[data-node-id="downstream"][data-node-role="downstream"]').count() === 1, 'Dependency downstream impact is missing');

  await page.getByRole('button', { name: 'Replay', exact: true }).click();
  await page.locator('.replay-timeline').waitFor();
  await page.getByRole('button', { name: 'Previous transaction' }).click();
  const historicalSeq = Number(await page.locator('.replay-view').getAttribute('data-replay-seq'));
  execFileSync(
    process.env.TREEWORK_TW,
    ['pause', '--reason', 'Installed SSE acceptance'],
    {
      cwd: process.env.TREEWORK_WORKSPACE,
      env: {
        ...process.env,
        TREEWORK_PLUGIN_ROOT: process.env.TREEWORK_PLUGIN_ROOT,
        TREEWORK_BUILD_DIR: process.env.TREEWORK_BUILD_DIR
      },
      stdio: 'pipe'
    }
  );
  await page.getByRole('button', { name: 'Return to Live' }).waitFor();
  assert(
    Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) === historicalSeq,
    'SSE live advance moved the historical Replay cursor'
  );
  await page.getByRole('button', { name: 'Return to Live' }).click();
  await page.getByText('Live', { exact: true }).waitFor();
  assert(externalRequests.length === 0, `Project Map made external requests: ${externalRequests}`);
  await context.close();
  await browser.close();
})().catch((error) => {
  console.error(error && error.stack ? error.stack : error);
  process.exit(1);
});
""",
        encoding="utf-8",
    )
    env = os.environ.copy()
    env.update(
        {
            "TREEWORK_PROJECT_MAP_URL": url,
            "TREEWORK_WORKSPACE": str(workspace),
            "TREEWORK_TW": str(TW),
            "TREEWORK_PLUGIN_ROOT": str(PLUGIN_ROOT),
            "TREEWORK_BUILD_DIR": str(build_dir),
            "NODE_PATH": str(NODE_MODULES),
        }
    )
    result = subprocess.run(
        [str(NODE), str(script)],
        cwd=PLUGIN_ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=180,
        check=False,
    )
    if result.returncode != 0:
        fail(
            "installed Project Map browser journey failed\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )


def assert_fresh_api(url: str) -> dict[str, Any]:
    root = url.removesuffix("/project-map.html")
    status, projection = request_json(f"{root}/api/project-map")
    if status != 200:
        fail(f"fresh Project Map projection returned {status}: {projection}")
    ids = [node["id"] for node in projection.get("nodes", [])]
    if ids != ["root", "accepted-base", "installed-map", "downstream"]:
        fail(f"fresh Project Map returned unexpected nodes: {ids}")
    dependency = next(
        (
            edge
            for edge in projection.get("dependencies", [])
            if edge["from"] == "installed-map" and edge["to"] == "accepted-base"
        ),
        None,
    )
    if not dependency or dependency.get("satisfied") is not True:
        fail("fresh Project Map lost satisfied dependency state")
    query = urllib.parse.urlencode({"id": "installed-map"})
    detail_status, detail = request_json(f"{root}/api/project-map/branch?{query}")
    if detail_status != 200 or detail.get("branch", {}).get("id") != "installed-map":
        fail(f"fresh Project Map Inspector failed: {detail_status} {detail}")
    replay_status, replay = request_json(f"{root}/api/project-map/replay")
    if (
        replay_status != 200
        or replay.get("reconstruction", {}).get("status") != "available"
        or len(replay.get("transactions", [])) < 5
    ):
        fail(f"fresh Project Map Replay failed: {replay_status} {replay}")
    return {
        "tree_revision": projection["tree_revision"],
        "state_event_seq": projection["state_event_seq"],
        "node_count": len(ids),
        "dependency_count": len(projection["dependencies"]),
        "replay_transactions": len(replay["transactions"]),
    }


def assert_legacy_api(url: str) -> dict[str, Any]:
    root = url.removesuffix("/project-map.html")
    status, projection = request_json(f"{root}/api/project-map")
    if status != 200:
        fail(f"legacy Project Map projection returned {status}: {projection}")
    child = next(
        (node for node in projection.get("nodes", []) if node["id"] == "legacy-child"),
        None,
    )
    if (
        not child
        or child["status"] != "paused"
        or child["verification"] != "partial"
        or child["depends_on"] != ["legacy-base"]
    ):
        fail(f"legacy Project Map lost visible branch state: {child}")
    query = urllib.parse.urlencode({"id": "legacy-child"})
    detail_status, detail = request_json(f"{root}/api/project-map/branch?{query}")
    if (
        detail_status != 200
        or "Legacy progress remains visible"
        not in detail.get("progress", {}).get("current_reality", "")
    ):
        fail(f"legacy Inspector did not preserve documents: {detail}")
    replay_status, replay = request_json(f"{root}/api/project-map/replay")
    if replay_status != 200 or replay.get("meta", {}).get("live_event_seq") != 5:
        fail(f"legacy Replay did not preserve event sequence: {replay}")
    gaps = replay.get("reconstruction", {}).get("gaps", [])
    earliest = replay.get("meta", {}).get("earliest_replayable_seq")
    unsupported = [
        transaction
        for transaction in replay.get("transactions", [])
        if transaction.get("replayable") is False
    ]
    if not gaps and earliest in (None, 0, 1) and not unsupported:
        fail("legacy Replay did not report unsupported pre-checkpoint coverage")
    return {
        "tree_revision": projection["tree_revision"],
        "state_event_seq": projection["state_event_seq"],
        "reconstruction": replay["reconstruction"]["status"],
        "coverage_gaps": len(gaps),
        "earliest_replayable_seq": earliest,
        "unsupported_transactions": len(unsupported),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence-json", type=Path)
    args = parser.parse_args()
    if not NODE.is_file() or not NODE_MODULES.is_dir():
        fail("bundled Node/Playwright runtime is unavailable")

    temp_root = Path(tempfile.mkdtemp(prefix="treework-installed-map-"))
    build_dir = temp_root / ".build"
    fresh = temp_root / "fresh"
    second = temp_root / "second"
    legacy = temp_root / "legacy"
    client: McpClient | None = None
    owned_processes: list[int] = []
    try:
        prepare_fresh(fresh, build_dir)
        prepare_fresh(second, build_dir)
        migration = prepare_legacy(legacy, build_dir)
        fresh_before = accepted_state_snapshot(fresh)
        legacy_before = accepted_state_snapshot(legacy)

        client = McpClient(build_dir)
        fresh_launch = assert_tool_result(
            client.request(
                "tools/call",
                {
                    "name": "treework_project_map",
                    "arguments": {"workspace": str(fresh)},
                },
            ),
            "treework_project_map",
        )
        fresh_reuse = assert_tool_result(
            client.request(
                "tools/call",
                {
                    "name": "treework_project_map",
                    "arguments": {"workspace": str(fresh)},
                },
            ),
            "treework_project_map",
        )
        second_launch = assert_tool_result(
            client.request(
                "tools/call",
                {
                    "name": "treework_project_map",
                    "arguments": {"workspace": str(second)},
                },
            ),
            "treework_project_map",
        )
        legacy_launch = assert_tool_result(
            client.request(
                "tools/call",
                {
                    "name": "treework_project_map",
                    "arguments": {"workspace": str(legacy)},
                },
            ),
            "treework_project_map",
        )
        owned_processes = [
            fresh_launch["process_id"],
            second_launch["process_id"],
            legacy_launch["process_id"],
        ]
        if (
            fresh_reuse["status"] != "reused"
            or fresh_reuse["url"] != fresh_launch["url"]
            or fresh_reuse["process_id"] != fresh_launch["process_id"]
        ):
            fail("installed launcher did not reuse the fresh workspace process")
        if len({fresh_launch["url"], second_launch["url"], legacy_launch["url"]}) != 3:
            fail("installed launcher cross-contaminated workspace URLs")

        fresh_api = assert_fresh_api(fresh_launch["url"])
        legacy_api = assert_legacy_api(legacy_launch["url"])
        browser_acceptance(fresh_launch["url"], fresh, build_dir, temp_root)
        if accepted_state_snapshot(legacy) != legacy_before:
            fail("legacy Project Map launch changed accepted state")
        if fresh_before == accepted_state_snapshot(fresh):
            fail("installed browser journey did not exercise live SSE state changes")

        evidence = {
            "schema_version": 1,
            "plugin": plugin_identity(),
            "fresh": fresh_api,
            "legacy": {**migration, **legacy_api},
            "workspace_isolation": {
                "urls_unique": True,
                "same_workspace_reused": True,
                "owned_process_count": len(owned_processes),
            },
            "accepted_state_boundary": {
                "legacy_launch_unchanged": True,
                "fresh_launch_hash": fresh_launch["accepted_state_hash_after"],
            },
            "browser": {
                "map": "passed",
                "dependency": "passed",
                "replay": "passed",
                "inspector": "passed",
                "sse_live_advance": "passed",
                "return_to_live": "passed",
                "external_requests": 0,
            },
            "recorded_at_unix": int(time.time()),
        }
        encoded_evidence = json.dumps(evidence, ensure_ascii=False)
        if str(PLUGIN_ROOT) in encoded_evidence or str(Path.home()) in encoded_evidence:
            fail("installed acceptance evidence contains a host-specific absolute path")
        if args.evidence_json:
            args.evidence_json.parent.mkdir(parents=True, exist_ok=True)
            args.evidence_json.write_text(
                json.dumps(evidence, indent=2, ensure_ascii=False) + "\n",
                encoding="utf-8",
            )
        print(
            "ok: installed Project Map fresh, legacy, browser, and "
            "workspace-isolation acceptance"
        )
    finally:
        if client is not None:
            client.close()
        for process_id in owned_processes:
            wait_for_process_exit(process_id)
        shutil.rmtree(temp_root, ignore_errors=True)


if __name__ == "__main__":
    main()
