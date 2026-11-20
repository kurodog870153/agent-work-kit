#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
source "$repo_root/os-scripts/mac/installer/state/state.sh"
source "$repo_root/os-scripts/mac/installer/global-instructions/policy.sh"
scratch="$(mktemp -d "$repo_root/.installer-test-global.XXXXXX")"
trap 'rm -rf -- "$scratch"' EXIT
source_file="$scratch/source.md"; target="$scratch/AGENTS.md"
printf 'new\n' > "$source_file"
installer_global_plan "$source_file" "$target" 0 1 ''; [[ "$installer_global_action" == skip ]]
installer_global_plan "$source_file" "$target" 1 1 ''; [[ "$installer_global_action" == install ]]
cp -- "$source_file" "$target"
installer_global_plan "$source_file" "$target" 1 1 ''; [[ "$installer_global_action" == unchanged ]]
printf 'old\n' > "$target"
installer_global_plan "$source_file" "$target" 1 0 ''; [[ "$installer_global_action" == confirm ]]
previous="$(installer_sha256 "$target")"
installer_global_plan "$source_file" "$target" 1 1 "$previous"; [[ "$installer_global_action" == replace ]]
if installer_global_plan "$source_file" "$target" 1 1 ''; then exit 1; fi
[[ "$(cat -- "$target")" == old ]]
printf 'macOS Global Instructions policy cases passed.\n'
