---
name: Web frontend task execution
description: Use to execute and validate TASK-fixed browser UI, interaction, accessibility and performance requirements; not applicable to backend-only or nonvisual Web work.
metadata:
  tags:
    - browser-ui
    - client-rendering
---

# Web frontend task execution

Instruction boundary: This layer only executes the TASK-fixed browser contract; it makes no new content, interaction, design or technology decisions.

1. [Required] Before execution, verify that the TASK fixes entry points, files, states, interactions, content sources, assets, viewports, accessibility, performance and VAL. Stop for a specification defect if inference is needed.
2. [Required] Keep the actual package manager, lockfile, configuration and existing component conventions. Change and validate only according to formal FILE, STEP, CMD, OP and VAL.
3. [Required] Do not fill missing requirements with placeholders, temporary copy, unauthorized assets or unlisted dependencies. Record the smallest gap and return it to Task.
4. [Required] Validation must cover TASK-specified build, static checks, browser states, keyboard, focus, responsive and manual conditions. Accurately record risks from unrun or unobservable checks.
5. [Required] If the actual UI, content, browser result or project capability differs from the TASK, do not accept new behavior or broaden changes independently; stop and return to the specification layer.
6. [Required] When the TASK develops frontend components in TypeScript, preserve props, browser-event and framework-component type contracts within the formal scope and load the TASK-selected `programming-language/typescript`.
