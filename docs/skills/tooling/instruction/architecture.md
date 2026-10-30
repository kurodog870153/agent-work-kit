# Tooling 指令階層架構規範

## 1. 範圍與權威

本文件規範 `skills/tooling/references/instructions/` 的階層、指令歸屬與組合方式。共用的啟用、來源驗證、按需探索及載入規則遵循[共用契約](../../bootstrap-contract.md)；Tooling 的實際 Runtime Policy 由自身的 `SKILL.md`、Bootstrap 與指令來源決定。

Tooling 是獨立 Skill，可在沒有 Work 工作流程時使用。Host 提供、授權並執行工具；Tooling 定義工具政策，不授予操作權限，也不實作工具執行引擎。Work 需要工具政策時按操作載入 Tooling，不改變 Work 的正式任務、指令選擇或歷史紀錄權威。

## 2. 階層與責任

```text
instructions/
├── instructions.md                 # 共通 Tooling 政策
├── capabilities/
│   ├── instructions.md             # 工具能完成的操作類型
│   └── code-analysis/
│       ├── instructions.md
│       ├── ast/instructions.md
│       └── lsp/instructions.md
├── interfaces/
│   ├── instructions.md             # Host 與工具互動的介面
│   └── mcp/instructions.md
└── families/
    ├── instructions.md             # 特定工具家族的行為
    ├── serena/instructions.md
    └── tree-sitter/instructions.md
```

1. Root 放所有工具操作都適用的政策。Capabilities 描述操作所需能力；Interfaces 描述 Host 實際提供的互動方式；Families 描述特定工具家族的行為。
2. 三個維度相互獨立。選取某個 Family 或 Interface 不隱含 Capability；MCP 只在 Host 實際透過 MCP 提供該操作時選取。
3. 子節點細化自己的祖先，不繼承其他維度的分支。新增節點時依其責任放在對應維度；不為常見工具組合建立跨維度的固定祖先關係。
4. 階層深度與各維度的子節點數量不固定。每個有子節點的分類目錄及 Root 都須有 `instructions.md`；新增葉節點時須保留完整祖先鏈。

## 3. 探索與組合

1. 依目前操作、目標及 Host 實際可用工具，逐層讀取直接子節點的 Metadata，選出所有適用路徑；不預先掃描完整樹或載入未選取的正文。
2. 載入每條選定路徑的完整祖先鏈與適用 Reference，按來源身分去重。各維度的規則合併適用；載入順序不構成優先權，不能用後載入的規則覆寫既有要求。
3. 操作需求、目標、工具能力或來源變更時，只重新讀取新增或失效的來源；切換政策前須驗證完整合併集合及衝突。必要規則衝突或來源無效時，停止受影響的操作。
4. Tooling 的選取不取代 Host 對實際副作用、目標綁定、授權及結果的查核。工具名稱或宣稱能力不能單獨證明適用性。

## 4. 來源格式與 Reference

1. 每個階層 `instructions.md` 與節點 `references/*.md` 都須從第一行開始使用有效 YAML frontmatter，只包含非空的 `name`、`description` 與 `metadata.tags`。Tags 須為不重複的小寫 kebab-case 字串。
2. 節點的 `## References` 只在有適用參考指令時使用，表格欄位為 `Reference | Load condition`，且只指向同節點一層 `references/*.md`。Reference 以實際路徑識別，不透過另一份 Reference 遞迴路由。
3. 使用來源前須驗證一般檔案、嚴格 UTF-8、frontmatter、路徑與 symlink 邊界、來源身分、raw SHA 及適用指紋。無效的 Root 或祖先阻擋所有相依選擇；無效分支阻擋其相依操作。

## 5. 變更要求

1. 調整階層、規則或 Reference 時，檢查受影響的 Bootstrap 路由、祖先鏈、跨維度組合及 Work 整合邊界。
2. 驗證有效與無效來源、按需探索、完整集合切換、衝突處理，以及受影響的安裝與來源載入測試；結果與限制記於對應任務文件。
