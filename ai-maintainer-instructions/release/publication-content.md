---
name: Release 內容準備與發布檢查
description: 準備或驗證 Release Metadata 與發布內容時適用；無關的安裝或 Rust 工作不適用。
metadata:
  tags:
    - release-metadata
    - publication
---

# Release 內容準備與發布檢查

## Purpose

依實際發布範圍準備完整的版本摘要，並在正式發布前後核對內容。

## Requirements

正式 Release 的 Tag 由 `VERSION` 決定；Title 採 `vX.Y.Z — 主要變更摘要`，版本號與本次 Tag 一致且只出現一次。Title 與 Description 概括自上一個成功發布的正式 Release（不含）至本次版本（含）的主要變更。最終 Release Body 依序包含宣告的版本摘要、GitHub 自動產生的 What's Changed 與 Full Changelog，以及該範圍的 Commit History。

## Procedure

### 確認發布範圍

1. 在 GitHub Releases 找出最近一次成功發布的正式版本。Draft、prerelease、Tag 或 `VERSION` 的存在，都不代表已正式發布；版本號可以跳號。
2. 檢查從該版本之後到本次預定發布 Commit 的合併 PR 與 Commit。列出確實納入本次版本的主要功能、修正及重要變更。
3. 若中間有未成功發布的版本，將其已納入本次版本的有效變更一起整理。合併重複敘述，不重述前一正式版以前的內容，也不宣稱尚未納入的功能已發布。
4. 同時檢查 `.github/release-metadata/` 的歷史檔案：除目前 `VERSION` 的準備中檔案外，每份檔案都應對應已正式發布的 Release。若有未發布舊版檔案，先核實其有效變更已納入本次摘要，再移除舊檔。

### 撰寫版本摘要

優先在原始碼提交 `.github/release-metadata/vX.Y.Z.md`，檔名須與 `VERSION` 對應。參考格式如下，請將佔位文字換成此次版本內容：

```markdown
---
title: vX.Y.Z — <major changes summary>
---

## Overview

<Release overview>

## Highlights

- <Release highlight>
```

`## Overview` 與 `## Highlights` 都是選填；沒有內容時移除整節，但 Description 仍須有實質摘要。版本 Metadata 可以省略；一旦建立，Title 與 Description 都必須填寫。Metadata 只存宣告摘要；自動生成的變更紀錄不寫回原始檔。單純調整 `VERSION` 或 Rust workspace 版本號屬於發布準備，不列入版本摘要或亮點；若版本欄位的契約或行為有變更，則應描述該實質變更。

1. 在 Metadata 中填寫本次 Title 與 Description。Title 的版本須與本次 `VERSION`／Tag 相同，摘要須概括「確認發布範圍」列出的整段發布範圍。
2. 沒有版本 Metadata 時，可在版本範圍內的已合併 PR 描述中，以 `## Release Title` 或 `## Release Description` 標題宣告。Title 須為單行，Description 可用 Markdown；兩欄可獨立宣告與回退，同一欄不得重複宣告。
3. 在發布前比對摘要與已確認的 PR／Commit，確認重要變更都已涵蓋、沒有重複或未發布聲明。Metadata 只寫版本摘要，GitHub 自動生成的紀錄由發布流程附加。
4. 逐項對照「確認發布範圍」列出的重要變更與 Title／Description；每項都應能在摘要找到對應敘述。再核對舊版 Metadata、相關 PR 的描述與實際程式或文件變更，確認跳過版本的內容沒有被最後一版覆蓋。Commit History 完整不代表摘要完整。

### 回退與發布前檢查

1. 有完整版本 Metadata 時，發布流程使用其中的 Title 與 Description。沒有版本檔時，每欄依序使用 PR 明確宣告、唯一相關已合併 PR 的 Title／描述。合法的單一 PR Title 回退會補上本次版本前綴。
2. 相關 PR 必須已合併至 `main`，能由版本範圍內的 Commit 查得，且合併結果 Commit 位於該範圍。多個相關 PR 的同欄宣告必須完全一致；沒有宣告且無法唯一選出 PR 時，發布會停止。Description 缺少 PR 描述時，以有意義的 PR Title 形成保守摘要。
3. 檢查 Title 是否缺少本次版本前綴、版本不符、只有版本、重複版本或僅為泛稱。檢查 Description 是否包含實質摘要。已建立版本 Metadata 時須補齊兩欄；沒有版本檔時，可新增完整版本檔或修正 PR 宣告。

### 發布順序

1. PR CI 先核對歷史 Metadata 與 Published Releases，再以最新 `main` 至前一正式 Release 的已合併 PR，加上目前 PR，檢查內容來源與回退規則。合併後，主線發布會依實際 Commit 範圍重新檢查。
2. 主線發布先驗證執行身分、未發布狀態、版本、歷史 Metadata 與 Published Releases 的對齊及內容來源，再產生 `release-title.txt`、`release-notes.md`、`release-content.json`。建立 Tag 前及 Draft 前，會重新核對來源 Commit、前一正式 Release 基準及內容 SHA-256。
3. 發布流程會比對 Draft 的實際 Title／Body 與已驗證的兩份文字檔，並檢查資產。同一執行在正式發布前可重跑失敗工作：既有 Tag 必須指向原 Commit，既有 Draft 的內容與資產必須相符。正式發布後不得重跑發布步驟。
4. Draft 轉為正式 Release 後，流程再次比對實際 Title／Body 與已驗證內容，並驗證資產；若比對失敗，發布驗證會失敗，需要人工查明原因。

### 發布後整理

1. 核對正式 Release 的 Title、Body、Tag 與 Assets，確認與已驗證的內容及產物相符。
2. 將有效內容已納入正式版本、且自身從未正式發布的舊版 Metadata 移除；保留已發布版本的 Metadata，不為跳過版本建立佔位檔。
