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
- `id`: stable, unique, path-safe branch identity. Do not change an ID to rename
  a branch.
- `title`: human-facing branch name.
- `purpose`: one concise sentence explaining why the branch exists.
- `spec`: optional path relative to `.TreeWork/` for the technical design owned
  by this branch.
- `depends_on`: optional list of branch IDs that must complete before this
  branch can proceed.
- `children`: optional ordered child branches.

Use nesting for hierarchy and `depends_on` only for execution prerequisites.
Do not add vague `related_to`, `affects`, `blocks`, or layout edges.

## Desired-State Editing

Edit the complete desired tree rather than writing procedural operations:

- add a nested node to create a branch;
- move the same `id` to change its parent;
- edit `title`, `purpose`, or `spec` to revise metadata;
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
