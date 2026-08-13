# Release Notes

## v0.1.8 - Two Independent Editions

- Adds `treework-manual`, an independently installable single-file TreeWork
  edition for writing, research, notes, planning, creative work, operations,
  and other evolving projects. Agents maintain its Tree and project state
  directly in Markdown.
- Publishes TreeWork for Coding Agents and TreeWork Manual as two independently
  installable assets from the same release tag.
- Withdraws the experimental Pi host adapter. The Codex plugin and persisted
  TreeWork project-state format are unchanged.

## v0.1.7 - Hierarchical Branch Artifacts

- Projects branch documents onto the same parent-child hierarchy as the
  accepted Tree, keeping each branch's Spec, Plan, Progress, Findings,
  Verification, and auxiliary evidence together.
- Migrates the legacy flat layout on the first protected mutation while keeping
  read-only Recall and Project Map compatible before migration.
- Moves parent branches and all descendants atomically during Tree Apply, with
  journaled rollback and collision, cycle, escape, and symlink validation.
- Routes CLI lifecycle operations, managed worktrees, Recall, Project Map, MCP,
  and release fixtures through one shared branch-artifact resolver.

## v0.1.6 - Atomic macOS Upgrade Publication

- Publishes the rebuilt CLI through a fresh sibling file and atomic rename
  instead of overwriting an existing plugin-data executable in place.
- Prevents macOS Gatekeeper from retaining a stale execution decision across a
  Codex Marketplace plugin upgrade.
- Adds regression coverage proving an existing CLI destination receives a new
  filesystem identity and leaves no temporary publication file behind.

## v0.1.5 - macOS Quarantine-Safe Bootstrap

- Prevents macOS Gatekeeper from terminating Cargo-generated executable build
  helpers when TreeWork is installed from a quarantined download.
- Removes quarantine only from TreeWork-owned executable Rust outputs and the
  final CLI binary; it does not disable Gatekeeper or alter unrelated files.
- Preserves existing `RUSTC_WRAPPER` configuration and leaves non-macOS
  bootstrap behavior unchanged.
- Adds deterministic Darwin/Linux bootstrap coverage, package validation for
  the shipped helper, and a real quarantined-copy macOS verification path.

## v0.1.4 - Release Verification

- Makes the declarative Tree regression fixture independent of Markdown
  placeholder trailing whitespace, so the public Linux CI validates the same
  normalized templates shipped by the plugin.
- Keeps the TreeWork `v0.1.3` product behavior unchanged while publishing a
  fully green release candidate.

## v0.1.3 - Public Repository Release

- Publishes TreeWork as a Codex plugin for state-native project memory while
  keeping the Agent-facing Skill concise and operational.
- Teaches five durable reasoning shifts: locate before acting, design before
  inventing, recover state instead of fragments, transition instead of jumping,
  and prove completion.
- Documents the complete managed-worktree lifecycle across Enter, Pause,
  switching, Complete, and Abort.
- Exposes revision-scoped lifecycle eligibility through Recall, with stable
  blocker reasons and command-time revalidation against committed state.
- Adds dependency-aware teamwork guidance: the Lead delegates independent
  branches, confirms scope through a handshake, and reviews returned evidence
  without adding a separate Team or Runner state system.
- Ships the accepted TreeWork plugin icon as PNG and SVG and validates both
  plugin manifest references during packaging.
- Includes the TreeWork system-paper draft, formal continuation guarantees,
  evaluation protocol, and visual explanation of state-native project memory.
- Includes the public contribution, security, CI, development, release, and
  repository documentation required for community maintenance.

## v0.1.2 - Traversal And Worktree Guidance

- Reframes the main Skill around the repeatable Tree traversal loop instead of
  product explanation.
- Makes the `tw enter` boundary explicit: the CLI prepares isolation and prints
  a path, while the Agent must move subsequent tool calls into that worktree.
- Defines how Pause, branch switching, Complete, and `--keep-worktree` change or
  preserve branch isolation.
- Keeps detailed syntax and stage rules progressively disclosed in references.

## v0.1.1 - End-to-End Runtime Baseline

TreeWork now provides a complete local project-state protocol for keeping
coding agents oriented through long-running software projects.

### Workflow

- Pre-Tree Alignment produces reviewed Requirements and technical Specs.
- Declarative Build Tree uses one `tree.yaml` editing pass and atomic Apply.
- Work Tree provides branch-local Plans, Progress, Findings, Verification,
  Recall, isolation, and protected lifecycle transitions.
- Parallel agents use ordinary independent branches and Git worktrees without a
  separate Team or Runner state system.

### Project Map

- Map, Dependency, and Replay views project accepted TreeWork state.
- After the first Tree is accepted, the Agent opens Project Map in the Codex
  in-app browser.
- Branch navigation includes bounded Back and Forward history.
- The panel remains read-only; temporary annotations stay in browser state.

### Runtime

- Rust transaction core with typed YAML parsing, semantic tree diffs, journals,
  checkpoints, validation, and replayable workflow events.
- Local MCP tools for branch Recall and Project Map launch.
- Hooks guard machine-owned state and report Stop-boundary consistency issues.
- Exact plugin build versions are visible through `tw version`.

### Repository

- The installable plugin is isolated under `plugins/treework/`.
- Agent-facing references are separated from maintainer architecture docs.
- Public contribution, security, CI, development, and release documentation is
  included.

## v0.1.0 - Internal Baseline

The frozen internal baseline before public repository preparation.
