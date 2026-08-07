import { existsSync, mkdirSync, readFileSync, realpathSync, rmSync } from "node:fs";
import { homedir, platform } from "node:os";
import { basename, dirname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { StringEnum } from "@earendil-works/pi-ai";
import {
  DEFAULT_MAX_BYTES,
  DEFAULT_MAX_LINES,
  formatSize,
  SessionManager,
  truncateHead,
  type ExtensionAPI,
  type ExtensionCommandContext,
} from "@earendil-works/pi-coding-agent";
import { Type } from "typebox";
import {
  activateTreeWorkTools,
  extractTreeWorkWorkspace,
  resetTreeWorkTools,
  shouldBlockProtectedTreeWorkAccess,
  TREEWORK_TOOLS,
} from "./core.mjs";
import { TreeWorkMcpClient } from "./mcp-client.mjs";

const adapterRoot = dirname(fileURLToPath(import.meta.url));
const repositoryRoot = resolve(adapterRoot, "../..");
const pluginRoot = join(repositoryRoot, "plugins", "treework");
const skillRoot = join(pluginRoot, "skills", "treework");
const twPath = join(skillRoot, "scripts", "tw");
const adapterVersion = JSON.parse(readFileSync(join(repositoryRoot, "package.json"), "utf8")).version;
const agentDir = process.env.PI_CODING_AGENT_DIR || join(homedir(), ".pi", "agent");
const buildDir = join(agentDir, "cache", "treework");

interface ControlDescriptor {
  version: number;
  project_id: string;
  control_root: string;
}

interface StrictControlDescriptor extends ControlDescriptor {
  controlRoot: string;
  commonDir: string;
}

interface WorktreeBinding {
  version: number;
  project_id: string;
  branch: string;
  workspace: string;
}

interface EnterCommand {
  branch: string;
  recall: boolean;
  resume: boolean;
}

interface PreparedFork {
  targetFile: string;
  target: string;
}

function commandOutput(result: { stdout: string; stderr: string }): string {
  return [result.stdout.trim(), result.stderr.trim()].filter(Boolean).join("\n");
}

function findTreeWorkAncestor(cwd: string): string | undefined {
  let current: string;
  try {
    current = realpathSync(cwd);
  } catch {
    return undefined;
  }
  while (true) {
    if (existsSync(join(current, ".TreeWork"))) return current;
    const parent = dirname(current);
    if (parent === current) return undefined;
    current = parent;
  }
}

function parseEnterCommand(args: string): EnterCommand {
  const tokens = args.trim().split(/\s+/).filter(Boolean);
  const branch = tokens.shift();
  if (!branch || branch.length > 512) {
    throw new Error("Usage: /treework-enter <branch> [--recall]");
  }
  const unknown = tokens.filter((token) => token !== "--recall" && token !== "--no-resume");
  if (unknown.length) throw new Error(`Unknown TreeWork Enter option(s): ${unknown.join(", ")}`);
  return {
    branch,
    recall: tokens.includes("--recall"),
    resume: !tokens.includes("--no-resume"),
  };
}

function parseReturnCommand(args: string): { resume: boolean } {
  const tokens = args.trim().split(/\s+/).filter(Boolean);
  const unknown = tokens.filter((token) => token !== "--no-resume");
  if (unknown.length) throw new Error(`Unknown TreeWork Return option(s): ${unknown.join(", ")}`);
  return { resume: !tokens.includes("--no-resume") };
}

function boundedText(content: Array<{ type: "text"; text: string }>) {
  const text = content.map((part) => part.text).join("\n");
  const truncation = truncateHead(text, {
    maxBytes: DEFAULT_MAX_BYTES - 1024,
    maxLines: DEFAULT_MAX_LINES - 3,
  });
  const suffix = truncation.truncated
    ? `\n\n[TreeWork output truncated: showing ${truncation.outputLines}/${truncation.totalLines} lines and ${formatSize(truncation.outputBytes)}/${formatSize(truncation.totalBytes)}.]`
    : "";
  return {
    content: [{ type: "text" as const, text: `${truncation.content}${suffix}` }],
    truncation: {
      truncated: truncation.truncated,
      truncatedBy: truncation.truncatedBy,
      totalLines: truncation.totalLines,
      totalBytes: truncation.totalBytes,
    },
  };
}

async function runTw(
  pi: ExtensionAPI,
  cwd: string,
  args: string[],
  signal?: AbortSignal,
  timeout = 120_000,
) {
  mkdirSync(buildDir, { recursive: true });
  return pi.exec(
    "env",
    [
      `TREEWORK_PLUGIN_ROOT=${pluginRoot}`,
      `TREEWORK_BUILD_DIR=${buildDir}`,
      twPath,
      ...args,
    ],
    { cwd, signal, timeout },
  );
}

async function gitPath(pi: ExtensionAPI, cwd: string, flag: "--git-dir" | "--git-common-dir") {
  const result = await pi.exec("git", ["rev-parse", flag], { cwd, timeout: 10_000 });
  if (result.code !== 0) {
    throw new Error(commandOutput(result) || `git rev-parse ${flag} failed`);
  }
  const value = result.stdout.trim();
  return realpathSync(isAbsolute(value) ? value : resolve(cwd, value));
}

async function treeWorkControlDescriptor(
  pi: ExtensionAPI,
  cwd: string,
): Promise<StrictControlDescriptor> {
  const commonDir = await gitPath(pi, cwd, "--git-common-dir");
  const descriptorPath = join(commonDir, "treework", "control.json");
  let descriptor: ControlDescriptor;
  try {
    descriptor = JSON.parse(readFileSync(descriptorPath, "utf8")) as ControlDescriptor;
  } catch (error) {
    throw new Error(`TreeWork control descriptor is unavailable at ${descriptorPath}: ${String(error)}`);
  }
  if (
    descriptor.version !== 1 ||
    !descriptor.project_id ||
    !descriptor.control_root ||
    !isAbsolute(descriptor.control_root)
  ) {
    throw new Error(`TreeWork control descriptor is invalid: ${descriptorPath}`);
  }
  const controlRoot = realpathSync(descriptor.control_root);
  const controlCommonDir = await gitPath(pi, controlRoot, "--git-common-dir");
  if (controlCommonDir !== commonDir) {
    throw new Error("TreeWork control descriptor does not belong to the current Git repository");
  }
  return { ...descriptor, controlRoot, commonDir };
}

async function treeWorkControlRoot(pi: ExtensionAPI, cwd: string): Promise<string> {
  return (await treeWorkControlDescriptor(pi, cwd)).controlRoot;
}

async function assertTreeWorkWorkspace(
  pi: ExtensionAPI,
  source: string,
  target: string,
): Promise<{ target: string; controlRoot: string; kind: "control" | "branch" }> {
  if (!isAbsolute(target)) throw new Error("TreeWork workspace switch requires an absolute path");
  const canonical = realpathSync(target);
  const sourceDescriptor = await treeWorkControlDescriptor(pi, source);
  const targetDescriptor = await treeWorkControlDescriptor(pi, canonical);
  if (
    sourceDescriptor.commonDir !== targetDescriptor.commonDir ||
    sourceDescriptor.project_id !== targetDescriptor.project_id ||
    sourceDescriptor.controlRoot !== targetDescriptor.controlRoot
  ) {
    throw new Error("Refusing to switch across different TreeWork projects");
  }

  const status = await runTw(pi, canonical, ["check", "--brief"], undefined, 60_000);
  if (status.code !== 0) {
    throw new Error(commandOutput(status) || "TreeWork runtime rejected the target workspace");
  }

  if (canonical === targetDescriptor.controlRoot) {
    return { target: canonical, controlRoot: canonical, kind: "control" };
  }

  const gitDir = await gitPath(pi, canonical, "--git-dir");
  const bindingPath = join(gitDir, "treework-branch.json");
  let binding: WorktreeBinding;
  try {
    binding = JSON.parse(readFileSync(bindingPath, "utf8")) as WorktreeBinding;
  } catch (error) {
    throw new Error(`Refusing to switch: invalid TreeWork branch binding at ${bindingPath}: ${String(error)}`);
  }
  if (
    binding.version !== 1 ||
    binding.project_id !== targetDescriptor.project_id ||
    !binding.branch ||
    !isAbsolute(binding.workspace) ||
    realpathSync(binding.workspace) !== canonical
  ) {
    throw new Error(`Refusing to switch: ${canonical} does not match its TreeWork branch binding`);
  }
  return {
    target: canonical,
    controlRoot: targetDescriptor.controlRoot,
    kind: "branch",
  };
}

async function resolveMcpWorkspace(pi: ExtensionAPI, cwd: string): Promise<string> {
  const descriptor = await treeWorkControlDescriptor(pi, cwd);
  const status = await runTw(pi, cwd, ["check", "--brief"], undefined, 60_000);
  if (status.code !== 0) throw new Error(commandOutput(status) || "TreeWork workspace is invalid");
  return descriptor.controlRoot;
}

function requirePersistedSession(ctx: { sessionManager: { getSessionFile(): string | undefined } }): string {
  const sessionFile = ctx.sessionManager.getSessionFile();
  if (!sessionFile || !existsSync(sessionFile)) {
    throw new Error("TreeWork workspace handoff requires a persisted Pi session; restart without --no-session");
  }
  return sessionFile;
}

function prepareSessionFork(sourceSession: string, target: string): PreparedFork {
  const targetSession = SessionManager.forkFrom(sourceSession, target);
  targetSession.appendSessionInfo(`TreeWork · ${basename(target)}`);
  const targetFile = targetSession.getSessionFile();
  if (!targetFile || !existsSync(targetFile)) {
    throw new Error("TreeWork failed to prepare the target Pi session");
  }
  return { targetFile, target };
}

async function switchPreparedSession(
  ctx: ExtensionCommandContext,
  prepared: PreparedFork,
  kind: "control" | "branch",
  handoffMessage: string,
  resume: boolean,
): Promise<boolean> {
  const switched = await ctx.switchSession(prepared.targetFile, {
    withSession: async (newCtx) => {
      newCtx.ui.notify(
        `TreeWork moved this conversation to the ${kind} workspace:\n${prepared.target}`,
        "info",
      );
      try {
        await newCtx.sendMessage(
          {
            customType: "treework-handoff",
            content: handoffMessage,
            display: true,
          },
          resume
            ? { triggerTurn: true, deliverAs: "followUp" }
            : { triggerTurn: false, deliverAs: "nextTurn" },
        );
      } catch (error) {
        newCtx.ui.notify(`TreeWork switched workspaces, but could not queue the resume message: ${String(error)}`, "warning");
      }
    },
  });
  if (switched.cancelled) {
    rmSync(prepared.targetFile, { force: true });
    return false;
  }
  return true;
}

async function pauseAfterFailedEnter(
  pi: ExtensionAPI,
  controlRoot: string,
  reason: string,
): Promise<string> {
  const paused = await runTw(
    pi,
    controlRoot,
    ["pause", "--reason", `Pi workspace handoff failed: ${reason}`],
    undefined,
    60_000,
  );
  return commandOutput(paused) || `tw pause exited ${paused.code}`;
}

export default function treeWorkPiAdapter(pi: ExtensionAPI) {
  const mcp = new TreeWorkMcpClient(pluginRoot, buildDir, adapterVersion);
  let lastCheckNotice = "";

  pi.on("resources_discover", () => ({ skillPaths: [skillRoot] }));

  pi.registerTool({
    name: "treework_recall",
    label: "TreeWork Recall",
    description:
      "Recover one TreeWork branch from the committed projection, including documents, relationships, isolation, allowed actions, and blockers.",
    parameters: Type.Object({
      workspace: Type.Optional(Type.String({ description: "TreeWork path; defaults to current Pi cwd" })),
      branch: Type.Optional(Type.String({ description: "Branch path; defaults to the current TreeWork branch" })),
      max_chars: Type.Optional(Type.Integer({ minimum: 1000, maximum: 50000 })),
    }),
    async execute(_id, params, signal, _onUpdate, ctx) {
      const workspace = await resolveMcpWorkspace(pi, resolve(params.workspace ?? ctx.cwd));
      const result = await mcp.callTool(
        "treework_recall",
        {
          workspace,
          ...(params.branch ? { branch: params.branch } : {}),
          ...(params.max_chars ? { max_chars: params.max_chars } : {}),
        },
        signal,
      );
      const bounded = boundedText(result.content);
      return {
        content: bounded.content,
        details: {
          workspace,
          branch: params.branch ?? null,
          ...bounded.truncation,
        },
      };
    },
  });

  pi.registerTool({
    name: "treework_project_map",
    label: "TreeWork Project Map",
    description:
      "Start or reuse TreeWork's read-only localhost Project Map without changing accepted project state.",
    parameters: Type.Object({
      workspace: Type.Optional(Type.String({ description: "TreeWork path; defaults to current Pi cwd" })),
      open: Type.Optional(
        Type.Boolean({ description: "Explicitly open the returned localhost URL in the system browser" }),
      ),
    }),
    async execute(_id, params, signal, _onUpdate, ctx) {
      const workspace = await resolveMcpWorkspace(pi, resolve(params.workspace ?? ctx.cwd));
      const result = await mcp.callTool("treework_project_map", { workspace }, signal);
      const url = result.structuredContent?.url;
      if (params.open && typeof url === "string") {
        const opener = platform() === "darwin" ? "open" : "xdg-open";
        const opened = await pi.exec(opener, [url], { signal, timeout: 10_000 });
        if (opened.code !== 0) throw new Error(commandOutput(opened) || `${opener} failed`);
      }
      const bounded = boundedText(result.content);
      return {
        content: bounded.content,
        details: {
          ...(result.structuredContent ?? {}),
          opened: Boolean(params.open),
          ...bounded.truncation,
        },
      };
    },
  });

  pi.registerTool({
    name: "treework_check",
    label: "TreeWork Check",
    description: "Run TreeWork's read-only consistency check against the authoritative control workspace.",
    parameters: Type.Object({
      workspace: Type.Optional(Type.String({ description: "TreeWork path; defaults to current Pi cwd" })),
    }),
    async execute(_id, params, signal, _onUpdate, ctx) {
      const workspace = await resolveMcpWorkspace(pi, resolve(params.workspace ?? ctx.cwd));
      const result = await mcp.callTool("treework_check", { workspace }, signal);
      const bounded = boundedText(result.content);
      return {
        content: bounded.content,
        details: {
          workspace,
          ok: result.structuredContent?.ok,
          ...bounded.truncation,
        },
      };
    },
  });

  pi.registerTool({
    name: "treework_tools",
    label: "TreeWork Tools",
    description:
      "Load TreeWork read-only tools on demand. Use memory for Recall/Check, map for Project Map, or all only when both are needed. Enter and Return are explicit Pi commands taught by the TreeWork Skill.",
    promptSnippet: "Load TreeWork memory or Project Map tools only when TreeWork is needed",
    promptGuidelines: [
      "Use treework_tools before TreeWork operations when the required TreeWork tool is not active.",
    ],
    parameters: Type.Object({
      capability: StringEnum(["memory", "map", "all"] as const),
    }),
    async execute(_id, params) {
      const active = pi.getActiveTools();
      const next = activateTreeWorkTools(active, params.capability);
      const added = next.filter((name) => !active.includes(name));
      pi.setActiveTools(next);
      return {
        content: [
          {
            type: "text" as const,
            text: added.length
              ? `Loaded TreeWork tools: ${added.join(", ")}`
              : "Requested TreeWork tools are already active.",
          },
        ],
        details: { capability: params.capability, added },
      };
    },
  });

  pi.on("session_start", () => {
    pi.setActiveTools(resetTreeWorkTools(pi.getActiveTools()));
  });

  pi.on("tool_call", (event, ctx) => {
    if (!findTreeWorkAncestor(ctx.cwd)) return undefined;
    if (!shouldBlockProtectedTreeWorkAccess(event.toolName, event.input, ctx.cwd)) return undefined;
    const reason =
      "TreeWork guardrail: use supported TreeWork CLI/MCP surfaces for machine-owned state, events, and generated blocks; mutations occur only through tw transactions.";
    if (ctx.hasUI) ctx.ui.notify(reason, "warning");
    return { block: true, reason };
  });

  pi.on("agent_settled", async (_event, ctx) => {
    if (!findTreeWorkAncestor(ctx.cwd)) return;
    const result = await runTw(pi, ctx.cwd, ["check", "--brief"], undefined, 60_000);
    const check = commandOutput(result);
    if (result.code === 0 && check.includes("TreeWork check: 0 issue(s)")) {
      lastCheckNotice = "";
      return;
    }
    const notice = `TreeWork stop check needs attention. ${check || `tw check exited ${result.code}`}`;
    if (notice === lastCheckNotice) return;
    lastCheckNotice = notice;
    if (ctx.hasUI) ctx.ui.notify(notice, "warning");
    pi.sendMessage(
      { customType: "treework-check", content: notice, display: true },
      { deliverAs: "nextTurn" },
    );
  });

  pi.on("session_shutdown", () => {
    mcp.close();
  });

  pi.registerCommand("treework-enter", {
    description: "Enter one accepted TreeWork branch and move this conversation into its managed worktree",
    handler: async (args, ctx) => {
      const request = parseEnterCommand(args);
      await ctx.waitForIdle();
      const sourceSession = requirePersistedSession(ctx);
      const sourceDescriptor = await treeWorkControlDescriptor(pi, ctx.cwd);
      const sourceStatus = await runTw(pi, ctx.cwd, ["check", "--brief"], undefined, 60_000);
      if (sourceStatus.code !== 0) {
        throw new Error(commandOutput(sourceStatus) || "TreeWork source workspace is invalid");
      }

      const preview = await runTw(
        pi,
        ctx.cwd,
        ["enter", request.branch, "--dry-run"],
        undefined,
        120_000,
      );
      const previewOutput = commandOutput(preview);
      if (preview.code !== 0) throw new Error(previewOutput || `tw enter --dry-run exited ${preview.code}`);
      const predicted = extractTreeWorkWorkspace(previewOutput);
      if (!predicted) throw new Error(`tw enter --dry-run did not return a workspace:\n${previewOutput}`);

      let prepared: PreparedFork;
      try {
        prepared = prepareSessionFork(sourceSession, predicted);
      } catch (error) {
        throw new Error(`TreeWork Enter stopped before changing state: ${String(error)}`);
      }

      const enterArgs = ["enter", request.branch];
      if (request.recall) enterArgs.push("--recall");
      const entered = await runTw(pi, ctx.cwd, enterArgs, undefined, 180_000);
      const enteredOutput = commandOutput(entered);
      if (entered.code !== 0) {
        rmSync(prepared.targetFile, { force: true });
        throw new Error(enteredOutput || `tw enter exited ${entered.code}`);
      }
      const workspace = extractTreeWorkWorkspace(enteredOutput);
      if (!workspace || resolve(workspace) !== resolve(predicted)) {
        rmSync(prepared.targetFile, { force: true });
        const pause = await pauseAfterFailedEnter(pi, sourceDescriptor.controlRoot, "workspace prediction mismatch");
        throw new Error(`TreeWork Enter paused after an unsafe handoff: ${pause}`);
      }

      try {
        const validated = await assertTreeWorkWorkspace(pi, ctx.cwd, workspace);
        const switched = await switchPreparedSession(
          ctx,
          prepared,
          validated.kind,
          `TreeWork Enter completed for ${request.branch}.\n\n${enteredOutput}`,
          request.resume,
        );
        if (switched) return;
        const pause = await pauseAfterFailedEnter(pi, sourceDescriptor.controlRoot, "Pi session switch was cancelled");
        ctx.ui.notify(`TreeWork Enter was paused because Pi cancelled the workspace switch.\n${pause}`, "warning");
      } catch (error) {
        rmSync(prepared.targetFile, { force: true });
        const pause = await pauseAfterFailedEnter(pi, sourceDescriptor.controlRoot, String(error));
        throw new Error(`TreeWork Enter could not complete its Pi handoff and was paused. ${pause}`);
      }
    },
  });

  pi.registerCommand("treework-return", {
    description: "Return this conversation from a branch worktree to its validated TreeWork control workspace",
    handler: async (args, ctx) => {
      const request = parseReturnCommand(args);
      await ctx.waitForIdle();
      const sourceSession = requirePersistedSession(ctx);
      const controlRoot = await treeWorkControlRoot(pi, ctx.cwd);
      if (realpathSync(ctx.cwd) === controlRoot) {
        ctx.ui.notify(`Already in TreeWork control workspace: ${controlRoot}`, "info");
        return;
      }
      const validated = await assertTreeWorkWorkspace(pi, ctx.cwd, controlRoot);
      const prepared = prepareSessionFork(sourceSession, validated.target);
      const switched = await switchPreparedSession(
        ctx,
        prepared,
        "control",
        `TreeWork returned this conversation to the control workspace: ${validated.target}`,
        request.resume,
      );
      if (!switched) ctx.ui.notify("Pi cancelled the TreeWork return; the branch session remains active.", "warning");
    },
  });

  pi.registerCommand("treework-adapter", {
    description: "Show TreeWork Pi adapter and runtime status",
    handler: async (_args, ctx) => {
      const version = await runTw(pi, ctx.cwd, ["version"], undefined, 120_000);
      const state = findTreeWorkAncestor(ctx.cwd) ? "initialized" : "not initialized";
      const text = [
        `TreeWork Pi adapter ${adapterVersion}: ${version.code === 0 ? version.stdout.trim() : "runtime unavailable"}`,
        `Workspace: ${ctx.cwd}`,
        `Project state: ${state}`,
        `Active TreeWork tools: ${pi.getActiveTools().filter((name) => TREEWORK_TOOLS.includes(name)).join(", ") || "deferred"}`,
        `Skill: ${skillRoot}`,
      ].join("\n");
      ctx.ui.notify(text, version.code === 0 ? "info" : "error");
    },
  });
}
