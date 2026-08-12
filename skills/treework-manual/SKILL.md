---
name: treework-manual
description: Use TreeWork Manual to keep long-running, multi-part work oriented through a Markdown project tree. Trigger for writing, research, note systems, planning, creative work, operations, or other evolving work where an Agent must preserve global direction, local progress, discoveries, and an exact restart point across interruptions or handoffs without relying on TreeWork CLI, Git worktrees, fixed development stages, or Project Map.
---

# TreeWork Manual

TreeWork Manual is a way of working, not a command system. Treat the work as a
tree of owned scopes instead of a flat task list or a conversation the Agent is
expected to remember. The root supplies the global map. Each branch owns one
coherent line of work. Move through that tree deliberately so another Agent, or
the same Agent after context loss, can recover the work from files rather than
reconstructing it from fragments.

```text
Start at the root
-> locate the right branch
-> work within that branch
-> leave a recoverable state
-> return to the root
-> choose the next branch
```

A branch is a work scope, not necessarily a Git branch. Depending on the work,
branches may be chapters, themes, research questions, note collections,
deliverables, phases, modules, clients, or any other useful decomposition.

## Core Mental Model

1. **Locate before acting.** Route each meaningful request to the root, the
   current branch, another existing branch, or a genuinely new branch before
   doing the work.
2. **Keep global and local state separate.** Keep the root concise and
   navigational. Keep details inside the branch that owns them.
3. **Move instead of jumping.** When attention must move elsewhere, first leave
   the current branch in a state that can be resumed without conversation
   history.
4. **Let reality correct the documents.** Plans express intent; Progress records
   what is actually true. When artifacts, user direction, and older documents
   disagree, investigate and synchronize them instead of preserving a stale
   story.
5. **Grow the Tree only as needed.** Sketch enough structure to orient the work,
   then refine it as understanding improves. Do not invent a large branch
   hierarchy merely to appear organized.

User requests may arrive in a jumpy order. Treat them as routing signals, not
permission to teleport between unrelated work. If the request belongs to
another branch, checkpoint the current branch, return to the root view, and
then enter the target branch.

## State Documents

Use this default layout:

```text
.TreeWork/
  PROJECT.md
  task_plan.md
  progress.md
  findings.md
  branches/
    <branch>/
      task_plan.md
      progress.md
      findings.md
    <parent>/
      task_plan.md
      progress.md
      findings.md
      <child>/
        task_plan.md
        progress.md
        findings.md
```

These four document roles are the minimum shared language of TreeWork:

- **`PROJECT.md` answers "What is this work and how is it organized?"** Keep the
  purpose, durable constraints, Tree map, and branch meanings here. Do not turn
  it into a status dashboard or detailed work log.
- **`task_plan.md` answers "What do we intend to do?"** Keep desired outcomes,
  ordered steps, dependencies, boundaries, and checklists here. A Plan is not
  evidence that the work happened.
- **`progress.md` answers "What is actually true now?"** Keep current reality,
  completed and open work, blockers, and the exact restart point here. Record
  meaningful state changes, not a diary of every action or command.
- **`findings.md` answers "What did we learn that should survive?"** Keep
  conclusions, decisions and reasons, useful evidence, source notes, changed
  assumptions, and risks here. Do not use Findings as another to-do list.

The root has all four documents. A branch normally has Plan, Progress, and
Findings; its identity and place in the whole live in root `PROJECT.md`. A
parent branch may also contain child branches when a local area needs its own
map.

Use existing artifacts in their natural locations. A writing branch should
point to its draft, a research branch to evidence, and a note branch to its
source material. Do not copy whole artifacts into `.TreeWork/` merely to make
the state directory look complete.

## Minimal Document Shape

Use these headings as a starting point, not as a rigid schema. Add or remove
sections when the domain genuinely needs a different shape.

Root `PROJECT.md`:

```md
# Project

## Purpose

## Durable Direction

## Tree
- `<branch>` - what this branch owns.
  - `<child>` - what this child owns.

## Global Constraints
```

Root or branch `task_plan.md`:

```md
# Plan

## Outcome

## Steps
- [ ] A meaningful work item.

## Dependencies And Boundaries
```

Root or branch `progress.md`. Keep the `Current branch` line at the root and
omit it inside a branch:

```md
# Progress

Current branch: `<branch path or root>`

## Current Reality

## Completed

## Open Or Blocked

## Resume From
```

Root or branch `findings.md`:

```md
# Findings

## Decisions And Reasons

## Learned

## Risks And Unknowns

## References
```

Prefer numbered lists for order and reasoning, checklists for work that can be
completed, and bullets for facts. Mark an item complete only when the relevant
outcome exists, not merely because the Agent touched it.

## Starting Or Adopting Work

1. Inspect the user's request, existing artifacts, and existing organization
   before creating TreeWork files.
2. Create `.TreeWork/` only when the work is long-running, has multiple scopes,
   is likely to be interrupted or handed off, or is already becoming hard to
   hold in one context.
3. Write root `PROJECT.md`, Plan, Progress, and Findings from what is currently
   known. Do not fabricate certainty to fill headings.
4. Create only the first useful branches. It is valid to work at the root when
   the work has one coherent scope.
5. Choose branch boundaries that future Agents can recognize without guessing.
   Prefer an existing branch whenever it can own the work coherently.

Create a branch when the scope has its own outcome, materials, continuation
state, or likely future follow-up. Split a branch when it has become too broad
to understand or resume locally. Do not create a branch for every small request,
temporary thought, or checklist item.

## Working Along The Tree

### Enter a branch

1. Read root `PROJECT.md` and root `progress.md` to recover global direction.
2. Read the target branch's Plan, Progress, and Findings.
3. Inspect only the linked artifacts and ancestor context needed for this
   branch.
4. Restate internally what this branch owns, what is already true, and what the
   next meaningful action is.

Do not load every branch into context. The Tree exists so the Agent can recover
the relevant path without rereading the whole project.

### Work inside a branch

- Keep effort inside the branch's declared scope.
- Update the Plan when intended work changes.
- Update Progress when reality materially changes or the restart point moves.
- Update Findings when a conclusion, decision, source, risk, or corrected
  assumption should survive the current context.
- Update root documents immediately when a local discovery changes global
  direction, another branch, or the Tree itself.

The Agent may decide local execution details while working. It must not silently
change the user's goal, durable constraints, branch ownership, or cross-branch
direction. Surface those changes and synchronize the relevant documents.

### Leave or switch branches

Before moving away:

1. Reconcile the branch Plan with what remains intended.
2. Record current reality and an exact restart point in branch Progress.
3. Preserve durable conclusions in branch Findings.
4. Update root `PROJECT.md` if the Tree or a branch meaning changed.
5. Update root Progress with the new current location; update root Plan or
   Findings only with information that matters globally.
6. Return mentally to the root, then route to the next branch.

Do not perform empty document maintenance after every minor action. Synchronize
when meaning, reality, direction, ownership, or the continuation point changes,
and always before leaving a branch or ending a long session.

### Finish or retire a branch

Treat a branch as finished when its intended outcome is actually present and
its remaining work is either resolved or deliberately moved elsewhere. Record
the result and where it lives, then update the root Tree. If work is abandoned
or superseded, preserve the reason and any reusable Findings instead of merely
deleting the branch.

TreeWork Manual does not require a fixed lifecycle vocabulary. Use plain states
appropriate to the domain when a status helps the root map, such as `active`,
`paused`, `done`, or `dropped`; do not create lifecycle ceremony that does not
improve recovery.

## Recovery And Handoff

To continue after interruption, compaction, or Agent handoff:

1. Read root `PROJECT.md`.
2. Read root `progress.md` and identify the current branch.
3. Follow the Tree path to that branch.
4. Read its Plan, Progress, and Findings.
5. Inspect the linked working artifacts to confirm the recorded reality.
6. Continue from `Resume From`, correcting the documents first if they have
   drifted.

This is state recovery, not historical retrieval. The goal is not to remember
every past action. The goal is to recover the accepted structure, current
reality, durable knowledge, and next viable movement.

## Boundaries

- Do not require CLI commands, hooks, Git branches, Git worktrees, transactions,
  schemas, fixed stages, verification machinery, or a visual Project Map.
- Do not force software-development concepts such as Requirements or Specs onto
  writing, notes, research, or creative work. Add domain documents only when
  they genuinely help that branch.
- Do not duplicate information across root and branch documents. Promote only
  information whose effect crosses branch boundaries.
- Do not let document upkeep become the work. Keep every file concise enough to
  recover direction quickly.
- If `.TreeWork/state/`, `.TreeWork/events.jsonl`, or `tree.yaml` indicates a
  full runtime-managed TreeWork project, do not edit machine-owned state or mix
  manual conventions into it. Use the full TreeWork workflow unless the user
  explicitly requests a migration.
