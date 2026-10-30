---
name: TypeScript task execution
description: Use to execute and validate TASK-fixed TypeScript types, modules and compilation requirements; not applicable to plain JavaScript or work without type changes.
metadata:
  tags:
    - static-typing
    - type-safety
---

# TypeScript task execution

Instruction boundary: This layer only executes the TASK-fixed cross-application TypeScript contract; it does not increase strictness, upgrade versions or broaden type refactoring.

1. [Required] Before changing code, verify that the TASK matches the actual tsconfig, manifest, lockfile, module resolution, type sources and check entry points. Return to Task if differences affect the approach.
2. [Required] Preserve type safety at data, event, configuration and external boundaries within the formal scope. Do not hide errors with `any`, ignore comments, non-null assertions or unsafe casts.
3. [Required] Validate external runtime data using the validator, schema or guard specified by the TASK and already present in the project. Static types do not replace runtime validation.
4. [Required] Regenerate or change generated types and declaration files from their formal sources; do not patch generated files manually.
5. [Required] Run the TASK-specified typecheck, build, lint and tests and retain the minimum valid evidence. Explicitly report uncovered public-type or compatibility risks.
