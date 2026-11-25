---
name: GitHub Actions 驗證規範
description: 使用或維護 GitHub Actions、Workflow、分支驗證、Job 相依、Runner、Artifact 或 CI 驗收證據時適用；單純需求規劃、版本摘要或不涉及 CI 的程式碼修改不適用。
metadata:
  tags:
    - github-actions
    - branch-validation
    - ci-evidence
---

# GitHub Actions 驗證規範

## 1. 驗證入口

| Workflow | 觸發及責任 |
| --- | --- |
| branch-validation.yml | 僅 workflow_dispatch 手動選取分支；開發階段固定完整驗證，不依賴 PR、升版或 Release Metadata。 |
| pr-release-validation.yml | pull_request；保留合併前完整驗證及版本、發布內容等條件。 |
| pr-release-credentials-validation.yml | 維持既有 PR Credentials 驗證。 |
| publish-release.yml | 維持既有正式發布契約；非開發驗證入口。 |

1. Branch 與 PR Validation 各自獨立，不新增跨 Workflow 跳過、結果重用或 PR 存在判斷機制。
2. Branch 與 PR 重用共用 Preflight、Rust Validation、Package Validation、嚴格證據驗收與 Summary 呈現機制；不複製測試、建置或驗收邏輯。
3. Branch Preflight 通過後，Rust 與 Package Validation 並行；兩者通過後執行嚴格證據驗收，所有必要 Job 與證據驗收成功才視為通過。最終 confirm Job 僅直接依賴嚴格證據驗收 Job，使用 always() 檢查其結果是否為 success；上游失敗或跳過時證據驗收不得執行成功。PR 與 Release 執行順序維持既有契約。
4. Branch 不執行 PR 專屬 Version Guard、Release Metadata、Credentials 或發布操作；不新增驗證範圍選項。
5. Branch Validation 以 Ref 為群組，重新手動觸發時取消同一 Ref 的舊任務；PR Release Validation 與 PR Release Credentials Validation 各自以 PR Number 為群組，新的執行取消同一 PR、同一 Workflow 的舊任務。各 Workflow 使用獨立群組前綴，不互相取消，不影響其他 Ref 或 PR。
6. Publish Release 維持既有 `cancel-in-progress: false` 與排程契約，不中止進行中的正式發布。

7. Job 僅直接依賴必要的相鄰前置階段，保留真正的並行匯合（Branch 的 Rust／Package）；PR 與 Publish 依既有順序形成單一路徑。不得為取得傳遞性結果或 outputs 而新增跨階段 needs；必要的 producer Attempt 經相鄰 outputs／inputs 原值傳遞。最終 Summary Job 僅依賴 Confirm 或 Post-publish Verification，使用同 Run Jobs API 取得其他階段實際結果，缺失標示 Unavailable；維持 always()、唯讀權限及非 Gate 責任。

8. Branch Validation 固定 Workflow 名稱為 `branch validation`，執行名稱使用 `branch validation: <branch name>`（`run-name: 'branch validation: ${{ github.ref_name }}'`），以辨識受驗分支。

## 2. 開發與驗收

1. 尚未建立 PR 時，優先使用正式 Branch Validation；不得為個別 Issue 建立臨時 Workflow，或僅為觸發 CI 而提前建立 PR、升版或調整 Release Metadata。
2. 可依使用者授權提交待驗證 Commit；Commit 成功不等於驗收完成。
3. 驗證失敗時讀取實際 Job Log，修正後重新觸發；僅依受驗 Commit SHA 的可追溯證據判斷完成。失敗、取消、跳過的必要 Job 或缺少證據不得視為通過。
4. Branch Validation 不取代 PR 合併前驗證；無法觸發 Workflow 時須回報限制，不自行改用臨時 Push Workflow。新 workflow_dispatch 入口須先存在於預設分支；正式入口已由 PR #135 部署後，開發分支應在建立 PR 前執行正式驗證。
5. 共用正式驗收證據及 Summary 契約；依實際用途命名 Artifact，保留仍適用的保存期限，驗證 Rust、已安裝 CLI、Package 及 Checksum 等證據與受驗 SHA 一致。不得為 Branch 另造驗收格式或放寬 PR／Release 驗收。
6. 兩平台的 Package Workflow 採相同驗收順序：原生回歸與建置、套件及 Downloader 驗證、一般完整 Release E2E、報告及 Artifact。保留跨磁碟、交易復原、安裝鎖及 Release E2E 網路隔離。已安裝 Host 模擬必須在非安裝 CWD 使用實體 SKILL.md 注入的路徑，不以預傳 Runtime 路徑替代入口；不要求真實 Host 或非管理員專項測試。安裝鎖僅驗證同使用者互斥、正常／異常退出後重新取得及 SID／UID 輕量識別；不新增跨使用者隔離或目標鎖測試。

## 3. 驗收結果與 Summary

1. Branch、PR、Credentials、Publish 每次 Workflow Attempt 在可執行條件下各產生一份統一 Summary；內部共用 Workflow／Job 不各自輸出。共用呈現依入口責任選取適用資訊，省略不適用項目；不得替代既有嚴格驗收、重跑測試、改變正式 Gate、順序、觸發或 Concurrency。
2. 優先呈現整體驗收結果、受驗 Ref／SHA、Run／Attempt 與實際可取得的執行時間；非平台專屬結果獨立呈現。Windows／macOS 使用相同驗收項目及資料結構橫向比較；Rust 階段、Workspace Test／Doc Test 統計、Package 主要階段、Codex／Claude 個別 Installed Host Simulation 與 Binary Host Simulation 分開呈現。明細留於證據；測試數量差異不新增 Gate。
3. Branch 不顯示 PR／發布專屬驗收；PR 顯示版本及 Release Content／Metadata 驗證；Credentials 只顯示其自身檢查與適用執行資訊；Publish 顯示現有 Content、Tag、Asset、Attestation、Publication、Post-publish 等正式驗收結果。
4. 狀態區分 Passed（實際成功）、Failed（實際失敗）、Skipped（適用但確定未執行）、Cancelled、Timed out、Unavailable（證據不足）；依實際 Step outcome／證據判定，不以 continue-on-error 後的 conclusion 或整個 Job 成功推定細部成功。缺失統計或耗時不推算為零；必要證據缺失仍不得正式驗收通過。
5. 上游失敗或跳過仍盡可能彙整可取得的結果、錯誤摘要與 Job／Log 連結；個別 Artifact 缺失不阻斷其他資料呈現。Summary 不成為新必要 Gate，不能改寫正式結果；強制取消、Runner 中斷或平台異常不保證產生 Summary，不建立跨 Workflow 補償機制。
6. 保留實際驗收階段耗時及可取得的 Runner OS／Architecture／Image、Rust／Cargo、Target 等資訊；Workflow 尚未結束時標示截至摘要產生的已經過時間，不以並行階段耗時加總為整體耗時。驗收及測試統計使用表格；僅跨平台耗時使用簡易 Markdown 長條圖及數值，區分缺失與零，不重複耗時表、不產生圓餅圖、圖檔或額外繪圖依賴。
7. 不常態執行慢測試排序、Cargo Compile Timing、Process Cost Probes、Test Cost Investigation、歷史效能比較及退化警告。保留正式測試驗證、完整範圍、有效證據及失敗診斷。
8. Credentials 維持 pull_request_target、既有 permissions 與 Token 隔離；不執行未受信任 PR 程式，不擴權，不輸出憑證、授權 Header 或敏感錯誤。共用呈現只能使用受信任 Workflow／基準版本程式。其他入口不得擴大非必要權限或改變發布安全契約。

## 4. 驗收證據與功能導向命名

1. Artifact 保存必要正式驗收證據及建置／發布產物；Job Log 保存執行細節；Summary 僅呈現。重用既有結構化資料，不建立平行驗收，不上傳 Summary，不保存無用途的重複 JSON、Markdown、Log 或效能調查資料。移除舊報告前須遷移其必要正式驗收職責及唯一證據。
2. 證據保留受驗 Commit SHA、Run、平台及實際生產 Attempt 等必要識別；重跑／部分重跑須選取同 Run、SHA 且仍有效的 producer 證據，不僅按當前 Attempt 或任意最新 Artifact 選取。保留既有發布 Attempt 傳遞及完整性檢查；Rust 必要測試證據沿用 90 天期限，其餘保留適用保存契約。歷史 Run／Artifact 依原政策到期。
3. 執行程式、函式、CLI、目錄、Workflow／Job／Step、Artifact 與資料檔依實際功能命名。Python 檔案／函式採 snake_case，Workflow 檔名、Job ID、Artifact 採 kebab-case；避免無明確責任的命名與過時 perf／performance 名稱。平台 Artifact 識別統一 macos-arm64、windows-x64-msvc；Rust Target 可另存於證據。
4. Artifact 名稱描述功能或資料用途，依必要性加入平台與 attempt-N 等區分；同步生產端、消費端、資料契約、測試、設定及現行規範，不保留無必要舊名稱、相容空殼或無使用者的資源。移除前檢查有效依賴，保留必要安全檢查、正式 Gate、測試及歷史紀錄。
5. 新增或遷移有效測試須由正常驗證入口執行；驗收涵蓋適用資訊、六種狀態、部分證據缺失、重跑、圖表、唯一 writer 與安全邊界。以語意和可追溯實際證據驗收；模擬與真實發布分開記錄，不為驗收擅自觸發正式發布。
