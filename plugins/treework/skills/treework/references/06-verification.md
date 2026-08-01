# Verification

Completion requires evidence.

Verification labels:

- `unverified`: no meaningful evidence.
- `partial`: useful checks passed but important gaps remain.
- `verified`: relevant acceptance checks passed.
- `failed`: a relevant check failed.

A protected completion gate should reject the branch if:

- verification is not `verified`;
- acceptance checklist is still unchecked;
- the branch is already aborted;
- generated blocks and state are inconsistent;
- graph state references missing branches.

Evidence must match branch acceptance. Use the project's real tests, targeted
manual checks, static analysis, screenshots, benchmarks, or review evidence as
appropriate. Record what was verified and any remaining gap; do not treat
running an unrelated generic command as proof.
