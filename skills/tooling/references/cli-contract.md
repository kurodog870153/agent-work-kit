# Tooling CLI public contract

Use the executable inside the discovered Tooling Skill root: `scripts/tooling` on macOS or `scripts/tooling.exe` on Windows. Invoke `--help`, `contract example --help`, `policy discover --help`, `policy load --help`, and `operation evaluate --help` there. Do not assume `tooling` is on `PATH`. The CLI reads UTF-8 JSON files for the two `--input-file` commands. Its output is one JSON object on stdout; the exit code is part of the result.

The authoritative JSON request examples are embedded from `tooling-model/resources/contract/examples` into the CLI. Obtain them with `contract example --name policy-load-first`, `contract example --name policy-load-reload`, or `contract example --name operation-evaluate`. Each success response has `data.request`; save that object as the UTF-8 JSON request file after substituting local paths or the previous summary. The reload example's empty `previous.sources` is a shape example, not a saved prior result; replace it with the previous successful `data.summary`.

Rejection examples are `contract example --name policy-load-invalid-missing-previous` and `contract example --name operation-evaluate-invalid-missing-target`. After replacing any local path placeholder, each must return exit code 3 with `reason_code: "invalid_request_json"` when submitted to its respective command. These illustrate a missing required field; other invalid cases are listed below.

With `<tooling>` set to the absolute installed executable and `<skill-root>` set to its parent Skill directory, the complete argv forms are:

```text
<tooling> --help
<tooling> contract example --name policy-load-first
<tooling> policy discover --skill-root <skill-root> --node .
<tooling> policy load --skill-root <skill-root> --input-file <request.json>
<tooling> operation evaluate --input-file <request.json>
```

## Commands and request files

| Command | Arguments | Request |
| --- | --- | --- |
| `policy discover` | `--skill-root <Tooling Skill root> --node <node>` | No file. `--node .` means the hierarchy root; otherwise use forward-slash-separated relative hierarchy segments. |
| `policy load` | `--skill-root <Tooling Skill root> --input-file <existing JSON file>` | Start from the `policy-load-first` or `policy-load-reload` CLI example. |
| `operation evaluate` | `--input-file <existing JSON file>` | Start from the `operation-evaluate` CLI example; replace the example paths with existing paths in the current project. |

`policy load` accepts exactly these top-level fields:

| Field | Type | Rule |
| --- | --- | --- |
| `selected_paths` | array of strings | Required. Hierarchy node paths relative to `references/instructions`, using `/` and complete valid segments; `[]` selects only the root policy. The loader adds each selected node's ancestors. |
| `applicable_references` | array of strings | Required. Paths relative to `references/instructions`, e.g. `interfaces/references/session.md`. Each must be declared by a selected node's one-level `## References` table. No duplicates. Use `[]` when none apply. |
| `previous` | `null` or object | Required. Use `null` for first load. For reload, pass the previous successful response's `data.summary` object unchanged. Its shape is `{"sources":[{"relative_path":"...","raw_sha256":"64 lowercase hex characters","canonical_sha256":"64 lowercase hex characters"}]}`. Paths must be nonempty, unique, forward-slash relative paths without empty, `.` or `..` segments. |
| `semantic_review` | `null` or object | Optional. If present as an object, both `reviewed` and `compatible` are required booleans. This is only a Host-reported claim; it does not change CLI validation or confer permission. |

`operation evaluate` accepts exactly two required string fields: `project_root` and `target`. The root must resolve to an existing directory. The target must exist, be lexically beneath the supplied root, and resolve beneath its canonical path. Relative paths are interpreted from the CLI process working directory; use absolute paths to avoid ambiguity. A missing new-file target is rejected by this command and still requires independent Host authorization before creation. An escaped symlink is rejected. This command does not validate tool fitness, authorization, policy compatibility, effects or outcome.

Unknown fields, wrong JSON types and missing required fields are rejected. A JSON request file must already exist and be a regular file. The examples contain portable placeholder paths only where the local filesystem path must be substituted; do not treat them as executable without that substitution.

## Response and interpretation

Every processed command returns `{"schema":"tooling-cli-result","status":"success|rejected|failed","reason_code":"...","message":"...","data":{...},"execution":{"host":{...}}}`. `execution.host` describes the inherited Host environment (`mode`, `os`, `architecture`, `rust_workspace_version`); it does not attest authorization. Exit code 0 is `success`; CLI usage is 2, malformed request or UTF-8 is 3, invalid selections/targets are 4, invalid policy sources are 5, and unavailable input or hierarchy is 8. Codes 8 and 10 map to `failed`; other nonzero codes map to `rejected`. Read `reason_code` and `data` for the precise failure; do not infer Host permission from status.

`policy discover` success has `data.node` and `data.children[]`; each child has `name` and either `metadata` or `error`. It returns direct-child metadata, not unselected bodies. `policy load` success has `data.sources[]` (`relative_path`, `raw_sha256`, `canonical_sha256`, `content`), `data.summary.sources[]` (the same identity and SHA fields without content), `data.change.added_or_invalidated[]`, `data.change.removed[]`, and `data.semantic_review` (`origin: "host_reported"`, `claim: null|{reviewed,compatible}`). `added_or_invalidated` lists sources whose raw SHA or presence changed since `previous`; `removed` lists prior identities absent from the new set. A successful load validates sources and references, but does not certify mandatory-policy compatibility or allow tool execution. The Host reviews the whole combined set and atomically switches its effective policy only after that review.

`operation evaluate` success has `reason_code: "target_valid"` and `data` with `target_valid: true`, `canonical_target`, `authorization_verified: false`, `tool_fit_verified: false`, `outcome_verified: false`. Failure uses `reason_code: "target_boundary_invalid"` for a missing, invalid or escaped target. The Host must separately determine all three unverified conditions.

Complete illustrative response envelopes are available through `contract example --name policy-discover-response`, `policy-load-response`, `operation-evaluate-response`, and `error-response`. They show response shape and status semantics; example digests and paths are illustrative, not observed evidence. Read the returned `data.response` object.

Examples of failures: malformed JSON or missing fields → `invalid_request_json` (3); duplicate or undeclared applicable Reference → `invalid_references` (4); invalid previous summary → `invalid_previous_summary` (4); invalid policy source → `policy_source_invalid` (5); unreadable request file → `input_file_read_failed` (8). The rejected command's `data` carries diagnostic fields that vary by reason. Legacy Host-claim fields in evaluate are rejected as `operation_contract_retired` (4).
