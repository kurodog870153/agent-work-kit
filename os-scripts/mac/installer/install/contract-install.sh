#!/usr/bin/env bash

installer_contract_subset_safe() {
    local target="$1" source="$2" entry relative
    [[ -d "$target" && ! -L "$target" ]] || return 1
    while IFS= read -r -d '' entry; do
        relative="${entry#"$target"/}"
        [[ ! -L "$entry" && ( -f "$entry" || -d "$entry" ) ]] || return 1
        if [[ -d "$entry" ]]; then
            [[ -d "$source/$relative" && ! -L "$source/$relative" ]] || return 1
        else
            [[ -f "$source/$relative" && ! -L "$source/$relative" &&
               ( "$relative" == *.md || "$relative" == *.yaml || "$relative" == *.yml ) ]] || return 1
        fi
    done < <(find "$target" -mindepth 1 -print0)
}

installer_contract_source_safe() {
    local source="$1"
    installer_contract_subset_safe "$source" "$source" || return 1
    [[ -f "$source/SKILL.md" && -f "$source/references/instruction-loading.md" &&
       -f "$source/references/instruction-loading/queries.md" ]] || return 1
    grep -q '^name: contract$' "$source/SKILL.md"
}

installer_contract_validate_payload() {
    local target="$1" source="$2" entry relative
    installer_contract_source_safe "$source" && installer_contract_subset_safe "$target" "$source" || return 1
    while IFS= read -r -d '' entry; do
        relative="${entry#"$source"/}"
        if [[ -d "$entry" ]]; then
            [[ -d "$target/$relative" ]] || return 1
        else
            [[ -f "$target/$relative" && ! -L "$target/$relative" ]] && cmp -s -- "$entry" "$target/$relative" || return 1
        fi
    done < <(find "$source" -mindepth 1 -print0)
}

installer_contract_inspect() {
    local target="$1" source="$2" parent_raw version
    installer_contract_kind=unsafe
    installer_contract_reason=''
    [[ "$target" == /* && "${target##*/}" == contract ]] || { installer_contract_reason='target must be an absolute contract directory'; return; }
    installer_directory_path_safe "$target" || { installer_contract_reason='unsafe Contract target path'; return; }
    [[ -e "$target" ]] || { installer_contract_kind=new; return; }
    [[ -d "$target" && ! -L "$target" ]] || { installer_contract_reason='Contract target is not a real directory'; return; }
    if [[ -z "$(find "$target" -mindepth 1 -print -quit)" ]]; then installer_contract_kind=new; return; fi
    installer_contract_subset_safe "$target" "$source" || { installer_contract_reason='unknown or unsafe Contract entry'; return; }
    [[ -f "$target/SKILL.md" ]] && grep -q '^name: contract$' "$target/SKILL.md" || {
        installer_contract_reason='Contract identity is invalid'; return;
    }
    [[ -f "${target%/*}/install-state.json" && ! -L "${target%/*}/install-state.json" ]] || {
        installer_contract_reason='Contract has no parent install state'; return;
    }
    parent_raw="$(cat -- "${target%/*}/install-state.json")" || return
    version="$(osascript -l JavaScript "$state_module" combined "$parent_raw" \
        "${target%/*}/work" "${target%/*}/tooling" rust-workspace-version "$selected_host")" || return
    [[ "$version" != - ]] || { installer_contract_reason='Contract parent install state is invalid'; return; }
    installer_contract_kind=managed
}
