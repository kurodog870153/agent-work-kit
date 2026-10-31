# Instruction maintenance guidance

## 1. Metadata, classification and ownership

1. [Required] Every `instructions.md` and `references/*.md` under `skills/work/references/instructions/` must begin with YAML frontmatter containing only nonempty string `name` and `description`, and `metadata` containing only `tags`. Tags must be a nonempty array of unique lowercase kebab-case strings.
2. [Required] The `description` must state when the file applies and does not apply in the shortest sufficient wording. Tags identify distinctive technologies or capabilities and must not repeat the hierarchy path.
3. [Required] The Host discovers Instruction files using normalized frontmatter summaries; discovery must not load or return body text. Sources with missing or unknown fields, invalid UTF-8, invalid YAML or invalid tags must not enter a formal selection.
4. [Required] Every normative numbered item and nested item must start with `[Required]`, `[Default]` or `[Recommended]`. Examples, code, templates and metadata do not need strength markers.
5. [Required] `[Required]` requires stopping unless satisfied; `[Default]` applies when evidence is insufficient and may be departed from with confirmation; `[Recommended]` does not block but requires explaining the effect of not following it.
6. [Required] Task and Execute node bodies must begin with `Instruction boundary:` to state their unique responsibility. Place new rules according to the architecture, not merely the current file location.

## 2. References and duplicate review

1. [Required] References are one level deep at most. Their identity comes from the actual mode, node and filename path. A parent `instructions.md` retains only the trigger condition and relative path; do not create a central manifest.
2. [Required] Exact-duplicate review must cover all instruction sources. Compare normative bodies after normalizing whitespace, line endings, strength markers and list numbers; exclude metadata, examples, code and templates. Identical normative bodies block completion.
3. [Required] Semantic similarity produces only a warning containing both sources and the reason for similarity. The user decides whether to retain, merge or distinguish them; model similarity must not delete instructions automatically.
4. [Required] When touching an instruction, read its official source, current project state and existing child layers. If it may be outdated or lacks evidence, present the evidence and obtain confirmation; do not update untouched instructions incidentally.
5. [Required] After changing an instruction or Reference route, use shared instruction-loading to re-evaluate triggers, topology, actual load order and fingerprints. For an existing formal TASK, use `task.general.task-records` to determine a TASK revision or separate `TASK-INSTRUCTION-AUDIT-*`; do not change only the index hash without impact evidence and user confirmation.

## 3. Wording and size

1. [Required] Prefer the shortest sufficient wording for trigger, requirement, exception and validation. Keep a minimal example only when it resolves a real ambiguity.
2. [Required] Describe capabilities, inputs and observable results without fixing an operating system, shell, command or tool. A formal TASK may fix the current operation from actual environment evidence.
3. [Required] Keep only principles stable across versions in `instructions.md`. The TASK records version and current technical conclusions from official sources; do not maintain a changelog or version matrix in instructions.
4. [Recommended] Report the total character count and the affected loading-chain character count for each instruction maintenance change. Use character targets to improve judgment, not as a hard limit that sacrifices necessary meaning.
