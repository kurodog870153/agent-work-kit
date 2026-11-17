---
name: Web frontend Astro task execution
description: Use to execute and validate TASK-fixed Astro pages, components, rendering, islands and build requirements; not applicable to non-Astro frontend work.
metadata:
  tags:
    - islands-architecture
    - static-rendering
---

# Web frontend Astro task execution

Instruction boundary: This layer only executes the formal Astro contract; it makes no new page-content, route, framework-integration, adapter or hydration decisions.

1. [Required] Before changing code, verify that the actual version, Astro config, adapter, integrations, output mode, directories, routes and commands match the TASK. Return to Task on any contract-affecting difference.
2. [Required] Change only the formally listed pages, layouts, Astro components, framework components, content sources and assets. Preserve server-only, client-bundle and environment-variable boundaries.
3. [Required] Implement client directives, prerendering, dynamic paths and data fetching exactly as the TASK specifies. Do not add hydration, change output mode or move an execution boundary to suppress an error.
4. [Required] Do not fill gaps with placeholders, temporary metadata, unconfirmed content or unauthorized integrations. Stop when the specification is insufficient.
5. [Required] Run the formal typecheck or Astro check, build and route validation. Where applicable, inspect hydration, output files, 404 behavior, assets and adapter results and record evidence.
