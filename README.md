# Agent Work Kit

Agent Work Kit（`agent-work-kit`）提供 Work 與 Tooling 兩個 Skill，協助你釐清需求、規劃與執行工作、保存驗收結果，並選擇適用的工具規範。

## 1. 功能

1. **Work：需求釐清**：逐題確認需求，保存原始需求、選項、回答與已確認決策，之後可從同一份 Discussion 繼續。
2. **Work：任務規劃**：建立以目標、需求、限制、決策與驗收條件為核心的 TASK；一份 Discussion 可對應多份 TASK。
3. **Work：任務執行與驗收**：保存 Execution、Attempt、結果與證據，分別記錄 TASK 與需求兩層驗收；必要驗收全部通過才能完成 TASK。
4. **Work：需求修訂**：調整既有需求影響的 TASK 或建立獨立新 TASK，受影響的驗收須重新確認，既有歷史與未受影響的結果保留。
5. **Tooling：工具規範**：依實際操作選擇並驗證適用的工具政策；由 Host 提供、授權與執行工具。

Work 的正式資料只透過 Work CLI 讀寫，以 Source、Discussion、TASK、Execution、Acceptance 與 Correction 記錄保存；已發布的歷史不可覆寫，錯誤以 Correction 或合法修訂處理。Skill CLI 負責依需要載入 Skill 的指令。

## 2. 安裝

1. [macOS ARM64 安裝腳本](https://github.com/kurodog870153/agent-work-kit/releases/latest/download/install-agent-work-kit-from-github-release.command)
2. [Windows x64 安裝腳本](https://github.com/kurodog870153/agent-work-kit/releases/latest/download/install-agent-work-kit-from-github-release.bat)

安裝步驟與選項請參閱 [Release 安裝指引](docs/release/README.md)。

## 3. 使用方式

在專案中開啟 Codex 或 Claude Code，直接描述想完成的工作。Codex 使用 `$work`，Claude Code 使用 `/work`：

```text
$work 為網站新增聯絡表單，送出後顯示成功或失敗訊息，並驗證必填欄位
```

接著依對話完成規劃：

1. 說明目標與限制。Work 一次問一個問題，提供編號選項、建議與理由，並保存每一輪的討論進度。
2. Work 透過 `work task preview` 提供完整 TASK 候選與相依資料版本，供你檢查需求、限制與驗收條件；需要調整就直接指出。
3. 明確同意該候選後，Work 才能透過 `work task apply` 核對批准內容與相依版本並建立或修訂正式 TASK。這一步不會開始執行；不提供直接寫入的 `task create`／`task revise` 命令。

若中途離開，稍後在新的對話請 Work 繼續同一份 Discussion，它會從已保存的記錄接續尚未回答的問題。要開始工作時，指定 TASK 並明確授權執行；執行、驗收與修訂都會保存在 Work 記錄，可從 Source、Discussion、TASK、Execution 追溯到 Acceptance。

Tooling 是獨立的 Skill，不依賴 Work 或 Global Instructions，可以單獨使用；Work 的操作需要工具規範時，Host 也會動態載入 Tooling。載入 Tooling 不代表授權執行工具。

## 4. 進一步閱讀

1. [AI 維護規範入口](ai-maintainer-instructions/README.md)。
2. [Skill 架構](ai-maintainer-instructions/skills/architecture.md)。
3. [Rust 架構](ai-maintainer-instructions/rust/architecture.md)。
4. [安裝器架構](ai-maintainer-instructions/release/installer-architecture.md)。

## 5. 授權

本專案採用 [PolyForm Noncommercial License 1.0.0](LICENSE)。商業用途需要著作權人另行授權；詳細條款以 [LICENSE](LICENSE) 為準。
