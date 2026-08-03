"""Shared repository paths and artifact-layout helpers for TreeWork tooling."""

import json

from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
PLUGIN_ROOT = REPOSITORY_ROOT / "plugins" / "treework"
DIST_ROOT = REPOSITORY_ROOT / "dist" / "treework"


def _encode_branch_segment(branch_id: str) -> str:
    safe = b"abcdefghijklmnopqrstuvwxyz0123456789-_"
    return "".join(
        chr(byte) if byte in safe else f"%{byte:02X}"
        for byte in branch_id.encode("utf-8")
    )


def branch_artifact_dir(workspace: Path, branch_id: str) -> Path:
    """Resolve one branch directory from the project's committed layout."""
    treework = workspace / ".TreeWork"
    if branch_id == "root":
        return treework
    project = json.loads((treework / "state" / "project.json").read_text(encoding="utf-8"))
    version = project.get("artifact_layout_version", 1)
    if version == 1:
        return treework / "branches" / branch_id
    if version != 2:
        raise ValueError(f"unsupported branch artifact layout version {version}")

    state = json.loads((treework / "state" / "branches.json").read_text(encoding="utf-8"))
    parents = {item["path"]: item.get("parent", "") for item in state["branches"]}
    chain: list[str] = []
    cursor = branch_id
    seen: set[str] = set()
    while cursor != "root":
        if cursor in seen:
            raise ValueError(f"branch parent cycle while resolving {branch_id}")
        seen.add(cursor)
        if cursor not in parents:
            raise ValueError(f"unknown branch {cursor}")
        chain.append(_encode_branch_segment(cursor))
        cursor = parents[cursor]
    return treework / "branches" / Path(*reversed(chain))
