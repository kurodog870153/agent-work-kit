---
name: Java persistence task execution
description: Use when executing the TASK's fixed Java persistence contract; not applicable to work outside the persistence layer.
metadata:
  tags:
    - persistence
    - relational-data
---

# Java persistence task execution

Instruction boundary: This layer only checks that the affected data technology matches the TASK; applicable child layers own implementation details.

1. [Required] Before execution, verify that the actual data-access flow, technology branch and schema source match the TASK. Record differences and return them to Task; do not change persistence technology independently.
