# Transaction Publication And Projection Contract

Status: accepted shared implementation contract for
`transaction-replay-events` and `project-map-read-model`. Read this before
changing TreeWork events, accepted-state publication, Project Map read models,
watchers, checkpoints, or Replay reduction.

## Purpose

TreeWork has two different ways to read one accepted workflow:

- Project Map and Dependency read the latest accepted state.
- Replay reads the accepted transaction trajectory.

Events do not build or drive the live Tree. A successful transaction publishes
accepted state and one semantic event as two views of the same commit. The
current-state read model must never reconstruct live state from events, and
Replay must never infer transactions from file modification times.

## Shared Terms

- **Accepted state**: `state/project.json`, `state/tree.json`,
  `state/branches.json`, and `state/graph.json` after a successful transaction.
- **Transaction**: one accepted TreeWork workflow mutation. One successful
  transaction owns one monotonically increasing event `seq`.
- **Publication marker**: `state/project.json`. Its `last_event_seq`,
  `tree_revision`, and `tree_hash` identify the committed accepted-state tuple.
- **Checkpoint**: an immutable Replay snapshot written at an accepted Tree
  revision. It contains TreeWork topology and visible workflow state, not source
  code or branch-document prose.
- **Projection**: disposable data derived from accepted state, events, and
  documents. A projection is never authoritative.

## Commit And Read Discipline

Every mutating CLI command already runs under the project-wide
`.TreeWork.lock/`. The transaction implementation must preserve that lock and
publish in this order:

1. Validate all inputs and prepare rollback data.
2. Write accepted state other than `state/project.json`, plus any transaction
   documents and checkpoint required by the event.
3. Append exactly one complete event record to `events.jsonl`.
4. Derive the complete intended result from transaction-owned roots and flush
   every intended non-marker file and parent directory.
5. Write and flush `state/project.json` last as the publication marker.
6. Verify the intended publication, remove recovery artifacts, then release the
   lock.

If any step before publication fails, rollback restores accepted state,
documents, checkpoint ownership, and the exact previous event-log bytes. A
failed transaction does not consume an event sequence.

Worktree creation for `enter` may happen before state publication because it is
an external Git side effect. Failure before accepted-state commit must not
publish a TreeWork event, and cleanup of a newly created external worktree
after a later state failure is best effort with an actionable error.

Completion validates its managed worktree before publication but releases or
removes that external worktree only after the completion transaction commits.
Cleanup failure cannot roll back an accepted branch completion; it returns a
warning and leaves explicitly unmanaged residue for manual cleanup.

The current-state reader uses the following coherent-read loop:

1. Do not begin while `.TreeWork.lock/` exists.
2. Read `state/project.json` as marker A.
3. Read the accepted Tree, branch state, graph state, and event tail required
   for the projection.
4. Read `state/project.json` again as marker B and confirm the lock is absent.
5. Accept only when markers A and B agree and:
   - `project.tree_revision == tree.revision`;
   - `project.tree_hash == tree.source_hash`;
   - the accepted Tree state hash is valid;
   - the final complete event sequence equals `project.last_event_seq`.

Revision zero may have no accepted `state/tree.json`; the read model returns the
initialized root state with an empty accepted topology revision. A mismatch,
partial JSONL line, active/stale lock, or malformed file never produces a mixed
projection. The server retains its last valid projection and reports degraded
status until a coherent read succeeds.

Direct branch-document edits are narrative changes rather than TreeWork state
transactions. The watcher debounces them and requires two stable reads before
publishing a new narrative revision.

## Event Envelope

New events use a typed, versioned envelope:

```json
{
  "schema_version": 1,
  "seq": 42,
  "time": "unix:1785211200",
  "type": "branch.paused",
  "subject": "project-map-read-model",
  "message": "Waiting for the shared transaction contract",
  "tree_revision": 7,
  "data": {}
}
```

`seq`, `time`, `type`, `subject`, `message`, and `tree_revision` remain common
fields. Event-specific replay data lives under `data`. Readers must accept
legacy records that lack `schema_version` or `data`, but writers only emit the
current format. New writers retain the existing `unix:<seconds>` time encoding
for compatibility; formatting it for people belongs to the projection/UI and
this branch does not migrate every historical `last_sync` field.

The minimum typed families are:

- `project.initialized`: resulting stage and current branch.
- `alignment.started` and `alignment.accepted`: previous/resulting stage.
- `tree.editing_started` and `tree.editing_updated`: previous/resulting stage
  and editing-session summary.
- `tree.applied`: resulting revision, grouped semantic operations, affected
  subjects, accepted hashes, and `snapshot_ref`.
- `branch.entered`: previous/resulting current branch, previous/resulting
  branch status, and isolation outcome needed for visible state.
- `branch.paused`, `branch.completed`, and `branch.aborted`:
  previous/resulting status and reason.
- `verification.recorded`: previous/resulting verification state plus concise
  verification evidence.

Generated-view synchronization is not a workflow transaction and does not
produce a Replay event. Raw command execution, file edits, code diffs, and
Agent reasoning are never events.

The event-family payload shapes are fixed at this semantic level:

```text
project.initialized
  stage.before / stage.after
  current_branch.before / current_branch.after
  snapshot_ref

alignment.started | alignment.accepted
  stage.before / stage.after

tree.editing_started | tree.editing_updated
  stage.before / stage.after
  editing.mode / base_tree_revision / base_event_seq / base_state_hash

tree.applied
  base.event_seq / tree_revision / state_hash
  result.tree_revision / tree_document_hash / accepted_tree_state_hash
  result.topology_changed
  operations[] / affected_subjects[] / snapshot_ref

branch.entered
  current_branch.before / current_branch.after
  status.before / status.after
  reason.before / reason.after
  isolation.mode / workspace_path / git_branch / managed_by_treework / action

branch.paused | branch.completed | branch.aborted
  status.before / status.after
  reason.before / reason.after
  branch.completed also includes verification.status

verification.recorded
  verification.before / verification.after
  evidence.command / result / gap
```

The Rust representation may use tagged enums or typed structs, but serialized
keys and meanings must remain stable. Legacy records are parsed into a separate
legacy/unsupported representation with an explicit replayability result; they
are never silently upgraded by inventing missing before-state.

State-preserving command repetitions do not create artificial trajectory:

- a second `init` is a no-op with no event;
- same-state lifecycle operations are rejected or no-op without an event;
- entering the already-current `in_progress` branch emits no event when visible
  isolation state is unchanged;
- recording byte-for-byte equivalent verification emits no event;
- a successfully accepted no-change `tree apply` remains a real transaction and
  does emit an event and checkpoint.

Public `tw align end` emits `alignment.accepted` after explicit user approval of
the Alignment Review. Its resulting stage is `build_tree` before the first
accepted Tree and `work_tree` when returning to an existing accepted Tree.
The event name remains stable so existing Replay history does not require a
synthetic migration.

## Checkpoints

Checkpoints live under:

```text
.TreeWork/history/checkpoints/
  tree-r000007-e000042.json
```

Every successful `tree apply` writes one checkpoint before its event and stores
the relative path in `tree.applied.data.snapshot_ref`. Fresh `init` writes
`tree-r000000-e000001.json` and references it from `project.initialized`, so new
projects have a Replay genesis. Existing projects without one remain readable:
the first new Apply creates the first guaranteed seek point, and older legacy
history is reported as non-reconstructable rather than guessed.

A checkpoint contains:

- schema version, event sequence, capture time, and Tree revision;
- project stage, current branch, editing state, and accepted hashes;
- the accepted Tree topology, order, Spec references, and dependencies;
- branch lifecycle, verification, reason, and other Project Map-visible state.

It excludes source files, Git diffs, full branch documents, worktree contents,
and browser notes. Checkpoints are immutable and content-validated when loaded.
The serialized shape is:

```text
schema_version
event_seq
captured_at
tree_revision
checkpoint_hash
project { stage, current_branch, tree_editing, tree_hash }
tree: accepted Tree state or null at revision zero
branches[] {
  id, parent, title, purpose, status, verification_status,
  status_reason, isolation
}
```

`checkpoint_hash` uses the runtime's accepted stable-hash algorithm over
canonical serialized checkpoint content with `checkpoint_hash` omitted.
The event records both `snapshot_ref` and `checkpoint_hash`.

If recovery finds a journal after `project.json` was written, it does not
blindly rollback. The journal stores the intended final publication marker:

- when the marker, event tail, checkpoint, and accepted files match the intended
  commit, recovery finishes forward by removing journal artifacts;
- otherwise recovery restores the exact before-state.

This makes `project.json` a real commit marker across process crashes rather than
only within one process.

## Current-State Read Model

Map and Dependency are built only from the coherent accepted-state tuple:

- `state/tree.json` owns hierarchy, stable sibling order, purpose, Spec
  reference, and `depends_on`;
- `state/branches.json` owns lifecycle, verification, status reason, and
  isolation summary;
- `state/project.json` owns stage, current branch, editing state, transaction
  sequence, and Tree revision;
- branch documents supply Inspector prose after safe branch-ID resolution.

The full projection contains:

```json
{
  "schema_version": 1,
  "tree_revision": 7,
  "state_event_seq": 42,
  "narrative_revision": "sha256:...",
  "tree_editing": false,
  "projected_at": "unix:1785211200",
  "health": { "status": "ok", "message": "" },
  "project": {
    "stage": "work_tree",
    "current_branch": "project-map-read-model",
    "topology_source": "accepted"
  },
  "nodes": [
    {
      "id": "project-map-read-model",
      "parent": "root",
      "order": 3,
      "title": "Project Map Read Model",
      "purpose": "Expose coherent accepted state.",
      "spec": "branches/project-map-read-model/spec.md",
      "status": "in_progress",
      "verification": "unverified",
      "status_reason": "",
      "is_current": true,
      "readiness": "active",
      "depends_on": ["build-tree-declarative-runtime"],
      "child_count": 0
    }
  ],
  "dependencies": [
    {
      "from": "project-map-read-model",
      "to": "build-tree-declarative-runtime",
      "satisfied": true
    }
  ]
}
```

Node order is the accepted sibling order, not a global sort. `readiness` is one
of `active`, `ready`, `waiting`, `complete`, `paused`, or `aborted`; it is
derived for presentation and never persisted as lifecycle state.

Branch detail returns the same metadata envelope plus an exact `branch` node and
section-oriented document summaries:

```text
task_plan { scope, acceptance, local_steps, out_of_scope, dependencies,
            branch_intake_gate }
progress  { current_reality, recent_work, open_issues, exit_notes }
findings  { decisions, interface_or_contract_effects, risks_and_unknowns }
verification { status, evidence, coverage_gap }
```

Each document field is Markdown text from the matching heading body. Missing
optional sections become empty strings/lists with a warning in branch-detail
health; headings are not guessed from unrelated prose.

Branch detail is addressed by an exact accepted branch ID, never by an
arbitrary filesystem path. IDs containing traversal components, absolute
prefixes, or symlink escapes are rejected.

Inspector prose prefers a branch's managed worktree only when all of these are
true:

- branch isolation says the worktree is TreeWork-managed;
- the worktree and descriptor resolve canonically;
- descriptor project ID, branch ID, and workspace path match accepted state.

Otherwise it reads the control-root branch documents. A bare persisted
`workspace_path` is never trusted.

At Tree revision zero, the projection includes one bootstrap root node derived
from accepted root branch state and marks its topology source as `bootstrap`.
It does not fabricate child topology. If the server has no last-good projection
and cannot obtain a coherent read, current-state routes return structured
`503 Service Unavailable` health data rather than waiting indefinitely or
returning partial state.

The production server exposes:

```text
GET /api/project-map
GET /api/project-map/branch?id=<branch-id>
GET /api/project-map/events
GET /api/project-map/replay?at=<seq>&after=<seq>&branch=<branch-id>
```

## Watcher And SSE

The watcher classifies changes instead of treating every write as a full
rebuild:

- publication marker or accepted state: coherent state refetch;
- `events.jsonl`: Replay metadata invalidation only after its sequence is
  published by `state/project.json`;
- stable branch document: branch Inspector and narrative revision;
- `tree.yaml` draft: editing indicator comes from project state; draft content
  is never projected.

Raw filesystem notifications are debounced and coalesced. SSE carries only
invalidation metadata, for example:

```text
event: invalidate
data: {
  "schema_version": 1,
  "kind": "project_map.invalidated",
  "changes": ["topology", "state"],
  "tree_revision": 7,
  "state_event_seq": 42,
  "narrative_revision": "sha256:..."
}
```

`changes` is a de-duplicated subset of `topology`, `state`, `narrative`,
`events`, and `health`. One debounced batch emits one invalidation containing
every relevant category.

The browser refetches the authoritative endpoint. SSE is not a second event
database and does not carry full nodes or transaction state.

## Replay Read Model

The replay route is:

```text
GET /api/project-map/replay?at=<seq>&after=<seq>&branch=<branch-id>
```

- `at` selects the reconstructed accepted state and defaults to the latest
  committed event sequence.
- `after` is exclusive and limits returned timeline transactions; it does not
  change how state is reconstructed.
- `branch` limits returned timeline transactions to events whose `subject` or
  `affected_subjects` include that branch. It does not filter the reconstructed
  global Tree.

The response contains:

```text
meta {
  live_event_seq, at_event_seq, checkpoint_event_seq,
  earliest_replayable_seq, tree_revision, projected_at
}
reconstruction {
  status: available | partial | unavailable
  gaps[] { from_seq, to_seq, reason }
}
state {
  project, nodes, dependencies
}
transactions[] {
  seq, time, type, subject, message, tree_revision,
  affected_subjects, changes, replayable
}
```

Reduction chooses the latest valid checkpoint whose `event_seq <= at`, then
requires a contiguous event sequence through `at`:

- `tree.applied` replaces reducer state with its referenced, verified
  checkpoint; its operations remain transaction-display data.
- stage events update project stage/editing state.
- lifecycle and verification events apply their explicit resulting state.
- unknown or legacy events with insufficient before/after data create an honest
  reconstruction gap. They may remain visible in the timeline but are not
  guessed into state.

Filtering is applied only after global reduction. Seeking or filtering one
branch must never remove unrelated nodes, skip prerequisite transactions, or
produce a state that never existed.

If no valid checkpoint can reach `at`, the route returns timeline and coverage
metadata with `state` unavailable rather than fabricating a snapshot. Invalid
sequence ranges return `400`; unknown accepted branch filters return `404`;
checkpoint integrity failures return degraded coverage and preserve the live
current-state projection.

## Acceptance

- No successful command can publish accepted state without exactly one semantic
  transaction event.
- No failed command advances `last_event_seq` or leaves a checkpoint/event/state
  combination that appears committed.
- Project Map never returns a half-written accepted-state tuple.
- Map and Dependency remain correct if the event log is unavailable beyond the
  committed tail; events never become the live state source.
- Replay can seek from checkpoints and deterministically reduce current-format
  events, while reporting unsupported legacy gaps honestly.
- Concurrent implementation preserves the ownership boundary above and
  requires no new Agent-facing command.
