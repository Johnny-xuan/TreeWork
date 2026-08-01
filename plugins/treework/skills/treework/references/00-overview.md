# TreeWork Overview

TreeWork is a state-native project memory plugin that gives an Agent project
space sense. Code and tests remain implementation truth; TreeWork preserves the
accepted organization, design, branch-local reality, verification, and
semantic development trajectory around that implementation. The project has
two connected structures:

- The Project Tree defines scope hierarchy.
- The graph defines execution relationships.
- The Lead Agent owns the declarative Tree candidate and structural decisions.
- Specs preserve technical development thinking formed before coding.
- Branch documents own branch-local Spec, plan, narrative reality, and
  conclusions.
- Structured state supports runtime behavior and projection; events remember and
  validators judge.

The plugin supports Project Map, read-only MCP, branch recall and isolation,
and prompt-guided external-agent collaboration without duplicating
authoritative state.

Transaction commands are durable workflow marks, not a replacement authoring
language for project knowledge. They record meaningful stage, Tree-editing,
branch-location, lifecycle, verification, and completion movement. Narrative
knowledge remains in the document that owns its meaning.

## Three Stages

1. Pre-Tree Alignment: turn vague intent into requirements, shared language, and
   an overall technical development conception.
2. Build Tree: shape declarative `tree.yaml` and relevant root/branch Specs
   together, then apply one accepted tree transaction.
3. Work Tree: enter branches, read the relevant Spec, execute the plan, verify,
   transition, and sync reality.

## Document Language

Each document has one durable job:

- `PROJECT.md` is the concise project entry point. It records project purpose,
  Tree Strategy, and links to durable documents without duplicating topology.
- `tree.yaml` is the declarative tree document. It records branch hierarchy,
  stable order, one-line purpose, Spec references, and `depends_on`.
- `requirements.md` records what the user needs, the constraints, and what
  success means.
- Root and branch `spec.md` files record the technical development thinking
  formed before coding.
- `task_plan.md` records executable scope, acceptance, and steps.
- `progress.md` records current reality and meaningful progress events.
- `findings.md` records conclusions discovered during development that cannot be
  recovered reliably from code.
- `verification.md` records evidence for acceptance.

`idea_inbox.md`, `assumptions.md`, and `references.md` support Alignment when
useful. They are not mandatory parallel versions of the Requirements or Spec
and should not become permanent dumping grounds. Questions that need user intent
belong in the conversation, not a separate durable question backlog.

## State Model

`tree.yaml` is the Lead Agent's topology authoring surface. Specs are
Agent-authored design references and remain ordinary Markdown rather than
structured state. When an installed runtime maintains accepted topology, the
tree document is edited as one desired-state candidate against the accepted base
and committed as one validated transaction. Root and branch Markdown own Specs,
plans, and narrative fields; runtime-owned lifecycle fields remain
machine-owned.

The project-state protocol is independent of a particular CLI spelling. Current runtime
bindings belong in `command-reference.md`; implementation semantics for the tree
document and Project Map belong in their dedicated design references.
