# Documentation

TreeWork documentation is separated by reader and responsibility.

## Users and Coding Agents

The installed Skill owns workflow guidance:

- `00-overview.md`
- `01-alignment.md`
- `02-build-tree.md`
- `03-work-tree.md`
- `04-branch-transition.md`
- `05-spec.md`
- `06-verification.md`
- `07-reporting.md`
- `08-teamwork.md`
- `tree-yaml.md`
- `command-reference.md`

These files live under
`plugins/treework/skills/treework/references/`.

## Host Adapters

- [TreeWork for Pi](../adapters/pi/README.md) defines Pi installation, lazy
  tools, lifecycle guardrails, cwd-bound conversation handoff, verification,
  and rollback.

## Product Maintainers

- [Project Map](product/project-map.md) defines Map, Dependency, Replay,
  Inspector, notes, refresh, and read-only behavior.

## Runtime Maintainers

- [Build Tree runtime](architecture/build-tree-runtime.md) defines parsing,
  editing sessions, Apply, validation, migration, and rollback.
- [Transaction and projection](architecture/transaction-projection.md) defines
  publication, events, checkpoints, coherent reads, watchers, and Replay
  reconstruction.
- [Hierarchical branch artifacts](architecture/hierarchical-branch-artifacts.md)
  defines the Tree-derived document layout, legacy migration, subtree moves,
  and rollback contract.

## Contributors

- [Development](development.md)
- [Releasing](releasing.md)
- [Historical design notes](archive/README.md)

Documents under `archive/` preserve design history. They do not override the
current Skill, product contracts, runtime contracts, or tests.
