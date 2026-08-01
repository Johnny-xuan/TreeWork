# Declarative Build Tree Document Design

Status: accepted and implemented maintainer contract. Read this when changing
the `.TreeWork/tree.yaml` parser, Tree Editing Session, or `tw tree apply`.
Agent authoring guidance lives in the plugin Skill's `tree-yaml.md`. Project Map
product and refresh behavior live in `../product/project-map.md`.

## Contents

1. Purpose and truth boundary
2. Document ownership
3. Tree document language
4. Planning and Branch Intake
5. Tree Editing Session
6. Apply transaction
7. Validation and failure behavior
8. Replay event boundary
9. Runtime modules and migration
10. Verification plan

## Purpose And Truth Boundary

Build Tree turns accepted Alignment and pre-coding design into a navigable
development tree. The Lead Agent edits one declarative desired-state document
instead of issuing one command per branch or describing a procedural list of
moves, renames, splits, and edges.

The first build is:

```text
accepted Alignment and project Spec
  -> tw tree start
  -> edit .TreeWork/tree.yaml and relevant Specs
  -> review the complete candidate
  -> tw tree apply
  -> accepted tree revision
  -> Work Tree
```

A later structural revision uses `tw tree update`. There is no normal preview
step. `apply` is the parse, validation, semantic diff, and atomic commit
boundary.

The four relevant surfaces are distinct:

- `.TreeWork/tree.yaml` is the Agent-authored desired tree during an open Tree
  Editing Session.
- `.TreeWork/PROJECT.md` is the concise project entry point and document index.
  It does not duplicate topology.
- Structured state is the accepted runtime tree after `apply` succeeds.
- Project Map renders accepted state and never treats a half-written YAML draft
  as the current project.

## Document Ownership

| Information | Owner |
|---|---|
| Project identity, purpose, Tree Strategy, and document links | `.TreeWork/PROJECT.md` |
| Desired branch identity, hierarchy, purpose, Spec reference, order, and dependency | `.TreeWork/tree.yaml` |
| Accepted topology, dependency graph, tree revision, current branch, lifecycle, and verification | `.TreeWork/state/*.json` |
| Project and module technical development design | Root or branch `spec.md` |
| Executable scope, acceptance, steps, and local non-branch prerequisites | Root or branch `task_plan.md` |
| Current reality, meaningful progress, open issues, and exit context | Root or branch `progress.md` |
| Decisions, contract effects, risks, and unknowns learned during work | Root or branch `findings.md` |
| Development trajectory | Successful TreeWork transaction events |

No generated Branch Map belongs in `PROJECT.md`, root progress, or a second
human-maintained tree document. Project Map has no independent project database.

## Tree Document Language

TreeWork uses a small declarative YAML schema:

```yaml
version: 1

tree:
  id: root
  title: TreeWork
  purpose: Build software with stable project direction and branch-local memory.
  spec: spec.md

  children:
    - id: spec-system
      title: Spec System
      purpose: Complete substantial technical conception before coding.
      spec: branches/spec-system/spec.md

    - id: project-map
      title: Project Map
      purpose: Show project structure, dependencies, state, and trajectory.
      spec: branches/project-map/spec.md
      depends_on:
        - spec-system

      children:
        - id: project-map-replay
          title: Trajectory Replay
          purpose: Replay accepted TreeWork transitions.
          spec: branches/project-map-replay/spec.md
          depends_on:
            - transaction-events
```

The schema has only these fields:

- `version`: schema version. Version 1 is required.
- `tree`: exactly one root node.
- `id`: stable, unique, path-safe branch identity.
- `title`: human-facing name. Editing it does not change identity.
- `purpose`: one concise statement explaining why the branch exists.
- `spec`: optional path, relative to `.TreeWork/`, to the design reference owned
  by this branch.
- `depends_on`: optional list of branch IDs that must precede this branch.
- `children`: optional ordered child nodes.

The machine-readable shape is
`schemas/tree-document.schema.json`. Rust semantic validation remains
responsible for cross-node uniqueness, cycles, accepted-history protection, and
filesystem-safe Spec resolution.

Nesting is the only parent-child language. YAML order is the stable sibling order
used by Project Map. A node's `depends_on` list names its prerequisites. TreeWork
does not store the inverse `blocks` relation, and version 1 does not add vague
`affects`, `influences`, or `related_to` edges.

Status, verification, current branch, layout coordinates, notes, progress, and
implementation detail never belong in `tree.yaml`.

Agent edits are desired-state edits:

- add a node to create a branch;
- move the same `id` to change its parent;
- edit `title`, `purpose`, or `spec` to revise metadata;
- edit `depends_on` to revise execution prerequisites;
- add children to split a large scope into owned work.

Omitting an accepted branch does not delete history. Apply rejects unexplained
omission. Use the Work Tree `abort` transition when work will not continue;
completed and aborted branches remain addressable for Project Map and Replay.

## Planning And Branch Intake

Before editing:

1. Read accepted requirements, the root Spec, and only the relevant open
   questions or references.
2. Inspect the accepted tree and the likely parent, children, siblings,
   dependencies, state, and existing ownership.
3. Reuse or clarify an existing branch before creating a new one.
4. Choose or revisit a module-first, phase-first, milestone-first, or hybrid
   shape that fits the project. Do not force a universal granularity formula.
5. Shape the tree and relevant Specs together while conceiving the project.
6. Add `depends_on` only when it changes start order, parallelism, or delivery.
7. Review material topology or technical-direction changes with the user unless
   that authority was explicitly delegated.

A new branch needs a clear purpose, one parent, distinct implementation or
verification value, and a scope that cannot be absorbed coherently by an
existing branch.

## Tree Editing Session

`tw tree start` and `tw tree update` open the same transaction envelope:

- `start` is for the first accepted tree and scaffolds the initial tree document
  when absent;
- `update` starts from the latest accepted tree;
- both capture the accepted base tree revision, state hash, and event sequence;
- neither changes accepted topology merely because `tree.yaml` was edited.

During the session, the Lead edits `tree.yaml` and relevant Specs. Project Map
continues to show the last accepted revision with a visible editing indicator.

## Apply Transaction

`tw tree apply` is the only normal Build Tree commit boundary. Invoking it after
review is the commit decision.

Apply must:

1. acquire the mutation lock and recover any pending journal;
2. stable-read the YAML candidate and accepted base;
3. parse YAML with a real YAML parser into a typed schema;
4. validate the complete tree and referenced accepted state;
5. reject stale base revision or state hash;
6. compute semantic changes from desired state rather than asking the Agent to
   author operations;
7. scaffold required documents for valid new branches without overwriting
   existing content;
8. prepare accepted topology, branch state, transaction event, managed document
   blocks, and projection metadata under one recovery journal;
9. commit every output or restore the complete before-state;
10. advance tree revision, close the session, and return to Work Tree.

The semantic change set may contain branch creation, parent movement, metadata
revision, sibling-order revision, dependency addition, and dependency removal.
Identity deletion is not inferred from omission.

## Validation And Failure Behavior

Apply rejects:

- invalid YAML or unsupported schema version;
- a Tree document larger than the source budget, pathological YAML nesting, or
  a branch hierarchy deeper than 48 levels;
- aliases, anchors, or merge keys that would make the small declarative
  language indirect;
- missing or multiple roots;
- duplicate, empty, or unsafe IDs;
- parent cycles or dependency cycles;
- missing dependency endpoints;
- a node depending on itself;
- invalid or escaping Spec paths;
- an unexplained accepted-branch omission;
- mutation that violates protected completed or aborted history;
- stale base revision, changed source during prepare, or any partial write.

Errors identify the YAML path and line/column when the parser provides them.
Failure leaves accepted topology, lifecycle state, tree revision, event stream,
branch documents, and Project Map unchanged while keeping the editing session
open.

## Replay Event Boundary

One successful `tree apply` produces one `tree.applied` event. It records the
accepted tree revision and semantic changes needed to present the structural
step in Project Map Replay. It does not record editor keystrokes, code changes,
commands run, or file-line diffs.

Work Tree lifecycle commands produce their own small transition events. Project
Map Replay uses these committed workflow events to show how the Agent moved
through the tree.

## Runtime Modules And Migration

The Rust runtime now implements this contract. Its active modules are:

- `tree_document.rs`: typed YAML structs, source locations, parsing, and
  schema and resource-budget validation;
- `tree_diff.rs`: desired-tree flattening and semantic comparison against
  accepted state;
- `tree_transaction.rs`: journal and Apply plan data structures;
- `tree_migration.rs`: one-time conversion from accepted Project Index state to
  declarative YAML and accepted `state/tree.json`;
- `main.rs`: editing-session base checks, prepare-then-commit orchestration,
  protected-history checks, branch and Spec scaffolding, rollback, accepted
  snapshot, and `tree.applied` event integration;
- existing lifecycle state remains separate from topology so Tree Apply cannot
  overwrite status or verification.

Use `serde-saphyr` with typed serialization/deserialization and location-aware
errors. Apply preserves source path, line, column, and a short source snippet.
The parser applies size, event, node, scalar, comment, and structural-depth
budgets before accepting the document. Includes, aliases, anchors, and YAML
merge keys are outside the authoring language; one explicit Tree document is
the review and transaction boundary.

For an older workspace without `state/tree.json`, opening Build Tree performs
one migration before the editing session:

1. archive any old `tree.yaml`, `state/graph.json`, and
   `state/project-index.json` under `.TreeWork/archive/`;
2. derive one declarative Tree from accepted branch order, parent links, and
   supported `depends_on` edges;
3. preserve branch identity, lifecycle, verification, documents, event
   sequence, and accepted revision;
4. write `tree.yaml` and versioned `state/tree.json`, then continue through the
   normal editing and Apply path.

Unsupported legacy relation kinds remain in the archived snapshot and are
reported instead of being guessed into the new dependency model. New projects
receive `tree.yaml`, root `spec.md`, and the current document templates from
`tw init`. The active Markdown Branch Map parser has been removed.

The public Build Tree command surface remains exactly `tree start`, `tree
update`, and `tree apply`.

## Verification Plan

The runtime regression suite covers:

- minimal and deeply nested valid YAML plus explicit source and depth budgets;
- duplicate IDs, unsafe paths, malformed fields, missing dependency endpoints,
  parent cycles, and dependency cycles;
- title, purpose, Spec link, sibling order, parent movement, and dependency
  semantic diffs;
- accepted-branch omission and protected complete/aborted history;
- stale session base and source mutation during prepare;
- injected failure after every prepared write with exact rollback;
- fresh-project init and existing Project Index migration;
- unchanged branch identity, lifecycle, verification, documents, and event
  sequence across migration;
- Project Map staying on the previous accepted revision after failed Apply;
- source help, MCP projection, clean packaging, and installed-plugin activation.

Project Map integration remains a downstream consumer of this accepted Tree
state; it is not part of the Build Tree Apply implementation.
