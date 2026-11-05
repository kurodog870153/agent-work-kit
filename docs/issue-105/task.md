# Issue #105：Codex／Claude Code 共用封裝與安裝任務

來源：[Issue #105](https://github.com/kurodog870153/agent-work-kit/issues/105)。任務狀態依實際驗證更新。

## 盤點範圍與實作前現況

| 範圍 | 相關檔案與目錄 | 現況與待處理相依 |
| --- | --- | --- |
| 共用規範與 Skill | 根目錄 `AGENTS.md`、`skills/work/`、`skills/tooling/`、`rust/Cargo.toml`、`rust/crates/work/`、`rust/crates/tooling/` | 尚無 `instructions/global-instructions.md`；Work、Tooling 與 Rust 須維持單一來源。 |
| 公開安裝入口 | `os-scripts/mac/install-work-from-{source,package,github-release}.command`、`os-scripts/windows/install-work-from-{source,package,github-release}.bat` | 三種入口均帶舊名稱；Downloader 與 Package 必須共用 Host／Platform 身分契約。 |
| 共用契約 | `os-scripts/common/installer/options.tsv`、`os-scripts/common/installer/install-state.json` | 選項含 `--global-agent` 等 Codex 語意，狀態尚無 Host 身分。 |
| 平台安裝器 | `os-scripts/{mac,windows}/installer/{cli,install,state,target,transaction,global-agents,package,lock}/` | 安裝核心、狀態、預檢、交易與復原目前以 Codex 目標及 `AGENTS.md` 為主；須集中 Host 差異並保留平台安全機制。 |
| 封裝與下載 | `.github/scripts/release/{release_package.py,release_package_integrity.py,release_install_e2e.py,release_downloader_e2e.py,downloader_fixture.py}` | 套件清單、名稱、內部完整性與端到端測試均需涵蓋四個 Host／Platform 組合。 |
| 發布驗證 | `.github/scripts/release/{release_publication_checks.py,release_post_publish_checks.py,release_verification_report.py,release_version_availability.py,release_tag_creation.py}`、`.github/scripts/release/{mac,windows}/verify_install.*` | 外部 checksum、Attestation、資產集合及發布報告仍以舊命名與兩個 ZIP 為基礎。 |
| CI 與測試 | `.github/workflows/{build-and-validate-packages,pr-release-validation,pr-release-credentials-validation,publish-release,rust-validation}.yml`、`.github/scripts/tests/release/`、`rust/crates/{work,tooling}/*/tests/` | 套件建置、安裝器單元與端到端測試、Windows batch 換行及 Rust 回歸檢查須一起更新。 |
| 使用與架構文件 | `README.md`、`docs/release/{README,architecture,install-state,recovery}.md`、`docs/rust/architecture.md`、`docs/skills/` | 安裝命令、目標、狀態、復原及破壞性變更說明須與新契約一致。 |

下表保留規劃時的基線，現行實作狀態以任務條件及驗證紀錄為準。實作前須重新檢查上述檔案與同名引用；盤點表示目前可見的責任區，不將未列出的同根因檔案排除在各任務範圍外。保留使用者既有安裝資料；不要求舊腳本、ZIP、參數或狀態相容。

## 已確認的安裝介面決策

1. 三種安裝入口均接受 `--host codex|claude`，每次只安裝一個 Host；不提供 `all`。互動模式未指定時顯示無預設值的 Host 選單；非互動模式未指定時報錯。Package 入口的選擇仍須與 ZIP 身分核對，錯配即拒絕。
2. 使用者層級預設位置依官方文件：Codex Skill 為 `~/.agents/skills/`、規範為 `~/.codex/AGENTS.md`；Claude Code Skill 為 `~/.claude/skills/`、規範為 `~/.claude/CLAUDE.md`。依 Host 分別保存並驗證 `install-state`，狀態新增 `host` 欄位。
3. 自訂路徑參數為 `--skills-dir`、`--instructions-dir`，只作用於本次選定的 Host。全域規範選擇使用 `--global-instructions yes|no`；與 `--global-instructions no` 同時指定 `--instructions-dir` 時報參數衝突。
4. 未指定 `--global-instructions` 時，互動模式詢問：首次安裝預設 `yes`，更新預選該 Host 的既有狀態。非互動模式首次安裝預設 `yes`，更新沿用既有狀態。
5. 舊版 `install-state` 若沒有 `host` 欄位，保留原狀、拒絕接管並提示人工處理；不自動補欄位或遷移。
6. 公開腳本名稱為 `install-agent-work-kit-from-{source,package,github-release}`，macOS 使用 `.command`、Windows 使用 `.bat`。

官方路徑依據：[OpenAI Docs：Skills](https://learn.chatgpt.com/docs/build-skills)、[OpenAI Docs：AGENTS.md](https://learn.chatgpt.com/docs/agent-configuration/agents-md)、[Claude Code Skills](https://code.claude.com/docs/en/skills)、[Claude Code Instructions](https://code.claude.com/docs/en/memory)。內部資料結構、函式及 Manifest Schema 依既有架構和測試於實作階段決定。

## 依序執行的任務

每項任務只有一個可驗收成果。任務中列出的檔案是預計修改範圍；寫入前須重新讀取實際內容與相同引用，跨檔案不可分割的變更須同組套用。各任務完成時執行直接相關測試；後續整體驗收不取代前面任務的驗證。

本次授權依序執行 Windows 與 macOS 全部任務。Windows 環境可執行的 macOS 邏輯測試照常執行；需要 macOS ARM64 主機或實際 Host 的驗收若無法執行，保留未完成並記錄原因，不以靜態檢查代替。

執行紀錄（Windows 主機）：四套件封裝單元測試、Release 相關 35 項 Python 測試、Windows 安裝核心與 Package 入口 fixture、Windows 鎖及 Tooling 交易測試、Downloader／入口 9 項測試均通過；其中 1 項 macOS 測試略過。Windows 核心 fixture 涵蓋兩個 Host 的 Source 安裝；兩個 Host 的實際 ZIP Downloader 端到端測試通過，含 Package／Downloader 中斷復原。Claude 首次重啟測試揭露 Tooling 交易缺少 Host 傳遞，修正後完整重驗通過。macOS 四個安裝腳本 Bash 語法、CLI 解析、Global Instructions policy／交易及 Node 狀態測試通過；macOS 原生鎖測試在 Windows 編譯器因缺少 POSIX API 無法執行，目標預檢需 macOS binary，原生安裝及實際 Host 載入亦尚未驗證。Host CLI 模式測試通過，尚不能替代實際 Host 載入。發布工作流 YAML 可解析，遠端 CI 尚未執行。曾因沙箱拒絕檔案操作而在沙箱外重跑 Windows fixture；失敗執行留下的暫存目錄未擅自清理。

### 1. 建立 Host 安裝狀態契約

相依：無。讓父層安裝狀態記錄並驗證 `host`、版本與目標；沒有 `host` 的舊狀態及不相容狀態保留原狀並拒絕。

預計檔案：`os-scripts/common/installer/install-state.json`、`os-scripts/{mac,windows}/installer/state/`、兩平台 state 測試。先查所有狀態讀寫，再同組修改模板、驗證器及測試。

- [x] 兩平台均能讀寫與驗證 Host 身分；錯誤 Host、版本、目標及舊狀態均在寫入前拒絕。
- [x] Codex 現有資料及未知狀態不被靜默覆寫；兩平台狀態測試通過。

驗證：Windows 狀態測試、macOS 狀態邏輯 Node 測試、Windows Installer Core fixture 均通過；macOS 原生安裝驗證留待任務 14、15。

### 2. 讓 Installer Core 選擇各 Host 的目標

相依：任務 1。將 Skill 與規範預設位置、`--skills-dir` 和 `--instructions-dir` 的目標解析集中於 Host 設定；維持單一安裝核心。

預計檔案：`os-scripts/{mac,windows}/installer/{install/install-core.*,target/preflight.*,install/work-install.*,install/tooling-install.*}`、兩平台 target/install 測試。驗證預設、自訂、跨 Host 路徑衝突與既有內容保護。

- [ ] Codex 與 Claude Code 各用官方預設目標；自訂路徑只影響所選 Host。
- [ ] 兩 Host 的目標與狀態隔離；不安全或被占用目標在寫入前拒絕，相關測試通過。

### 3. 建立雙 Host 的共用規範安裝交易

相依：任務 1、2。從同一規範來源安裝 `AGENTS.md` 或 `CLAUDE.md`，保留可選安裝、差異確認、安全交易與 Recovery。搬移根目錄 `AGENTS.md` 時，所有來源讀取及直接測試引用須整組更新，避免留下失效中間狀態。

預計檔案：`AGENTS.md`、`instructions/global-instructions.md`、`os-scripts/{mac,windows}/installer/{install,global-agents,transaction}/`、`.github/scripts/release/release_package.py`、直接引用規範來源的測試。先列出根目錄引用再完整套用。

- [x] 唯一共用來源為 `instructions/global-instructions.md`；封裝與安裝不讀根目錄 `AGENTS.md`。規範依使用者後續授權合併重複條文、精簡執行敘述並移除圖片預覽檔名規則。
- [ ] 兩 Host 的規範檔名、差異確認、停用選擇及失敗復原均有測試並通過。

### 4. 實作兩平台共通 CLI 選項與互動行為

相依：任務 1 至 3。同步調整共用選項表及 macOS／Windows 解析器，實作已確認的 Host 選單、規範選擇、參數衝突及非互動規則。

預計檔案：`os-scripts/common/installer/options.tsv`、`os-scripts/{mac,windows}/installer/cli/parse-options.*`、兩平台 cli 測試、安裝核心中的互動選擇。兩平台同組變更及驗證。

- [x] `--host`、`--skills-dir`、`--instructions-dir`、`--global-instructions yes|no` 符合已確認決策；舊參數不再受支援。
- [ ] 無預設 Host 選單、非互動缺少 Host 拒絕、規範選擇與衝突處理、Help／Dry Run／verbose 均通過兩平台測試。

### 5. 更新 Source 安裝入口

相依：任務 4。將 macOS／Windows Source 腳本更名為 `install-agent-work-kit-from-source`，從原始碼建置共用 Rust binary，再將所選 Host 交給同一安裝核心。

預計檔案：`os-scripts/{mac,windows}/install-work-from-source.*`、`.github/scripts/tests/release/{test_installer_entries.py,mac/install/,windows/install/}` 及直接引用入口的檢查。同步修改名稱與引用後執行 Source 安裝測試。

- [ ] 兩平台 Source 入口可安裝任一 Host，參數與 Help 正確，舊名稱不作為公開入口。
- [ ] 建置或預檢失敗不改動正式目標；Source 入口相關測試通過。

### 6. 定義並驗證 Package 身分

相依：任務 1 至 4。讓 Package 的 Host、Platform、Version、來源身分與內部完整性可驗證，所選 Host 或執行平台不符時拒絕。

預計檔案：`os-scripts/{mac,windows}/installer/package/verify-integrity.*`、`.github/scripts/release/{release_package.py,release_package_integrity.py}`、package integrity 測試。Manifest 的具體欄位由現有架構與測試決定。

- [ ] 兩平台會在安裝前驗證套件身分、檔案清單及內容完整性。
- [ ] Host／Platform 錯配、變造或缺漏套件被拒絕，且目標未變；相關測試通過。

### 7. 更新 Package 安裝入口

相依：任務 4、6。將兩平台 Package 腳本更名為 `install-agent-work-kit-from-package`，驗證套件後進入共用安裝核心；互動選擇必須與 ZIP 身分核對。

預計檔案：`os-scripts/{mac,windows}/install-work-from-package.*`、`.github/scripts/tests/release/{test_installer_entries.py,mac/install/,windows/package/,windows/install/}`。驗證正確與錯誤 Host、互動及非互動情境。

- [ ] 兩平台 Package 入口可安裝所屬 Host，Host 或 Platform 錯配在寫入前拒絕。
- [ ] 新腳本名稱、選單、參數傳遞及舊名稱退役的測試通過。

### 8. 產生四個獨立 Release ZIP

相依：任務 3、6、7。由單一版本、Commit、Rust Workspace 及 Skill 來源產生四個完整 ZIP。

預計檔案：`.github/scripts/release/{release_package.py,release_package_integrity.py}`、`.github/scripts/tests/release/{test_release_package.py,test_release_package_integrity.py}`。實際建置套件並檢查每個 ZIP 的內容與身分。

- [ ] 四個 ZIP 符合 `agent-work-kit-vX.Y.Z-{host}-{platform}.zip`，各自含 Work、Tooling、規範、binary 與安裝資源。
- [ ] 四個 ZIP 的版本、Commit、來源與內部 Manifest 驗證通過；封裝不依賴 `work-v*`。

### 9. 更新 GitHub Release Downloader 入口

相依：任務 4、7、8。將兩平台 Downloader 更名為 `install-agent-work-kit-from-github-release`，依所選 Host 與平台下載對應 ZIP，驗證外部 checksum 後轉交 Package。

預計檔案：`os-scripts/{mac,windows}/install-work-from-github-release.*`、`.github/scripts/release/{release_downloader_e2e.py,downloader_fixture.py}`、`.github/scripts/tests/release/{test_downloader_entry.py,common/}`。驗證引數原樣轉交及拒絕錯誤下載。

- [ ] 兩平台能選取正確 Host ZIP；checksum、Host 或 Platform 錯誤時不安裝。
- [ ] 新名稱、互動／非互動及 Downloader 端到端測試通過。

### 10. 更新四套件建置與 PR 驗證 CI

相依：任務 5 至 9。CI 建置並驗證四個 ZIP、兩平台安裝入口及相關回歸；本任務只調整驗證流程，不觸發遠端執行。

預計檔案：`.github/workflows/{build-and-validate-packages,pr-release-validation,pr-release-credentials-validation,rust-validation}.yml`、`.github/scripts/release/{mac/verify_install.sh,windows/verify_install.ps1}`、工作流檢查測試。靜態檢查工作流及本地可執行項目。

- [ ] CI 矩陣涵蓋四個 Host／Platform 組合與必要 Rust、Installer、Release 測試。
- [ ] 任一必要套件或驗證失敗會使 CI 失敗；工作流檢查通過。

### 11. 更新 Release 發布與事後驗證

相依：任務 8 至 10。發布資產集合、外部 `SHA256SUMS.txt`、Attestation、來源 Commit、Draft／Publish／Post-publish 驗證與報告均涵蓋四個 ZIP 和新腳本；本任務不實際發布。

工作流相依決策：`verify and attest release assets` 只接在套件 checksum 驗證完成後；所需 tag、version 沿該相依路徑傳遞，不額外直接依賴先前驗證 job。

預計檔案：`.github/workflows/publish-release.yml`、`.github/scripts/release/{release_publication_checks.py,release_post_publish_checks.py,release_verification_report.py,release_version_availability.py,release_tag_creation.py}`、對應 `.github/scripts/tests/release/`。以本地模擬資產檢查成功與缺漏情境。

- [ ] 預期資產、checksum、Attestation 與來源 Commit 檢查涵蓋全部四個 ZIP。
- [x] 缺少任一必要資產或驗證失敗時，不會產生整體成功結論；本地發布檢查測試通過。

### 12. 驗證 Codex Windows x64 安裝

相依：任務 1 至 11。在 Windows x64 以 Codex 目標驗證三種入口、安裝、更新、非法輸入、既有資料保護與失敗復原。

預計檔案：必要的 `.github/scripts/tests/release/windows/`、`.github/scripts/release/release_install_e2e.py` 與既有驗收記錄。執行 Codex Windows 測試並記錄環境。

- [x] Codex Windows 的三種入口、安裝、更新及必要拒絕／Recovery 情境實測通過。

### 13. 驗證 Claude Code Windows x64 安裝

相依：任務 1 至 11。以 Claude Code 目標執行與任務 12 對等的 Windows 驗收。

預計檔案：必要的 `.github/scripts/tests/release/windows/`、`.github/scripts/release/release_install_e2e.py` 與既有驗收記錄。執行 Claude Code Windows 測試並記錄環境。

- [x] Claude Code Windows 的三種入口、安裝、更新及必要拒絕／Recovery 情境實測通過。

### 14. 驗證 Codex macOS ARM64 安裝

相依：任務 1 至 11。在 macOS ARM64 以 Codex 目標驗證三種入口、安裝、更新、非法輸入、既有資料保護與失敗復原。

預計檔案：必要的 `.github/scripts/tests/release/mac/`、`.github/scripts/release/release_install_e2e.py` 與既有驗收記錄。執行 Codex macOS 測試並記錄環境。

- [ ] Codex macOS 的三種入口、安裝、更新及必要拒絕／Recovery 情境實測通過。

### 15. 驗證 Claude Code macOS ARM64 安裝

相依：任務 1 至 11。以 Claude Code 目標執行與任務 14 對等的 macOS 驗收。

預計檔案：必要的 `.github/scripts/tests/release/mac/`、`.github/scripts/release/release_install_e2e.py` 與既有驗收記錄。執行 Claude Code macOS 測試並記錄環境。

- [ ] Claude Code macOS 的三種入口、安裝、更新及必要拒絕／Recovery 情境實測通過。

### 16. 驗證雙 Host 共存與隔離

相依：任務 12 至 15。在兩平台先後安裝並更新兩個 Host，驗證安裝內容、狀態、差異確認、鎖與 Recovery 彼此隔離；測試未知及舊版狀態不會被接管。

預計檔案：必要的 `.github/scripts/tests/release/{mac,windows}/` 與既有驗收記錄。執行跨 Host 情境及相關 Installer 回歸。

- [ ] 兩平台的雙 Host 共存、獨立更新、狀態隔離與失敗復原有實測證據。
- [ ] 未知、舊版或不安全狀態被拒絕且既有資料保留。

### 17. 驗證 Codex 的 Skill 載入與 CLI 操作

相依：任務 12、14、16。在實際 Codex 中載入 Work、Tooling Skill 並操作共用 CLI；不以安裝腳本成功代替 Host 使用驗收。

預計檔案：必要的 `rust/crates/{work,tooling}/*/tests/` 與既有驗收記錄。執行相關 Rust 回歸並記錄 Codex 實測結果。

- [ ] Codex 能載入 Work、Tooling Skill 並完成代表性 CLI 操作；相關回歸與實測證據齊備。

### 18. 驗證 Claude Code 的 Skill 載入與 CLI 操作

相依：任務 13、15、16。在實際 Claude Code 中載入 Work、Tooling Skill 並操作共用 CLI；驗證共享來源在此 Host 的載入行為。

預計檔案：必要的 `rust/crates/{work,tooling}/*/tests/` 與既有驗收記錄。執行相關 Rust 回歸並記錄 Claude Code 實測結果。

- [ ] Claude Code 能載入 Work、Tooling Skill 並完成代表性 CLI 操作；相關回歸與實測證據齊備。

### 19. 更新使用與復原文件

相依：任務 12 至 18。說明四個 ZIP、三種入口、已確認參數、預設／自訂目標、錯誤拒絕、復原與舊版人工處理。

預計檔案：`README.md`、`docs/release/{README,install-state,recovery}.md`。逐項對照實際腳本、Help 與驗收結果。

- [ ] 文件命令及路徑可對照已驗證產物；`docs/release/` 只描述目前版本的安裝、狀態及復原方式。

### 20. 更新 Installer 與 Release 架構文件

相依：任務 1 至 19。讓 Installer、Release、Global Instructions 架構與最終實作一致。

預計檔案：`docs/release/architecture.md`、必要的 `docs/rust/`、`docs/skills/`。對照實際模組、Host 差異及資料流，檢查文件連結。

- [ ] 架構文件正確描述單一 Rust／Skill／Installer Core、Host 差異、四套件及規範來源。

### 21. 清理現行舊名稱引用

相依：任務 1 至 20。搜尋並修正 Script、CI、測試、Manifest、文件及範例中仍指向舊公開介面的引用；歷史證據保留原貌。

預計檔案：全庫搜尋找到的現行引用。逐項分類、修正並重驗直接相關項目，不順便修改無關內容。

- [ ] 現行引用不再誤導使用者；相關檢查通過，歷史資料未被改寫。

## 執行邊界

1. 本次不修改 Work 正式資料驗證、Tooling Policy、Host 原生權限系統，也不建立 Host 專用 Rust Workspace 或重複 Skill 實作。
2. Task 的 checkbox 僅在對應程式、文件與實際驗證證據齊備後勾選；AI 檢查、測試與使用者確認分別記錄。
3. 遠端 GitHub Release 的建立、修改或發布不屬於本任務文件的寫入授權，執行前需另行授權。
