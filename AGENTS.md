# TreeWork Repository Guidance

This repository contains an installable Codex plugin, a focused Pi package
adapter, and the shared source used to maintain both.

## Read Before Editing

- Workflow behavior: read the relevant file under
  `plugins/treework/skills/treework/references/`.
- Build Tree runtime: read `docs/architecture/build-tree-runtime.md`.
- Transaction, event, checkpoint, or projection behavior: read
  `docs/architecture/transaction-projection.md`.
- Project Map product behavior: read `docs/product/project-map.md`.
- Development and release commands: read `docs/development.md` and
  `docs/releasing.md`.

## Repository Boundaries

- `plugins/treework/` is the installable Codex plugin and shared runtime. Keep
  it free of project history, UI source, prototypes, and maintainer-only
  documents.
- `adapters/pi/` is the focused Pi host surface. Reuse the shared Skill, CLI,
  transactions, and MCP server; do not fork their state or semantics.
- `skills/treework-manual/` is the standalone manual variant. Keep its working
  contract in one `SKILL.md` and do not make it depend on the CLI, hooks,
  schemas, Git worktrees, fixed coding stages, or Project Map.
- Agent references explain how to use TreeWork. Do not put Rust modules, API
  internals, migration plans, or frontend architecture there.
- `project-map-ui/` is source; `plugins/treework/assets/graph-panel/`
  is generated output.
- `.TreeWork/` is user-project state created by TreeWork and must never be
  bundled into the installable plugin.

## Change Discipline

- Preserve the three-stage workflow: Pre-Tree Alignment, Build Tree, Work Tree.
- Keep the Agent-facing command surface small.
- Do not add Team, Runner, assignment, or provider-session state.
- Project Map remains a read-only projection.
- Machine-owned accepted state changes only through transactions.
- Add or update tests for public behavior.
- Run `make test` and `make validate` before declaring a change complete.
