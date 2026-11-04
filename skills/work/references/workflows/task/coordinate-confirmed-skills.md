# Coordinate confirmed skills

1. The Host discovers and recommends skills, then obtains explicit confirmation before recording the Task-owned formal skill selection. The immutable Source preserves the requirement; it does not select skills or instructions.
2. Split work into minimum independently verifiable TASK outcomes and confirm boundaries, dependencies and main acceptance responsibilities.
3. Bind each TASK to exactly one confirmed executable skill ID. Use `null` only for explicitly confirmed base-only work.
4. Give each TASK only applicable paths authorized by the Task-owned hierarchy selection. An empty selection loads `general`; a non-empty path must be confirmed, an ancestor or a descendant and exist in both Task and Execute catalogs. A shared ancestor does not authorize sibling branches.
5. A required skill must support Task and Execute. Return missing or unsupported skills to Task planning for an explicit selection decision; never silently replace a skill.
6. For the selected TASK, retain the complete TASK boundary, its one confirmed skill snapshot, fixed Source, current Work instructions, repository evidence and saved discussion. The Host may use a native Subagent with the same formal Work CLI contract and authorization boundary as the main Agent.
7. Merge the result into the selected TASK discussion. Resolve semantic conflicts through user decisions. Formalize only after all TASK candidates satisfy the complete contract and receive approval.
