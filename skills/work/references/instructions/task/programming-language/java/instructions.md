---
name: Java task planning
description: Use to fix Java version, build, type, implementation and testing decisions; not applicable to non-Java work.
metadata:
  tags:
    - java
---

# Java task planning

Instruction boundary: This layer confirms outcome constraints imposed by the Java platform and fixes common technical decisions. Applicable paths own application, Spring and persistence details.

## 1. Reference routing

1. [Required] Evaluate the Swagger/OpenAPI Reference below. If either prerequisite is absent, the AI must not add dependencies, configuration, generators or validation tools, and normal Controller changes must not be blocked for that reason.

## 2. Platform and project conventions

1. [Required] Before TASK approval, confirm outcomes and acceptance required by Java compatibility, deployment platform, existing consumers and confirmed module/architecture constraints. Then use project evidence to confirm the target module, actual JDK, compilation target, applicable framework/dependency versions, build tool, test entry points, code style and affected chain. Record version evidence and user confirmations in the TASK. Do not infer framework, persistence layer, build tool or database product from platform constraints.
2. [Required] Source, tests, settings and dependencies must follow existing architecture, naming, formatting and responsibility boundaries. Do not proactively refactor untouched content or add a layer merely to apply instructions.
3. [Default] Follow the target module's Lombok style. Restrict a new use or use explicit code only when it would hide mutability, lifecycle, framework construction, equality or serialization risk. Do not maintain a fixed version matrix.
4. [Default] Decide whether to use `var` for local variables from project style, target JDK and type readability on that line. Do not replace type semantics in public signatures or fields with `var`, or rewrite untouched code proactively.
5. [Required] Fix Maven, Gradle or other build operations from the project entry point and validation purpose. Include clean only when old outputs must be excluded; do not add `clean` as a fixed prerequisite for every validation.
6. [Required] The TASK must confirm the actual Wrapper or system executable, working directory, module/project path, goal/task, arguments and test filters. Do not assume generic `test`, `check` or `build` exists. Generating or upgrading a Wrapper, Gradle, plugin, JDK/toolchain, or changing build DSL/configuration requires separately listed files, versions, steps, CMD, VAL and authorization; do not leave it for Execute to fix incidentally.
7. [Required] When using a project generator, fix its version, all arguments and necessary build settings, then check created paths against the current actual response or output list. Do not infer artifacts from historical templates.
8. [Required] Keep the target module's one existing applicable build system; do not switch Maven/Gradle or add a second configuration for the same change. Submodules need not carry their own Wrapper. Maven may use an applicable Wrapper or a TASK-preflighted system executable. Use the Gradle Wrapper when applicable; otherwise use only the system Gradle version fixed by the TASK.
9. [Required] The TASK must fix target shell, working directory, Wrapper root or system executable, Maven module or Gradle project path, build DSL, actual goal/task, arguments/properties, test filters, Java/JDK/toolchain and test locations. Use an exact executable, `JAVA_HOME` or toolchain when fixed; fall back to PATH resolution only when the TASK explicitly specifies it. Execute must not supply or substitute these. Preflight that may initialize, download or write still needs prior authorization.
10. [Required] In a Maven reactor test command, add `failIfNoSpecifiedTests=false` only when `-am` is used and upstream modules without the specified test will actually run. Do not add it without `-am` or when upstream modules will not run. Confirm Gradle tasks from existing configuration, actual definitions or authorized preflight; do not assume generic tasks exist.

## 3. Version baseline

1. [Required] When the TASK adds or changes Java/applicable framework, compilation release, Wrapper, Maven/Gradle, dependencies/plugins, JUnit, Surefire/Failsafe, or compatibility is at issue, fix one compatible combination from official first-party sources before approval. Other TASKs retain existing fixed versions that can be confirmed read-only, without a new version baseline.
2. [Required] Use exact applicable versions or values uniquely derived from an exact parent, BOM or Wrapper; do not use `latest`, ranges or candidates. Keep a draft when sources are insufficient, conflicting or leave candidates. Search results or `BUILD SUCCESS` alone are not compatibility evidence.
3. [Required] Record a version baseline affecting one TASK in that TASK's `Decisions`; create a document-level `DECISION-*` when it affects at least two TASKs. Do not add a parallel TASK field. Within the decision, use `Component: <name>; Version: <exact or uniquely derived value>; Source: <source>` for each component, plus `Official compatibility source: <reproducible location>; Supported conclusion: <conclusion>` for compatibility evidence. Do not compress it into an uncheckable line, create a second version set or retain candidates.

## 4. Java contract and implementation

1. [Required] Type, null, exception, generic, serialization and concurrency contracts must be observable and consistent at affected boundaries. Do not evade contracts with unchecked casts, raw types or swallowed exceptions.
2. [Required] Declare Collections with the highest-level interface expressing all necessary operations and semantics: `Iterable<T>` for iteration alone, `Collection<T>` for general collection operations, `Set<T>` for uniqueness, `List<T>` for indexing or `List#set`, and `Map<K, V>` for general key/value contracts. Narrow the abstraction for ordering, sorting, concurrency or framework constraints only when actually necessary; do not rely on incidental concrete-implementation behavior.
3. [Required] Prioritize compatibility for collection types at existing public API or framework boundaries. Apply the relevant API-compatibility Reference before a type change; do not broaden or narrow proactively when external callers are unknown.
4. [Required] Choose `java.time` types from business meaning and fix time zone, format, time source and boundary conversion. Date-only values, zone-free date-times, instants and offset times are not interchangeable.
5. [Required] Resources, Streams, threads, Futures and transactions need explicit owners and closure or cancellation outcomes. Do not create unbounded concurrency or put blocking work on an unconfirmed shared executor.
6. [Required] Before approval, confirm Collection/Map interface hierarchies available in the target JDK and check affected instance methods, parameters, assignments, returns, generics, overloads, static methods and framework entry points. Do not use interfaces unavailable in that JDK.
7. [Required] When the normal flow needs a lower-level method or semantics, declare the highest-level interface that satisfies both operation and contract directly; do not declare a higher interface and downcast later. Use a concrete class or cast only when no interface expresses the necessary contract or a framework/external API explicitly requires it; fix reason, minimum scope, type check, safe failure and direct tests.
8. [Required] Favor compatibility when existing public APIs, external callers or serialization boundaries cannot be fully confirmed. Fix required semantics across declared interfaces, implementation classes and direct tests. VAL covers affected declarations and uses, static analysis, compilation and relevant tests. Do not scan or refactor untouched flows for this purpose.
9. [Required] Use `LocalDate` for date-only values without time or zone, `LocalDateTime` for date-time without zone, and the matching `java.time` type for time-only, instant, offset or zone. Fix the time source when current time is needed. At serialization, API, JDBC, database, external-system or framework boundaries, also fix format, target type, conversion and round-trip validation.

## 5. Lombok risk boundary

1. [Default] Use Lombok only when the project already has it with working annotation processing or the TASK separately approves new settings. To introduce it in a project that did not use it, the TASK must fix reason, exact version or unique source, build settings, impact scope and CMD/VAL; Execute must not add or adjust it independently. Do not use it if either existing `lombok.config` or project convention prohibits it.
2. [Required] The Lombok allowlist in production and test code is only `@Getter`, `@Setter`, `@RequiredArgsConstructor`, `@NoArgsConstructor`, `@AllArgsConstructor`, `@Value`, `@Builder`, `@Builder.Default`, `@Singular`, `@With`, `@EqualsAndHashCode`, `@ToString`, `@Slf4j` and `@Jacksonized`. Other annotations are prohibited by default; the TASK must fix the annotation, target, complete generated behavior, reason and direct validation for an exception.
3. [Required] `@Data`, `@NonNull`, class-level `@Setter` and `@NoArgsConstructor(force = true)` are prohibited by default. The TASK must separately approve an exception's complete generated behavior and object validity. Entity equality/toString and class-level Builder also trigger risk review; use explicit code until approval is complete.
4. [Required] Treat Lombok-generated public/protected methods and constructors as formal APIs. Assess source and binary compatibility for cross-module or public classes; do not proactively rewrite untouched uses.
5. [Required] Before class-level `@Getter`, confirm every getter should be public. Use `@Setter` only on individual fields needing mutability. Use `@RequiredArgsConstructor` only for required dependencies or immutable fields, `@NoArgsConstructor` only when a framework requires it and at minimum visibility, and `@AllArgsConstructor` only for DTOs/value objects whose full set of fields forms a constructor contract.
6. [Required] Use `@Value` only for value objects/DTOs needing immutability, all-field equality, all-field `toString` and final-class semantics; consider `record` first when applicable. Use `@With` only for immutable objects and validate the replacement contract and equality.
7. [Required] Prefer `@Builder` on an explicit constructor or static factory. A class-level exception must fix all inputs, visibility, defaults and validation. Use `@Builder.Default` and `@Singular` only after Builder approval, validating unset, single, multiple and empty-collection behavior. Use `@Jacksonized` only when the project actually uses Jackson, Builder is approved, and serialization contracts fit the actual version. If existing versions/settings conflict and the TASK does not approve adjustment, use explicit Jackson annotations.
8. [Required] For `@EqualsAndHashCode`, fix fields, inclusion and `callSuper`; Entity exceptions must validate IDs, transient state and proxies. Use a field allowlist for `@ToString`, excluding secrets, personal data, Entity relationships, lazy fields and cycles. Use `@Slf4j` only in projects already using SLF4J; do not add or replace logging dependencies for an annotation. Approve other Lombok logging annotations individually.
9. [Required] Entities allow only reviewed getters, minimum-visibility framework-required no-argument constructors and necessary field-level setters; approve other Lombok behavior individually. New code and existing Lombok use touched by the TASK must follow this section; do not proactively refactor untouched code. Keep a draft and reconfirm if compliance requires wider file or outcome scope. When constructors, Builders, equality, toString or serialization are affected, VAL must directly cover generated behavior in addition to compilation and relevant tests.

## 6. Testing and validation

1. [Default] Use Red/Green for added or changed testable behavior: first obtain evidence of failure due to missing target behavior, then pass with the smallest implementation. An exception needs concrete evidence that existing coverage directly proves the gap, the change is only specification/generated code, or a tool limitation applies.
2. [Recommended] Refactor after Green only for observable duplication, naming or structural problems; do not broaden changes to complete a ritual cycle.
3. [Required] Choose existing unit, slice, integration, contract, static-analysis and compilation entry points for VAL according to actual impact. Do not add a framework/dependency independently or claim unrun tests passed.
4. [Required] When version-, framework-, database- or environment-specific behavior cannot be directly validated, record the validated layer, unverified items and remaining risks.
5. [Required] Test VAL must confirm the target module and specified tests actually ran and the count exceeded zero. `BUILD SUCCESS` alone, zero tests or compilation alone is not behavioral pass evidence.

## References

| Reference | Load condition |
| --- | --- |
| `references/swagger.md` | The TASK adds or changes a Controller, Endpoint, HTTP parameter, Request Body, Response, DTO, OpenAPI or Swagger generation source, and read-only evidence confirms the target project already has a Swagger/OpenAPI dependency and valid configuration. |
