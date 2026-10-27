---
name: Task execution
description: Use for preflight, authorized operations, validation, records, state and recovery of a formal TASK; not for requirement or technical decisions.
metadata:
  tags:
    - task-execution
---

# Task execution

Instruction boundary: Execute controls only preflight, Attempt authorization, execution, records, state, locks and recovery for approved TASKs. It may handle execution deviations under user decisions when the requirement, acceptance, deliverable, safety and external side-effect boundaries remain unchanged. It must not make specification decisions, complete a formal TASK specification or repeat Task instructions.

## 1. Loading and Reference routing

1. [Required] Execute only formal, approved `TASK-*` items. Select one External Skill solely from the validated formal TASK collection's `skill_selection` by the target TASK's `skill_id`; `null` means base-only. Do not explore, recommend, add, replace, combine or invoke other Skills.
2. [Required] Load the Execution records Reference below when creating, updating or interpreting an index, Attempt, state, fingerprint or result.
3. [Required] Load the Execution recovery Reference below when finding an existing lock, in-progress record, interruption, partial write, continuation or Correction.

4. [Required] Load an Execute Reference body into the model only when needed. The tool must include every currently triggered and applicable Reference in `EXECUTE-INSTRUCTIONS-SHA-256` and recompute it when References are added or removed dynamically, even if their bodies need not enter model context.
5. [Required] Normal execution loads only the TASK collection baseline, formal index, target TASK item, required dependencies and referenced DECISIONs, Execution index, applicable latest Attempt, continuation chain and Corrections. Load every item or full history only for complete collection validation, audit, conflict or a user request.

## 2. Read-only preflight

1. [Required] Before creating an Attempt, confirm TASK-SPEC; collection, formal index and target item SHAs; Task-owned `hierarchy_selection_sha256` and `skill_selection_sha256`; fixed Source path, raw bytes and SHA with `task_source` binding; and that the Execution index and target TASK's hierarchy, `skill_id`, Skill snapshot and bundle, Execute mode, Work instruction fingerprints, state, dependencies, required inputs, current files, worktree differences, path boundaries and existing locks match the formal TASK.
2. [Required] Treat missing, contradictory, placeholder or inferred TASK goals, requirements, acceptance, deliverables, safety boundaries, external side effects, versions, Swagger or other applicable contracts as specification defects and stop. Only necessary command, validation, inapplicable-record skip or implementation adjustments that preserve these semantic boundaries may be classified as execution deviations under Section 4.
3. [Required] If the target TASK's instruction sources or content differ from its indexed `TASK-INSTRUCTIONS-SHA-256`, do not modify deliverables or write unauthorized records. First report expected and observed values, source differences and impact read-only. Only with explicit user authorization may you create a stopped/instructions-changed Attempt, mark the index blocked and return to Task audit. Otherwise leave state unchanged.
4. [Required] Continue past existing differences only when each path and its content can be explained by the approved TASK, a valid Attempt or Correction, or known earlier output of this run. Stop if a difference may belong to the user or cannot be classified.
5. [Required] Reapply the complete shared instruction-loading path-safety checks for requirement artifacts before every read or write. Do not acquire a lock or write a record if safety cannot be confirmed.
6. [Required] Only a pending or pending_retry TASK can start a new Attempt. For in_progress, resume only the unique Attempt referenced by the index. Confirm the blocker is resolved before proceeding from blocked; never execute completed or cancelled TASKs.
7. [Required] Apart from valid resumption or continuation, stop on a preflight failure when a create path exists, a modify path does not, a move source or destination mismatches, a dependency is incomplete, a required input is absent or a command entry point is unavailable. Do not change the operation type.
8. [Required] Report ordinary preflight failures only in the conversation; do not create an Attempt or change the index. Only the instruction change in Item 3 creates a stopped record under Execution records.
9. [Required] Exclude only the current Execution directory from worktree implementation-difference checks, never all of `outputs/work/`. Temporary output normally produced by an approved CMD and not a deliverable is not an implementation difference.
10. [Required] When an in-progress Attempt's recorded `EXECUTE-INSTRUCTIONS-SHA-256` differs from the currently applicable value, list source differences, executed work and possible impact read-only, then obtain authorization. Close the old Attempt as stopped/instructions-changed under the original hash stored in its lock, synchronize the index and release the old lock; then confirm impact. Only evidence still valid for the current TASK and new Execute instructions may be carried into a separately authorized new Attempt. Never replace instructions or overwrite the hash in the old Attempt.
11. [Required] Before a new Attempt, pass the Work CLI's read-only `execute preflight` for TASK and index identity, state, direct dependencies, Task instruction fingerprints, existing lock, Execute instruction fingerprints, input readiness and the target TASK's live create, modify and move state. Pass a `user_provided` or `external` input as `--confirmed-input <TASK-ID>/<INPUT-ID>` only with existing user or applicable external evidence, never a secret value. Stop immediately on nonzero exit.
12. [Required] After preflight passes, run read-only `execute worktree` with the same parameters. `target_task`, `completed_dependency` and `unrelated` indicate only path intersection, not change ownership. For `review_status: required`, review each item under Items 4 and 9; stop on anything unexplained.

## 3. Authorization and start

1. [Required] The Attempt authorization summary lists the TASK, goal, files, steps, CMDs, OPs, VALs, external operations, expected side effects, risks, validation, normal close conditions, and creation of the Attempt, index update and result recording. Authorization binds the complete summary and current worktree snapshot. Do not ask again for each record reservation, execution, result, routine validation or normal close within that summary.
2. [Required] Original authorization may cover the goal, modules, change types, risks, validation and explicitly allowed added files, test adjustments, known retries and same-cause fixes in the Task collection. Explicit exact-file limits remain binding. Obtain a decision for new goals, side effects or external operations, changes to other Task schedules, unconfirmed worktree drift, unknown results or recovery. Do not bypass separate Host permission requirements.
3. [Required] After authorization, first acquire a single-writer coordination lock pointing to the next Attempt, then create the Attempt and synchronize the index to in_progress. On failure at any step, retain the lock and follow the recovery Reference.
4. [Required] Keep lock, Attempt, deliverable write and index update in a determinable order. Do not rely on operating-system-specific atomic-lock semantics or automatic expiration.

## 4. Execution and local corrections

1. [Required] Perform CMD, OP, VAL and complete changes under the Attempt's original authorization and formally recorded scope adjustments. For necessary added files or same-cause fixes that preserve authorized goal, modules, change types, risks and acceptance without affecting other Task schedules, first read all related content, record the reason and exact versions, recheck dependencies and conflicts, then express complete action, files and impact through the existing execution deviation. Reuse original authorization for an explicitly permitted scope adjustment. Exact-file, reversal and Host limits remain binding; obtain a new decision or return to Task/Revise only when crossing a boundary or changing semantics.
2. [Required] Execution deviations use the existing semantic prepare → preview → deviation-record flow. The formal contract checks source, identity, versions and concrete impact. Do not add Attempt fields or a parallel authorization flow. Continue authorized added files, tests and retries without treating each record or Task transition as another approval point.
3. [Required] Immediately record the minimal actual result of each completed CMD, OP and VAL in execution order. Do not reask for an authorized record with a known result. Do not store secrets, complete external responses or large output unrelated to acceptance.
4. [Required] Apart from command corrections governed by Items 1 and 2, when the cause is known and both correction and revalidation fit wholly within authorized files, operations, side effects and validation, fix it in the same Attempt and rerun validation under the original ID with a sequence suffix.
5. [Required] Stop for an unknown cause, specification defect, out-of-scope file, capability or external operation, conflict with user changes, permission issue or safety issue. Confirm an explicit baseline for user content added between changes; do not retroactively treat unconfirmed drift or unknown failures as reversible.
6. [Required] After each indivisible change set and before the next side effect, inspect the limited diff for unintended deletion, truncation, reordering, encoding, newline or unrelated changes.
7. [Default] Do not retry a failed external operation automatically. Retry only when the TASK and Attempt authorization fixed retry conditions and the current error meets them, within the approved limit. Investigate an uncertain result before resending.
8. [Required] When modifying existing text, preserve unchanged content and the TASK-fixed encoding, BOM and newlines. Before writing a new file, establish the same-type project convention or TASK-fixed format; immediately strict-decode the result and check non-ASCII content.

## 5. Closing

1. [Required] On completion, stop or block, finish the Attempt's final record first, then synchronize latest Attempt, TASK and overall states in one index update and release the lock. If index synchronization fails, retain the lock and enter recovery.
2. [Required] Report actual changed files, CMD/OP/VAL results, tool-derived primary and TASK acceptance states with actual evidence, Attempt and TASK states, partial successes or uncertain outcomes, unresolved items and remaining risks.
3. [Required] Never modify a closed Attempt. Use a separate Correction to correct records; do not rewrite history or fabricate evidence for work not run.
4. [Required] At close, collect accepted execution deviations and let the user decide once whether to write back all, selected ones or none beyond execution evidence. Write back through the shared formal-document revision process without changing the closed Attempt. Classify specification defects as `task_only` or `task_and_execution` by impact. The handoff must include the actual requirement ID, TASK and Execution paths, fixed Source identity, target TASK, Attempt and preflight information, confirmed approach, affected IDs and validation requirements, without placeholders or later inference.
5. [Required] After an Attempt stops or blocks, or preflight finds a specification defect, ask the user for the next direction one decision at a time; stop asking when the user explicitly declines further work. Return specification changes confirmed in the same conversation to the parent agent for the private artifact editor; the original Execute neither edits the spec nor releases the lock itself. Across conversations, hand off specification defects only with `$work task -- <complete requirement>`, and handle confirmed formal revisions through `$work revise`. When no specification change is needed, state only the reason, release conditions and next Execute authorization; do not produce a specification handoff command.
6. [Required] A specification handoff uses `execute_to_task` and includes the fixed marker, requirement ID, actual paths, TASK spec, target TASK, `skill_id`, `execute_skill_selection_sha256`, TASK and instruction SHAs, Attempt or preflight information, confirmed approach, requested changes, preservation scope, affected IDs and validation requirements. Do not store target hierarchy.
7. [Required] Put the handoff in one JSON code block in the conversation only. Execute must not modify Source or TASK or create a specification lock. Do not create another Attempt until the corresponding Task/Revise flow or authorized private artifact-editor transaction is complete and Execute checks pass again. The handoff itself authorizes no write.

## References

| Reference | Load condition |
| --- | --- |
| `references/execution-records.md` | Creating, updating or interpreting an index, Attempt, state, fingerprint or result. |
| `references/execution-recovery.md` | Finding an existing lock, in-progress record, interruption, partial write, continuation or Correction. |
