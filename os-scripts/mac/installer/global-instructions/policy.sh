#!/usr/bin/env bash

# installer_global_plan SOURCE TARGET ENABLED NON_INTERACTIVE PREVIOUS_SHA
# Sets installer_global_action to skip, install, unchanged, replace or confirm; fails on unsafe or unauthorized targets.
# A target whose bytes equal the previously installed copy is still ours and may be replaced.
installer_global_plan() {
    local source="$1" target="$2" enabled="$3" non_interactive="$4" previous="$5"
    installer_global_action=skip
    installer_global_source_sha=''
    installer_global_target_sha=''
    [[ "$enabled" == 1 ]] || return 0
    [[ -f "$source" && ! -L "$source" ]] || return 1
    installer_global_source_sha="$(installer_sha256 "$source")" || return 1
    if [[ ! -e "$target" && ! -L "$target" ]]; then installer_global_action=install; return 0; fi
    [[ -f "$target" && ! -L "$target" ]] || return 1
    installer_global_target_sha="$(installer_sha256 "$target")" || return 1
    if [[ "$installer_global_source_sha" == "$installer_global_target_sha" ]]; then installer_global_action=unchanged; return 0; fi
    if [[ -n "$previous" && "$installer_global_target_sha" == "$previous" ]]; then installer_global_action=replace; return 0; fi
    [[ "$non_interactive" != 1 ]] || return 1
    installer_global_action=confirm
}
