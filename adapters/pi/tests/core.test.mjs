import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { test } from "node:test";
import {
  activateTreeWorkTools,
  containsGeneratedTreeWorkMarker,
  extractTreeWorkWorkspace,
  isProtectedTreeWorkPath,
  resetTreeWorkTools,
  shouldBlockProtectedTreeWorkAccess,
} from "../core.mjs";

test("protects machine-owned TreeWork state, including symlink aliases", () => {
  const temp = mkdtempSync(join(tmpdir(), "treework-pi-path-"));
  try {
    const state = join(temp, ".TreeWork", "state");
    mkdirSync(state, { recursive: true });
    symlinkSync(state, join(temp, "state-link"));
    assert.equal(isProtectedTreeWorkPath(".TreeWork/state/project.json", temp), true);
    assert.equal(isProtectedTreeWorkPath(join(temp, ".TreeWork", "events.jsonl"), temp), true);
    assert.equal(isProtectedTreeWorkPath("state-link/new-state.json", temp), true);
    assert.equal(
      shouldBlockProtectedTreeWorkAccess(
        "bash",
        { command: "printf hacked > state-link/project.json" },
        temp,
      ),
      true,
    );
    assert.equal(
      shouldBlockProtectedTreeWorkAccess(
        "bash",
        { command: "printf hacked > .TreeWork/st?te/project.json" },
        temp,
      ),
      true,
    );
    assert.equal(isProtectedTreeWorkPath(".TreeWork/progress.md", temp), false);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
});

test("protects generated TreeWork blocks", () => {
  assert.equal(containsGeneratedTreeWorkMarker("<!-- treework:status:start -->"), true);
  assert.equal(containsGeneratedTreeWorkMarker("ordinary markdown"), false);
});

test("blocks explicit file and common split-path shell access to protected internals", () => {
  assert.equal(
    shouldBlockProtectedTreeWorkAccess("read", { path: ".TreeWork/state/project.json" }, "/repo"),
    true,
  );
  assert.equal(
    shouldBlockProtectedTreeWorkAccess("edit", { path: ".TreeWork/state/project.json" }, "/repo"),
    true,
  );
  assert.equal(
    shouldBlockProtectedTreeWorkAccess(
      "write",
      { path: ".TreeWork/progress.md", content: "ok" },
      "/repo",
    ),
    false,
  );
  assert.equal(
    shouldBlockProtectedTreeWorkAccess(
      "bash",
      { command: "cd .TreeWork && cat state/project.json" },
      "/repo",
    ),
    true,
  );
  assert.equal(
    shouldBlockProtectedTreeWorkAccess("bash", { command: "rm .TreeWork/events.jsonl" }, "/repo"),
    true,
  );
  assert.equal(
    shouldBlockProtectedTreeWorkAccess("bash", { command: "cargo test" }, "/repo"),
    false,
  );
});

test("keeps one loader active and expands only the requested capability", () => {
  assert.deepEqual(
    resetTreeWorkTools(["read", "treework_recall", "treework_check"]),
    ["read", "treework_tools"],
  );
  assert.deepEqual(
    activateTreeWorkTools(["read", "treework_tools"], "map"),
    ["read", "treework_tools", "treework_project_map"],
  );
  assert.deepEqual(
    activateTreeWorkTools(["treework_tools"], "memory"),
    ["treework_tools", "treework_recall", "treework_check"],
  );
});

test("extracts only absolute workspaces from enter output", () => {
  assert.equal(
    extractTreeWorkWorkspace("Isolation:\n  workspace: /tmp/project-branch\n  status: created\n"),
    "/tmp/project-branch",
  );
  assert.equal(extractTreeWorkWorkspace("workspace: relative/path"), undefined);
});
