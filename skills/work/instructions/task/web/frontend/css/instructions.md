---
name: Web frontend CSS task planning
description: Use to fix CSS cascade, layout, tokens, responsive behavior, interaction states and style-validation contracts; not applicable to frontend work without style changes.
metadata:
  tags:
    - visual-styling
    - responsive-layout
---

# Web frontend CSS task planning

Instruction boundary: This layer confirms styling and accessibility outcomes, then fixes a framework-neutral CSS contract. Child layers add utility-framework rules.

1. [Required] The TASK must first confirm applicable viewports, zoom, text direction, themes, interaction states, print and cross-browser outcomes. Then use project evidence to confirm existing global, scoped and module styles, CSS-in-JS, preprocessors, resets, cascade layers, selectors and import order. Do not introduce another architecture without evidence.
2. [Required] Fix token sources, inheritance, specificity, states, container or viewport conditions, overflow and behavior with varying text lengths for affected styles. Colors, typography, spacing, breakpoints and content come from confirmed requirements or existing design, not invented brand or business content. For an existing style system, confirm token, layout, selector and visual compatibility boundaries; do not assume global overrides or renaming are allowed. Do not use extensive `!important` rules to hide an unanalyzed cascade.
3. [Required] Fix grid, flex, flow, positioning, size and breakpoint responsibilities for layouts. Cover representative minimum and maximum viewports, zoom and content growth rather than assuming fixed pixels and unchanged content.
4. [Required] Fix focus, hover, active, disabled, invalid, loading, reduced-motion, forced-colors and print styles only when applicable. Do not remove visible focus or convey state by color alone.
5. [Required] When adding or changing animations, transitions, fonts, images or effects, fix fallback, loading, performance and system-preference behavior.
6. [Required] VAL must cover project-supported style lint, build, representative viewports, keyboard focus, contrast, text zoom, reduced motion, applicable system preferences, overflow, themes and supported browsers. Manual visual validation needs explicit comparison criteria.
