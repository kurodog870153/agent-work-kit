#!/usr/bin/env bash
set -euo pipefail

version=$(cat VERSION)
for host in codex claude; do
  zip="dist/agent-work-kit-v${version}-${host}-macos-arm64.zip"
  echo "Verifying macOS release installation: ${zip}"
  sandbox-exec -p '(version 1)(allow default)(deny network*)' \
    python3 -B .github/scripts/release/release_install_e2e.py \
    --zip "$zip" --checksums dist/SHA256SUMS.txt --platform macos-arm64 --source-install
done
echo "Verifying macOS dual Host coexistence"
sandbox-exec -p '(version 1)(allow default)(deny network*)' \
  python3 -B .github/scripts/release/release_install_e2e.py \
  --zip "dist/agent-work-kit-v${version}-codex-macos-arm64.zip" \
  --peer-zip "dist/agent-work-kit-v${version}-claude-macos-arm64.zip" \
  --checksums dist/SHA256SUMS.txt --platform macos-arm64
