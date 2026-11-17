---
name: Programming language task execution
description: Use when executing a fixed language-specific contract; not applicable to work without programming-language implementation.
metadata:
  tags:
    - language-contract
---

# Programming language task execution

Instruction boundary: This layer only checks the formal language selection; applicable child nodes own language-specific operations.

1. [Required] Before execution, verify that the TASK's selected programming language matches the target sources, configuration and toolchain. Do not switch languages or apply unselected language rules.
