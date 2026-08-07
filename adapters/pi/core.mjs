import { existsSync, globSync, realpathSync } from "node:fs";
import { basename, dirname, isAbsolute, normalize, resolve, sep } from "node:path";

const GENERATED_MARKERS = [
  "treework:status:start",
  "treework:root-status:start",
  "treework:branch-table:start",
];

export const TREEWORK_CAPABILITY_TOOLS = Object.freeze({
  memory: Object.freeze(["treework_recall", "treework_check"]),
  map: Object.freeze(["treework_project_map"]),
});

export const TREEWORK_TOOLS = Object.freeze([
  ...TREEWORK_CAPABILITY_TOOLS.memory,
  ...TREEWORK_CAPABILITY_TOOLS.map,
]);

function normalizedPath(path) {
  return normalize(path).split(sep).join("/");
}

function canonicalizeLoose(path) {
  let existing = path;
  const missing = [];
  while (!existsSync(existing)) {
    const parent = dirname(existing);
    if (parent === existing) break;
    missing.unshift(basename(existing));
    existing = parent;
  }
  try {
    return resolve(realpathSync(existing), ...missing);
  } catch {
    return path;
  }
}

function hasProtectedPathShape(path) {
  const normalized = normalizedPath(path);
  return (
    normalized.includes("/.TreeWork/state/") ||
    normalized.endsWith("/.TreeWork/state") ||
    normalized.endsWith("/.TreeWork/events.jsonl")
  );
}

export function isProtectedTreeWorkPath(path, cwd = process.cwd()) {
  if (typeof path !== "string" || path.trim() === "") return false;
  const absolute = isAbsolute(path) ? path : resolve(cwd, path);
  return hasProtectedPathShape(absolute) || hasProtectedPathShape(canonicalizeLoose(absolute));
}

export function containsGeneratedTreeWorkMarker(value) {
  if (typeof value !== "string") return false;
  return GENERATED_MARKERS.some((marker) => value.includes(marker));
}

export function shouldBlockProtectedTreeWorkAccess(toolName, input, cwd = process.cwd()) {
  if (toolName === "read") {
    const path = typeof input?.path === "string" ? input.path : "";
    return isProtectedTreeWorkPath(path, cwd);
  }

  if (toolName === "write" || toolName === "edit") {
    const path = typeof input?.path === "string" ? input.path : "";
    if (isProtectedTreeWorkPath(path, cwd)) return true;
    return containsGeneratedTreeWorkMarker(JSON.stringify(input ?? {}));
  }

  if (toolName !== "bash") return false;
  const command = typeof input?.command === "string" ? input.command : "";
  if (!command) return false;
  const unquoted = command.replace(/["']/g, "");
  const explicitProtectedPath =
    unquoted.includes(".TreeWork/state/") ||
    unquoted.includes(".TreeWork/events.jsonl");
  const splitProtectedPath =
    unquoted.includes(".TreeWork") &&
    (/(?:^|[\s/])st(?:ate|[?*\[].*?)(?:[\s/]|$)/m.test(unquoted) ||
      /(?:^|[\s/])event(?:s|[?*\[].*?)\.jsonl(?:[\s/]|$)/m.test(unquoted));
  const pathTokens = command
    .split(/[\s;&|<>]+/)
    .map((token) => token.replace(/^["'`()]+|["'`()]+$/g, ""))
    .filter((token) => token.includes("/"));
  const expandedProtectedPath = pathTokens.some((token) => {
    if (isProtectedTreeWorkPath(token, cwd)) return true;
    try {
      return globSync(token, { cwd }).some((match) => isProtectedTreeWorkPath(match, cwd));
    } catch {
      return false;
    }
  });
  return (
    explicitProtectedPath ||
    splitProtectedPath ||
    expandedProtectedPath ||
    containsGeneratedTreeWorkMarker(command)
  );
}

export function resetTreeWorkTools(activeTools) {
  return [...new Set([...activeTools.filter((name) => !TREEWORK_TOOLS.includes(name)), "treework_tools"])];
}

export function activateTreeWorkTools(activeTools, capability) {
  const requested =
    capability === "all" ? TREEWORK_TOOLS : TREEWORK_CAPABILITY_TOOLS[capability] ?? [];
  return [...new Set([...activeTools, ...requested])];
}

export function extractTreeWorkWorkspace(output) {
  if (typeof output !== "string") return undefined;
  const match = output.match(/^\s*workspace:\s*(.+?)\s*$/m);
  if (!match) return undefined;
  const workspace = match[1]?.trim();
  return workspace && isAbsolute(workspace) ? normalize(workspace) : undefined;
}
