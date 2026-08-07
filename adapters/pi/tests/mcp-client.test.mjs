import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { test } from "node:test";
import { setTimeout as delay } from "node:timers/promises";
import { TreeWorkMcpClient } from "../mcp-client.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(here, "../../..");
const pluginRoot = join(repositoryRoot, "plugins", "treework");
const tw = join(pluginRoot, "skills", "treework", "scripts", "tw");

test("Pi MCP client calls the shipped TreeWork server", { timeout: 180_000 }, async () => {
  const temp = mkdtempSync(join(tmpdir(), "treework-pi-mcp-"));
  const workspace = join(temp, "workspace");
  const buildDir = join(temp, "build");
  const env = {
    ...process.env,
    TREEWORK_PLUGIN_ROOT: pluginRoot,
    TREEWORK_BUILD_DIR: buildDir,
  };
  mkdirSync(workspace);
  execFileSync(tw, ["init"], { cwd: workspace, env, stdio: "pipe" });

  const client = new TreeWorkMcpClient(pluginRoot, buildDir, "test");
  try {
    const result = await client.callTool("treework_check", { workspace });
    assert.equal(result.isError, false);
    assert.equal(result.structuredContent?.ok, true);
    assert.match(result.content[0]?.text ?? "", /TreeWork check:/);
  } finally {
    client.close();
    await delay(1200);
    rmSync(temp, { recursive: true, force: true });
  }
});

test("MCP client skips notifications and preserves Unicode line separators", async () => {
  const temp = mkdtempSync(join(tmpdir(), "treework-pi-fake-mcp-"));
  const scripts = join(temp, "scripts");
  mkdirSync(scripts);
  const launcher = join(scripts, "start-mcp.sh");
  const server = join(temp, "server.py");
  writeFileSync(launcher, '#!/usr/bin/env bash\nexec python3 "$(dirname "$0")/../server.py"\n');
  chmodSync(launcher, 0o755);
  writeFileSync(
    server,
    `import json, sys
for raw in sys.stdin:
    request = json.loads(raw)
    if request.get("method") == "initialize":
        print(json.dumps({"jsonrpc":"2.0","method":"server/ready","params":{}}, ensure_ascii=False), flush=True)
        print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"fake","version":"1"}}}, ensure_ascii=False), flush=True)
    elif request.get("method") == "tools/call":
        print(json.dumps({"jsonrpc":"2.0","id":request["id"],"result":{"content":[{"type":"text","text":"before\\u2028after"}],"isError":False}}, ensure_ascii=False), flush=True)
`,
  );

  const client = new TreeWorkMcpClient(temp, join(temp, "build"), "test");
  try {
    const result = await client.callTool("fake", {});
    assert.equal(result.content[0].text, "before\u2028after");
  } finally {
    client.close();
    await delay(50);
    rmSync(temp, { recursive: true, force: true });
  }
});

test("MCP client rejects pre-cancelled calls before spawning", async () => {
  const controller = new AbortController();
  controller.abort();
  const client = new TreeWorkMcpClient("/path/that/does/not/exist", "/tmp/unused", "test");
  await assert.rejects(client.callTool("fake", {}, controller.signal), /cancelled/);
});

test("MCP client reports spawn failures", async () => {
  const client = new TreeWorkMcpClient("/path/that/does/not/exist", "/tmp/unused", "test");
  await assert.rejects(client.callTool("fake", {}), /failed to start|exited/);
  client.close();
});
