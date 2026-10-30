# Compose Tooling Policy

1. Select policy for the current operation, not a fixed tool or entire task. Load selected ancestors and applicable References once by path identity; preserve source identity when the same ancestor serves multiple branches.
2. Review the full combined mandatory requirements. A later source does not override an earlier one; unresolved conflicts block the dependent operation.
3. When applicability changes, retain only independently valid cached sources, load added or invalidated sources, then validate the whole new set before switching. Do not mix an unverified new branch with the old effective set.
