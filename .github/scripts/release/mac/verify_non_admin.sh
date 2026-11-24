#!/usr/bin/env bash
set -euo pipefail
repo="$(pwd -P)"
python="$(command -v python3)"
stage="$(mktemp -d /private/tmp/awk-permission-smoke.XXXXXXXX)"
trap 'sudo -n rm -rf -- "$stage"' EXIT
"$python" -B .github/scripts/release/permission_smoke.py --stage "$repo" "$stage" macos-arm64
mkdir "$stage/home" "$stage/tmp"
uid="$(id -u nobody)"
# The runner only prepares the fixture. The existing non-admin UID executes
# the exact same package test as Windows, without sudo inside the installer.
sudo -n chown -R nobody "$stage"
sudo -n -u nobody /usr/bin/env -i HOME="$stage/home" TMPDIR="$stage/tmp" PATH=/usr/bin:/bin CI=true \
    "$python" -B "$stage/release/permission_smoke.py" --macos "$stage" "$stage/home" "$uid" "$GITHUB_SHA"
