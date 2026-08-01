#!/usr/bin/env python3
"""Measure production Project Map browser interactions and enforce calibrated ceilings."""

from __future__ import annotations

import argparse
import json
import math
import os
import platform
import re
import shutil
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Any

from _paths import PLUGIN_ROOT, REPOSITORY_ROOT

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
URL_PATTERN = re.compile(r"http://127\.0\.0\.1:(\d+)/project-map\.html")
DEFAULT_LIMITS = REPOSITORY_ROOT / "scripts" / "project_map_performance_limits.json"
METRIC_FLOORS_MS = {
    "initial_render": 8_000,
    "map_to_dependency": 4_000,
    "dependency_focus": 3_000,
    "dependency_depth_expand": 3_000,
    "dependency_to_map": 3_000,
    "map_search": 2_000,
    "map_collapse": 2_000,
    "map_focus": 2_000,
    "map_pan": 1_000,
    "map_zoom": 1_000,
    "map_to_replay": 5_000,
    "replay_seek": 4_000,
}
CALIBRATION_MULTIPLIER = 4.0


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
) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(
        [str(TW), *args],
        cwd=workspace,
        env=environment(build_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=300,
        check=False,
    )
    if result.returncode != 0:
        fail(
            f"tw {' '.join(args)} failed with {result.returncode}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def append_node(
    lines: list[str],
    indent: int,
    branch_id: str,
    title: str,
    purpose: str,
    dependencies: list[str] | None = None,
) -> None:
    prefix = " " * indent
    field = " " * (indent + 2)
    lines.extend(
        [
            f"{prefix}- id: {branch_id}",
            f"{field}title: {title}",
            f"{field}purpose: {purpose}",
        ]
    )
    if dependencies:
        lines.append(f"{field}depends_on:")
        lines.extend(f"{field}  - {dependency}" for dependency in dependencies)


def performance_tree() -> str:
    lines = [
        "version: 1",
        "tree:",
        "  id: root",
        "  title: Performance Foundation",
        "  purpose: Exercise deep, wide, dependency-heavy, and Replay-rich behavior.",
        "  spec: spec.md",
        "  children:",
    ]
    append_node(
        lines,
        4,
        "deep-000",
        "Deep Level 0",
        "Begin the supported deep hierarchy fixture.",
    )
    lines.append("      children:")
    for index in range(1, 40):
        indent = 8 + (index - 1) * 4
        append_node(
            lines,
            indent,
            f"deep-{index:03d}",
            f"Deep Level {index}",
            f"Exercise depth column {index}.",
        )
        if index < 39:
            lines.append(" " * (indent + 2) + "children:")

    for index in range(560):
        append_node(
            lines,
            4,
            f"wide-{index:03d}",
            f"Wide Branch {index}",
            "Exercise stable wide sibling layout and search.",
        )

    for index in range(150):
        dependencies = []
        if index:
            dependencies.append(f"dep-{index - 1:03d}")
        wide_needed = 15 - len(dependencies)
        dependencies.extend(
            f"wide-{(index * 17 + offset * 31) % 560:03d}"
            for offset in range(wide_needed)
        )
        append_node(
            lines,
            4,
            f"dep-{index:03d}",
            f"Dependency Branch {index}",
            "Exercise focused causal traversal and fan-in.",
            dependencies,
        )
    return "\n".join(lines) + "\n"


def prepare_workspace(workspace: Path, build_dir: Path) -> None:
    workspace.mkdir()
    run_tw(workspace, build_dir, "init")
    run_tw(workspace, build_dir, "align", "end")
    run_tw(workspace, build_dir, "tree", "start")
    (workspace / ".TreeWork" / "tree.yaml").write_text(
        performance_tree(),
        encoding="utf-8",
    )
    run_tw(workspace, build_dir, "tree", "apply")


def prepare_replay_workspace(workspace: Path, build_dir: Path) -> None:
    workspace.mkdir()
    run_tw(workspace, build_dir, "init")
    run_tw(workspace, build_dir, "align", "end")
    run_tw(workspace, build_dir, "tree", "start")
    lines = [
        "version: 1",
        "tree:",
        "  id: root",
        "  title: Replay Performance",
        "  purpose: Exercise a transaction-rich Replay catalog.",
        "  spec: spec.md",
        "  children:",
    ]
    for index in range(30):
        append_node(
            lines,
            4,
            f"replay-{index:03d}",
            f"Replay Branch {index}",
            "Exercise transaction seeking and historical reconstruction.",
        )
    (workspace / ".TreeWork" / "tree.yaml").write_text(
        "\n".join(lines) + "\n",
        encoding="utf-8",
    )
    run_tw(workspace, build_dir, "tree", "apply")
    for index in range(40):
        branch = f"replay-{index % 30:03d}"
        run_tw(workspace, build_dir, "enter", branch, "--no-isolate")
        run_tw(
            workspace,
            build_dir,
            "pause",
            "--reason",
            f"Replay performance event {index}",
        )
    run_tw(workspace, build_dir, "enter", "replay-000", "--no-isolate")


def start_server(
    workspace: Path,
    build_dir: Path,
) -> tuple[subprocess.Popen[str], str]:
    process = subprocess.Popen(
        [str(TW), "graph", "serve", "--port", "0"],
        cwd=workspace,
        env=environment(build_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdout is not None
    deadline = time.monotonic() + 120
    lines: list[str] = []
    while time.monotonic() < deadline:
        line = process.stdout.readline()
        if line:
            lines.append(line)
            match = URL_PATTERN.search(line)
            if match:
                return process, match.group(0)
        elif process.poll() is not None:
            break
    process.terminate()
    stdout, stderr = process.communicate(timeout=10)
    fail(
        "Project Map server did not report a URL\n"
        f"stdout:\n{''.join(lines)}{stdout}\nstderr:\n{stderr}"
    )


def stop_server(process: subprocess.Popen[str]) -> None:
    if process.poll() is not None:
        return
    process.terminate()
    try:
        process.wait(timeout=8)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def browser_metrics(
    url: str,
    temp_root: Path,
) -> dict[str, float]:
    script = temp_root / "project-map-performance.js"
    script.write_text(
        r"""
const { chromium } = require('playwright');

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

async function settled(page) {
  await page.evaluate(() => new Promise((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(resolve))
  ));
}

async function measure(metrics, name, action) {
  const start = performance.now();
  await action();
  await settled(globalThis.performancePage);
  metrics[name] = Math.round((performance.now() - start) * 100) / 100;
}

(async () => {
  const browser = await launchBrowser();
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    reducedMotion: 'reduce'
  });
  const page = await context.newPage();
  globalThis.performancePage = page;
  page.setDefaultTimeout(20000);
  const metrics = {};

  const initialStart = performance.now();
  await page.goto(process.env.TREEWORK_PROJECT_MAP_URL, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => document.querySelectorAll('.branch-node').length === 751);
  await settled(page);
  metrics.initial_render = Math.round((performance.now() - initialStart) * 100) / 100;
  assert(await page.locator('#projectMapSvg').count() === 1, 'initial SVG is blank');
  const initialVisibility = await page.evaluate(() => {
    const surface = document.querySelector('#mapSurface')?.getBoundingClientRect();
    const visibleNodes = [...document.querySelectorAll('.branch-node')].filter((node) => {
      const rect = node.getBoundingClientRect();
      const style = getComputedStyle(node);
      return (
        surface &&
        style.display !== 'none' &&
        style.visibility !== 'hidden' &&
        rect.width > 1 &&
        rect.height > 1 &&
        rect.right > surface.left &&
        rect.left < surface.right &&
        rect.bottom > surface.top &&
        rect.top < surface.bottom
      );
    });
    return {
      visibleNodes: visibleNodes.length,
      surface: surface
        ? { width: surface.width, height: surface.height }
        : null
    };
  });
  assert(
    initialVisibility.visibleNodes > 0,
    `large initial render has no visible node pixels: ${JSON.stringify(initialVisibility)}`
  );
  const initialSearch = page.getByLabel('Search branches');
  await initialSearch.fill('Dependency Branch 75');
  await initialSearch.press('Enter');
  await page.locator('[data-node-id="dep-075"][aria-selected="true"]').waitFor();
  await page.locator('#branchInspector').waitFor();
  await initialSearch.fill('');

  await measure(metrics, 'map_to_dependency', async () => {
    await page.getByRole('button', { name: 'Dependency', exact: true }).click();
    await page.locator('[data-node-id="dep-075"][data-node-role="focus"]').waitFor();
  });
  await measure(metrics, 'dependency_focus', async () => {
    const search = page.getByLabel('Search branches');
    await search.fill('Dependency Branch 74');
    await search.press('Enter');
    await page.locator('[data-node-id="dep-074"][data-node-role="focus"]').waitFor();
  });
  await measure(metrics, 'dependency_depth_expand', async () => {
    await page.getByRole('button', { name: 'Expand upstream depth' }).click();
    await page.getByRole('button', { name: 'Expand downstream depth' }).click();
    await page.locator('[data-node-id="dep-072"][data-node-role="upstream"]').waitFor();
    await page.locator('[data-node-id="dep-076"][data-node-role="downstream"]').waitFor();
  });
  await measure(metrics, 'dependency_to_map', async () => {
    await page.getByRole('button', { name: 'Map', exact: true }).click();
    await page.waitForFunction(() => document.querySelectorAll('.branch-node').length === 751);
  });
  await measure(metrics, 'map_search', async () => {
    const search = page.getByLabel('Search branches');
    await search.fill('Wide Branch 559');
    await search.press('Enter');
    await page.locator('[data-node-id="wide-559"][aria-selected="true"]').waitFor();
  });
  const mapSearch = page.getByLabel('Search branches');
  await mapSearch.fill('Deep Level 0');
  await mapSearch.press('Enter');
  await page.locator('[data-node-id="deep-000"][aria-selected="true"]').waitFor();
  await mapSearch.fill('');
  await measure(metrics, 'map_collapse', async () => {
    await page.locator('[data-node-id="deep-000"] .collapse-control').click();
    await page.waitForFunction(() => document.querySelectorAll('.branch-node').length === 712);
  });
  await measure(metrics, 'map_focus', async () => {
    await mapSearch.fill('Dependency Branch 75');
    await mapSearch.press('Enter');
    await page.locator('[data-node-id="dep-075"][aria-selected="true"]').waitFor();
    await page.locator('#branchInspector').waitFor();
  });
  await mapSearch.fill('');
  await page.getByRole('button', { name: 'Close branch inspector' }).click();
  const panTransformBefore = await page.locator('#projectMapSvg .map-content').getAttribute('transform');
  await measure(metrics, 'map_pan', async () => {
    await page.locator('#mapSurface').dispatchEvent('wheel', {
      deltaX: 32,
      deltaY: 72,
      bubbles: true,
      cancelable: true
    });
    await page.waitForFunction(
      (before) =>
        document.querySelector('#projectMapSvg .map-content')?.getAttribute('transform') !== before,
      panTransformBefore
    );
  });
  const panTransformAfter = await page.locator('#projectMapSvg .map-content').getAttribute('transform');
  assert(panTransformAfter !== panTransformBefore, 'map pan did not change the viewport transform');
  const zoomTransformBefore = panTransformAfter;
  await measure(metrics, 'map_zoom', async () => {
    await page.locator('.canvas-tools button[aria-label="Zoom in"]').click();
    await page.waitForFunction(
      (before) =>
        document.querySelector('#projectMapSvg .map-content')?.getAttribute('transform') !== before,
      zoomTransformBefore
    );
  });
  const zoomTransformAfter = await page.locator('#projectMapSvg .map-content').getAttribute('transform');
  assert(zoomTransformAfter !== zoomTransformBefore, 'map zoom did not change the viewport transform');

  console.log(JSON.stringify(metrics));
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
    env["TREEWORK_PROJECT_MAP_URL"] = url
    env["NODE_PATH"] = str(NODE_MODULES)
    result = subprocess.run(
        [str(NODE), str(script)],
        cwd=PLUGIN_ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=300,
        check=False,
    )
    if result.returncode != 0:
        fail(
            "Project Map performance browser run failed\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    try:
        return {
            key: float(value)
            for key, value in json.loads(result.stdout.strip()).items()
        }
    except (json.JSONDecodeError, ValueError) as error:
        fail(f"performance browser output is invalid: {error}\n{result.stdout}")


def replay_browser_metrics(
    url: str,
    temp_root: Path,
) -> dict[str, float]:
    script = temp_root / "project-map-replay-performance.js"
    script.write_text(
        r"""
const { chromium } = require('playwright');

async function launchBrowser() {
  try {
    return await chromium.launch({ channel: 'chrome', headless: true, timeout: 15000 });
  } catch (_error) {
    return chromium.launch({ headless: true, timeout: 15000 });
  }
}

async function settled(page) {
  await page.evaluate(() => new Promise((resolve) =>
    requestAnimationFrame(() => requestAnimationFrame(resolve))
  ));
}

(async () => {
  const browser = await launchBrowser();
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    reducedMotion: 'reduce'
  });
  const page = await context.newPage();
  page.setDefaultTimeout(20000);
  await page.goto(process.env.TREEWORK_PROJECT_MAP_URL, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => document.querySelectorAll('.branch-node').length === 31);

  const switchStart = performance.now();
  await page.getByRole('button', { name: 'Replay', exact: true }).click();
  await page.getByLabel('Replay timeline').waitFor();
  await page.locator('.replay-stage #projectMapSvg').waitFor();
  await settled(page);
  const mapToReplay = performance.now() - switchStart;

  const range = page.getByLabel('Replay transaction');
  const maximum = Number(await range.getAttribute('max'));
  if (maximum < 80) throw new Error(`Replay fixture has only ${maximum + 1} transactions`);
  const seekStart = performance.now();
  await range.fill(String(Math.floor(maximum / 2)));
  await page.waitForFunction(() => {
    const view = document.querySelector('.replay-view');
    return view && view.getAttribute('data-replay-live') === 'false';
  });
  await page.locator('.replay-stage #projectMapSvg').waitFor();
  await settled(page);
  const replaySeek = performance.now() - seekStart;

  console.log(JSON.stringify({
    map_to_replay: Math.round(mapToReplay * 100) / 100,
    replay_seek: Math.round(replaySeek * 100) / 100
  }));
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
    env["TREEWORK_PROJECT_MAP_URL"] = url
    env["NODE_PATH"] = str(NODE_MODULES)
    result = subprocess.run(
        [str(NODE), str(script)],
        cwd=PLUGIN_ROOT,
        env=env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        timeout=300,
        check=False,
    )
    if result.returncode != 0:
        fail(
            "Project Map Replay performance run failed\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    try:
        return {
            key: float(value)
            for key, value in json.loads(result.stdout.strip()).items()
        }
    except (json.JSONDecodeError, ValueError) as error:
        fail(f"Replay performance output is invalid: {error}\n{result.stdout}")


def machine_metadata() -> dict[str, Any]:
    return {
        "platform": platform.platform(),
        "machine": platform.machine(),
        "python": platform.python_version(),
        "cpu_count": os.cpu_count(),
    }


def percentile(values: list[float], fraction: float) -> float:
    ordered = sorted(values)
    index = max(0, min(len(ordered) - 1, math.ceil(len(ordered) * fraction) - 1))
    return ordered[index]


def calibrated_limits(samples: list[dict[str, float]]) -> dict[str, int]:
    limits: dict[str, int] = {}
    for metric, floor in METRIC_FLOORS_MS.items():
        observed = percentile([sample[metric] for sample in samples], 0.95)
        limits[metric] = int(
            math.ceil(max(float(floor), observed * CALIBRATION_MULTIPLIER) / 100)
            * 100
        )
    return limits


def write_evidence(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--mode",
        choices=["calibrate", "verify"],
        required=True,
    )
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--limits", type=Path, default=DEFAULT_LIMITS)
    parser.add_argument("--runs", type=int, default=3)
    args = parser.parse_args()
    if not NODE.is_file() or not NODE_MODULES.is_dir():
        fail("bundled Node/Playwright runtime is unavailable")
    if args.runs < 1:
        fail("--runs must be positive")
    if args.mode == "verify" and not args.limits.is_file():
        fail(f"performance limits file is unavailable: {args.limits}")

    temp_root = Path(tempfile.mkdtemp(prefix="treework-map-performance-"))
    workspace = temp_root / "workspace"
    replay_workspace = temp_root / "replay-workspace"
    build_dir = temp_root / ".build"
    server: subprocess.Popen[str] | None = None
    try:
        prepare_started = time.perf_counter()
        prepare_workspace(workspace, build_dir)
        fixture_prepare_ms = round(
            (time.perf_counter() - prepare_started) * 1000,
            2,
        )
        server, url = start_server(workspace, build_dir)
        map_samples = [browser_metrics(url, temp_root) for _ in range(args.runs)]
        stop_server(server)
        server = None
        replay_prepare_started = time.perf_counter()
        prepare_replay_workspace(replay_workspace, build_dir)
        replay_prepare_ms = round(
            (time.perf_counter() - replay_prepare_started) * 1000,
            2,
        )
        server, replay_url = start_server(replay_workspace, build_dir)
        replay_samples = [
            replay_browser_metrics(replay_url, temp_root)
            for _ in range(args.runs)
        ]
        samples = [
            {**map_sample, **replay_sample}
            for map_sample, replay_sample in zip(map_samples, replay_samples)
        ]
        base = {
            "schema_version": 1,
            "mode": args.mode,
            "machine": machine_metadata(),
            "fixture": {
                "nodes": 751,
                "dependencies": 2250,
                "deep_levels": 40,
                "wide_siblings": 560,
                "dependency_nodes": 150,
                "replay_nodes": 31,
                "replay_transactions_minimum": 85,
                "large_fixture_prepare_ms": fixture_prepare_ms,
                "replay_fixture_prepare_ms": replay_prepare_ms,
            },
            "samples_ms": samples,
            "calibration": {
                "runs": len(samples),
                "percentile": "p95-nearest-rank",
                "multiplier": CALIBRATION_MULTIPLIER,
                "minimum_floors_ms": METRIC_FLOORS_MS,
            },
            "recorded_at_unix": int(time.time()),
        }
        if args.mode == "calibrate":
            limits = calibrated_limits(samples)
            evidence = {
                **base,
                "ceilings_ms": limits,
                "result": "calibrated",
            }
        else:
            limits_source = json.loads(args.limits.read_text(encoding="utf-8"))
            limits = limits_source.get("ceilings_ms")
            if not isinstance(limits, dict):
                fail("limits file has no ceilings_ms object")
            measured = {
                metric: percentile([sample[metric] for sample in samples], 0.95)
                for metric in METRIC_FLOORS_MS
            }
            failures = {
                metric: {
                    "measured_ms": measured[metric],
                    "ceiling_ms": limits.get(metric),
                }
                for metric in measured
                if not isinstance(limits.get(metric), (int, float))
                or measured[metric] > limits[metric]
            }
            evidence = {
                **base,
                "limits_source": (
                    args.limits.resolve()
                    .relative_to(REPOSITORY_ROOT.resolve())
                    .as_posix()
                    if args.limits.resolve().is_relative_to(REPOSITORY_ROOT.resolve())
                    else f"external:{args.limits.name}"
                ),
                "ceilings_ms": limits,
                "measured_p95_ms": measured,
                "failures": failures,
                "result": "passed" if not failures else "failed",
            }
            write_evidence(args.output, evidence)
            if failures:
                fail(f"Project Map performance ceilings failed: {failures}")
        write_evidence(args.output, evidence)
        print(
            f"ok: Project Map performance {args.mode} "
            f"result={evidence['result']} evidence={args.output}"
        )
    finally:
        if server is not None:
            stop_server(server)
        shutil.rmtree(temp_root, ignore_errors=True)


if __name__ == "__main__":
    main()
