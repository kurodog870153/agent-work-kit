---
name: Web frontend Tailwind task execution
description: Use to execute and validate TASK-fixed utility-first styles, tokens, class scanning and build requirements; not applicable to non-Tailwind styling work.
metadata:
  tags:
    - utility-first
    - design-tokens
---

# Web frontend Tailwind task execution

Instruction boundary: This layer only executes the formal Tailwind contract; it does not upgrade versions, reset themes, create brand content or add class-tool dependencies.

1. [Required] Before changing styles, verify that actual versions, CSS entry points, integrations, config, content scanning, themes, plugins, prefixes and preflight match the TASK. Do not use syntax from another version.
2. [Required] Compose utilities, variants and responsive states only for formal component responsibilities, using existing tokens and class helpers. Do not create unnecessary abstractions merely to shorten class strings.
3. [Required] Dynamic classes must use complete scannable values or an explicit safelist as fixed by the TASK. Do not use unbounded concatenation or undocumented configuration to prevent output removal.
4. [Required] Limit arbitrary values and global utility, theme or plugin changes to formally listed exceptions. Return to Task if a reusable value lacks a confirmed token.
5. [Required] Run the actual build and verify affected utilities, states, responsive variants and output. When applicable, run lint, tests and representative visual VAL, recording scanning or version limits.
