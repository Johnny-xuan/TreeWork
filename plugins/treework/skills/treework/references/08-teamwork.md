# Teamwork On The Tree

Teamwork is a parallel execution pattern inside Work Tree. It is not a fourth
stage, a persistent Team mode, or an execution runtime. The accepted Tree
organizes the work; the Lead maps subagents onto branches that can proceed in
parallel.

TreeWork does not launch or identify agents. The Lead may use built-in
subagents, Agent Caller, Claude Code, or another provider. The same dispatch and
handoff discipline applies to all of them.

## Lead And Worker Roles

The Lead owns:

- the accepted Tree and dependency interpretation;
- deciding which branches can run concurrently;
- user alignment and cross-branch product or architecture decisions;
- topology changes, integration order, final review, and completion.

A Worker owns one assigned Work Branch in one bound worktree. It reads that
branch's normal Spec, Plan, Progress, Findings, and Verification, then executes
the same branch loop used in solo work.

One Worker may not silently create or move branches, expand its scope, change a
shared contract, or claim another branch. Report those needs to the Lead.

## Choose Parallel Branches

Parallelize only when:

1. each branch already exists in the accepted Tree;
2. its real prerequisites are complete, verified, or explicitly accepted;
3. its Spec, Scope, Acceptance, and external contracts are clear enough to
   delegate;
4. the branches have no parent-child execution relationship;
5. they are unlikely to edit the same files or redefine the same interface.

Dependency independence is necessary but not sufficient. Two branches can have
no graph edge and still conflict through shared files, implicit contracts, data
formats, or generated assets. The Lead checks both topology and code ownership.

Keep Alignment and Build Tree Lead-owned. Subagents may investigate or review,
but one Lead integrates those results into Requirements, Specs, and the
accepted Tree.

## Dispatch

From the control workspace, the Lead prepares or reuses one managed worktree
for each selected branch. Start each subagent with its tool cwd set to that
branch's printed worktree path.

The dispatch prompt must identify:

- the branch ID and worktree path;
- the branch purpose and expected deliverable;
- Scope and Out Of Scope;
- the relevant Spec, Plan, and required upstream references;
- dependencies and frozen interfaces;
- Acceptance, verification expectations, and the required handoff.

Do not paste the whole project into every prompt. Point the Worker to durable
documents and include only the context needed to interpret them.

## Handshake Before Implementation

Dispatch has two phases. During the first phase, the Worker may inspect the
assigned branch and repository but must not edit implementation files.

The Worker replies with:

1. its understanding of the branch goal and deliverable;
2. what it will and will not change;
3. the dependencies and contracts it believes are authoritative;
4. its intended implementation and verification approach;
5. unresolved questions, risks, or likely overlap with other branches.

The Lead compares that response with the Tree, Spec, and Plan. Correct any
misunderstanding and explicitly confirm the assignment before implementation.
Silence is not confirmation.

## Work Inside The Branch

After confirmation, the Worker:

- operates only from the assigned bound worktree;
- follows the relevant Spec and Task Plan;
- decides local coding details without inventing product behavior, boundaries,
  architecture, or shared contracts;
- updates branch Progress, Findings, and Verification as reality changes;
- commits coherent branch-local changes before handoff.

Keep one writer per worktree. Branch-scoped commands resolve from the
worktree's private binding, while authoritative state mutations return to the
shared control root and serialize through its transaction lock. The global
`current_branch` is the Lead cursor, not a registry of active Workers.

If the task no longer fits the branch, stop implementation. Record the current
reality and tell the Lead whether the existing branch needs a Spec change, a
Tree update, or work from another branch. Do not create a new branch merely to
avoid asking.

## Handoff And Integration

A Worker stops at **Ready for Lead Review** rather than declaring final
completion. Its handoff reports:

- commits and meaningful files changed;
- which Acceptance items are satisfied;
- verification commands and results;
- Findings, contract effects, risks, and open issues;
- possible conflicts or required integration order.

The Lead reviews the diff and evidence against the branch Spec and Acceptance.
The Lead then integrates in dependency order, runs any cross-branch validation,
and performs the authoritative completion transition. If review fails, return
the work to the same branch with precise corrections; do not create a duplicate
branch for the retry.

If a Worker stops early, preserve useful work, update Progress and Exit Notes,
and pause the branch. The Lead may later resume or reassign that same branch.

## What TreeWork Does Not Store

Do not add Assignment files, Worker IDs, provider sessions, message queues, or
Team lifecycle state to TreeWork. Those belong to the host Agent system when
needed. TreeWork keeps only the Project Tree, dependency DAG, branch state,
documents, transactions, verification, and worktree bindings needed for any
Agent to continue the project.
