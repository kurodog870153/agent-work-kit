# Issue #103 — Host 與正式資料的信任邊界草案

本文件記錄 Task 2 的契約設計，後續須落到 CLI request／response schema、實作及正反測試後才可驗收。Host 可以是 Codex 主 Agent、Claude 主 Agent 或其原生 Subagent；正式資料不記錄私有 Sender／Role。Issue #103 已依使用者決定修訂：Host 回報的成功執行結果可作為 Work VAL 正式證據，Work 不另要求獨立核實 Host 命令退出狀態。

## 權威與來源

| 事實 | 權威 | Work 的驗證責任 |
| --- | --- | --- |
| 使用哪個 Skill、工具、Subagent，如何詢問與編輯 | Host | 不把 Host 選擇視為正式資料批准。 |
| 使用者對 Scope／Side Effects／發布指紋的批准 | 使用者，經 Host 傳遞 | 驗證正式批准記錄的內容、來源引用、範圍與指紋；不能從普通 `authorized: true` 推斷批准。 |
| Source、TASK、Specification、Execution、Attempt、Correction、Discussion 決策 | Work CLI 正式 Artifact | 重新讀取原始 bytes、身份、版本、SHA／Fingerprint、Journal／Lock 和合法轉移。 |
| Tooling Policy Source | Tooling CLI | 驗證完整有效政策集合的路徑、階層、References、Schema、SHA／Fingerprint 與 Drift；不核發 Host 工具權限。 |
| CMD／OP／VAL 實際執行 | Host 工具或外部系統 | 接受並標明 Host 回報的結果來源；核對正式身份、批准範圍、目前檔案 bytes／版本及 Acceptance 條件，不宣稱 Work 曾獨立觀察命令執行。 |

## Request／Result 的必要欄位

1. Host 提交語意 Request 時，明確指定 operation、正式 `task_id`／`attempt_id`／`record_id`（若適用）、目前 Source／TASK／Instruction／Authorization Fingerprint、預期 Scope 與 Side Effects、所引用 Evidence 的來源和身份。不存在正式 TASK 時不得填造 `TASK-*`。
2. Work Result 回傳 operation、正式 Artifact 身份與版本、持久化狀態、reason code、目前驗證狀態、受影響範圍及可供重試／復原的已提交身份。成功傳輸 Request 只表示 CLI 處理完成，不表示 Host 行為或 Acceptance 已通過。
3. 錯誤至少區分 `invalid_request`、`source_drift`、`authorization_missing`、`scope_exceeded`、`evidence_missing`、`evidence_unverified`、`result_unknown`、`partial_result`、`version_conflict`、`recovery_required`、`unsupported_legacy_contract`。名稱可依現有錯誤碼風格調整，但語意不可合併為一般成功／失敗。
4. Evidence 區分驗收類型 `source`（procedure／ai_review／user_confirmation）與執行結果來源 `result_origin`（host_reported／external_receipt／work_owned_legacy），並記錄 `source_reference`、可用原始 bytes 或不可變引用的 SHA、收集者、收集時間、關聯的正式 Record 與版本及完整性狀態。Host 結果可供正式驗收，但不得誤標為 CLI 自行觀察。
5. Host 執行結果需綁定正式 `task_id`／`attempt_id`／`record_id`、批准的 argv／工作目錄、實際退出狀態、可用輸出／報告引用及完整性狀態。Work 記錄 `host_reported` 來源，驗證欄位與目前正式資料相符；此驗證不等於 Work 獨立觀察 Host 執行。不得憑缺失、未知、部分成功或矛盾的結果完成 VAL。

## CMD／OP／VAL 與檔案變更

1. CMD／OP 的起始與結束須分別綁定同一正式 Attempt、批准的 argv／工作目錄／環境／Side Effects、來源及 Authorization Fingerprint。Host 回報成功可作為正式結果；Work 同時檢查可觀察檔案變化並把未能觀察的外部副作用明確標為 Host 回報。
2. VAL `procedure` 可根據已綁定且標明來源的 Host 命令退出狀態、輸出／報告、條件引擎及目前輸入版本完成。`ai_review` 必須引用具體可重讀 Artifact／檔案及理由；`user_confirmation` 必須引用實際使用者文件與其原始 bytes。未滿足的必要條件維持 pending。
3. 專案檔案修改以操作前已記錄的路徑／原始 SHA 和操作後目前 bytes／SHA、範圍、版本與完整檔案集合核對。SHA 僅證明內容與聲明一致，不能單獨證明修改者、執行時間或外部副作用。多檔結果不明或部分成功時記錄現況，禁止推定原子成功或擅自重試。
4. 正式 Artifact 的 Prepare／Preview／批准綁定 Apply／Verify／Recover 仍由 Work CLI 執行，不能交由 Host 直接寫入。Host 權限與 Work 狀態批准是兩個必須同時滿足的邊界。

## 跨對話與三種啟用模式

1. 新對話從已提交 Source、DiscussionSession、TASK、Execution Index、Attempt／Correction 讀取並重新驗證；未持久化的語意決策標為未知，重新詢問，不使用專用 Handoff 推測。
2. Work-only：Work 正式驗證獨立運作，Host 自行選工具。Tooling-only：Tooling Policy 驗證獨立運作，不建立 Work Attempt。Work+Tooling：Host 對需要 Tooling Policy 的操作調用其 CLI；Policy Fingerprint 不寫入 Work 正式 Selection／TASK Fingerprint。
3. Policy 無效只阻擋依賴該 Policy 的操作；Work 正式 Source／TASK／Attempt 不因 Tooling 未啟用而無條件停擺。Work 正式資料無效也不使獨立 Tooling Policy 驗證失效。

## Contract 案例與拒絕條件

下表定義後續 public schema 與實作的正反案例；目前文件案例不是已執行的 CLI 測試。

| 案例 | Host 輸入 | Work 預期結果 |
| --- | --- | --- |
| CMD 成功 | `source=host_reported`、符合批准的 Task／Attempt／Record、argv／工作目錄、`status=exited`、`exit_code=0`、完整輸出及目前輸入版本 | 記錄 Host 回報來源；可按宣告條件供 VAL 使用，不宣稱 CLI 獨立執行命令。 |
| 預期非零退出 | 同上，`exit_code` 非零但正式 VAL 條件明訂該值 | 依條件引擎評估，不能以一般 `success=true` 取代條件。 |
| 身份或批准範圍不符 | `attempt_id`、`record_id`、argv、工作目錄或 Side Effects 與正式批准不同 | `scope_exceeded` 或 `invalid_request`；不寫成已完成 CMD／VAL。 |
| 來源／檔案漂移 | Source／TASK／Instruction Fingerprint 或目前輸入檔案版本不同 | `source_drift`／`version_conflict`；重新核對或走正式 Recovery。 |
| Host 拒絕、逾時、輸出截斷 | `status` 非已知完成，或必要輸出不完整 | `result_unknown`／`evidence_missing`；保留已知事實，不自動重跑。 |
| 多檔部分成功 | 只觀察到部分批准檔案的變更 | `partial_result`；記錄實際目前 bytes 與剩餘風險，不把整組視為原子成功。 |
| 重播與矛盾 | 同一 Record 再交不同退出狀態或與已提交結果相衝突 | `version_conflict` 或 `recovery_required`；不覆寫已關閉歷史。 |
| 缺使用者批准 | Host 聲明 `authorized=true`，但沒有正式 Attempt 授權或超過其 Scope | `authorization_missing`；Host 回報不能核發正式批准。 |

CLI 的正式 Result 須區分 `accepted_host_report` 與 `verified_formal_state`：前者指結果來源已記錄，後者指 Work 自己完成身份、授權、版本、狀態與交易驗證。兩者同時成立且 VAL 條件滿足時，VAL 才能記錄 `completed`。工具調用完成、傳輸成功或 Host 的自然語言結論均不單獨構成完成狀態。
