# Work Tree

Work Tree means entering one branch worksite, doing local work, verifying it,
and synchronizing the branch before moving through the tree.

## Enter And Work

Enter one Work Branch from the control workspace before changing its owned
files. By default, Enter creates or reuses a managed Git worktree and prints the
branch workspace path. The CLI cannot change the parent Agent's cwd, so set the
workdir of every subsequent filesystem and terminal tool call to that printed
path. Confirm the tool is operating inside the branch worktree before editing.

The worktree binding is the isolation boundary. Branch-scoped commands run
inside it resolve to that branch and cannot silently fall back to the control
workspace cursor. Do not continue implementation in the control workspace
after Enter merely because the command itself succeeded.

Before coding, read the current implementation branch's `spec.md`. Read only the
root or parent Spec material needed to understand inherited direction, then use
`task_plan.md` as the executable work list. If an implementation branch has no
Spec, write its technical conception before coding rather than reconstructing
the design inside the coding loop.

Coding Agents may decide local implementation details. They should not silently
invent product behavior, system boundaries, core architecture, or module
contracts. If one of those is missing or changes materially, pause coding and
write the relevant design into Spec first. Update the Tree only when project
structure also changed.

Inside the branch, keep local facts local:

- technical development thinking in Spec;
- executable scope, acceptance, and durable local steps in Task Plan;
- current reality, meaningful progress events, open issues, and Exit Notes in
  Progress;
- conclusions learned during implementation, contract effects, risks, and
  unknowns in Findings;
- acceptance evidence in Verification.

Do not copy session command logs or transient todo chatter into branch memory.

## Recall

Recall when returning to a branch with history, after an interruption, or
whenever local reality is unclear. Recall is the single branch-context recovery
surface.

It derives branch status, isolation, parent/children, related edges, documents,
verification, and machine-computed action eligibility directly from one
committed state. Blocked actions include stable reason codes and human-readable
reasons. The projection also carries the Tree revision and publication marker;
commands revalidate before publishing in case state changes after Recall. Recall
does not persist a floor, workspace report, context file, or any other second
copy of branch truth.

Recall does not replace reading the branch Spec.

A new or obvious branch usually needs only Enter. Add Recall at entry only when
full recovery is genuinely useful. Exact runtime syntax is an implementation
binding, not part of the conceptual workflow.

## Verify And Leave

Record evidence according to `06-verification.md`. Before leaving unfinished
work, update `task_plan.md` Local Steps, `progress.md` Current Reality/Open
Issues, and Exit Notes. Before completing, run the validator boundary and
satisfy the protected completion gate.

Pause preserves the managed worktree and binding so later Enter can reuse the
same worksite. To switch, return the Agent's tool workdir to the control
workspace and Enter the next branch; entering another branch from a bound
worktree is rejected.

After verification, document synchronization, and the relevant Git commit,
prefer running Complete from the control workspace. Completion blocks cleanup
when the managed worktree is dirty. A successful Complete removes the clean
worktree by default; `--keep-worktree` keeps it as an ordinary Git worktree but
removes its TreeWork binding. Complete never merges the Git branch; integration
remains a Lead decision.

Read `04-branch-transition.md` for pause, abort, switch, and completion behavior.

## Parallel Branches

Parallel work needs no separate Team mode. The Lead identifies ready,
independent branches from the accepted Tree, gives each subagent one branch and
one Git worktree, and confirms the subagent's understanding before
implementation begins. Inside that boundary, the subagent follows the normal
solo Work Tree loop.

When a runtime provides worktree binding, the worktree is the machine boundary:
branch-scoped operations resolve to that worktree's branch. TreeWork does not
record Agent identity, assignments, handshakes, or provider sessions. Keep one
writer in a worktree at a time, and let the Lead review and integrate returned
work.

Read `08-teamwork.md` for dependency-based dispatch, the required handshake,
Worker boundaries, handoff, and Lead integration.
