#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
source "$repo_root/os-scripts/mac/installer/install/tooling-install.sh"
source "$repo_root/os-scripts/mac/installer/transaction/tooling.sh"

scratch="$(mktemp -d "$repo_root/.tooling-transaction-test.XXXXXXXX")"
[[ "$scratch" == "$repo_root"/.tooling-transaction-test.* ]] || exit 1
trap 'rm -rf -- "$scratch"' EXIT
target="$scratch/tooling"
source_tooling="$repo_root/skills/tooling"
txn="$(mktemp -d "$scratch/.tooling-install.XXXXXXXX")"
printf '%s\n' "$target" > "$txn/target-path.txt"
installer_tooling_transaction_safe "$txn" "$source_tooling"
installer_tooling_transaction_pending "$target"
printf keep > "$txn/unknown.txt"
if installer_tooling_transaction_safe "$txn" "$source_tooling"; then
    printf 'FAIL: unsafe Tooling transaction was accepted.\n' >&2; exit 1
fi
if installer_tooling_cleanup "$txn" "$source_tooling"; then
    printf 'FAIL: unsafe Tooling transaction was removed.\n' >&2; exit 1
fi
[[ "$(cat -- "$txn/unknown.txt")" == keep ]] || exit 1
rm -- "$txn/unknown.txt"
installer_tooling_cleanup "$txn" "$source_tooling"
[[ ! -e "$txn" ]] || exit 1
printf 'macOS Tooling transaction cases passed.\n'
