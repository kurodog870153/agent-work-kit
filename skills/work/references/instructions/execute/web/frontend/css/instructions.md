---
name: Web frontend CSS task execution
description: Use to execute and validate TASK-fixed CSS cascade, layout, responsive, state and styling requirements; not applicable to frontend work without style changes.
metadata:
  tags:
    - visual-styling
    - responsive-layout
---

# Web frontend CSS task execution

Instruction boundary: This layer only executes the formal CSS contract; it does not redesign, change brand content or introduce another styling architecture.

1. [Required] Before changing styles, verify that actual global, scoped and module styles, preprocessors, cascade layers, tokens, imports and selectors match the TASK. Stop on undocumented global impact.
2. [Required] Change cascade, layout, responsive behavior, states, themes, motion and asset styling only within the formal scope. Do not mask root causes with `!important`, fixed dimensions or wider selectors.
3. [Required] Preserve confirmed behavior for keyboard focus, contrast, zoom, reduced motion, forced colors, content overflow and varying text lengths.
4. [Required] Run the TASK-specified style lint, build, viewport, browser and manual visual VAL. Do not replace explicit conditions with one screenshot or subjective similarity.
5. [Required] If actual tokens, breakpoints, browsers or design sources differ from the TASK, record the difference and return to Task rather than choosing substitute values independently.
