---
name: GitHub Issue 任務規劃
description: 收到 GitHub issue 編號或連結並需規劃專案工作時適用；不適用於未指定 issue 的一般維護工作。
metadata:
  tags:
    - github-issue
    - task-planning
---

# GitHub Issue 任務規劃

## Purpose

將 GitHub issue 的需求與目前專案狀態整理成可依序執行、可獨立驗收的任務文件。此流程只規劃任務；後續實作依使用者授權與專案規範執行。

## Sources

1. 閱讀 issue 本文、後續討論、連結的相關資料及其更新狀態。
2. 盤點工作區相關的原始碼、測試、文件、設定、工作流程與既有任務文件；確認目前內容及未提交變更。
3. 依工作範圍閱讀本目錄中其他適用規範。

## Requirements

1. 全面盤點與 issue 需求、相依關係及驗收相關的檔案，不只依 issue 提到的路徑決定範圍。記錄已確認的現況、缺口與證據來源。
2. 若需求、行為、範圍或驗收方式有影響規劃的疑問，逐項與使用者討論；在必要資訊明確前，不將猜測寫成已確定的任務。
3. 將工作拆成大小適當、各有單一成果與明確完成條件的 task，依相依關係排列。不可分割的多檔變更保留為同一 task。
4. 將規劃存於 `docs/issue-{number}/task.md`，其中 `{number}` 為 issue 編號。使用 Markdown checkbox 表示每項完成條件的狀態；未驗證的條件維持未完成。

## Procedure

1. 從 GitHub issue 連結取得 repository 與編號；若使用者只提供「GitHub issue 124」等編號敘述，先由目前專案的 Git remote 確認 repository。無法唯一確認 repository 時，先詢問使用者，不猜測 issue 來源。
2. 依確認的 repository 與編號讀取 issue 本文、相關討論及連結資料。
3. 搜尋並閱讀相關檔案，對照現有實作、測試與文件，整理受影響範圍及相依順序。
4. 逐項提出必須由使用者決定的問題，根據回答更新規劃；沒有此類問題時直接進入任務整理。
5. 參照既有 issue 任務文件的慣例，撰寫 `docs/issue-{number}/task.md`，列出各 task 的目標、範圍、完成條件與驗證方式。

## Validation

1. 確認任務涵蓋 issue 的全部需求及相關檔案，並可依序執行。
2. 確認每項 task 大小適當、可獨立驗收，完成條件均有正確的 checkbox 狀態。
3. 確認文件路徑與 issue 編號一致，且未將待討論事項當成已決定的需求。
