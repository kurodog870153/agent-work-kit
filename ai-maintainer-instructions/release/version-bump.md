---
name: 版本升級與 Release Metadata 準備
description: 為 PR 或下一個 Release 調整根目錄 `VERSION` 並準備對應 Release Metadata 時適用；只撰寫摘要而不調整版本時改用 Release 內容規範，Rust workspace 版本或安裝器工作不適用。
metadata:
  tags:
    - version-bump
    - release-metadata
---

# 版本升級與 Release Metadata 準備

## Purpose

依 Git 版本紀錄與已使用的 Tag／Release 決定根目錄 `VERSION` 的下一版本，避免同一分支重複升版或使用已占用的版本，並為該版本準備 Release Metadata。

## Sources

1. `.github/scripts/version-guard/version_guard.py`：PR 的 `VERSION` 必須是合法 SemVer，且大於 `origin/main:VERSION`。
2. `.github/scripts/release/release_version_availability.py`：PR 的 `vX.Y.Z` 不得已有 Tag 或 Release。
3. [Release 內容準備與發布檢查](publication-content.md)：Metadata 的發布範圍、格式、回退與檢查要求。

## Requirements

### 版本來源與界線

1. 發布版本只取自根目錄 `VERSION`，內容為不含 `v` 前綴的單行 `X.Y.Z`，以換行結尾。
2. `rust/Cargo.toml` 的 `workspace.package.version` 與 `VERSION` 是不同的版本；本流程不修改它、`Cargo.lock` 或任何 crate 版本，除非使用者明確要求。
3. 新版本不帶 prerelease 或 build 後綴，除非使用者明確指定。

### 升版等級

1. 預設升 patch：`X.Y.Z` → `X.Y.(Z+1)`。
2. 只有使用者明確指定時才升 minor（`X.(Y+1).0`）或 major（`(X+1).0.0`）；AI 不得依變更類型自行提高等級。

### 基準版本

1. `main` 版本：`origin/main:VERSION`。
2. 已使用版本：本地 Tag、遠端 Tag，以及 GitHub 上任何狀態的 Release（含 Draft 與 prerelease）中符合 `vX.Y.Z` 的版本。
3. 升版基準為「`main` 版本」與「最高已使用版本」中較大者，比較規則與 `version_guard.py` 的 SemVer 比較一致。

### 不重複升版

1. 目前分支的 `VERSION` 已大於 `main` 版本、`vVERSION` 未被任何 Tag 或 Release 使用，且符合使用者指定的升版等級時，保留現值，不再升版。
2. 只有下列情況才重新計算版本：`VERSION` 小於或等於 `main` 版本、`vVERSION` 已被使用，或使用者指定的等級高於目前已升的等級。
3. 重新計算時，一律從基準版本升一級；不得從分支目前的 `VERSION` 再往上累加。
4. 單純調整 `VERSION` 屬於發布準備，不列入版本摘要或亮點。

## Procedure

### 確認版本紀錄

1. 確認遠端參照可能過期時，向使用者說明並取得授權後再執行 `git fetch origin --tags`；未取得授權時以現有參照計算，並在回報中標示遠端版本未更新。
2. 讀取版本資料：

   ```sh
   git show origin/main:VERSION
   cat VERSION
   git tag --list 'v*'
   git ls-remote --tags origin 'v*'
   gh release list --limit 100 --json tagName,isDraft,isPrerelease
   ```

3. 依「基準版本」與「不重複升版」判斷保留現值或計算新版本，並記錄 `main` 版本、最高已使用版本、目前 `VERSION` 及判斷結果。

### 寫入版本

1. 需要升版時，只將新版本寫入 `VERSION`，保持單行並以換行結尾。
2. 保留現值時不寫入 `VERSION`。

### 準備 Release Metadata

1. 依 [Release 內容準備與發布檢查](publication-content.md) 確認發布範圍，並建立或更新 `.github/release-metadata/vX.Y.Z.md`；檔名與 Title 的版本須與 `VERSION` 一致。
2. 本分支先前已為較舊的未發布版本建立準備中 Metadata（相對 `origin/main` 新增的檔案）時，將其內容移到新版本檔名並更新 Title 版本，不留下舊檔；不得移除 `origin/main` 已有的 Metadata，除非 publication-content 的整理規則要求且其有效內容已納入本次摘要。
3. 版本保留現值時，仍依發布範圍核對既有 Metadata 是否涵蓋本分支新增的變更，必要時更新摘要。

### 操作界線

1. 不建立 Tag、Release 或 Draft，不提交、不推送；這些操作須另有使用者明確要求。
2. GitHub 只做唯讀查詢。

## Validation

1. `python3 -B .github/scripts/version-guard/version_guard.py` 通過。
2. `vX.Y.Z` 不在本地 Tag、遠端 Tag 與 GitHub Releases 中。
3. `.github/release-metadata/vX.Y.Z.md` 存在，檔名、Title 版本與 `VERSION` 一致，且版本號在 Title 中只出現一次。
4. 依 publication-content 完成摘要覆蓋對照，確認重要變更都有對應敘述，沒有未發布的聲明。
5. `rust/Cargo.toml`、`Cargo.lock` 與 crate 版本未被修改，除非使用者明確要求。
6. 回報 `main` 版本、最高已使用版本、原 `VERSION`、最終 `VERSION` 及是否實際升版。

## Failure Handling

1. 無法讀取 `origin/main:VERSION`、遠端 Tag 或 GitHub Releases 時，停止升版並回報缺少的證據，不得以推測的版本寫入。
2. `VERSION` 不是合法 SemVer，或已使用版本中出現無法判讀的版本時，停止並交由使用者決定。
3. 使用者指定的等級與分支已升版本衝突，或 `main` 在作業期間前進時，以最新基準重新計算並說明差異。
4. 驗證失敗時保留已寫入的內容並回報原因，不自動還原或再次升版。
