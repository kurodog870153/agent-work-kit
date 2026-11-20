---
name: Java MyBatis task planning
description: Use to fix Mapper, SQL, plugin and validation contracts for MyBatis or MyBatis-Plus flows; not applicable to non-MyBatis data access.
metadata:
  tags:
    - mybatis
    - mybatis-plus
---

# Java MyBatis task planning

Instruction boundary: This layer only fixes contracts specific to affected MyBatis/MyBatis-Plus flows; inherit common SQL, transaction and data rules from Java and persistence parents.

## 1. Applicable branch

1. [Required] The TASK must use the target module and Mapper/flow to confirm actual MyBatis, MyBatis-Plus, Spring integration, configuration, Mapper, XML/annotations, `SqlSession` and persistence architecture. List boundaries separately when both technologies are affected.
2. [Required] A Mapper/flow may use only the confirmed branch's APIs, plugins and pagination contract. Do not migrate plain MyBatis because MyBatis-Plus exists elsewhere, or vice versa.
3. [Required] Fixing a technology branch does not authorize adding, upgrading or migrating dependencies, configuration, plugins or architecture. Do not refactor untouched flows proactively.
4. [Required] For each affected Mapper/flow, the formal TASK must fix branch, target module, evidence, actual MyBatis core/MyBatis-Plus version, Spring integration or `SqlSession` management, Mapper scanning, XML resources, configuration, SQL source, transaction participation and data-access architecture. Keep a draft when evidence is insufficient or conflicting.
5. [Default] Prefer existing valid project conventions; without a clear convention use this file's safe defaults. Fix reason, applicable boundary, alternative and direct validation for departures.

## 2. Mapper and SQL

1. [Required] Fix Mapper methods, parameter names/types, binding, returns, SQL sources and result mappings according to existing conventions. XML namespace, statement ID, `resultMap`/`resultType` and Mapper must agree.
2. [Required] Fix SQL sources to the existing annotation, XML or provider method. Use another method only when the existing one cannot express the requirement correctly, fixing reason, boundary and tests. Also fix auto-mapping, aliases, `jdbcType`, `notNullColumn`, column prefixes, nested or constructor mappings when affected.
3. [Required] Use `#{}` for value binding. `${}` and APIs that accept SQL content must not receive untrusted input. Generate dynamic identifiers, table names, sorts and fragments from backend allowlists. If raw interpolation is necessary, fix reason, trusted source, allowlist, boundary and direct validation.
4. [Required] Fix and directly validate null, enum, TypeHandler, dynamic WHERE/SET, empty-collection IN, LIKE escaping and null semantics only when actually affected. Do not add unapproved implicit conversions.
5. [Required] Do not hide data anomalies in single-record queries by taking an arbitrary first row, adding arbitrary `LIMIT 1` or ignoring duplicates. For pagination, follow the existing mechanism and fix parameters, return contract, per-page limit, stable ordering with a unique tie-breaker, count, total rows and overflow outcome.
6. [Required] Fix conditions, scope and expected affected row count for updates/deletes. Full-table operations are prohibited by default; separately fix reasons, protections and validation when necessary. Fix primary-key source, type, generation strategy and writeback by branch, including `useGeneratedKeys`, `keyProperty`, `keyColumn`, composite keys, sequences and batch writeback when applicable.
7. [Required] For multi-row or cross-Mapper writes, fix transaction boundaries in the coordinating layer of the confirmed architecture. For large writes, fix a batch mechanism supported by the existing version, data volume, batch size, failures, rollback, partial success and key writeback. Do not substitute uncontrolled per-row loops.
8. [Required] Follow common Java instructions to fix database date/time types, JDBC behavior, necessary TypeHandlers/conversions and round-trip validation.

## 3. Plain MyBatis

1. [Required] Do not add or use MyBatis-Plus APIs in plain MyBatis flows. Fix the existing pagination mechanism, dialect, parameters, returns, count and interceptors; do not apply `Page<T>`/`IPage<T>`.
2. [Required] Fix executor type, session/transaction lifecycle, flush, batch size and error handling for a batch executor or manual `SqlSession` only when the requirement actually uses one. Do not add it merely to apply instructions.

## 4. MyBatis-Plus and architecture

1. [Default] In MyBatis-Plus flows, prefer Mapper inheritance from `BaseMapper` when the actual version and project architecture support it. Only when the TASK selects a Persistence Service layer, prefer its interface to inherit `IService` and its implementation to inherit `ServiceImpl`. Do not add a layer, migrate untouched flows or replace an existing public API merely to apply instructions.
2. [Default] Prefer `lambdaQuery()` when the TASK uses Persistence Service/`IService` and the query can be expressed clearly. For complex joins, aggregation, special SQL or an inapplicable architecture, use a custom Mapper/XML only with TASK-fixed reason, boundary and tests. A four-layer architecture cannot omit Business Service or Persistence Service, and Controllers cannot call Mappers directly; follow the confirmed chain for other architectures.
3. [Required] Do not pass Wrappers or SQL fragments from Controllers, RPC or other untrusted boundaries. APIs such as `last()`, `apply()`, `inSql()` and `exists()` must satisfy allowlist and parameter-binding requirements.
4. [Required] Do not use `getOne(..., false)` to conceal duplicate data. Check existing pagination contracts or conventions separately at Controller, Service and Mapper layers and preserve each. Where a layer has no existing instruction, default to `Page<T>` for request/parameter and `IPage<T>` for response/return at that layer. Do not break existing Controller or other public APIs; fix maximum row count, overflow, stable ordering and total-count semantics.
5. [Required] When pagination, tenancy, soft deletion, data permissions, optimistic locking or full-table-protection plugins are affected, fix actual version, configuration, InnerInterceptor order, scope, exclusions and direct validation. Custom SQL must not bypass existing controls unintentionally.
6. [Required] Fix transaction responsibility, session lifecycle, flush, batch size, rollback, partial success and key writeback for multi-Mapper writes, batch executors or manual `SqlSession` under the existing architecture.
7. [Required] VAL covers actual impacts on Mapper XML loading, mappings, dynamic SQL, null/uniqueness, key writeback, pagination/count, transactions/rollback, batches, optimistic locks, soft deletion, tenancy, data permissions and plugins. Handle database connections and remaining risks under the relational-data Reference.
