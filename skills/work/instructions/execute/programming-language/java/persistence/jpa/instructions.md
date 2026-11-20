---
name: Java JPA task execution
description: Use for TASK-fixed JPA or Hibernate changes, data validation and evidence recording; not applicable to non-JPA data access.
metadata:
  tags:
    - jpa
    - hibernate
---

# Java JPA task execution

Instruction boundary: This layer only performs TASK-fixed JPA/Hibernate changes and evidence work; it makes no new metamodel, mapping, relationship, transaction or locking decisions.

1. [Required] Before execution, verify that the actual JPA/Hibernate version, namespace, Entity, Repository and schema source match the TASK. Stop if they differ; do not migrate or upgrade independently.
2. [Required] Verify the processor, generated source and compilation only when the TASK requires a static metamodel. Do not edit generated files manually or add a processor when the TASK does not require one.
3. [Required] Run the TASK's applicable VAL for mappings, relationships, query count, pagination, transactions, locks, schema and migrations. Do not repair failures with global EAGER loading, extra cascades or a new architecture.
4. [Required] Confirm authorization for the OP before connecting to a database or changing data. Without a connection, report only actual evidence and unverified dialect, lock, native SQL or migration risks.
5. [Required] Check all applicable JPA instructions in the TASK before execution. Stop if mapping, relationships, queries, pagination, transactions, locks, bulk operations, batching, auditing, schema or validation is missing. Do not fill gaps with JPA/Hibernate defaults, Open Session in View, extra saves, global EAGER loading or broader cascades.
