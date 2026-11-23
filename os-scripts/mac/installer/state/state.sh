#!/usr/bin/env bash

# One authoritative State, strict fixed layout; no legacy schema or migration.
installer_version_valid() { [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; }
installer_path_valid() { [[ "$1" == /* && "$1" != *[[:cntrl:]\"\\]* ]]; }
installer_relative_valid() { [[ "$1" =~ ^[A-Za-z0-9._/-]+$ && "/$1/" != *'/../'* && "/$1/" != *'/./'* && "$1" != /* && "$1" != */ && "$1" != *//* ]]; }

installer_sha256() {
    local digest
    digest="$(shasum -a 256 -- "$1")" || return 1
    printf '%s\n' "${digest%% *}"
}

installer_state_clear() {
    installer_state_host=''
    installer_state_version=''
    installer_state_rust_version=''
    installer_state_install_dir=''
    installer_state_global_dir=''
    installer_state_files=()
}

installer_state_read() {
    local file="$1" line index=0 count last hash relative comma
    local re_host='^  "host": "(codex|claude)",$'
    local re_version='^  "version": "([0-9]+\.[0-9]+\.[0-9]+)",$'
    local re_rust='^  "rust_workspace_version": "([0-9]+\.[0-9]+\.[0-9]+)",$'
    local re_install='^  "install_dir": "([^"\\]+)",$'
    local re_global='^  "global_instructions_dir": (null|"([^"\\]+)"),$'
    local re_entry='^    "([0-9a-f]{64})  ([A-Za-z0-9._/-]+)"(,?)$'
    local -a lines=()
    installer_state_clear
    [[ -f "$file" && ! -L "$file" ]] || return 1
    while IFS= read -r line || [[ -n "$line" ]]; do lines+=("$line"); done < "$file"
    count=${#lines[@]}
    (( count >= 11 )) || return 1
    [[ "${lines[0]}" == '{' && "${lines[count-1]}" == '}' && "${lines[count-2]}" == '  ]' ]] || return 1
    [[ "${lines[1]}" =~ $re_host ]] && installer_state_host="${BASH_REMATCH[1]}" || return 1
    [[ "${lines[2]}" =~ $re_version ]] && installer_state_version="${BASH_REMATCH[1]}" || return 1
    [[ "${lines[3]}" =~ $re_rust ]] && installer_state_rust_version="${BASH_REMATCH[1]}" || return 1
    [[ "${lines[4]}" =~ $re_install ]] && installer_state_install_dir="${BASH_REMATCH[1]}" || return 1
    [[ "${lines[5]}" =~ $re_global ]] || return 1
    installer_state_global_dir="${BASH_REMATCH[2]}"
    [[ "${lines[6]}" == '  "status": "installed",' && "${lines[7]}" == '  "files": [' ]] || return 1
    installer_path_valid "$installer_state_install_dir" || return 1
    [[ -z "$installer_state_global_dir" ]] || installer_path_valid "$installer_state_global_dir" || return 1
    last=$((count - 3))
    for ((index=8; index<=last; index++)); do
        [[ "${lines[index]}" =~ $re_entry ]] || { installer_state_clear; return 1; }
        hash="${BASH_REMATCH[1]}"; relative="${BASH_REMATCH[2]}"; comma="${BASH_REMATCH[3]}"
        if (( index == last )); then [[ -z "$comma" ]] || { installer_state_clear; return 1; }
        else [[ -n "$comma" ]] || { installer_state_clear; return 1; }; fi
        installer_relative_valid "$relative" || { installer_state_clear; return 1; }
        installer_state_files+=("$hash  $relative")
    done
    (( ${#installer_state_files[@]} > 0 ))
}

# installer_state_write FILE HOST VERSION RUST_VERSION INSTALL_DIR GLOBAL_DIR_OR_EMPTY ENTRY...
installer_state_write() {
    local file="$1" host="$2" version="$3" rust="$4" install="$5" global="$6" entry index=0 total
    shift 6
    total=$#
    {
        printf '{\n  "host": "%s",\n  "version": "%s",\n  "rust_workspace_version": "%s",\n' "$host" "$version" "$rust"
        printf '  "install_dir": "%s",\n' "$install"
        if [[ -n "$global" ]]; then printf '  "global_instructions_dir": "%s",\n' "$global"
        else printf '  "global_instructions_dir": null,\n'; fi
        printf '  "status": "installed",\n  "files": [\n'
        for entry in "$@"; do
            index=$((index + 1))
            if (( index < total )); then printf '    "%s",\n' "$entry"; else printf '    "%s"\n' "$entry"; fi
        done
        printf '  ]\n}\n'
    } > "$file"
}

# Exact SHA-256 manifest of managed Skills and CLI files; never scan the Host Skills parent.
installer_actual_manifest() {
    local install="$1" staged="${2:-0}" name dir file relative
    for name in work tooling shared; do
        if [[ "$staged" == 1 ]]; then dir="$install/$name"; else dir="$install/skills/$name"; fi
        [[ -e "$dir" || -L "$dir" ]] || continue
        [[ -d "$dir" && ! -L "$dir" ]] || return 1
        while IFS= read -r -d '' file; do
            [[ -f "$file" && ! -L "$file" ]] || return 1
            if [[ "$staged" == 1 ]]; then relative="skills/$name/${file#"$dir"/}"
            else relative="${file#"$install"/}"; fi
            installer_relative_valid "$relative" || return 1
            printf '%s  %s\n' "$(installer_sha256 "$file")" "$relative"
        done < <(find "$dir" -type f -print0)
        [[ -z "$(find "$dir" ! -type f ! -type d -print -quit)" ]] || return 1
    done
    dir="$install/bin"
    if [[ -e "$dir" || -L "$dir" ]]; then
        [[ -d "$dir" && ! -L "$dir" ]] || return 1
        while IFS= read -r -d '' file; do
            [[ -f "$file" && ! -L "$file" ]] || return 1
            relative="${file#"$install"/}"
            installer_relative_valid "$relative" || return 1
            printf '%s  %s\n' "$(installer_sha256 "$file")" "$relative"
        done < <(find "$dir" -type f -print0)
        [[ -z "$(find "$dir" ! -type f ! -type d -print -quit)" ]] || return 1
    fi
}

# installer_manifest_matches INSTALL_DIR ENTRY...; Global Instructions are verified independently.
installer_manifest_matches() {
    local install="$1" expected actual
    shift
    expected="$(printf '%s\n' "$@" | grep -v '^[0-9a-f]\{64\}  global/' | LC_ALL=C sort)" || true
    actual="$(installer_actual_manifest "$install")" || return 1
    actual="$(printf '%s\n' "$actual" | LC_ALL=C sort)"
    [[ "$expected" == "$actual" ]]
}
