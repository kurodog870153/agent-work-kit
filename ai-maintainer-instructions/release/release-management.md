---
name: Release 版本與內容維護規範
description: 調整 Release 版本、準備或驗證 Release Metadata 及整理發布摘要時適用；Rust workspace 升版、安裝器維護及 CI/CD 實作不適用。
metadata:
  tags:
    - version-bump
    - release-metadata
    - publication
---

# Release 版本與內容維護規範

## 1. 版本管理

1. Release Version 以根目錄 `VERSION` 為唯一權威來源，使用合法 SemVer，不含 `v` 前綴，單行並以換行結尾；Release Tag 使用對應的 `vX.Y.Z`。除非使用者明確指定，版本不帶 prerelease 或 build 後綴。
2. Rust Workspace Version 以 `rust/Cargo.toml` 的 `workspace.package.version` 為權威來源，與 Release Version 獨立管理。Release 升版不要求同步調整 Rust workspace、crate 版本或 Cargo.lock。
3. 預設升 patch；僅在使用者明確指定時升 minor 或 major，不得依變更類型自行提高升版等級。
4. 升版前確認當前分支版本、遠端 `main` 版本、本地及遠端 Tag，以及任何狀態的 GitHub Release（含 Draft、prerelease）。以 `main` 版本與已使用的最高 `vX.Y.Z` 版本中較大者為基準，依 SemVer 比較；缺少必要版本證據時不得以推測結果升版。
5. 當前分支版本已大於 `main`、未被任何 Tag 或 Release 使用且符合指定升版等級時，保留現值，不重複升版。版本不高於 `main`、已被占用或使用者要求更高升版等級時，從上述基準重新計算，不從當前分支版本再累加。
6. 升版時同步準備對應 Release Metadata；保留版本時仍須依實際發布範圍檢查並更新 Metadata。

## 2. Release Metadata

1. 每個待發布版本必須提供 `.github/release-metadata/vX.Y.Z.md`，其檔名與 `VERSION` 一致。此文件是 Release Title 與 Description 的唯一來源，不得省略或以 PR 的標題、描述或其他宣告替代，也不設 PR 內容回退。
2. YAML Frontmatter 必須提供非空、單行的 `title`，格式為 `vX.Y.Z — 主要變更摘要`；本次版本號僅出現一次，摘要須具體且能概括本次主要變更。
3. Frontmatter 後的 Markdown 正文為 Description，須包含實質變更摘要，不限制章節名稱、數量或順序；Overview、Highlights 等章節皆為選填。
4. Metadata 僅保存人工或 AI 撰寫的 Title 與 Description，不將自動附加的 What's Changed、Full Changelog 或 Commit History 寫回 Metadata。

## 3. 發布摘要與歷史管理

1. 以最近一次成功發布的正式 Release 為摘要基準，不以相鄰版本號、Tag、Draft 或 prerelease 代替。檢查基準版本之後至本次預定發布 Commit 的實際變更，包含相關 PR、Commit 及檔案差異。
2. Title 與 Description 須概括完整發布範圍的主要功能、修正及重要變更；納入跳過或未成功發布版本中確實存在於本次發布的有效變更，不重述基準版本以前的內容，不重複摘要或宣稱未納入的功能已發布。
3. 單純調整 Release 或 Rust workspace 版本號不列入摘要；版本契約或行為本身的實質變更仍須描述。
4. 僅保留已正式發布版本及本次待發布版本的 Metadata。已發布版本的 Metadata 須保留；未發布舊版的有效變更經確認已整合至本次摘要後，移除對應舊檔。不得為跳過版本建立佔位檔，也不得移除狀態或內容歸屬尚未確認的 Metadata。

## 4. 工作邊界

1. AI 依授權調整 Release Version、準備及整理 Metadata，並驗證結果；只撰寫或核對摘要時不得自行升版。
2. PR、Commit 及既有 Metadata 可作為分析變更的證據，不作為 Release Title 或 Description 的替代來源。
3. 未經使用者授權，不提交、不推送、不建立 Tag、Draft 或 Release，也不執行正式發布操作。

## 5. 驗證要求

1. 涉及升版時，確認 Release Version 格式、升版等級、基準版本及不重複升版規則均符合要求，待發布版本大於遠端 `main` 版本且未被 Tag 或 Release 占用；未經授權的 Rust 版本變更不存在。
2. 確認待發布 Metadata 存在，檔名、Title 及 Release Version 一致，Title 與 Description 均具有實質內容且符合契約。
3. 逐項對照實際發布範圍的主要有效變更，確認摘要完整、無重複或未納入變更的發布聲明；自動生成的變更紀錄不能替代摘要驗證。
4. 確認歷史 Metadata 與正式發布紀錄一致，未發布舊版已依保留規則整理，且已發布 Metadata 未被移除。
