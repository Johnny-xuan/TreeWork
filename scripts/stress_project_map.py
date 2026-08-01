#!/usr/bin/env python3
"""Stress-check TreeWork Project Map rendering with synthetic state."""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any

from _paths import PLUGIN_ROOT

TW = PLUGIN_ROOT / "skills" / "treework" / "scripts" / "tw"


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def run_tw(workspace: Path, build_dir: Path, *args: str) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    env["TREEWORK_BUILD_DIR"] = str(build_dir)
    return subprocess.run(
        [str(TW), *args],
        cwd=workspace,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )


def make_synthetic_treework(workspace: Path, branch_count: int, relation_count: int) -> None:
    tw = workspace / ".TreeWork"
    state = tw / "state"
    state.mkdir(parents=True)
    (tw / "out").mkdir()
    (tw / "branches").mkdir()
    (tw / "PROJECT.md").write_text("# Project\n\nSynthetic large graph stress workspace.\n", encoding="utf-8")
    (tw / "progress.md").write_text("# Progress\n\n", encoding="utf-8")
    (tw / "events.jsonl").write_text("", encoding="utf-8")

    branches = [
        {
            "path": "root",
            "parent": "",
            "status": "in_progress",
            "verification_status": "unverified",
            "sync_status": "clean",
            "status_reason": "",
            "last_sync": "unix:0",
        }
    ]
    statuses = ["pending", "in_progress", "paused", "complete", "aborted"]
    verifications = ["unverified", "partial", "failed", "verified"]
    for idx in range(branch_count):
        status = statuses[idx % len(statuses)]
        verification = "verified" if status == "complete" else verifications[idx % len(verifications)]
        branches.append(
            {
                "path": f"branch-{idx:04d}",
                "parent": "root",
                "status": status,
                "verification_status": verification,
                "sync_status": "clean",
                "status_reason": "Synthetic retirement reason." if status == "aborted" else "",
                "last_sync": f"unix:{idx}",
            }
        )

    edges = []
    for idx in range(branch_count):
        branch = f"branch-{idx:04d}"
        edges.append(
            {
                "id": f"edge-parent-{idx:04d}",
                "from": "root",
                "to": branch,
                "kind": "parent_of",
                "user_label": f"root contains {branch}",
                "interpreted_relation": "parent_of",
            }
        )
    for idx in range(relation_count):
        source = idx % branch_count
        target = (idx * 17 + 23) % branch_count
        if target == source:
            target = (target + 1) % branch_count
        kind = "depends_on" if idx % 3 == 0 else "affects"
        edges.append(
            {
                "id": f"edge-rel-{idx:04d}",
                "from": f"branch-{source:04d}",
                "to": f"branch-{target:04d}",
                "kind": kind,
                "user_label": f"synthetic {kind} relation {idx}",
                "interpreted_relation": kind,
            }
        )

    write_json(
        state / "project.json",
        {
            "schema_version": "0.1",
            "stage": "work_tree",
            "current_branch": "root",
            "last_event_seq": 0,
            "last_sync": "unix:0",
        },
    )
    write_json(state / "branches.json", {"branches": branches})
    write_json(state / "graph.json", {"edges": edges})


def verify_projection(workspace: Path, expected_nodes: int, expected_edges: int) -> None:
    graph_path = workspace / ".TreeWork" / "out" / "graph.json"
    html_path = workspace / ".TreeWork" / "out" / "project-map.html"
    app_path = workspace / ".TreeWork" / "out" / "app.js"
    styles_path = workspace / ".TreeWork" / "out" / "styles.css"
    graph = json.loads(graph_path.read_text(encoding="utf-8"))
    meta = graph.get("meta")
    nodes = graph.get("nodes")
    edges = graph.get("edges")
    if not isinstance(meta, dict) or meta.get("current_branch") != "root":
        fail("expected graph meta.current_branch to be root")
    layout_meta = meta.get("layout")
    if not isinstance(layout_meta, dict) or layout_meta.get("algorithm") != "treework_tidy_v1":
        fail("expected graph meta.layout.algorithm to be treework_tidy_v1")
    if layout_meta.get("node_count") != expected_nodes or layout_meta.get("edge_count") != expected_edges:
        fail("expected layout meta node_count/edge_count to match projection size")
    if not isinstance(nodes, list) or len(nodes) != expected_nodes:
        fail(f"expected {expected_nodes} graph nodes, got {len(nodes) if isinstance(nodes, list) else 'invalid'}")
    if not isinstance(edges, list) or len(edges) != expected_edges:
        fail(f"expected {expected_edges} graph edges, got {len(edges) if isinstance(edges, list) else 'invalid'}")
    if "notes" in graph or "edit_commands" in graph:
        fail("read-only graph projection still contains note or edit-command state")
    coordinates: set[tuple[float, float]] = set()
    x_values: list[float] = []
    y_values: list[float] = []
    root_layout = None
    for node in nodes:
        layout = node.get("layout") if isinstance(node, dict) else None
        if not isinstance(layout, dict):
            fail("expected every graph node to include layout metadata")
        x = layout.get("x")
        y = layout.get("y")
        depth = layout.get("depth")
        order = layout.get("order")
        subtree_size = layout.get("subtree_size")
        if not isinstance(x, (int, float)) or not isinstance(y, (int, float)):
            fail("expected graph node layout x/y to be numeric")
        if not isinstance(depth, int) or depth < 0:
            fail("expected graph node layout depth to be a non-negative integer")
        if not isinstance(order, int) or order < 0:
            fail("expected graph node layout order to be a non-negative integer")
        if not isinstance(subtree_size, int) or subtree_size < 1:
            fail("expected graph node layout subtree_size to be positive")
        coordinates.add((round(float(x), 6), round(float(y), 6)))
        x_values.append(float(x))
        y_values.append(float(y))
        if node.get("id") == "root":
            root_layout = layout
    if root_layout is None or root_layout.get("depth") != 0:
        fail("expected root layout depth to be 0")
    if len(coordinates) < min(expected_nodes, 3):
        fail("expected layout coordinates not to collapse into a single point")
    if expected_nodes > 1 and (max(x_values) - min(x_values) <= 0 or max(y_values) - min(y_values) <= 0):
        fail("expected layout to have non-zero x/y span")

    html = html_path.read_text(encoding="utf-8")
    app = app_path.read_text(encoding="utf-8")
    styles = styles_path.read_text(encoding="utf-8")
    required_html = [
        "./styles.css",
        "./app.js",
        "id=\"root\"",
    ]
    for needle in required_html:
        if needle not in html:
            fail(f"project-map.html missing expected static asset wiring: {needle}")
    required_app = [
        "/api/project-map",
        "/api/project-map/events",
        "treework-project-map:v3",
        "projectMapSvg",
        "branchSearch",
        "statusFilter",
        "sessionAnnotation",
        "Dependency",
        "Replay",
    ]
    for needle in required_app:
        if needle not in app:
            fail(f"project-map app missing expected production behavior: {needle}")
    if (
        ".branch-inspector" not in styles
        or ".branch-node" not in styles
        or ".depth-guides" not in styles
        or "--tw-ember" not in styles
    ):
        fail("project-map styles should include the Strata V3 production surfaces")
    retired = [
        "window.treeworkGraph",
        "graphology",
        "sigma",
        "mermaid",
        "/api/graph",
    ]
    if any(value in html or value in app for value in retired):
        fail("project-map production frontend still contains retired graph runtime")
    if (
        "https://fonts." in html
        or "https://fonts." in styles
        or "src=\"http" in html
        or "href=\"http" in html
        or "url(http" in styles
    ):
        fail("project-map production frontend should not require network assets")
    font_path = html_path.parent / "vendor" / "fonts" / "fraunces-latin-500-normal.woff2"
    if not font_path.is_file():
        fail("project-map render did not copy locally bundled font assets")
def main() -> None:
    parser = argparse.ArgumentParser(description="Stress-check TreeWork Project Map rendering.")
    parser.add_argument("--branches", type=int, default=750, help="non-root branch count")
    parser.add_argument("--relations", type=int, default=1500, help="extra non-parent relationship edge count")
    parser.add_argument("--keep", action="store_true", help="keep the temporary workspace for inspection")
    args = parser.parse_args()

    if args.branches < 1:
        fail("--branches must be positive")
    if args.relations < 0:
        fail("--relations cannot be negative")
    if not TW.is_file():
        fail(f"missing tw wrapper at {TW}")

    temp_root = Path(tempfile.mkdtemp(prefix="treework-map-stress-"))
    build_dir = temp_root / ".build"
    workspace = temp_root / "workspace"
    workspace.mkdir()
    try:
        make_synthetic_treework(workspace, args.branches, args.relations)
        expected_nodes = args.branches + 1
        expected_edges = args.branches + args.relations

        started = time.perf_counter()
        for command in [("graph", "render"), ("check",)]:
            result = run_tw(workspace, build_dir, *command)
            if result.returncode != 0:
                fail(
                    f"tw {' '.join(command)} failed:\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}"
                )
        elapsed_ms = int((time.perf_counter() - started) * 1000)
        verify_projection(workspace, expected_nodes, expected_edges)

        graph_size = (workspace / ".TreeWork" / "out" / "graph.json").stat().st_size
        html_size = (workspace / ".TreeWork" / "out" / "project-map.html").stat().st_size
        print(
            "ok: project map stress "
            f"nodes={expected_nodes} edges={expected_edges} "
            f"elapsed_ms={elapsed_ms} graph_bytes={graph_size} html_bytes={html_size}"
        )
        if args.keep:
            print(f"kept: {workspace}")
    finally:
        if not args.keep:
            shutil.rmtree(temp_root, ignore_errors=True)


if __name__ == "__main__":
    main()
