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

1. [macOS ARM64 安裝腳本](https://github.com/kurodog870153/agent-work-kit/releases/latest/download/install-agent-work-kit-from-github-release.command)
2. [Windows x64 安裝腳本](https://github.com/kurodog870153/agent-work-kit/releases/latest/download/install-agent-work-kit-from-github-release.bat)

安裝步驟與選項請參閱 [Release 安裝指引](docs/release/README.md)。

## 3. 使用方式

### 建立 TASK

在專案中開啟 Codex 或 Claude Code，直接描述想完成的工作。Codex 使用 `$work task`，Claude Code 使用 `/work task`：

```text
$work task -- 為網站新增聯絡表單，送出後顯示成功或失敗訊息，並驗證必填欄位
```

Claude Code 範例：

```text
/work task -- 為網站新增聯絡表單，送出後顯示成功或失敗訊息，並驗證必填欄位
```

接著依對話完成規劃：

1. 說明目標與限制，依提示提供需求編號並回答確認問題。Work 會保存原始需求及討論進度。
2. 檢查 Work 提供的完整 TASK 預覽，包括工作範圍、順序與驗收條件；需要調整就直接指出。
3. 確認預覽內容後，明確同意發布該版本的 TASK。Work 會建立正式任務並驗證結果；這一步不會開始執行任務。

若中途離開，使用先前確認的需求編號繼續。例如需求編號是 `example`：

```text
$work task -- resume example
```

正式 TASK 建立後，如要開始工作，再指定任務並授權執行：

```text
$work execute -- 執行 example 的正式 TASK-001
```

Claude Code 將上述 `$work` 換成 `/work`。其他模式：`revise` 用於修改已建立的需求或任務；`migration` 用於診斷舊工作資料。

Tooling 是獨立的 Skill，負責驗證 Host 選定工具適用的政策來源。它可以單獨使用；Work 的操作需要工具規範時，Host 也會動態載入 Tooling。載入 Tooling 不代表授權執行工具。

## 4. 進一步閱讀

1. [AI 維護規範入口](ai-maintainer-instructions/README.md)。
2. [Work 指引架構](ai-maintainer-instructions/skills/work-instruction-architecture.md)。
3. [Tooling 指引架構](ai-maintainer-instructions/skills/tooling-instruction-architecture.md)。
4. [Rust 架構](ai-maintainer-instructions/rust/architecture.md)。

## 5. 授權

本專案採用 [PolyForm Noncommercial License 1.0.0](LICENSE)。商業用途需要著作權人另行授權；詳細條款以 [LICENSE](LICENSE) 為準。
