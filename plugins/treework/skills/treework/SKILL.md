---
name: treework
description: Use TreeWork, a tree-guided development plugin for complex or evolving software projects that need staged alignment, pre-coding technical Specs, declarative project-tree planning, branch-scoped Git worktrees, durable recovery, and protected completion. It teaches the Agent to move through development along the Tree instead of jumping between unrelated tasks.
---

# TreeWork

TreeWork is a tree-guided development plugin. It externalizes accepted
project organization, design, branch-local reality, verification, and
development trajectory so an Agent does not have to reconstruct them from
partial code inspection or retrieved history. The Project Tree is its
navigational model: it tells the Agent where work belongs, how branches relate,
where the Agent is, and where it can move next. TreeWork is not a command
catalogue or a fixed sequence of steps. Keep direction by making every
meaningful change happen inside a deliberate branch traversal.

```text
Pre-Tree Alignment -> Build Tree -> Work Tree
```

Alignment determines what should be built. Specs preserve the technical
development thinking before coding. Build Tree turns that thinking into a
navigable project structure. Work Tree repeatedly enters one branch, develops
it against its Spec and Plan, synchronizes reality, returns to the control
workspace, and then enters the next branch.

```text
沿 Tree 移动
按 Spec 开发
用 Plan 执行
让 Progress 反映现实
用 Findings 保存后来发现的结论
```

## How TreeWork Changes Reasoning

- **Locate before acting:** map each request to the current branch, another
  existing branch, or a deliberate Tree update before touching code.
- **Design before inventing:** decide product behavior, boundaries,
  architecture, and contracts in the relevant Spec; improvise only local coding
  details during implementation.
- **Recover state, not fragments:** code inspection shows implementation
  reality and generic memory may retrieve what once happened; the Tree and
  branch documents supply the accepted context around them: where the project
  is, why the branch exists, what remains open, and what evidence is required.
- **Transition instead of jumping:** close and record the current branch, return
  to the control workspace, and then enter the next branch.
- **Prove instead of assuming completion:** use Acceptance and Verification
  rather than the Agent's feeling that enough code has been written.

## Traversal Loop

Use this loop until the accepted Tree is finished:

```text
control workspace
  -> inspect the Tree and choose one branch
  -> enter that branch and move tools into its worktree
  -> read its Spec and Plan
  -> implement and verify only that branch
  -> update Plan / Progress / Findings / Verification
  -> commit relevant Git changes
  -> pause, abort, or complete the branch
  -> return tools to the control workspace
  -> choose the next branch
```

Do not teleport from one area of the codebase to another because a new request
arrived. First identify which existing branch owns it. Close the current
branch's transition, return to the control workspace, and enter the target
branch. If no branch can own the work coherently, revise Alignment or update
the Tree before implementation.

## Start Or Resume

For a new adoption, initialize TreeWork, then follow Alignment before designing
the first Tree. Initialized templates are empty working surfaces, not completed
requirements or design.

For an existing `.TreeWork/`, first read:

- `.TreeWork/PROJECT.md` for project orientation;
- `.TreeWork/tree.yaml` for accepted organization and dependencies;
- root `.TreeWork/progress.md` for current global reality.

Use those files to identify the current stage and branch. Then load only the
reference and branch documents needed for the next action. Recall a branch when
returning after interruption or when its local reality is unclear; do not make
Recall a ritual for every new branch.

Before coding, read the implementation branch's `spec.md` and `task_plan.md`,
plus only the root or parent Spec material needed for inherited direction. If
the branch needs product behavior, architecture, boundaries, or contracts that
its Spec does not define, pause and design them before implementation.

## Worktree Lifecycle

`tw enter` prepares or reuses a branch-bound Git worktree and prints its
workspace path. A CLI process cannot change the parent Agent's cwd. After Enter,
run every filesystem and terminal action for that branch from the printed
workspace path; merely running Enter from the control workspace is not enough.

- **Pause:** park unfinished work after updating branch documents and committing
  what should be durable. The managed worktree and binding stay in place for
  later reuse.
- **Switch:** never enter another branch from a bound branch worktree. Return
  the Agent's tool workdir to the control workspace, then enter the target
  branch. The previous paused worktree remains isolated.
- **Complete:** update documents, verify, and commit before completion. Prefer
  running Complete from the control workspace after leaving the branch
  worktree. A dirty managed worktree blocks cleanup. Success removes the clean
  worktree by default; `--keep-worktree` preserves it but removes TreeWork's
  binding.
- **Abort:** record why the branch will not continue. Abort is not permission to
  discard uncommitted files; preserve or clean remaining Git work deliberately.

## Stage Routing

- Read `references/00-overview.md` on first adoption or when document ownership
  is unclear.
- Read `references/01-alignment.md` when intent, requirements, language, or
  overall direction needs confirmation.
- Read `references/05-spec.md` when creating or materially revising a project or
  branch Spec.
- Read `references/02-build-tree.md` before the first Tree or any structural
  update. Read `references/tree-yaml.md` only when editing exact YAML fields.
- Read `references/03-work-tree.md` before implementation and
  `references/04-branch-transition.md` before leaving or switching branches.
- Read `references/08-teamwork.md` before delegating parallel branches to
  subagents.
- Read `references/06-verification.md` before recording evidence or completing.
- Read `references/07-reporting.md` before the final project report.
- Read `references/command-reference.md` only for exact runtime syntax.

## Durable Ownership

- The Tree is the project navigation index.
- Specs own pre-coding technical design.
- Task Plans own executable branch work.
- Progress owns current reality and meaningful events.
- Findings own conclusions that code cannot explain reliably.
- Verification owns acceptance evidence.
- Transaction commands mark stage, Tree, location, lifecycle, verification, and
  completion changes. They do not replace the documents above.
- Runtime-owned state, events, bindings, and projections are never edited by
  hand.

## Non-Negotiable Rules

1. Requirements and relevant Specs are reviewed before implementation relies on
   them.
2. Reuse an existing branch before creating another; update the whole Tree as
   one coherent transaction when structure changes.
3. Work in one branch and one bound worktree at a time per Agent.
4. Local coding details may be decided during implementation; product behavior,
   system boundaries, architecture, and module contracts may not be invented
   silently.
5. Before every branch movement, synchronize documents, record verification and
   open issues, and commit relevant Git changes.
6. Completion is protected by acceptance and verification, not by the Agent's
   feeling that the work is probably done.
