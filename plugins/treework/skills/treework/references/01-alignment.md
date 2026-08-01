# Pre-Tree Alignment

Alignment turns low-resolution intent into a shared basis for development.
Creating its default documents is not Alignment, and filling them from the
Agent's first interpretation is not user confirmation.

Alignment is a problem-driven reasoning loop, not a fixed document-filling
sequence:

1. Preserve raw input only when it is still unresolved. User ideas, links,
   sketches, and fragments may go to `idea_inbox.md`; they are not yet project
   truth.
2. Investigate facts before asking the user to repeat them. Inspect existing
   code, documents, constraints, and history. When the user names a product,
   project, paper, protocol, or standard, consult authoritative sources before
   proposing directions. Record only evidence that materially affects the
   Requirements or Spec in `references.md`.
3. Form a concrete interpretation of what the user is trying to achieve,
   including likely product behavior and relevant technical choices. Present
   that interpretation and a recommendation to the user, expose meaningful
   alternatives or trade-offs, and ask for correction or confirmation.
4. Ask the user directly about intent, preference, boundaries, success, and
   trade-offs that evidence cannot decide. Do not create a durable question
   document instead of having the conversation. Write an item to
   `assumptions.md` only when an unresolved belief must survive across turns or
   current work genuinely depends on it; remove or resolve it once known.
5. After confirmation, write `requirements.md` as the user's accepted purpose,
   needs, observable outcome, boundaries, and success criteria.
6. Write or revise `.TreeWork/spec.md` as the Agent's coherent
   execution-before-action design based on those Requirements and the
   investigated reality. Spec applies to greenfield creation, maintenance,
   debugging, and exploration, not only feature coding.
7. Present Requirements and Spec together as the Alignment Review. Revise them
   until the user explicitly permits the project to leave Alignment.

The Agent may skip, revisit, or repeat these actions according to the actual
problem. Update only documents that currently carry useful durable knowledge.
Do not turn Alignment into a questionnaire or a ritual that produces empty
files.

Adapt the depth to the starting point. Greenfield work needs broad project
conception; mid-project work first reconstructs current reality; maintenance
work documents only the affected design horizon unless broader reconstruction
has a real project reason.

Read `05-spec.md` before writing the Spec. Idea Inbox, References, and temporary
Assumptions support the reasoning process; they do not replace the confirmed
Requirements or coherent Spec that later Agents use.

## Alignment Documents

- `idea_inbox.md`: optional raw input that has not yet been accepted.
- `references.md`: project or external evidence that materially informs the
  Requirements or Spec.
- `assumptions.md`: optional unresolved beliefs that must persist because
  current reasoning depends on them; not a question backlog.
- `requirements.md`: user-confirmed purpose, needs, outcomes, boundaries, and
  success criteria.
- `spec.md`: the Agent's coherent technical development design derived from the
  Requirements and investigated reality.

Requirements and Spec are the durable Alignment outputs. Use the other files
only when they improve current reasoning or preserve information that would
otherwise be lost.

## Alignment Exit

Alignment is ready to move forward when:

- the Agent and user share the same interpretation of the intended outcome;
- meaningful boundaries and success conditions are explicit;
- named references and discoverable project facts have been investigated;
- Requirements contain user-confirmed product truth;
- Spec contains a coherent technical direction sufficient for the next Tree
  decision;
- unresolved assumptions do not block that decision; and
- the user explicitly approves the Alignment Review.

Only after that approval does the Agent run `tw align end`. The command records
the accepted movement; it cannot perform or judge the reasoning that precedes
it. Build Tree must not begin from initialized-but-unreviewed documents.
