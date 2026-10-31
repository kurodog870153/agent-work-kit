---
name: Formal TASK and index records
description: Use when planning a formal TASK collection, acceptance, and source contracts; not applicable to informal TASK records.
metadata:
  tags:
    - task-records
---

# Formal TASK and index records

## 1. Formal collection and source

1. [Required] The formal entry point is `outputs/work/tasks/<requirement-id>/index.json`. The collection contains one canonical `work-task-index` and a canonical `work-task-item` for each reference; a standalone TASK file is invalid. A custom path must match the collection's own `artifacts.task`.
2. [Required] The index stores the requirement, `TASK-SPEC-*`, status, title, summary, `artifacts`, `source`, independently confirmed `hierarchy_selection`, `skill_selection`, requirement-level `acceptance_criteria`, document-level `instruction_selection`, TASK references and `readiness`. Follow the CLI contract description for optional fields and canonical order; do not maintain another JSON structure.
3. [Required] `artifacts` contains only `source`, `task` and `execution`. A normal TASK uses `source.kind: snapshot`; `source.manifest` binds the immutable Source manifest, completion marker and original content. Validate the full triplet with `<work-cli> source read` and `source validate`. Do not rewrite an existing source; a new source requires a new snapshot.
4. [Required] An approved migration TASK without a Source uses `source.kind: migration` and preserves `path`, `raw_sha256`, `size`, `raw` and `approval_sha256` for every reviewed original source. This is not a way to omit a Source for a new requirement. Migration needs explicit approval of provenance and the candidate collection; do not rewrite original historical evidence.
5. [Required] Requirement-level acceptance uses `ACCEPTANCE-*` and a verifiable `criterion`. Each TASK also has nonempty `acceptance_criteria` using `<TASK-ID>-ACCEPTANCE-*`. A TASK's `traceability.acceptance_ids` references only requirement-level acceptance, never child TASK acceptance IDs. At least one TASK must cover each requirement acceptance condition, and VAL must cover both levels.
6. [Required] Document-level `instruction_selection` is the first-occurrence union of each TASK's instruction sources and References. Each item stores its own selection and instructions SHA. The collection's `skill_selection` and each item's `skill_id` express External Skill identity; validate Work instructions and External Skills separately. Only Task and Execute instruction modes are supported.

## 2. TASKs, dependencies and traceability

1. [Required] An item uses `schema: work-task-item`, `id`, `title`, `skill_id`, `instruction_selection`, `traceability`, `acceptance_criteria`, `goal`, `steps` and `validations`. The contract description defines other optional fields. The index does not duplicate item content or dependencies.
2. [Required] Each `TASK-*` is unique in the collection. Within each TASK, `INPUT-*`, `TASK-DECISION-*`, `FILE-*`, `RISK-*`, `STEP-*`, `CMD-*`, `OP-*` and `VAL-*` each start at `001`; shared requirement-level decisions use `DECISION-*`. Never renumber or reuse existing IDs.
3. [Required] Use short IDs for references within a TASK and `<TASK-ID>/<ITEM-ID>` across TASKs. `dependencies` lists direct dependencies only; reject unknown IDs, self-dependencies, cycles and redundant derivable dependencies. Schedule execution in dependency order.
4. [Required] Store a decision affecting two or more TASKs at document level and list its `task_ids`; store a decision affecting one TASK in its item. Document-level data has no prerequisite goal, deliverable or milestone ID from the old source model.

## 3. Inputs, files and steps

1. [Required] `inputs[]` uses `id`, `kind`, `source` and `precondition`. A kind is `task_output`, `project_state`, `user_provided` or `external`. Never store secret values.
2. [Required] `task_output.source` uses `<TASK-ID>/<FILE-ID>`; the source TASK must be an ancestor in the full DAG. Do not add a redundant direct edge for a transitive dependency. A semantic candidate's `dependency_position` still resolves against existing direct-dependency positions. `project_state.source` uses a normalized project-relative path for the prerequisite state, but free-text source or precondition does not grant CMD read access. Other sources store only nonsecret identifiers or descriptions.
3. [Required] A `files[]` action is `create`, `modify` or `move`. The first two use `path`; a move uses `source` and `destination`. Delete is unsupported. Explicitly validate dependencies for cross-TASK file conflicts.
4. [Required] `risks[]` stores `condition`, `impact` and `mitigation`. `steps[]` stores `id`, `action` and nonempty `references`; array order is execution order. A STEP must reference every FILE, CMD, OP and VAL.

## 4. CMD, OP and acceptance VAL

1. [Required] `execution_defaults` and a command's complete `execution` override use `working_directory`, `os` and `shell`. The Host selects an available execution tool for the bound command and observes its own platform limits. Work verifies the reserved command, authorization, declared inputs, file version and complete result evidence; it does not install a native isolation policy or toolchain.
2. [Required] `commands[]` uses `mode: argv` with `argv`, or `mode: shell` with `script`; mode-specific fields are mutually exclusive. `operations[]` describes local_state or external_state side effects and stores action, target and validation_id, adding command_id for command execution.
3. [Required] An automated VAL stores `command_ids`, `pass_condition` and optional `verification`. With no explicit condition, verify that every declared command actually exited with code zero. Explicit conditions support all, any and not, actual nonzero exit codes, complete stdout or stderr, and matching, hashing or JSON Pointer checks of declared reports. Referenced commands must match command_ids; unknown cannot pass. Both ai_review and manual store `confirmer` and `criteria`: the former identifies AI review, the latter actual confirmation by the named user. Do not infer provenance from a name or attach verification. Candidates use command keys; semantic deviations use command positions; formalization preserves existing mappings and does not rewrite JSON assertion values as references. `acceptance_ids` may reference requirement-level or this TASK's child acceptance, never another TASK's child acceptance. Every required condition needs complete evidence from the right source and current version.
4. [Required] The validator checks only the contract; it does not execute CMD or OP. Obtain authorization before execution or a side-effecting precheck and preserve results in Attempt or Correction. A successfully started command does not establish acceptance.
5. [Required] An argv CMD runs in the Host environment with the project's existing tools, dependencies and configuration, preserving real build and test artifacts. Task uses file dependencies and the DAG to order cross-TASK reads and writes; free-text inputs do not grant operation rights. Preview fixes the actual executable or batch launcher, argv, cwd, source, Host environment, exit time and current file versions. Recheck relevant drift; a hash does not replace existing authorization.
6. [Required] The Host and original project manage tools, packages, dynamic libraries and local dependencies. Work does not copy tool snapshots, rewrite dynamic loading, create private signatures or forcibly move artifacts. Preserve evidence of Host rejection, missing tools or actual failures and handle them within the existing scope. Do not install automatically, escalate privileges or bypass limits.

7. [Required] Ordinary development commands such as cargo, mvn and npm use the same argv entry point and generate artifacts under the original project's rules; do not build tool-specific adapter workflows. Do not claim success without actual execution or with an unknown result, and do not rerun automatically to discover a result.

## 5. Canonical paths and fingerprints

1. [Required] Every index reference contains `id`, `path` and `canonical_sha256`. Its relative path is `tasks/TASK-NNN.json`, with digits matching the item ID. Reject absolute paths, empty segments, dot or parent segments, reserved device names, unsafe portable characters and escaping symlinks.
2. [Required] Paths must remain unique after case-folded NFC portable-path normalization. Every direct regular JSON file in `tasks` must be referenced exactly once; missing or extra items are integrity errors.
3. [Required] `task_item_sha256` and `task_index_sha256` bind the exact UTF-8 bytes of the canonical item and index, respectively. Each reference SHA must equal the actual item raw SHA; the index does not store its own SHA.
4. [Required] `task_collection_sha256` is the canonical SHA of the derived `work-task-collection-fingerprint`, including the index SHA and item IDs and SHAs in index order; it is not another formal file. Collection validation reconstructs complete logical TASKs and checks source, instructions, Skills, dependencies, inputs, steps, file conflicts and both acceptance levels.

## 6. Revisions and transactions

1. [Required] The first version is `TASK-SPEC-001` and omits changes. After approval, increment the version once for a substantive canonical specification change. `TASK-CHANGE-*` stores spec_id, ISO date, reason, affected IDs and complete structured edits; do not use change IDs from the removed source model.
2. [Required] Edits use JSON Pointer: add has only after, replace has before and after, and remove has only before. Renaming only the requirement ID and three artifact paths does not increment the version. A change in instruction source identity or order requires a specification revision; a content change to the same source may use instruction audit if the TASK remains valid.
3. [Required] Formal readiness has `status: passed` and a spec_id matching the top level. Keep exploratory evidence before approval in the discussion; it does not authorize Execute.
4. [Required] The first version uses `task preview → apply` to create the TASK collection and Execution index; approval binds the complete candidate, source and revision. The formal index, item targets and requirement Execution directory must not already exist. All initial states are pending; create no Attempt, Correction, lock or audit.
5. [Required] The Execution index uses `work-execution-index` and stores fingerprints and states for the spec, collection, index, items, hierarchy and Skill selections, and document and per-TASK instructions; it does not duplicate TASK or Skill text. States are pending, in_progress, pending_retry, blocked, completed and cancelled; the CLI derives overall_status.
6. [Required] Validate writer lock, spec_update and execution lock boundaries under the transaction contract. File-by-file publication does not claim filesystem atomicity across files. On interruption, retain the journal, marker and published bytes; recover only with the same candidate and approval fingerprint. Do not handwrite formal JSON or overwrite history.
7. [Required] Revise uses `specification prepare → preview → apply/recover`. Before replacing a Source, confirm effects on the overall requirement, primary acceptance and every TASK. Preserve old Source, TASK and Execution transaction evidence. Recompute completion and acceptance states for affected TASKs and downstream TASKs, retaining the latest Attempt, Correction and all history.
8. [Required] Reconciliation explicitly selects retain-only or semantic migration. The preview binds actual Attempt bytes, Execution, ledger and original sources. Apply and recover must recheck the same approval; invalidated acceptance cannot become valid again. Approve migration provenance and candidates without a Source only through the public migration flow.
9. [Required] `task apply` and `task recover` do not execute CMD or OP and do not create an Attempt, execution lock or instruction audit. All publication and recovery remain within user authorization.

## 7. Query the current contract

1. [Required] Use `<work-cli> contract describe work-task-index`, `work-task-item` and `work-task-collection-fingerprint` to query fields, canonical order, constraints, nested references and valid examples.
2. [Required] Source, Execution, Specification, Migration and Reconciliation use their registered CLI contracts. Treat actual contract descriptions and validators as structural authority; do not maintain copied full JSON templates.

## 8. Minimal outcomes and shared-file review

1. [Required] Each Task produces one verifiable outcome. AI reviews and splits independent outcomes. Preserve the reason for an indivisible multi-file outcome in the existing Discussion semantic review; do not split by file, line or step count.
2. [Required] Formalization needs the complete planning structure, current context and decision versions, nonempty semantic-review evidence and a completed review state. Work does not additionally require outcome mapping or a transaction-feasibility declaration. Block formalization when a required goal, step, acceptance condition, decision or review is missing; confirm uncertainties one by one.
3. [Required] Changes to different regions of one file still conflict as writes. Every write-write and read-write conflict needs a valid ancestor order; shared reads need no dependency and receive no independence exemption. Task inputs must express required project sources. Do not add a redundant direct edge when an indirect dependency suffices; Work does not add edges automatically.
