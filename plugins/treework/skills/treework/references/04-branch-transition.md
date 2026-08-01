# Branch Transition Protocol

TreeWork transactions record accepted movement: Tree Apply may create branches;
Enter selects one; Pause, Abort, and Complete change its lifecycle; Verify
records acceptance evidence. Switching branches is not a separate command. It
is the deliberate sequence of closing the current branch, returning the
Agent's tools to the control workspace, and entering the target branch.

The control workspace is the project-level navigation point. A managed branch
worktree is one branch's isolated worksite. Moving through the Tree means
deliberately moving the Agent's tool workdir between those two places; a CLI
subprocess cannot move it for the Agent.

In the control workspace, branch-scoped defaults resolve through the current
branch cursor. In a managed branch worktree, they resolve through that
worktree's private branch binding and never fall back to the control cursor.

From the control workspace, Enter moves the current branch cursor and prepares
or reuses branch isolation. It prints the resulting workspace path but cannot
change the parent Agent's cwd. Move subsequent tool calls to the printed path.
In a bound branch worktree, entering the same branch does not move the control
cursor and entering another branch is rejected. Entering a paused branch
returns it to `in_progress`; there is no separate resume transition.

Pause when the current branch is actually being parked before leaving unfinished
work. Update branch documents and make the relevant commit first. Pause changes
lifecycle state but deliberately keeps the managed worktree and binding, so a
later Enter can reuse the same isolation.

To switch branches:

1. Finish the current branch's document and Git handoff.
2. Pause, abort, or complete it as appropriate.
3. Set the Agent's tool workdir back to the control workspace.
4. Inspect the Tree and Enter the target branch.
5. Move tool calls into the target's printed worktree path.

Abort only when the branch will not continue. Record the reason so Project Map,
Recall, and history can distinguish deliberate retirement from unfinished work.
Abort does not authorize deletion of uncommitted files; preserve or clean
remaining Git work deliberately.

Recall at entry only when the Agent needs full historical context. New or
obvious branches usually do not need it. Recall later when returning to an older
branch, switching after an interruption, or checking branch-local reality
without changing the current branch. Runtime-specific dry-run or no-isolation
options belong in the command reference.

When leaving a branch, keep unfinished work in the documents that already own
it. Durable work belongs in `task_plan.md` Local Steps. Current reality, open
issues, and handoff context belong in `progress.md`. Agent-local todo lists stay
in the Agent runtime and are not persisted by TreeWork.

If implementation materially changed the intended design, update `spec.md`
before leaving so the next Agent does not inherit a false reference. Do not
touch Spec merely because a transition occurred.

Before switching away from a branch, the Agent must ensure:

1. Branch progress has current reality.
2. Verification status and coverage gap are recorded.
3. Open issues and Exit Notes are precise enough for recall to resume the work.
4. Material design changes are reflected in the relevant Spec.
5. Relevant Git changes are committed or deliberately preserved.
6. The correct lifecycle transition is used before returning to the control
   workspace.

When TreeWork manages worktree isolation, Complete refuses destructive cleanup
of a dirty worktree. Prefer leaving the branch worktree and running Complete
from the control workspace. A clean managed worktree is removed by default.
`--keep-worktree` preserves it but removes the TreeWork binding, so it becomes
an ordinary unmanaged Git worktree. Complete does not merge the Git branch;
integration remains a Lead decision.

Use transactions instead of hand-editing fixed state.
