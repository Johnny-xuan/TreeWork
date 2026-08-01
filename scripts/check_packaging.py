#!/usr/bin/env python3
"""Local packaging checks for the TreeWork plugin."""

from __future__ import annotations

import json
import os
import stat
import struct
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any

from _paths import PLUGIN_ROOT, REPOSITORY_ROOT

MANIFEST_PATH = PLUGIN_ROOT / ".codex-plugin" / "plugin.json"


def fail(message: str) -> None:
    print(f"fail: {message}")
    raise SystemExit(1)


def ok(message: str) -> None:
    print(f"ok: {message}")


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        fail(f"missing {path.relative_to(PLUGIN_ROOT)}")
    except json.JSONDecodeError as exc:
        fail(f"{path.relative_to(PLUGIN_ROOT)} is not valid JSON: {exc}")


def require_path(raw: str, label: str, *, directory: bool = False) -> Path:
    if not raw.startswith("./"):
        fail(f"{label} must start with ./")
    path = (PLUGIN_ROOT / raw[2:]).resolve()
    try:
        path.relative_to(PLUGIN_ROOT)
    except ValueError:
        fail(f"{label} must stay inside plugin root")
    if directory and not path.is_dir():
        fail(f"{label} directory does not exist: {raw}")
    if not directory and not path.exists():
        fail(f"{label} path does not exist: {raw}")
    return path


def check_manifest() -> None:
    manifest = load_json(MANIFEST_PATH)
    if not isinstance(manifest, dict):
        fail(".codex-plugin/plugin.json must contain an object")
    for key in ["name", "version", "description", "author", "license", "interface"]:
        if key not in manifest:
            fail(f"plugin.json missing required field {key}")
    if manifest.get("name") != "treework":
        fail("plugin.json name must be treework")
    if "hooks" in manifest:
        fail("plugin.json should omit hooks when using default hooks/hooks.json")
    if "skills" in manifest:
        require_path(manifest["skills"], "skills", directory=True)
    interface = manifest.get("interface")
    if not isinstance(interface, dict):
        fail("plugin.json interface must be an object")
    prompts = interface.get("defaultPrompt")
    if not isinstance(prompts, list) or not prompts:
        fail("interface.defaultPrompt must be a non-empty array")
    if len(prompts) > 3:
        fail("interface.defaultPrompt should contain at most 3 prompts")
    for field in ["composerIcon", "logo"]:
        value = interface.get(field)
        if not isinstance(value, str):
            fail(f"interface.{field} must identify the TreeWork icon")
        require_path(value, f"interface.{field}")
    ok("plugin manifest shape")


def check_license() -> None:
    plugin_license = PLUGIN_ROOT / "LICENSE"
    repository_license = REPOSITORY_ROOT / "LICENSE"
    if not plugin_license.is_file():
        fail("installable plugin must include LICENSE")
    if plugin_license.read_bytes() != repository_license.read_bytes():
        fail("plugin LICENSE must match repository LICENSE")
    ok("MIT license included in plugin")


def check_hooks() -> None:
    hooks_path = PLUGIN_ROOT / "hooks" / "hooks.json"
    hooks = load_json(hooks_path)
    if not isinstance(hooks, dict) or not isinstance(hooks.get("hooks"), dict):
        fail("hooks/hooks.json must contain a hooks object")
    encoded = json.dumps(hooks)
    if "$PLUGIN_ROOT" not in encoded and "${PLUGIN_ROOT}" not in encoded:
        fail("hook commands should resolve through PLUGIN_ROOT")
    for script in sorted((PLUGIN_ROOT / "hooks").glob("*.sh")):
        mode = script.stat().st_mode
        if not (mode & stat.S_IXUSR):
            fail(f"hook script is not executable: {script.relative_to(PLUGIN_ROOT)}")
    ok("default hook bundle")


def check_mcp_manifest() -> None:
    mcp_path = PLUGIN_ROOT / ".mcp.json"
    mcp = load_json(mcp_path)
    servers = mcp.get("mcpServers") if isinstance(mcp, dict) else None
    if not isinstance(servers, dict):
        fail(".mcp.json must contain an mcpServers object")
    server = servers.get("treework")
    if not isinstance(server, dict):
        fail(".mcp.json missing mcpServers.treework")
    if server.get("command") != "bash":
        fail("treework MCP command must be bash")
    if server.get("args") != ["./scripts/start-mcp.sh"]:
        fail("treework MCP args must launch ./scripts/start-mcp.sh")
    if server.get("cwd") != ".":
        fail("treework MCP cwd must be .")
    start = PLUGIN_ROOT / "scripts" / "start-mcp.sh"
    server_py = PLUGIN_ROOT / "mcp" / "treework_mcp.py"
    if not start.is_file() or not os.access(start, os.X_OK):
        fail("scripts/start-mcp.sh must exist and be executable")
    if not server_py.is_file() or not os.access(server_py, os.X_OK):
        fail("mcp/treework_mcp.py must exist and be executable")
    ok("mcp manifest")


def check_state_schemas() -> None:
    schema_dir = PLUGIN_ROOT / "skills" / "treework" / "schemas"
    required = {
        "branch.schema.json",
        "event.schema.json",
        "graph.schema.json",
        "project.schema.json",
        "tree-document.schema.json",
        "tree-state.schema.json",
    }
    for file_name in sorted(required):
        schema = load_json(schema_dir / file_name)
        if not isinstance(schema, dict) or schema.get("$schema") is None:
            fail(f"{file_name} must contain a JSON Schema object")

    project_schema = load_json(schema_dir / "project.schema.json")
    project_required = project_schema.get("required", [])
    if "tree_hash" not in project_required or "project_index_hash" in project_required:
        fail("project.schema.json must require tree_hash, not project_index_hash")

    tree_document = load_json(schema_dir / "tree-document.schema.json")
    if tree_document.get("required") != ["version", "tree"]:
        fail("tree-document.schema.json must require version and tree")

    tree_state = load_json(schema_dir / "tree-state.schema.json")
    tree_state_required = set(tree_state.get("required", []))
    if not {"revision", "source_hash", "state_hash", "root", "nodes"} <= tree_state_required:
        fail("tree-state.schema.json is missing accepted Tree state fields")
    ok("declarative Tree schemas")


def check_treework_cli() -> None:
    tw = PLUGIN_ROOT / "skills" / "treework" / "scripts" / "tw"
    if not tw.exists():
        fail("missing skills/treework/scripts/tw")
    rustc_wrapper = PLUGIN_ROOT / "scripts" / "rustc-unquarantine.sh"
    if not rustc_wrapper.is_file() or not os.access(rustc_wrapper, os.X_OK):
        fail("scripts/rustc-unquarantine.sh must exist and be executable")
    manifest = load_json(PLUGIN_ROOT / ".codex-plugin" / "plugin.json")
    expected = f"tw {manifest['version']}"
    env = os.environ.copy()
    env["TREEWORK_PLUGIN_ROOT"] = str(PLUGIN_ROOT)
    with tempfile.TemporaryDirectory(prefix="treework-packaging-") as build_dir:
        env["TREEWORK_BUILD_DIR"] = build_dir
        for args in [["--version"], ["version"]]:
            result = subprocess.run(
                [str(tw), *args],
                cwd=PLUGIN_ROOT,
                env=env,
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                check=False,
            )
            if result.returncode != 0:
                fail(f"tw {' '.join(args)} failed: {result.stderr.strip()}")
            if result.stdout.strip() != expected:
                fail(
                    f"tw {' '.join(args)} reported {result.stdout.strip()!r}; "
                    f"expected {expected!r}"
                )
    ok(expected)


def check_assets() -> None:
    icon_png = PLUGIN_ROOT / "assets" / "treework-icon.png"
    icon_svg = PLUGIN_ROOT / "assets" / "treework-icon.svg"
    if not icon_png.is_file() or not icon_svg.is_file():
        fail("TreeWork brand icon must ship as PNG and SVG")
    png = icon_png.read_bytes()
    if not png.startswith(b"\x89PNG\r\n\x1a\n"):
        fail("assets/treework-icon.png is not a PNG image")
    width, height = struct.unpack(">II", png[16:24])
    if (width, height) != (1024, 1024):
        fail("assets/treework-icon.png must be a 1024px high-DPI export")
    svg = icon_svg.read_text(encoding="utf-8")
    if 'viewBox="24 24 464 464"' not in svg or "<title" not in svg:
        fail("assets/treework-icon.svg must retain its accessible tight crop")

    legacy_vendor = PLUGIN_ROOT / "assets" / "vendor"
    if legacy_vendor.exists() and any(legacy_vendor.iterdir()):
        fail("retired Sigma/Graphology vendor assets must not ship")

    panel_dir = PLUGIN_ROOT / "assets" / "graph-panel"
    for file_name in ["index.html", "app.js", "styles.css"]:
        if not (panel_dir / file_name).is_file():
            fail(f"missing graph panel asset assets/graph-panel/{file_name}")
    index_html = (panel_dir / "index.html").read_text(encoding="utf-8")
    app_js = (panel_dir / "app.js").read_text(encoding="utf-8")
    styles_css = (panel_dir / "styles.css").read_text(encoding="utf-8")

    source_dir = REPOSITORY_ROOT / "project-map-ui"
    if source_dir.is_dir():
        package = load_json(source_dir / "package.json")
        lock = load_json(source_dir / "package-lock.json")
        dependencies = (
            package.get("dependencies") if isinstance(package, dict) else None
        )
        expected_dependencies = {
            "@fontsource/fraunces": "5.3.0",
            "@fontsource/ibm-plex-mono": "5.3.0",
            "d3": "7.9.0",
            "lucide-react": "1.27.0",
            "react": "19.2.8",
            "react-dom": "19.2.8",
        }
        if dependencies != expected_dependencies:
            fail("project-map-ui production dependencies must remain exactly pinned")
        if not isinstance(lock, dict) or lock.get("lockfileVersion") is None:
            fail("project-map-ui must retain a package lock")

    licenses_dir = panel_dir / "vendor" / "licenses"
    notices = (licenses_dir / "THIRD_PARTY_NOTICES.md").read_text(encoding="utf-8")
    if "Fraunces" not in notices or "IBM Plex Mono" not in notices:
        fail("published third-party notices must attribute both bundled fonts")
    for license_name in [
        "Fraunces-OFL-1.1.txt",
        "IBM-Plex-Mono-OFL-1.1.txt",
    ]:
        license_path = licenses_dir / license_name
        if (
            not license_path.is_file()
            or "SIL OPEN FONT LICENSE Version 1.1"
            not in license_path.read_text(encoding="utf-8")
        ):
            fail(f"missing bundled font license {license_name}")

    expected_fonts = {
        "fraunces-latin-400-normal.woff2",
        "fraunces-latin-500-normal.woff2",
        "ibm-plex-mono-latin-300-normal.woff2",
        "ibm-plex-mono-latin-400-normal.woff2",
        "ibm-plex-mono-latin-500-normal.woff2",
    }
    font_dir = panel_dir / "vendor" / "fonts"
    if not expected_fonts <= {path.name for path in font_dir.glob("*.woff2")}:
        fail("graph panel is missing locally bundled production fonts")
    if "./vendor/fonts/" not in styles_css:
        fail("graph panel CSS must reference locally bundled fonts")

    limits_path = REPOSITORY_ROOT / "scripts" / "project_map_performance_limits.json"
    limits = load_json(limits_path)
    expected_metrics = {
        "initial_render",
        "map_to_dependency",
        "dependency_focus",
        "dependency_depth_expand",
        "dependency_to_map",
        "map_search",
        "map_collapse",
        "map_focus",
        "map_pan",
        "map_zoom",
        "map_to_replay",
        "replay_seek",
    }
    ceilings = limits.get("ceilings_ms")
    if (
        limits.get("renderer") != "strata-v3-svg"
        or not isinstance(ceilings, dict)
        or set(ceilings) != expected_metrics
        or any(not isinstance(value, (int, float)) or value <= 0 for value in ceilings.values())
    ):
        fail("published Project Map performance limits are incomplete or invalid")

    forbidden_write_surfaces = [
        "window.treeworkWriteApi",
        "/api/transaction",
        "pendingTray",
        "stageChange",
        "branch.move",
        "branch.rename",
        "note.add",
        "state.graph.notes",
    ]
    retired_frontend = [
        "/api/graph",
        "window.treeworkGraph",
        "graphology",
        "sigma",
        "mermaid",
    ]
    if any(
        needle in app_js or needle in index_html
        for needle in forbidden_write_surfaces + retired_frontend
    ):
        fail("graph panel must remain a read-only accepted-state projection")
    if (
        "./app.js" not in index_html
        or "./styles.css" not in index_html
        or 'id="root"' not in index_html
    ):
        fail("graph panel entrypoint does not wire the production React bundle")
    if (
        "/api/project-map" not in app_js
        or "/api/project-map/events" not in app_js
        or "EventSource" not in app_js
    ):
        fail("graph panel bundle must consume the real Project Map API and SSE")
    if (
        "https://fonts." in index_html
        or "https://fonts." in styles_css
        or "src=\"http" in index_html
        or "href=\"http" in index_html
        or "url(http" in styles_css
    ):
        fail("graph panel must not require runtime network assets")
    ok("offline Strata V3 graph panel assets")


def check_product_docs() -> None:
    readme = (REPOSITORY_ROOT / "README.md").read_text(encoding="utf-8")
    skill_root = PLUGIN_ROOT / "skills" / "treework"
    skill = (skill_root / "SKILL.md").read_text(encoding="utf-8")
    build_tree = (skill_root / "references" / "02-build-tree.md").read_text(
        encoding="utf-8"
    )
    work_tree = (skill_root / "references" / "03-work-tree.md").read_text(
        encoding="utf-8"
    )
    teamwork = (skill_root / "references" / "08-teamwork.md").read_text(
        encoding="utf-8"
    )
    transition = (
        skill_root / "references" / "04-branch-transition.md"
    ).read_text(encoding="utf-8")
    mcp = (PLUGIN_ROOT / "mcp" / "treework_mcp.py").read_text(encoding="utf-8")
    if "treework_graph" in readme or "treework_graph" in skill:
        fail("shipped docs still teach the removed treework_graph MCP tool")
    if "Project Map" not in readme:
        fail("public README must describe Project Map")
    if any("treework_project_map" not in source for source in [build_tree, mcp]):
        fail("Build Tree guidance and MCP must agree on treework_project_map")
    launch_contract = [
        "first successful",
        "successful Apply",
        "Codex in-app browser",
        "explicit user request",
    ]
    if any(term not in build_tree for term in launch_contract):
        fail("Build Tree guidance must define the bounded first-Tree Project Map handoff")
    if len(skill.split()) > 1200:
        fail("main Skill exceeds the 1200-word progressive-disclosure budget")
    traversal_contract = [
        "## Traversal Loop",
        "control workspace",
        "Do not teleport",
        "A CLI process cannot change the parent Agent's cwd",
        "return tools to the control workspace",
    ]
    if any(term not in skill for term in traversal_contract):
        fail("main Skill must teach the Tree traversal and cwd boundary")
    isolation_contract = [
        "Pause preserves the managed worktree and binding",
        "entering another branch from a bound",
        "`--keep-worktree`",
    ]
    if any(
        term not in f"{work_tree}\n{transition}" for term in isolation_contract
    ):
        fail("Work Tree guidance must define pause, switch, and completion isolation")
    teamwork_contract = [
        "## Handshake Before Implementation",
        "Ready for Lead Review",
        "one writer per worktree",
    ]
    if (
        "references/08-teamwork.md" not in skill
        or "08-teamwork.md" not in work_tree
        or any(term not in teamwork for term in teamwork_contract)
    ):
        fail("Agent workflow must route parallel work through the teamwork contract")
    ok("Agent workflow and Project Map guidance")


def check_agent_reference_boundary() -> None:
    references = (
        PLUGIN_ROOT / "skills" / "treework" / "references"
    )
    expected = {
        "00-overview.md",
        "01-alignment.md",
        "02-build-tree.md",
        "03-work-tree.md",
        "04-branch-transition.md",
        "05-spec.md",
        "06-verification.md",
        "07-reporting.md",
        "08-teamwork.md",
        "command-reference.md",
        "tree-yaml.md",
    }
    actual = {path.name for path in references.glob("*.md")}
    if actual != expected:
        fail(
            "Agent references must contain only workflow guidance: "
            f"missing={sorted(expected - actual)} extra={sorted(actual - expected)}"
        )
    combined = "\n".join(
        (references / name).read_text(encoding="utf-8") for name in sorted(actual)
    )
    developer_only_terms = [
        "Runtime Modules And Migration",
        "Watcher And SSE",
        "Replay Read Model Convergence",
        "Implementation Dependency Order",
    ]
    if any(term in combined for term in developer_only_terms):
        fail("Agent references contain developer-only implementation contracts")
    ok("Agent reference boundary")


def check_clean_distribution() -> None:
    tracked_result = subprocess.run(
        [
            "git",
            "ls-files",
            "--",
            str(PLUGIN_ROOT.relative_to(REPOSITORY_ROOT)),
        ],
        cwd=REPOSITORY_ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if tracked_result.returncode != 0:
        fail(f"could not inspect tracked plugin files: {tracked_result.stderr.strip()}")
    plugin_prefix = f"{PLUGIN_ROOT.relative_to(REPOSITORY_ROOT).as_posix()}/"
    tracked = {
        line.removeprefix(plugin_prefix)
        for line in tracked_result.stdout.splitlines()
        if line.startswith(plugin_prefix)
    }
    forbidden = [
        ".TreeWork",
        ".agents",
        "dist",
        "project-map-ui",
        "prototypes",
        "target",
    ]
    for name in forbidden:
        if any(path == name or path.startswith(f"{name}/") for path in tracked):
            fail(f"clean package contains forbidden development path {name}")
    if any(
        part == "node_modules" for path in tracked for part in Path(path).parts
    ):
        fail("clean package contains node_modules")
    if any(path.endswith(".map") for path in tracked):
        fail("clean package contains source maps")
    if any(
        path.endswith(".pyc") or "__pycache__" in Path(path).parts for path in tracked
    ):
        fail("clean package contains Python cache artifacts")
    ok("clean distribution contents")


def check_hook_runtime() -> None:
    script = REPOSITORY_ROOT / "scripts" / "check_hooks.py"
    if not script.is_file():
        fail("missing scripts/check_hooks.py")
    result = subprocess.run(
        [sys.executable, str(script)],
        cwd=REPOSITORY_ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(f"hook runtime check failed:\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}")
    ok("hook runtime behavior")


def check_mcp_runtime() -> None:
    script = REPOSITORY_ROOT / "scripts" / "check_mcp.py"
    if not script.is_file():
        fail("missing scripts/check_mcp.py")
    result = subprocess.run(
        [sys.executable, str(script)],
        cwd=REPOSITORY_ROOT,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(f"mcp runtime check failed:\nstdout:\n{result.stdout}\nstderr:\n{result.stderr}")
    ok("mcp runtime behavior")


def check_activation_attestation_tests() -> None:
    script = REPOSITORY_ROOT / "scripts" / "test_check_activation.py"
    if not script.is_file():
        fail("missing scripts/test_check_activation.py")
    result = subprocess.run(
        [sys.executable, str(script)],
        cwd=REPOSITORY_ROOT / "scripts",
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if result.returncode != 0:
        fail(
            "activation attestation tests failed:\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    ok("candidate-bound activation attestation tests")


def main() -> None:
    check_manifest()
    check_license()
    check_hooks()
    check_mcp_manifest()
    check_state_schemas()
    check_treework_cli()
    check_assets()
    check_product_docs()
    check_agent_reference_boundary()
    check_clean_distribution()
    check_hook_runtime()
    check_mcp_runtime()
    check_activation_attestation_tests()


if __name__ == "__main__":
    main()
