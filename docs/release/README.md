# Work Release 安裝

本 ZIP 包含對應平台的預編譯 Work binary、完整 Work 與 Tooling Skill、Global `AGENTS.md` 與安裝器。安裝器將 Work 與 Tooling 安裝在同一 Skill 父目錄。

## 1. 下載

從同一個 [GitHub Release](https://github.com/kurodog870153/agent-work-kit/releases) 下載 `SHA256SUMS.txt` 及適用平台的 ZIP：

1. Windows x64 使用 `work-v<版本>-windows-x64-msvc.zip`
2. macOS ARM64 使用 `work-v<版本>-macos-arm64.zip`

`<版本>` 是 Release tag 去掉開頭 `v` 的版本；例如 tag `v1.2.3`，檔名即以 `work-v1.2.3-` 開頭。把兩個檔案放在同一目錄。

## 2. 驗證 ZIP

先切換到下載目錄，再將所屬平台的整段命令貼上執行；不必修改版本字串。

命令會從 `SHA256SUMS.txt` 找出該平台唯一的 ZIP 項目，再比對下載檔的 SHA-256。

找不到項目、ZIP 或雜湊不符時，不要解壓或執行安裝器。

如有 Artifact Attestation，也應核對資產的來源與 digest。

### Windows x64：在 PowerShell 執行

```powershell
& {
  $entries = @(Select-String -LiteralPath .\SHA256SUMS.txt -Pattern '^[0-9a-fA-F]{64}\s+work-v.+-windows-x64-msvc\.zip$' |
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
count=$(awk '$2 ~ /^work-v.+-macos-arm64[.]zip$/ { n++ } END { print n+0 }' SHA256SUMS.txt) || exit 1
if [ "$count" -ne 1 ]; then
  echo 'SHA256SUMS.txt 中找不到唯一的 macOS ZIP 項目' >&2
  exit 1
fi
zip=$(awk '$2 ~ /^work-v.+-macos-arm64[.]zip$/ { print $2 }' SHA256SUMS.txt) || exit 1
expected=$(awk '$2 ~ /^work-v.+-macos-arm64[.]zip$/ { print $1 }' SHA256SUMS.txt) || exit 1
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

以下範例只列出預計動作，不修改正式安裝目標。Work 與 Tooling 均完整安裝。

Windows PowerShell：

```powershell
.\install-work-from-package.bat --dry-run
```

macOS Terminal：

```sh
./install-work-from-package.command --dry-run
```

macOS 若顯示「權限不足」：

1. 先依上一節確認 ZIP 與 `SHA256SUMS.txt` 來自預期的 Release，且雜湊相符。
2. 在解壓後的根目錄執行 `chmod +x install-work-from-package.command`，再重試上方命令。
3. 若不想更改權限，可直接執行 `bash install-work-from-package.command --dry-run`。

### 3.3 執行安裝

確認預計動作後，移除 `--dry-run` 執行安裝。

Windows PowerShell：

```powershell
.\install-work-from-package.bat
```

macOS Terminal：

```sh
./install-work-from-package.command
```

安裝器每次安裝完整 Work 與 Tooling。首次安裝預設啟用 Global `AGENTS.md`；已有有效安裝狀態時，沿用原本的 Global `AGENTS.md` 設定。安裝器會先驗證包內 manifest、平台及預編譯 binary，再修改正式目標。

### 3.4 可用參數

其他參數接在入口檔名後面，多個參數以空白分隔：

1. `--help` 或 `-h`：顯示用法，不安裝。
2. `--global-agent`／`--no-global-agent`：啟用／停用 Global `AGENTS.md` 安裝；兩者不可同時使用。
3. `--skills-install-dir <path>`：指定 Work 與 Tooling 的共同父目錄；路徑含空白時須加引號。
4. `--global-agent-install-dir <path>`：指定 Global `AGENTS.md` 所在目錄；路徑含空白時須加引號。
5. `--non-interactive`：不讀取互動輸入；需要確認時會拒絕安裝並保留原目標。
6. `--dry-run`：列出預計動作，不修改正式安裝目標。
7. `--verbose`：顯示較詳細的診斷資訊。舊 `--install-dir`、`--work-install-dir`、`--skills` 與 `--list-skills` 不再接受。

### 3.5 安裝時的確認

安裝器會依目標目錄的狀態處理：

1. Work 與 Tooling 目錄不存在或為空時，視為首次安裝；沒有父層 `install-state.json` 是正常情況。
2. Work 或 Tooling 目錄已有內容，但共同父目錄的 `install-state.json` 缺少、損毀或不符合目前規格時，安裝器保留原目錄並拒絕安裝。舊版子目錄狀態檔不作為更新依據。
3. 已選擇安裝 Global `AGENTS.md`，且目標檔案內容與 ZIP 中的版本不同時，安裝器會顯示差異並詢問是否取代。拒絕取代時保留原檔案。
4. 使用 `--non-interactive` 時，安裝器不會詢問；遇到 Global `AGENTS.md` 差異等必須確認的情況，會拒絕安裝並保留原目標。

### 3.6 安全警告與錯誤處理

這些 binary 未經 Apple 或 Microsoft 平台簽章，作業系統可能顯示安全警告。確認下載來源、Checksum 與 Attestation 後，再依平台提供的正常操作方式允許執行。

若驗證回報 checksum、manifest 或平台錯誤，請重新下載同一 Release 的正確 ZIP 與 `SHA256SUMS.txt`。因未知內容、無有效父層狀態或不安全連結被拒絕時，保留原資料並整理目標或改選安裝目錄；非互動模式不會跳過必要確認。復原失敗時保留安裝器指出的現有目錄與錯誤訊息。
