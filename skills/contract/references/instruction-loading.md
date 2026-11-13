# Contract Query Bootstrap

1. Activate only when the Host needs a public CLI command, JSON field, request scaffold, or example. The Host chooses Work or Tooling for the actual domain operation and retains its own routing, tool permissions, and authorization checks. This Skill is not a Work formal Instruction Selection source or a Tooling Policy Source.
2. Use the Host-discovered Contract Skill root. Verify it is a regular Skill directory named `contract`, and read the shared parent `install-state.json` as a regular UTF-8 file. Require `status: installed` and a `skill_install_dir` resolving to that parent. Reject a missing, invalid, interrupted, or mismatched state. Do not use another installation found on `PATH`.
3. Resolve sibling `work` and `tooling` Skill roots and their platform binaries: `work/scripts/work` and `tooling/scripts/tooling` on macOS, `.exe` counterparts on Windows. Check actual paths and symlinks or junctions before use; each Skill root and binary must remain inside the same canonical parent and be a regular executable file. Reject missing or escaped targets. Do not silently switch versions or roots.
4. Check the selected binary's `--help` and `contract ... --help` before constructing argv. A successful query returns the selected CLI's existing JSON envelope. Confirm exit code, status, schema, and `reason_code` before reading `data`; failures do not supply partial query results.
5. Use `contract list [--scope commands|contracts] [--prefix <prefix>]` to discover full command paths and contract IDs. Use `contract describe --command "<full CLI path>"` for public arguments and contract references. Use `contract describe <contract-id>` for a compact field and scenario index; add `--field <RFC 6901 data pointer>` and, only for a discriminator object, `--variant <variant>` for its selected constraints.
6. Use `contract scaffold <contract-id> [--branch <pointer>=<variant>]...` only for a caller-constructible request. Fill every `unresolved_paths` entry from actual task evidence, including required nullable fields. An initial `null` is a placeholder, not confirmed intent. A Generated Request must come from its formal prepare command. Use `contract example <contract-id> --scenario <scenario>` only for a selected registered scenario; `valid: false` marks a rejection example.
7. A query, scaffold, or example proves only public structure. Validate a completed request through the domain's Rust model and business workflow, and obtain the Host authorization required for any side effect. Query results never attest that a business command is safe or already authorized.

## Modules

| Module | Load condition |
| --- | --- |
| `queries.md` | Constructing or interpreting one of the four Contract CLI queries. |
