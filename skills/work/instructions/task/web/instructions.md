---
name: Web task planning
description: Use to fix Web deliverable contracts, consumer outcomes, compatibility impact and validation boundaries; not applicable to non-Web work.
metadata:
  tags:
    - web
---

# Web task planning

Instruction boundary: This layer confirms user and consumer outcomes across frontend and backend, then fixes the Web deliverable contract. Closer Task layers own backend, security, API compatibility and technical implementation details.

1. [Required] The TASK must first confirm users, consumers, main interactions and observable outcomes, then use project evidence to fix entry points, inputs, success and error outputs, identity boundaries and compatibility outcomes. For nonpublic capabilities, confirm who can and cannot access them and the failure result. For existing consumers, confirm allowed migration outcomes; do not assume no impact without evidence. Do not create an empty contract list when nothing applies.
2. [Required] Fix semantics, format, necessity, boundaries and invalid-input outcomes for every user-controlled path, query, header, Cookie, Body, file or message.
3. [Required] For HTTP, fix method, path, media type, status codes, cache or idempotency semantics and affected consumers. Applicable technical layers add framework annotation and generated-documentation requirements.
4. [Required] VAL must directly accept the fixed Web contract and consumer outcomes; internal-method success alone cannot replace boundary-behavior evidence.
5. [Required] A cross-layer TASK must cover actually affected UI, API, data flow, validation, security, testing and deployment items. Confirm the interface contract before splitting work in explicit dependency order.
6. [Required] List applicable compatibility impact, performance, accessibility outcomes, risks, tests and static checks. Tests and static checks need CMD and objective passing conditions.
7. [Required] Do not include unconfirmed cross-layer requirements in a formal TASK when they affect outcomes, scope, files, steps, validation or deployment operations.
