# Development

## Layout

The installable plugin is `plugins/treework/`. Runtime source and
bundled assets stay inside that directory. Project Map TypeScript source,
repository tests, and maintainer documents remain outside it.

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

## Plugin Surfaces

```bash
python3 scripts/check_hooks.py
python3 scripts/check_mcp.py
python3 scripts/check_packaging.py
python3 scripts/test_check_activation.py
```

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

Run `make test` for the normal suite and `make validate` for release-facing
structure and packaging checks.
