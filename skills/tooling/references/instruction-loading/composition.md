# Compose Tooling Policy

1. Select policy for the current operation, not a fixed tool or entire task. Load selected ancestors and applicable References once by path identity; preserve source identity when the same ancestor serves multiple branches.
2. Review the full combined mandatory requirements. A later source does not override an earlier one; unresolved conflicts block the dependent operation.
3. When applicability changes, retain only independently valid cached sources, load added or invalidated sources, then validate the whole new set before switching. Do not mix an unverified new branch with the old effective set.
4. For the first CLI load, set `previous` to `null`. On every subsequent load, pass the prior successful `data.summary` unchanged. An unchanged set returns empty `data.change` arrays. Source-byte changes add that source to `added_or_invalidated`; changed Reference applicability adds or removes its source identity. These arrays tell the Host what to reread or remove, not whether the candidate policy is compatible. Review the complete candidate, including all mandatory rules, before switching it into effect.
5. `data.semantic_review` is a Host-reported claim. CLI `success` can coexist with `compatible: false`; in that case the Host must not treat the candidate as compatible or use it to authorize an operation.
