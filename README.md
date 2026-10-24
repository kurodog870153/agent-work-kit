# Codex Workflow Kit

Codex Workflow Kit 透過 `$work` 協助你規劃需求、拆分任務並執行工作。

## 1. 功能

1. 整理原始需求，確認工作類型、需要的技能與驗收條件。
2. 將需求拆分為可執行的任務，安排相依關係與驗證步驟。
3. 在正式建立或修訂任務前，提供預覽供你審查與核准。
4. 依整體授權連續執行範圍內任務與開發修正，保留既有修改、工具產物及可追溯的程序／AI 審查／使用者確認證據。
5. 保存討論進度，方便稍後接續規劃。
6. 分析不相容或損壞的既有工作資料，保留原始紀錄並協助遷移。

## 2. 使用方式

### 2.1 選擇工作模式

```text
$work <mode> -- <request>
```

在 Codex 對話中輸入以下指令，選擇適合目前工作的模式：

1. `task`：從新需求開始，確認驗收條件並建立任務。
2. `revise`：修改已建立的需求或任務，檢查對後續工作的影響。
3. `migration`：分析不相容或損壞的既有工作資料，審查遷移方案。
4. `execute`：在驗證與授權後執行指定任務。

### 2.2 指令範例

```text
$work task -- 建立一個包含 UI、frontend 與 backend 的網站
$work revise -- 調整 example 的 TASK-001 驗收條件並檢查下游影響
$work migration -- 分析 example 的既有文件，重建損壞的 Task 與 Execution 關聯
$work execute -- 執行 example 的正式 TASK-001
```

建議先用 `task` 規劃，審查並核准任務後再用 `execute` 執行。

需求改變時使用 `revise`；舊資料無法正常使用時，先用 `migration` 分析。

### 2.3 需求編號與授權

規劃時會保存原始需求，後續討論沿用同一份紀錄。

Work 會請你提供並確認需求編號，例如 `example`，用來區分不同需求的資料。

Work 也可能在一般對話中建議適合的模式，經你確認模式與需求後才啟動。

啟動 Work 本身不代表授權寫入檔案或執行任務；請在審查範圍與操作內容後給予授權。

### 2.4 保存討論進度

1. 確認需求編號、專案位置及討論進度的保存範圍。
2. Work 會保存已確認事項、待決定的方案、問題與續談位置。
3. 用下列指令接續已保存的討論；將 `example` 換成你的需求編號。
3. 保存討論的授權不代表授權建立正式任務或執行工作。

```text
$work task -- resume example
```

## 3. 工作資料與中斷處理

### 3.1 資料存放位置

工作資料保存在專案的 `outputs/work/` 下，依需求編號分類：

1. `sources/<requirement-id>/`：原始需求與附件。
2. `tasks/<requirement-id>/`：正式任務及驗收條件。
3. `executions/<requirement-id>/`：執行結果、修正與驗證紀錄。
4. `discussions/<requirement-id>/`：目前討論進度與歷史版本。
5. `transactions/`：各次操作的輸入、請求與回應，包含尚未確認需求編號的操作。
6. `runtime/`：執行期間的鎖定與暫存資料，由 Work 管理。

### 3.2 備份工作資料

備份時請保留完整的工作資料。

原始需求、討論歷史與執行紀錄會持續保存。

專案的 Git ignore 預設排除 `transactions/`，若要備份操作輸入與回應，請一併保存該目錄。

### 3.3 中斷與舊資料處理

操作中斷時，請提供需求編號與錯誤訊息，讓 Work 檢查並透過對應流程復原。

不要手動修改工作紀錄或刪除鎖定檔，部分執行證據可能表示命令已經啟動，需要先確認結果，才能決定後續處理。

舊版工作資料若遭到拒絕，請使用 `migration` 分析，依審查結果保留原始資料並進行遷移。

## 4. 必要環境

1. 預編譯套件支援
   - macOS ARM64
   - Windows x64 MSVC

2. 從原始碼安裝
   1. 須有 Rust 與 Cargo 1.99.1 以上及平台建置工具
   2. macOS 需要 Xcode Command Line Tools
   3. Windows 需要 MSVC linker
   4. 首次建置可能下載 `rust/Cargo.lock` 指定的 crate

3. Package 與 Downloader 安裝不需要 Rust、Cargo 或建置工具。

## 5. 安裝

### 5.1 選擇安裝方式

macOS ARM64 與 Windows x64 MSVC 都有三種安裝方式。

1. 一般使用者可先選「下載最新正式版」。
2. 已下載 ZIP 或需要離線安裝時選「從 ZIP 安裝」。
3. 自行建置時選「從原始碼安裝」。

| 安裝方式     | macOS ARM64                                                                                                                            | Windows x64 MSVC                                                                                                               |
| -------- | -------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| 下載最新正式版  | [下載 `.command`](https://github.com/kurodog870153/codex-workflow-kit/releases/latest/download/install-work-from-github-release.command) | [下載 `.bat`](https://github.com/kurodog870153/codex-workflow-kit/releases/latest/download/install-work-from-github-release.bat) |
| 從 ZIP 安裝 | 解壓後執行 `install-work-from-package.command`                                                                                              | 解壓後執行 `install-work-from-package.bat`                                                                                          |
| 從原始碼安裝   | 在本專案執行 `os-scripts/mac/install-work-from-source.command`                                                                               | 在本專案執行 `os-scripts/windows/install-work-from-source.bat`                                                                       |

### 5.2 下載最新正式版

1. 從上表下載所屬平台的安裝腳本。
2. 執行腳本。
   1. 它會下載內建固定 Release tag 對應的 ZIP，核對外部 SHA-256 並檢查套件，再啟動安裝器。
   2. 執行 `--help` 只會顯示說明，不會下載。

macOS 下載後若無法執行腳本：

1. 確認腳本來自上表的 GitHub Release，並核對該 Release 的 checksum。
2. 在腳本所在目錄執行 `chmod +x install-work-from-github-release.command`，再執行 `./install-work-from-github-release.command`；或直接執行 `bash install-work-from-github-release.command`。
3. 若 macOS 顯示未簽章提示，確認來源與完整性後，依系統提示允許執行。請保留系統安全防護。


### 5.3 從 ZIP 安裝

1. 從同一個 [Release](https://github.com/kurodog870153/codex-workflow-kit/releases/latest) 下載所屬平台的 ZIP 與 `SHA256SUMS.txt`。
2. 驗證 ZIP 的 SHA-256，通過後才解壓。
3. 在解壓後的根目錄執行上表的 Package 入口。離線時也可用此方式安裝。

Windows 和 macOS 可直接複製執行的驗證命令，以及 Package 入口的安裝範例，請依序參照 [Release 安裝指引](docs/release/README.md)。

### 5.4 從原始碼安裝

在本專案的 checkout 執行上表的 Source 入口。這種方式需要 Rust、Cargo 1.99.1 以上及平台建置工具；首次建置可能下載 crate。Package 與 Downloader 方式不需要這些建置工具。

### 5.5 安裝選項與預設設定

三種入口都可在檔名後加參數；常用選項如下：

1. `--help`：顯示用法；`--list-skills`：列出可用的 Skill ID。
2. `--skills general|all|<逗號分隔的 Skill ID>`：選擇要安裝的 Skill。
3. `--global-agents` 或 `--no-global-agents`：啟用或停用 Global `AGENTS.md` 安裝；停用不會刪除既有檔案。
4. `--work-install-dir <path>`：指定最終 Work 目錄。
5. `--global-agents-install-dir <path>`：指定 Global `AGENTS.md` 所在目錄，檔名固定為 `AGENTS.md`。
6. `--dry-run`：先檢查並列出預計動作，不修改正式目標；`--verbose`：顯示詳細資訊。
7. `--non-interactive`：不詢問；遇到需要確認的情況會拒絕安裝。

Work 預設安裝在 `~/.agents/skills/work`，Global `AGENTS.md` 預設安裝在 `~/.codex/AGENTS.md`；互動模式可分別選擇自訂目錄。首次安裝或首次更新預設選 `general`，並啟用 Global `AGENTS.md`；已有有效安裝狀態時沿用上次設定。舊 `--install-dir` 已移除。安裝器不會安裝 Rust，也不會永久修改 PATH。

### 5.6 更新與需要確認的情況

更新時重新執行原本的入口。新內容會先在目標同層準備並驗證，成功後再切換；失敗時會嘗試復原。

1. 既有 Work 目錄缺少有效的 `install-state.json` 時，安裝器會檢查目錄並要求明確確認後才首次更新；有未知檔案或不安全連結時會拒絕接管。
2. Global `AGENTS.md` 內容與套件不同時，須確認後才會替換。
3. 若使用 `--non-interactive`，上述需要確認的情況會直接拒絕安裝。

### 5.7 安全提示與失敗處理

下載的腳本與執行檔可能出現作業系統安全提示。確認來源與 checksum 後，再依平台正常程序決定是否執行。若安裝器回報無法自動復原，請保留錯誤訊息指出的目錄，檢查後再處理。參數錯誤回傳 Exit 2；下載、驗證或安裝失敗回傳非零碼。

### 5.8 常見錯誤

1. Downloader 無法下載或 checksum 不符：檢查連線與 Release 資產，或手動下載同一 Release 的 ZIP 與 `SHA256SUMS.txt` 並比對後改用 Package 入口。
2. Package 回報平台或 manifest 不符：確認 ZIP 對應 macOS ARM64／Windows x64 MSVC，並重新取得完整套件。
3. 首次更新遭拒絕：檢查目標中的未知檔案或連結；保留資料，整理後重試或選擇其他目錄。非互動模式無法代替必要確認。
4. Source 建置失敗：確認 Rust、Cargo、平台 linker／SDK，以及 crate 是否可取得；已安裝的 Work 不會因建置失敗而直接被覆寫。

## 6. 進一步閱讀

1. [Work 流程與操作指引](skills/work/references/instruction-loading.md)。
2. [既有資料診斷與遷移指引](skills/work/references/instruction-loading/artifact-migration.md)。
3. [Rust 架構與執行產物管理](docs/work/rust/architecture.md)。
4. [工作指引架構](docs/work/instruction/architecture.md)。

## 7. 清理本機 Codex 資料與 ChatGPT 快取

> [!WARNING]
> 清理腳本會永久刪除本機 Codex 工作階段、封存工作階段、產生的圖片、歷史紀錄與相關狀態資料，並清除找到的 ChatGPT 桌面版快取。執行前請先關閉 Codex 與 ChatGPT，並確認不需要保留這些 Codex 資料。

1. Windows：`os-scripts/windows/clean-codex-data.bat`
2. macOS：`os-scripts/mac/clean-codex-data.command`

ChatGPT 僅清除快取，保留登入與設定。確認畫面顯示的刪除範圍後，再依提示操作。

## 8. License

This project is licensed under the
[PolyForm Noncommercial License 1.0.0](LICENSE).

You may use, modify, and distribute this project only for purposes permitted
under the License.

Commercial use is not licensed under the PolyForm Noncommercial License 1.0.0.
A separate commercial license or other written permission from the licensor is
required before any commercial use.

Examples of commercial use that require separate authorization include,
but are not limited to:

- Using this project as part of paid software development or consulting work
- Using this project to provide services to paying clients
- Incorporating this project into a commercial product, service, or SaaS
- Selling access to, or commercially distributing, this project or a derivative work

These examples are provided for clarification only and do not modify or replace
the terms of the License. In the event of any inconsistency, the terms in the
[LICENSE](LICENSE) file govern.

For commercial licensing inquiries, please contact the project author.
