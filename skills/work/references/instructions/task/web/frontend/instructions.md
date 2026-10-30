---
name: Web frontend task planning
description: Use to fix browser pages, components, interactions, accessibility, performance and validation contracts; not applicable to backend-only or nonvisual Web work.
metadata:
  tags:
    - browser-ui
    - client-rendering
---

# Web frontend task planning

Instruction boundary: This layer first confirms browser-observable outcomes and quality boundaries, then fixes a framework-neutral contract. Closer technical layers add language, styling and framework details.

1. [Required] The TASK must first confirm target devices, input methods, browsers, responsive behavior, accessibility and loading/interaction outcomes. Then use current project evidence to fix entry points, routes, page or component boundaries, data sources, states, interactions, errors, empty states, loading and applicable offline outcomes. Do not invent product positioning, audience, content structure, brand voice, marketing sections or UI copy. Confirm each missing content decision that affects the outcome.
2. [Required] Fix each interaction's trigger, keyboard behavior, focus result, state transition, repeated-operation behavior and failure result. Semantic HTML and accessible names must come from confirmed content.
3. [Required] Fix representative viewports, input methods, zoom, overflow and support scope for responsive and cross-browser requirements; one desktop screenshot is insufficient. For an existing UI, confirm allowed and prohibited visual, interaction, route and state compatibility outcomes. Do not assume redesign is allowed without evidence.
4. [Required] Fix source, format, size or loading boundary, alternative-text responsibility and failure behavior for images, fonts, icons and other assets. Do not fetch unauthorized assets ad hoc.
5. [Required] Set measurable build-output, loading, rendering, interaction or asset conditions for performance according to actual impact; subjective impressions cannot replace VAL.
6. [Required] Files, commands, tests and static checks must follow the actual package manager, manifest, lockfile, configuration and project conventions. Confirm any new dependency or tool separately.
7. [Required] VAL must directly cover at least the main browser outcome, critical states, keyboard and accessibility, responsive behavior and actual build. State a human confirmer and explicit criteria for visual conditions that cannot be automated.
8. [Required] When developing frontend components in TypeScript, the TASK must fix affected props, browser events, framework-component type sources and compatibility, and select `programming-language/typescript`.
