<div align="center">
  <img src="plugins/treework/assets/treework-icon.png" width="128" alt="TreeWork logo">
  <p>
    <a href="README.md">English</a> |
    <a href="README.zh-CN.md">Chinese</a> |
    <a href="https://johnny-xuan.github.io/TreeWork/"><img src="web-paper/public/web-paper-icon.svg" width="17" alt=""> Web Paper</a> |
    <a href="https://github.com/Johnny-xuan/TreeWork/releases/download/v0.1.4/TreeWork-paper-draft-v0.1.4.pdf">PDF</a>
  </p>
</div>

# TreeWork

TreeWork is a **tree-guided working model for Agents**. It externalizes evolving
work as a persistent Tree, gives every line of effort a branch, and teaches an
Agent to move from the root into one branch and back instead of chasing requests
as a flat queue. Plans, progress, and findings keep each location recoverable
across interruptions, context resets, and handoffs.

This repository provides two independently installable editions. They share the
same root-to-branch mental model, but each defines its own state and operating
contract.

## TreeWork Editions

- **TreeWork for Coding Agents** is a runtime-backed development system. It adds
  Alignment, Specs, declarative Tree transactions, isolated Git worktrees,
  protected completion, Recall, and Project Map through the Codex plugin or Pi
  adapter.
- **[TreeWork Manual](skills/treework-manual/SKILL.md)** is a standalone,
  single-file Agent Skill for writing, research, notes, planning, creative work,
  operations, and other evolving work. The Agent maintains the Tree and its
  project state directly in Markdown.

Choose an edition from the user's needs and the actual work rather than treating
one as a fallback for the other. Do not combine both state models inside one
project without an explicit migration.

## Installation

### TreeWork Manual

Install or load [`skills/treework-manual`](skills/treework-manual) as a normal
Agent Skill. The directory is self-contained and consists of one `SKILL.md`;
it does not install the Coding Agent runtime.

### TreeWork For Coding Agents

TreeWork currently targets Codex and Pi on macOS and Linux. Native Windows
support has not been release-tested.

Runtime prerequisites: Git, Bash, Python 3, Rust, and Cargo. The Project Map
frontend is bundled; Node.js is needed only for frontend development.

#### Pi

Install the focused Pi package directly from this repository:

```bash
pi install git:github.com/Johnny-xuan/TreeWork
```

Restart Pi, run `/treework-adapter` to verify the runtime, then invoke
`/skill:treework` or ask Pi to use TreeWork. The adapter reuses the shipped
Skill and MCP server, loads read-only tools on demand, ports TreeWork's mutation
and stop-check guardrails, and provides explicit `/treework-enter` and
`/treework-return` commands that fork the conversation across cwd-bound Pi
sessions. See [TreeWork for Pi](adapters/pi/README.md) for
the complete install, use, verification, and rollback contract.

#### Codex guided install

Give the following prompt to a Codex Agent with terminal access:

```text
Install and prepare TreeWork for me from:
https://github.com/Johnny-xuan/TreeWork

Act as both the installer and my onboarding guide. Do not initialize TreeWork
inside the current project until I explicitly approve it.

1. Inspect my operating system, shell, Codex CLI, and the availability and
   versions of Git, Bash, Python 3, Rust, and Cargo. Node.js is not required for
   normal use.
2. If a prerequisite is missing, explain what is missing and ask before using
   a package manager, rustup, sudo, or changing my shell configuration. Make
   sure Cargo is available to non-interactive shells after any approved setup.
3. Inspect `codex plugin marketplace list --json` and
   `codex plugin list --available --json`. Reuse or upgrade an existing
   TreeWork marketplace instead of creating a duplicate.
4. If this is a first install, run:
   `codex plugin marketplace add https://github.com/Johnny-xuan/TreeWork`
   `codex plugin add treework@treework`
   Handle an existing installation deliberately; do not remove or overwrite it
   without first explaining why.
5. Verify with `codex plugin list --json` that `treework@treework` is installed.
   Report the installed version, marketplace source, and any unresolved
   environment issue rather than assuming success from command exit codes.
6. Tell me that I must start a new Codex task before the installed Skill,
   hooks, and MCP server are available. The first `tw` command may compile the
   Rust runtime and download uncached Cargo dependencies.
7. Give me a short practical introduction: Pre-Tree Alignment clarifies intent
   and produces Requirements and Specs; Build Tree creates the declarative
   project Tree; Work Tree moves through one isolated branch at a time. Explain
   that `.TreeWork/` is shared project state that normally belongs in version
   control, and that Project Map is a read-only view of accepted state.
8. Finish by asking whether I want to use TreeWork in an existing project or a
   new one. Leave project initialization to the new Codex task after I choose.
```

#### Codex manual install

```bash
codex plugin marketplace add https://github.com/Johnny-xuan/TreeWork
codex plugin add treework@treework
```

Start a new Codex task so the Skill, hooks, and MCP server load from the
installed plugin, then ask:

```text
Use TreeWork to align this project, design its Specs, build the Tree,
and work branch by branch.
```

The first `tw` invocation compiles the Rust runtime. Cargo may need network
access when dependencies are not already cached.

<p align="center">
  <img src="paper/assets/persistent_project_state_infographic_8k_ultra_clear.png" width="100%" alt="TreeWork persistent project state and deterministic recovery overview">
</p>

## Three-Stage Protocol

```mermaid
flowchart LR
    A["Pre-Tree Alignment<br/>Investigate · Confirm · Spec"]
    B["Build Tree<br/>Design branches · dependencies · Spec links"]

    subgraph W["Work Tree"]
        direction LR
        C["Choose branch"] --> D["Enter isolated worktree"]
        D --> E["Implement from Spec + Plan"]
        E --> F["Verify · sync docs · commit"]
        F --> G{"Transition"}
        G -->|"next branch"| C
    end

    H["Tree complete"]
    A --> B --> C
    G -->|"all accepted"| H
    G -.->|"Tree revision"| B
    B -.->|"intent unresolved"| A
```

### Pre-Tree Alignment

The Agent investigates the repository and relevant external evidence, clarifies
the user's intent, and produces reviewed Requirements and a project-level
technical Spec before implementation begins.

### Build Tree

The Lead Agent designs Specs and project structure together, then writes one
declarative `.TreeWork/tree.yaml` containing branch hierarchy, stable order,
concise purpose, Spec references, and real dependencies. TreeWork validates and
publishes the complete desired state as one atomic Tree transaction.

### Work Tree: Traverse, Isolate, Return

`tw enter` prepares or reuses a branch-bound Git worktree and returns its path.
The Agent moves its tools there, reads the branch Spec and Plan, implements and
verifies the branch, and synchronizes Progress and Findings. Before moving
elsewhere, it records verification and open issues, commits durable changes,
pauses, aborts, or completes the branch, and returns its tools to the control
workspace.

Independent branches may be delegated in parallel, but each subagent receives
one branch and one worktree, confirms its understanding with the Lead, and
returns evidence for Lead review rather than silently expanding scope.

## What TreeWork Maintains

Across that protocol, TreeWork gives each stage durable state:

- **Project Tree:** created during Build Tree as an ordered rooted tree. Each
  branch is a stable address for a project, phase, module, or other coherent
  unit of work; parent-child edges express scope ownership.
- **Dependency DAG:** also authored during Build Tree to express prerequisites
  and possible parallelism over the same branches, without confusing dependency
  with hierarchy.
- **Scoped documents:** Alignment establishes Requirements and project-level
  Specs; Build Tree links Specs to branches; Work Tree uses Plans and updates
  Progress, Findings, and Verification.
- **Branch lifecycle:** Work Tree records whether each branch is pending, in
  progress, paused, complete, or aborted, separately from verification state.
- **Semantic event trajectory:** accepted transitions across all three stages,
  such as Alignment acceptance, Tree application, branch entry, pause,
  verification, and completion. It is not a shell log, code diff, or record of
  private reasoning.
- **Deterministic projections:** Recall recovers one branch, Project Map shows
  the current project, and Replay reconstructs earlier accepted states.

The Project Tree and Dependency DAG form the navigable project topology.
Documents and runtime state give each location enough meaning to resume work.

## Project Map

TreeWork includes a local, read-only Project Map:

- **Map** shows project hierarchy and the current route.
- **Dependency** shows prerequisites and downstream work for one branch.
- **Replay** reconstructs accepted TreeWork transitions over time.

After the first Tree is accepted, the Agent uses its host adapter's Project Map
handoff. Codex opens the localhost URL in its in-app browser; Pi returns the URL
and opens the system browser only on explicit request. The panel projects
accepted state; it does not edit the project.

## Design Rationale

A fixed workflow prescribes the next step. TreeWork instead defines a shared
project state space and valid movement through it, while leaving local
implementation decisions to the Agent. We call this perspective
**trajectory engineering**: making the accepted evolution of long-running work
explicit, recoverable, and inspectable without prescribing every action.

The mismatch between partial code observation, retrieved history, and current
accepted state is the **observation-state reconstruction gap**. TreeWork reduces
how much of that state must be rebuilt inside model context.

<p align="center">
  <img src="paper/assets/two_panel_agent_workflow_comparison_4k_final.png" width="100%" alt="Partial repository inspection and retrieval memory compared with TreeWork state-native project memory">
</p>

The resulting mental shift is concise:

- locate work before acting;
- decide product behavior, boundaries, architecture, and contracts in Specs;
- recover project state instead of reconstructing context from fragments;
- transition between branches instead of jumping;
- demonstrate completion through Acceptance and Verification.

Read the formal model and evaluation design in the
[Web Paper](https://johnny-xuan.github.io/TreeWork/), or see the
[paper source and build instructions](paper/README.md).

## Package Contents

The installable Codex plugin lives at
[`plugins/treework`](plugins/treework) and includes:

- the staged project-state Skill and Agent-facing references;
- the Rust `tw` transaction runtime;
- branch transition and completion guard hooks;
- a local read-only MCP server for Recall and Project Map launch;
- bundled Project Map assets.

The Pi package manifest and focused extension live under
[`adapters/pi`](adapters/pi) and directly reuse that same Skill, runtime, and
MCP server. TreeWork stores project state under `.TreeWork/`; neither host
adapter creates a second source of truth.

The independently installable [`skills/treework-manual`](skills/treework-manual)
edition contains one self-contained Agent Skill.

## Repository Layout

```text
plugins/treework/              Installable Codex plugin and shared runtime
adapters/pi/                  Focused Pi extension, tests, and host docs
skills/treework-manual/       Standalone manual TreeWork Skill
project-map-ui/               React/D3/SVG Project Map source
docs/product/                 Product behavior and UX contracts
docs/architecture/            Runtime and transaction contracts
scripts/                      Development, validation, and release tooling
paper/                        Research paper source and assets
```

The plugin's `references/` directory contains only instructions an Agent needs
while using TreeWork. Maintainer implementation contracts stay under `docs/`.

## Community and Help Wanted

TreeWork currently ships and is release-tested for Codex and Pi. Support for
Claude Code, Cursor, Gemini CLI, OpenCode, and other agent hosts is welcome
through focused host adapters.

High-impact contribution areas include:

- improving Project Map interaction design, navigation, accessibility,
  responsive behavior, and large-Tree performance;
- adding and testing host adapters while preserving document, transaction,
  lifecycle, and verification semantics;
- building controlled evaluations for state recovery, handoff cost,
  development drift, quality, and operational overhead;
- improving documentation, examples, translations, packaging, and platform
  support.

TreeWork intentionally keeps its Agent-facing surface small. New commands,
persisted fields, lifecycle states, or document types need a durable
project-state reason, not only convenience. Read
[Contributing](CONTRIBUTING.md) and open an issue before changing a public
contract.

## Development

The local setup, test matrix, and release process live in:

- [Development guide](docs/development.md)
- [Release guide](docs/releasing.md)
- [Documentation map](docs/README.md)
- [Release notes](RELEASE-NOTES.md)

```bash
make test
make validate
```

## Status

`v0.1.7` is the current runtime version. Alignment, declarative Tree
construction, hierarchy-aligned branch documents, protected branch traversal,
Recall, Project Map, and Replay form a usable end-to-end loop. Codex and Pi host
surfaces share those semantics. Project Map interaction design will continue to
evolve.

## Privacy

TreeWork runs against local project files and serves Project Map on loopback.
It has no telemetry. The local service rejects non-loopback browser hosts and
origins.

## License

TreeWork is available under the [MIT License](LICENSE).
