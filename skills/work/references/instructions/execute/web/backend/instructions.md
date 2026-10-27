---
name: Web backend task execution
description: Use for TASK-fixed backend security, compatibility, resilience, data and external-operation validation; not applicable to frontend-only work.
metadata:
  tags:
    - backend
    - server-side
---

# Web backend task execution

Instruction boundary: This layer only executes the backend TASK's fixed security, compatibility, resilience, data and external-operation validation; it does not redefine the technical contract.

1. [Required] Before execution, confirm that the TASK includes security, API compatibility, resilience and external-operation references for their actual triggers. Stop for a specification defect when current evidence makes one applicable but the TASK omits it.
2. [Required] Use the TASK and existing tools to validate success, invalid input, rejection, errors, consistency, compatibility, resource limits and minimum operational signals. Record only evidence needed for acceptance.
3. [Required] Before an external operation, reconfirm environment, target, authorization and idempotency conditions. Close partial or uncertain results through execution records; do not guess or resubmit without authorization.
4. [Required] Stop when actual framework, service, configuration or consumer evidence differs from the TASK. Execute must not upgrade dependencies, add tools, change architecture or loosen security controls.
5. [Required] Before Red, confirm the preceding minimal skeleton and target test exist and that the command can compile, load and initialize far enough to reach the specified test. Count Red as passed only when the TASK-fixed target failure occurs; handle other failures under the general error rules.
