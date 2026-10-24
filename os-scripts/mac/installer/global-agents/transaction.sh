#!/usr/bin/env bash

installer_global_agents_hash() {
    local digest
    digest="$(/usr/bin/openssl dgst -sha256 "$1")" || return 1
    printf '%s\n' "${digest##* }"
}

installer_global_agents_stage() {
    local source="$1" target="$2" txn="$3" action="$4" source_hash="$5" target_hash="$6" failure="${7:-}"
    local prepared="$txn/global-prepared" previous="$txn/global-previous" actual
    [[ "$action" == skip || "$action" == unchanged ]] && return 0
    [[ "$action" == install || "$action" == replace ]] || return 1
    [[ -d "$txn" && ! -L "$txn" && -d "${target%/*}" ]] || return 1
    [[ ! -e "$prepared" && ! -L "$prepared" && ! -e "$previous" && ! -L "$previous" ]] || return 1
    actual="$(installer_global_agents_hash "$source")" || return 1
    [[ "$actual" == "$source_hash" ]] || return 1
    cp -- "$source" "$prepared" || return 1
    actual="$(installer_global_agents_hash "$prepared")" || return 1
    [[ "$actual" == "$source_hash" ]] || return 1
    if [[ "$action" == replace ]]; then
        [[ -f "$target" && ! -L "$target" ]] || return 1
        actual="$(installer_global_agents_hash "$target")" || return 1
        [[ "$actual" == "$target_hash" ]] || return 1
        mv -- "$target" "$previous" || return 1
    else
        [[ ! -e "$target" && ! -L "$target" ]] || return 1
    fi
    if [[ "$failure" == publish || "$failure" == restore ]] || ! mv -- "$prepared" "$target" ||
        [[ "$(installer_global_agents_hash "$target")" != "$source_hash" ]]; then
        if [[ -e "$target" ]]; then mv -- "$target" "$txn/global-failed" || return 1; fi
        if [[ -e "$previous" ]]; then
            if [[ "$failure" == restore ]] || ! mv -- "$previous" "$target"; then
                printf 'Error: Global AGENTS restore failed; previous bytes remain at %s\n' "$previous" >&2
                return 1
            fi
        fi
        return 1
    fi
}

installer_global_agents_undo() {
    local target="$1" txn="$2" action="$3" source_hash="$4" target_hash="$5" previous="$2/global-previous" actual
    [[ "$action" == skip || "$action" == unchanged ]] && return 0
    if [[ -e "$target" || -L "$target" ]]; then
        [[ -f "$target" && ! -L "$target" ]] || return 1
        actual="$(installer_global_agents_hash "$target")" || return 1
        [[ "$actual" == "$source_hash" ]] || return 1
        mv -- "$target" "$txn/global-undone" || return 1
    fi
    if [[ "$action" == replace ]]; then
        [[ -f "$previous" && ! -L "$previous" ]] || return 1
        mv -- "$previous" "$target" || return 1
        actual="$(installer_global_agents_hash "$target")" || return 1
        [[ "$actual" == "$target_hash" ]] || return 1
    fi
}

installer_global_record_transaction() {
    local txn="$1" work="$2" target="$3" action="$4" source_hash="$5" target_hash="${6:-}"
    [[ "$work" == /* && "$target" == /* && "$work" != *$'\n'* && "$target" != *$'\n'* ]] || return 1
    [[ "$action" == install || "$action" == replace ]] || return 1
    [[ "$source_hash" =~ ^[0-9a-f]{64}$ ]] || return 1
    [[ "$action" == install && -z "$target_hash" || "$action" == replace && "$target_hash" =~ ^[0-9a-f]{64}$ ]] || return 1
    printf '%s\n' "$work" > "$txn/work-target.txt" || return 1
    printf '%s\n' "$target" > "$txn/global-target.txt" || return 1
    printf '%s\n' "$action" > "$txn/action.txt" || return 1
    printf '%s\n' "$source_hash" > "$txn/source-sha256.txt" || return 1
    printf '%s\n' "${target_hash:--}" > "$txn/previous-sha256.txt"
}

installer_global_mark_work_verified() {
    local txn="$1" work_txn="$2" temporary
    [[ -f "$work_txn/verified.txt" && -f "$work_txn/state-sha256.txt" &&
       -f "$work_txn/binary-sha256.txt" ]] || return 1
    temporary="$txn/work-verified.tmp"
    [[ ! -e "$temporary" && ! -L "$temporary" ]] || return 1
    { cat -- "$work_txn/state-sha256.txt"; cat -- "$work_txn/binary-sha256.txt"; } > "$temporary" || return 1
    mv -- "$temporary" "$txn/work-verified.txt"
}

installer_global_record_safe() {
    local txn="$1" entry name hash
    [[ -d "$txn" && ! -L "$txn" ]] || return 1
    for name in work-target.txt global-target.txt action.txt source-sha256.txt previous-sha256.txt; do
        [[ -f "$txn/$name" && ! -L "$txn/$name" ]] || return 1
    done
    installer_global_work_target="$(cat -- "$txn/work-target.txt")" || return 1
    installer_global_target="$(cat -- "$txn/global-target.txt")" || return 1
    installer_global_action="$(cat -- "$txn/action.txt")" || return 1
    installer_global_source_hash="$(cat -- "$txn/source-sha256.txt")" || return 1
    installer_global_previous_hash="$(cat -- "$txn/previous-sha256.txt")" || return 1
    [[ "$installer_global_work_target" == /* && "$installer_global_target" == /* &&
       "$installer_global_work_target" != *$'\n'* && "$installer_global_target" != *$'\n'* &&
       "$installer_global_source_hash" =~ ^[0-9a-f]{64}$ ]] || return 1
    case "$installer_global_action" in
        install) [[ "$installer_global_previous_hash" == - ]] || return 1 ;;
        replace) [[ "$installer_global_previous_hash" =~ ^[0-9a-f]{64}$ ]] || return 1 ;;
        *) return 1 ;;
    esac
    local probe="${installer_global_target%/*}"
    while [[ "$probe" != / ]]; do
        [[ ! -L "$probe" && ( ! -e "$probe" || -d "$probe" ) ]] || return 1
        probe="${probe%/*}"
        [[ -n "$probe" ]] || probe=/
    done
    for entry in "$txn"/* "$txn"/.[!.]*; do
        [[ -e "$entry" || -L "$entry" ]] || continue
        [[ -f "$entry" && ! -L "$entry" ]] || return 1
        name="${entry##*/}"
        case "$name" in
            work-target.txt|global-target.txt|action.txt|source-sha256.txt|previous-sha256.txt) ;;
            work-verified.txt|work-verified.tmp)
                [[ "$(sed -n '1p' "$entry")" =~ ^[0-9a-f]{64}$ &&
                   "$(sed -n '2p' "$entry")" =~ ^[0-9a-f]{64}$ ]] || return 1 ;;
            global-prepared|global-failed|global-undone)
                hash="$(installer_global_agents_hash "$entry")" || return 1
                [[ "$hash" == "$installer_global_source_hash" ]] || return 1 ;;
            global-previous)
                hash="$(installer_global_agents_hash "$entry")" || return 1
                [[ "$hash" == "$installer_global_previous_hash" ]] || return 1 ;;
            *) return 1 ;;
        esac
    done
}

installer_global_cleanup_transaction() {
    local txn="$1" entry
    installer_global_record_safe "$txn" || return 1
    for entry in "$txn"/*; do
        [[ -e "$entry" ]] || continue
        rm -- "$entry" || return 1
    done
    rmdir -- "$txn"
}

installer_global_transaction_pending() {
    local work="$1" txn record
    for txn in "${work%/*}"/.agents-install.*; do
        [[ -d "$txn" && ! -L "$txn" ]] || continue
        record="$txn/work-target.txt"
        [[ -f "$record" && ! -L "$record" ]] || continue
        [[ "$(cat -- "$record")" == "$work" ]] && return 0
    done
    return 1
}

installer_global_recover_interrupted() {
    local work="$1" source_work="$2" txn='' entry count=0 work_txn='' work_count=0
    local current_hash previous
    for entry in "${work%/*}"/.agents-install.*; do
        [[ -d "$entry" && ! -L "$entry" && -f "$entry/work-target.txt" &&
           ! -L "$entry/work-target.txt" ]] || continue
        [[ "$(cat -- "$entry/work-target.txt")" == "$work" ]] || continue
        txn="$entry"; count=$((count + 1))
    done
    (( count == 0 )) && return 0
    (( count == 1 )) || { printf 'Error: multiple Global AGENTS transactions claim Work.\n' >&2; return 1; }
    installer_global_record_safe "$txn" || { printf 'Error: unsafe Global AGENTS recovery record: %s\n' "$txn" >&2; return 1; }
    for entry in "${work%/*}"/.work-install.*; do
        [[ -d "$entry" && ! -L "$entry" && -f "$entry/target-path.txt" &&
           ! -L "$entry/target-path.txt" ]] || continue
        [[ "$(cat -- "$entry/target-path.txt")" == "$work" ]] || continue
        work_txn="$entry"; work_count=$((work_count + 1))
    done
    (( work_count <= 1 )) || return 1
    if [[ ! -f "$txn/work-verified.txt" && -n "$work_txn" && -f "$work_txn/verified.txt" ]]; then
        installer_work_matches_record "$work" "$source_work" "$work_txn" || return 1
        installer_global_mark_work_verified "$txn" "$work_txn" || return 1
    fi
    if [[ -f "$txn/work-verified.txt" ]]; then
        installer_work_directory_safe "$work" "$source_work" || return 1
        current_hash="$(shasum -a 256 -- "$work/install-state.json")" || return 1
        [[ "${current_hash%% *}" == "$(sed -n '1p' "$txn/work-verified.txt")" ]] || return 1
        current_hash="$(shasum -a 256 -- "$work/scripts/work")" || return 1
        [[ "${current_hash%% *}" == "$(sed -n '2p' "$txn/work-verified.txt")" ]] || return 1
        [[ -f "$installer_global_target" && ! -L "$installer_global_target" ]] || return 1
        [[ "$(installer_global_agents_hash "$installer_global_target")" == "$installer_global_source_hash" ]] || return 1
        installer_global_cleanup_transaction "$txn" || {
            printf 'Warning: verified Global AGENTS transaction awaits cleanup: %s\n' "$txn" >&2
        }
        return 0
    fi
    previous="$txn/global-previous"
    if [[ -e "$installer_global_target" || -L "$installer_global_target" ]]; then
        [[ -f "$installer_global_target" && ! -L "$installer_global_target" ]] || return 1
        current_hash="$(installer_global_agents_hash "$installer_global_target")" || return 1
        if [[ "$installer_global_action" == replace && ! -e "$previous" &&
              "$current_hash" == "$installer_global_previous_hash" ]]; then
            :
        elif [[ "$current_hash" == "$installer_global_source_hash" ]]; then
            installer_global_agents_undo "$installer_global_target" "$txn" "$installer_global_action" \
                "$installer_global_source_hash" "$installer_global_previous_hash" || return 1
        else
            return 1
        fi
    elif [[ "$installer_global_action" == replace && -e "$previous" ]]; then
        installer_global_agents_undo "$installer_global_target" "$txn" "$installer_global_action" \
            "$installer_global_source_hash" "$installer_global_previous_hash" || return 1
    elif [[ "$installer_global_action" == replace ]]; then
        return 1
    fi
    installer_global_cleanup_transaction "$txn" || return 1
}
