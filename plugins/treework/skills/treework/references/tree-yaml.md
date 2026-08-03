# Tree Document Reference

Read this reference when writing `.TreeWork/tree.yaml`. It describes the small
declarative language available to the Lead Agent. Build Tree workflow and review
discipline remain in `02-build-tree.md`.

## Shape

```yaml
version: 1

tree:
  id: root
  title: Example Project
  purpose: Build the accepted product.
  spec: spec.md

  children:
    - id: foundation
      title: Foundation
      purpose: Establish shared runtime and data contracts.
      spec: branches/foundation/spec.md

      children:
        - id: runtime
          title: Runtime
          purpose: Implement the shared execution core.
          spec: branches/foundation/runtime/spec.md

    - id: project-map
      title: Project Map
      purpose: Make accepted project structure visible to the user.
      spec: branches/project-map/spec.md
      depends_on:
        - foundation
```

The document has exactly one root. Nested `children` define parent-child
hierarchy, and YAML order defines stable sibling order.

## Fields

- `version`: document schema version. Use `1`.
- `tree`: the single root branch.
- `id`: stable, unique branch identity used by commands, dependencies, events,
  Replay, and worktree bindings. Do not change an ID to rename a branch.
- `title`: human-facing branch name.
- `purpose`: one concise sentence explaining why the branch exists.
- `spec`: optional path relative to `.TreeWork/` for the technical design owned
  by this branch.
- `depends_on`: optional list of branch IDs that must complete before this
  branch can proceed.
- `children`: optional ordered child branches.

Use nesting for hierarchy and `depends_on` only for execution prerequisites.
Do not add vague `related_to`, `affects`, `blocks`, or layout edges.

## Canonical Artifact Paths

Tree hierarchy also determines branch-document directories. Every non-root
branch contributes one encoded directory segment beneath its parent:

```text
root / foundation / runtime
  -> .TreeWork/branches/foundation/runtime/
```

Its `spec` value must name the canonical `spec.md` in that directory, relative
to `.TreeWork/`. The other branch documents live beside it and are not listed
in YAML. Root documents remain directly under `.TreeWork/`.

The branch ID remains stable when a branch moves; the artifact path does not.
Move the node and update its `spec` value in the same candidate. Apply derives
and atomically relocates the branch and every descendant. Do not move managed
branch directories by hand.

IDs may contain the schema's supported punctuation, but each complete ID is one
filesystem segment. Lowercase ASCII letters, digits, `-`, and `_` remain
literal; other bytes are percent encoded. For example, branch ID `api/v2` under
`platform` owns `branches/platform/api%2Fv2/spec.md`; the slash does not create
another semantic level.

## Desired-State Editing

Edit the complete desired tree rather than writing procedural operations:

- add a nested node to create a branch;
- move the same `id` to change its parent;
- edit `title` or `purpose` to revise metadata;
- update `spec` when nesting changes so it remains the canonical derived path;
- edit `depends_on` to revise prerequisites;
- add children when a scope needs distinct owned work.

Reuse or correct an existing branch before creating another one. A new branch
needs a clear purpose, one parent, distinct implementation or verification
value, and a scope that cannot be absorbed coherently by an existing branch.

Omission is not deletion. Do not remove accepted branches from YAML to retire
them; use the Work Tree abort transition so history remains addressable.

## What Does Not Belong Here

Do not put any of the following in `tree.yaml`:

- lifecycle status or current branch;
- verification or progress;
- implementation detail copied from a Spec;
- Project Map layout coordinates;
- temporary notes;
- Agent identity or assignments;
- procedural `move`, `rename`, `split`, or `delete` instructions.

Specs own technical design. Plans own executable work. Progress owns reality.
TreeWork transactions own accepted lifecycle state.

## Apply Boundary

Review the whole candidate and relevant Specs before `tw tree apply`. Apply
parses and validates the document as one transaction. If validation fails,
correct the reported YAML path or source location and apply again; accepted
state remains unchanged until a complete candidate succeeds.
