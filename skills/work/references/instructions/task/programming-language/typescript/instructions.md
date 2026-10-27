---
name: TypeScript task planning
description: Use to fix TypeScript types, compiler settings, module interfaces and static-validation contracts; not applicable to plain JavaScript or work without type changes.
metadata:
  tags:
    - static-typing
    - type-safety
---

# TypeScript task planning

Instruction boundary: This layer confirms cross-application type-safety and compatibility outcomes and fixes the TypeScript contract; other applicable paths own application and framework requirements.

1. [Required] The TASK must use the actual manifest, lockfile, tsconfig, build settings and source to confirm TypeScript version, module resolution, JSX, path aliases, strictness and typecheck entry points. Do not upgrade or rebuild configuration independently.
2. [Required] First confirm acceptable type-safety outcomes for affected data, events, functions, settings, environment variables and external-module boundaries. Then fix type sources, nullability, narrowing, errors and compatibility behavior. Explicitly confirm gradual JavaScript/TypeScript adoption, conversion or strictness changes; do not assume full conversion or stricter settings.
3. [Required] Do not trust external or runtime data through assertions alone. Use an existing validator, schema or explicit guard; list a missing approach as a decision to confirm.
4. [Default] Prefer inferable types, discriminated unions, `unknown` and precise interfaces. Do not spread `any`, non-null assertions, ignore comments or unsafe casts merely to pass checks.
5. [Required] Fix sources and update methods for generated types, ambient declarations, package types and framework types. Do not edit generated files manually.
6. [Required] VAL must use the project's actual typecheck and build entry points and cover lint, tests, unused code and public-type compatibility as impact requires. An editor with no red squiggles is insufficient evidence.
