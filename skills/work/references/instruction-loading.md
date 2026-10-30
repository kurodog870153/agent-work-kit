# Work Instruction Loading Bootstrap

1. Validate the explicit invocation with `invocation parse`; for an ordinary request, obtain confirmation of the exact mode and request before `invocation confirm`. Follow the `invocation.md` route below; retain the original input, complete CLI response and origin. Activation does not imply confirmation or authorization.
2. For Task, Revise and Execute, call `workflow status` or `workflow next`, require VALID routing and verify the canonical selection manifest fingerprint. Load only `required_instruction_sources` in `source_order` for the current `next_action`; do not scan or preload instruction, workflow or subagent trees. Revise uses the internal Specification workflow. Migration uses its separate reviewed commands and does not enter ordinary lifecycle routing.
3. Before first Source capture, use a portable requirement ID explicitly supplied by the user or formal handoff; ask for confirmation if absent. Task captures the exact requirement payload once as immutable Source, then independently confirms hierarchy, ordinary External Skill selection and main acceptance before planning. Reuse that fixed Source binding.
4. Re-run routing when operation, state, lifecycle, selection, authorization, role or formal events change. Stop dependent operations on `REVIEW_REQUIRED`, source/state drift, missing sources, insufficient authorization or incomplete envelopes. Never reuse a failed or recovery context.
5. Keep user decisions and authorization in the main flow. A routed worker receives only a fingerprint-bound operation envelope and verified sources; it cannot expand scope or approval. Authorization, safety, transaction, recovery, source validation and artifact integrity gates remain mandatory even when a detailed module is not needed for a read-only operation.
6. Load only the modules whose conditions below hold. A module name resolves to exactly one regular file directly under `references/instruction-loading/` after realpath validation; reject duplicate, missing or escaped module paths. A module cannot route another module. Report invalid routing before its dependent operation.
7. When creating or changing Work instructions or references, use `--instruction-maintenance` with `workflow status` or `workflow next` so the shared maintenance source is selected in any mode.
8. When a concrete Work operation needs a Host-provided tool, use the controlled Runtime Tool Policy channel in `tool-policy-integration.md` after Work routing and authorization checks. Discover Tooling through the Host, activate its own Bootstrap on demand and keep its policy sources separate from Work's formal `required_instruction_sources`, `source_order`, Selection Manifest and TASK fingerprint. A required unavailable or invalid policy blocks that operation; it does not rewrite formal Work sources.

## Modules

| Module | Load condition |
| --- | --- |
| `invocation.md` | Parsing an explicit Work invocation or confirming a proposed implicit invocation. |
| `runtime.md` | Resolving the installed Work binary, Skill root or runtime preflight. |
| `cli-transport.md` | Creating a CLI request file or invoking a CLI command with file transport. |
| `cli-response.md` | Reading, validating or retaining a CLI response. |
| `transaction-workspace.md` | Allocating or using a Work transaction workspace. |
| `source-loading.md` | Loading, validating or refreshing routed Workflow, Instruction or Source content. |
| `routed-references.md` | A selected hierarchy instruction declares an applicable Reference. |
| `fingerprints.md` | Computing or validating Source, Instruction, Selection or Artifact hashes. |
| `skill-discovery.md` | Discovering or recommending ordinary External Skills for Task. |
| `skill-selection.md` | Building, validating or using confirmed ordinary External Skill selection. |
| `discussion-progress.md` | Reading or updating Task discussion and acceptance progress. |
| `formal-task-validation.md` | Validating or relying on a formal TASK collection. |
| `artifact-paths.md` | Resolving or validating formal Work artifact paths or requirement IDs. |
| `artifact-revision.md` | Revising an independently valid formal TASK collection. |
| `artifact-migration.md` | Migration mode or reviewed reconstruction of incompatible historical evidence. |
| `handoff.md` | Receiving or preparing a formal handoff. |
| `internal-envelope.md` | Building or validating a private role operation envelope. |
| `private-roles.md` | Delegating to or receiving results from a private Work role. |
| `instruction-maintenance.md` | Creating or changing Work instructions or references. |
| `tool-policy-integration.md` | A routed Work operation needs Host-provided tool policy or an Integration Gate check. |
