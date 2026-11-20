#!/usr/bin/env bash

# One transaction covers Work, Tooling, Shared, the CLI binaries, optional Global Instructions and the
# install state. Nothing is valid until the "committed" marker exists; recovery before the marker restores
# every previous component, recovery after it only finishes the state publication.
#
# TX layout: plan (name<TAB>had-previous<TAB>final path), created (directories created for this install),
# staged/<name> (new content), backup/<name> (previous content), committed (commit marker).
installer_inject() {
    [[ "${INSTALLER_TEST_CRASH:-}" != "$1" ]] || exit 97
    if [[ "${INSTALLER_TEST_FAILURE:-}" == "$1" ]]; then printf 'Error: injected failure at %s\n' "$1" >&2; return 1; fi
}

installer_tx_final() {
    case "$1" in
        work|tooling|shared) printf '%s/%s\n' "$installer_skills_dir" "$1" ;;
        bin) printf '%s/bin\n' "$installer_cli_dir" ;;
        state) printf '%s/install-state.json\n' "$installer_skills_dir" ;;
        global) printf '%s\n' "$installer_global_target" ;;
    esac
}

installer_create_directories() {
    local probe="$1" index
    local -a missing=()
    while [[ ! -d "$probe" ]]; do
        [[ ! -e "$probe" && ! -L "$probe" && "$probe" != / ]] || return 1
        missing+=("$probe")
        probe="${probe%/*}"
        [[ -n "$probe" ]] || probe=/
    done
    for ((index=${#missing[@]}-1; index>=0; index--)); do
        mkdir -- "${missing[index]}" || return 1
        installer_created_dirs+=("${missing[index]}")
    done
}

installer_remove_created_directories() {
    local index
    for ((index=${#installer_created_dirs[@]}-1; index>=0; index--)); do
        rmdir -- "${installer_created_dirs[index]}" 2>/dev/null || break
    done
}

installer_tx_remove() {
    [[ -d "$installer_tx" && ! -L "$installer_tx" && -z "$(find "$installer_tx" -type l -print -quit)" ]] || return 1
    rm -rf -- "$installer_tx"
}

installer_tx_begin() {
    local entry
    mkdir -- "$installer_tx" && mkdir -- "$installer_tx/staged" "$installer_tx/backup" || return 1
    : > "$installer_tx/created" || return 1
    for entry in "${installer_created_dirs[@]+"${installer_created_dirs[@]}"}"; do printf '%s\n' "$entry" >> "$installer_tx/created"; done
}

installer_tx_note_created() {
    local entry
    for entry in "$@"; do printf '%s\n' "$entry" >> "$installer_tx/created" || return 1; done
}

# installer_tx_write_plan NAME...: records the components and whether each had a previous version.
installer_tx_write_plan() {
    local name final had
    : > "$installer_tx/plan.tmp" || return 1
    for name in "$@"; do
        final="$(installer_tx_final "$name")"
        had=0
        [[ ! -e "$final" && ! -L "$final" ]] || had=1
        printf '%s\t%s\t%s\n' "$name" "$had" "$final" >> "$installer_tx/plan.tmp" || return 1
    done
    mv -- "$installer_tx/plan.tmp" "$installer_tx/plan"
}

installer_tx_apply_component() {
    local name="$1" final staged backup
    final="$(installer_tx_final "$name")"
    staged="$installer_tx/staged/$name"
    backup="$installer_tx/backup/$name"
    [[ -e "$staged" ]] || return 1
    if [[ -e "$final" || -L "$final" ]]; then mv -- "$final" "$backup" || return 1; fi
    installer_inject "apply-$name-moved" || return 1
    mv -- "$staged" "$final" || return 1
    installer_inject "apply-$name"
}

installer_tx_rollback_component() {
    local name="$1" had="$2" final="$3" staged backup
    staged="$installer_tx/staged/$name"
    backup="$installer_tx/backup/$name"
    if [[ -e "$backup" || -L "$backup" ]]; then
        if [[ -e "$final" || -L "$final" ]]; then
            [[ ! -e "$staged" && ! -L "$staged" ]] || return 1
            rm -rf -- "$final" || return 1
        fi
        mv -- "$backup" "$final" || return 1
    elif [[ "$had" == 0 && ! -e "$staged" && ! -L "$staged" && ( -e "$final" || -L "$final" ) ]]; then
        rm -rf -- "$final" || return 1
    fi
}

# Reads and validates the recorded plan into installer_tx_names/had/finals. Returns 2 when no plan exists.
installer_tx_load_plan() {
    local name had final entry
    installer_tx_names=(); installer_tx_had=(); installer_tx_finals=()
    [[ -d "$installer_tx" && ! -L "$installer_tx" ]] || return 1
    for entry in "$installer_tx"/* "$installer_tx"/.[!.]*; do
        [[ -e "$entry" || -L "$entry" ]] || continue
        case "${entry##*/}" in
            staged|backup) [[ -d "$entry" && ! -L "$entry" ]] || return 1 ;;
            plan|plan.tmp|created|committed) [[ -f "$entry" && ! -L "$entry" ]] || return 1 ;;
            *) return 1 ;;
        esac
    done
    [[ -z "$(find "$installer_tx" -type l -print -quit)" ]] || return 1
    [[ -f "$installer_tx/plan" ]] || return 2
    while IFS=$'\t' read -r name had final; do
        case "$name" in
            work|tooling|shared|bin|state) [[ "$final" == "$(installer_tx_final "$name")" ]] || return 1 ;;
            global) [[ "$final" == /*/AGENTS.md || "$final" == /*/CLAUDE.md ]] && installer_path_valid "$final" || return 1 ;;
            *) return 1 ;;
        esac
        [[ "$had" == 0 || "$had" == 1 ]] || return 1
        installer_tx_names+=("$name"); installer_tx_had+=("$had"); installer_tx_finals+=("$final")
    done < "$installer_tx/plan"
}

installer_tx_cleanup() {
    installer_tx_remove || printf 'Warning: installation transaction awaits cleanup: %s\n' "$installer_tx" >&2
    return 0
}

installer_tx_rollback() {
    local index status
    installer_tx_load_plan; status=$?
    if (( status == 1 )); then installer_fail "unsafe installation transaction; preserved for review: $installer_tx"; return 1; fi
    if (( status == 0 )); then
        for ((index=${#installer_tx_names[@]}-1; index>=0; index--)); do
            installer_tx_rollback_component "${installer_tx_names[index]}" "${installer_tx_had[index]}" "${installer_tx_finals[index]}" ||
                { installer_fail "could not restore ${installer_tx_names[index]}; previous content remains under $installer_tx/backup"; return 1; }
        done
    fi
    installer_tx_restore_created || return 1
}

# Removes the transaction and then the directories it created (the transaction directory itself lives in one of them).
installer_tx_restore_created() {
    local -a created=()
    local entry index
    if [[ -f "$installer_tx/created" && ! -L "$installer_tx/created" ]]; then
        while IFS= read -r entry; do [[ "$entry" == /* ]] && created+=("$entry"); done < "$installer_tx/created"
    fi
    installer_tx_remove || { installer_fail "could not remove installation transaction: $installer_tx"; return 1; }
    for ((index=${#created[@]}-1; index>=0; index--)); do
        rmdir -- "${created[index]}" 2>/dev/null || break
    done
}

installer_tx_finish() {
    local index status
    installer_tx_load_plan; status=$?
    (( status == 0 )) || { installer_fail "unsafe installation transaction; preserved for review: $installer_tx"; return 1; }
    if [[ -e "$installer_tx/staged/state" ]]; then installer_tx_apply_component state || return 1; fi
    installer_state_read "$installer_skills_dir/install-state.json" || { installer_fail 'published install state is invalid'; return 1; }
    installer_tx_cleanup
}

# Called at the start of a run: completes or undoes an interrupted transaction.
installer_tx_recover() {
    local status
    [[ -e "$installer_tx" || -L "$installer_tx" ]] || return 0
    if [[ -f "$installer_tx/committed" ]]; then installer_tx_finish; return; fi
    installer_tx_rollback
}
