#!/usr/bin/env bash

# Read-only checks of the environment, sources and installation targets.
installer_fail() { printf 'Error: %s\n' "$1" >&2; return 1; }

installer_absolute_path() {
    local input="$1" component result='' index
    local -a components stack=()
    [[ "$input" == /* ]] || input="$PWD/$input"
    IFS=/ read -r -a components <<< "$input"
    for component in "${components[@]}"; do
        case "$component" in
            ''|.) ;;
            ..) if (( ${#stack[@]} )); then index=$((${#stack[@]} - 1)); unset 'stack[index]'; fi ;;
            *) stack+=("$component") ;;
        esac
    done
    for component in "${stack[@]}"; do result+="/$component"; done
    printf '%s\n' "${result:-/}"
}

# Every existing component of the path must be a real directory (no links, no files).
installer_directory_path_safe() {
    local probe="$1"
    while [[ "$probe" != / ]]; do
        [[ ! -L "$probe" && ( ! -e "$probe" || -d "$probe" ) ]] || return 1
        probe="${probe%/*}"
        [[ -n "$probe" ]] || probe=/
    done
}

installer_path_overlaps() {
    [[ "$1" == "$2" || "$1" == "$2"/* || "$2" == "$1"/* ]]
}

# A source tree holds only regular, non-linked Markdown or YAML files below real directories.
installer_source_tree_safe() {
    local source="$1" entry
    [[ -d "$source" && ! -L "$source" ]] || return 1
    while IFS= read -r -d '' entry; do
        [[ ! -L "$entry" && ( -d "$entry" || -f "$entry" ) ]] || return 1
        if [[ -f "$entry" ]]; then
            [[ "$entry" == *.md || "$entry" == *.yaml || "$entry" == *.yml ]] || return 1
            installer_relative_valid "${entry#"$source"/}" || return 1
        fi
    done < <(find "$source" -mindepth 1 -print0)
}

installer_sources_safe() {
    local root="$1"
    installer_source_tree_safe "$root/skills/work" && [[ -f "$root/skills/work/SKILL.md" ]] &&
        grep -q '^name: work$' "$root/skills/work/SKILL.md" || { installer_fail 'Work Skill source is missing or unsafe'; return 1; }
    installer_source_tree_safe "$root/skills/tooling" && [[ -f "$root/skills/tooling/SKILL.md" ]] &&
        grep -q '^name: tooling$' "$root/skills/tooling/SKILL.md" || { installer_fail 'Tooling Skill source is missing or unsafe'; return 1; }
    installer_source_tree_safe "$root/skills/shared" && [[ -f "$root/skills/shared/bootstrap.md" && ! -e "$root/skills/shared/SKILL.md" ]] ||
        { installer_fail 'Shared source is missing or unsafe'; return 1; }
}

# installer_inspect_targets SKILLS CLI: sets installer_target_kind to
# new (nothing installed), managed (valid state and matching content) or unsafe.
installer_inspect_targets() {
    local skills="$1" cli="$2" name entry
    installer_target_kind=unsafe
    installer_target_reason=''
    installer_state_clear
    if [[ -e "$skills/install-state.json" || -L "$skills/install-state.json" ]]; then
        installer_state_read "$skills/install-state.json" || { installer_target_reason='install-state.json is invalid'; return; }
        [[ "$installer_state_skills_dir" == "$skills" && "$installer_state_cli_dir" == "$cli" ]] ||
            { installer_target_reason='install state belongs to another Skills or CLI directory'; return; }
        installer_manifest_matches "$skills" "$cli" "${installer_state_files[@]}" ||
            { installer_target_reason='installed content differs from the recorded installation'; return; }
        installer_target_kind=managed
        return
    fi
    for name in work tooling shared; do
        entry="$skills/$name"
        if [[ -e "$entry" || -L "$entry" ]]; then
            [[ -d "$entry" && ! -L "$entry" && -z "$(find "$entry" -mindepth 1 -print -quit)" ]] ||
                { installer_target_reason="existing content without a valid install state: $entry"; return; }
        fi
    done
    if [[ -e "$cli/bin" || -L "$cli/bin" ]]; then
        [[ -d "$cli/bin" && ! -L "$cli/bin" && -z "$(find "$cli/bin" -mindepth 1 -print -quit)" ]] ||
            { installer_target_reason="existing content without a valid install state: $cli/bin"; return; }
    fi
    installer_target_kind=new
}
