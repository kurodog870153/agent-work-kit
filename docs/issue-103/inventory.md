# Issue #103 — 現況入口與責任盤點

本表以目前的 `work` CLI command tree、`tooling` CLI command builder、Rust 原始碼路徑及 Skill 入口為準。`H` 表示 Host 操作、`W` 表示 Work 正式資料、`T` 表示 Tooling Policy、`M` 表示須拆分；標記是待實作的處置，不代表現況已符合目標。

## 公開 CLI 命令分類

| 類別 | 目前公開子命令 | 責任與後續處置 |
| --- | --- | --- |
| `contract` | `list`, `describe`, `scaffold` | W；刪除退役 contract 的當前 scaffold，保留正式資料 contract 查詢。 |
| `paths` | `resolve` | W；保留正式 Artifact 安全路徑解析。 |
| `hierarchy` | `resolve`, `selection-build`, `selection-validate` | M；Host 選指令，Work 保留正式 Selection 來源驗證。 |
| `instructions` | `catalog`, `resolve`, `load`, `select`, `impact`, `recover` | M；Host Discovery／Loading，Work 保留正式引用、版本、Fingerprint／Drift 驗證。 |
| `fingerprint` | `text` | W；保留純確定性運算。 |
| `skills` | `catalog`, `snapshot`, `selection-validate`, `selection-build` | M；Host 語意選取，Work 保留正式 Provenance。 |
| `handoff` | `validate`, `render`, `build-discussion`, `verify-task-to-execute`, `build-task-to-execute`, `build-execute-to-task`, `verify-execute-to-task` | H；退役當前傳輸入口，歷史資料僅按原格式讀取／拒絕。 |
| `attempt`, `correction` | `validate`, `render` | W；保留正式記錄驗證。 |
| `discussion` | `init`, `read`, `status`, `update`, `question`, `answer`, `history`, `recover` | M；Host 詢問／協調，Work 保留已提交 Session、決策、版本與 Recovery。 |
| `task` | `preview`, `apply`, `recover`, `validate` | W；保留正式 TASK 交易與驗證。 |
| `specification` | `prepare`, `preview`, `apply`, `recover`, `verify`, `reconciliation-preview`, `reconciliation-prepare`, `reconciliation-apply`, `reconciliation-recover` | W；保留候選、批准綁定 Apply、驗證、交易及復原。 |
| `workflow` | `status`, `next` | M；保留正式狀態查詢／合法轉移；退役 Agent／Prompt 下一步指揮。 |
| `workspace` | `create` | M；分辨正式交易 Workspace 與私有傳輸 Workspace，前者保留。 |
| `execute`（正式記錄） | `preflight`, `attempt-start`, `attempt-start-prepare`, `recover-attempt-start`, `record-begin`, `command-correction`, `record-finish`, `attempt-close`, `correction-create`, `recover`, `recovery-prepare`, `deviation-prepare-semantic`, `deviation-record` | M；保留正式 Attempt、Correction、Evidence 與轉移，移除其中 Host 命令／編輯調度。 |
| `execute`（Host 操作） | `worktree`, `command-prepare`, `command-run`, `file-prepare`, `file-apply`, `file-recovery-prepare`, `file-restore` | H/M；退役 Git／Process／專案檔案執行器，保留必要前置與結果的確定性驗證。 |
| `invocation` | `parse`, `confirm` | H/M；退役對話確認與模式協調，保留普通參數／Request Schema 驗證。 |
| `migration` | `analyze`, `prepare`, `preview`, `apply`, `recover`, `verify` | W；保留正式 Artifact Migration。 |
| `delegation` | `validate`, `build` | H/M；退役私有 Envelope／Role，正式資料授權檢查轉入對應 Work 操作。 |
| `source` | `capture`, `read`, `validate` | W；保留來源原始 bytes、身份與 Fingerprint。 |
| Tooling `policy` | `discover`, `load` | T；保留完整有效 Policy Source Set 的確定性驗證。 |
| Tooling `operation` | `evaluate` | 已精簡為目前目標的 realpath／專案邊界純驗證；舊 Host 工具／授權／結果宣告格式明確拒絕。 |

## R01–R26 對照

| Issue ID | 主要現況位置 | 分類 | 保留／退役界線與後續 Task |
| --- | --- | --- | --- |
| R01–R04 | `skills/work/references/subagents/{task-coordinator,task-skill,execute,artifact-editor}.md` | H | 四個角色與當前引用退役；Task 7。 |
| R05 | `work-{model,operations,flow,feature}/src/delegation.rs`、`work-infrastructure/src/delegation_storage.rs`、`delegation *` | M | 公開命令在 Task 6 退役；Role／Envelope 與私有實作於 Task 7 退役，正式授權留存。 |
| R06 | 各層 `handoff.rs`、`handoff_storage.rs`、`handoff *`、`discussion-handoff` contract | H | 當前傳輸退役，歷史相容另驗；Task 8、17。 |
| R07 | 各層 `workflow.rs`、`routing_catalog.json`、`workflow next` | M | Agent 指揮退役，正式狀態保留；Task 9。 |
| R08 | `work-operations/src/routing.rs`、`skills/work/references/instruction-loading/` | M | 動態 Prompt 調度退役，正式來源身份保留；Task 9–10。 |
| R09 | 各層 `invocation.rs`、`invocation parse/confirm` | M | 對話 Gate 退役，參數驗證保留；Task 9。 |
| R10 | `skills/tooling/` | T | 獨立 Skill 與專業政策保留；Task 3–5。 |
| R11 | `tooling-{cli,flow,feature,model,infrastructure}` 的 `policy` | T | `discover/load` 與來源驗證保留；Task 4。 |
| R12 | `tooling-{flow,feature,operations,infrastructure}` 的 `operation*`／`host_binding` | M | 已保留目標 realpath 邊界純驗證；Host 調度、授權代理、Outcome／Fallback 退役；Task 5。 |
| R13 | `skills/work/references/instruction-loading/tool-policy-integration.md`、`work-feature/src/workflow.rs` | M | Work Tool Runtime Gate 退役，正式 Evidence Gate 保留；Task 3、9、14。 |
| R14–R15 | `skills *`、`instructions *`、`hierarchy *`、selection contracts | M | Host 載入／選擇，Work Provenance 保留；Task 10。 |
| R16–R17 | `execute command-prepare/run`、`work-infrastructure/src/execution/command/runner.rs` | M | Runner 退役，Attempt／Evidence 保留；Task 12。 |
| R18–R19 | `execute file-*`、`work-infrastructure/src/execution/file_transaction/` | M | 專案檔案 Writer 退役，正式 Artifact 交易保留；Task 13。 |
| R20–R21 | `execute worktree`、`work-feature/src/tool_evidence.rs`、`execution/host_evidence.rs` | M | Git 修改入口與未使用的 Tool Evidence 摘要轉接已退役；唯讀 Git 狀態、範圍／版本檢查及 CMD Host 證據保留；Task 14。 |
| R22–R23 | `discussion *`、`discussion/workflow.rs`、`workflow status/next` | M | Host 對話安排，Work 已提交決策保留；Task 9、11。 |
| R24–R25 | `private-roles.md`、`internal-envelope.md`、`transaction-workspace.md`、`workflows/` | M | 私有傳輸引用退役，Journal／Workspace 保留；Task 7–9、18。 |
| R26 | `rust/Cargo.toml`、`os-scripts/`、`.github/`、`docs/`、`README.md` | M | 清理退役產物，Work＋Tooling 安裝與 Recovery 保留；Task 18–19。 |

## 檢查面與資料界線

1. Work parser 為 `rust/crates/work/work-cli/resources/parser/commands.json`；公開 contract 在 `work-model/resources/contract/catalog.json`、各類 schema 與 scaffold orders；operation effects 與 routing catalog 必須跟每個入口同步核對。
2. Work 的 model／operations／flow／feature／infrastructure／CLI 六層、Tooling 的對應六層、`work-cli/tests`、`tooling-cli/tests`、`work-feature/tests` 及 `work-infrastructure/fixtures` 均在變更面內；歷史 fixture 用來證明相容性，不能預設清理。
3. Shared／Work／Tooling Bootstrap、Work Workflow Modules、Tooling Capability／Interface／Family、macOS／Windows installer、Release package／checksum、CI、README 與架構文件是發布面。具體檔案需在各任務寫入前重新唯讀檢查，以避免依賴過期內容。

## Task 10 — Skill 與 Instruction 入口處置

1. `skills catalog`、`instructions catalog` 屬 Work 主導的廣域發現入口，現回報 `host_discovery_required`；Host 由已啟用的 Skill 根目錄自行發現並決定是否載入。
2. `skills snapshot/selection-build/selection-validate` 保留對 Host 明示選擇的身份、來源、支援模式與 Fingerprint 驗證。`hierarchy resolve/selection-build/selection-validate` 與 `instructions resolve/load/select` 僅處理指定路徑、祖先鏈、來源摘要及正式 Selection；它們不選擇 Host 的下一步或讀入指令正文供執行。
3. `instructions impact/recover` 保留正式來源變更與 Journal 復原；正式 TASK 驗證仍核對完整 Source、Hierarchy、Instruction、Skill Selection 與版本，缺來源、非法祖先鏈或漂移時拒絕。
