---
name: Java Spring Boot task planning
description: Use to fix Spring Boot or Spring MVC-specific contracts; not applicable to Java work without that framework.
metadata:
  tags:
    - spring
    - spring-boot
---

# Java Spring Boot task planning

Instruction boundary: This layer only fixes Spring Boot and Spring MVC-specific contracts and inherits common Java rules.

## 1. Spring Web contract

1. [Default] Use `Collection<T>` for multi-value Spring MVC `@RequestParam` arguments, `List<T>` when order or indexing matters, and `Set<T>` when uniqueness matters. Do not replace a confirmed framework binding contract with `Iterable<T>`. Determine serialization boundaries such as Request Body/Response from the actual framework version and round-trip evidence.
2. [Required] The TASK must confirm actual Spring Boot, Spring MVC and related module versions and affected entry points. Do not apply Web binding rules when Spring MVC is not used.

## 2. Confirmed four-layer architecture

1. [Required] Apply `Controller → Business Service → Persistence Service → Database Access Layer` only when the project already uses four layers or the user expressly approves an architecture change. If the existing architecture is uncertain, propose and confirm each part first. Do not refactor untouched code proactively.
2. [Required] Controllers handle only transport, input validation, current-user information, request/response conversion and calls to Business Services. They must not contain business rules or directly call other layers. Obtain current-user information according to existing style; by default pass only the user ID needed by the use case, and pass an existing user object only when business behavior actually needs it. Business Services must not access Controllers, HTTP Session or security-framework context.
3. [Required] Business Services own business rules, use-case flow, coordination across Persistence Services and transaction boundaries, but must not query or persist data directly. Persistence Services encapsulate data operations and call the Database Access Layer, but must not own business rules, use-case flow or transaction boundaries. The Database Access Layer only queries and persists data; it must not orchestrate data operations or contain business logic.
4. [Default] Follow project naming for four layers. Without a clear convention, name Controllers `Controller` and Business Services `Service`, but use `BusinessService` if the project's `Service` already names the data-operation layer. Name Persistence Services `PersistenceService`, JPA access `Repository`, and MyBatis/MyBatis-Plus access `Mapper`; do not prefix interfaces with `I`, and suffix implementations with `Impl`. Include and confirm every default in the proposal. Do not apply these defaults to other architectures.
5. [Default] Follow project package and directory conventions. With four layers and no clear convention, propose `controller/<feature>`, `service/business/<feature>`, `service/persistence/<feature>`, and `repository/<feature>` for JPA or `mapper/<feature>` for MyBatis/MyBatis-Plus, then confirm before approval. Fix other architectures from their existing structure.
