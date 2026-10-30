---
name: API compatibility guidance
description: Use when planning backend API contracts or consumer compatibility; not applicable to work that does not affect an API.
metadata:
  tags:
    - api-compatibility
---

# API compatibility guidance

1. [Required] Judge compatibility by whether affected consumers retain promised outcomes, not by server compilation or one responding path.
2. [Required] The TASK must list identifiable direct and indirect consumer chains, contract sources and version or deployment relationships. Favor compatibility and list unknown external consumers as a risk; do not assume they do not exist.
3. [Required] Fix affected paths/topics, methods, media types, parameters, requiredness, names, types, formats, status codes, error structures, fields and schemas in a structural matrix. Unchanged items may be summarized together.
4. [Required] Fix affected defaults, null/absence behavior, sorting, pagination, uniqueness, idempotency, authorization, caching, transactions, errors and timing in a behavior matrix. Identical structure does not imply behavioral compatibility.
5. [Required] Plan breaking changes such as removal, renaming, narrower allowed values, new required fields or changed types or semantics only after the user approves a migration contract. Fix consumers, transition period, dual-path or version boundary, completion condition and rollback outcome.
6. [Default] Prefer compatible extension through optional capabilities when existing consumers can be preserved. Do not retain erroneous or unsafe behavior merely for nominal backward compatibility.
7. [Required] VAL must use existing applicable contract, integration or consumer tests to cover the affected matrices. Without automation, use human-observable criteria and disclose remaining consumer risks.
