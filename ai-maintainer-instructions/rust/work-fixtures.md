---
name: Work Infrastructure 測試 Fixture 規範
description: 新增、修改或使用 Work Infrastructure 測試 fixture 時適用；不涉及 Work 測試素材的 Rust 架構或其他領域工作不適用。
metadata:
  tags:
    - work-fixtures
    - rust-testing
---

# Work Infrastructure 測試 Fixture 規範

## Purpose

定義 Work 測試 fixture 的目錄分層、素材使用方式、新增規則與驗證要求，避免測試改動原始素材或破壞歷史證據。

本文件說明的檔案目錄為 `rust/crates/work/work-infrastructure/fixtures/`，後文的相對路徑皆以此目錄為起點。

## Sources

1. `rust/crates/work/work-infrastructure/src/fixture_support.rs`：`copy_fixture_sources` 的實作。
2. `rust/crates/work/work-cli/tests/current_only_contract.rs`：歷史位置、完整 SHA 與退休契約的保護測試。
3. [Issue #79](https://github.com/kurodog870153/agent-work-kit/issues/79)：fixture 目錄重整的來源 issue。

## Requirements

### 目錄分層

1. `cases/<domain>/<feature>/<case>/` 保存單一測試情境。`input/` 是請求與 setup 輸入；`expected/` 是唯讀預期結果；`project/` 是專案根目錄樣本，正式 Work 路徑從其 `outputs/work/` 開始。case 外層路徑不進 artifact、request 或 approval。
2. `shared/<name>/` 保存已確認由多個測試共用的唯讀素材。`task-diagnostics-project` 與 `specification-migration-project` 是明確專案根目錄；`specification-reconciliation-inputs` 保存共用 Attempt、Execution index 及 migration preview。測試明確指定資源，不跨讀其他 case 的 project。
3. `historical/` 保存歷史 bytes、舊契約拒絕證據與保留的 instruction baseline。用途包含 Delegation 的退休 plan／progress-saver、Task assembly 與 draft-sources、Reconciliation 的原始 Attempt，以及 Task 的歷史 instruction baseline。負例中的舊資料也可位於 case project；分類不會解除精確 SHA 保護。

### 歷史證據保護

1. `rust/crates/work/work-cli/tests/current_only_contract.rs` 保護精確歷史位置與完整 SHA，掃描現行 JSON 與嵌入 stdout，維持退休契約拒絕。Reconciliation 的 reviewed journal 從 case 的 `input/publication.json` 綁定 `project/` 正式路徑；未審查 journal 必須失敗，不以檔名或整個 `historical/` 目錄放寬。
2. Issue #79 重整時，225 筆原始檔案與最終位置的映射、子任務及驗證紀錄原記於 `docs/issue-79/task.md` 附錄 F；該檔目前不在 repository 中。Task assembly 中沒有一般 loader 的伴隨證據仍保留，不能因未被直接載入而刪除。

## Procedure

### 使用 fixture

1. 測試只將必要 project 素材複製到獨立暫存專案，再於暫存專案執行寫入與 recovery。input 與 expected 分別載入，不整包複製到專案，不回寫 fixture 原件。唯讀流程可直接使用明確 project 根目錄，並驗證前後 bytes 不變。
2. `fixture_support::copy_fixture_sources` 接受實際 project 根目錄，複製完整的 Source Snapshot triplets：manifest、完成 marker 與原始 content bytes。必要 project 或 Source 缺失會失敗。Migration provenance 或 ledger-only 等明確無 Snapshot 情境不呼叫此 helper，也不虛構 Source。

### 新增案例

1. 新案例依既有 domain／feature 與情境命名，僅建立實際需要的 input、expected 或 project。`latest-attempts` 的預期結果由正式 project 資料衍生，沒有虛構 input。
2. 需要 Source 時保留真實 Snapshot triplets；刻意損毀 bytes 與歷史指紋不重新序列化或修復。
3. 現行指紋變更使用既有 fixture builders，完整準備後才更新一個 case。

## Validation

1. 在 `rust/` 執行完整驗證：

   ```sh
   cargo fmt --all -- --check
   cargo check --workspace --all-targets --locked
   cargo test --workspace --locked
   ```

2. 驗證前後確認 fixture payload bytes 不變；歷史 digest 不以重新計算的新值替換原始證據。
