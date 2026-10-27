#!/usr/bin/env bash

installer_tooling_source_safe() {
    local source="$1" entry relative
    [[ -d "$source" && ! -L "$source" && -f "$source/SKILL.md" &&
       -f "$source/references/instruction-loading.md" &&
       -f "$source/references/instructions/instructions.md" ]] || return 1
    while IFS= read -r -d '' entry; do
        relative="${entry#"$source"/}"
        [[ ! -L "$entry" ]] || return 1
        if [[ -d "$entry" ]]; then continue; fi
        [[ -f "$entry" && ( "$relative" == *.md || "$relative" == *.yaml || "$relative" == *.yml ) ]] || return 1
    done < <(find "$source" -mindepth 1 -print0)
}

installer_tooling_inspect() {
    local target="$1" source="$2" entry relative version parent_raw
    installer_tooling_kind=unsafe
    installer_tooling_reason=''
    installer_tooling_version=-
    [[ "$target" == /* && "${target##*/}" == tooling ]] || { installer_tooling_reason='target must be an absolute tooling directory'; return; }
    installer_directory_path_safe "$target" || { installer_tooling_reason='unsafe target path'; return; }
    [[ -e "$target" ]] || { installer_tooling_kind=new; return; }
    [[ -d "$target" && ! -L "$target" ]] || { installer_tooling_reason='target is not a real directory'; return; }
    if [[ -z "$(find "$target" -mindepth 1 -print -quit)" ]]; then installer_tooling_kind=new; return; fi
    installer_tooling_subset_safe "$target" "$source" || { installer_tooling_reason='unknown or unsafe Tooling entry'; return; }
    [[ -f "$target/SKILL.md" && -f "$target/references/instruction-loading.md" &&
       -f "$target/references/instructions/instructions.md" ]] || { installer_tooling_reason='required Tooling file is missing'; return; }
    grep -q '^name: tooling$' "$target/SKILL.md" || { installer_tooling_reason='Tooling identity is invalid'; return; }
    version=-
    if [[ -f "${target%/*}/install-state.json" &&
          ! -L "${target%/*}/install-state.json" ]]; then
        parent_raw="$(cat -- "${target%/*}/install-state.json")" || return
        version="$(osascript -l JavaScript "$state_module" combined "$parent_raw" \
            "${target%/*}/work" "$target" tooling-version)" || return
    fi
    installer_tooling_version="$version"
    if [[ "$version" == - ]]; then installer_tooling_kind=first-update
    else installer_tooling_kind=managed; fi
}

installer_tooling_subset_safe() {
    local target="$1" source="$2" entry relative
    [[ -d "$target" && ! -L "$target" ]] || return 1
    while IFS= read -r -d '' entry; do
        relative="${entry#"$target"/}"
        [[ ! -L "$entry" && ( -f "$entry" || -d "$entry" ) ]] || return 1
        if [[ "$relative" == install-state.json ]]; then
            [[ -f "$entry" ]] || return 1
        elif [[ "$relative" == scripts ]]; then
            [[ -d "$entry" ]] || return 1
        elif [[ "$relative" == scripts/tooling ]]; then
            [[ -f "$entry" ]] || return 1
        elif [[ -d "$entry" ]]; then
            [[ -d "$source/$relative" && ! -L "$source/$relative" ]] || return 1
        else
            [[ -f "$source/$relative" && ! -L "$source/$relative" ]] || return 1
        fi
    done < <(find "$target" -mindepth 1 -print0)
}

installer_tooling_validate_payload() {
    local target="$1" source="$2" state_module="$3" expected_version="$4" final="$5" binary="$6" entry relative raw version
    installer_tooling_subset_safe "$target" "$source" || return 1
    [[ -f "$target/install-state.json" && ! -L "$target/install-state.json" ]] || return 1
    raw="$(cat -- "$target/install-state.json")" || return 1
    version="$(osascript -l JavaScript "$state_module" version "$raw" "$final")" || return 1
    [[ "$version" == "$expected_version" ]] || return 1
    [[ -f "$target/scripts/tooling" && ! -L "$target/scripts/tooling" ]] &&
        cmp -s -- "$binary" "$target/scripts/tooling" || return 1
    while IFS= read -r -d '' entry; do
        relative="${entry#"$source"/}"
        if [[ -d "$entry" ]]; then [[ -d "$target/$relative" ]] || return 1
        else [[ -f "$target/$relative" && ! -L "$target/$relative" ]] && cmp -s -- "$entry" "$target/$relative" || return 1; fi
    done < <(find "$source" -mindepth 1 -print0)
}
