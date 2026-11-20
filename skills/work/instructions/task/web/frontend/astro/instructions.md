---
name: Web frontend Astro task planning
description: Use to fix Astro pages, components, rendering modes, islands, integrations and build validation; not applicable to non-Astro frontend work.
metadata:
  tags:
    - islands-architecture
    - static-rendering
---

# Web frontend Astro task planning

Instruction boundary: This layer only fixes the Astro-specific implementation contract; it does not decide product content, page sections or UI copy.

1. [Required] The TASK must use the actual manifest, lockfile, Astro config, adapter, integrations and directories to confirm version capabilities, output mode, routing conventions and available commands. Do not upgrade or switch adapters independently.
2. [Required] Fix file, input, output, data-fetching and error boundaries for every affected route, layout, page, Astro component, framework component and content source.
3. [Required] Fix static, server, hybrid, prerender and dynamic-route behavior according to the existing deployment contract and requirements; do not change site-wide output mode for convenience.
4. [Required] Use client directives only for confirmed browser-side interactions, fixing the framework, hydration timing, fallback and JavaScript cost. Do not hydrate static content without a reason.
5. [Required] Keep head metadata, assets, image handling, environment variables, integrations and server-only code within existing Astro boundaries. Do not send secrets into the client bundle.
6. [Required] When dynamic paths, content collections or data sources apply, fix schema, missing-data behavior, build-time and request-time behavior and representative test data.
7. [Required] VAL must run at least the actual typecheck or Astro check, build and affected-route validation. When applicable, cover hydration, output files, adapter, 404 behavior, assets and deployment compatibility.
