---
name: Java MyBatis task execution
description: Use for TASK-fixed MyBatis or MyBatis-Plus Mapper, SQL and plugin validation; not applicable to non-MyBatis data access.
metadata:
  tags:
    - mybatis
    - mybatis-plus
---

# Java MyBatis task execution

Instruction boundary: This layer only executes the TASK-fixed MyBatis/MyBatis-Plus branch, Mapper, SQL and plugin validation; it does not replace the persistence API or architecture.

1. [Required] Before execution, check each Mapper/flow to confirm that the actual MyBatis or MyBatis-Plus branch, version and configuration match the TASK. Stop when evidence differs; do not use an API from the other branch.
2. [Required] Change Mapper, XML/annotations, bindings and result mappings according to the TASK. In the bounded diff, confirm that namespace, statement, parameter and return contracts have no unapproved changes.
3. [Required] Run the TASK's applicable VAL for Mapper loading, dynamic SQL, uniqueness, pagination, transactions, batching and plugins. Use `BaseMapper`, `IService`, `ServiceImpl`, `lambdaQuery()`, `Page<T>` or `IPage<T>` only when the TASK has fixed them based on existing instructions at each layer or MyBatis-Plus defaults. Do not change architecture or public APIs or add a plugin merely to pass validation.
4. [Required] Confirm authorization for the OP before connecting to a database or changing data. Without a connection, disclose unverified dialect, SQL, transaction, batch and plugin behavior.
5. [Required] Check all applicable MyBatis instructions in the TASK before execution. Stop if a Mapper/SQL source, binding, mapping, primary key, pagination, transaction, batch or plugin decision is missing. Do not change the SQL source, use another branch's API, add `LIMIT 1`, loosen untrusted SQL handling or add a plugin to fill the gap.
