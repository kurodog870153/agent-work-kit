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
2. Branch 與 PR 重用共用 Preflight、Rust Validation、Package Validation 與驗證報告機制；不複製測試、建置或驗收邏輯。
3. Branch Preflight 通過後，Rust 與 Package Validation 並行；兩者通過後彙整報告，所有必要 Job 與報告成功才視為通過。最終 confirm Job 僅直接依賴 rust-validation-report，使用 always() 檢查報告結果是否為 success；上游失敗或跳過時報告不得執行成功。PR 與 Release 執行順序維持既有契約。
4. Branch 不執行 PR 專屬 Version Guard、Release Metadata、Credentials 或發布操作；不新增驗證範圍選項。
5. Branch Validation 以 Ref 為群組，重新手動觸發時取消同一 Ref 的舊任務；PR Release Validation 與 PR Release Credentials Validation 各自以 PR Number 為群組，新的執行取消同一 PR、同一 Workflow 的舊任務。各 Workflow 使用獨立群組前綴，不互相取消，不影響其他 Ref 或 PR。
6. Publish Release 維持既有 `cancel-in-progress: false` 與排程契約，不中止進行中的正式發布。

7. Branch Validation 固定 Workflow 名稱為 `branch validation`，執行名稱使用 `branch validation: <branch name>`（`run-name: 'branch validation: ${{ github.ref_name }}'`），以辨識受驗分支。

## 2. 開發與驗收

1. 尚未建立 PR 時，優先使用正式 Branch Validation；不得為個別 Issue 建立臨時 Workflow，或僅為觸發 CI 而提前建立 PR、升版或調整 Release Metadata。
2. 可依使用者授權提交待驗證 Commit；Commit 成功不等於驗收完成。
3. 驗證失敗時讀取實際 Job Log，修正後重新觸發；僅依受驗 Commit SHA 的可追溯證據判斷完成。失敗、取消、跳過的必要 Job 或缺少證據不得視為通過。
4. Branch Validation 不取代 PR 合併前驗證；無法觸發 Workflow 時須回報限制，不自行改用臨時 Push Workflow。新 workflow_dispatch 入口須先存在於預設分支；正式入口已由 PR #135 部署後，開發分支應在建立 PR 前執行正式驗證。
5. 沿用既有報告、Artifact 命名、保存期限及彙整方式；驗證 Rust、已安裝 CLI、Package 及 Checksum 等證據與受驗 SHA 一致。不得為 Branch 另造報告格式或放寬 PR／Release 驗收。
6. 兩平台的 Package Workflow 採相同驗收順序：原生回歸與建置、套件及 Downloader 驗證、一般完整 Release E2E、報告及 Artifact。保留跨磁碟、交易復原、安裝鎖及 Release E2E 網路隔離。已安裝 Host 模擬必須在非安裝 CWD 使用實體 SKILL.md 注入的路徑，不以預傳 Runtime 路徑替代入口；不要求真實 Host 或非管理員專項測試。安裝鎖僅驗證同使用者互斥、正常／異常退出後重新取得及 SID／UID 輕量識別；不新增跨使用者隔離或目標鎖測試。
