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

安裝器每次共同安裝 Work、Tooling、Shared 與 Work／Skill CLI；CLI 安裝在 Skills 目錄旁的 `agent-work-kit/bin`，與 Skill 目錄分離。互動模式會詢問 Host；非互動模式必須指定 `--host`，且須與 ZIP 相符。Codex 的 Global Instructions 目標是 `~/.codex/AGENTS.md`，Claude Code 是 `~/.claude/CLAUDE.md`。安裝器會先驗證包內 manifest、Host、平台及 binary，再修改正式目標。

### 3.4 可用參數

其他參數接在入口檔名後面，多個參數以空白分隔：

1. `--help` 或 `-h`：顯示用法，不安裝。
2. `--host codex|claude`：選擇 Host；非互動模式必填。
3. `--global-instructions yes|no`：啟用或停用 Global Instructions。
4. `--skills-dir <path>`：指定 Work、Tooling 與 Shared 的共同父目錄；CLI 目錄為其同層的 `agent-work-kit`。路徑含空白時須加引號。
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

安裝器會在 Work、Tooling 與 Shared 的共同父目錄管理唯一的 `install-state.json`，用來辨識安裝版本、位置、Host、Global Instructions 設定與安裝結果。不需要手動建立或修改。

| Host | 預設狀態檔位置 |
| --- | --- |
| Codex | `~/.agents/skills/install-state.json` |
| Claude Code | `~/.claude/skills/install-state.json` |

使用 `--skills-dir <path>` 時，Skill、Shared 與狀態檔會移至指定的共同父目錄，CLI 目錄隨之位於其同層。重新安裝時會沿用同一 Host 的有效 Global Instructions 設定，仍可用 `--global-instructions yes|no` 指定此次選擇；首次安裝預設啟用。狀態檔存在不代表安裝有效，安裝器還會檢查實際內容。

### 3.7 安全警告與錯誤處理

這些 binary 未經 Apple 或 Microsoft 平台簽章，作業系統可能顯示安全警告。確認下載來源、Checksum 與 Attestation 後，再依平台提供的正常操作方式允許執行。

若驗證回報 checksum、manifest 或平台錯誤，請重新下載同一 Release 的正確 ZIP 與 `SHA256SUMS.txt`。因未知內容、無有效安裝狀態或不安全連結被拒絕時，保留原資料並整理目標或改選安裝目錄；非互動模式不會跳過必要確認。復原失敗時保留安裝器指出的現有目錄與錯誤訊息。

### 3.8 安裝中斷

1. 保留安裝目錄、狀態檔、安裝器指出的交易資料及錯誤訊息。
2. 重新執行相同的安裝命令，使用原本的安裝位置與選項，不加上 `--dry-run`，因為預覽不會處理待復原的交易。
3. 確認安裝器回報的結果；只有通過驗證的內容才視為安裝完成。若無法辨識檔案、發現外部修改或復原失敗，停止操作並保留現場與錯誤訊息，不要自行刪除安裝目錄或交易資料。
