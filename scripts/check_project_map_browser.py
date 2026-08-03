#!/usr/bin/env python3
"""Real-server browser acceptance for the production Project Map."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
import tempfile
from pathlib import Path

from _paths import PLUGIN_ROOT, REPOSITORY_ROOT, branch_artifact_dir

TW = PLUGIN_ROOT / "skills" / "treework" / "scripts" / "tw"
BUNDLED_RUNTIME = (
    Path.home()
    / ".cache"
    / "codex-runtimes"
    / "codex-primary-runtime"
    / "dependencies"
)
NODE = Path(os.environ.get("TREEWORK_NODE", BUNDLED_RUNTIME / "node" / "bin" / "node"))
NODE_MODULES = Path(
    os.environ.get("TREEWORK_NODE_PATH", BUNDLED_RUNTIME / "node" / "node_modules")
)
EVIDENCE_DIR = Path(
    os.environ.get(
        "TREEWORK_EVIDENCE_DIR",
        REPOSITORY_ROOT
        / ".TreeWork"
        / "branches"
        / "project-map-hardening"
        / "evidence",
    )
)


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def environment(build_dir: Path) -> dict[str, str]:
    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    env["TREEWORK_BUILD_DIR"] = str(build_dir)
    return env


def run_tw(workspace: Path, build_dir: Path, *args: str) -> None:
    result = subprocess.run(
        [str(TW), *args],
        cwd=workspace,
        env=environment(build_dir),
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        timeout=180,
    )
    if result.returncode != 0:
        fail(
            f"tw {' '.join(args)} failed\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )


def tree_source(*, include_late_branch: bool) -> str:
    late_branch = """
        - id: ui-late
          title: Late Topology Branch
          purpose: Prove topology invalidation and viewport anchoring.
          spec: branches/ui-parent/ui-late/spec.md
""" if include_late_branch else ""
    return f"""version: 1
tree:
  id: root
  title: Browser Acceptance
  purpose: Coordinate the production Project Map browser acceptance.
  spec: spec.md
  children:
    - id: foundation
      title: Foundation Layer
      purpose: Hold lifecycle examples without changing the current route.
      children:
        - id: done-leaf
          title: Accepted Foundation
          purpose: Represent settled and verified work.
          spec: branches/foundation/done-leaf/spec.md
        - id: shared-base
          title: Shared Accepted Base
          purpose: Prove minimum-distance fan-in deduplication.
          spec: branches/foundation/shared-base/spec.md
        - id: paused-leaf
          title: Parked Foundation
          purpose: Represent paused work using pattern and type.
        - id: aborted-leaf
          title: Retired Foundation
          purpose: Represent an intentionally aborted branch.
    - id: ui-parent
      title: Project Map Interface
      purpose: Own the production read-only interaction surface.
      spec: branches/ui-parent/spec.md
      children:
        - id: ui-source
          title: Strata Map View
          purpose: Render the current depth-column manuscript view.
          spec: branches/ui-parent/ui-source/spec.md
          depends_on:
            - done-leaf
          children:
            - id: map-toolbar
              title: Restrained Controls
              purpose: Search, filter, locate, fit, pan, and zoom.
            - id: map-inspector
              title: Branch Inspector
              purpose: Read real branch narrative and dependencies.
            - id: map-responsive
              title: Responsive Surface
              purpose: Preserve useful hierarchy on narrow screens.
              children:
                - id: mobile-pass
                  title: Mobile Acceptance
                  purpose: Verify the compact toolbar and bottom Inspector.
        - id: ui-other
          title: Dependency View
          purpose: Reserved sibling for the next production view.
        - id: dependency-focus
          title: Focused Causal Manuscript
          purpose: Explain direct and transitive prerequisites and dependents.
          spec: branches/ui-parent/dependency-focus/spec.md
          depends_on:
            - done-leaf
            - ui-source
            - prereq-a
            - prereq-b
          children:
            - id: nested-ready
              title: Nested Ready Scope
              purpose: Prove nested scope is excluded from parallel candidates.
{late_branch}    - id: ready-leaf
      title: Ready Follow-up
      purpose: Show a pending branch whose prerequisite is complete.
      depends_on:
        - done-leaf
    - id: waiting-leaf
      title: Waiting Follow-up
      purpose: Show a pending branch blocked by current work.
      depends_on:
        - ui-source
    - id: prereq-a
      title: Fan In Prerequisite A
      purpose: Reach the shared accepted base through one upstream path.
      depends_on:
        - shared-base
    - id: prereq-b
      title: Fan In Prerequisite B
      purpose: Reach the shared accepted base through another upstream path.
      depends_on:
        - shared-base
    - id: down-one
      title: Downstream Fan Out One
      purpose: Depend directly on the focused causal branch.
      depends_on:
        - dependency-focus
    - id: down-two
      title: Downstream Fan Out Two
      purpose: Prove direct downstream fan-out.
      depends_on:
        - dependency-focus
    - id: down-deep
      title: Transitive Downstream
      purpose: Prove downstream level expansion.
      depends_on:
        - down-one
    - id: parallel-ready
      title: Structurally Separate Ready Work
      purpose: Show advisory dependency-independent opportunity.
"""


def complete_fixture_branch(
    workspace: Path,
    build_dir: Path,
    branch_id: str,
) -> None:
    run_tw(workspace, build_dir, "enter", branch_id, "--no-isolate")
    plan = branch_artifact_dir(workspace, branch_id) / "task_plan.md"
    plan.write_text(
        plan.read_text(encoding="utf-8").replace("- [ ]", "- [x]"),
        encoding="utf-8",
    )
    run_tw(
        workspace,
        build_dir,
        "verify",
        "--cmd",
        f"{branch_id} browser fixture verification",
        "--result",
        "passed",
        "--gap",
        "none",
    )
    run_tw(workspace, build_dir, "complete")


def prepare_workspace(workspace: Path, build_dir: Path) -> None:
    run_tw(workspace, build_dir, "init")
    run_tw(workspace, build_dir, "align", "end")
    run_tw(workspace, build_dir, "tree", "start")
    (workspace / ".TreeWork" / "tree.yaml").write_text(
        tree_source(include_late_branch=False),
        encoding="utf-8",
    )
    run_tw(workspace, build_dir, "tree", "apply")

    complete_fixture_branch(workspace, build_dir, "done-leaf")
    complete_fixture_branch(workspace, build_dir, "shared-base")

    run_tw(workspace, build_dir, "enter", "paused-leaf", "--no-isolate")
    run_tw(
        workspace,
        build_dir,
        "pause",
        "--reason",
        "Waiting for browser acceptance review",
    )

    run_tw(workspace, build_dir, "enter", "aborted-leaf", "--no-isolate")
    run_tw(
        workspace,
        build_dir,
        "abort",
        "--reason",
        "Superseded fixture path",
    )
    run_tw(workspace, build_dir, "enter", "ui-source", "--no-isolate")


def main() -> None:
    if not NODE.is_file():
        fail(f"missing bundled Node runtime at {NODE}")
    if not NODE_MODULES.is_dir():
        fail(f"missing bundled Node modules at {NODE_MODULES}")

    temp_root = Path(tempfile.mkdtemp(prefix="treework-project-map-browser-"))
    workspace = temp_root / "workspace"
    build_dir = temp_root / ".build"
    updated_tree = temp_root / "updated-tree.yaml"
    workspace.mkdir()
    EVIDENCE_DIR.mkdir(parents=True, exist_ok=True)
    updated_tree.write_text(
        tree_source(include_late_branch=True),
        encoding="utf-8",
    )
    server: subprocess.Popen[str] | None = None
    try:
        prepare_workspace(workspace, build_dir)
        branches_path = workspace / ".TreeWork" / "state" / "branches.json"
        before_state = branches_path.read_bytes()

        server = subprocess.Popen(
            [str(TW), "graph", "serve", "--port", "0"],
            cwd=workspace,
            env=environment(build_dir),
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        assert server.stdout is not None
        first_line = server.stdout.readline()
        match = re.search(
            r"http://127\.0\.0\.1:(\d+)/project-map\.html",
            first_line,
        )
        if not match:
            stderr = server.stderr.read() if server.stderr else ""
            fail(
                "graph serve did not print URL\n"
                f"stdout:{first_line}\nstderr:{stderr}"
            )
        url = f"http://127.0.0.1:{match.group(1)}/project-map.html"

        smoke_js = temp_root / "project_map_browser_acceptance.js"
        smoke_js.write_text(
            r"""
const { chromium } = require('playwright');
const { execFileSync } = require('child_process');
const fs = require('fs');
const path = require('path');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function sameJson(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

async function waitFor(predicate, description, timeoutMs = 12000) {
  const deadline = Date.now() + timeoutMs;
  let lastError = null;
  while (Date.now() < deadline) {
    try {
      if (await predicate()) return;
    } catch (error) {
      lastError = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 60));
  }
  throw new Error(`Timed out waiting for ${description}${lastError ? `: ${lastError.message}` : ''}`);
}

function runTw(...args) {
  execFileSync(process.env.TREEWORK_TW, args, {
    cwd: process.env.TREEWORK_WORKSPACE,
    env: process.env,
    stdio: 'pipe'
  });
}

async function rawGeometry(page) {
  return page.locator('.branch-node').evaluateAll((elements) =>
    Object.fromEntries(elements.map((element) => [
      element.dataset.nodeId,
      {
        depth: Number(element.dataset.depth),
        transform: element.getAttribute('transform'),
        status: element.dataset.status,
        readiness: element.dataset.readiness,
        verification: element.dataset.verification
      }
    ]))
  );
}

async function rawDependencyGeometry(page) {
  return page.locator('.dependency-content .branch-node').evaluateAll((elements) =>
    Object.fromEntries(elements.map((element) => [
      element.dataset.nodeId,
      {
        role: element.dataset.nodeRole,
        depth: Number(element.dataset.depth),
        transform: element.getAttribute('transform'),
        status: element.dataset.status,
        readiness: element.dataset.readiness,
        verification: element.dataset.verification
      }
    ]))
  );
}

async function screenCenter(page, id) {
  return page.locator(`[data-node-id="${id}"]`).evaluate((element) => {
    const rect = element.getBoundingClientRect();
    return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 };
  });
}

function overlaps(left, right, allowance = 0) {
  return (
    left.left < right.right - allowance &&
    left.right > right.left + allowance &&
    left.top < right.bottom - allowance &&
    left.bottom > right.top + allowance
  );
}

async function visibleRect(page, selector) {
  return page.locator(selector).evaluate((element) => {
    const style = getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return {
      left: rect.left,
      right: rect.right,
      top: rect.top,
      bottom: rect.bottom,
      width: rect.width,
      height: rect.height,
      visible:
        style.display !== 'none' &&
        style.visibility !== 'hidden' &&
        Number(style.opacity) > 0.01
    };
  });
}

async function selectReplaySequence(page, transactions, sequence) {
  const index = transactions.findIndex((transaction) => transaction.seq === sequence);
  assert(index >= 0, `Replay catalog is missing sequence ${sequence}`);
  await page.getByLabel('Replay transaction').fill(String(index));
  await waitFor(
    async () => Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) === sequence,
    `Replay cursor at sequence ${sequence}`
  );
}

async function captureViewportSet(context, viewport, suffix) {
  const evidencePage = await context.newPage();
  const errors = [];
  evidencePage.setDefaultTimeout(12000);
  evidencePage.on('console', (message) => {
    if (message.type() === 'error') errors.push(`console: ${message.text()}`);
  });
  evidencePage.on('pageerror', (error) => errors.push(`page: ${error.message}`));
  await evidencePage.setViewportSize(viewport);

  const inspect = async (view, surfaceSelector, contentSelector) => {
    const layout = await evidencePage.evaluate(({ surfaceSelector, view }) => {
      const rect = (selector) => {
        const value = document.querySelector(selector)?.getBoundingClientRect();
        return value
          ? {
              left: value.left,
              right: value.right,
              top: value.top,
              bottom: value.bottom,
              width: value.width,
              height: value.height
            }
          : null;
      };
      const clippedButtons = [...document.querySelectorAll('button')]
        .filter((button) => {
          const style = getComputedStyle(button);
          return (
            style.display !== 'none' &&
            style.visibility !== 'hidden' &&
            button.textContent?.trim() &&
            (button.scrollWidth > button.clientWidth + 2 ||
              button.scrollHeight > button.clientHeight + 2)
          );
        })
        .map((button) => button.getAttribute('aria-label') || button.textContent.trim());
      return {
        view,
        viewport: { width: innerWidth, height: innerHeight },
        topbar: rect('.topbar'),
        surface: rect(surfaceSelector),
        horizontalOverflow: document.documentElement.scrollWidth - innerWidth,
        clippedButtons
      };
    }, { surfaceSelector, view });
    const scene = await evidencePage.locator(contentSelector).evaluate((content) => {
      const box = content.getBBox();
      return {
        width: box.width,
        height: box.height,
        nodes: content.querySelectorAll('.branch-node').length
      };
    });
    assert(
      layout.horizontalOverflow <= 1,
      `${suffix} ${view} has horizontal overflow: ${JSON.stringify(layout)}`
    );
    assert(
      layout.clippedButtons.length === 0,
      `${suffix} ${view} clips controls: ${JSON.stringify(layout.clippedButtons)}`
    );
    assert(
      layout.surface &&
      layout.surface.height >= 120 &&
      layout.topbar &&
      layout.surface.top >= layout.topbar.bottom - 1 &&
      layout.surface.bottom <= layout.viewport.height + 1,
      `${suffix} ${view} has invalid surface geometry: ${JSON.stringify(layout)}`
    );
    assert(
      scene.nodes >= 8 && scene.width > 500 && scene.height > 250,
      `${suffix} ${view} scene is blank or incomplete: ${JSON.stringify(scene)}`
    );
    return { ...layout, scene };
  };

  try {
    await evidencePage.goto(process.env.TREEWORK_PROJECT_MAP_URL, {
      waitUntil: 'domcontentloaded'
    });
    await evidencePage.locator('[data-node-id="ui-source"]').waitFor();
    await evidencePage.evaluate(() => document.fonts.ready);
    await evidencePage.getByRole('button', { name: 'Fit tree' }).click();
    await evidencePage.waitForTimeout(320);
    const map = await inspect('map', '#mapSurface', '#projectMapSvg .map-content');
    await evidencePage.screenshot({
      path: path.join(process.env.TREEWORK_EVIDENCE_DIR, `map-${suffix}.png`),
      fullPage: false,
      animations: 'disabled'
    });

    await evidencePage.locator('#branchSearch').fill('Focused Causal Manuscript');
    await evidencePage.locator('#branchSearch').press('Enter');
    await evidencePage.getByRole('button', { name: 'Dependency', exact: true }).click();
    await evidencePage.locator('#dependencySurface').waitFor();
    await evidencePage.locator('#branchSearch').fill('');
    const closeInspector = evidencePage.getByRole('button', {
      name: 'Close branch inspector'
    });
    if (await closeInspector.count()) await closeInspector.click();
    await evidencePage.getByRole('button', { name: 'Fit dependency view' }).click();
    await evidencePage.waitForTimeout(320);
    const dependency = await inspect(
      'dependency',
      '#dependencySurface',
      '#dependencySvg .dependency-content'
    );
    await evidencePage.screenshot({
      path: path.join(
        process.env.TREEWORK_EVIDENCE_DIR,
        `dependency-${suffix}.png`
      ),
      fullPage: false,
      animations: 'disabled'
    });

    await evidencePage.getByRole('button', { name: 'Replay', exact: true }).click();
    await evidencePage.getByLabel('Replay timeline').waitFor();
    await evidencePage.locator('.replay-stage #projectMapSvg').waitFor();
    await evidencePage.getByRole('button', { name: 'Fit tree' }).click();
    await evidencePage.waitForTimeout(320);
    const replay = await inspect(
      'replay',
      '.replay-stage',
      '.replay-stage #projectMapSvg .map-content'
    );
    const replayTimeline = await visibleRect(evidencePage, '.replay-timeline');
    const replayDetail = await visibleRect(
      evidencePage,
      '.replay-transaction-detail'
    );
    const replayDetailScroll = await visibleRect(
      evidencePage,
      '.replay-detail-scroll'
    );
    const replayControls = await evidencePage.evaluate(() =>
      [
        '[aria-label="Play Replay"]',
        '[aria-label="Replay transaction"]',
        '[aria-label="Filter Replay by branch"]',
        '.replay-speed'
      ].map((selector) => {
        const element = document.querySelector(selector);
        const rect = element?.getBoundingClientRect();
        return {
          selector,
          visible: Boolean(
            element &&
            rect &&
            rect.width > 0 &&
            rect.height > 0 &&
            rect.top >= 0 &&
            rect.left >= 0 &&
            rect.right <= innerWidth + 1 &&
            rect.bottom <= innerHeight + 1
          ),
          rect: rect
            ? {
                left: rect.left,
                right: rect.right,
                top: rect.top,
                bottom: rect.bottom
              }
            : null
        };
      })
    );
    assert(
      !overlaps(replay.surface, replayTimeline, 1) &&
      !overlaps(replayTimeline, replayDetail, 1),
      `${suffix} Replay regions overlap: ${JSON.stringify({
        stage: replay.surface,
        replayTimeline,
        replayDetail
      })}`
    );
    assert(
      replayTimeline.bottom <= viewport.height + 1 &&
      replayDetail.bottom <= viewport.height + 1 &&
      replayDetailScroll.bottom <= replayDetail.bottom + 1 &&
      replayDetailScroll.top >= replayDetail.top - 1 &&
      replayControls.every((control) => control.visible),
      `${suffix} Replay controls or detail are outside the viewport: ${JSON.stringify({
        replayTimeline,
        replayDetail,
        replayDetailScroll,
        replayControls
      })}`
    );
    await evidencePage.screenshot({
      path: path.join(process.env.TREEWORK_EVIDENCE_DIR, `replay-${suffix}.png`),
      fullPage: false,
      animations: 'disabled'
    });
    assert(errors.length === 0, `${suffix} browser errors: ${JSON.stringify(errors)}`);
    return {
      viewport,
      screenshots: {
        map: `map-${suffix}.png`,
        dependency: `dependency-${suffix}.png`,
        replay: `replay-${suffix}.png`
      },
      map,
      dependency,
      replay: {
        ...replay,
        timeline: replayTimeline,
        detail: replayDetail,
        detailScroll: replayDetailScroll,
        controls: replayControls
      }
    };
  } finally {
    await evidencePage.close();
  }
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
  const context = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 1
  });
  const page = await context.newPage();
  page.setDefaultTimeout(12000);
  const consoleErrors = [];
  const pageErrors = [];
  const requests = [];
  const failedResponses = [];
  page.on('console', (message) => {
    if (message.type() === 'error') {
      const location = message.location();
      consoleErrors.push(
        location.url ? `${message.text()} [${location.url}]` : message.text()
      );
    }
  });
  page.on('pageerror', (error) => pageErrors.push(error.message));
  page.on('request', (request) => requests.push(new URL(request.url()).pathname));
  page.on('response', (response) => {
    if (response.status() >= 500) {
      failedResponses.push(
        response.text().then((body) => ({
          status: response.status(),
          url: response.url(),
          body
        }))
      );
    }
  });

  try {
    await page.goto(process.env.TREEWORK_PROJECT_MAP_URL, { waitUntil: 'domcontentloaded' });
    await page.locator('[data-node-id="ui-source"]').waitFor();
    await page.evaluate(() => document.fonts.ready);

    assert(requests.includes('/api/project-map'), 'frontend did not request the real Project Map read model');
    assert(requests.includes('/api/project-map/events'), 'frontend did not open the SSE invalidation stream');
    assert(!requests.includes('/api/graph'), 'frontend requested the hidden compatibility graph');
    assert(await page.getByRole('button', { name: 'Map', exact: true }).getAttribute('aria-pressed') === 'true', 'Map mode is not active');
    const dependencyMode = page.getByRole('button', { name: 'Dependency', exact: true });
    assert(!(await dependencyMode.isDisabled()), 'Dependency must be enabled');
    const replayMode = page.getByRole('button', { name: 'Replay', exact: true });
    assert(!(await replayMode.isDisabled()), 'Replay must be enabled');
    assert(await page.locator('[data-mode="edit"], #pendingTray').count() === 0, 'write controls are visible');
    assert(await page.evaluate(() => !window.treeworkGraph && !window.graphology && !window.Sigma), 'retired graph globals are exposed');

    await page.keyboard.press('Tab');
    const topbarFocus = await page.evaluate(() => {
      const element = document.activeElement;
      const style = element ? getComputedStyle(element) : null;
      return {
        name: element?.getAttribute('aria-label') || element?.textContent?.trim(),
        outlineStyle: style?.outlineStyle,
        outlineWidth: style?.outlineWidth
      };
    });
    assert(
      topbarFocus.name === 'Map' &&
      topbarFocus.outlineStyle !== 'none' &&
      parseFloat(topbarFocus.outlineWidth || '0') >= 2,
      `topbar keyboard focus is not visible: ${JSON.stringify(topbarFocus)}`
    );

    const svgEvidence = await page.locator('#projectMapSvg').evaluate((svg) => {
      const content = svg.querySelector('.map-content');
      const box = content.getBBox();
      return {
        width: box.width,
        height: box.height,
        paths: svg.querySelectorAll('.parent-connectors path').length,
        nodes: svg.querySelectorAll('.branch-node').length
      };
    });
    assert(svgEvidence.nodes >= 14, `real map is too small for deep/wide acceptance: ${JSON.stringify(svgEvidence)}`);
    assert(svgEvidence.paths === svgEvidence.nodes - 1, `hierarchy connector count is wrong: ${JSON.stringify(svgEvidence)}`);
    assert(svgEvidence.width > 700 && svgEvidence.height > 500, `SVG scene is blank or collapsed: ${JSON.stringify(svgEvidence)}`);

    const initialGeometry = await rawGeometry(page);
    const positions = Object.entries(initialGeometry).map(([id, value]) => {
      const match = /^translate\(([-\d.]+) ([-\d.]+)\)$/.exec(value.transform || '');
      assert(match, `node ${id} has invalid geometry ${value.transform}`);
      return { id, depth: value.depth, x: Number(match[1]), y: Number(match[2]) };
    });
    const depthColumns = new Map();
    for (const position of positions) {
      const values = depthColumns.get(position.depth) || [];
      values.push(position.x);
      depthColumns.set(position.depth, values);
    }
    let previousX = -Infinity;
    for (const depth of [...depthColumns.keys()].sort((a, b) => a - b)) {
      const values = depthColumns.get(depth);
      assert(Math.max(...values) - Math.min(...values) < 0.001, `depth ${depth} is not column-aligned`);
      assert(values[0] > previousX, `depth ${depth} does not advance horizontally`);
      previousX = values[0];
    }
    const localRects = positions.map((position) => ({
      id: position.id,
      left: position.x,
      right: position.x + 276,
      top: position.y,
      bottom: position.y + 86
    }));
    for (let index = 0; index < localRects.length; index += 1) {
      for (let other = index + 1; other < localRects.length; other += 1) {
        assert(!overlaps(localRects[index], localRects[other], 0.5), `nodes overlap: ${localRects[index].id} and ${localRects[other].id}`);
      }
    }

    const lifecycleEncoding = await page.evaluate(() => {
      const read = (id) => {
        const node = document.querySelector(`[data-node-id="${id}"]`);
        const rail = getComputedStyle(node.querySelector('.status-rail'));
        const symbol = node.querySelector('.status-symbol')?.textContent || '';
        return {
          className: node.getAttribute('class'),
          fill: rail.fill,
          stroke: rail.stroke,
          dash: rail.strokeDasharray,
          opacity: rail.opacity,
          symbol
        };
      };
      const root = getComputedStyle(document.documentElement);
      return {
        current: read('ui-source'),
        complete: read('done-leaf'),
        paused: read('paused-leaf'),
        aborted: read('aborted-leaf'),
        ready: read('ready-leaf'),
        waiting: read('waiting-leaf'),
        palette: {
          paper: root.getPropertyValue('--tw-paper').trim(),
          ink: root.getPropertyValue('--tw-ink').trim(),
          ember: root.getPropertyValue('--tw-ember').trim(),
          moss: root.getPropertyValue('--tw-moss').trim(),
          ochre: root.getPropertyValue('--tw-ochre').trim(),
          error: root.getPropertyValue('--tw-error').trim(),
          frost: root.getPropertyValue('--tw-frost').trim()
        }
      };
    });
    assert(lifecycleEncoding.palette.paper && lifecycleEncoding.palette.ink && lifecycleEncoding.palette.ember, 'Strata palette is incomplete');
    assert(!lifecycleEncoding.palette.moss && !lifecycleEncoding.palette.ochre && !lifecycleEncoding.palette.error && !lifecycleEncoding.palette.frost, `multi-color semantic palette leaked back in: ${JSON.stringify(lifecycleEncoding.palette)}`);
    assert(lifecycleEncoding.current.fill.includes('184, 71, 42'), `current rail is not cinnabar: ${JSON.stringify(lifecycleEncoding.current)}`);
    assert(lifecycleEncoding.paused.dash !== lifecycleEncoding.aborted.dash, 'paused and aborted rails are not pattern-distinct');
    assert(lifecycleEncoding.paused.symbol !== lifecycleEncoding.aborted.symbol, 'paused and aborted symbols are not distinct');
    assert(lifecycleEncoding.ready.symbol !== lifecycleEncoding.waiting.symbol, 'ready and waiting are not distinguishable without color');

    await page.locator('[data-node-id="ui-source"]').click();
    await page.locator('#branchInspector').waitFor();
    await page.getByText('Dependencies', { exact: true }).waitFor();
    const inspectorText = await page.locator('#branchInspector').innerText();
    assert(inspectorText.includes('Accepted Foundation'), `Inspector did not render real dependency data: ${inspectorText}`);
    assert(inspectorText.toLowerCase().includes('spec reference'), `Inspector did not render the branch Spec reference: ${inspectorText}`);

    await page.locator('#sessionAnnotation').fill('Temporary browser acceptance thought');
    await waitFor(async () => page.evaluate(() => {
      const value = JSON.parse(sessionStorage.getItem(`treework-project-map:v3:${location.pathname}`) || 'null');
      return value?.annotations?.['ui-source'] === 'Temporary browser acceptance thought';
    }), 'session annotation persistence');

    const inspectorRect = await visibleRect(page, '#branchInspector');
    const toolsRect = await visibleRect(page, '.canvas-tools');
    const topbarRect = await visibleRect(page, '.topbar');
    assert(!overlaps(inspectorRect, toolsRect, 1), 'desktop Inspector overlaps canvas controls');
    assert(!overlaps(inspectorRect, topbarRect, 1), 'desktop Inspector overlaps the top bar');

    await page.locator('#openSettings').click();
    await page.locator('#settingsPanel').waitFor();
    const settingsRect = await visibleRect(page, '#settingsPanel');
    assert(!overlaps(settingsRect, inspectorRect, 1), 'settings panel overlaps the Inspector');
    await page.locator('#zoomSensitivity').fill('0.3');
    await waitFor(async () => (await page.locator('#zoomSensitivityValue').textContent()) === '30%', 'zoom setting update');
    await page.locator('#closeSettings').click();

    const content = page.locator('.map-content');
    const panBefore = await content.getAttribute('transform');
    await page.locator('#mapSurface').dispatchEvent('wheel', {
      deltaX: 0,
      deltaY: 80,
      bubbles: true,
      cancelable: true
    });
    const panAfter = await content.getAttribute('transform');
    assert(panAfter !== panBefore, 'wheel pan did not move the viewport');
    await page.locator('#mapSurface').dispatchEvent('wheel', {
      deltaX: 0,
      deltaY: -80,
      ctrlKey: true,
      bubbles: true,
      cancelable: true
    });
    const zoomAfter = await content.getAttribute('transform');
    assert(zoomAfter !== panAfter, 'modified wheel did not zoom the viewport');

    const geometryBeforePresentation = await rawGeometry(page);
    await page.locator('#branchSearch').fill('Foundation');
    await page.waitForTimeout(80);
    assert(sameJson(await rawGeometry(page), geometryBeforePresentation), 'search moved node coordinates');
    await page.locator('#statusFilter').selectOption('complete');
    await page.waitForTimeout(80);
    assert(sameJson(await rawGeometry(page), geometryBeforePresentation), 'status filter moved node coordinates');
    await page.locator('#statusFilter').selectOption('all');
    await page.locator('#branchSearch').fill('');

    const projectionRequestsBeforeLifecycle = requests.filter((value) => value === '/api/project-map').length;
    const coordinatesBeforeLifecycle = await rawGeometry(page);
    runTw('pause', '--reason', 'Browser lifecycle geometry check');
    await waitFor(
      () => requests.filter((value) => value === '/api/project-map').length > projectionRequestsBeforeLifecycle,
      'lifecycle SSE projection refetch'
    );
    await page.waitForFunction(() => document.querySelector('[data-node-id="ui-source"]')?.dataset.status === 'paused');
    const coordinatesAfterLifecycle = await rawGeometry(page);
    for (const id of Object.keys(coordinatesBeforeLifecycle)) {
      assert(
        coordinatesBeforeLifecycle[id].transform === coordinatesAfterLifecycle[id].transform,
        `lifecycle-only update moved ${id}`
      );
    }

    const projectionBeforeNarrative = requests.filter((value) => value === '/api/project-map').length;
    const branchBeforeNarrative = requests.filter((value) => value.startsWith('/api/project-map/branch')).length;
    const progressPath = path.join(
      process.env.TREEWORK_UI_SOURCE_ARTIFACT_DIR,
      'progress.md'
    );
    const progress = fs.readFileSync(progressPath, 'utf8');
    const updatedProgress = progress.replace(
      /(## Current Reality[^\n]*\n)/,
      '$1\nBrowser narrative refreshed through SSE.\n'
    );
    assert(updatedProgress !== progress, 'could not update the fixture Current Reality section');
    fs.writeFileSync(progressPath, updatedProgress);
    await page.getByText('Browser narrative refreshed through SSE.', { exact: true }).waitFor();
    assert(
      requests.filter((value) => value === '/api/project-map').length === projectionBeforeNarrative,
      'narrative-only invalidation refetched the global projection'
    );
    assert(
      requests.filter((value) => value.startsWith('/api/project-map/branch')).length > branchBeforeNarrative,
      'narrative-only invalidation did not refetch branch detail'
    );

    const anchorBefore = await screenCenter(page, 'ui-source');
    const projectionBeforeTopology = requests.filter((value) => value === '/api/project-map').length;
    runTw('tree', 'update');
    fs.copyFileSync(process.env.TREEWORK_UPDATED_TREE, path.join(process.env.TREEWORK_WORKSPACE, '.TreeWork', 'tree.yaml'));
    runTw('tree', 'apply');
    await page.locator('[data-node-id="ui-late"]').waitFor();
    await page.getByText('Revision 2', { exact: true }).waitFor();
    await waitFor(
      () => requests.filter((value) => value === '/api/project-map').length > projectionBeforeTopology,
      'topology SSE projection refetch'
    );
    const anchorAfter = await screenCenter(page, 'ui-source');
    const anchorDistance = Math.hypot(anchorBefore.x - anchorAfter.x, anchorBefore.y - anchorAfter.y);
    assert(anchorDistance < 1.5, `topology update lost selected viewport anchor by ${anchorDistance.toFixed(2)}px`);

    const geometryAfterTopology = await rawGeometry(page);
    await page.reload({ waitUntil: 'domcontentloaded' });
    await page.locator('[data-node-id="ui-late"]').waitFor();
    await page.evaluate(() => document.fonts.ready);
    assert(sameJson(await rawGeometry(page), geometryAfterTopology), 'same accepted topology produced non-deterministic coordinates');
    await page.locator('#branchInspector').waitFor();

    const geometryBeforeCollapse = await rawGeometry(page);
    const countBeforeCollapse = await page.locator('.branch-node').count();
    await page.locator('[data-node-id="foundation"] .collapse-control').dispatchEvent('click');
    await waitFor(async () => (await page.locator('.branch-node').count()) === countBeforeCollapse - 4, 'subtree collapse');
    await page.locator('[data-node-id="foundation"] .collapse-control').dispatchEvent('click');
    await waitFor(async () => (await page.locator('.branch-node').count()) === countBeforeCollapse, 'subtree expand');
    assert(sameJson(await rawGeometry(page), geometryBeforeCollapse), 'collapse/expand did not restore deterministic coordinates');

    await page.locator('[data-node-id="ui-parent"]').focus();
    await page.keyboard.press('ArrowRight');
    assert(
      await page.evaluate(() => document.activeElement?.getAttribute('data-node-id')) === 'ui-source',
      'tree keyboard navigation did not move to the first child'
    );
    const nodeFocus = await page.locator('[data-node-id="ui-source"]').evaluate((node) => {
      const selection = getComputedStyle(node.querySelector('.node-selection'));
      return {
        label: node.getAttribute('aria-label'),
        stroke: selection.stroke,
        strokeWidth: selection.strokeWidth
      };
    });
    assert(
      nodeFocus.label?.includes('Strata Map View') &&
      nodeFocus.label?.toLowerCase().includes('unverified') &&
      nodeFocus.stroke !== 'none' &&
      parseFloat(nodeFocus.strokeWidth || '0') >= 1.5,
      `tree node keyboard focus is not visible or named: ${JSON.stringify(nodeFocus)}`
    );

    await page.getByRole('button', { name: 'Fit tree' }).click();
    await page.waitForTimeout(320);
    await page.screenshot({
      path: path.join(process.env.TREEWORK_EVIDENCE_DIR, 'map-desktop.png'),
      fullPage: false,
      animations: 'disabled'
    });

    await page.locator('#branchSearch').fill('Focused Causal Manuscript');
    await page.locator('#branchSearch').press('Enter');
    const branchBack = page.getByRole('button', { name: 'Previous focused branch' });
    const branchForward = page.getByRole('button', { name: 'Next focused branch' });
    assert(!(await branchBack.isDisabled()), 'branch history did not record search navigation');
    assert(await branchForward.isDisabled(), 'branch history forward action should start disabled');
    await branchBack.click();
    assert(!(await branchForward.isDisabled()), 'branch history forward action did not activate after Back');
    await branchForward.click();
    assert(
      await page.locator('[data-node-id="dependency-focus"][aria-selected="true"]').count() === 1,
      'branch history Forward did not restore the searched branch'
    );
    await page.getByRole('button', { name: 'Dependency', exact: true }).click();
    await page.locator('#dependencySurface').waitFor();
    await page.locator('#branchSearch').fill('');
    assert(
      await page.getByRole('button', { name: 'Dependency', exact: true }).getAttribute('aria-pressed') === 'true',
      'Dependency mode is not active'
    );
    assert(
      await page.getByRole('navigation', { name: 'Focused branch hierarchy path' }).count() === 1,
      'Dependency breadcrumb does not describe the focused hierarchy path'
    );
    assert(
      await page.locator('[data-node-id="dependency-focus"][data-node-role="focus"][role="option"][aria-selected="true"]').count() === 1,
      'focused dependency node is missing valid option selection semantics'
    );
    for (const id of ['done-leaf', 'ui-source', 'prereq-a', 'prereq-b']) {
      assert(
        await page.locator(`[data-node-id="${id}"][data-node-role="upstream"][data-depth="1"]`).count() === 1,
        `missing direct upstream ${id}`
      );
    }
    for (const id of ['down-one', 'down-two']) {
      assert(
        await page.locator(`[data-node-id="${id}"][data-node-role="downstream"][data-depth="1"]`).count() === 1,
        `missing direct downstream ${id}`
      );
    }
    assert(await page.locator('[data-node-id="shared-base"]').count() === 0, 'transitive upstream leaked into direct depth');
    assert(await page.locator('[data-node-id="down-deep"]').count() === 0, 'transitive downstream leaked into direct depth');
    assert(
      await page.locator('[data-node-id="parallel-ready"][data-node-role="parallel"]').count() === 1,
      'ready independent branch is not shown as a parallel opportunity'
    );
    assert(await page.locator('[data-node-id="nested-ready"]').count() === 0, 'nested focus scope was suggested as parallel work');
    assert(await page.locator('[data-node-id="ui-parent"]').count() === 0, 'focus ancestor was suggested as parallel work');
    assert(
      await page.locator('[data-node-id="foundation"][data-node-role="parallel"]').count() === 0,
      'parent coordination branch was suggested as executable parallel work'
    );
    const waitingText = await page.locator('.dependency-waiting').innerText();
    assert(waitingText.includes('Strata Map View') && waitingText.includes('Fan In Prerequisite A'), `waiting explanation is incomplete: ${waitingText}`);
    assert(!waitingText.includes('Accepted Foundation'), `satisfied prerequisite leaked into waiting explanation: ${waitingText}`);
    assert(
      (await page.locator('.parallel-disclaimer').textContent()).includes('check shared files before assigning'),
      'parallel opportunity disclaimer is missing'
    );
    assert(
      await page.locator('[data-edge="done-leaf:dependency-focus"][data-satisfied="true"]').count() === 1,
      'satisfied direct relation is missing'
    );
    assert(
      await page.locator('[data-edge="ui-source:dependency-focus"][data-satisfied="false"]').count() === 1,
      'unsatisfied direct relation is missing'
    );

    const dependencyDefaultGeometry = await rawDependencyGeometry(page);
    for (const [id, value] of Object.entries(dependencyDefaultGeometry)) {
      const match = /^translate\(([-\d.]+) ([-\d.]+)\)$/.exec(value.transform || '');
      assert(match, `dependency node ${id} has invalid geometry ${value.transform}`);
      const x = Number(match[1]);
      if (value.role === 'focus') assert(Math.abs(x) < 0.001, `focus ${id} is not in the center column`);
      if (value.role === 'upstream') assert(x === -value.depth * 360, `upstream ${id} is in the wrong column`);
      if (value.role === 'downstream') assert(x === value.depth * 360, `downstream ${id} is in the wrong column`);
    }
    const dependencySceneEvidence = await page.locator('#dependencySvg').evaluate((svg) => {
      const content = svg.querySelector('.dependency-content');
      const box = content.getBBox();
      return {
        width: box.width,
        height: box.height,
        nodes: svg.querySelectorAll('.branch-node').length,
        edges: svg.querySelectorAll('.dependency-connectors > g').length
      };
    });
    assert(
      dependencySceneEvidence.width > 700 &&
      dependencySceneEvidence.height > 300 &&
      dependencySceneEvidence.nodes >= 8 &&
      dependencySceneEvidence.edges === 7,
      `Dependency SVG is blank or incomplete: ${JSON.stringify(dependencySceneEvidence)}`
    );

    const dependencyPalette = await page.evaluate(() => {
      const appearance = (id) => {
        const node = document.querySelector(`[data-node-id="${id}"]`);
        const rail = getComputedStyle(node.querySelector('.status-rail'));
        const selection = getComputedStyle(node.querySelector('.node-selection'));
        return { rail: rail.fill, selectionStroke: selection.stroke };
      };
      return {
        focus: appearance('dependency-focus'),
        current: appearance('ui-source')
      };
    });
    assert(
      dependencyPalette.current.rail.includes('184, 71, 42'),
      `true current branch lost cinnabar identity: ${JSON.stringify(dependencyPalette)}`
    );
    assert(
      !dependencyPalette.focus.selectionStroke.includes('184, 71, 42'),
      `dependency focus incorrectly uses cinnabar selection: ${JSON.stringify(dependencyPalette)}`
    );

    await page.locator('[data-node-id="prereq-a"]').click();
    await page.locator('[data-node-id="prereq-a"][data-node-role="focus"]').waitFor();
    await waitFor(async () => {
      const center = await screenCenter(page, 'prereq-a');
      const surface = await visibleRect(page, '#dependencySurface');
      return (
        Math.abs(center.x - surface.width / 2) < 2 &&
        Math.abs(center.y - (surface.top + surface.height * 0.42)) < 2
      );
    }, 'new dependency focus viewport centering');

    await page.locator('#branchSearch').fill('Focused Causal Manuscript');
    await page.locator('#branchSearch').press('Enter');
    await page.locator('#branchSearch').fill('');
    await page.locator('[data-node-id="dependency-focus"][data-node-role="focus"]').waitFor();
    await page.getByRole('button', { name: 'Expand upstream depth' }).click();
    await page.getByRole('button', { name: 'Expand downstream depth' }).click();
    assert(
      await page.locator('[data-node-id="shared-base"][data-node-role="upstream"][data-depth="2"]').count() === 1,
      'shared transitive upstream was not rendered once at minimum distance'
    );
    assert(
      await page.locator('[data-node-id="down-deep"][data-node-role="downstream"][data-depth="2"]').count() === 1,
      'transitive downstream expansion is missing'
    );
    assert(
      await page.locator('[data-node-id="shared-base"]').count() === 1,
      'fan-in shared prerequisite was duplicated'
    );
    const expandedDependencyGeometry = await rawDependencyGeometry(page);
    for (const id of ['dependency-focus', 'done-leaf', 'ui-source', 'prereq-a', 'prereq-b', 'down-one', 'down-two']) {
      assert(
        expandedDependencyGeometry[id].transform === dependencyDefaultGeometry[id].transform,
        `depth expansion moved existing causal node ${id}`
      );
    }
    const dependencyRects = await page.locator('.dependency-content .branch-node').evaluateAll((elements) =>
      elements.map((element) => {
        const transform = element.getAttribute('transform') || '';
        const match = /^translate\(([-\d.]+) ([-\d.]+)\)$/.exec(transform);
        return {
          id: element.dataset.nodeId,
          left: Number(match[1]),
          right: Number(match[1]) + 224,
          top: Number(match[2]),
          bottom: Number(match[2]) + 82
        };
      })
    );
    for (let index = 0; index < dependencyRects.length; index += 1) {
      for (let other = index + 1; other < dependencyRects.length; other += 1) {
        assert(!overlaps(dependencyRects[index], dependencyRects[other], 0.5), `dependency nodes overlap: ${dependencyRects[index].id} and ${dependencyRects[other].id}`);
      }
    }

    const dependencyBeforePresentation = await rawDependencyGeometry(page);
    await page.locator('#branchSearch').fill('Fan In');
    await page.locator('#statusFilter').selectOption('complete');
    await page.waitForTimeout(80);
    assert(sameJson(await rawDependencyGeometry(page), dependencyBeforePresentation), 'Dependency search/filter moved coordinates');
    await page.locator('#branchSearch').fill('');
    await page.locator('#statusFilter').selectOption('all');

    const dependencyProjectionBeforeLifecycle = requests.filter((value) => value === '/api/project-map').length;
    const dependencyBeforeLifecycle = await rawDependencyGeometry(page);
    runTw('enter', 'ui-source', '--no-isolate');
    await waitFor(
      () => requests.filter((value) => value === '/api/project-map').length > dependencyProjectionBeforeLifecycle,
      'Dependency lifecycle SSE projection refetch'
    );
    await page.waitForFunction(() => document.querySelector('[data-node-id="ui-source"]')?.dataset.status === 'in_progress');
    const dependencyAfterLifecycle = await rawDependencyGeometry(page);
    for (const id of Object.keys(dependencyBeforeLifecycle)) {
      if (dependencyAfterLifecycle[id]) {
        assert(
          dependencyBeforeLifecycle[id].transform === dependencyAfterLifecycle[id].transform,
          `Dependency lifecycle-only update moved ${id}`
        );
      }
    }

    await page.getByRole('button', { name: 'Close branch inspector' }).click();
    assert(
      await page.locator('[data-node-id="dependency-focus"][data-node-role="focus"][aria-selected="true"]').count() === 1,
      'closing desktop Dependency Inspector cleared focus'
    );
    await page.getByRole('button', { name: 'Fit dependency view' }).click();
    await page.waitForTimeout(320);
    await page.screenshot({
      path: path.join(process.env.TREEWORK_EVIDENCE_DIR, 'dependency-desktop.png'),
      fullPage: false,
      animations: 'disabled'
    });

    await page.getByRole('button', { name: 'Map', exact: true }).click();
    await page.locator('#mapSurface').waitFor();
    assert(
      await page.locator('[data-node-id="dependency-focus"][aria-selected="true"]').count() === 1,
      'Map return did not preserve and select Dependency focus'
    );
    await waitFor(async () => page.evaluate(() => {
      const value = JSON.parse(sessionStorage.getItem(`treework-project-map:v3:${location.pathname}`) || 'null');
      return value?.activeView === 'map' && value?.selected === 'dependency-focus';
    }), 'Map return session persistence');

    const replayRequestsBefore = requests.filter((value) => value === '/api/project-map/replay').length;
    await replayMode.click();
    await page.getByLabel('Replay timeline').waitFor();
    await waitFor(
      () => requests.filter((value) => value === '/api/project-map/replay').length > replayRequestsBefore,
      'Replay catalog request'
    );
    assert(
      await replayMode.getAttribute('aria-pressed') === 'true',
      'Replay mode is not active'
    );
    assert(await page.locator('#branchInspector').count() === 0, 'Replay mounted the current branch Inspector');

    const replayCatalog = await page.evaluate(async () => {
      const response = await fetch('/api/project-map/replay');
      return response.json();
    });
    const replayTransactions = replayCatalog.transactions;
    assert(
      replayCatalog.reconstruction.status === 'available' &&
      replayCatalog.state &&
      replayTransactions.length > 10,
      `real Replay catalog is incomplete: ${JSON.stringify(replayCatalog.meta)}`
    );
    assert(
      replayTransactions.every((transaction, index) =>
        index === 0 || transaction.seq > replayTransactions[index - 1].seq
      ),
      'Replay transactions are not strictly ordered by sequence'
    );
    assert(
      new Set(replayTransactions.map((transaction) => transaction.seq)).size === replayTransactions.length,
      'Replay timeline contains duplicate transaction steps'
    );

    const groupedApply = replayTransactions.find(
      (transaction) =>
        transaction.type === 'tree.applied' &&
        Array.isArray(transaction.changes?.operations) &&
        transaction.changes.operations.length > 1
    );
    assert(groupedApply, 'fixture did not publish a grouped tree.applied transaction');
    const groupedIndex = replayTransactions.findIndex((transaction) => transaction.seq === groupedApply.seq);
    assert(groupedIndex > 0, 'grouped Tree Apply has no seekable prior transaction');

    const beforeApply = replayTransactions[groupedIndex - 1];
    await selectReplaySequence(page, replayTransactions, beforeApply.seq);
    await page.locator('#projectMapSvg').waitFor();
    assert(
      await page.locator('[data-node-id="ui-source"]').count() === 0,
      'pre-checkpoint Replay scene contains a branch before Tree Apply'
    );

    await selectReplaySequence(page, replayTransactions, groupedApply.seq);
    await page.locator('[data-node-id="ui-source"]').waitFor();
    const groupedDetail = await page.locator('.replay-transaction-detail').innerText();
    const groupedSemanticLines = await page.locator('.replay-transaction-detail section li').count();
    assert(
      groupedSemanticLines >= groupedApply.changes.operations.length + 1,
      `grouped Tree Apply detail lost semantic operations: ${groupedSemanticLines}`
    );
    assert(
      groupedDetail.includes('Tree applied') &&
      groupedDetail.includes('Created') &&
      !groupedDetail.includes('{"'),
      `grouped Tree Apply detail is not human-readable: ${groupedDetail}`
    );
    const positionText = await page.locator('.replay-position').innerText();
    assert(
      positionText.includes(`of ${replayTransactions.length}`),
      `grouped Tree Apply was not represented in the one-transaction timeline: ${positionText}`
    );

    await page.getByRole('button', { name: '4 times speed' }).click();
    await page.getByRole('button', { name: 'Play Replay' }).click();
    const afterGrouped = replayTransactions[groupedIndex + 1];
    await waitFor(
      async () => Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) === afterGrouped.seq,
      'Replay playback advance'
    );
    await page.getByRole('button', { name: 'Pause Replay' }).click();

    const latestUiSourceEntry = [...replayTransactions].reverse().find(
      (transaction) =>
        transaction.type === 'branch.entered' &&
        transaction.subject === 'ui-source'
    );
    assert(latestUiSourceEntry, 'fixture has no ui-source entry transaction');
    await selectReplaySequence(page, replayTransactions, latestUiSourceEntry.seq);
    await page.locator('[data-node-id="ui-source"]').waitFor();
    await page.getByLabel('Filter Replay by branch').selectOption('aborted-leaf');
    const abortedTransactions = replayTransactions.filter(
      (transaction) =>
        transaction.subject === 'aborted-leaf' ||
        transaction.affected_subjects.includes('aborted-leaf')
    );
    const expectedFiltered = [...abortedTransactions]
      .reverse()
      .find((transaction) => transaction.seq <= latestUiSourceEntry.seq) ??
      abortedTransactions[0];
    await waitFor(
      async () =>
        Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) ===
        expectedFiltered.seq,
      'nearest filtered Replay sequence'
    );
    const expectedFilteredSnapshot = await page.evaluate(async (sequence) => {
      const response = await fetch(`/api/project-map/replay?at=${sequence}&after=${sequence}`);
      return response.json();
    }, expectedFiltered.seq);
    await page.locator('#projectMapSvg').waitFor();
    assert(
      await page.locator('.replay-stage .branch-node').count() ===
        expectedFilteredSnapshot.state.nodes.length &&
      await page.locator('[data-node-id="ready-leaf"]').count() === 1,
      'branch filtering cropped the globally reconstructed Tree'
    );

    const filteredGapIndex = abortedTransactions.findIndex(
      (transaction, index) =>
        index > 0 && transaction.seq - abortedTransactions[index - 1].seq > 1
    );
    assert(filteredGapIndex > 0, 'fixture did not create a non-contiguous filtered timeline gap');
    await page.getByLabel('Replay transaction').fill(String(filteredGapIndex));
    await waitFor(
      async () =>
        Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) ===
          abortedTransactions[filteredGapIndex].seq &&
        await page.locator('#projectMapSvg').count() === 1,
      'filtered Replay step before a sequence gap'
    );
    await page.getByRole('button', { name: 'Previous transaction' }).click();
    const filteredGapTarget = abortedTransactions[filteredGapIndex - 1];
    await waitFor(
      async () =>
        Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) ===
        filteredGapTarget.seq &&
        await page.locator('#projectMapSvg').count() === 1,
      'non-contiguous filtered Replay step'
    );
    await waitFor(
      async () => await page.locator('.branch-node-visual[class*="is-replay-"]').count() === 0,
      'direct scene replacement across a filtered sequence gap'
    );

    await page.getByRole('button', { name: 'Return to Live' }).click();
    await waitFor(
      async () =>
        Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) ===
          replayCatalog.meta.live_event_seq &&
        await page.getByLabel('Filter Replay by branch').inputValue() === '',
      'global Return to Live'
    );
    assert(
      await page.locator('.replay-view').getAttribute('data-replay-live') === 'true',
      'Return to Live did not enter global Live'
    );

    const treeKeySequence = Number(await page.locator('.replay-view').getAttribute('data-replay-seq'));
    await page.locator('[data-node-id="ui-parent"]').focus();
    await page.keyboard.press('ArrowRight');
    assert(
      await page.evaluate(() => document.activeElement?.getAttribute('data-node-id')) === 'ui-source',
      'Replay tree keyboard navigation did not move to the first child'
    );
    assert(
      Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) === treeKeySequence,
      'Replay tree ArrowRight also moved the timeline'
    );
    await page.keyboard.press('Space');
    await waitFor(
      async () => await page.getByLabel('Filter Replay by branch').inputValue() === 'ui-source',
      'Replay branch selection with Space'
    );
    assert(
      await page.getByRole('button', { name: 'Play Replay' }).count() === 1 &&
      Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) === treeKeySequence,
      'Replay tree Space also toggled playback or moved the timeline'
    );
    await page.getByRole('button', { name: 'Return to Live' }).click();
    await waitFor(
      async () => await page.getByLabel('Filter Replay by branch').inputValue() === '',
      'Return to Live after tree selection'
    );

    await page.getByRole('button', { name: 'Open canvas settings' }).click();
    await page.locator('#settingsPanel').waitFor();
    const replaySettingsRect = await visibleRect(page, '#settingsPanel');
    const replayTimelineRect = await visibleRect(page, '.replay-timeline');
    const replayDetailRect = await visibleRect(page, '.replay-transaction-detail');
    assert(!overlaps(replaySettingsRect, replayTimelineRect, 1), 'Replay settings overlap the timeline');
    assert(!overlaps(replaySettingsRect, replayDetailRect, 1), 'Replay settings overlap transaction detail');
    await page.locator('#panSensitivity').fill('1.25');
    await page.locator('#zoomSensitivity').fill('0.75');
    await waitFor(
      async () =>
        (await page.locator('#panSensitivityValue').textContent()) === '125%' &&
        (await page.locator('#zoomSensitivityValue').textContent()) === '75%',
      'Replay shared canvas settings update'
    );
    await page.locator('#panSensitivity').fill('0.55');
    await page.locator('#zoomSensitivity').fill('0.3');
    await waitFor(
      async () =>
        (await page.locator('#panSensitivityValue').textContent()) === '55%' &&
        (await page.locator('#zoomSensitivityValue').textContent()) === '30%',
      'restored shared canvas settings baseline'
    );
    await page.locator('#closeSettings').click();
    const replayContent = page.locator('.replay-stage .map-content');
    const replayPanBefore = await replayContent.getAttribute('transform');
    await page.locator('.replay-stage #mapSurface').dispatchEvent('wheel', {
      deltaX: 0,
      deltaY: 80,
      bubbles: true,
      cancelable: true
    });
    assert(
      await replayContent.getAttribute('transform') !== replayPanBefore,
      'Replay did not use shared pan sensitivity settings'
    );

    await page.getByRole('button', { name: 'Previous transaction' }).click();
    const priorLiveSequence = replayCatalog.meta.live_event_seq;
    await waitFor(
      async () =>
        Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) <
        priorLiveSequence,
      'historical Replay cursor before live advancement'
    );
    const historicalSequence = Number(await page.locator('.replay-view').getAttribute('data-replay-seq'));
    runTw('pause', '--reason', 'Replay live cursor stability check');
    await page.getByText(`Live seq ${priorLiveSequence + 1}`, { exact: true }).waitFor();
    assert(
      Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) === historicalSequence,
      'new accepted event moved a historical Replay cursor'
    );
    await page.getByRole('button', { name: 'Return to Live' }).click();
    await waitFor(
      async () =>
        Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) ===
        priorLiveSequence + 1,
      'Return to newly advanced global Live'
    );

    await page.emulateMedia({ reducedMotion: 'reduce' });
    await page.waitForFunction(() => matchMedia('(prefers-reduced-motion: reduce)').matches);
    await page.getByRole('button', { name: 'Previous transaction' }).click();
    await waitFor(
      async () =>
        Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) ===
        priorLiveSequence,
      'reduced-motion Replay step'
    );
    assert(
      await page.locator('.branch-node-visual[class*="is-replay-"]').count() === 0,
      'reduced-motion Replay retained scene animation classes'
    );
    await page.getByRole('button', { name: 'Return to Live' }).click();
    await page.emulateMedia({ reducedMotion: 'no-preference' });
    await page.getByRole('button', { name: 'Fit tree' }).click();
    await page.waitForTimeout(320);
    await page.screenshot({
      path: path.join(process.env.TREEWORK_EVIDENCE_DIR, 'replay-desktop.png'),
      fullPage: false,
      animations: 'disabled'
    });

    await page.getByRole('button', { name: 'Map', exact: true }).click();
    await page.locator('#mapSurface').waitFor();
    await waitFor(async () => page.evaluate(() => {
      const value = JSON.parse(sessionStorage.getItem(`treework-project-map:v3:${location.pathname}`) || 'null');
      return value?.activeView === 'map';
    }), 'Map session persistence after Replay');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.reload({ waitUntil: 'domcontentloaded' });
    await page.locator('[data-node-id="ui-source"]').waitFor();
    await page.keyboard.press('Escape');
    await waitFor(
      async () => await page.locator('#branchInspector').count() === 0,
      'closed mobile Map Inspector'
    );
    for (let index = 0; index < 4; index += 1) {
      await page.locator('.canvas-tools button[aria-label="Zoom in"]').click();
    }
    await page.locator('.canvas-tools button[aria-label="Locate current branch"]').click();
    await page.locator('[data-node-id="ui-source"]').click();
    await page.locator('#branchInspector').waitFor();
    await page.waitForTimeout(320);
    const mobileTopbar = await visibleRect(page, '.topbar');
    const mobileInspector = await visibleRect(page, '#branchInspector');
    const mobileTools = await visibleRect(page, '.canvas-tools');
    const mobileInspectorAppearance = await page.locator('#branchInspector').evaluate((element) => {
      const style = getComputedStyle(element);
      const channels = style.backgroundColor.match(/[\d.]+/g)?.map(Number) || [];
      return {
        opacity: Number(style.opacity),
        background: style.backgroundColor,
        backgroundAlpha: channels.length === 4 ? channels[3] : 1,
        color: style.color
      };
    });
    assert(mobileTopbar.right <= 390.5 && mobileTopbar.bottom <= 96.5, `mobile topbar overflows: ${JSON.stringify(mobileTopbar)}`);
    assert(!overlaps(mobileTopbar, mobileInspector, 1), 'mobile Inspector overlaps topbar');
    assert(!mobileTools.visible, 'mobile canvas controls remain visible behind the Inspector');
    assert(
      mobileInspectorAppearance.opacity >= 0.99 &&
      mobileInspectorAppearance.backgroundAlpha >= 0.95,
      `settled mobile Inspector is translucent: ${JSON.stringify(mobileInspectorAppearance)}`
    );
    assert(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1),
      'mobile page has horizontal UI overflow'
    );
    const mobileModeLabels = await page.locator('.view-switcher button').evaluateAll((buttons) =>
      buttons.map((button) => {
        const short = button.querySelector('.view-label-short');
        return {
          name: button.getAttribute('aria-label'),
          short: short?.textContent,
          visible: short ? getComputedStyle(short).display !== 'none' : false
        };
      })
    );
    assert(mobileModeLabels.every((item) => item.visible && item.short?.length === 1), `mobile mode switcher is not compact: ${JSON.stringify(mobileModeLabels)}`);
    await page.screenshot({
      path: path.join(process.env.TREEWORK_EVIDENCE_DIR, 'map-portrait.png'),
      fullPage: false,
      animations: 'disabled'
    });

    await page.locator('#branchSearch').fill('Focused Causal Manuscript');
    await page.locator('#branchSearch').press('Enter');
    await page.getByRole('button', { name: 'Dependency', exact: true }).click();
    await page.locator('#dependencySurface').waitFor();
    await page.locator('#branchSearch').fill('');
    await page.getByRole('button', { name: 'Close branch inspector' }).click();
    assert(
      await page.locator('[data-node-id="dependency-focus"][data-node-role="focus"][aria-selected="true"]').count() === 1,
      'closing mobile Dependency Inspector cleared focus'
    );
    assert(
      (await page.getByLabel('Visible upstream depth').textContent()) === '2' &&
      (await page.getByLabel('Visible downstream depth').textContent()) === '2',
      'Dependency depth preferences did not survive focus and view changes'
    );
    await page.getByRole('button', { name: 'Center focused branch' }).click();
    await page.waitForTimeout(320);
    const mobileLocatedFocus = {
      center: await screenCenter(page, 'dependency-focus'),
      surface: await visibleRect(page, '#dependencySurface'),
      width: await page.locator('[data-node-id="dependency-focus"]').evaluate(
        (node) => node.getBoundingClientRect().width
      )
    };
    assert(
      mobileLocatedFocus.width >= 120 &&
      Math.abs(mobileLocatedFocus.center.x - mobileLocatedFocus.surface.width / 2) < 5 &&
      Math.abs(
        mobileLocatedFocus.center.y -
        (mobileLocatedFocus.surface.top + mobileLocatedFocus.surface.height * 0.42)
      ) < 2,
      `mobile focused-branch locate is not centered at a readable scale: ${JSON.stringify(mobileLocatedFocus)}`
    );

    await page.locator('#dependencySurface').dispatchEvent('wheel', {
      deltaX: -160,
      deltaY: 0,
      bubbles: true,
      cancelable: true
    });
    await page.waitForTimeout(120);
    const readableUpstream = await page.locator('[data-node-id="prereq-a"]').evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return { left: rect.left, right: rect.right, width: rect.width };
    });
    const readableFocus = await page.locator('[data-node-id="dependency-focus"]').evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return { left: rect.left, right: rect.right, width: rect.width };
    });
    assert(
      readableUpstream.left >= 0 &&
      readableUpstream.right <= 390 &&
      readableUpstream.width >= 125 &&
      readableFocus.left >= 0 &&
      readableFocus.right <= 390,
      `mobile readable viewport does not show focus and direct prerequisites: ${JSON.stringify({ readableUpstream, readableFocus })}`
    );

    const mobilePanOrigin = await page.locator('.dependency-content').getAttribute('transform');
    await page.locator('#dependencySurface').dispatchEvent('wheel', {
      deltaX: 600,
      deltaY: 0,
      bubbles: true,
      cancelable: true
    });
    await page.waitForTimeout(120);
    const downstreamPanRect = await page.locator('[data-node-id="down-one"]').evaluate((node) => {
      const rect = node.getBoundingClientRect();
      return { left: rect.left, right: rect.right, width: rect.width };
    });
    assert(
      downstreamPanRect.left >= 0 &&
      downstreamPanRect.right <= 390 &&
      downstreamPanRect.width >= 125,
      `mobile pan cannot reach direct downstream nodes: ${JSON.stringify(downstreamPanRect)}`
    );
    assert(
      await page.locator('.dependency-content').getAttribute('transform') !== mobilePanOrigin,
      'mobile Dependency horizontal pan did not move the viewport'
    );
    await page.locator('#dependencySurface').dispatchEvent('wheel', {
      deltaX: -600,
      deltaY: 0,
      bubbles: true,
      cancelable: true
    });
    await page.waitForTimeout(120);
    const mobileDependencyTopbar = await visibleRect(page, '.topbar');
    const mobileDepthControls = await visibleRect(page, '.dependency-depth-controls');
    const mobileDependencyTools = await visibleRect(page, '.canvas-tools');
    assert(!overlaps(mobileDependencyTopbar, mobileDepthControls, 1), 'mobile Dependency depth controls overlap topbar');
    assert(mobileDepthControls.right <= 390.5, `mobile Dependency depth controls overflow: ${JSON.stringify(mobileDepthControls)}`);
    assert(mobileDependencyTools.visible, 'mobile Dependency canvas controls are unavailable after Inspector closes');
    assert(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1),
      'mobile Dependency page has horizontal UI overflow'
    );
    const mobileDependencyEvidence = await page.locator('#dependencySvg').evaluate((svg) => {
      const box = svg.querySelector('.dependency-content').getBBox();
      return {
        width: box.width,
        height: box.height,
        nodes: svg.querySelectorAll('.branch-node').length,
        selected: svg.querySelectorAll('[role="option"][aria-selected="true"]').length
      };
    });
    assert(
      mobileDependencyEvidence.width > 700 &&
      mobileDependencyEvidence.height > 300 &&
      mobileDependencyEvidence.selected === 1,
      `mobile Dependency SVG is blank or has invalid selection: ${JSON.stringify(mobileDependencyEvidence)}`
    );
    await page.screenshot({
      path: path.join(process.env.TREEWORK_EVIDENCE_DIR, 'dependency-portrait.png'),
      fullPage: false,
      animations: 'disabled'
    });

    await page.getByRole('button', { name: 'Replay', exact: true }).click();
    await page.getByLabel('Replay timeline').waitFor();
    await page.locator('.replay-stage #projectMapSvg').waitFor();
    const replayNodeVisibility = async (id) =>
      page.locator(`.replay-stage [data-node-id="${id}"]`).evaluate((node) => {
        const nodeRect = node.getBoundingClientRect();
        const surfaceRect = document.querySelector('.replay-stage #mapSurface').getBoundingClientRect();
        return {
          left: nodeRect.left,
          right: nodeRect.right,
          top: nodeRect.top,
          bottom: nodeRect.bottom,
          width: nodeRect.width,
          surfaceLeft: surfaceRect.left,
          surfaceRight: surfaceRect.right,
          surfaceTop: surfaceRect.top,
          surfaceBottom: surfaceRect.bottom,
          fullyVisible:
            nodeRect.left >= surfaceRect.left &&
            nodeRect.right <= surfaceRect.right &&
            nodeRect.top >= surfaceRect.top &&
            nodeRect.bottom <= surfaceRect.bottom
        };
      });
    try {
      await waitFor(async () => {
        const evidence = await replayNodeVisibility('ui-source');
        return evidence.fullyVisible && evidence.width >= 125;
      }, 'readable mobile Replay focus');
    } catch (error) {
      const diagnostic = {
        node: await replayNodeVisibility('ui-source'),
        transform: await page.locator('.replay-stage .map-content').getAttribute('transform'),
        viewport: await page.evaluate(() => {
          const value = JSON.parse(sessionStorage.getItem(`treework-project-map:v3:${location.pathname}`) || 'null');
          return value?.replayViewport;
        })
      };
      throw new Error(`${error.message}: ${JSON.stringify(diagnostic)}`);
    }
    const mobileReplayFocus = await replayNodeVisibility('ui-source');
    assert(
      mobileReplayFocus.fullyVisible && mobileReplayFocus.width >= 125,
      `mobile Replay focus is not readable: ${JSON.stringify(mobileReplayFocus)}`
    );
    const replayMobilePanBefore = await page.locator('.replay-stage .map-content').getAttribute('transform');
    const replayPanDelta = await page.evaluate(() => {
      const root = document.querySelector('.replay-stage [data-node-id="root"]').getBoundingClientRect();
      const surface = document.querySelector('.replay-stage #mapSurface').getBoundingClientRect();
      const session = JSON.parse(
        sessionStorage.getItem(`treework-project-map:v3:${location.pathname}`) || 'null'
      );
      const sensitivity = session?.settings?.panSensitivity || 0.55;
      const targetX = surface.left + surface.width * 0.34;
      const targetY = surface.top + surface.height * 0.46;
      return {
        deltaX: (root.left + root.width / 2 - targetX) / sensitivity,
        deltaY: (root.top + root.height / 2 - targetY) / sensitivity
      };
    });
    await page.locator('.replay-stage #mapSurface').dispatchEvent('wheel', {
      deltaX: replayPanDelta.deltaX,
      deltaY: replayPanDelta.deltaY,
      bubbles: true,
      cancelable: true
    });
    await waitFor(async () => {
      const evidence = await replayNodeVisibility('root');
      return evidence.fullyVisible && evidence.width >= 125;
    }, 'mobile Replay pan to another Tree area');
    await page.waitForTimeout(180);
    const replayMobilePanTransform = await page.locator('.replay-stage .map-content').getAttribute('transform');
    assert(
      replayMobilePanTransform !== replayMobilePanBefore,
      'mobile Replay pan did not move the historical Tree'
    );
    await page.getByRole('button', { name: 'Previous transaction' }).click();
    await waitFor(
      async () =>
        await page.locator('.replay-stage #projectMapSvg').count() === 1 &&
        await page.locator('.replay-stage .map-content').getAttribute('transform') ===
          replayMobilePanTransform,
      'mobile Replay viewport preservation across a transaction step'
    );
    const steppedReplayRoot = await replayNodeVisibility('root');
    assert(
      steppedReplayRoot.fullyVisible && steppedReplayRoot.width >= 125,
      `mobile Replay step recentered the user viewport: ${JSON.stringify(steppedReplayRoot)}`
    );
    await page.getByRole('button', { name: 'Return to Live' }).click();
    await waitFor(
      async () =>
        Number(await page.locator('.replay-view').getAttribute('data-replay-seq')) ===
          priorLiveSequence + 1 &&
        await page.locator('.replay-stage #projectMapSvg').count() === 1,
      'mobile Return to Live scene'
    );
    await page.getByRole('button', { name: 'Locate Replay focus' }).click();
    await waitFor(async () => {
      const evidence = await replayNodeVisibility('ui-source');
      return evidence.fullyVisible && evidence.width >= 125;
    }, 'active mobile Replay locate');
    const mobileReplayTopbar = await visibleRect(page, '.topbar');
    const mobileReplayStage = await visibleRect(page, '.replay-stage');
    const mobileReplayTimeline = await visibleRect(page, '.replay-timeline');
    const mobileReplayDetail = await visibleRect(page, '.replay-transaction-detail');
    assert(!overlaps(mobileReplayTopbar, mobileReplayStage, 1), 'mobile Replay stage overlaps topbar');
    assert(!overlaps(mobileReplayStage, mobileReplayTimeline, 1), 'mobile Replay stage overlaps timeline');
    assert(!overlaps(mobileReplayTimeline, mobileReplayDetail, 1), 'mobile Replay timeline overlaps detail');
    assert(
      mobileReplayStage.height >= 210 &&
      mobileReplayTimeline.right <= 390.5 &&
      mobileReplayDetail.right <= 390.5 &&
      mobileReplayDetail.height >= 145,
      `mobile Replay layout is unreadable: ${JSON.stringify({
        mobileReplayStage,
        mobileReplayTimeline,
        mobileReplayDetail
      })}`
    );
    const mobileReplayEvidence = await page.locator('.replay-stage #projectMapSvg').evaluate((svg) => {
      const box = svg.querySelector('.map-content').getBBox();
      return {
        width: box.width,
        height: box.height,
        nodes: svg.querySelectorAll('.branch-node').length
      };
    });
    assert(
      mobileReplayEvidence.width > 700 &&
      mobileReplayEvidence.height > 500 &&
      mobileReplayEvidence.nodes >= 14,
      `mobile Replay SVG is blank or incomplete: ${JSON.stringify(mobileReplayEvidence)}`
    );
    await page.getByRole('button', { name: 'Open canvas settings' }).click();
    await page.locator('#settingsPanel').waitFor();
    const mobileReplaySettings = await visibleRect(page, '#settingsPanel');
    assert(!overlaps(mobileReplaySettings, mobileReplayTimeline, 1), 'mobile Replay settings overlap timeline');
    assert(!overlaps(mobileReplaySettings, mobileReplayDetail, 1), 'mobile Replay settings overlap detail');
    await page.locator('#closeSettings').click();
    assert(
      await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1),
      'mobile Replay page has horizontal UI overflow'
    );
    await page.screenshot({
      path: path.join(process.env.TREEWORK_EVIDENCE_DIR, 'replay-portrait.png'),
      fullPage: false,
      animations: 'disabled'
    });

    const viewportMatrix = {
      schema_version: 1,
      generated_at: new Date().toISOString(),
      product: 'Strata V3 Project Map',
      desktop: {
        viewport: { width: 1440, height: 900 },
        screenshots: {
          map: 'map-desktop.png',
          dependency: 'dependency-desktop.png',
          replay: 'replay-desktop.png'
        },
        acceptance: 'main real-server workflow'
      },
      tablet: await captureViewportSet(
        context,
        { width: 1024, height: 768 },
        'tablet'
      ),
      portrait: {
        viewport: { width: 390, height: 844 },
        screenshots: {
          map: 'map-portrait.png',
          dependency: 'dependency-portrait.png',
          replay: 'replay-portrait.png'
        },
        acceptance: 'main narrow-screen interaction workflow'
      },
      landscape: await captureViewportSet(
        context,
        { width: 844, height: 390 },
        'landscape'
      ),
      textZoom200: {
        physical_viewport_equivalent: { width: 1440, height: 900 },
        browser_zoom: 2,
        css_viewport: { width: 720, height: 450 },
        evidence: await captureViewportSet(
          context,
          { width: 720, height: 450 },
          'text-zoom-200'
        )
      }
    };
    fs.writeFileSync(
      path.join(process.env.TREEWORK_EVIDENCE_DIR, 'viewport-matrix.json'),
      `${JSON.stringify(viewportMatrix, null, 2)}\n`
    );

    const acceptedProjection = await page.evaluate(async () => {
      const response = await fetch('/api/project-map');
      return response.json();
    });

    const coverageCatalog = await context.request.get(
      new URL('/api/project-map/replay', process.env.TREEWORK_PROJECT_MAP_URL).href
    );
    const coverageLiveSequence = (await coverageCatalog.json()).meta.live_event_seq;
    const coverageSnapshots = {};
    for (const sequence of [coverageLiveSequence - 1, coverageLiveSequence - 2]) {
      const snapshotUrl = new URL('/api/project-map/replay', process.env.TREEWORK_PROJECT_MAP_URL);
      snapshotUrl.searchParams.set('at', String(sequence));
      snapshotUrl.searchParams.set('after', String(sequence));
      const response = await context.request.get(snapshotUrl.href);
      coverageSnapshots[sequence] = await response.json();
    }
    const coveragePage = await context.newPage();
    coveragePage.setDefaultTimeout(12000);
    await coveragePage.route('**/api/project-map/replay**', async (route) => {
      const requestUrl = new URL(route.request().url());
      if (!requestUrl.searchParams.has('at')) {
        await route.continue();
        return;
      }
      const sequence = Number(requestUrl.searchParams.get('at'));
      const body = structuredClone(coverageSnapshots[sequence]);
      assert(body, `missing prefetched coverage snapshot for sequence ${sequence}`);
      const partial = sequence === coverageLiveSequence - 1;
      body.reconstruction = {
        status: partial ? 'partial' : 'unavailable',
        gaps: [{
          from_seq: sequence,
          to_seq: sequence,
          reason: partial
            ? 'Synthetic partial browser coverage gap'
            : 'Synthetic unavailable browser coverage gap'
        }]
      };
      if (!partial) body.state = null;
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(body)
      });
    });
    await coveragePage.goto(process.env.TREEWORK_PROJECT_MAP_URL, { waitUntil: 'domcontentloaded' });
    await coveragePage.locator('[data-node-id="ui-source"]').waitFor();
    await coveragePage.getByRole('button', { name: 'Replay', exact: true }).click();
    await coveragePage.getByLabel('Replay timeline').waitFor();
    await coveragePage.getByRole('button', { name: 'Previous transaction' }).click();
    await coveragePage.getByText('Historical coverage is partial', { exact: true }).waitFor();
    assert(
      await coveragePage.locator('.replay-stage #projectMapSvg').count() === 0 &&
      await coveragePage.getByText('Synthetic partial browser coverage gap', { exact: false }).count() === 1 &&
      await coveragePage.locator('.replay-transaction-detail').count() === 1,
      'partial Replay coverage leaked a scene or hid transaction detail'
    );
    await coveragePage.getByRole('button', { name: 'Previous transaction' }).click();
    await coveragePage.getByText('Historical scene is unavailable', { exact: true }).waitFor();
    const unavailableEvidence = {
      scenes: await coveragePage.locator('.replay-stage #projectMapSvg').count(),
      gaps: await coveragePage.getByText('Synthetic unavailable browser coverage gap', { exact: false }).count(),
      coverage: await coveragePage.locator('.replay-scene-state').innerText()
    };
    assert(
      unavailableEvidence.scenes === 0 && unavailableEvidence.gaps === 1,
      `unavailable Replay coverage leaked a live or stale scene: ${JSON.stringify(unavailableEvidence)}`
    );
    await coveragePage.unrouteAll({ behavior: 'wait' });
    await coveragePage.close();

    const emptyPage = await context.newPage();
    await emptyPage.route('**/api/project-map', async (route) => {
      if (new URL(route.request().url()).pathname === '/api/project-map') {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify({
            ...acceptedProjection,
            nodes: [],
            dependencies: []
          })
        });
      } else {
        await route.continue();
      }
    });
    await emptyPage.goto(process.env.TREEWORK_PROJECT_MAP_URL, { waitUntil: 'domcontentloaded' });
    await emptyPage.getByText('No accepted branches', { exact: true }).waitFor();
    await emptyPage.screenshot({
      path: path.join(process.env.TREEWORK_EVIDENCE_DIR, 'map-empty.png'),
      fullPage: false,
      animations: 'disabled'
    });
    await emptyPage.close();

    const errorPage = await context.newPage();
    await errorPage.route('**/api/project-map', async (route) => {
      if (new URL(route.request().url()).pathname === '/api/project-map') {
        await route.fulfill({
          status: 503,
          contentType: 'application/json',
          body: JSON.stringify({ ok: false, error: 'Synthetic unavailable projection' })
        });
      } else {
        await route.continue();
      }
    });
    await errorPage.goto(process.env.TREEWORK_PROJECT_MAP_URL, { waitUntil: 'domcontentloaded' });
    await errorPage.getByText('Project Map is unavailable', { exact: true }).waitFor();
    await errorPage.screenshot({
      path: path.join(process.env.TREEWORK_EVIDENCE_DIR, 'map-error.png'),
      fullPage: false,
      animations: 'disabled'
    });
    await errorPage.close();

    const writeResult = await context.request.post(
      new URL('/api/transaction', process.env.TREEWORK_PROJECT_MAP_URL).href,
      { data: { action: 'branch.move' } }
    );
    const writeResponse = {
      status: writeResult.status(),
      body: await writeResult.json()
    };
    assert(writeResponse.status === 405 && writeResponse.body.ok === false, `write endpoint was not rejected: ${JSON.stringify(writeResponse)}`);
    const failedResponseDetails = await Promise.all(failedResponses);
    assert(
      consoleErrors.length === 0,
      `browser console errors: ${JSON.stringify({
        consoleErrors,
        failedResponses: failedResponseDetails
      })}`
    );
    assert(pageErrors.length === 0, `browser page errors: ${JSON.stringify(pageErrors)}`);
  } finally {
    await context.close();
    await browser.close();
  }
})().catch((error) => {
  console.error(error && error.stack ? error.stack : error);
  process.exit(1);
});
""",
            encoding="utf-8",
        )

        env = environment(build_dir)
        env.update(
            {
                "TREEWORK_PROJECT_MAP_URL": url,
                "TREEWORK_WORKSPACE": str(workspace),
                "TREEWORK_UI_SOURCE_ARTIFACT_DIR": str(
                    branch_artifact_dir(workspace, "ui-source")
                ),
                "TREEWORK_TW": str(TW),
                "TREEWORK_UPDATED_TREE": str(updated_tree),
                "TREEWORK_EVIDENCE_DIR": str(EVIDENCE_DIR),
                "NODE_PATH": str(NODE_MODULES),
            }
        )
        result = subprocess.run(
            [str(NODE), str(smoke_js)],
            cwd=PLUGIN_ROOT,
            env=env,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=180,
        )
        if result.returncode != 0:
            fail(
                "Project Map browser acceptance failed\n"
                f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
            )

        after_state = branches_path.read_bytes()
        if after_state == before_state:
            fail("browser acceptance did not exercise lifecycle and topology updates")
        graph = json.loads(
            (workspace / ".TreeWork" / "out" / "graph.json").read_text(
                encoding="utf-8"
            )
        )
        if "notes" in graph or "edit_commands" in graph:
            fail("read-only compatibility projection contains write-oriented state")
        for screenshot in [
            "map-desktop.png",
            "dependency-desktop.png",
            "replay-desktop.png",
            "map-tablet.png",
            "dependency-tablet.png",
            "replay-tablet.png",
            "map-portrait.png",
            "dependency-portrait.png",
            "replay-portrait.png",
            "map-landscape.png",
            "dependency-landscape.png",
            "replay-landscape.png",
            "map-text-zoom-200.png",
            "dependency-text-zoom-200.png",
            "replay-text-zoom-200.png",
            "map-empty.png",
            "map-error.png",
        ]:
            path = EVIDENCE_DIR / screenshot
            if not path.is_file() or path.stat().st_size < 10_000:
                fail(f"missing or empty browser evidence {path}")
        viewport_matrix = EVIDENCE_DIR / "viewport-matrix.json"
        if not viewport_matrix.is_file():
            fail(f"missing browser viewport matrix {viewport_matrix}")
        print(
            "ok: production Project Map browser acceptance "
            f"screenshots={EVIDENCE_DIR}"
        )
    finally:
        if server is not None:
            server.terminate()
            try:
                server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait(timeout=5)
        shutil.rmtree(temp_root, ignore_errors=True)


if __name__ == "__main__":
    main()
