#!/usr/bin/env bash

# Verify every package file before either installer writes an installation target.
verify_release_package() {
    local root="$1" manifest line hash relative entry actual_hash found count=0 index
    local -a paths=() hashes=()
    if [[ ! -d "$root" || -L "$root" ]]; then
        printf 'Error: release package root is not a real directory.\n' >&2
        return 1
    fi
    root="$(cd -- "$root" && pwd -P)" || return 1
    manifest="$root/SHA256SUMS"
    if [[ ! -f "$manifest" || -L "$manifest" ]]; then
        printf 'Error: release SHA256SUMS is missing or is a symlink.\n' >&2
        return 1
    fi
    if [[ "$(tail -c 1 "$manifest" | od -An -tu1 | tr -d '[:space:]')" != 10 ]]; then
        printf 'Error: release SHA256SUMS must end with LF.\n' >&2
        return 1
    fi
    while IFS= read -r line; do
        hash="${line:0:64}"
        relative="${line:66}"
        if [[ "${line:64:2}" != '  ' || ! "$hash" =~ ^[0-9a-f]{64}$ || ! "$relative" =~ ^[A-Za-z0-9._/-]+$ || "$relative" == /* || "$relative" == */ || "$relative" == SHA256SUMS || "/$relative/" == *'/../'* || "/$relative/" == *'/./'* || "$relative" == *'//'* ]]; then
            printf 'Error: unsafe release SHA256SUMS entry.\n' >&2
            return 1
        fi
        for entry in "${paths[@]}"; do
            if [[ "$entry" == "$relative" ]]; then
                printf 'Error: duplicate release SHA256SUMS path: %s\n' "$relative" >&2
                return 1
            fi
        done
        paths+=("$relative")
        hashes+=("$hash")
    done < "$manifest"
    if (( ${#paths[@]} == 0 )); then
        printf 'Error: release SHA256SUMS is empty.\n' >&2
        return 1
    fi
    while IFS= read -r -d '' entry; do
        relative="${entry#"$root"/}"
        if [[ ! "$relative" =~ ^[A-Za-z0-9._/-]+$ || "$relative" == /* || "$relative" == */ || "/$relative/" == *'/../'* || "/$relative/" == *'/./'* || "$relative" == *'//'* || -L "$entry" ]]; then
            printf 'Error: unsafe release package path: %s\n' "$relative" >&2
            return 1
        fi
        if [[ -d "$entry" ]]; then
            continue
        fi
        if [[ ! -f "$entry" ]]; then
            printf 'Error: special file in release package: %s\n' "$relative" >&2
            return 1
        fi
        if [[ "$relative" == SHA256SUMS ]]; then
            continue
        fi
        found=0
        for (( index=0; index<${#paths[@]}; index++ )); do
            if [[ "${paths[index]}" == "$relative" ]]; then
                found=1
                actual_hash="$(shasum -a 256 -- "$entry")" || return 1
                if [[ "${actual_hash:0:64}" != "${hashes[index]}" ]]; then
                    printf 'Error: release SHA-256 mismatch: %s\n' "$relative" >&2
                    return 1
                fi
                break
            fi
        done
        if (( ! found )); then
            printf 'Error: extra release package file: %s\n' "$relative" >&2
            return 1
        fi
        (( count++ ))
    done < <(find "$root" -mindepth 1 -print0)
    if (( count != ${#paths[@]} )); then
        printf 'Error: release package file list differs from SHA256SUMS.\n' >&2
        return 1
    fi
    printf 'Release package SHA-256 verification passed.\n'
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    if (( $# != 1 )); then
        printf 'Usage: %s PACKAGE_ROOT\n' "$0" >&2
        exit 2
    fi
    verify_release_package "$1"
fi
