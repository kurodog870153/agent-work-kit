---
name: Requirement discussion and task planning
description: Applies when clarifying requirements, recording Discussion or Source, planning TASKs or revising planned requirements; not applicable to carrying out an approved TASK.
metadata:
  tags:
    - requirement-discussion
    - task-planning
---

# Requirement discussion and task planning

1. [Required] Lead the discussion. Before each question, survey the relevant project functionality and facts. Ask one question at a time with numbered options, a recommendation and its reason.
2. [Required] After each answer, restate the requirement and check for conflicts with confirmed content. Stop asking once the necessary information is clear; do not repeat questions or over-design.
3. [Required] Save the Discussion each round with the original requirement, questions, options, answers, confirmed decisions and open items. After resuming from a saved Discussion, map every answer to the correct question and option.
4. [Required] Keep the user's original requirement in the Discussion. Save a GitHub Issue, external document or attachment as a Source only when its original evidence must be kept, and link it instead of saving it twice.
5. [Required] Do not invent requirements. Separate confirmed requirements, facts, inferences and assumptions, and ask about missing information, conflicts with project evidence or results that cannot be determined uniquely.
6. [Required] A TASK centers on a goal, requirements, constraints, confirmed decisions and acceptance conditions, each outcome having at least one observable acceptance condition. Record dependencies, risks, files, steps, commands and tool operations only as actually needed.
7. [Required] A Discussion may exist without a TASK, and one Discussion may map to several TASKs. Link each TASK to its Discussion and Source so the original requirement, decisions and acceptance stay traceable.
8. [Required] Decide technology, versions and files from actual project evidence. Treat user-supplied detail as input to check or as a confirmed constraint, and do not expand the surrounding design.
9. [Required] When changing an existing requirement, revise the TASKs it affects; create a new TASK for an independent new requirement. Determine the affected scope, reconfirm affected acceptance, keep unaffected results and immutable history, and obtain confirmation for any change to requirements, decisions or acceptance conditions.
10. [Required] For TASK creation or revision, use `work task preview` to show the complete candidate and reviewed dependency versions; only after the user approves that exact candidate use `work task apply` with the unchanged candidate and dependency snapshot. Never publish via a direct TASK create/revise command. Saving progress or confirming a single decision is not formal approval.
