# Work 安裝狀態

## 1. 位置與用途

成功安裝後，最終 Work 目錄中的 `install-state.json` 記錄目前版本和下次安裝可沿用的設定。預設 Work 目錄為 `~/.agents/skills/work`；`--work-install-dir` 可指定另一個以 `work` 為名稱的目錄。新狀態隨新 Work 一起發布，不是 Release ZIP 來源 manifest 的一部分。

專案內的 `os-scripts/common/installer/install-state.json` 是欄位皆為 `null` 的安裝器模板。Source 從專案讀取它，Package 從 ZIP 的 `installer/state/install-state.json` 讀取它，再填入實際安裝值。模板不是已安裝狀態。

## 2. 格式

狀態檔為 UTF-8 JSON。以下範例只示範欄位形狀，路徑須與實際安裝目標相同：

```json
{
  "version": "1.1.0",
  "skills": ["general", "programming-language", "typescript"],
  "work_dir": "/Users/example/.agents/skills/work",
  "global_agents_dir": "/Users/example/.codex"
}
```

| 欄位 | 意義與有效值 |
| --- | --- |
| `version` | `主版.次版.修訂版` 形式的版本字串 |
| `skills` | 非空、無重複的實際 Skill ID 陣列，包含 `general`、所有選取項目的父層，並按正式目錄順序排列 |
| `work_dir` | 絕對 Work 目標路徑，讀取時必須與目前目標相符 |
| `global_agents_dir` | 絕對 Global `AGENTS.md` 目標目錄；停用 Global AGENTS 時為 `null` |

`global_agents_dir` 有路徑表示此次選擇啟用 Global AGENTS，`null` 表示停用；它不代表既有 `AGENTS.md` 一定已被取代，因為使用者可拒絕取代不同內容。啟用時保存選定位置，供下次安裝沿用。

## 3. Skill 選擇與設定沿用

正式 Skill ID 與階層定義於 `os-scripts/common/installer/options.tsv`，Package ZIP 內也有對應目錄。`general` 表示只選基本 Skill；`all` 表示選取全部；逗號清單可選實際 ID。選取子層時會補齊其父層和 `general`，狀態檔保存展開後的結果，而非原始輸入文字。

設定依下列順序決定：

1. 明確指定的 CLI 選項覆寫對應設定。
2. 未指定的選項沿用目前目標的有效 `install-state.json`，包括啟用 Global AGENTS 時的自訂目錄；從受管理的預設 Work 路徑遷移時，可沿用該舊位置的有效狀態。
3. 沒有可用的有效狀態時，Skill 預設為 `general`，Global AGENTS 預設啟用。

互動模式只詢問未由 CLI 指定的設定；非互動模式使用上述結果，不讀取 stdin。`--no-global-agents` 不新增、更新或刪除既有 Global `AGENTS.md`。

## 4. 有效性與安全接管

讀取狀態時，解析器核對 JSON 物件的完整欄位集合、型別、版本、Skill ID 與展開順序、`work_dir` 與目前目標是否一致，以及 `global_agents_dir` 是否為絕對目錄或 `null`。macOS 逐字比對 Work 路徑；Windows 對 Work 路徑使用不分大小寫比對。缺檔、空檔、損毀、欄位缺失或不相容的狀態一律當作無有效狀態，不能用來沿用設定。

狀態無效不代表可以直接覆蓋既有目錄：

| Work 目錄 | 安裝處理 |
| --- | --- |
| 不存在或為空 | 依首次安裝設定規劃 |
| 非空且沒有有效狀態，但能辨識為舊 Work 且沒有未知內容 | 互動模式顯示檢查結果並要求明確確認；非互動模式拒絕接管 |
| 含未知檔案、不安全連結或無法確認 Work 身分 | 保留原內容並拒絕安裝 |
| 有效受管理狀態 | 依版本與目標預檢結果規劃更新 |

目標安全辨識由平台的 `installer/target/preflight.*` 執行，與狀態 JSON 的格式驗證是兩道不同檢查。安裝交易及中斷後的狀態判定見[安裝復原](recovery.md)；逐項 CLI 行為見 [Issue #95 CLI 契約](../issue-95/cli-contract.md)。
