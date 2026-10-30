# Validate Tooling Sources

1. Verify every selected node and its complete ancestor chain before use. Read its YAML frontmatter with strict UTF-8 and at most one leading BOM; accept only non-empty `name`, `description` and unique lowercase kebab-case `metadata.tags`.
2. Resolve each selected path beneath this Skill root as a regular file. References come only from the declaring node's two-column `## References` table and direct `references/*.md`; reject missing, duplicate, escaped or symlinked-out sources.
3. Preserve raw bytes for source identity and raw SHA. Canonicalize instruction text only for the applicable instruction fingerprint. Report both when they differ; do not normalize historical or external evidence into a new source.
