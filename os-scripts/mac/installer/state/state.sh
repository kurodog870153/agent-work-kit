#!/usr/bin/env bash

# The single authoritative install state. It is written in one fixed layout and parsed strictly,
# so any deviation (hand edits, partial writes, other tools) makes the state invalid.
installer_version_valid() { [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; }
installer_path_valid() { [[ "$1" == /* && "$1" != *[[:cntrl:]\"\]* ]]; }
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
    installer_state_skills_dir=''
    installer_state_cli_dir=''
    installer_state_global_dir=''
    installer_state_files=()
}

# installer_state_read FILE: fills installer_state_* from a valid state; fails on any deviation.
installer_state_read() {
    local file="$1" line index=0 count last hash relative comma
    local re_host='^  "host": "(codex|claude)",$'
    local re_version='^  "version": "([0-9]+\.[0-9]+\.[0-9]+)",$'
    local re_rust='^  "rust_workspace_version": "([0-9]+\.[0-9]+\.[0-9]+)",$'
    local re_skills='^  "skill_install_dir": "([^"\]+)",$'
    local re_cli='^  "cli_install_dir": "([^"\]+)",$'
    local re_global='^  "global_instructions_dir": (null|"([^"\]+)"),$'
    local re_entry='^    "([0-9a-f]{64})  ([A-Za-z0-9._/-]+)"(,?)$'
    local -a lines=()
    installer_state_clear
    [[ -f "$file" && ! -L "$file" ]] || return 1
    while IFS= read -r line || [[ -n "$line" ]]; do lines+=("$line"); done < "$file"
    count=${#lines[@]}
    (( count >= 12 )) || return 1
    [[ "${lines[0]}" == '{' && "${lines[count-1]}" == '}' && "${lines[count-2]}" == '  ]' ]] || return 1
    [[ "${lines[1]}" =~ $re_host ]] && installer_state_host="${BASH_REMATCH[1]}" || return 1
    [[ "${lines[2]}" =~ $re_version ]] && installer_state_version="${BASH_REMATCH[1]}" || return 1
    [[ "${lines[3]}" =~ $re_rust ]] && installer_state_rust_version="${BASH_REMATCH[1]}" || return 1
    [[ "${lines[4]}" =~ $re_skills ]] && installer_state_skills_dir="${BASH_REMATCH[1]}" || return 1
    [[ "${lines[5]}" =~ $re_cli ]] && installer_state_cli_dir="${BASH_REMATCH[1]}" || return 1
    [[ "${lines[6]}" =~ $re_global ]] || return 1
    installer_state_global_dir="${BASH_REMATCH[2]}"
    [[ "${lines[7]}" == '  "status": "installed",' && "${lines[8]}" == '  "files": [' ]] || return 1
    installer_path_valid "$installer_state_skills_dir" && installer_path_valid "$installer_state_cli_dir" || return 1
    [[ -z "$installer_state_global_dir" ]] || installer_path_valid "$installer_state_global_dir" || return 1
    last=$((count - 3))
    for ((index=9; index<=last; index++)); do
        [[ "${lines[index]}" =~ $re_entry ]] || { installer_state_clear; return 1; }
        hash="${BASH_REMATCH[1]}"; relative="${BASH_REMATCH[2]}"; comma="${BASH_REMATCH[3]}"
        if (( index == last )); then [[ -z "$comma" ]] || { installer_state_clear; return 1; }
        else [[ -n "$comma" ]] || { installer_state_clear; return 1; }; fi
        installer_relative_valid "$relative" || { installer_state_clear; return 1; }
        installer_state_files+=("$hash  $relative")
    done
    (( ${#installer_state_files[@]} > 0 )) || return 1
}

# installer_state_write FILE HOST VERSION RUST_VERSION SKILLS_DIR CLI_DIR GLOBAL_DIR_OR_EMPTY ENTRY...
installer_state_write() {
    local file="$1" host="$2" version="$3" rust="$4" skills="$5" cli="$6" global="$7" entry index=0 total
    shift 7
    total=$#
    {
        printf '{\n  "host": "%s",\n  "version": "%s",\n  "rust_workspace_version": "%s",\n' "$host" "$version" "$rust"
        printf '  "skill_install_dir": "%s",\n  "cli_install_dir": "%s",\n' "$skills" "$cli"
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

# Hash lines ("sha  skills/<name>/<file>" or "sha  cli/bin/<file>") of the managed files on disk.
installer_actual_manifest() {
    local skills="$1" cli="$2" name file relative
    for name in work tooling shared; do
        [[ -e "$skills/$name" || -L "$skills/$name" ]] || continue
        while IFS= read -r -d '' file; do
            [[ -f "$file" && ! -L "$file" ]] || return 1
            relative="${file#"$skills"/}"
            installer_relative_valid "$relative" || return 1
            printf '%s  skills/%s\n' "$(installer_sha256 "$file")" "$relative"
        done < <(find "$skills/$name" -type f -print0)
        [[ -z "$(find "$skills/$name" ! -type f ! -type d -print -quit)" ]] || return 1
    done
    if [[ -e "$cli/bin" || -L "$cli/bin" ]]; then
        while IFS= read -r -d '' file; do
            [[ -f "$file" && ! -L "$file" ]] || return 1
            printf '%s  cli/bin/%s\n' "$(installer_sha256 "$file")" "${file#"$cli"/bin/}"
        done < <(find "$cli/bin" -type f -print0)
        [[ -z "$(find "$cli/bin" ! -type f ! -type d -print -quit)" ]] || return 1
    fi
}

# installer_manifest_matches SKILLS CLI ENTRY...: the managed files on disk equal the entries exactly
# (global/ entries describe the Global Instructions file and are checked separately).
installer_manifest_matches() {
    local skills="$1" cli="$2" expected actual
    shift 2
    expected="$(printf '%s\n' "$@" | grep -v '^[0-9a-f]\{64\}  global/' | LC_ALL=C sort)" || true
    actual="$(installer_actual_manifest "$skills" "$cli")" || return 1
    actual="$(printf '%s
' "$actual" | LC_ALL=C sort)"
    [[ "$expected" == "$actual" ]]
}
