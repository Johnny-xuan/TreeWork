# TreeWork Repository Guidance

This repository contains two independently installable TreeWork editions and
the source used to maintain them.

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

- `plugins/treework/` is the installable Coding Agents edition. Keep
  it free of project history, UI source, prototypes, and maintainer-only
  documents.
- `skills/treework-manual/` is the independently installable Manual edition.
  Keep its complete working contract in one self-contained `SKILL.md`.
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
