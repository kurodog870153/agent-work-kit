# AI 維護規範文件格式

本格式適用於 `ai-maintainer-instructions/` 內的領域規範文件，不適用於 `README.md`、`TEMPLATE.md` 及 Runtime Skill。

## 1. YAML Frontmatter

文件須以 YAML Frontmatter 開頭，以 `---` 作為起始及結束標記，僅允許以下欄位：

| 欄位 | 要求 |
| --- | --- |
| `name` | 必填、非空字串，描述規範主題。 |
| `description` | 必填、非空字串，描述規範適用及不適用的情境。 |
| `metadata.tags` | 必填、非空陣列，元素為不重複的小寫 kebab-case 字串。 |

不允許額外欄位。

## 2. Markdown 正文

1. Frontmatter 後須有且僅有一個 H1，作為文件標題。
2. H2 至 H6 的名稱、數量及順序不受限制。
3. 正文依規範內容組織，不要求固定章節。

## 3. 格式範例

```markdown
---
name: 領域規範名稱
description: 規範適用及不適用的情境。
metadata:
  tags:
    - domain-capability
---

# 領域規範標題

規範正文。
```