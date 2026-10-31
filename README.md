# Agent Work Kit

Agent Work Kit（`agent-work-kit`）提供 Work 與 Tooling 兩個 Skill，協助你規劃、執行工作並驗證適用的工具規範。

## 1. 功能

1. **Work：需求規劃**：整理原始需求，確認範圍、任務相依關係與驗收條件。
2. **Work：任務修訂**：調整已建立的需求或任務，檢查對後續工作的影響。
3. **Work：任務執行**：依授權執行工作，保存驗證結果與執行證據。
4. **Work：討論續接**：保存討論進度，稍後從同一需求繼續。
5. **Work：資料診斷**：分析不相容或損壞的工作資料，協助審查遷移方案。
6. **Tooling：工具規範**：驗證 Host 選定工具所適用的政策來源與操作目標。

工作資料依需求編號保存在專案的 `outputs/work/`，包括原始需求、任務、執行紀錄與討論進度。

## 2. 安裝

預編譯套件支援 macOS ARM64 與 Windows x64 MSVC。可下載正式版安裝腳本，或使用 Release ZIP 離線安裝；從原始碼安裝需要 Rust、Cargo 1.99.1 以上及平台建置工具。

| 安裝方式 | macOS ARM64 | Windows x64 MSVC |
| --- | --- | --- |
| 下載正式版 | [安裝腳本](https://github.com/kurodog870153/agent-work-kit/releases/latest/download/install-work-from-github-release.command) | [安裝腳本](https://github.com/kurodog870153/agent-work-kit/releases/latest/download/install-work-from-github-release.bat) |
| 從 ZIP 安裝 | 解壓後執行 `install-work-from-package.command` | 解壓後執行 `install-work-from-package.bat` |
| 從原始碼安裝 | 執行 `os-scripts/mac/install-work-from-source.command` | 執行 `os-scripts/windows/install-work-from-source.bat` |

安裝器會將 Work 與 Tooling 安裝在同一個 Skill 父目錄。安裝步驟與選項請參閱 [Release 安裝指引](docs/release/README.md)。

## 3. 使用方式

在 Codex 對話中輸入：

```text
$work <mode> -- <request>
```

支援的模式：

1. `task`：規劃新需求並建立任務。
2. `revise`：修訂需求或任務。
3. `migration`：診斷工作資料並規劃遷移。
4. `execute`：執行已確認的任務。

例如：

```text
$work task -- 建立一個包含 UI、frontend 與 backend 的網站
$work execute -- 執行 example 的正式 TASK-001
```

Work 會在建立或修訂正式任務前提供預覽，並在執行需要授權的操作前確認範圍。

Tooling 是獨立的 Skill，負責驗證 Host 選定工具適用的政策來源。它可以單獨使用；Work 的操作需要工具規範時，Host 也會動態載入 Tooling。載入 Tooling 不代表授權執行工具。

## 4. 進一步閱讀

1. [Work 指引架構](docs/skills/work/instruction/architecture.md)。
2. [Tooling 指引架構](docs/skills/tooling/instruction/architecture.md)。
3. [Rust 架構與執行產物管理](docs/rust/architecture.md)。

## 5. 授權

本專案採用 [PolyForm Noncommercial License 1.0.0](LICENSE)。商業用途需要著作權人另行授權；詳細條款以 [LICENSE](LICENSE) 為準。
