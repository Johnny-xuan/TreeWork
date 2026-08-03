# Spec-First Development

TreeWork adds design memory to tree-guided development:

```text
沿 Tree 移动
按 Spec 开发
用 Plan 执行
让 Progress 反映现实
用 Findings 保存后来发现的结论
```

## What Spec Means

A Spec is a technical development white paper written before coding. It
externalizes how the Agent intends to design and develop a project, module, or
phase while its reasoning context is still clean and globally aware.

Spec is not:

- a raw conversation transcript;
- a requirements list;
- a task checklist;
- progress history;
- a duplicate of the Project Tree.

Requirements describe what the user needs and which constraints matter. Spec
records the Agent's considered technical response. Task Plan turns that design
into executable work.

## Root And Branch Specs

Use `.TreeWork/spec.md` for project-level development thinking. It explains the
overall technical direction, important modules or phases, their relationships,
and the intended development approach.

Use the branch directory's `spec.md` for the concrete development thinking that
belongs to one implementation branch. The directory follows the accepted Tree:
branch `api` under `platform` uses
`.TreeWork/branches/platform/api/spec.md`. A branch Spec inherits the relevant
project direction without copying unrelated global or sibling detail. A purely
organizational branch needs its own Spec only when it adds shared technical
direction for descendants.

Root and branch Specs are the same kind of document at different scopes. Do not
invent separate Spec types for intermediate branches.

## Tree And Specs Grow Together

Tree and Specs are two views of the Agent's project conception:

- Tree is the project structure and index: it records how development is
  organized, where work moves, which branch owns detail, and which prerequisites
  affect order.
- Specs record how the project and its parts are intended to be designed and
  developed.

While conceiving the project, the Agent may revise `tree.yaml` because a Spec
reveals a better structure, or revise a Spec because the Tree reveals a missing
relationship. This is normal. Spec-first means design before code, not a rigid
`finish every Spec -> generate Tree` sequence.

Keep Tree nodes concise. Do not copy branch Spec bodies into `purpose`; use the
node's canonical `spec` path and hierarchy-derived branch directory to locate
Spec, Plan, Progress, Findings, and Verification. The branch ID is stable even
when its parent changes; Tree Apply relocates the complete document subtree.

Choose a tree shape that fits the actual project. Module-first and phase-then-
module are common, but TreeWork does not impose a branch-granularity algorithm.

## Adapt To The Starting Point

- Greenfield: develop the overall technical conception and the relevant
  root/branch Specs while building the first Tree.
- Mid-project: inspect code and existing documents, then capture the current
  architecture and the design needed for remaining work.
- Maintenance: write only the design context needed for the affected scope; do
  not reconstruct the whole project without a project reason.

Scale the Spec to the work. A small change may need only a short, precise design
note. A large project may need several branch-scoped Specs so no Agent must load
unrelated design detail.

## Write For The Work

Do not force every Spec through one universal set of headings. Organize it around
the decisions that would otherwise be invented during coding. Depending on the
work, that may include architecture, interaction states, data flow, interfaces,
failure behavior, compatibility, migration, testing, or development strategy.

The test is practical: after reading the relevant Spec, a Coding Agent should
understand the intended design without reconstructing it from old chat or
inventing important behavior while implementing.

Humans or other Agents may review a Spec before implementation. Preserve the
coherent design and the reasons future implementers cannot recover from code;
do not preserve every exploratory thought.

## Coding Boundary

A Coding Agent may decide local implementation details. It should not silently
invent product behavior, system boundaries, core architecture, or module
contracts. When one of those is missing or changes materially, pause coding,
update the relevant Spec, and adjust the Tree only if project structure also
changed.

Do not touch a Spec on every transition. Update it only when the intended design
changed or when the existing reference is materially incomplete.

Findings preserve what implementation taught the project. When a finding changes
the intended design, keep the learned conclusion in Findings and revise the
relevant Spec so later Coding Agents follow the new design rather than the old
one.

## Command Boundary

Spec is Agent-authored Markdown. TreeWork needs no `spec` command, Spec state
machine, coverage score, or universal readiness gate. Consider automation later
only if ordinary file editing proves unable to keep a small, repetitive,
machine-owned part reliable.
