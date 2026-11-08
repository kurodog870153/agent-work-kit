# Release 內容宣告與發布檢查

正式 Release 的 Tag 由 `VERSION` 決定；Title 是面向使用者的單行功能摘要，不包含版本號。Description 依序包含版本摘要、GitHub 自動產生的變更紀錄與 Full Changelog，以及自前一正式 Release 起的 Commit History。

## 1. 宣告格式

優先在原始碼提交 `.github/release-metadata/vX.Y.Z.md`，檔名須與 `VERSION` 對應。參考格式如下，請將佔位文字換成此次版本內容：

```markdown
---
title: <Release title>
---

## Overview

<Release overview>

## Highlights

- <Release highlight>
```

`## Overview` 與 `## Highlights` 都是選填；沒有內容時移除整節，但 Description 仍須有實質摘要。版本 Metadata 檔案可以省略；一旦建立，單行 Title 與 Markdown Description 都必須填寫。缺少其中一欄會在建立 Tag 前被拒絕。

沒有版本 Metadata 時，可在版本範圍內的已合併 PR 描述中，以 `## Release Title` 或 `## Release Description` 標題宣告。Title 須為單行；Description 可用 Markdown。PR 的兩欄可獨立宣告與回退，同一欄不得重複宣告。

## 2. 回退與錯誤

有完整版本 Metadata 時，使用其中的 Title 與 Description；沒有版本檔時，每欄依序使用 PR 明確宣告、唯一相關已合併 PR 的 Title／描述。相關 PR 必須能從前一正式 Release 至本次 Commit 的 GitHub commit-to-PR 關聯查得、已合併至 `main`，且 PR 的合併結果 Commit 位於版本範圍內。多個相關 PR 的同欄宣告必須完全一致；若沒有宣告且無法唯一選出 PR，發布會停止。Description 缺少 PR 描述時，只以有意義的 PR Title 形成保守摘要。

空白、單純版本號、重複版本號、Merge Commit 與泛稱標題會被拒絕。錯誤訊息會指出失敗欄位或來源；已有版本檔時須補齊兩欄，沒有版本檔時可新增完整版本檔或修正 PR 宣告。既有失敗發布不可重跑；依版本策略建立新版本。

## 3. 發布順序

主線發布先驗證 first-attempt、版本與內容來源，再產生 `release-title.txt`、`release-notes.md`、`release-content.json`。三份檔案作為同一 workflow 的固定產物；建立 Tag 前與建立 Draft 前重新核對來源 Commit、前一正式 Release 基準及內容 SHA-256。Draft 的實際 Title／Description 仍與兩份文字檔逐欄比對。PR CI 可提早檢查版本檔與本 PR 宣告格式；主線 gate 才決定完整版本範圍與回退來源。
