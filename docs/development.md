# Development

## Layout

The installable Codex plugin and shared TreeWork runtime are under
`plugins/treework/`. The focused Pi package is declared by the root
`package.json` and implemented under `adapters/pi/`; it reuses the shared Skill
and MCP server rather than copying them. Project Map TypeScript source,
repository tests, and maintainer documents remain outside the plugin.

## Rust Runtime

```bash
cargo test \
  --manifest-path plugins/treework/crates/treework-cli/Cargo.toml
python3 scripts/check_cli_regression.py
python3 scripts/check_project_map_read_model.py
```

The `tw` wrapper builds into `$TREEWORK_BUILD_DIR`, Codex plugin data storage, or
the plugin-local `target/treework` fallback.

On macOS, the wrapper removes `com.apple.quarantine` only from TreeWork's newly
compiled Rust outputs and final CLI binary. This allows a plugin downloaded from
the internet to complete its first-run Cargo build without disabling Gatekeeper
or changing unrelated files. Existing `RUSTC_WRAPPER` configuration is chained.

## Project Map

```bash
cd project-map-ui
npm ci
npm test
npm run typecheck
npm run build
cd ..
```

The build writes committed production assets to
`plugins/treework/assets/graph-panel/`.

The browser acceptance and performance harnesses use Playwright from the Codex
bundled runtime by default:

```bash
python3 scripts/check_project_map_browser.py
python3 scripts/check_project_map_installed.py
python3 scripts/measure_project_map_performance.py --mode verify \
  --output .artifacts/project-map-performance.json
python3 scripts/stress_project_map.py --branches 750 --relations 1500
```

## Host Surfaces

Codex plugin checks:

```bash
python3 scripts/check_hooks.py
python3 scripts/check_mcp.py
python3 scripts/check_packaging.py
python3 scripts/test_check_activation.py
```

Pi adapter checks require Node.js 22+. The Pi executable is optional locally;
set `TREEWORK_REQUIRE_PI=1` to make its absence fail as it does in CI.

```bash
node --test adapters/pi/tests/*.test.mjs
python3 scripts/check_pi_adapter.py
python3 scripts/check_pi_workspace_switch.py
```

The round-trip test initializes a temporary TreeWork repository and drives Pi
from a repository subdirectory in offline RPC mode. It exercises the deferred
Enter command, real managed-worktree switch, and Return command; verifies that
history and the parent-session chain survive; and proves a switch cancelled by
another extension recovers the entered branch to `paused` without leaving an
orphan Pi session.

Plugin and Skill schema validation requires PyYAML:

```bash
python3 /path/to/plugin-creator/scripts/validate_plugin.py \
  plugins/treework
python3 /path/to/skill-creator/scripts/quick_validate.py \
  plugins/treework/skills/treework
```

## Generated and Authoritative Files

- Edit `project-map-ui/`; do not edit the bundled `app.js` or `styles.css`.
- Edit Agent workflow guidance under the plugin Skill.
- Edit developer contracts under `docs/`.
- Do not manually edit `.TreeWork/state/` or managed progress blocks.
- Do not construct flat `.TreeWork/branches/<branch-id>/` paths in runtime or
  tests. Resolve branch artifacts from the accepted semantic Tree so nested
  branches and legacy layout migration share one contract.

Run `make test` for the normal suite and `make validate` for release-facing
structure and packaging checks.
