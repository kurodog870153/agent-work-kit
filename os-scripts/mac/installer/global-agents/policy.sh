#!/usr/bin/env bash

installer_global_agents_plan() {
    local source="$1" target="$2" enabled="$3" non_interactive="$4" confirmed="$5" digest diff_output status
    installer_global_agents_action=''
    installer_global_agents_source_sha256=''
    installer_global_agents_target_sha256=''
    installer_global_agents_diff=''
    if [[ "$enabled" != 1 ]]; then installer_global_agents_action=skip; return 0; fi
    [[ -f "$source" && ! -L "$source" ]] || return 1
    digest="$(/usr/bin/openssl dgst -sha256 "$source")" || return 1
    installer_global_agents_source_sha256="${digest##* }"
    if [[ ! -e "$target" && ! -L "$target" ]]; then installer_global_agents_action=install; return 0; fi
    [[ -f "$target" && ! -L "$target" ]] || return 1
    digest="$(/usr/bin/openssl dgst -sha256 "$target")" || return 1
    installer_global_agents_target_sha256="${digest##* }"
    if [[ "$installer_global_agents_source_sha256" == "$installer_global_agents_target_sha256" ]]; then
        installer_global_agents_action=unchanged
        return 0
    fi
    if diff_output="$(diff -u -- "$target" "$source")"; then
        diff_output=''
    else
        status=$?
        (( status == 1 )) || return 1
    fi
    installer_global_agents_diff="$(printf '%s\n' "$diff_output" | sed -n '1,100p')"
    if [[ "$non_interactive" == 1 ]]; then return 1; fi
    if [[ "$confirmed" == 1 ]]; then installer_global_agents_action=replace
    else installer_global_agents_action=confirm; fi
}
