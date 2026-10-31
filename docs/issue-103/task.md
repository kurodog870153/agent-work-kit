# Issue #103 — Work／Tooling 與 Host 協同去重

來源：[Issue #103](https://github.com/kurodog870153/agent-work-kit/issues/103)。本文件是執行計畫；所有任務及驗收條件均待實作與實測後勾選。任務依相依關係重新編號，每項交付一個可獨立審查的成果；若同一契約須跨檔同步修改，視為同一任務。

## 盤點範圍與保留界線

1. Work 公開入口與協調：`rust/crates/work/work-cli/resources/parser/commands.json`、`work-{model,operations,flow,feature,infrastructure}` 的 `delegation`、`handoff`、`workflow`、`routing`、`invocation`、`discussion`、`selection` 模組，`routing_catalog.json`、`operation_effects.json`、公開 contract catalog／schema、fixtures 與 CLI 測試。
2. Work 執行與正式資料：`work-{model,operations,flow,feature,infrastructure}` 的 `execution`、`source`、`task`、`specification`、`contract` 模組；特別檢查 command runner、project file transaction、worktree、host/tool evidence、Attempt／Correction／Acceptance、Journal／Recovery 的界線。正式 Artifact 交易、驗證與歷史資料必須保留。
3. Tooling：`rust/crates/tooling/tooling-{cli,model,flow,feature,operations,infrastructure}`，尤其 `policy`、`operation_evaluation`、`operation_coordination`、`operation_safety`、`operation_outcome`、`host_binding` 與各自測試；`skills/tooling/` 的 Capability／Interface／Family 與專業政策全部納入相容性盤點。
4. Skill／契約：`docs/skills/bootstrap-contract.md`、`skills/{work,tooling}/SKILL.md`、兩份 `references/instruction-loading.md`、其 Modules、`skills/work/references/subagents/`、`workflows/`、Tool Policy Integration，以及 `docs/skills/{work,tooling}/instruction/architecture.md`。
5. 發布與文件：`rust/Cargo.toml`、各 crate manifest、`os-scripts/`、`.github/workflows/`、`.github/scripts/release/` 與相關測試、`docs/{rust,release}/`、`README.md`。歷史 fixtures 僅用於相容性驗證，不預設清除。

## 依序執行的任務

### Task 1 — 完成公開入口與責任對照

- [x] 將 R01–R26 逐項對應到實際 CLI 命令、Rust 模組、Skill 入口、contract、測試及封裝入口，標記「Host 操作／Work 正式資料／Tooling Policy／混合待拆」。沒有未分類的公開入口。
- [x] 對每個混合入口列出保留的確定性驗證及應退役的調度／執行行為；確認歷史 fixture 與正式 Artifact 不會被當成可清理產物。
- [x] 以 `rg`、CLI parser／contract catalog、模組引用及 Git 差異唯讀核對清單；將結果記錄於本 issue 的技術文件。

驗證：2026-10-30 唯讀檢查 Work parser 的 19 個頂層命令群、Tooling CLI 的 2 個命令群、Work contract catalog 的 93 項，以及 118 個關鍵引用檔案；R01–R26 與保留／退役界線見 [inventory.md](inventory.md)。

### Task 2 — 定義 Host 中立的正式資料與證據契約

- [x] 定義 Request／Result／Evidence／錯誤碼／來源綁定，涵蓋 CMD、OP、VAL、檔案版本、Fingerprint、Acceptance、未知結果及部分成功；不需要私有 Sender／Role／Envelope。
- [x] 區分 Host 操作權限、使用者批准、Work 狀態轉換與出版指紋授權；Host 回報的成功執行結果可作為正式 VAL 證據，但須標明來源並通過身份、授權範圍與版本檢查；未知、部分成功或矛盾結果保持未完成。
- [x] 定義跨對話以已提交 Source／Discussion／TASK／Execution／Attempt 重建與重新驗證的流程，以及 Work-only／Tooling-only／共同使用的信任邊界。
- [x] 以正反 contract 案例審查新契約；受影響文件與 schema 同步更新。

驗證：2026-10-30 已將修訂後的 Host 結果規則同步至 Issue #103、[host-contract.md](host-contract.md) 與 `work-host-execution-report` 公開 contract／model；`cargo test -p work-model` 99 項通過，`cargo fmt --all -- --check` 通過。此 Task 僅定義契約；CLI 寫入及 VAL 使用此報告屬 Task 12／16。

### Task 3 — 修訂 Shared／Work／Tooling Bootstrap 契約

- [x] 同步更新 Shared、Work、Tooling Bootstrap 及 Work Tool Policy Integration，清楚區分 Host 選擇、Tooling Policy Source Validation、Work 正式 Selection／Fingerprint Validation。
- [x] 保留最小入口、Modules、階層、Frontmatter、單層 References、按需載入、SHA／Fingerprint、增量重驗、原子切換與受影響操作 fail-closed；不建立跨 Skill Runtime 依賴。
- [x] Work 不強制啟用 Tooling，不把 Tooling Policy 混入 Work `source_order`／Selection／TASK Fingerprint；Tooling 可獨立啟用。
- [x] 新增或更新三種啟用模式及 drift／缺失／衝突政策的正反測試；同一 Release 同步交付契約與 Bootstrap。

驗證：2026-10-31 `skill-infrastructure` 新增三模式測試，逐一驗證 Work-only、Tooling-only、共同啟用的獨立 Bootstrap 路由；任一 Skill 的必要 Module 缺失只使其自身驗證失敗，另一 Skill 仍可驗證。既有 `hierarchy_sources` 正反測試覆蓋缺失、原始 SHA 與跨分支強制政策衝突；`tooling-feature` 測試覆蓋來源漂移與失效範圍。Release 封裝測試確認同一 ZIP 同時包含 Work／Tooling 的入口與 Bootstrap，CI macOS／Windows Package 均從同一 checkout 和 commit 建置。`cargo test -p skill-infrastructure` 21 項、`cargo test -p tooling-feature -p tooling-cli`、Release Python 測試 45 項（1 項既有跳過）、`cargo clippy -p skill-infrastructure --all-targets -- -D warnings`、`cargo fmt --all -- --check` 與 `git diff --check` 通過。此項驗證同版封裝契約，不表示已發布 Release；平台安裝實測屬 Task 18／19。

### Task 4 — 保住 Tooling Policy 的獨立驗證入口

- [x] 確認 `policy discover/load`、Schema、路徑／symlink、Hierarchy、Frontmatter、References、祖先鏈、SHA／Fingerprint／Drift 與多分支合併仍有可呼叫的 CLI／contract。
- [x] Tooling Skill 的 Capability／Interface／Family、Serena／Tree-sitter／LSP／AST／MCP 政策可在沒有 Work 時載入。
- [x] 執行 Tooling policy 的正反測試，作為後續退役工作的保護基線。

驗證：2026-10-30 `cargo test -p tooling-cli --test policy` 2 項通過、`cargo test -p skill-infrastructure` 18 項通過；直接執行 `tooling policy discover --skill-root skills/tooling --node .` 成功列出 Capability／Family／Interface 三個分支，未啟用 Work。

### Task 5 — 精簡 Tooling `operation evaluate`

- [x] 逐項分類 Binding／Effect／Authorization／Outcome／Fallback：保留可信輸入下可機器判定的驗證，退役工具選擇、權限代理、Session、下一步與 Fallback 調度。
- [x] `semantic_review`、`observed_tools`、`authorization`、`outcome` 標明來源與證據性質；Host 的 `authorized`／`tool_fit`／`success` 聲明不得標為 CLI 獨立核實。
- [x] 更新 Tooling model／flow／feature／operations／infrastructure／CLI contract 及正反測試，確認 Tooling 不執行 Shell／MCP、不管理 Agent 或平行 Work Attempt。

驗證：2026-10-30 `operation evaluate` 僅核實 realpath 目標邊界並回傳各 Host 聲明均未由 CLI 核實；舊格式明確以 `operation_contract_retired` 拒絕。`policy load` 把語意審查標為 `host_reported`，缺少審查聲明仍可驗證政策來源。`cargo test -p tooling-cli -p tooling-model -p tooling-operations`、`cargo fmt --all -- --check`、`git diff --check` 通過。空的相容 crate 留待 Task 18 清除封裝。

### Task 6 — 退役公開 Delegation 命令

- [x] `delegation build/validate` 以固定退役錯誤拒絕，不再建構私有 Role／Sender／Envelope；舊輸入不靜默轉換。
- [x] 更新公開 CLI 的正反測試，確認正式 Source／TASK／Execution 入口仍可用，且退役命令不修改正式資料。

驗證：2026-10-30 `cargo test -p work-cli` 全部通過（單元 60、架構 5、目前契約 10、衍生架構 6、程序邊界 51、執行安全 25）；`cargo fmt --all -- --check` 通過。舊 `build/validate` 均回報 `delegation_command_retired`，四種舊角色及唯讀邊界由程序測試覆蓋。私有模組、schema 與 Skill 指令清理由 Task 7 處理。

### Task 7 — 移除 Work 私有 Delegation 與四個 Subagent

- [x] 移除四個 `skills/work/references/subagents/*.md` 的當前入口及對應 Delegation build／validate、Role／Sender／Envelope／Fallback；保留正式來源與授權驗證。
- [x] 更新 schema、Rust 各層、Skill 引用、contract 與測試；歷史 fixture 保留作相容性證據。
- [x] 主 Agent 與任意 Host 原生 Subagent 可使用相同正式 CLI 契約；正式 TASK 建立前不得假造 `TASK-*` 或 Fingerprint。

驗證：2026-10-31 移除四個私有角色入口、五層 Delegation 實作與三份公開私有 contract；保留 `delegation build/validate` 的固定退役錯誤與正式 Source／TASK／Execution 驗證。五份舊 Delegation fixture 以精確 SHA 保留為歷史證據。`cargo test` 涵蓋六個 Work crate 與 `skill-infrastructure`（其中 `work-infrastructure` 328 項通過、1 項既有忽略）；`cargo fmt --all -- --check` 與 `git diff --check` 通過。Host 原生 Subagent 共用正式 CLI 入口由無私有角色／Sender 輸入的 CLI 契約與程序測試確認；Task 9 將另行處理一般 Workflow／Routing 調度。

### Task 8 — 退役 Work Handoff 傳輸協定

- [x] 移除當前 `handoff *` 及 Task→Execute、Execute→Task、Discussion 的 `WORK-HANDOFF` 傳輸依賴；續接改由 Task 2 的持久化正式資料契約支撐。
- [x] 更新 Rust 各層、contract、Workflow／Bootstrap 引用和正反測試；歷史 Handoff 資料不得偽造為已驗證正式狀態。

驗證：2026-10-31 七個 `handoff` 子命令在讀取輸入前回報 `handoff_command_retired`，程序測試確認正式 TASK 位元組不變。五份公開 Handoff contract 與五層傳輸實作、三份當前 Skill 傳輸指令已退役；八份舊 Handoff JSON 以精確 SHA 保留為歷史證據。六個受影響 Work crate 與 `skill-infrastructure` 測試通過（`work-infrastructure` 317 項通過、1 項既有忽略）；`cargo fmt --all -- --check`、`git diff --check` 通過。

### Task 9 — 精簡 Workflow／Routing／Invocation

- [x] 退役私有 Agent 下一步、Prompt／`source_order` 動態調度和 `invocation parse/confirm` 對話流程；保留正式資料狀態查詢、合法轉移及 CLI Request Schema 拒絕。
- [x] 更新 `routing_catalog.json`、`operation_effects.json`、parser、各層實作、Skill Workflow、contract 與測試；Host 決定 Task／Execute／Revise 及是否委派。

驗證：2026-10-31 `workflow next`、`invocation parse/confirm` 在讀取輸入前回報固定退役錯誤；`workflow status` 僅回傳正式狀態與證據欄位，不再輸出 Prompt、`next_action` 或 `source_order`。歷史 TASK／Execution 使用隔離的舊 Workflow 快照驗證，`issue-101-baseline` 保留原樣。六個 Work crate 與 `skill-infrastructure` 測試通過（`work-infrastructure` 317 項通過、1 項既有忽略）；更新 parser help 後 `work-cli` 全套再通過，`cargo fmt --all -- --check` 與 `git diff --check` 通過。

### Task 10 — 拆分 Skill 發現與正式 Provenance 驗證

- [x] Host 接手實際 Skill Discovery／Loading 與語意選取；Work 僅驗證正式 TASK 所引用 Skill／Instruction 的身份、版本、來源及 Fingerprint。
- [x] 逐項處理 `skills catalog`、`instructions *`、`hierarchy *`，保留可重用的純驗證與拒絕條件，清理 Work 主導的 Prompt／Skill 調度。
- [x] 以選取漂移、缺來源、非法祖先鏈及有效正式 TASK 的正反測試驗證。

驗證：2026-10-31 `skills catalog`、`instructions catalog` 以固定 `host_discovery_required` 退役；其餘入口僅驗證 Host 明示的路徑、來源與正式 Selection，處置見 [inventory.md](inventory.md)。`work-cli` 全套、`work-infrastructure` Instruction Source 11 項、`work-feature` Skill 測試及正式 TASK 正反來源測試通過；`cargo fmt --all -- --check`、`git diff --check` 與 parser JSON 格式檢查通過。

### Task 11 — 將 Discussion 對話編排交還 Host

- [x] 拆分提問安排／續接提示與已提交 DiscussionSession、Question ID／Revision／Content Hash、確認證據和版本驗證。
- [x] 更新 Discussion CLI／Rust／Skill 引用與測試；跨對話重建只能使用持久化且已驗證的決策。

驗證：2026-10-31 Discussion 續接僅回傳經完整性驗證的 Session view 與 `planning_ready`，不產生下一步命令；`discussion read/status/question/answer/history` 仍按已提交版本及 Question ID／選項映射驗證。四個受影響 Work crate 測試、`work-infrastructure` 317 項（1 項既有忽略）與跨對話 D008 程序測試通過；`cargo fmt --all -- --check`、`git diff --check` 通過。

### Task 12 — 退役 Work Command Launcher／Runner

- [x] Host CMD 結果以專案內可重讀證據檔、原始 SHA、來源標記及正式身份／版本／授權綁定寫入；VAL 只使用已重驗的結果與完整輸出。
- [x] 移除 `execute command-prepare/run` 的命令啟動器與 Process Runner；Host 執行 CMD／OP／VAL，歷史 Work 收據仍依原格式驗證。
- [x] 保留 Attempt Authorization、Scope／Side Effects、命令結果不明與 Correction 的拒絕及追溯能力；更新 parser、schema、各層、fixture 和測試。

驗證：2026-10-31 `execute command-binding` 凍結 CMD、授權、檔案版本及宣告輸入指紋；Host report 以專案內證據與原始 SHA 寫入，VAL、AI review、carried records 與完成 Attempt 重新讀取並驗證。程序測試覆蓋綁定不符、部分結果、SHA／輸入／輸出漂移、非零退出、Correction、跨 Attempt 承接及正式資料不變。`command-prepare/run` 在讀取 Request 前固定退役，生產 CLI／Flow／Infrastructure 不編譯 `HostCommandRunner`；原 Runner 僅供既有歷史測試編譯，舊收據依原格式驗證。`work-infrastructure` 317 項（1 項既有忽略）、`work-cli` 全套、`work-feature` 74 項、`work-operations` 229 項及 `work-model` 99 項通過；`cargo fmt --check` 與 `git diff --check` 通過。保留的 test-only Runner 及舊收據 schema 不屬目前公開執行入口，Task 17 再審查歷史契約清理。

### Task 13 — 退役專案原始碼 File Writer

- [x] 退役 `execute file-prepare/apply/file-recovery-prepare/file-restore` 對專案原始碼的 Prepare／Apply／Restore；Host 編輯專案檔案。
- [x] 以目前檔案、版本、範圍與真實 Evidence 驗證 Host 變更；未知或非原子多檔結果維持 fail-closed。
- [x] 正式 Source／TASK／Specification／Execution Artifact 的 Lock、Journal、交易及 Recovery 保持完整；更新相應 contract／Rust／測試。

設計決定：2026-10-31 使用者選擇接受 Host 明示的多檔原子完成聲明。Work 在編輯前保存完整前後候選與操作前證據，編輯後重讀 Host 報告原始 SHA 與目前全部檔案／metadata／身份；只發佈正式歷史，不由 Work 寫入或還原專案原始碼。部分、未知或不一致保留原狀並阻擋完成。

驗證：2026-10-31 `file-binding`／`file-record` 凍結完整候選及授權後，只在 Host 明示 `completed_atomic`、原始證據 SHA、完整目標位元組／metadata／身份和目前正式來源相符時記錄歷史。兩檔只改一檔、`partial`／`unknown`、錯誤 Host 證據及出版後證據漂移均被拒絕。四個舊公開 File Writer 命令於讀取 Request 前固定退役且不改動正式檔案；原交易實作透過內部歷史測試路徑保留相容驗證。受影響的 `work-cli`、`work-infrastructure`、`work-model`、`work-operations` 測試通過（`work-infrastructure` 318 項、1 項既有忽略），正式 Source／TASK／Specification／Execution Lock／Journal／Recovery 測試維持通過。Task 17 再審查歷史實作的封裝邊界。

### Task 14 — 精簡 Git Worktree 與 Tool Evidence 轉接

- [x] Git／Worktree 操作交 Host；Work 保留執行前輸入、相依、目前來源、工作樹狀態、修改範圍及檔案版本的確定性檢查。
- [x] 去除僅服務私有 Tool Runtime 的 Evidence 轉接；正式 VAL／Acceptance 可接受標明來源的 Host 成功回報，但拒絕越界、缺授權、身份或版本不符的結果。
- [x] 以已拒絕操作、未知副作用、部分成功及漂移的正反測試確認狀態轉換。

驗證：2026-10-31 Work Git adapter 移除 `mutate`，只接受正式工作樹檢查所需兩組固定唯讀 argv；Git 修改由 Host 工具執行。刪除無呼叫者的 `tool_evidence` 摘要轉接，保留 CMD Host 報告的原始證據／身份／版本重驗。`work-feature` 72 項與 `work-infrastructure` 317 項（1 項既有忽略）及相關 worktree／Git 正反測試通過；Task 12–13 的 Host 回報測試已涵蓋越界、缺授權、部分／未知、輸入和輸出漂移，且正式狀態不被誤完成。

### Task 15 — 驗證 Source／TASK／Specification 正式資料治理

- [x] Source 原始 bytes、固定身份、SHA、路徑安全及歷史不可變性通過正反測試。
- [x] TASK Preview／Validate／Apply、Task ID／Dependency／Acceptance、Skill／Instruction Provenance 與 Source Binding 通過正反測試。
- [x] Specification／Revise／Migration 的 Preview、Approval-bound Apply、Verify、Recovery、版本衝突與歷史保留通過正反測試；只修復本重構造成的回歸。

驗證：2026-10-31 `work-infrastructure` Source Snapshot 15 項、TASK 5 項、Specification 70 項，以及 `work-model`／`work-operations` TASK 30 項正反測試通過。Source 測試涵蓋原始 bytes、SHA／身份、別名路徑、歷史版本與中斷復原；TASK 測試涵蓋正式集合、Provenance、Dependency、Acceptance、批准綁定及出版；Specification 測試涵蓋預覽、批准出版、驗證、版本漂移、遷移與 Journal Recovery。本任務未為了通過測試調整條件。

### Task 16 — 驗證 Execution／Attempt／Correction 正式資料治理

- [x] Execute 的正式 TASK、Current Source／Instruction、Preflight、輸入／相依、Attempt Authorization 與批准 Scope 通過正反測試。
- [x] Execution Index、Attempt、Correction、VAL／Acceptance、Lock、Journal、合法轉移及已關閉歷史保留通過正反測試。
- [x] Host 直接編寫正式 Artifact、假收據、未知或部分成功時均被拒絕或保留未完成狀態；Host 成功回報須經正式身份、授權範圍與版本檢查。

驗證：2026-10-31 `work-model` Execution 13 項、`work-operations` Execution 110 項、`work-cli` 程序邊界 52 項及執行安全 27 項通過；`work-infrastructure` 317 項（1 項既有忽略）包含 Attempt／Correction／VAL／Journal／Lock／Recovery。程序測試核對正式 TASK 與 Preflight 漂移、批准 Scope、偽造或不完整 Host 回報、部分／未知結果、原始輸出和證據漂移、跨 Attempt 承接與正式資料不變；只有綁定完整且可重讀的 Host 成功結果可進入 VAL 判定。歷史原始收據仍須依原格式重驗。

### Task 17 — 完成舊入口與持久化資料相容性

- [x] 舊 Delegation／Handoff／Tooling 調度入口有明確退役錯誤或版本處理；必要 Tooling Policy CLI 可用。
- [x] 舊 Source／TASK／Discussion／Execution／Attempt／Correction 依可驗證狀態讀取；不相容或證據不足回報精確原因，必要時經明確 Migration。
- [x] 清理當前 contract catalog、scaffold、fixture／golden 與失效引用；歷史證據不被覆寫或靜默轉換。

驗證：2026-10-31 `work-cli --test current_only_contract` 10 項、`work-cli` 程序邊界 52 項、`tooling-cli` Operation／Policy 4 項及受影響 Work crate 測試通過。舊 Delegation／Handoff／Invocation、Command Launcher、File Writer 入口均有固定退役錯誤；舊 Source／TASK／Discussion／Execution／Attempt／Correction 按原始位元組與正式身份讀取，損壞、不相容或缺證據的 fixture 保留並拒絕，必要路徑導向明確 Migration。當前 file request scaffold 改為 Host `bind`，Command Run 舊 scaffold 標為不可由 caller 建構；歷史 fixture 的 SHA 及原文不改寫。`issue-101-baseline` 仍由歷史 Instruction 測試引用，必須保留。

### Task 18 — 完成 Skill、文件與安裝封裝清理

- [x] 清理當前 Prompt、Reference、Routing 與文件中的私有角色、Handoff、Runner 和重複 Tooling 調度引用；更新 README、Rust／Release 架構，當前專案名稱統一使用 `agent-work-kit`（歷史證據保留原文）。
- [ ] macOS／Windows 安裝、升級、清理、Release Package／Checksum 及 CI 持續交付 Work＋Tooling Skill 和必要 CLI；只移除退役產物。
- [ ] 執行受影響的 Release／安裝正反測試，確認不清除使用者資料或正式歷史 Artifact。

進度與驗證：2026-10-31 已更新 README、Rust／Release 架構、Skill 續接文字和兩平台下載器 Release 連結；macOS 安裝鎖路徑保留舊名稱以維持與既有安裝器互斥。Release 清單排除已退役的追蹤 Skill 檔，兩平台前檢精確允許舊版受管理目錄中的退役檔案，仍拒絕未知資料。Release Python 測試 45 項通過、1 項既有跳過，macOS 下載器 9 項、前檢及安裝／交易 Shell 整合測試通過；本次 Release Work＋Tooling CLI 建置成功，從目前 Skill 樹成功產生 7,240,155 bytes 的本地 macOS ARM64 ZIP。Windows／PowerShell 實機驗證尚無可用環境，故安裝與完整 Release 驗收維持未完成。

### Task 19 — 完成 Host 協同與跨平台驗收

- [ ] 覆蓋單 Agent、原生 Subagent、Task→Execute、Execute→Revise、跨對話續接、Work-only／Tooling-only／共同使用及政策 Drift／衝突的正反測試。
- [ ] 分別在可用的 Codex、Claude Code 環境驗收 Skill、Shell、編輯、原生 Subagent、Work 正式 CLI 與 Tooling Policy CLI；逐項記錄環境、實際證據及不可驗證差異，不代填通過。
- [ ] `cargo fmt --check`、`cargo check`、`cargo clippy`、Workspace tests 及受影響 Release／安裝測試完成並記錄最終結果；未執行平台維持未完成。

進度與驗證：2026-10-31 macOS ARM64、Codex 工作環境中 `cargo +1.99.1 fmt --all -- --check`、`check --workspace --locked`、`clippy --workspace --all-targets --locked -- -D warnings`、`test --workspace --locked` 與 `git diff --check` 通過；Workspace 內 `work-infrastructure` 317 項通過、1 項既有忽略。`cargo build --release --locked -p work-cli -p tooling-cli` 通過；本次 Release `work --help` 與獨立 `tooling policy discover --skill-root skills/tooling --node .` 實際回報成功，後者列出 Capability／Family／Interface 三分支。Windows／PowerShell 與 Claude Code 執行檔未安裝；其實機 Skill、Shell、編輯及原生 Subagent 驗收未執行，不以 Codex 或靜態審查代填。

## 執行約束與風險

1. Task 2–5 的保留契約與 Tooling 基線必須先成立，才能執行 Task 6–14 的退役；Task 15–17 隨相應變更持續重驗，Task 18–19 最後總驗收。
2. 本計畫涉及公開 CLI 相容性及舊資料。每個退役任務須先確認使用點與歷史資料讀取路徑，再完整修改相關層與測試；不以刪除測試或放寬驗證通過。
3. Task 19 的 Codex／Claude 實機環境可能不同。缺少環境時僅記錄未驗證與風險，不能將靜態審查冒充平台驗收。
