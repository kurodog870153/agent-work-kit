# 安裝狀態

安裝器會在 Work、Tooling 與 Contract 的共同父目錄存放一份 `install-state.json`。它用來辨識目前安裝的版本與位置，並決定重新安裝時可沿用哪些設定。這個檔案由安裝器管理，不需要手動建立或修改。

## 1. 檔案位置

| 狀態 | 預設狀態檔位置 |
| --- | --- |
| Codex 的 Work、Tooling 與 Contract | `~/.agents/skills/install-state.json` |
| Claude Code 的 Work、Tooling 與 Contract | `~/.claude/skills/install-state.json` |

使用 `--skills-dir <path>` 時，三個 Skill 會改裝在指定目錄下的 `work/`、`tooling/` 與 `contract/`，狀態檔也會跟著移動。

## 2. 狀態檔記錄什麼

狀態檔記錄 Host、整體發布版本、Work 與 Tooling 的 Rust workspace 版本、共同安裝父目錄、Global Instructions 的目錄設定，以及安裝進度。發布版本取自根目錄 `VERSION`；Work、Tooling 與 Skill 的 crate 版本繼承 `rust/Cargo.toml` 的 `workspace.package.version`。

父層狀態的版本欄位為 `rust_workspace_version`，Global Instructions 目錄欄位為 `global_instructions_dir`。

發布套件的 `release-info.json` 也以 `rust_workspace_version` 記錄同一版本。Work 命令執行資料與 Tooling CLI JSON 回應的 `execution.host.rust_workspace_version` 由共用的 Rust host 中繼資料程式碼產生。

安裝交易期間可能暫時在 Work 或 Tooling 目錄保留子狀態檔作為復原證據；完成驗證後會移除。正式安裝狀態以共同父目錄的檔案為準。

安裝器會檢查狀態檔是否符合預期，也會檢查安裝目錄的內容。狀態檔存在不代表安裝一定有效。

## 3. 重新安裝時會怎麼處理

1. **有有效狀態**：安裝器辨識相同 Host 的現有安裝，並沿用 Global Instructions 設定。你仍可用 `--global-instructions yes|no` 指定此次選擇。
2. **目錄不存在或為空**：視為首次安裝，沒有狀態檔是正常的。首次安裝預設啟用 Global Instructions。
3. **目錄有內容但父層狀態無效或缺失**：安裝器保留原目錄並拒絕安裝。

若安裝中斷，請參閱[安裝復原](recovery.md)；請保留狀態檔與安裝器指出的交易資料，以便檢查。
