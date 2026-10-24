#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
source "$repo_root/os-scripts/mac/installer/global-agents/policy.sh"
scratch="$(mktemp -d "$repo_root/.issue95-global.XXXXXX")"
[[ "$(cd "$scratch" && pwd -P)" == "$scratch" && "$scratch" == "$repo_root"/.issue95-global.* ]] || exit 1
trap 'rm -rf -- "$scratch"' EXIT
source_file="$scratch/source.md"
target="$scratch/AGENTS.md"
printf 'new\n' > "$source_file"
installer_global_agents_plan "$source_file" "$target" 0 1 0
[[ "$installer_global_agents_action" == skip ]]
installer_global_agents_plan "$source_file" "$target" 1 1 0
[[ "$installer_global_agents_action" == install ]]
cp -- "$source_file" "$target"
installer_global_agents_plan "$source_file" "$target" 1 1 0
[[ "$installer_global_agents_action" == unchanged ]]
printf 'old\n' > "$target"
installer_global_agents_plan "$source_file" "$target" 1 0 0
[[ "$installer_global_agents_action" == confirm && "$installer_global_agents_diff" == *'-old'* && "$installer_global_agents_diff" == *'+new'* ]]
installer_global_agents_plan "$source_file" "$target" 1 0 1
[[ "$installer_global_agents_action" == replace ]]
if installer_global_agents_plan "$source_file" "$target" 1 1 1; then exit 1; fi
[[ "$(cat -- "$target")" == old ]]
printf 'macOS Global AGENTS policy cases passed.\n'
