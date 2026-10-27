---
name: Web task execution
description: Use to execute and validate TASK-fixed Web contracts, consumer outcomes and compatibility requirements; not applicable to non-Web work.
metadata:
  tags:
    - web
---

# Web task execution

Instruction boundary: This layer only executes and validates the TASK-fixed Web contract; it makes no new routing, input, response, authorization or compatibility decisions.

1. [Required] Before execution, verify that the TASK fully fixes affected entry points, consumers, inputs, success/error outputs and VAL. Stop for a specification defect if Execute would have to infer them.
2. [Required] Operate the actual entry points and existing validation capabilities only as specified by the TASK. Record status, contract and consumer-observable outcomes; an internal method's success cannot replace boundary VAL.
3. [Required] If the actual Web contract differs from the TASK, do not update documentation or accept new behavior independently. Record the smallest difference and return to Task.
4. [Required] Execute only the TASK-fixed compatibility, security, performance, accessibility and deployment items. Do not start or deploy a service absent from CMD or perform an external-state operation absent from OP.
