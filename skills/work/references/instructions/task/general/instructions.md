---
name: Task planning
description: Use to turn confirmed requirements into technical decisions, files, steps, commands, operations and validation; not applicable to execution.
metadata:
  tags:
    - task-specification
---

# Task planning

Instruction boundary: Task plans requirements and technology from a fixed Source Snapshot. It is the sole authority for outcomes, acceptance, technical decisions, files, steps, commands, operations and validation in the formal TASK collection; Execute must not supply or infer gaps.

## 1. Fixed source and ownership

1. [Required] A requirement source may be a GitHub Issue fully obtained by the Host, original file bytes or the user's verbatim UTF-8 text. Use a verified Source Snapshot; the core must not refresh external sources itself or parse a PDF in place of the original file.
2. [Required] One normal TASK collection binds one Snapshot. Source identity, original content, hash and size must be complete and verifiable. Recapture creates a new version without rewriting an old Snapshot. List the impact of source replacement on outcomes, technical decisions, TASK boundaries, acceptance and selection and obtain confirmation.
3. [Required] Load a Reference only when a trigger below applies. A formal TASK stores only its globally unique logical name derived from its actual path, never an absolute path.

4. [Required] Task performs hierarchy, Skill discovery/selection and instruction selection from repository evidence; use only user-confirmed selections without drift. Bind each TASK to one `skill_id`, using `null` when no External Skill is needed. Do not combine several External Skills in one TASK.
5. [Required] Task owns initial planning and creation. Send revision of an existing formal TASK to `$work revise` and unverifiable legacy artifacts to migration; do not publish them directly from planning. Original migration evidence may lack a valid Snapshot but cannot serve as a normal TASK source.

## 2. Outcomes, evidence and confirmation

1. [Required] First confirm the objective, scope, constraints, deliverables, inputs, compatibility and observable acceptance. Each outcome needs at least one acceptance condition. Include only the minimum scope needed; do not add candidate improvements before confirmation.
2. [Required] Ask about insufficient information, conflict between the source and repository evidence, or a result that cannot be determined uniquely. Do not invent requirements. Confirm one decision at a time, showing relevant evidence, impact and numbered options.
3. [Required] Determine technology, versions, files, side effects, risks and validation from actual project evidence. Treat user-supplied detail as input to check or a confirmed constraint; do not expand surrounding design. Show evidence and obtain confirmation for judgments affecting formal outcomes.
4. [Required] When a new decision may affect confirmed content, list affected outcomes, acceptance, References, selection and side effects before reconfirming each. State explicitly if there is no impact. After each answer, update the current objective, scope, constraints, outcomes, acceptance and open questions.
5. [Default] Batch read-only exploration within the scope needed for one decision. Reuse unchanged evidence within the same conversation; in a new conversation first check saved sources and current project state. Add dependencies, risks and schedules only when applicable.

## 3. Minimal TASK and formalization

1. [Required] Each `TASK-*` produces one independently acceptable outcome. Split separable outcomes; treat an indivisible multi-file change as one complete change set. Dependency order must give each TASK the needed inputs and executable prerequisites.
2. [Required] Task must fix all files, ordered steps, `CMD-*`, `OP-*`, `VAL-*`, failure handling and authorization boundaries needed by Execute. State dependencies when several TASKs touch one path; file lifecycles must match current project state.
3. [Required] The current formal specification consists of one plain-JSON `work-task-index` and a plain-JSON `work-task-item` for each TASK, with `status: confirmed` and `TASK-SPEC-nnn`. Items live beside the index at `tasks/TASK-NNN.json`; the index does not duplicate item content.
4. [Required] Keys, enums, IDs, statuses, paths, references and hashes are in English; semantic strings may be Traditional Chinese. Work CLI registry/validator defines required and optional fields, nested contract references, canonical key order and reference rules. Do not define synonymous fields or copy the full JSON structure.
5. [Required] Each TASK stores one `goal`, Skill and instruction selections, traceability, ordered `steps` and at least one `VAL-*`. TASK-owned stable identifiers and coverage express acceptance and fully cover the fixed source without depending on another requirement artifact's ordinal. Omit empty optional fields.
6. [Required] TASK hierarchy paths must be a valid subset of confirmed Task selection. Every nonempty path exists in both Task and Execute catalogs. Instruction sources store only `kind`, `logical_name` and `canonical_sha256`, not source layer or absolute path.
7. [Required] Source, formal TASK entry point and Execution default to `outputs/work/sources/<requirement-id>/`, `outputs/work/tasks/<requirement-id>/index.json` and `outputs/work/executions/<requirement-id>/`. For nondefault paths, confirm all three project-relative locations and the same requirement ID. Apply shared path-safety checks for every read and write.
8. [Required] Use a valid portable requirement ID supplied in the current user input or a validated committed Source; do not infer it from filenames or other conversations. Ask the user to supply and confirm a missing ID before first Source capture, then preview and approve capture. Do not defer ID confirmation until the complete candidate. Saving progress or confirming an ID is neither capture nor formal approval.
9. [Required] All TASKs for one requirement share one `work-discussion-session` stored at `outputs/work/discussions/<requirement-id>/session.json` and immutable history. Confirm authorization for scoped saves first, then read committed state each round. Save and validate question mapping before asking. Saving, decision confirmation and completed planning are not formal approval and cannot be handed to Execute.
10. [Required] After all necessary decisions, use `task preview` to validate and show the complete initial candidate. Bind formal approval to `approval_sha256`, then publish the complete collection once with `task apply`; the AI must not manually write formal JSON.

## 4. Validation and continuation

1. [Required] The validator checks Snapshot integrity, requirement coverage, Task-owned hierarchy/Skill/instruction selection, Skill bundle, three artifact paths, one Skill per TASK, Work References, DAG, IDs, file lifecycles and acceptance coverage. Reject writes after approval if the source drifts.
2. [Required] Read plain-JSON requests under the public contract; the shared renderer creates formal path documents. Task does not execute arbitrary CMD or OP. Show read-only evidence of command entry points, syntax, current project state and validation classification in the conversation.
3. [Required] Keep the candidate informal when any determinacy or readiness check fails. TASK index and items do not store execution state, Attempt, lock, validation evidence or runtime instruction audits.
4. [Required] Create or synchronize the Execution index only after formal TASK approval. Task does not create Attempts, execute deliverables or modify existing Attempts. When moving to Execute or returning for revision, identify the committed Source, TASK, Execution and Attempt records and revalidate their current identities and versions. Conversation context alone authorizes no artifact changes.
5. [Required] The completion report lists the TASK spec, each TASK's Skill, changed files, Work References, actual validation results and remaining Execute work. Do not claim unrun checks passed.

## References

| Reference | Load condition |
| --- | --- |
| `references/task-records.md` | Creating, changing or validating formal TASKs, index, state, fields or instruction sources. |
| `references/external-operations.md` | A formal TASK contains an `external_state` OP. |
