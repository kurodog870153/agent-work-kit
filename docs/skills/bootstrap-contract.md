# Shared Skill Bootstrap Contract

## 1. Scope and authority

1. This document is the shared Bootstrap contract for Skill design and maintenance. Each Skill carries its own complete Runtime Bootstrap; runtime does not read this document or another Skill, or compare versions across Skills.
2. This contract governs only activation entry points, hierarchical sources, discovery, loading, validation and failure handling. The Host decides whether to activate a Skill, which tool or native Subagent to use, and whether a source is semantically applicable. Tooling CLI validates its Policy Source Set; Work CLI validates formal TASK/Instruction Selection and fingerprints. Neither CLI grants Host tool permission.
3. The strength markers are `[Required]`, `[Default]` and `[Recommended]`. A dependent operation cannot skip a required rule. A default rule may be departed from only under an explicitly applicable exception. A recommended rule may be adopted according to the situation. A later-loaded source does not override an earlier one; incompatible required rules across branches must be resolved before use.

## 2. Activation and document structure

1. [Required] `SKILL.md` provides only a minimal activation entry point to its own `references/instruction-loading.md`. The main entry point has a two-column `## Modules` table headed `Module | Load condition` and loads Bootstrap modules from its own `references/instruction-loading/` only when their conditions apply.
2. [Required] The hierarchy root and every category directory with children have an `instructions.md`. A node's `## References` table has two columns headed `Reference | Load condition`. It may route only one-level files in that node's `references/*.md`; a Reference cannot recursively route another node or escape its boundary. Omit the section when no Reference applies.
3. [Required] Every hierarchical `instructions.md` and `references/*.md` begins with YAML Frontmatter containing only nonempty `name`, `description` and `metadata.tags`. Tags are nonempty, unique lowercase kebab-case strings. A Reference is identified by its actual path and has no `reference-name`. Bootstrap documents follow their own Schema and need not use hierarchical Frontmatter.
4. [Required] Category depth and top-level dimensions are not fixed. Several branches may apply at once; their rules compose without cross-branch inheritance. Parent-child inheritance follows only the complete ancestor chain of the actual path.

## 3. Runtime stages

1. **Activation:** [Required] Find the activated Skill through the runtime's Skill Discovery. Do not assume a fixed installation path or the presence of another Skill.
2. **Initial Loading:** [Required] Load this Skill's main Bootstrap and check applicable Modules. Shared docs and other Skills are not runtime dependencies.
3. **Discovery/Selection:** [Required] Read only direct-child Metadata from the current node and choose the next level according to this Skill's needs and applicability conditions. Explore multiple branches to any depth without scanning the entire tree, maintaining a persistent Metadata Index or preloading unselected bodies. Host semantic selection does not replace Tooling Policy Source Validation or Work formal TASK/Instruction Selection Validation; these authorities are specific to their own data.
4. **Source Validation:** [Required] Before loading an applicable source, check its file type, strict UTF-8 with at most one leading BOM, Schema, Frontmatter/Metadata, path/realpath/symlink boundary, source identity, raw SHA and applicable canonical SHA/fingerprint. Do not confuse original bytes with normalized content.
5. **Instruction Loading:** [Required] Load the complete ancestor chain of selected paths and their applicable References. Deduplicate by source identity and check Module/Reference conditions, order and cross-branch compatibility. Children supplement or refine ancestors without overriding them; load order does not confer priority.
6. **Re-evaluation:** [Required] When needs, applicability conditions or sources change, reread only new or invalidated sources. Before switching, validate the complete combined applicable set, ancestors, References, deduplication, fingerprints and conflicts. Update the effective set atomically only after success.
7. **Failure Handling:** [Required] An invalid node blocks dependent loads; an invalid shared root or ancestor affects all dependent selections. A still-valid old set may be retained after new-set validation fails, but cannot bypass sources required by a new selection. Stop dependent loading when source state is unknown. Failure blocks operations depending on that source or policy, not unrelated Host work or another independent Skill.
8. **Validation/Failure Report:** [Required] Report applicable sources, source identities, validation results, failure reasons and affected selection scope. Each Skill handles subsequent state according to its own recording and reporting rules.

## 4. Same-release maintenance and acceptance

1. [Required] For each change to this contract, complete impact analysis, each affected Skill's Runtime Bootstrap update and affected positive and negative tests in the same Release. Runtime cross-Skill version comparison is not required.
2. [Required] Acceptance covers valid and invalid Bootstrap, strict UTF-8/BOM/YAML, Metadata, path/symlink safety, complete ancestor chains, Modules/References, on-demand Discovery, incremental loading with whole-set validation, cross-branch conflicts and source SHA/applicable fingerprints.
3. [Required] Retain reproducible steps or commands, actual results and evidence links for each case. Do not mark unrun or unsupported cases as passed; explain limits and equivalent verification.

