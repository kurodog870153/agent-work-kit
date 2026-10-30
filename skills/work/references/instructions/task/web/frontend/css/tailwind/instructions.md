---
name: Web frontend Tailwind task planning
description: Use to fix utility-first styles, theme tokens, class scanning, composition and build validation; not applicable to non-Tailwind styling work.
metadata:
  tags:
    - utility-first
    - design-tokens
---

# Web frontend Tailwind task planning

Instruction boundary: This layer only fixes the Tailwind-specific contract; it does not create brand, content, sections or UI copy.

1. [Required] The TASK must use the manifest, lockfile, CSS entry, PostCSS or bundler integration, config and existing code to confirm actual version capabilities and configuration. Do not apply conventions from another version or upgrade independently.
2. [Required] Keep themes, tokens, variants, plugins, prefixes, `important`, dark mode and preflight aligned with actual project sources. Fix impact scope and compatibility validation for new or changed global behavior.
3. [Required] Utility composition must preserve readable component responsibilities and state variants. Extract classes, components or helpers only when repetition, conditional composition or project convention requires it; do not create unused abstractions.
4. [Required] Dynamic classes must be discoverable by actual content scanning or an explicit safelist. Do not produce removable utilities through unbounded string concatenation.
5. [Required] Use arbitrary values only when existing tokens cannot express a confirmed requirement correctly. Manage reusable design values through the project's existing token mechanism; do not scatter magic values.
6. [Required] Include class-merge, variant-helper or formatting tools only if the project already uses them or they are separately confirmed. Do not add dependencies merely to organize classes.
7. [Required] VAL must run the actual build and show that affected utilities, variants, responsive and state classes are present in output without scanning omissions. Run applicable lint, tests and representative visual validation as well.
