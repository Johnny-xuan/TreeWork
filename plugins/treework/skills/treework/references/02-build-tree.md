# Build Tree

Build Tree creates or revises the declarative Tree that guides Work Tree. It uses
one coherent editing pass instead of one command per branch or dependency.

## Open One Editing Pass

Use the first Build Tree pass to create the initial tree. Use Update Tree for a
later structural revision. Both start from accepted project reality and treat
the complete candidate as one reviewable transaction.

## Plan Before Editing

1. Read accepted requirements, the project Spec, and only the relevant
   assumptions or references.
2. If a Tree exists, inspect its branches, parent-child structure, status,
   purpose, Spec references, and dependencies.
3. Choose or revisit a module-first, phase-first, milestone-first, or hybrid
   shape that fits the project. Module-first and phase-then-module are common;
   do not force one granularity formula.
4. Shape the Tree and relevant Specs together. Design may reveal a better branch
   structure, and tree structure may reveal missing design.
5. Review material topology or technical-direction changes with the user unless
   that authority was explicitly delegated.

## Branch Intake Gate

Reusing an existing branch is the default. Before creating one:

1. Check the likely parent, children, siblings, dependencies, status, and current
   ownership.
2. Reuse an existing branch when its scope can absorb the work coherently.
3. Correct stale scope rather than creating a duplicate owner.
4. Move or split only when the structure itself is wrong.
5. Create only when the branch has a clear purpose, parent, independent
   implementation or verification value, and reason.

## Edit The Tree Document

Edit `.TreeWork/tree.yaml` as desired state:

- nesting defines parent-child hierarchy;
- YAML order defines stable sibling order;
- `purpose` gives Project Map one concise branch explanation;
- `spec` points to the relevant technical development white paper;
- `depends_on` records prerequisites that affect order or parallelism.

Do not write lifecycle state, verification, current branch, progress, layout, or
notes into the Tree document. Do not describe procedural operations such as
`move`, `rename`, or `split`; move the stable node, edit its metadata, or add
children so Apply can derive the semantic change.

Create or revise `.TreeWork/branches/<branch>/spec.md` for implementation
branches with meaningful local design. A purely organizational branch needs a
Spec only when it adds shared technical direction for descendants.

Read `tree-yaml.md` when the exact Agent-facing YAML fields and editing language
matter. The corresponding machine-readable shape is
`../schemas/tree-document.schema.json`.

## Apply

After review, apply the complete candidate as one transaction. There is no
normal preview step.

Apply parses typed YAML, validates IDs, references, parent and dependency cycles,
Spec paths, protected history, and the accepted base revision. It computes the
semantic diff, scaffolds valid new branch documents, and commits state,
transaction event, managed blocks, and projection metadata atomically.

If validation fails, it reports the YAML path and location when available,
changes no accepted state, and leaves the editing session open. Fix the
candidate and apply again.

## Project Map Boundary

Project Map continues to show the last accepted Tree while an editing session is
open. Successful Apply triggers topology and dependency refresh. Lifecycle
transactions update status and the current route without rebuilding the whole
layout. The Agent never operates Project Map refresh machinery.

The first successful Apply also completes a user-facing handoff. When the Tree
revision moves from 0 to 1, call `treework_project_map` with the absolute
workspace path and open the returned localhost URL in the Codex in-app browser.
Do not substitute the system browser, and do not stop after merely printing the
URL when the in-app browser is available. Later Apply transactions do not open
another tab; an open panel updates itself, and a closed panel is relaunched only
on an explicit user request.

## Boundaries

- `PROJECT.md` is project orientation, not topology storage.
- `tree.yaml` is the only Agent-maintained topology document.
- Specs provide design reference, not a second Tree.
- Structured state is accepted runtime truth after Apply.
- Omission never silently deletes accepted history.
- Only the Lead edits topology; workers report structural needs.
