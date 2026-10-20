# Work Release 安裝

本 ZIP 包含對應平台的預編譯 Work binary、完整 Work Skill、Global `AGENTS.md` 與安裝器。安裝不需要 Rust、Cargo、Git、SDK、GitHub CLI 或網路。

1. 從同一個 GitHub Release 下載對應平台的 ZIP 與 `SHA256SUMS.txt`，核對 ZIP 的 SHA-256；如有 Artifact Attestation，也應核對資產的來源與 digest。
2. 解壓 ZIP 到本機目錄。macOS ARM64 執行 `install-release.command`；Windows x64 執行 `install-release.bat`。安裝器會先驗證包內所有檔案，再詢問是否安裝 Global `AGENTS.md`，最後安裝 Work。
3. Global `AGENTS.md` 預設不安裝。選擇安裝時可使用預設的 `.codex` 目錄或自訂位置；建立目錄及覆蓋現有檔案都需確認。Work 預設安裝至 `.agents/skills/work`，也可指定既有目錄，安裝至其 `skills/work`。
4. 覆蓋安裝會把完整舊檔案保存在畫面顯示的復原目錄 `previous`。若替換失敗，安裝器會嘗試還原；若還原失敗，依錯誤訊息中的復原目錄手動處理。Global `AGENTS.md` 成功後若 Work 安裝失敗，前者不會回滾。
5. 安裝後執行 `.agents/skills/work/scripts/work --help`（Windows 為 `.agents\skills\work\scripts\work.exe --help`）確認預編譯程式可啟動。自訂安裝位置時，改用該位置的 `skills/work/scripts` 路徑。

這些 binary 未經 Apple 或 Microsoft 平台簽章，作業系統可能顯示安全警告。確認下載來源、Checksum 與 Attestation 後，再依平台提供的正常操作方式允許執行。
