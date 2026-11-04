#!/usr/bin/env bash
set -euo pipefail

version=$(cat VERSION)
zip="dist/work-v${version}-macos-arm64.zip"
echo "Verifying macOS release installation: ${zip}"
sandbox-exec -p '(version 1)(allow default)(deny network*)' \
  python3 -B .github/scripts/release/release_install_e2e.py \
  --zip "$zip" --checksums dist/SHA256SUMS.txt --platform macos-arm64 --source-install
