#!/usr/bin/env bash

installer_migration_record() { printf '%s/.work-migration-cleanup' "${1%/*}"; }

installer_migration_tree_hash() {
    local path="$1" file relative digest
    [[ -d "$path" && ! -L "$path" ]] || return 1
    while IFS= read -r -d '' file; do
        [[ -f "$file" && ! -L "$file" ]] || return 1
        relative="${file#"$path"/}"
        [[ "$relative" != *$'\n'* ]] || return 1
        digest="$(shasum -a 256 -- "$file")" || return 1
        printf '%s  %s\n' "${digest%% *}" "$relative"
    done < <(find "$path" -type f -print0 | sort -z)
}

installer_migration_writable() {
    local path="$1" directory
    [[ -w "${path%/*}" ]] || return 1
    while IFS= read -r -d '' directory; do
        [[ -w "$directory" ]] || return 1
    done < <(find "$path" -type d -print0)
}

installer_migration_write_record() {
    local target="$1" old="$2" source="$3" state="$4" record tree_hash state_hash temporary
    installer_inspect_target "$old" "$source" 1
    [[ "$installer_target_kind" == managed && "$old" != *$'\n'* && "$target" != *$'\n'* ]] || return 1
    record="$(installer_migration_record "$target")"
    tree_hash="$(installer_migration_tree_hash "$old" | shasum -a 256)" || return 1
    state_hash="$(printf '%s\n' "$state" | shasum -a 256)" || return 1
    if [[ -e "$record" || -L "$record" ]]; then
        [[ -f "$record" && ! -L "$record" ]] || return 1
        local recorded_target recorded_old recorded_state recorded_tree
        { IFS= read -r recorded_target && IFS= read -r recorded_old &&
          IFS= read -r recorded_state && IFS= read -r recorded_tree; } < "$record" || return 1
        [[ "$recorded_target" == "$target" && "$recorded_old" == "$old" &&
           "$recorded_tree" == "${tree_hash%% *}" ]] || return 1
    fi
    temporary="$(mktemp "${record}.XXXXXXXX")" || return 1
    if ! printf '%s\n%s\n%s\n%s\n' "$target" "$old" "${state_hash%% *}" "${tree_hash%% *}" > "$temporary" ||
       ! mv -- "$temporary" "$record"; then
        rm -f -- "$temporary"
        return 1
    fi
}

installer_migration_retry_cleanup() {
    local target="$1" source="$2" record recorded_target old state_hash tree_hash actual
    record="$(installer_migration_record "$target")"
    [[ -e "$record" || -L "$record" ]] || return 0
    [[ -f "$record" && ! -L "$record" ]] || { printf 'Warning: unsafe migration cleanup record: %s\n' "$record" >&2; return 0; }
    { IFS= read -r recorded_target && IFS= read -r old && IFS= read -r state_hash && IFS= read -r tree_hash; } < "$record" || return 0
    if [[ "$recorded_target" != "$target" || "$old" != /* || "$old" == "$target" ||
          ! "$state_hash" =~ ^[0-9a-f]{64}$ || ! "$tree_hash" =~ ^[0-9a-f]{64}$ ]]; then
        printf 'Warning: invalid migration cleanup record: %s\n' "$record" >&2; return 0
    fi
    if [[ ! -e "$old" && ! -L "$old" ]]; then rm -- "$record" || true; return 0; fi
    installer_inspect_target "$target" "$source" 1
    [[ "$installer_target_kind" == managed ]] || { printf 'Warning: migration target cannot be verified: %s\n' "$target" >&2; return 0; }
    actual="$(shasum -a 256 -- "$target/install-state.json")" || return 0
    [[ "${actual%% *}" == "$state_hash" ]] || { printf 'Warning: migration target changed: %s\n' "$target" >&2; return 0; }
    installer_inspect_target "$old" "$source" 1
    [[ "$installer_target_kind" == managed ]] || { printf 'Warning: previous Work changed: %s\n' "$old" >&2; return 0; }
    actual="$(installer_migration_tree_hash "$old" | shasum -a 256)" || return 0
    [[ "${actual%% *}" == "$tree_hash" ]] || { printf 'Warning: previous Work contents changed: %s\n' "$old" >&2; return 0; }
    if installer_binary_in_use "$old/scripts/work"; then
        printf 'Warning: previous Work is in use: %s\n' "$old" >&2; return 0
    fi
    if ! installer_migration_writable "$old"; then
        printf 'Warning: previous Work cannot be removed yet: %s\n' "$old" >&2; return 0
    fi
    if rm -rf -- "$old"; then rm -- "$record" || true
    else printf 'Warning: previous Work remains for later cleanup: %s\n' "$old" >&2; fi
}
