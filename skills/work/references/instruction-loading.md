# Work Instruction Loading Bootstrap

1. Activate from the Host-discovered Work Skill root. The Host identifies Task, Execute, Revise or Migration from the user's request, loads only applicable Work instructions and decides whether to use native Subagents or tools. Skill activation does not approve execution, publication or a change of scope.
2. Before a formal operation, use Work CLI to validate current Source, TASK, Instruction Selection, fingerprint and legal state transition. `workflow status` supplies formal state information but cannot dispatch an Agent, choose a prompt or grant Host authority. The Host coordinates Task, Execute and Revise.
3. Before first Source capture, use an explicit portable requirement ID and preserve the exact supplied requirement bytes. Do not invent a `TASK-*` identity before formal TASK publication. A new conversation resumes from committed Source, DiscussionSession, TASK, Execution and Attempt records after current-state validation; unrecorded semantic decisions remain unknown.
4. Keep user decisions, Host permissions and Work formal approvals distinct. Stop a dependent formal operation on Source or Instruction drift, invalid TASK or Attempt, missing approval, scope mismatch or uncertain transaction state. Formal Artifact locks, journals, publication and recovery remain Work CLI responsibilities.
5. Load only modules whose conditions below hold. Each route resolves to one regular file directly under `references/instruction-loading/` after realpath validation; reject duplicate, missing or escaped paths. Modules cannot route other modules. Source identities, raw SHA, applicable fingerprints, ancestors and References must be revalidated when affected.
6. Tooling is an independent optional Skill. The Host may activate it for an operation requiring its Policy. Work does not require Tooling for unrelated operations and does not add Tooling sources to formal `required_instruction_sources`, `source_order`, Selection Manifest or TASK fingerprint. Use `tool-policy-integration.md` only to check the separation of the two formal boundaries.

## Modules

| Module | Load condition |
| --- | --- |
| `runtime.md` | Resolving the installed Work binary, Skill root or runtime preflight. |
| `cli-transport.md` | Creating a CLI request file or invoking a CLI command with file transport. |
| `cli-response.md` | Reading, validating or retaining a CLI response. |
| `transaction-workspace.md` | Allocating or using a Work transaction workspace. |
| `source-loading.md` | Loading or validating Work formal Instruction or Source content. |
| `routed-references.md` | A selected hierarchy instruction declares an applicable Reference. |
| `fingerprints.md` | Computing or validating Source, Instruction, Selection or Artifact hashes. |
| `skill-selection.md` | Validating formal TASK Skill selection or provenance. |
| `discussion-progress.md` | Reading or updating Task discussion and acceptance progress. |
| `formal-task-validation.md` | Validating or relying on a formal TASK collection. |
| `artifact-paths.md` | Resolving or validating formal Work artifact paths or requirement IDs. |
| `artifact-revision.md` | Revising an independently valid formal TASK collection. |
| `artifact-migration.md` | Migration mode or reviewed reconstruction of incompatible historical evidence. |
| `instruction-maintenance.md` | Creating or changing Work instructions or references. |
| `tool-policy-integration.md` | A Work operation also uses independently activated Tooling Policy. |
