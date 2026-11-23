# Issue #131 — 安裝根目錄整合與 Host Skill 探索隔離：Task 規劃

> Issue：[ #131 — refactor(installer): consolidate install root and isolate Host Skill discovery via symlinks](https://github.com/kurodog870153/agent-work-kit/issues/131)  
> 依據：Issue #131 已確認的 D01–D12，以及 21 項 Issue 驗收 A01–A05、B01–B06、C01–C04、D01–D06。  
> 狀態：**待實作**。本文僅定義 Task 與驗收方式，不代表已完成修改；不在此階段勾選 Issue Checkbox。

## 1. 目標與已確認邊界

避免 Skill CLI 以 Host Skills 父目錄為安裝來源時，把第三方 `find-skills`／外部 Symbolic Link 納入掃描而觸發 `mixed_install`。將所有受管理資源集中在 `install_dir`，Host 探索目錄只保留 Work、Tooling 兩個受管理 Directory Symbolic Link；**不可放寬套件內部來源、symlink／junction、Shared／Skill 邊界及來源逃逸的安全拒絕**。

### 1.1 目標安裝結構

```text
<HostSkillsDir>/                       # Codex: ~/.agents/skills; Claude Code: ~/.claude/skills
├── work    -> <install_dir>/skills/work       # 絕對目標 Directory Symbolic Link
├── tooling -> <install_dir>/skills/tooling
└── find-skills/                       # 第三方 Skill，不納入本套件管理

<install_dir>/                          # 預設或由使用者自訂
├── skills/
│   ├── work/SKILL.md
│   ├── tooling/SKILL.md
│   └── shared/bootstrap.md
├── bin/
│   ├── work[.exe]
│   └── skill[.exe]
└── install-state.json
```

1. Installer 公開 `--install-dir <path>` **取代** `--skills-dir`，不保留舊選項；Codex 預設 `~/.agents/agent-work-kit`，Claude Code 預設 `~/.claude/agent-work-kit`。Host Skills 目錄固定由 Host 決定，不可自訂，也不得修改 Host 原生探索設定。
2. 安裝 State 放於 `<install_dir>/install-state.json`，僅保存路徑欄位 `install_dir`，不保存 `skill_install_dir`、`cli_install_dir`、`host_skills_dir` 或連結專屬欄位。檔案清單／SHA-256 需覆蓋實際受管理資源（`skills/...`、`bin/...`）；Global Instructions 保留既有獨立政策。
3. Skill CLI 現有 `--install` 在安裝後須指向 `<install_dir>/skills`；`skills/work`、`skills/tooling`、`skills/shared` 各維持原有獨立來源邊界。Repository 原始碼 `skills/{work,tooling,shared}` 結構不變。
4. macOS／Windows **統一絕對目標 Directory Symbolic Link**，Windows 不使用 Junction 或 fallback。有效重新安裝沿用原連結，不重建；錯誤、缺失、未知連結則拒絕，不自動修復。
5. 自訂 `install_dir` 重新安裝須再次指定；未提供時使用 Host 預設位置，不從連結反向定位。安裝位置變更不屬於自動遷移。
6. **不辨識、解析或遷移舊版**；未知／占用內容必須保護且安全拒絕，不能以不支援舊版為由忽略目標安全。
7. Release ZIP 根目錄保留 `README.md`；`install_dir` 不安裝 README，Manifest 不納入 README。`shared` 不建立 Host Skill 探索連結。
8. 保留全域鎖、Preflight／Prepare／Apply／Verify／Commit、安裝完整性、選用 Global Instructions、跨磁碟安全復原，以及 Codex／Claude × macOS ARM64／Windows x64 四組驗收。真實 Host 實測不是必要條件；Host 模擬不得冒充實測。

## 2. 專案現況與修改缺口

本規劃依 Issue 與 GitHub `main` 上的 Installer、State、交易、Release 與測試現況整理；執行時須再以實際分支檢查差異。

| 現況 | 缺口與必要處理 |
| --- | --- |
| Installer Parser／Help 使用 `--skills-dir` | 調整公開參數、預設與互動選項、Source／Package／Downloader 傳遞 |
| 安裝核心把 Markdown Skill 裝至 Host Skills 父目錄、Binary 放同層另一目錄 | 集中實體資源至 `install_dir/{skills,bin}`，Host Skills 只管理兩個探索連結 |
| State 位於 Host Skills 目錄，欄位 `skill_install_dir`、`cli_install_dir` | 改為 `install_dir`、單一 State 路徑、可驗證 Manifest 與重新安裝判定 |
| Preflight／Transaction 針對舊實體位置驗證及 Apply | 新的受管理路徑、Symbolic Link、未知來源拒絕與安全 Recovery |
| Host 模擬與 Installer E2E 把 Host Skills 目錄傳給 Skill CLI | 改成 `<install_dir>/skills`，另模擬 Host 從 `<HostSkillsDir>/{work,tooling}` 探索 |
| ZIP 已包含 README，Installer 未安裝 README | 繼續保留此界線，更新文件、驗證與封裝測試，不新增 README 安裝流程 |

### 2.1 相關檔案

1. `os-scripts/common/installer/options.tsv`、`os-scripts/{mac,windows}/installer/cli/parse-options.*`。
2. `os-scripts/{mac,windows}/installer/{install,state,target,transaction}/*`。
3. `.github/scripts/release/{release_package.py,release_install_e2e.py,release_downloader_e2e.py,host_simulation.py}`。
4. `.github/scripts/ci/rust_validation_steps.py`、`.github/scripts/tests/release/**` 及必要 Workflow。
5. `docs/release/README.md`、`ai-maintainer-instructions/release/installer-architecture.md`、`ai-maintainer-instructions/skills/architecture.md`；`skills/{work,tooling}/SKILL.md` 與 `skills/shared/bootstrap.md` 只在來源定位確實需要時修改。

## 3. Task 排程與依賴

| Task | 可獨立驗收的交付成果 | 前置 |
| --- | --- | --- |
| T01 | 新公開安裝參數與路徑推導契約 | 無 |
| T02 | 單一 State、Manifest、安全 Preflight | T01 |
| T03 | 集中式實體安裝、連結與跨位置交易／Recovery | T01、T02 |
| T04 | Source／Package／Downloader／Release 契約整合 | T03 |
| T05 | 安全拒絕、Host 探索模擬、四組跨平台 E2E | T03、T04 |
| T06 | 文件、CI 證據、最終一致性檢查 | T01–T05 |

Task 可按相依關係實作、測試和驗收，不強制每個 Task 對應獨立 Commit 或 PR；驗收時不得只憑 Task 完成直接勾選 Issue。

### T01 — 公開安裝參數與路徑契約

**目標：** 確定單一安裝根目錄與固定 Host 探索路徑，統一各平台公開參數。

**工作範圍：**
1. `options.tsv`、macOS／Windows Parser 與 Help 使用 `--install-dir`，拒絕舊 `--skills-dir`；統一缺參數、重複參數與非法路徑處理。
2. Host 決定預設根目錄及探索目錄；自訂根目錄不改變探索目錄；同步 Source、Package、Downloader 參數語意。
3. 保留 Dry Run 無副作用、非互動、Host、Global Instructions 等既有契約。
4. 將固定的新路徑語意同步至必要 Installer 架構說明，供 T02–T05 使用。

**驗收：**
- [ ] **T01-C01** 兩平台的 Help、Parser、Source／Package／Downloader 公開參數一致，`--install-dir` 可用且 `--skills-dir` 無相容別名。
- [ ] **T01-C02** Codex／Claude 的預設與自訂安裝根、固定探索位置正確，不擅改 Host 設定。
- [ ] **T01-C03** Dry Run、非互動、Global Instructions 參數、非法路徑拒絕測試通過。

**驗證方式：** 平台 Parser 單元測試、`--help`、非法輸入、路徑推導及 Dry Run 測試。

### T02 — 單一 Install State、Manifest 與安全 Preflight

**目標：** 將管理權威限制在 `install_dir`，確認有效新安裝與探索連結；保護未知內容。

**工作範圍：**
1. State Schema 改為單一 `install_dir`，並將唯一 State 移到根目錄；更新 Manifest 的來源、相對名稱與完整性驗證。
2. 預檢 `install_dir/{skills,bin,install-state.json}`、Host 身分、路徑與 Global Instructions／其他 Host 的安全隔離，不處理舊版狀態。
3. 固定探索入口 `work`、`tooling`：首次安裝只接受未占用位置；重裝需完整驗證既有 Directory Symbolic Link 的類型、絕對目標、實際解析結果及相應 State。
4. 保留 Work／Tooling／Shared 各自來源邊界、非法內部連結、Junction、`mixed_install`、來源逃逸與未知內容的安全拒絕；第三方 Skills 不能進本套件 Manifest。

**驗收：**
- [ ] **T02-C01** State 只有 `install_dir` 路徑欄位，State 與 Manifest 驗證實際檔案且拒絕錯誤 Host／版本／位置。
- [ ] **T02-C02** 預檢拒絕未知目標、被占用的探索入口、錯誤／損毀／缺失連結，不刪除、覆蓋或接管使用者資料。
- [ ] **T02-C03** 有效新安裝可被識別，正確重裝路徑與現有連結可驗證；不反向尋找自訂安裝位置。
- [ ] **T02-C04** 第三方 `find-skills` 不屬於管理範圍，內部不安全來源仍 fail-closed；預檢唯讀、無舊版相容或修復。

**驗證方式：** State、Manifest、Preflight 單元與整合測試，包含未知目標、Host 共存、內外連結負向案例。

### T03 — 集中式安裝、探索連結與交易／Recovery

**目標：** 在 macOS／Windows 上可安全首次安裝、重新安裝、提交與恢復。

**工作範圍：**
1. 安裝 Work、Tooling、Shared 至 `<install_dir>/skills`，Binary 至 `<install_dir>/bin`，State 至根目錄，不安裝 README。
2. 首次安裝建立指向實際 Skill 目錄的絕對目標 Directory Symbolic Link；Windows 不能建立時失敗，不使用 Junction。
3. 有效重裝沿用連結不重建，無效連結拒絕且不自動修復；安裝後核對目標、Manifest、State 與 CLI 可執行性。
4. 連結建立、Skills、Binary、State 與選用 Global Instructions 均受同一全域鎖及邏輯原子交易約束；驗證完成前不得將結果視為成功。
5. 修改交易、Rollback／Recovery，涵蓋 Prepare／Apply／Verify／Commit、提交後中斷、安裝根目錄與 Host 探索目錄不同父目錄／磁碟；避免依賴跨磁碟原子 move，並保護原有內容。

**驗收：**
- [ ] **T03-C01** 兩平台首次安裝符合完整布局，Work／Tooling 的絕對目標 Directory Symbolic Link 均能驗證。
- [ ] **T03-C02** 有效連結重裝沿用，不因更新重建；異常、未知或權限不足時安全拒絕且不降級為 Junction。
- [ ] **T03-C03** 所有受管理資源與選用 Global Instructions 在同一交易中正確提交或安全復原，未完成結果不被當作有效安裝。
- [ ] **T03-C04** 故障／中斷／Recovery、跨目錄及 Windows 跨磁碟測試可重現，未知內容不受損。

**驗證方式：** macOS Shell／Windows PowerShell Installer 測試、故障注入、崩潰恢復、權限失敗、不同磁碟情境。

### T04 — Source／Package／Downloader／Release 整合

**目標：** 所有公開安裝入口都產生相同的新版安裝結果，保留原有 Release 完整性契約。

**工作範圍：**
1. Source、Package 共用各平台的新版安裝核心與預檢／交易契約；Downloader 正確傳遞 `--install-dir`。
2. 更新 Release 封裝、下載器及其測試對安裝結果的預期；保留 ZIP 根 README、必要資源、預編譯 Binary 與內部 Manifest、外部 SHA-256。
3. 保留 Host／平台／版本一致性檢查、正常／異常下載、解壓及來源驗證。
4. 不安裝 README、不新增舊版選項相容、不自行升版／正式發布 Release。

**驗收：**
- [ ] **T04-C01** Source、Package、Downloader 依相同公開參數、路徑與 State 契約運作，能以預設／自訂位置正確安裝。
- [ ] **T04-C02** ZIP／Manifest／外部 SHA-256、版本、Host、平台、Binary 安全檢查未回歸。
- [ ] **T04-C03** ZIP 根 README 保留，正式安裝無 README；Downloader／封裝不再依賴舊 `--skills-dir` 或路徑。

**驗證方式：** Release Package 單元測試、Downloader E2E、Source／Package 產物比對及錯誤封裝拒絕案例。

### T05 — 第三方共存、Host 模擬與四組 E2E

**目標：** 以已安裝的真實 CLI Binary 驗證，透過 Host 固定探索連結能使用同組 Skill／Shared，第三方連結不再造成無關的 `mixed_install`。

**工作範圍：**
1. Host 模擬先從 `<HostSkillsDir>/{work,tooling}` 讀取入口，再以 `<install_dir>/skills` 呼叫 Skill CLI 的 `validate`、`discover`、`resolve` 及適用的 `switch`。
2. 建立含第三方 `find-skills`（含外部 Symbolic Link）的情境，證明不被掃描／接管；本套件非法內部 symlink／junction、來源越界仍拒絕。
3. 整合 Installer E2E、Downloader E2E、Host Simulation 和平台測試，覆蓋 Codex／Claude Code × macOS ARM64／Windows x64、預設／自訂位置、重新安裝、未知入口與 Host 共存。
4. 測試 Global Instructions 開／關、Shared 唯一性、每個 Skill 獨立啟用及真實 Binary Smoke Test。
5. 腳本模擬輸出保留 Host、平台、Binary、安裝目錄及執行證據，明確區分模擬與真實 Host 實測。

**驗收：**
- [ ] **T05-C01** Host 經固定探索入口發現 Work／Tooling；Skill CLI 指向實體來源且代表性指令與 Shared 解析通過。
- [ ] **T05-C02** 第三方來源不引起誤判，但內部來源逃逸、不安全連結及異常安裝仍拒絕。
- [ ] **T05-C03** 四種 Host × 平台安裝、重新安裝、失敗復原、實際 Binary Smoke Test 及適用自訂／跨磁碟場景通過。
- [ ] **T05-C04** Host／Global Instructions 隔離正確；證據可重現，且不宣稱真實 Host 已實測。

**驗證方式：** `release_install_e2e.py`、`release_downloader_e2e.py`、`host_simulation.py`、相關 Installer 測試及 GitHub Actions。

### T06 — 文件同步、CI 與整體驗收

**目標：** 讓使用者文件、架構、程式、封裝與測試一致，提供 Issue #131 可追溯的驗收證據。

**工作範圍：**
1. 更新 `docs/release/README.md`、Installer 與 Skill 架構文件及必要導覽：新布局、預設與 `--install-dir`、Host 固定入口、Link 失敗、安全拒絕與 Recovery。
2. 全面檢查仍有效的 Installer Script、Help、CI、測試與模擬中的過時 `--skills-dir`、State 欄位及原安裝位置；歷史紀錄不視為有效契約。
3. 執行必要 Rust／Installer／Release／Downloader／Host 模擬／E2E 測試，保留 GitHub Actions 與腳本證據。
4. 逐項查核 Issue 的 A01–A05、B01–B06、C01–C04、D01–D06；未通過或無證據不得勾選。Task 驗收不取代 Issue 的獨立整體驗收。
5. 禁止無關 Work 業務重構、舊版遷移、不必要的 Host 設定引擎、安裝 README 或自行發布 Release。

**驗收：**
- [ ] **T06-C01** 文件、Help、Script、State／Manifest、Runtime 解析路徑一致，無過時的有效入口。
- [ ] **T06-C02** 必要 CI、四組平台／Host、已安裝 Binary、Installer／Release／Downloader 安全與 Host 模擬測試通過。
- [ ] **T06-C03** Issue #131 全部 21 項驗收已對照實際證據；仍未通過的項目有明確紀錄且保持未勾選。
- [ ] **T06-C04** 無未授權發布、版本變更、README 安裝、資料遷移或安全邊界降級。

**驗證方式：** 文件／契約比對、完整 CI、四組實體安裝驗證報告及 Issue Checklist 對照。

## 4. Issue 驗收對應

| Issue Checklist | 主要對應 Task |
| --- | --- |
| A01 | T01、T04 |
| A02 | T03、T04、T05 |
| A03 | T02、T05 |
| A04 | T02、T03、T05 |
| A05 | T05 |
| B01 | T02、T05 |
| B02 | T02、T05 |
| B03 | T02、T03、T05 |
| B04 | T02、T03、T05 |
| B05 | T02、T03、T05 |
| B06 | T02、T06 |
| C01 | T03、T05 |
| C02 | T03、T05 |
| C03 | T03、T05 |
| C04 | T03、T05 |
| D01 | T04、T05、T06 |
| D02 | T05 |
| D03 | T04、T06 |
| D04 | T01–T06 |
| D05 | T01、T06 |
| D06 | T06 |

## 5. 完成原則

1. T01–T06 是以可獨立驗收成果拆分的執行任務；完成個別 Task 不自動代表 Issue 符合全部驗收。
2. 按相依順序作業，執行時重新檢查當前分支與檔案；除非發現真正影響需求的新衝突，否則不重複詢問既有決策。
3. Source、Package、Downloader 和兩平台須符合同一安裝契約；來源、Host、Global Instructions、安全交易等既有要求不可退化。
4. 本文 Checkbox 預設全部未勾選；任務文件建立本身不構成實作或測試通過證據。
