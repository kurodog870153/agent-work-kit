#!/usr/bin/env bash

installer_tooling_transaction_safe() {
    local txn="$1" source="$2" entry name
    [[ -d "$txn" && ! -L "$txn" ]] || return 1
    for entry in "$txn"/* "$txn"/.[!.]*; do
        [[ -e "$entry" || -L "$entry" ]] || continue
        [[ ! -L "$entry" ]] || return 1
        name="${entry##*/}"
        case "$name" in
            target-path.txt|state-sha256.txt|verified.txt) [[ -f "$entry" ]] || return 1 ;;
            prepared|previous|published-failed|published-interrupted)
                installer_tooling_subset_safe "$entry" "$source" || return 1 ;;
            *) return 1 ;;
        esac
    done
}

installer_tooling_cleanup() {
    local txn="$1" source="$2" name
    installer_tooling_transaction_safe "$txn" "$source" || return 1
    for name in previous prepared published-failed published-interrupted; do
        if [[ -e "$txn/$name" ]]; then rm -rf -- "$txn/$name" || return 1; fi
    done
    for name in target-path.txt state-sha256.txt verified.txt; do
        if [[ -e "$txn/$name" ]]; then rm -- "$txn/$name" || return 1; fi
    done
    rmdir -- "$txn"
}

installer_tooling_transaction_pending() {
    local target="$1" parent="${1%/*}" entry
    for entry in "$parent"/.tooling-install.*; do
        [[ -d "$entry" && ! -L "$entry" && -f "$entry/target-path.txt" &&
           ! -L "$entry/target-path.txt" ]] || continue
        [[ "$(cat -- "$entry/target-path.txt")" == "$target" ]] && return 0
    done
    return 1
}

installer_tooling_recover() {
    local target="$1" source="$2" version="$3" binary="$4" parent="${1%/*}" entry txn='' count=0 previous hash
    for entry in "$parent"/.tooling-install.*; do
        [[ -d "$entry" && ! -L "$entry" && -f "$entry/target-path.txt" &&
           ! -L "$entry/target-path.txt" ]] || continue
        [[ "$(cat -- "$entry/target-path.txt")" == "$target" ]] || continue
        txn="$entry"; count=$((count + 1))
    done
    (( count <= 1 )) || { installer_fail 'multiple Tooling transactions claim target'; return 1; }
    (( count == 1 )) || return 0
    installer_tooling_transaction_safe "$txn" "$source" || { installer_fail "unsafe Tooling recovery directory: $txn"; return 1; }
    previous="$txn/previous"
    if [[ -f "$txn/verified.txt" ]]; then
        installer_tooling_validate_payload "$target" "$source" "$installer_tooling_state_module" "$version" "$target" "$binary" || return 1
        hash="$(shasum -a 256 -- "$target/install-state.json")" || return 1
        [[ "${hash%% *}" == "$(cat -- "$txn/state-sha256.txt")" ]] || return 1
        installer_tooling_cleanup "$txn" "$source" || return 1
        return 0
    fi
    if [[ -e "$previous" ]]; then
        if [[ -e "$target" ]]; then
            installer_tooling_validate_payload "$target" "$source" "$installer_tooling_state_module" "$version" "$target" "$binary" || return 1
            [[ ! -e "$txn/published-interrupted" ]] || return 1
            mv -- "$target" "$txn/published-interrupted" || return 1
        fi
        mv -- "$previous" "$target" || return 1
    elif [[ ! -e "$txn/prepared" && -e "$target" ]]; then
        installer_tooling_validate_payload "$target" "$source" "$installer_tooling_state_module" "$version" "$target" "$binary" || return 1
        [[ ! -e "$txn/published-interrupted" ]] || return 1
        mv -- "$target" "$txn/published-interrupted" || return 1
    fi
    installer_tooling_cleanup "$txn" "$source"
}

installer_tooling_publish() {
    local target="$1" source="$2" version="$3" state_json="$4" non_interactive="$5" confirmed="$6" binary="$7"
    local parent="${1%/*}" txn prepared previous failed hash
    installer_tooling_recover "$target" "$source" "$version" "$binary" || return 1
    installer_tooling_inspect "$target" "$source"
    [[ "$installer_tooling_kind" == new || "$installer_tooling_kind" == managed ||
       "$installer_tooling_kind" == first-update && "$non_interactive" != 1 && "$confirmed" == 1 ]] || return 1
    txn="$(mktemp -d "$parent/.tooling-install.XXXXXXXX")" || return 1
    prepared="$txn/prepared"; previous="$txn/previous"
    printf '%s\n' "$target" > "$txn/target-path.txt" || return 1
    mkdir -- "$prepared" || return 1
    cp -R -- "$source/." "$prepared/" || return 1
    mkdir -- "$prepared/scripts" || return 1
    cp -- "$binary" "$prepared/scripts/tooling" || return 1
    printf '%s\n' "$state_json" > "$prepared/install-state.json" || return 1
    installer_tooling_validate_payload "$prepared" "$source" "$installer_tooling_state_module" "$version" "$target" "$binary" || {
        installer_fail "Tooling preparation failed; recovery directory: $txn"; return 1;
    }
    hash="$(shasum -a 256 -- "$prepared/install-state.json")" || return 1
    printf '%s\n' "${hash%% *}" > "$txn/state-sha256.txt" || return 1
    installer_tooling_inspect "$target" "$source"
    [[ "$installer_tooling_kind" == new || "$installer_tooling_kind" == managed ||
       "$installer_tooling_kind" == first-update && "$non_interactive" != 1 && "$confirmed" == 1 ]] || return 1
    [[ ! -e "$target" ]] || mv -- "$target" "$previous" || return 1
    if ! mv -- "$prepared" "$target" ||
       ! installer_tooling_validate_payload "$target" "$source" "$installer_tooling_state_module" "$version" "$target" "$binary"; then
        if [[ -e "$target" ]]; then failed="$txn/published-failed"; mv -- "$target" "$failed" || return 1; fi
        [[ ! -e "$previous" ]] || mv -- "$previous" "$target" || return 1
        installer_fail "Tooling publish failed; recovery directory: $txn"; return 1
    fi
    printf 'verified\n' > "$txn/verified.txt" || return 1
    installer_tooling_cleanup "$txn" "$source" || printf 'Warning: verified Tooling retained; transaction awaits cleanup: %s\n' "$txn" >&2
}
