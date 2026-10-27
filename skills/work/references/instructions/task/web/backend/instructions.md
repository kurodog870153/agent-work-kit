---
name: Web backend task planning
description: Use to fix backend architecture, entry points, data, security, resilience, external operations and validation contracts; not applicable to frontend-only work.
metadata:
  tags:
    - backend
    - server-side
---

# Web backend task planning

Instruction boundary: This layer confirms backend business outcomes, data responsibilities and observable failures, then fixes common technical contracts from project evidence. Closer Task layers own Java, JPA, MyBatis and other technology-specific rules; Execute only performs settled work.

## 1. Reference routing

1. [Required] Load the Web backend References below when their TASK conditions apply; one TASK may match several rows.
2. [Required] Derive selected Reference logical names from their actual paths; do not load unselected Reference bodies.
3. [Required] Re-evaluate the actually applicable Reference set and fingerprint when conditions change.
4. [Required] One trigger may load several References. The formal TASK's `instruction_selection.references` records applicable names without copying Reference bodies.

## 2. Project evidence and architecture

1. [Required] Before TASK approval, read-only check the target module, existing architecture, public boundaries, actual framework/version, data responsibilities, error handling, validation entry points and affected consumers. The user must confirm evidence that affects decisions.
2. [Required] Follow existing project responsibility boundaries; do not assume a four-layer Controller/Service/Persistence Service/Repository architecture. A new layer, cross-layer access or architecture migration needs a confirmed reason, scope and validation.
3. [Required] Record version conclusions in the TASK with official sources or project-locked data. Instructions do not hold a version matrix; Execute must not upgrade, downgrade or add dependencies independently.
4. [Required] Use only existing applicable project build, test, static-analysis and security tools. A new dependency, service, plugin or tool must be confirmed TASK content.
5. [Required] Before approval, check every path to create, modify or move and whether classes, methods, settings and tests cited by steps already exist or will be created by this TASK before use. Use adjacent comparable implementation in the target module to confirm package/directory, naming, layering and test placement. Check each referenced symbol's location, signature, visibility and responsibility. A new path consistent with existing structure may enter the TASK; incorrect or conflicting path, symbol and responsibility assumptions must not be approved or left for Execute to fix.

## 3. Backend contract

1. [Required] For each entry point, first confirm business outcomes, data lifecycle, post-completion state, trust boundaries and consumer input/output semantics. Then use project evidence to fix inputs, outputs, errors, authorization, transaction or consistency boundaries, idempotency, resource limits and observable results. Omit inapplicable items; do not insert vague placeholders. Do not infer schema, database columns or HTTP technical fields directly from outcome constraints.
2. [Required] Fix exception and error-response categories, external information, retention of internal causes and masking of sensitive data. Do not expose stack traces, SQL, secrets or internal identifiers directly to consumers.
3. [Required] For data changes or external operations, first confirm atomicity, acceptable partial success, uncertain outcomes and observable remediation results; then apply `task.general.external-operations`. Confirm allowed and prohibited compatibility and migration outcomes for existing data or integrations. Proactively present evidence, risks and side effects and obtain user authorization for database-specific or real external-environment validation.
4. [Required] The TASK must fix the minimum logs, metrics or traces needed to observe the outcome and how they handle sensitive data. Do not introduce a new observability service merely to apply instructions.
5. [Required] VAL must cover success, invalid input, permission, failure, consistency, compatibility and resource limits at affected boundaries. List environment-specific behavior that cannot be verified as remaining risk.
6. [Required] For Web binding, validation or error responses, trace the actual `request → binding/validation → exception type → handler → HTTP status and body` path. When the contract changes, include necessary handlers and tests in the TASK; do not infer results from similar names.
7. [Required] Check Controller parameters and validation annotations, advice/handlers, exception inheritance and matching conditions, and existing tests directly covering that path. HTTP status, error code and response shape must match actual handling. If the requirement changes behavior, list necessary files and plan Red and Green VAL that distinguish old and new behavior. Keep the TASK as a draft if handler selection or response cannot be confirmed read-only.

## 4. Red/Green and external validation

1. [Default] Use Red/Green for automatable behavior changes. Red and Green have separate VAL and may share CMD; applicable technical instructions decide whether to Refactor after Green. The first TASK needing a test must include that test and the minimum compilable skeleton needed to reach it. Tests may come only from the current baseline, explicitly dependent earlier TASKs or the current TASK, never be backfilled by a later TASK.
2. [Required] Red passes only when the specified test is reached and fails in the one expected way because the target behavior is not implemented. Production/test compilation, framework startup, fixture, setup or unrelated test failures are not valid Red.
3. [Required] All applicable validation must pass when the TASK completes. Do not exclude, disable, replace or postpone required tests. Applicable technical instructions decide whether to Refactor after Green.
4. [Required] Connected validation against relational databases, Redis, messaging systems or external APIs is a completion condition only when expressly authorized by the TASK or user. Without authorization, use existing offline validation and list unconfirmed external behavior and risks. Do not call a mock or successful build external evidence.
5. [Required] Red must fix the target test class, project-relative path, module and source. Classes, interfaces, methods, error codes and settings referenced by the target test must come from the baseline, an earlier TASK or a minimum compilable skeleton created by this TASK before the test and lacking target behavior.
6. [Required] When this TASK adds the target test, its steps create all minimum compilable scaffolding, then the test, then run Red CMD. A baseline command run before the test exists or compiles cannot count as Red.
7. [Required] Production code, test code, generated code, settings and resources compiled, loaded or initialized before Red CMD reaches the target test form the prerequisite closure. Trace all affected references from additions, removals, renames or signature/contract changes across the actual module, reactor, project and test scope; update them or provide compatible scaffolding before Red.
8. [Required] Red's passing condition must list stages that succeed before reaching the target test and the expected failing test, assertion or behavior. Compilation, framework startup, fixture/setup or unrelated test failures are not Red. Keep the TASK as a draft if any source, prerequisite, file operation or order cannot be confirmed.
9. [Required] Put reproducible relevant tests in the implementation TASK. Create a separate validation TASK with explicit prerequisites for full regression that needs particular local files. List only applicable tests, and explain any high-impact test omission whose reason is not obvious.

## References

| Reference | Load condition |
| --- | --- |
| `references/security.md` | The TASK adds or changes trust boundaries, identity, permissions, sensitive data or untrusted input. |
| `references/api-compatibility.md` | The TASK adds, changes or removes a consumer-visible API, event, data or error contract. |
| `references/resilience.md` | The TASK involves remote calls, timeouts, retries, queues, scheduling, batches, concurrency, resource pools, health or degradation. |
