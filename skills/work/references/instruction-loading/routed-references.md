# Load routed references


1. Do not enumerate or eagerly load every file below a hierarchy node's `references/` directory.
2. When a loaded `instructions.md` lists a reference in its two-column `## References` table and its condition applies:
   1. Resolve it only relative to the directory containing that `instructions.md`.
   2. Require it to be a regular file inside that same hierarchy directory after platform normalization and link resolution.
   3. Derive its globally unique logical name from the mode, declaring hierarchy path and actual filename; load it immediately after the declaring instruction. Reject a selected reference absent from that node's table.
3. Decode routed references using the same strict UTF-8 and optional BOM behavior as instruction files.
4. Apply only references whose trigger is established by the current request or confirmed task state. Record both loaded references and references whose triggers did not apply.
5. Re-evaluate reference triggers when new evidence or a confirmed decision changes applicability. Add newly applicable references, remove references that no longer apply, rebuild actual source order and instruction fingerprints, list affected decisions, and reconfirm each affected decision before formal TASK approval.
6. The Host reviews the combined ancestor and selected branch requirements before relying on them. If two applicable `[Required]` requirements contradict each other, stop the dependent selection and resolve the conflict with the user; later load order never overrides either requirement. Record the source paths, conflicting requirements and decision. Do not add a machine-readable conflict field to the instruction schema.
