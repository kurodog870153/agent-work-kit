# Agent Work Kit 的 AI 維護規範

這裡存放維護本專案時使用的技術規範。它們不會隨 Work／Tooling Skill 安裝，也不參與 Runtime 指令載入或正式資料驗證。撰寫規範的格式見 [文件格式範本](TEMPLATE.md)。

## 規範位置

| 領域目錄 | 涵蓋範圍 |
| --- | --- |
| `skills/` | 共用 Skill Bootstrap，以及 Work／Tooling 指令架構。 |
| `release/` | 版本升級、Release 內容準備與安裝器架構。 |
| `rust/` | Rust Workspace 的 crate 責任、公開契約與資料流，以及 Work 測試 fixture。 |

## 選讀方式

1. 依工作範圍進入相關領域目錄，列出其中可能適用的文件。
2. 先讀候選文件開頭的 YAML Frontmatter，以 `description` 與 `metadata.tags` 判斷適用性；此時不需讀取未選取文件的正文。
3. 完整閱讀所有適用文件。一項工作可以同時適用多份規範；工作範圍改變時重新判斷。

## 執行邊界

維護規範須與 Host 限制、使用者授權及專案強制規範共同遵守。若適用規範有無法調和的強制要求，停止受影響的操作並回報；讀取順序不決定優先權。使用 Host 現有的檔案讀取能力即可，不需要額外的 Router、CLI、索引或載入引擎。
