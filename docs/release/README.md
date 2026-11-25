# Agent Work Kit Release 安裝

每個 ZIP 對應一個 Host 與平台，包含預編譯 Work CLI 與 Skill CLI、Work 與 Tooling Skill、兩個 Skill 共用的 Shared 指令資源、選用的 Global Instructions 及安裝器。Host 可選 Codex 或 Claude Code。

## 1. 下載

從同一個 [GitHub Release](https://github.com/kurodog870153/agent-work-kit/releases) 下載 `SHA256SUMS.txt` 及適用平台的 ZIP：

1. Codex Windows x64：`agent-work-kit-v<版本>-codex-windows-x64-msvc.zip`
2. Claude Code Windows x64：`agent-work-kit-v<版本>-claude-windows-x64-msvc.zip`
3. Codex macOS ARM64：`agent-work-kit-v<版本>-codex-macos-arm64.zip`
4. Claude Code macOS ARM64：`agent-work-kit-v<版本>-claude-macos-arm64.zip`

`<版本>` 是 Release tag 去掉開頭 `v` 的版本。把所選 ZIP 與 `SHA256SUMS.txt` 放在同一目錄。

## 2. 驗證 ZIP

先切換到下載目錄，再將所屬平台的整段命令貼上執行；不必修改版本字串。

命令會從 `SHA256SUMS.txt` 找出所選 Host 與平台的 ZIP 項目，再比對 SHA-256。先將下列 `<host>` 換成 `codex` 或 `claude`。

找不到項目、ZIP 或雜湊不符時，不要解壓或執行安裝器。

正式 Release 發佈流程會為四個 ZIP、兩個下載器及 `SHA256SUMS.txt` 建立 Artifact Attestation。下載後也應核對資產的來源與 digest。

### Windows x64：在 PowerShell 執行

```powershell
& {
  $hostName = '<host>'
  $entries = @(Select-String -LiteralPath .\SHA256SUMS.txt -Pattern "^[0-9a-fA-F]{64}\s+agent-work-kit-v.+-$hostName-windows-x64-msvc\.zip$" |
    ForEach-Object { $_.Line })
  if ($entries.Count -ne 1) { throw 'SHA256SUMS.txt 中找不到唯一的 Windows ZIP 項目' }
  $parts = $entries[0] -split '\s+', 3
  $expected, $zip = $parts[0], $parts[1]
  if ($expected -cnotmatch '^[0-9a-fA-F]{64}$') { throw 'SHA-256 格式不正確' }
  $actual = (Get-FileHash -LiteralPath ".\$zip" -Algorithm SHA256 -ErrorAction Stop).Hash
  if ($actual -ine $expected) { throw "SHA-256 不符：$zip" }
  Write-Host "SHA-256 驗證通過：$zip"
}
```

### macOS ARM64：在 Terminal 執行

```sh
(
host='<host>'
count=$(awk -v host="$host" '$2 ~ "^agent-work-kit-v.+-" host "-macos-arm64[.]zip$" { n++ } END { print n+0 }' SHA256SUMS.txt) || exit 1
if [ "$count" -ne 1 ]; then
  echo 'SHA256SUMS.txt 中找不到唯一的 macOS ZIP 項目' >&2
  exit 1
fi
zip=$(awk -v host="$host" '$2 ~ "^agent-work-kit-v.+-" host "-macos-arm64[.]zip$" { print $2 }' SHA256SUMS.txt) || exit 1
expected=$(awk -v host="$host" '$2 ~ "^agent-work-kit-v.+-" host "-macos-arm64[.]zip$" { print $1 }' SHA256SUMS.txt) || exit 1
if ! printf '%s\n' "$expected" | grep -Eq '^[[:xdigit:]]{64}$'; then
  echo 'SHA-256 格式不正確' >&2
  exit 1
fi
if [ ! -f "$zip" ]; then
  echo "找不到 ZIP：$zip" >&2
  exit 1
fi
actual=$(shasum -a 256 "$zip" | awk '{ print $1 }') || exit 1
if [ "$actual" != "$expected" ]; then
  echo "SHA-256 不符：$zip" >&2
  exit 1
fi
echo "SHA-256 驗證通過：$zip"
)
```

`SHA256SUMS.txt` 只證明下載檔與清單一致；仍須確認兩者來自預期的 GitHub Release。

## 3. 解壓並安裝

### 3.1 解壓並切換目錄

驗證 ZIP 通過後解壓，並在 PowerShell 或 Terminal 切換到解壓後的根目錄。下列命令都從該目錄執行。

### 3.2 預覽安裝動作

以下範例只列出預計動作，不修改正式安裝目標。Work、Tooling、Shared 與 Work／Skill CLI 一律共同安裝。

Windows PowerShell：

```powershell
.\install-agent-work-kit-from-package.bat --dry-run
```

macOS Terminal：

```sh
./install-agent-work-kit-from-package.command --dry-run
```

macOS 若顯示「權限不足」：

1. 先依上一節確認 ZIP 與 `SHA256SUMS.txt` 來自預期的 Release，且雜湊相符。
2. 在解壓後的根目錄執行 `chmod +x install-agent-work-kit-from-package.command`，再重試上方命令。
3. 若不想更改權限，可直接執行 `bash install-agent-work-kit-from-package.command --dry-run`。

### 3.3 執行安裝

確認預計動作後，移除 `--dry-run` 執行安裝。

Windows PowerShell：

```powershell
.\install-agent-work-kit-from-package.bat
```

macOS Terminal：

```sh
./install-agent-work-kit-from-package.command
```

安裝器每次共同安裝 Work、Tooling、Shared 與 Work／Skill CLI，實體資源集中在 `<install_dir>/skills` 與 `<install_dir>/bin`，狀態檔位於 `<install_dir>/install-state.json`。Host Skills 目錄只安裝 Work／Tooling 的實體 `SKILL.md`，由 Installer 注入 Bootstrap、Root、必要 CLI 及安裝狀態等絕對路徑，Runtime 指令樹不保留入口副本；Release ZIP 的 `README.md` 不會被複製到正式安裝目錄。互動模式會詢問 Host；非互動模式必須指定 `--host`，且須與 ZIP 相符。Codex 的 Global Instructions 目標是 `~/.codex/AGENTS.md`，Claude Code 是 `~/.claude/CLAUDE.md`。安裝器會先驗證包內 manifest、Host、平台及 binary，再修改正式目標。

### 3.4 可用參數

其他參數接在入口檔名後面，多個參數以空白分隔：

1. `--help` 或 `-h`：顯示用法，不安裝。
2. `--host codex|claude`：選擇 Host；非互動模式必填。
3. `--global-instructions yes|no`：啟用或停用 Global Instructions。
4. `--install-dir <path>`：指定 agent-work-kit 的**實體安裝根目錄**；不支援自訂 Host Skills 探索目錄。路徑含空白時須加引號。
5. `--instructions-dir <path>`：指定 Global Instructions 所在目錄；路徑含空白時須加引號。
6. `--non-interactive`：不讀取互動輸入；需要確認時會拒絕安裝並保留原目標。
7. `--dry-run`：列出預計動作，不修改正式安裝目標。
8. `--verbose`：顯示較詳細的診斷資訊。

### 3.5 安裝時的確認

安裝器會依目標目錄的狀態處理：

1. 目標目錄不存在或為空時，視為首次安裝；沒有 `install-state.json` 是正常情況。
2. 目標已有內容，但 `install-state.json` 缺少、損毀或不符合目前規格時，安裝器保留原內容並拒絕接管；本安裝器不解析舊版安裝，也不提供升級、遷移或專用清理。
3. 已選擇安裝 Global Instructions，且目標檔案內容與 ZIP 中的版本不同時，安裝器會顯示差異並詢問是否取代。拒絕取代時保留原檔案。
4. 使用 `--non-interactive` 時，安裝器不會詢問；遇到 Global Instructions 差異等必須確認的情況，會拒絕安裝並保留原目標。

### 3.6 安裝狀態與重新安裝

安裝器在 `<install_dir>/install-state.json` 管理唯一的正式安裝狀態，以 `install_dir` 記錄套件根目錄，Skill／CLI 位置由它推導；探索目錄由 Host 固定決定，不另記錄 `host_skills_dir`。Manifest 以 `host-skills/work/SKILL.md`、`host-skills/tooling/SKILL.md` 記錄生成入口的 SHA-256。不需要手動建立或修改。

| Host | 預設狀態檔位置 |
| --- | --- |
| Codex | `~/.agents/agent-work-kit/install-state.json` |
| Claude Code | `~/.claude/agent-work-kit/install-state.json` |

預設根目錄為上表狀態檔所在的 `agent-work-kit` 目錄。使用 `--install-dir <path>` 自訂後，重新安裝必須再提供**相同**安裝位置；不反向搜尋或自動遷移。Codex 實體入口為 `~/.agents/skills/{work,tooling}/SKILL.md`，Claude Code 為 `~/.claude/skills/{work,tooling}/SKILL.md`；不建立 Shared 入口。原始碼及 ZIP 保留模板，正式 Runtime 不安裝模板副本。重新安裝先驗證 State、Manifest 與實體入口，成功後更新入口；缺失、被修改、未知內容或非法連結均安全拒絕，不自行修復。重新安裝時沿用同一 Host 的有效 Global Instructions 設定，也可用 `--global-instructions yes|no` 指定；首次安裝預設啟用。狀態檔存在不代表安裝有效，還須驗證所有受管理資源。

### 3.7 安全警告與錯誤處理

這些 binary 未經 Apple 或 Microsoft 平台簽章，作業系統可能顯示安全警告。確認下載來源、Checksum 與 Attestation 後，再依平台提供的正常操作方式允許執行。

若驗證回報 checksum、manifest 或平台錯誤，請重新下載同一 Release 的正確 ZIP 與 `SHA256SUMS.txt`。因未知內容、無有效安裝狀態、已占用的 Work／Tooling 探索入口或不安全連結被拒絕時，保留原資料並由使用者自行處理；不覆蓋任何第三方 Skill。安裝鎖以實際作業系統使用者為範圍，同一使用者並行安裝會安全拒絕；不同使用者可同時安裝至各自獨立的位置。不支援不同使用者共用同一實體安裝根目錄、Host 探索目錄或 Global Instructions 目標，請勿將自訂目錄設為共用安裝位置。目標不可寫入時安裝安全中止，不自動提權或修改系統安全設定。舊版使用 Junction／Symbolic Link 的安裝與舊版交易不相容，不自動遷移或刪除。先停止相依 Host 操作、備份並確認既有資源歸屬；若仍有舊版未完成交易，使用相應舊版安全復原。確認舊版兩個入口與實體安裝根目錄確屬本套件後，由使用者手動處理或移至安全備份位置，再執行新安裝。不得刪除不明入口、連結指向的其他資源、第三方 Skill 或使用者資料。非互動模式不會跳過必要確認。復原失敗時保留安裝器指出的現有目錄與錯誤訊息。


Host 的原生 Skill 探索設定不由 Installer 修改；若自行變更 Host 探索目錄，須由使用者自行配置，檔案已安裝不代表真實 Host 一定已探索成功。

### 3.8 安裝中斷

1. 保留安裝目錄、狀態檔、安裝器指出的交易資料及錯誤訊息。
2. 重新執行相同的安裝命令，使用原本的安裝位置與選項，不加上 `--dry-run`，因為預覽不會處理待復原的交易。
3. 確認安裝器回報的結果；只有通過驗證的內容才視為安裝完成。若無法辨識檔案、發現外部修改或復原失敗，停止操作並保留現場與錯誤訊息，不要自行刪除安裝目錄或交易資料。

### 互動式安裝結束

Windows 與 macOS 的 Source、Package、Downloader 在互動式安裝成功、失敗、取消或 Dry Run 後，保留最後訊息並等待 Enter 後退出；不需區分雙擊或終端機執行。使用 `--non-interactive`、CI 或沒有可用互動輸入時不等待。退出碼仍反映原始執行結果。
