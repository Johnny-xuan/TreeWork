# Project Map Product And Projection Design

Status: accepted product and implementation contract. Declarative accepted
Tree state, semantic transaction events, checkpoints, the coherent current
read model, the Replay read model, and the production Strata V3 React/D3/SVG
Map, Dependency, and Replay views are implemented and covered by release tests;
their interaction design remains iterative. The shared publication, event,
checkpoint, watcher, and parallel-ownership rules are normative in
`../architecture/transaction-projection.md`. Read these references only when
implementing or reviewing Project Map, its read model, transaction events, or
refresh behavior. Ordinary TreeWork Agents do not operate the panel.

## Contents

1. Product purpose
2. Three views
3. Inspector, transaction detail, and notes
4. Projection inputs and read model
5. Rendering architecture
6. Update triggers
7. Replay event contract
8. Draft and failure behavior
9. Read-only and command boundaries
10. Runtime and frontend modules
11. Implementation dependency order
12. Verification plan
13. Acceptance

## Product Purpose

TreeWork already guides an Agent without a graphical workbench. Project Map
exists for the user: it makes a long-running project visible, understandable,
and reviewable without reading raw state or every branch document.

The panel answers three different questions through three explicit views:

```text
Map         -> How is the project organized, and where is work now?
Dependency  -> What must happen before this branch, and what does it unlock?
Replay      -> How did the Agent move through and revise the tree over time?
```

These are structural, causal, and temporal views of one accepted TreeWork state.
They are not separate databases.

## Map View

Map is the default view. It uses the accepted long-scroll depth-column
hierarchy:

- one visual column per parent-child depth;
- siblings and complete subtrees retain contiguous vertical regions;
- stable sibling order comes from accepted `tree.yaml` order;
- parent-child connectors remain continuously visible;
- the root-to-current route is the primary highlighted path;
- `current` and `in_progress` remain distinct;
- lifecycle state is readable without relying on color alone;
- collapsed branches preserve a visible child count and state summary;
- search locates branches without destroying spatial context;
- filtering dims non-matches by default instead of globally reflowing the tree.

Map derives `ready` and `waiting` presentation:

- a pending branch is ready when every structured `depends_on` prerequisite is
  complete;
- a pending branch is waiting when at least one prerequisite is not complete;
- this is a visual derivation, not a new persisted lifecycle state.

## Dependency View

Dependency is a focused causal view, not a global overlay of every edge.
Selecting a branch arranges:

```text
upstream prerequisites -> selected branch -> downstream dependents
```

The selected branch remains centered. Direct dependencies appear first; the user
may expand additional upstream or downstream levels. Each dependency node keeps
its lifecycle state and can become the new focus.

Dependency View must show:

- which prerequisites are satisfied or unsatisfied;
- why a pending branch is waiting;
- which downstream branches the selected branch unlocks;
- which ready leaf branches have no dependency path to the selected branch and
  can therefore be considered for parallel work;
- a breadcrumb back to the selected branch's parent hierarchy.

Parallel suggestions exclude parent coordination branches, the selected
branch's ancestors and descendants, and every branch in its upstream or
downstream dependency closure. They remain advisory: dependency independence
does not prove that two branches cannot edit the same files.

Only structured `depends_on` relations participate in this graph. Textual
external prerequisites from `task_plan.md` appear in the Inspector but do not
become graph edges automatically.

Selecting a branch is a navigable action rather than a destructive focus
replacement. Map and Dependency share a bounded browser-session branch history:
new branch navigation clears the forward stack, while visible Back and Forward
controls restore earlier focused branches. Search, canvas nodes, Inspector
relations, and locate-current all use the same history. Replay keeps its own
timeline semantics and does not enter branch focus history.

## Replay View

Replay presents accepted TreeWork trajectory, not code editing history.

It provides:

- play, pause, step, speed, and a draggable transaction timeline;
- branch filtering for one branch's creation, entry, status, and structural
  history, including branches no longer present in the live Tree;
- visible branch creation, movement, metadata revision, dependency revision,
  entry, pause, completion, and abortion;
- a transaction detail panel explaining the accepted workflow change;
- return to live state without mutating the project.

One successful TreeWork transaction is one timeline step. A `tree apply`
transaction may contain several semantic tree changes, displayed as one grouped
step. Replay does not record keystrokes, commands run, code diffs, or internal
Agent reasoning.

## Inspector, Transaction Detail, And Notes

Map and Dependency share one branch Inspector:

- identity, title, purpose, parent, and lifecycle state;
- current and verification signals;
- Spec link;
- Task Plan scope, acceptance, and textual external dependencies;
- Progress current reality, latest substantive work, open issues, and exit
  notes;
- Findings decisions, contract effects, risks, and unknowns;
- verification evidence and gaps.

Replay does not pretend current branch documents are historical documents. It
uses a transaction detail panel for sequence, time, event type, subject,
message, tree revision, affected branches, replayability, and human-readable
semantic changes. Selecting a Replay node filters that branch's transaction
history while preserving the complete reconstructed Tree.

Project Map notes are temporary user scratch space. They are local UI data, do
not change branch state or dependencies, and are never promoted automatically.
If a note implies a real structural change, the user asks the Agent to update
`tree.yaml` through Build Tree.

## Projection Inputs And Read Model

| Input | Responsibility |
|---|---|
| Accepted `state/tree.json` | Branch identity, hierarchy, order, purpose, Spec reference, and `depends_on` |
| Accepted project and branch state | Stage, current branch, lifecycle, verification, status reason, and tree revision |
| Transaction events | Replay timeline and incremental lifecycle updates |
| Branch documents | Inspector narrative |
| Browser session state | Collapse, selection, filters, viewport, and temporary notes |

The backend exposes one read model rather than making the browser parse project
files:

```text
GET /api/project-map
GET /api/project-map/branch?id=<branch-id>
GET /api/project-map/replay?at=<seq>&after=<seq>&branch=<branch-id>
GET /api/project-map/events
```

Replay reconstructs the complete global state at `at` before applying `after`
or `branch` to the returned transaction timeline. Historical branch identity is
resolved from live state, valid checkpoints, and transaction history rather
than only the current Tree.

`/api/project-map/events` is a Server-Sent Events stream carrying revision
notifications. The browser refetches authoritative read models after a relevant
notification; SSE messages are invalidation signals, not a second state stream.

Every full projection includes:

```json
{
  "tree_revision": 14,
  "state_event_seq": 392,
  "narrative_revision": "sha256:...",
  "tree_editing": false,
  "projected_at": "2026-07-27T10:00:00+08:00"
}
```

Generated JSON, HTML, layout coordinates, and browser state are disposable
projections or caches.

## Rendering Architecture

The production panel uses:

- TypeScript and React for application state, routing, controls, and Inspector;
- D3 hierarchy for deterministic depth-column layout;
- SVG for connectors, nodes, hit targets, zoom, pan, and replay transitions.

Mermaid V1/V2 and the Sigma/WebGL workbench remain prototypes and engineering
history. They are not production dependencies. Strata V3 is the implemented
visual language, while React, D3 hierarchy, and SVG remain the production data
and rendering architecture.

The renderer keeps branch IDs stable across updates. Lifecycle-only changes
patch node presentation without recomputing hierarchy. Tree revision changes
recompute layout while preserving positions for unchanged subtrees where
possible.

## Update Triggers

Project Map refresh semantics are fixed:

1. **Panel open or reconnect**: fetch the latest complete accepted projection.
2. **Successful `tree apply`**: increment `tree_revision`, rebuild topology and
   dependency indexes, and recompute Map layout.
3. **Successful lifecycle transaction**: increment `state_event_seq`; patch the
   affected node, current route, readiness derivations, and Replay index without
   global relayout.
4. **Stable branch-document change**: update only the Inspector after a
   debounced stable read.
5. **Transaction event append**: extend Replay metadata; it does not imply a
   topology rebuild unless the event is `tree.applied`.
6. **Temporary note change**: update browser session state immediately and
   nowhere else.
7. **User refresh**: discard projection caches and refetch accepted state,
   narrative, and Replay metadata.

The first accepted Tree has one additional presentation trigger. After a
successful `tree apply` advances revision 0 to revision 1, the Agent starts or
reuses the Project Map service and opens its localhost URL in the Codex in-app
browser. This is a Build Tree exit handoff, not a state transaction and not a
general auto-open policy. Later applies only publish state and invalidations;
they do not open additional tabs.

The local backend watches accepted state, the transaction event file, and
Inspector document paths. It debounces writes, waits for the publication marker
defined by `../architecture/transaction-projection.md`, and emits SSE
invalidations.
TreeWork CLI commands do not contact an open browser.

`tree start` and `tree update` change only the editing indicator. Editing
`tree.yaml` does not refresh accepted topology. Failed apply leaves the prior
map revision visible.

## Replay Event Contract

Events exist only for successful TreeWork state transactions. The minimum
replayable event families are:

- `tree.applied`: resulting tree revision plus semantic branch and dependency
  changes;
- `branch.entered`: previous current branch and new current branch;
- `branch.paused`: branch plus status transition and optional reason;
- `branch.completed`: branch plus status transition and verification summary;
- `branch.aborted`: branch plus status transition and reason;
- verification changes when they alter visible branch state.

Each event uses the versioned envelope and typed `data` payload defined by
`../architecture/transaction-projection.md`. No generic file-operation log is
required. Checkpoints at accepted tree revisions allow timeline seeking without
replaying the entire project from initialization.

Each accepted Tree Apply writes an immutable checkpoint under a history-owned
path and records its relative `snapshot_ref` in `tree.applied`. Lifecycle events
after that checkpoint are reduced in sequence to reconstruct a selected time.
The checkpoint stores TreeWork topology and visible lifecycle state, not source
files or code.

The raw event file remains internal infrastructure. There is no Agent-facing
`log` command, and Replay is a Project Map view rather than a CLI printout.

## Draft And Failure Behavior

While Tree Editing is open:

- show the last accepted topology;
- show a visible editing indicator;
- do not parse the YAML draft in the panel;
- publish only after successful apply.

If topology or narrative refresh fails, preserve the last valid projection and
show a degraded indicator. The panel never guesses missing parents, silently
drops dependencies, or creates lifecycle defaults.

## Read-Only And Command Boundaries

Project Map may inspect, search, filter, focus, expand, collapse, replay, and
hold temporary notes. It has no endpoint for branch creation, movement,
dependency mutation, lifecycle transition, or document writing.

No Project Map render, sync, rebuild, or refresh command belongs in the
Agent-facing Skill. Internal server, projector, migration, and diagnostic entry
points may exist for the product and its tests.

## Runtime And Frontend Modules

The local product backend uses Rust with Axum, Tokio, and `notify`:

- `project_map_read_model.rs` composes accepted Tree, lifecycle state, and
  document summaries;
- `project_map_replay.rs` loads checkpoints and reduces transaction events;
- `project_map_server.rs` exposes read-only JSON routes, static assets, and SSE;
- a file watcher debounces accepted state, event, and branch-document changes
  before publishing revision invalidations.

Bind only to `127.0.0.1`. Public routes are GET-only, and the server rejects
non-loopback `Host` or browser `Origin` values to prevent local-service
rebinding. The server must normalize branch IDs and reject path traversal
before reading branch documents.

The frontend lives in a dedicated TypeScript/Vite package and builds static
assets into the plugin:

```text
project-map-ui/
  src/
    app/
    views/map/
    views/dependency/
    views/replay/
    inspector/
    data/
    layout/
```

React owns view mode, selection, filters, Inspector state, viewport, and SSE
revalidation. D3 hierarchy computes parent-child geometry. One shared SVG scene
and stable keyed branch nodes are reused by Map, Dependency, and Replay rather
than building three unrelated renderers.

Temporary notes and view preferences use browser session state keyed by project
identity. They never pass through the Rust API.

The Vite build bundles React and D3. Sigma, Graphology, Mermaid, and their
vendor manifests are absent from the production package.

## Runtime Layering

Project Map is built from five stable layers:

1. **Declarative Tree contract**: `tree.yaml` describes a proposed topology;
   accepted topology is published to `state/tree.json` only by a successful
   Build Tree transaction.
2. **Transaction and event layer**: lifecycle and topology changes publish
   accepted state, semantic events, and replay checkpoints coherently.
3. **Read-model layer**: current-state projections combine accepted topology,
   branch state, branch documents, and dependency data. Replay projections
   reconstruct an earlier accepted state from checkpoints and events.
4. **Presentation layer**: Map, Dependency, and Replay share the React/D3/SVG
   scene, selection model, Inspector, search, and viewport controls.
5. **Validation layer**: coherence checks, traversal protection, reconnect
   behavior, accessibility tests, and deep/wide fixtures protect the read-only
   projection without changing accepted project state.

Map and Dependency consume the current-state read model. Replay additionally
depends on the event and checkpoint reducer. All three views use the same branch
identity, lifecycle vocabulary, and accepted topology, so switching views does
not create a second interpretation of project state.

## Verification Plan

Backend and reducer tests must cover:

- exact refresh classification for tree, lifecycle, document, event, reconnect,
  note, and manual refresh changes;
- stale or partial narrative writes retaining the last valid Inspector data;
- SSE reconnect and missed-event refetch by revision;
- checkpoint seeking and deterministic reduction to an arbitrary event sequence;
- branch-filtered Replay without corrupting the reconstructed global state;
- invalid branch IDs, path traversal, malformed event records, and missing
  checkpoints.

Browser tests must cover:

- full-tree readability at desktop and mobile viewports;
- stable positions after lifecycle-only updates and filters;
- collapse, expand, search, locate current, branch Back/Forward history, zoom,
  pan, and keyboard navigation;
- direct and transitive Dependency traversal with satisfied and unsatisfied
  prerequisites, plus leaf-only advisory parallel candidates;
- Replay play, pause, step, scrub, speed, branch filter, and return to live;
- nonblank SVG output and no incoherent overlap on deep and wide fixtures;
- reduced motion, color-independent status, focus visibility, and screen-reader
  labels.

Performance tests should use real-size, deep, wide, and stress trees. Add
viewport culling only if measurement shows accepted SVG rendering needs it.

## Acceptance

- The user can read the complete parent-child tree and locate current work.
- Dependency View explains prerequisites, waiting work, downstream impact, and
  actionable parallel opportunities for a selected branch without presenting
  parent coordination branches as executable work.
- Branch focus changes can be reversed and replayed with visible Back and
  Forward controls.
- Replay reconstructs accepted TreeWork trajectory by transaction and can filter
  to one branch.
- Project Map always renders accepted state, never an uncommitted YAML draft.
- Topology, lifecycle, narrative, Replay, and note changes trigger only the
  smallest necessary refresh.
- The panel remains read-only and adds no Agent workflow command.
- Mermaid and Sigma/WebGL are absent from the production rendering path.
