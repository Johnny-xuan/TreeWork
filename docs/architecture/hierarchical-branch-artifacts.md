# Hierarchical Branch Artifacts

TreeWork's semantic Tree and its branch-document filesystem must describe the
same project shape. In TreeWork 0.1.6, branch state is hierarchical while
branch documents are stored in a flat `.TreeWork/branches/<branch-id>/`
directory. A custom Spec path can additionally split one branch's documents
between two locations.

TreeWork 0.1.7 replaces that split model with a deterministic filesystem
projection of the accepted Tree.

## Storage Contract

Branch identity and branch location are intentionally separate:

- the branch ID is the stable identity used by commands, events, dependencies,
  Replay, and worktree bindings;
- the artifact directory is derived from the branch's accepted parent chain;
- the derived path is never persisted as a second topology source;
- root documents remain directly under `.TreeWork/`;
- every non-root branch owns `spec.md`, `task_plan.md`, `progress.md`,
  `findings.md`, and `verification.md` in one directory.

For example:

```text
.TreeWork/
├── spec.md
├── task_plan.md
├── progress.md
├── findings.md
└── branches/
    └── public-release-adoption/
        ├── spec.md
        ├── task_plan.md
        ├── progress.md
        ├── findings.md
        ├── verification.md
        └── release-maintenance/
            ├── spec.md
            ├── task_plan.md
            ├── progress.md
            ├── findings.md
            └── verification.md
```

Each branch ID is encoded as one filesystem segment. Lowercase ASCII letters,
digits, `-`, and `_` remain literal. Every other allowed byte is percent
encoded, including `.` and `/`. This preserves a one-to-one mapping and stops
an ID from creating undeclared levels or colliding with managed filenames.

## Layout Versions

`state/project.json` records `artifact_layout_version`:

- missing or `1` means the legacy flat layout;
- `2` means the hierarchy derived from accepted parent relationships.

Read-only operations understand both layouts and never migrate implicitly.
The first locked mutation of a legacy project performs a protected one-time
migration. New projects start at layout version 2.

Migration moves every branch-owned file, not just the five standard Markdown
documents. Custom Specs become the canonical `spec.md` for their branch.
Branch IDs, lifecycle state, verification, dependencies, event sequence, Tree
revision, and worktree bindings do not change. `.TreeWork/archive/` is outside
the live resolver and is never migrated.

## Tree Apply

Tree Apply compares the committed layout with the candidate layout. Moving a
branch under another parent also changes the path of every descendant. Apply
therefore stages affected directories deepest-first and publishes them
shallowest-first inside the existing publication transaction.

No non-identical destination may be overwritten. Symlinks, paths escaping the
branches root, missing parents, cycles, and duplicate destinations fail closed.
Before the publication marker, any failure restores the exact previous paths
and bytes. After the durable marker, recovery only finishes the accepted state
forward.

## Runtime Rule

All consumers use the same resolver: scaffolding, lifecycle commands, Recall,
completion validation, managed worktrees, Project Map narratives and watchers,
MCP delegation, hooks, fixtures, and packaging tests. Production code must not
construct `.TreeWork/branches/<branch-id>` directly.
