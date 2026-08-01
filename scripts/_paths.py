"""Shared repository paths for TreeWork development tooling."""

from pathlib import Path


REPOSITORY_ROOT = Path(__file__).resolve().parents[1]
PLUGIN_ROOT = REPOSITORY_ROOT / "plugins" / "treework"
DIST_ROOT = REPOSITORY_ROOT / "dist" / "treework"
