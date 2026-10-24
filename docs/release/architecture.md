# Work Release 安裝器架構

## 1. 範圍

本文件說明 Windows 與 macOS 的 Work 安裝入口、模組責任和資料流。使用者操作見 [Release 安裝說明](README.md)；安裝狀態與中斷處理分別見[安裝狀態](install-state.md)和[安裝復原](recovery.md)。

## 2. 三種安裝入口

| 入口 | 取得 Work binary | 交給安裝核心前的責任 |
| --- | --- | --- |
| Source | 從 repository 建置 | 使用原始碼樹中的版本、Skill 與 Global `AGENTS.md` |
| Package | 使用 ZIP 內的預編譯 binary | 驗證套件 manifest、平台與 binary |
| Downloader | 從腳本所屬的固定 GitHub Release tag 下載平台 ZIP | 驗證外部 SHA-256 與 ZIP 內容，再交給 Package |

每個平台各有這三個公開入口。Downloader 負責網路資產的邊界；Package 負責封裝內容的邊界；Source 和 Package 最終進入該平台的安裝核心。Downloader 對安裝選項維持原始引數順序與邊界，由 Package 解析並執行。

```mermaid
flowchart LR
    Source[Source] --> Build[建置 binary]
    Downloader[Downloader] --> Assets[下載與驗證 Release 資產]
    Assets --> Package[Package]
    Package --> Verify[驗證套件與 binary]
    Build --> Core[平台安裝核心]
    Verify --> Core
    Core --> Work[Work 目錄]
    Core --> Global[Global AGENTS.md]
```

## 3. 安裝核心的責任

`os-scripts/common/installer/options.tsv` 是正式選項與 Skill ID／路徑的共用目錄，兩平台各自解析引數。`os-scripts/common/installer/install-state.json` 是 Source 與 Package 共用的安裝狀態模板。安裝核心決定 Work 與 Global `AGENTS.md` 的目標、讀取有效安裝狀態、完成互動選擇，再執行目標與來源預檢。設定來源與狀態有效性由[安裝狀態](install-state.md)說明。

目標預檢辨識空目錄、受管理 Work、可安全接管的舊 Work，以及未知或不安全內容。正式寫入前，核心也檢查版本、正在使用的 Work binary 和 Global AGENTS 的取代決策。Global AGENTS 未啟用時，不參與其目標檢查或交易。

Source 在安裝前建置 binary；Package 使用已驗證的預編譯 binary。兩者將選定的 Skill、binary 與新 `install-state.json` 放入暫存 Work，再透過平台交易模組發布。安裝鎖、正式路徑切換、完成判定和中斷復原由[安裝復原](recovery.md)說明。

## 4. 模組位置

| 責任 | macOS | Windows |
| --- | --- | --- |
| CLI 解析 | `installer/cli/parse-options.sh` | `installer/cli/parse-options.ps1` |
| 安裝狀態與目標預檢 | `installer/state/state-model.js`、`installer/target/preflight.sh` | `installer/state/state-model.ps1`、`installer/target/preflight.ps1` |
| 安裝鎖與核心 | `installer/lock/native-lock.c`、`installer/runtime/execute-locked.sh` | `installer/lock/mutex.ps1`、`installer/runtime/install.ps1` |
| Work 與 Global AGENTS 交易 | `installer/transaction/work.sh`、`installer/global-agents/transaction.sh` | `installer/transaction/work.ps1`、`installer/global-agents/transaction.ps1` |
| 路徑遷移清理 | `installer/runtime/migration-cleanup.sh` | `installer/runtime/migration-cleanup.ps1` |

表內路徑分別相對於 `os-scripts/mac/` 與 `os-scripts/windows/`。`.github/scripts/release/` 負責產生與驗證 Release 資產，不參與使用者電腦上的安裝交易。正式入口的引數與退出碼細節保留在 [Issue #95 CLI 契約](../issue-95/cli-contract.md)。
