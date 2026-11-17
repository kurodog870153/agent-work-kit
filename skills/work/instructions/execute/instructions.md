---
name: Task execution and acceptance
description: Applies when carrying out an approved TASK, recording Execution or Attempt results, recording Acceptance, correcting records or recovering transactions; not applicable to requirement or technical decisions.
metadata:
  tags:
    - task-execution
    - acceptance
---

# Task execution and acceptance

1. [Required] Execute only approved TASKs. Treat a missing, contradictory or inferred goal, requirement, acceptance condition or boundary as a specification defect: stop and return it for revision instead of deciding it during execution.
2. [Required] Before starting, check the TASK, its dependencies, required inputs and current project state, and protect user changes. Do not change files that the approved TASK and the user's authorization do not cover.
3. [Required] Obtain a user decision before crossing the authorized goal, scope, side effects or external operations. Do not retry an external operation of unknown outcome blindly; inspect its actual state first.
4. [Required] Record each Execution and Attempt with important changes, results, anomalies, acceptance results and necessary evidence, supporting retries and history. Do not record every command or tool operation, and never store secrets or large irrelevant output.
5. [Required] Keep the closed Attempt unchanged. Correct a record with a Correction, handle an interrupted transaction with Recovery, and never rewrite history or fabricate evidence for work that was not run.
6. [Required] Record TASK Acceptance and Requirement Acceptance separately, each with the conclusion, its source and evidence. Work stores the conclusion from the Host or a designated user and does not judge its truth.
7. [Required] Complete a TASK only when every required acceptance record exists and passed. Completing every TASK never makes the Requirement Acceptance pass by itself.
8. [Required] After a requirement revision, reconfirm the affected acceptance and keep unaffected results.
9. [Required] When an Attempt stops or a specification defect appears, report the actual TASK, Attempt state, evidence and impact, then ask for the next direction one decision at a time.
