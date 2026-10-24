#!/usr/bin/env bash

installer_work_directory_safe() {
    local path="$1" source="$2"
    installer_inspect_target "$path" "$source" 0
    [[ "$installer_target_kind" == first-update || "$installer_target_kind" == managed ||
       "$installer_target_kind" == new && -d "$path" ]]
}

installer_work_subset_safe() {
    local path="$1" source="$2" entry relative
    [[ -d "$path" && ! -L "$path" ]] || return 1
    while IFS= read -r -d '' entry; do
        relative="${entry#"$path"/}"
        [[ ! -L "$entry" && ( -f "$entry" || -d "$entry" ) ]] || return 1
        installer_managed_relative "$relative" || return 1
        if [[ "$relative" != scripts && "$relative" != scripts/work &&
              "$relative" != install-state.json && ! -e "$source/$relative" ]]; then
            return 1
        fi
    done < <(find "$path" -mindepth 1 -print0)
}

installer_work_matches_record() {
    local final="$1" source="$2" txn="$3" state_hash binary_hash
    installer_work_directory_safe "$final" "$source" || return 1
    [[ -f "$final/install-state.json" && ! -L "$final/install-state.json" &&
       -f "$final/scripts/work" && ! -L "$final/scripts/work" ]] || return 1
    [[ -f "$txn/state-sha256.txt" && -f "$txn/binary-sha256.txt" &&
       ! -L "$txn/state-sha256.txt" && ! -L "$txn/binary-sha256.txt" ]] || return 1
    state_hash="$(shasum -a 256 -- "$final/install-state.json")" || return 1
    binary_hash="$(shasum -a 256 -- "$final/scripts/work")" || return 1
    [[ "${state_hash%% *}" == "$(cat -- "$txn/state-sha256.txt")" &&
       "${binary_hash%% *}" == "$(cat -- "$txn/binary-sha256.txt")" ]]
}

installer_work_transaction_safe() {
    local txn="$1" source="$2" entry name
    [[ -d "$txn" && ! -L "$txn" ]] || return 1
    for entry in "$txn"/* "$txn"/.[!.]*; do
        [[ -e "$entry" || -L "$entry" ]] || continue
        [[ ! -L "$entry" ]] || return 1
        name="${entry##*/}"
        case "$name" in
            target-path.txt|state-sha256.txt|binary-sha256.txt|verified.txt)
                [[ -f "$entry" ]] || return 1 ;;
            prepared|previous|published-failed|published-interrupted)
                installer_work_subset_safe "$entry" "$source" || return 1 ;;
            *) return 1 ;;
        esac
    done
}

installer_cleanup_work_transaction() {
    local txn="$1" source="$2" name
    installer_work_transaction_safe "$txn" "$source" || return 1
    for name in previous prepared published-failed published-interrupted; do
        [[ -e "$txn/$name" ]] || continue
        rm -rf -- "$txn/$name" || return 1
    done
    for name in state-sha256.txt binary-sha256.txt verified.txt target-path.txt; do
        [[ -e "$txn/$name" ]] || continue
        rm -- "$txn/$name" || return 1
    done
    rmdir -- "$txn"
}

installer_work_transaction_pending() {
    local final="$1" parent="${1%/*}" entry record
    for entry in "$parent"/.work-install.*; do
        [[ -d "$entry" && ! -L "$entry" ]] || continue
        record="$entry/target-path.txt"
        [[ -f "$record" && ! -L "$record" ]] || continue
        [[ "$(cat -- "$record")" == "$final" ]] && return 0
    done
    return 1
}

installer_recover_interrupted_work() {
    local final="$1" source="$2" parent="${1%/*}" entry record txn='' previous count=0 interrupted
    for entry in "$parent"/.work-install.*; do
        [[ -d "$entry" && ! -L "$entry" ]] || continue
        record="$entry/target-path.txt"
        [[ -f "$record" && ! -L "$record" ]] || continue
        [[ "$(cat -- "$record")" == "$final" ]] || continue
        txn="$entry"
        count=$((count + 1))
    done
    (( count <= 1 )) || { printf 'Error: multiple recovery directories claim Work.\n' >&2; return 1; }
    (( count == 1 )) || return 0
    installer_work_transaction_safe "$txn" "$source" || {
        printf 'Error: unsafe Work recovery directory: %s\n' "$txn" >&2; return 1;
    }
    previous="$txn/previous"
    if [[ -f "$txn/verified.txt" ]]; then
        installer_work_matches_record "$final" "$source" "$txn" || {
            printf 'Error: verified Work no longer matches recovery record: %s\n' "$txn" >&2; return 1;
        }
        if ! installer_cleanup_work_transaction "$txn" "$source"; then
            printf 'Warning: verified Work retained; old transaction awaits cleanup: %s\n' "$txn" >&2
        fi
        return 0
    fi
    if [[ -e "$previous" ]]; then
        installer_work_directory_safe "$previous" "$source" || {
            printf 'Error: unsafe previous Work directory: %s\n' "$previous" >&2; return 1;
        }
        if [[ -e "$final" || -L "$final" ]]; then
            installer_work_matches_record "$final" "$source" "$txn" || {
                printf 'Error: unverified final Work cannot be identified: %s\n' "$final" >&2; return 1;
            }
            interrupted="$txn/published-interrupted"
            [[ ! -e "$interrupted" && ! -L "$interrupted" ]] || return 1
            mv -- "$final" "$interrupted" || return 1
        fi
        if ! mv -- "$previous" "$final"; then
            printf 'Error: Work restore failed; previous installation remains at %s\n' "$previous" >&2
            return 1
        fi
    elif [[ ! -e "$txn/prepared" && ( -e "$final" || -L "$final" ) ]] &&
         installer_work_matches_record "$final" "$source" "$txn"; then
        interrupted="$txn/published-interrupted"
        [[ ! -e "$interrupted" && ! -L "$interrupted" ]] || return 1
        mv -- "$final" "$interrupted" || return 1
    elif [[ ! -e "$txn/prepared" && ! -e "$txn/published-failed" &&
            ! -e "$txn/published-interrupted" && ( -e "$final" || -L "$final" ) ]]; then
        printf 'Error: unverified first installation cannot be identified: %s\n' "$final" >&2
        return 1
    fi
    installer_cleanup_work_transaction "$txn" "$source" || {
        printf 'Error: incomplete Work transaction could not be cleaned: %s\n' "$txn" >&2; return 1;
    }
}

installer_publish_work() {
    local final="$1" source="$2" state_valid="$3" confirmed="$4" non_interactive="$5"
    local prepare_fn="$6" validate_fn="$7" failure_point="${8:-}" defer_cleanup="${9:-0}" parent txn prepared previous failed state_hash binary_hash
    [[ "$final" == /* && "${final##*/}" == work && "$final" != */../* ]] || {
        printf 'Error: Work target must be an absolute work directory.\n' >&2; return 1;
    }
    parent="${final%/*}"
    [[ -d "$parent" && ! -L "$parent" ]] || return 1
    installer_recover_interrupted_work "$final" "$source" || return 1
    installer_inspect_target "$final" "$source" "$state_valid"
    installer_target_allows_install "$non_interactive" "$confirmed" || return 1
    txn="$(mktemp -d "$parent/.work-install.XXXXXXXX")" || return 1
    prepared="$txn/prepared"
    previous="$txn/previous"
    printf '%s\n' "$final" > "$txn/target-path.txt" || return 1
    mkdir -- "$prepared" || return 1
    if ! "$prepare_fn" "$prepared" || ! "$validate_fn" "$prepared" ||
        ! installer_work_directory_safe "$prepared" "$source" || [[ "$failure_point" == prepare ]]; then
        printf 'Error: Work preparation failed; recovery directory: %s\n' "$txn" >&2
        return 1
    fi
    state_hash="$(shasum -a 256 -- "$prepared/install-state.json")" || return 1
    binary_hash="$(shasum -a 256 -- "$prepared/scripts/work")" || return 1
    printf '%s\n' "${state_hash%% *}" > "$txn/state-sha256.txt" || return 1
    printf '%s\n' "${binary_hash%% *}" > "$txn/binary-sha256.txt" || return 1
    installer_inspect_target "$final" "$source" "$state_valid"
    if ! installer_target_allows_install "$non_interactive" "$confirmed"; then
        printf 'Error: Work target changed before publish; recovery directory: %s\n' "$txn" >&2
        return 1
    fi
    if [[ -e "$final" ]]; then
        mv -- "$final" "$previous" || return 1
    fi
    if [[ "$failure_point" == replace || "$failure_point" == restore ]] ||
        ! mv -- "$prepared" "$final" || ! "$validate_fn" "$final" ||
        ! installer_work_directory_safe "$final" "$source" || [[ "$failure_point" == verify ]]; then
        if [[ -e "$final" ]]; then
            failed="$txn/published-failed"
            mv -- "$final" "$failed" || return 1
        fi
        if [[ -e "$previous" ]]; then
            if [[ "$failure_point" == restore ]] || ! mv -- "$previous" "$final"; then
                printf 'Error: Work restore failed; previous installation remains at %s\n' "$previous" >&2
                return 1
            fi
        fi
        printf 'Error: Work publish failed; recovery directory: %s\n' "$txn" >&2
        return 1
    fi
    printf 'verified\n' > "$txn/verified.txt" || return 1
    if [[ "$defer_cleanup" == 1 ]]; then
        installer_last_work_transaction="$txn"
        printf '%s\n' "$final"
        return 0
    fi
    if [[ "$failure_point" == cleanup ]] || ! installer_cleanup_work_transaction "$txn" "$source"; then
        printf 'Warning: Work installed; old transaction awaits cleanup: %s\n' "$txn" >&2
    fi
    printf '%s\n' "$final"
}

installer_publish_combined_work() {
    local final="$1" source="$2" state_valid="$3" confirmed="$4" non_interactive="$5"
    local prepare_fn="$6" validate_fn="$7" global_source="$8" global_target="$9"
    local action="${10}" source_hash="${11}" target_hash="${12}" work_failure="${13:-}" global_failure="${14:-}"
    local global_txn parent work_txn
    if [[ "$action" == skip || "$action" == unchanged ]]; then
        installer_publish_work "$final" "$source" "$state_valid" "$confirmed" "$non_interactive" \
            "$prepare_fn" "$validate_fn" "$work_failure"
        return $?
    fi
    installer_inspect_target "$final" "$source" "$state_valid"
    installer_target_allows_install "$non_interactive" "$confirmed" || return 1
    parent="${final%/*}"
    global_txn="$(mktemp -d "$parent/.agents-install.XXXXXXXX")" || return 1
    installer_global_record_transaction "$global_txn" "$final" "$global_target" "$action" "$source_hash" "$target_hash" || return 1
    if ! installer_global_agents_stage "$global_source" "$global_target" "$global_txn" \
        "$action" "$source_hash" "$target_hash" "$global_failure"; then
        printf 'Error: Global AGENTS stage failed; recovery directory: %s\n' "$global_txn" >&2
        return 1
    fi
    if ! installer_publish_work "$final" "$source" "$state_valid" "$confirmed" "$non_interactive" \
        "$prepare_fn" "$validate_fn" "$work_failure" 1; then
        if ! installer_global_agents_undo "$global_target" "$global_txn" "$action" "$source_hash" "$target_hash"; then
            printf 'Error: Global AGENTS rollback failed; recovery directory: %s\n' "$global_txn" >&2
        fi
        return 1
    fi
    work_txn="$installer_last_work_transaction"
    if ! installer_global_mark_work_verified "$global_txn" "$work_txn"; then
        printf 'Warning: Work installed; combined transaction awaits verification cleanup: %s\n' "$global_txn" >&2
        return 0
    fi
    if ! installer_cleanup_work_transaction "$work_txn" "$source"; then
        printf 'Warning: Work installed; old transaction awaits cleanup: %s\n' "$work_txn" >&2
    fi
    if ! installer_global_cleanup_transaction "$global_txn"; then
        printf 'Warning: Global AGENTS previous file awaits cleanup: %s\n' "$global_txn" >&2
    fi
}
