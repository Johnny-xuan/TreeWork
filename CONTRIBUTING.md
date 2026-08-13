# Contributing to TreeWork

TreeWork accepts focused fixes and improvements that strengthen the shared
project-state protocol without adding project-specific policy to the core.

TreeWork currently ships and is release-tested as a Codex plugin. Contributions
for other coding-agent hosts are welcome when they preserve the same document,
transaction, lifecycle, and verification semantics through a focused adapter.

## High-Impact Contribution Areas

- Project Map interaction design, navigation, accessibility, responsive
  behavior, and large-Tree performance.
- Host adapters for Claude Code, Cursor, Gemini CLI, OpenCode, and other coding
  agents, with installation documentation and host-specific tests.
- Controlled evaluations of state recovery, agent handoffs, development drift,
  quality, and operational overhead.
- Documentation, examples, translations, packaging, and platform support.

The Project Map is intentionally still evolving. Interaction improvements are
welcome, but the panel must remain a read-only projection of accepted TreeWork
state rather than a second source of truth.

## Before Opening a Change

1. Search existing issues and pull requests.
2. Reproduce the problem against the latest `main`.
3. Decide whether the change belongs to the Agent protocol, the runtime, the
   Project Map, or repository tooling.
4. Open an issue before introducing a new command, lifecycle state, persisted
   field, or public document type.

TreeWork intentionally keeps its Agent-facing surface small. A new CLI command
must represent a durable project-state transition or a necessary recovery
boundary; convenience alone is not enough.

## Development Setup

Required tools:

- Rust stable and Cargo
- Python 3.11 or newer
- Node.js 22 or newer
- Git and Bash

Install frontend dependencies:

```bash
cd project-map-ui
npm ci
cd ..
```

Install the validator dependency in an isolated Python environment:

```bash
python3 -m venv .venv
. .venv/bin/activate
python -m pip install -r requirements-dev.txt
```

Run the main checks:

```bash
make test
make validate
```

See [docs/development.md](docs/development.md) for the full matrix.

## Documentation Boundaries

- `plugins/treework/skills/.../references/` is for Agents using
  TreeWork.
- `docs/product/` defines user-facing behavior.
- `docs/architecture/` defines implementation contracts for maintainers.
- Historical design notes are intentionally excluded from the public
  repository; durable decisions belong in the product or architecture docs.

Do not solve a maintainer documentation need by expanding the installed Skill.

## Pull Requests

Keep each pull request focused. Include:

- the user-visible or maintainer problem;
- the chosen behavior and important trade-offs;
- tests or evidence that cover the change;
- documentation updates when a public contract changes.

Generated Project Map assets under
`plugins/treework/assets/graph-panel/` must match the
`project-map-ui/` source. Do not hand-edit the generated bundle.

By contributing, you agree that your contribution is licensed under the MIT
License.
