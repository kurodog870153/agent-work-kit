---
name: Java task execution
description: Use when changing and validating against a confirmed Java technical baseline; not applicable to non-Java work.
metadata:
  tags:
    - java
---

# Java task execution

Instruction boundary: This layer executes the TASK against the confirmed Java technical baseline; it does not choose versions, architecture, build or test strategy.

1. [Required] Before execution, check through existing entry points that the actual JDK, build tool, framework, dependencies and target module still match the TASK. Record evidence and stop for a specification defect if they do not.
2. [Required] Make changes according to the TASK steps and existing style, leaving unlisted code unchanged. Stop for renewed authorization if a new dependency, plugin, module, generator or tool is needed.
3. [Required] Run the TASK's applicable Red/Green, compilation, tests, static analysis and other VAL. Whether to clean is determined only by the TASK and the purpose of this validation; do not add it by default.
4. [Required] When the Swagger reference applies to the TASK, actually generate or load the OpenAPI contract and run its completeness VAL. Do not ask again about an endpoint contract already confirmed in Task; stop and reconfirm only when the actual artifact differs from the approved content. When Swagger does not apply, do not add related dependencies, configuration or tools, and do not block a Controller change for that reason.
5. [Required] When generated output differs from its source, change only the formal source listed in the TASK and regenerate. Do not edit generated files manually to conceal the difference.
6. [Required] For test VAL, record the target module, specified tests and actual test count. Zero tests, compilation alone or a run that never reaches the target tests is not a pass.
7. [Required] Before execution, verify that the TASK's actual Task instruction hierarchy and references fully fix the build entry point, version evidence, architecture, collections, date/time handling, Lombok, data technology and applicable validation. Stop for a specification defect if anything is missing or inconsistent with current state; Execute must not supply the missing decision.
8. [Required] Run each CMD with the exact environment and arguments fixed by the TASK. Use the exact executable, `JAVA_HOME` or toolchain when specified; use PATH resolution only when the TASK explicitly specifies it. Do not switch build systems, substitute a Wrapper or executable, install a tool, change versions, add clean or rewrite test filters. Handle an equivalent command correction only through the general Execute authorization flow. Verify that the specified module/project and task/goal actually ran tests and that the test count is greater than zero.
