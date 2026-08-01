#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLUGIN_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

export TREEWORK_PLUGIN_ROOT="$PLUGIN_ROOT"
exec python3 "$PLUGIN_ROOT/mcp/treework_mcp.py"
