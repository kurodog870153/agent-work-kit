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
        host-work|host-tooling) printf '%s/%s/SKILL.md\n' "$installer_host_skills_dir" "${1#host-}" ;;
        bin) printf '%s/bin\n' "$installer_cli_dir" ;;
        state) printf '%s/install-state.json\n' "$installer_install_dir" ;;
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
    printf 'physical-entries-v1\n' > "$installer_tx/format" || return 1
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
        if [[ "$name" == host-* || "$name" == global ]]; then
            [[ ! -e "$final.agent-work-kit-tmp" && ! -L "$final.agent-work-kit-tmp" ]] || return 1
        fi
        had=0
        [[ ! -e "$final" && ! -L "$final" ]] || had=1
        printf '%s\t%s\t%s\n' "$name" "$had" "$final" >> "$installer_tx/plan.tmp" || return 1
    done
    mv -- "$installer_tx/plan.tmp" "$installer_tx/plan"
}

# External file participants may be on a different volume. Keep the prepared
# file and an intact backup in the transaction; publish from a sibling temp file.
installer_tx_copy_file() {
    local from="$1" to="$2" tmp="$2.agent-work-kit-tmp"
    installer_directory_path_safe "${to%/*}" || return 1
    [[ -f "$from" && ! -L "$from" && ! -L "$to" &&
       ( ! -e "$to" || -f "$to" ) && ! -e "$tmp" && ! -L "$tmp" ]] || return 1
    (set -C; : > "$tmp") || return 1
    cp -- "$from" "$tmp" || return 1
    cmp -s "$from" "$tmp" || return 1
    mv -f -- "$tmp" "$to"
}

installer_tx_restore_file() {
    local final="$1" staged="$2" backup="$3" had="$4" tmp="$1.agent-work-kit-tmp"
    installer_directory_path_safe "${final%/*}" || return 1
    [[ ! -L "$final" && ( ! -e "$final" || -f "$final" ) &&
       ! -L "$tmp" && ( ! -e "$tmp" || -f "$tmp" ) ]] || return 1
    if [[ -f "$backup" && ! -L "$backup" ]]; then
        if [[ -e "$final" ]]; then
            cmp -s "$final" "$backup" || cmp -s "$final" "$staged" || return 1
        fi
        rm -f -- "$tmp" || return 1
        installer_tx_copy_file "$backup" "$final" || return 1
    elif [[ "$had" == 0 && -e "$final" ]]; then
        cmp -s "$final" "$staged" || return 1
        rm -- "$final" || return 1
    fi
    rm -f -- "$tmp"
}

installer_tx_apply_component() {
    local name="$1" final staged backup
    final="$(installer_tx_final "$name")"
    staged="$installer_tx/staged/$name"
    backup="$installer_tx/backup/$name"
    if [[ "$name" == host-* || "$name" == global ]]; then
        [[ -f "$staged" && ! -L "$staged" ]] || return 1
        if [[ -e "$final" || -L "$final" ]]; then
            installer_tx_copy_file "$final" "$backup" || return 1
        fi
        installer_inject "apply-$name-moved" || return 1
        installer_tx_copy_file "$staged" "$final" || return 1
        installer_inject "apply-$name"
        return
    fi
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
    if [[ "$name" == host-* || "$name" == global ]]; then
        installer_tx_restore_file "$final" "$staged" "$backup" "$had"
        return
    fi
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
    local name had final entry state_file global_target="" global_name=AGENTS.md seen="|" required
    installer_tx_names=(); installer_tx_had=(); installer_tx_finals=()
    [[ -d "$installer_tx" && ! -L "$installer_tx" ]] || return 1
    [[ -f "$installer_tx/format" && ! -L "$installer_tx/format" &&
       "$(cat "$installer_tx/format")" == physical-entries-v1 ]] || return 1
    for entry in "$installer_tx"/* "$installer_tx"/.[!.]*; do
        [[ -e "$entry" || -L "$entry" ]] || continue
        case "${entry##*/}" in
            staged|backup) [[ -d "$entry" && ! -L "$entry" ]] || return 1 ;;
            plan|plan.tmp|created|committed|format) [[ -f "$entry" && ! -L "$entry" ]] || return 1 ;;
            *) return 1 ;;
        esac
    done
    [[ -z "$(find "$installer_tx" -type l -print -quit)" ]] || return 1
    [[ -f "$installer_tx/plan" ]] || return 2
    state_file="$installer_tx/staged/state"
    [[ -f "$state_file" ]] || state_file="$installer_install_dir/install-state.json"
    installer_state_read "$state_file" || return 1
    [[ "$installer_state_host" == "$selected_host" && "$installer_state_install_dir" == "$installer_install_dir" ]] || return 1
    [[ "$selected_host" != claude ]] || global_name=CLAUDE.md
    [[ -z "$installer_state_global_dir" ]] || global_target="$installer_state_global_dir/$global_name"
    while IFS=$'\t' read -r name had final; do
        case "$name" in
            work|tooling|shared|bin|state|host-work|host-tooling) [[ "$final" == "$(installer_tx_final "$name")" ]] || return 1 ;;
            global) [[ -n "$global_target" && "$final" == "$global_target" ]] && installer_path_valid "$final" || return 1 ;;
            *) return 1 ;;
        esac
        [[ "$had" == 0 || "$had" == 1 ]] || return 1
        [[ "$seen" != *"|$name|"* ]] && installer_directory_path_safe "${final%/*}" || return 1
        seen="$seen$name|"
        installer_tx_names+=("$name"); installer_tx_had+=("$had"); installer_tx_finals+=("$final")
    done < "$installer_tx/plan"
    for required in work tooling shared bin host-work host-tooling state; do
        [[ "$seen" == *"|$required|"* ]] || return 1
    done
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
        rmdir -- "${created[index]}" 2>/dev/null || true
    done
}

installer_tx_finish() {
    local index status
    installer_tx_load_plan; status=$?
    (( status == 0 )) || { installer_fail "unsafe installation transaction; preserved for review: $installer_tx"; return 1; }
    if [[ -e "$installer_tx/staged/state" ]]; then installer_tx_apply_component state || return 1; fi
    installer_state_read "$installer_install_dir/install-state.json" || { installer_fail 'published install state is invalid'; return 1; }
    installer_manifest_matches "$installer_install_dir" "${installer_state_files[@]}" || return 1
    installer_tx_cleanup
}

# Called at the start of a run: completes or undoes an interrupted transaction.
installer_tx_recover() {
    local status
    [[ -e "$installer_tx" || -L "$installer_tx" ]] || return 0
    if [[ -f "$installer_tx/committed" ]]; then installer_tx_finish; return; fi
    installer_tx_rollback
}
