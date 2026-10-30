---
name: 正式 TASK 與索引參考指令
description: 規劃正式 TASK collection、驗收與來源契約時使用；非正式 TASK 紀錄不適用。
reference-name: task.general.task-records
metadata:
  work-tags:
    - task-records
---

# 正式 TASK 與索引參考指令

## 1. 正式集合與來源

1. [強制] 正式入口為 `outputs/work/tasks/<requirement-id>/index.json`；集合包含一份 canonical `work-task-index` 及每個引用的 canonical `work-task-item`，不接受單檔 TASK。自訂路徑必須與集合自己的 `artifacts.task` 一致。
2. [強制] Index 保存需求、`TASK-SPEC-*`、狀態、標題、摘要、`artifacts`、`source`、獨立確認的 `hierarchy_selection`、`skill_selection`、需求層 `acceptance_criteria`、文件層 `instruction_selection`、TASK 引用及 `readiness`；選用欄位與 canonical 順序以 CLI contract description 為準，不自行維護另一套 JSON 結構。
3. [強制] `artifacts` 只有 `source`、`task`、`execution`。一般 TASK 的 `source.kind` 為 `snapshot`，`source.manifest` 綁定不可變 Source manifest、完成標記與原始內容。以 `<work-cli> source read`／`source validate` 驗證完整 triplet，不改寫既有來源；新來源只能建立新 snapshot。
4. [強制] 經核准的無 Source migration TASK 使用 `source.kind: migration`，保存全部已審查原始來源的 `path`、`raw_sha256`、`size`、`raw` 與 `approval_sha256`。這不是一般新需求的免 Source 捷徑；Migration 必須明確核准 provenance 與候選集合，原始歷史證據不得改寫。
5. [強制] 需求層驗收使用 `ACCEPTANCE-*` 與可驗證的 `criterion`；每個 TASK 另有非空 `acceptance_criteria`，使用 `<TASK-ID>-ACCEPTANCE-*`。TASK 的 `traceability.acceptance_ids` 只引用需求層驗收，不混入子任務驗收 ID。每項需求驗收至少由一個 TASK 覆蓋，所有兩層驗收均須被 VAL 覆蓋。
6. [強制] 文件層 `instruction_selection` 是各 TASK 指令來源及 reference 的第一出現聯集。每個 item 保存自己的 selection 與 instructions SHA；外部技能身分由集合 `skill_selection` 與 item `skill_id` 表達，Work 指令與外部技能分別驗證。只支援 Task／Execute 指令 mode。

## 2. TASK、相依與追溯

1. [強制] Item 使用 `schema: work-task-item`、`id`、`title`、`skill_id`、`instruction_selection`、`traceability`、`acceptance_criteria`、`goal`、`steps`、`validations`；其他選用欄位由 contract description 決定。Index 不複製 item 內容或 dependencies。
2. [強制] `TASK-*` 在集合內唯一。每個 TASK 的 `INPUT-*`、`TASK-DECISION-*`、`FILE-*`、`RISK-*`、`STEP-*`、`CMD-*`、`OP-*`、`VAL-*` 各自由 `001` 開始；需求層共用決策使用 `DECISION-*`。既有 ID 不重編或重用。
3. [強制] TASK 內引用使用短 ID；跨 TASK 使用 `<TASK-ID>/<ITEM-ID>`。`dependencies` 只列直接相依，禁止未知 ID、自我相依、循環及可推導的冗餘相依；依據相依順序安排執行。
4. [強制] 影響兩個以上 TASK 的共用決策保存於文件層並列出 `task_ids`；單一 TASK 的決策保存於 item。文件層沒有舊來源模型的 goal／deliverable／milestone ID 前置條件。

## 3. 輸入、檔案與步驟

1. [強制] `inputs[]` 使用 `id`、`kind`、`source`、`precondition`；kind 為 `task_output`、`project_state`、`user_provided` 或 `external`，不得保存機密值。
2. [強制] `task_output.source` 使用 `<TASK-ID>/<FILE-ID>`，來源 TASK 必須在完整 DAG 祖先中，不為傳遞相依另加冗餘 direct edge；semantic candidate 的 dependency_position 仍依既有直接相依位置解析。`project_state.source` 使用 normalized project-relative path 表達前置狀態，但自由文字 source／precondition 不自動授予 CMD 讀取權限；其他 source 只保存非機密識別或描述。
3. [強制] `files[]` 的 action 為 `create`、`modify` 或 `move`；前兩者使用 `path`，move 使用 `source` 與 `destination`，不支援 delete。跨 TASK 檔案衝突須明確驗證相依。
4. [強制] `risks[]` 保存 `condition`、`impact`、`mitigation`。`steps[]` 保存 `id`、`action`、非空 `references`，陣列順序就是執行順序；每個 FILE、CMD、OP、VAL 都須被 STEP 引用。

## 4. CMD、OP 與驗收 VAL

1. [強制] `execution_defaults` 與命令的完整 `execution` 覆寫使用 `working_directory`、`os`、`shell`；契約可描述 windows／macos／linux 與 powershell／pwsh／cmd／bash／zsh／sh，但目前一般 argv runner 只執行 Windows／macOS；Linux 與 shell-mode 拒絕。Work 繼承宿主限制，不建立原生隔離政策或安裝工具鏈。
2. [強制] `commands[]` 使用 `mode: argv` 與 `argv`，或 `mode: shell` 與 `script`；專屬欄位互斥。`operations[]` 描述 local_state／external_state 副作用，保存 action、target、validation_id，命令執行時加入 command_id。
3. [強制] automated VAL 保存 `command_ids`、`pass_condition` 及選用的 `verification`；未指定條件時核對全部宣告命令的實際退出碼為零。明確條件支援 all／any／not、實際非零退出碼、完整 stdout／stderr 或宣告報告的比對／hash／JSON Pointer；引用集合須與 command_ids 相符，未知不可通過。ai_review 與 manual 均保存 `confirmer`、`criteria`，前者明確表示 AI 審查，後者表示指定使用者實際確認，不依名稱猜測來源，不帶 verification。Candidate 使用 command keys，語意偏差使用 command positions，正式化沿既有映射，JSON assertion 值不改寫為引用。`acceptance_ids` 可引用需求層或本 TASK 子驗收，不能引用其他 TASK 子驗收；全部必要條件須有正確來源及目前版本的完整證據通過。
4. [強制] Validator 只檢查契約，不執行 CMD／OP。執行或具副作用預檢必須先取得授權，結果保存於 Attempt／Correction；不把指令成功啟動等同驗收完成。
5. [強制] argv CMD 在宿主環境使用專案原有工具、依賴與設定，保留實際建置／測試產物。Task 使用檔案相依與 DAG 明確安排跨 Task 讀寫；自由文字輸入不自動授予操作權限。預覽固定實際 executable／batch launcher、argv、cwd、來源、宿主環境、退出時間與目前檔案版本；相關內容漂移須重新核對，不以 hash 取代既有授權。
6. [強制] 工具、套件、動態函式庫與本地相依由宿主及原專案管理；Work 不複製工具快照、重寫動態載入、建立私人簽章或強制搬移產物。宿主拒絕、工具缺漏或實際失敗須保留證據，依既有範圍處理；不得自動安裝、擴權或繞過限制。

7. [強制] cargo、mvn、npm 等一般開發命令使用同一 argv 入口；依原專案規則生成產物，不建立各工具專用適配流程。未實際執行或結果不明時不宣告成功，不為探知結果而自動重跑。

## 5. Canonical 路徑與指紋

1. [強制] 每個 index 引用包含 `id`、`path`、`canonical_sha256`；相對路徑固定為 `tasks/TASK-NNN.json`，數字必須與 item ID 一致。拒絕絕對路徑、空段、dot／parent 段、保留裝置名、不安全 portable 字元與越界 symlink。
2. [強制] case-folded NFC portable-path 正規化後路徑仍須唯一；`tasks` 目錄內每個直接 regular JSON 檔案必須恰好引用一次，遺漏或額外 item 都是完整性錯誤。
3. [強制] `task_item_sha256` 與 `task_index_sha256` 分別綁定 canonical item／index 的精確 UTF-8 bytes；每個引用 SHA 必須等於實際 item raw SHA，index 不保存自己的 SHA。
4. [強制] `task_collection_sha256` 是衍生 `work-task-collection-fingerprint` 的 canonical SHA，包含 index SHA 與依 index 順序的 item ID／SHA；不是額外正式檔案。集合驗證重建完整邏輯 TASK，驗證來源、指令、技能、相依、輸入、步驟、檔案衝突及兩層驗收。

## 6. 升版與交易

1. [強制] 初版為 `TASK-SPEC-001`，省略 changes；canonical 規格實質變更經核准後只升版一次。`TASK-CHANGE-*` 保存 spec_id、ISO date、reason、affected IDs 及完整結構化 edits，不使用已移除來源模型的 change IDs。
2. [強制] Edit 使用 JSON Pointer：add 只有 after，replace 保存 before／after，remove 只有 before。純需求編號與三個 artifacts 路徑重新命名不升版；指令來源身分或順序變更需要規格修訂，相同來源內容變更且 TASK 仍有效可走 instruction audit。
3. [強制] 正式 readiness 為 `status: passed` 且 spec_id 與頂層一致；核准前的探索證據保留於討論，不當作 Execute 授權。
4. [強制] 初版由 `task preview → apply` 建立 TASK collection 與 Execution index，核准綁定完整候選、來源及 revision；要求正式 index、item targets 與需求 Execution 目錄尚不存在。初版狀態全為 pending，不建立 Attempt／Correction／lock／audit。
5. [強制] Execution index 使用 `work-execution-index`，保存 spec、collection／index／各 item、hierarchy／skill selection、文件／各 TASK instructions 的指紋及狀態；不複製 TASK 或技能全文。狀態為 pending／in_progress／pending_retry／blocked／completed／cancelled，overall_status 由 CLI 推導。
6. [強制] Writer lock、spec_update 與 execution lock 邊界依交易契約驗證。逐檔發布不宣稱檔案系統的多檔案原子性；中斷保留 journal、marker 與已發布 bytes，僅以同一候選與核准指紋 recover，不手寫正式 JSON 或覆寫歷史。
7. [強制] Revise 使用 `specification prepare → preview → apply／recover`。Source 置換先確認整體需求、主驗收及每個 TASK 影響；保存舊 Source／TASK／Execution 交易證據。受影響 TASK 與下游的完成／驗收狀態重新推導，保留 latest Attempt／Correction 與全部歷史。
8. [強制] Reconciliation 明確選擇 retain-only 或需 semantic migration，預覽綁定實際 Attempt bytes、Execution、ledger 與原始來源；apply／recover 必須重新核對同一 approval，失效驗收不得恢復為有效。Migration 的無 Source provenance 與候選只可由公開 migration 流程核准。
9. [強制] `task apply`／`task recover` 不執行 CMD／OP，不建立 Attempt、execution lock 或 instruction audit。所有發布與復原仍須落在使用者授權範圍內。

## 7. 查詢當前契約

1. [強制] 使用 `<work-cli> contract describe work-task-index`、`work-task-item`、`work-task-collection-fingerprint` 查詢欄位、canonical 順序、constraints、nested references 與有效例子。
2. [強制] Source、Execution、Specification、Migration 與 Reconciliation 使用各自 registered CLI contracts；以實際 contract descriptions 與 validator 為結構權威，不複製維護完整 JSON 模板。

## 8. 最小成果與共用檔案審查

1. [強制] 每個 Task 產生單一可驗證成果；AI 審查獨立成果並拆分，不可分割的多檔成果在既有 Discussion 語意審查保存理由，不以檔案數、行數或步驟數切分。
2. [強制] 正式化須有完整規劃結構、目前 context／decision 版本、非空語意審查證據及已完成審查狀態；Work 不另要求成果映射或交易可行宣告。必要目標、步驟、驗收、決策或審查缺漏時阻擋正式化，有疑問逐項確認。
3. [強制] 同檔不同區域的修改仍屬寫入衝突，所有寫寫／讀寫衝突都須有合法祖先順序；讀讀共用不需相依，不提供獨立性豁免。Task inputs 必須表達必要專案來源；間接相依足夠時不得新增冗餘 direct edge，Work 不自動補邊。
