---
name: Web backend security guidance
description: Use when planning backend trust boundaries, authorization and sensitive-data handling; not applicable without a backend security contract.
metadata:
  tags:
    - backend-security
---

# Web backend security guidance

1. [Required] Trigger security planning by trust boundary, not file type. The TASK must list untrusted sources, protected resources, identity sources, allowed principals, denial outcomes and audit needs.
2. [Default] Deny access by default to new nonpublic endpoints, operations or data. Allow only principals and conditions listed in the TASK; the user must explicitly confirm public capabilities.
3. [Required] Enforce object-level authorization in a trusted backend responsibility layer covering the complete business operation, using the current principal's relationship to the target object. UI hiding, route existence or a client-supplied owner field is insufficient.
4. [Required] Use field allowlists for requests, deserialization and update models. Do not bind unauthorized input directly to persistence or permission fields; expose only approved output fields.
5. [Required] Fix both syntactic and semantic input allowlists, including limits for length, count, depth, size, pagination, sorting, files, regular expressions and other resource use. Dynamic fields, paths, query fragments and sorts must come from backend allowlists.
6. [Required] For cross-site requests, cross-origin access, redirects, files, templates, serialization, cryptography or secrets, fix protections and direct validation from the actual framework and threat. Do not invent cryptographic algorithms or disable existing security controls to pass tests.
7. [Required] Authentication, authorization, denial, sensitive operations and security failures need minimum necessary audit evidence, but must not log passwords, tokens, private keys, full secrets or unnecessary personal data.
8. [Required] Use only existing project security frameworks and tools. Confirm any new tool, dependency or external scan separately. Show the user existing configuration and test evidence that affects a security decision.
