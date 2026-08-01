# Reporting

TreeWork reports should be short but stateful.

When finishing a TreeWork-guided turn, report:

- Stage and current branch.
- Branch status and verification status.
- Material Spec changes, when the intended design changed.
- Material Tree, Spec, or branch-document changes.
- Verification results and unresolved document-validation errors.
- Parallel-worker handoff or integration caveats, when relevant.
- Next branch transition.

Do not claim a branch is complete unless its protected completion boundary
succeeded.

Example:

```text
Branch: state-harness
Status: complete
Verification: verified
Map change: none
Open gaps: Codex hook trust still requires user review
Next: return tools to the control workspace and choose the next branch
```
