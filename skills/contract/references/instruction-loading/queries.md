# Contract Query Details

1. `list` always returns `data.commands` and `data.contracts` arrays. `--scope` leaves the other array empty; `--prefix` matches the start of a complete command path or contract ID. Zero matches are valid.
2. `describe --command` is exclusive with a contract ID. A contract field pointer addresses JSON data fields, not Registry schema file paths. Do not infer a discriminator branch; request one specific `--variant` when needed.
3. Default `describe` maps field names to summaries. An omitted `required` or `nullable` flag means `true` or `false`, respectively. `details_available: true` means `--field` provides the full constraints; `reference` identifies a linked contract.
4. `scaffold` returns `contract_id`, `scaffold`, `unresolved_paths`, and `selected_branches`. Repeat `--branch` for independent or nested discriminator objects. Select a parent branch before relying on a nested branch; any invalid selection rejects the whole query.
5. `example` returns exactly one `contract_id`, `scenario`, `valid`, and `example`. Replace illustrative paths and IDs with actual task data. Generated Request examples are explanatory and cannot replace formal prepare output.
