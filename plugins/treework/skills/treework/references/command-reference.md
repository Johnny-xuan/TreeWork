# TreeWork Agent Command Reference

Status: accepted Agent-facing surface. Declarative Build Tree is implemented.
Internal diagnostics remain callable by hooks, MCP, packaging, and test
harnesses but are hidden from Agent help.

Agents are not expected to memorize a command catalogue. Use a command only when
TreeWork must make an accepted state transition reliably.

## Initialize

```bash
tw init
```

Create the initial `.TreeWork/` document and state structure. Run once when
adopting TreeWork in a project.

## Alignment

```bash
tw align start
tw align end
```

`start` returns a project to Alignment when its intent, Requirements, or
technical direction needs review. `end` records that the user explicitly
approved the Requirements and Spec Alignment Review; it does not judge those
documents itself.

For a project without an accepted Tree, `end` moves to `build_tree` so the Lead
can run `tw tree start`. For a project with an accepted Tree, `end` returns to
`work_tree`; run `tw tree update` only when the revised direction changes
topology, dependencies, or Spec routing.

## Build Tree

```bash
tw tree start
tw tree update
tw tree apply
```

`start` opens the first Tree Editing Session. `update` opens a later revision.
The Lead edits declarative `.TreeWork/tree.yaml` and relevant Specs. `apply`
parses, validates, derives semantic changes, and commits the accepted tree
atomically. There is no preview or second confirmation command.

Apply itself checks syntax, schema, branch identity, parent and dependency
cycles, Spec paths, protected history, and stale base state. The Agent does not
run a separate `check` ritual.

## Enter And Recover

```bash
tw enter <branch> [--recall] [--dry-run] [--no-isolate]
tw recall [<branch>] [--brief] [--json]
```

`enter` changes the current branch and prepares or reuses its managed worktree.
It prints the workspace path but cannot change the parent Agent's cwd. After it
succeeds, run branch work from that printed path. From a bound branch worktree,
entering another branch is rejected; return tool execution to the control
workspace before switching.

`recall` reconstructs branch-local context when returning to work with history.
Its JSON projection includes branch documents, `allowed_actions`, blocked action
reason codes, and the committed Tree revision/event marker. Eligibility is a
read-time statement; lifecycle commands revalidate before publishing. Global
orientation comes from `PROJECT.md`, the accepted Tree, root Progress, and
Project Map rather than another status command.

## Branch Lifecycle

```bash
tw pause [--reason "why work is being parked"]
tw abort --reason "why this branch will not continue"
tw verify --cmd "command or manual check" --result <passed|partial|failed> --gap "remaining gap"
tw complete [--keep-worktree]
```

The fixed lifecycle is `pending`, `in_progress`, `paused`, `complete`, and
`aborted`. Entering a paused branch returns it to `in_progress`; there is no
separate resume command. These transactions own lifecycle state, verification,
events, managed progress fields, and Project Map node state.

Pause preserves the managed worktree and binding. Abort records terminal state
but never implies that uncommitted files may be discarded.

Completion validates a managed worktree before committing the lifecycle
transaction. Prefer invoking it from the control workspace after leaving the
branch worktree. Dirty worktrees block completion cleanup. Worktree or binding
cleanup runs only after the branch completion event is committed. Accepted
isolation state releases TreeWork management and records the cleanup intent. A
cleanup failure is reported as a warning while the command remains successful
and the branch remains complete; any remaining worktree is an unmanaged residue
for manual cleanup. `--keep-worktree` keeps the clean worktree and removes only
its TreeWork binding.

## Version

```bash
tw version
```

Use only when diagnosing installation or compatibility. It reports the exact
installed plugin build, including the Codex cachebuster suffix.

## Internal Infrastructure

The following are not Agent workflow commands:

- state consistency checks, which run inside relevant transactions and test
  harnesses;
- raw event inspection, which has no public `log` command;
- Replay reduction, which belongs to Project Map;
- Project Map server, refresh, projection, and migration entry points.

The event stream remains durable internal data even though raw logging is not a
public command.

## Boundaries

- TreeWork exposes no Team, worker, runner, or Assignment command family.
- Project Map is read-only and has no Agent-facing operation command.
- Branch-scoped defaults resolve from the worktree's private branch binding.
- An unbound or conflicting worktree fails before mutation.
- TreeWork records workflow state transitions, not code edits or command
  history.
