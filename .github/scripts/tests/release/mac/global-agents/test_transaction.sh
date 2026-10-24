#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
source "$repo_root/os-scripts/mac/installer/global-agents/policy.sh"
source "$repo_root/os-scripts/mac/installer/global-agents/transaction.sh"
scratch="$(mktemp -d "$repo_root/.issue95-global-transaction.XXXXXX")"
[[ "$(cd "$scratch" && pwd -P)" == "$scratch" && "$scratch" == "$repo_root"/.issue95-global-transaction.* ]] || exit 1
trap 'rm -rf -- "$scratch"' EXIT
source_file="$scratch/source.md"
target="$scratch/AGENTS.md"
printf 'new\n' > "$source_file"
printf 'old\r\n' > "$target"
installer_global_agents_plan "$source_file" "$target" 1 0 1
source_hash="$installer_global_agents_source_sha256"
target_hash="$installer_global_agents_target_sha256"
for failure in publish restore; do
    txn="$scratch/$failure"
    mkdir -- "$txn"
    if installer_global_agents_stage "$source_file" "$target" "$txn" replace "$source_hash" "$target_hash" "$failure"; then
        printf 'FAIL: injected %s failure accepted.\n' "$failure" >&2; exit 1
    fi
    if [[ "$failure" == restore ]]; then mv -- "$txn/global-previous" "$target"; fi
    [[ "$(installer_global_agents_hash "$target")" == "$target_hash" ]] || exit 1
done
txn="$scratch/success"
mkdir -- "$txn"
installer_global_agents_stage "$source_file" "$target" "$txn" replace "$source_hash" "$target_hash"
[[ "$(installer_global_agents_hash "$target")" == "$source_hash" ]] || exit 1
installer_global_agents_undo "$target" "$txn" replace "$source_hash" "$target_hash"
[[ "$(installer_global_agents_hash "$target")" == "$target_hash" ]] || exit 1
rm -- "$target"
installer_global_agents_plan "$source_file" "$target" 1 1 0
txn="$scratch/new"
mkdir -- "$txn"
installer_global_agents_stage "$source_file" "$target" "$txn" install "$source_hash" ''
installer_global_agents_undo "$target" "$txn" install "$source_hash" ''
[[ ! -e "$target" ]] || exit 1
printf 'macOS Global AGENTS transaction cases passed.\n'
