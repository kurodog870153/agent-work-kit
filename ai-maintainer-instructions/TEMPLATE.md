# AI 維護規範文件格式

本文件是 `ai-maintainer-instructions/` 內業務規範文件的唯一格式範本。[README.md](README.md) 與本範本負責導覽和說明，不必套用下列正文格式。Runtime Skill 有自己的 Frontmatter 契約，不受本範本變更。

## YAML Frontmatter

文件第一行必須是 `---`，並以另一行 `---` 結束 Frontmatter。只允許下列欄位：

| 欄位 | 要求 |
| --- | --- |
| `name` | 必填、非空；使用能辨識規範主題的名稱。 |
| `description` | 必填、非空；說明何時適用，以及何時不適用。 |
| `metadata.tags` | 必填、非空陣列；每個 tag 都是唯一的小寫 kebab-case 字串。`metadata` 不得有其他欄位。 |

格式範例：

```yaml
---
name: 領域維護規範
description: 修改此領域時適用；不涉及此領域時不適用。
metadata:
  tags:
    - domain-capability
---
```

欄位形狀參考 `rust/crates/skill/skill-model/resources/instruction-frontmatter.yaml`；本目錄不屬於其 Runtime 載入來源。

## Markdown 正文

Frontmatter 後須有且僅有一個 H1。H2 只能使用下表的英文名稱，並依表中順序排列：

| 順序 | H2 標題 | 必填 | 用途 |
| --- | --- | --- | --- |
| 1 | `Purpose` | 是 | 規範目的與適用範圍。 |
| 2 | `Sources` | 否 | 判斷或維護時參考的來源。 |
| 3 | `Requirements` | 是 | 技術契約與限制。 |
| 4 | `Procedure` | 否 | 執行步驟或載入流程。 |
| 5 | `Validation` | 否 | 驗證項目與證據要求。 |
| 6 | `Failure Handling` | 否 | 失敗時的處理方式。 |

沒有內容的選填章節應省略。需要時可使用 H3 以下標題、表格、程式碼及圖表；不得為了縮短文件而刪除必要的技術契約。

## 儲存前檢查

1. 確認 Frontmatter 只有表列欄位；缺欄、空值、額外欄位、重複 tag 或不符合小寫 kebab-case 的 tag 都不合格。
2. 確認只有一個 H1，且 H2 沒有缺少必填章節、額外章節、重複章節或順序錯誤。
3. 確認 `description` 能用於適用性判斷，正文保留完整規範。格式通過不代表技術內容已通過審查。
