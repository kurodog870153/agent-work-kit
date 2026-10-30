---
name: Relational data guidance
description: Use when planning Java relational data structures, queries and transactions; not applicable to work without relational data.
metadata:
  tags:
    - relational-data
---

# Relational data guidance

1. [Required] The TASK must read-only confirm the project's single schema source of truth, migration mechanism, target database/dialect, connection and transaction management, naming conventions and affected data lifecycle. Do not choose between conflicting sources independently.
2. [Required] When adding or changing a table, column, constraint, index, foreign key, sequence or default, fix names, types, nullability, length, precision/scale, uniqueness, referential behavior and upgrade/downgrade outcomes from the single schema source.
3. [Default] Add database comments only when existing project schema conventions require them. If there is no convention but a comment could form a necessary contract, propose its impact for user confirmation; do not turn comments into a hard rule independently.
4. [Required] A migration must fix existing-data conversion, deployment order, lock and downtime risks, rerun behavior, partial failure and recovery outcome. For destructive recovery or deletion, provide only exact steps for the user to execute.
5. [Required] Bind SQL values as parameters. Generate dynamic identifiers, sorts, directions and SQL fragments only from backend allowlists; never interpolate untrusted input directly.
6. [Required] Fix uniqueness, null, not-found behavior, stable ordering, pagination limits and large-data boundaries for queries. Decide indexes only from actual queries, selectivity, write cost and target-database evidence.
7. [Required] Fix the transaction-owning layer, isolation needs, expected affected row count, concurrency conflicts, rollback and partial-success outcomes for writes. Full-table update/delete is prohibited by default; separately confirm protections and validation if necessary.
8. [Required] Proactively propose minimum validation for database-specific migrations, DDL, locks, native SQL, query plans or round-trip behavior. Disclose environment, side effects and risks and obtain authorization before connecting or changing data.
9. [Required] Without an actual connection to the target database, VAL may claim only completed static, compilation or alternative-environment evidence and must list unconfirmed dialect, locking, performance and migration risks.
