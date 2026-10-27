---
name: External operation guidance
description: Use when planning operations that affect state outside the workspace; not applicable to local-only changes.
metadata:
  tags:
    - external-operations
---

# External operation guidance

1. [Required] External operations change remote services, data, deployments, messages, accounts, permissions or other state outside the local workspace. Even a read-only query must record its source and possible staleness.
2. [Required] Use the existing Input, Decision, Risk, Step, `OP-*` and `VAL-*` fields to fix target, prerequisites, side effects, authorization boundary, success evidence and failure outcomes completely. Do not create a parallel format.
3. [Required] Add corresponding risks and handling only when the operation involves network access, permissions, cost, irreversibility, partial success, duplicate submission or uncertain outcome. Do not substitute a generic risk list for an actual decision.
4. [Default] Do not automatically retry a failed external operation. Retry only when the error is confirmed transient, the operation is idempotent or has a reliable idempotency key, and attempt limit and stop conditions are fixed.
5. [Required] Compensation is a separate external operation with its own OP, risk, authorization and validation. Deletion, restoration or reverse writes are not implicit steps of the original operation.
6. [Required] The TASK must distinguish success, failure, partial success and uncertain result. Do not claim success without confirming final remote state or resend to guess the state.
7. [Required] Do not write secret values into TASKs, commands, records or replies. When a secret is necessary, record only its secure acquisition method and the boundary for user execution.
8. [Required] Use one result schema for each applicable OP and the overall operation: `Overall result` accepts only `Complete success`, `Partial success`, `Failure` or `Uncertain result`; list `Effective`, `Not effective` and `Unknown` separately, omitting empty categories. OP success means confirmed effect, OP failure belongs under Not effective, and unconfirmed outcomes belong under Unknown. The overall result must agree with OP details.
9. [Required] A failed external operation blocks the TASK. Retry within the same Attempt up to an approved limit only when the TASK fixed safe retry conditions and they currently hold; otherwise close it and obtain new authorization for continuation.
10. [Required] Local CMD/VAL and file-recovery evidence do not grant External OP authorization, prove remote success or provide cross-system atomicity. Handle External OP side effects, unknown outcomes and compensation separately. When the formal execution entry point lacks a trustworthy effect boundary, retain Unknown rejection; command or validation labels do not grant capability.
