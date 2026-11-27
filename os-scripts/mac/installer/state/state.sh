#!/usr/bin/env bash

# One authoritative State, strict semantic validation; no legacy schema or migration.
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

# Decode using the macOS system JavaScript engine; no third-party parser or installed CLI.
# JSON.parse alone silently accepts repeated keys, so check decoded keys in each object.
installer_state_decode() {
    /usr/bin/osascript -l JavaScript - "$1" <<'JXA'
ObjC.import('Foundation');
function exactKeys(value, keys) {
    return value !== null && typeof value === 'object' && !Array.isArray(value) &&
        Object.keys(value).length === keys.length &&
        keys.every(function (key) { return Object.prototype.hasOwnProperty.call(value, key); });
}
function rejectDuplicates(text) {
    var tokens = /"(?:[^"\\]|\\.)*"|[{}\[\]:,]/g, stack = [], match;
    while ((match = tokens.exec(text)) !== null) {
        var token = match[0];
        if (token === '{') stack.push([]);
        else if (token === '[') stack.push(null);
        else if (token === '}' || token === ']') stack.pop();
        else if (token[0] === '"' && /^[ \t\r\n]*:/.test(text.slice(tokens.lastIndex))) {
            var keys = stack[stack.length - 1], key = JSON.parse(token);
            if (!keys || keys.indexOf(key) !== -1) throw new Error('duplicate JSON key');
            keys.push(key);
        }
    }
}
function run(argv) {
    var text = ObjC.unwrap($.NSString.stringWithContentsOfFileEncodingError(argv[0], $.NSUTF8StringEncoding, null));
    if (typeof text !== 'string') throw new Error('invalid state encoding');
    var state = JSON.parse(text);
    rejectDuplicates(text);
    if (!exactKeys(state, ['host', 'version', 'rust_workspace_version', 'install_dir', 'global_instructions_dir', 'status', 'files']) ||
        (state.host !== 'codex' && state.host !== 'claude') || state.status !== 'installed' ||
        !Array.isArray(state.files) || state.files.length === 0) throw new Error('invalid state structure');
    function safeString(value) {
        return typeof value === 'string' && value.length > 0 && !/[\u0000-\u001f\u007f]/.test(value);
    }
    if (![state.version, state.rust_workspace_version, state.install_dir].every(safeString) ||
        (state.global_instructions_dir !== null && (!safeString(state.global_instructions_dir) || state.global_instructions_dir[0] !== '/'))) throw new Error('invalid state fields');
    var result = [state.host, state.version, state.rust_workspace_version, state.install_dir,
        state.global_instructions_dir === null ? '-' : state.global_instructions_dir];
    state.files.forEach(function (file) {
        if (!exactKeys(file, ['path', 'sha256']) || !safeString(file.path) || !safeString(file.sha256) ||
            !/^[A-Za-z0-9._/-]+$/.test(file.path) || !/^[0-9a-f]{64}$/.test(file.sha256)) throw new Error('invalid file record');
        result.push(file.sha256 + '  ' + file.path);
    });
    // Tab is excluded from all accepted values; avoid Apple event newline conversion.
    return result.join('\t');
}
JXA
}

installer_state_read() {
    local file="$1" parsed line index count hash relative seen="|" global_count=0 global_name
    local re_entry='^([0-9a-f]{64})  ([A-Za-z0-9._/-]+)$'
    local -a lines=() entries=()
    installer_state_clear
    [[ -f "$file" && ! -L "$file" ]] || return 1
    parsed="$(installer_state_decode "$file")" || return 1
    while IFS= read -r line; do lines+=("$line"); done <<< "$(printf '%s' "$parsed" | tr '\t' '\n')"
    count=${#lines[@]}
    (( count >= 7 )) || return 1
    installer_version_valid "${lines[1]}" && installer_version_valid "${lines[2]}" || return 1
    installer_path_valid "${lines[3]}" || return 1
    [[ "${lines[4]}" == '-' ]] || installer_path_valid "${lines[4]}" || return 1
    global_name=AGENTS.md
    [[ "${lines[0]}" != claude ]] || global_name=CLAUDE.md
    for ((index=5; index<count; index++)); do
        [[ "${lines[index]}" =~ $re_entry ]] || return 1
        hash="${BASH_REMATCH[1]}"; relative="${BASH_REMATCH[2]}"
        installer_relative_valid "$relative" || return 1
        [[ "$seen" != *"|$relative|"* ]] || return 1
        seen="$seen$relative|"
        case "$relative" in
            skills/work/*|skills/tooling/*|skills/shared/*|bin/*|host-skills/work/SKILL.md|host-skills/tooling/SKILL.md) ;;
            "global/$global_name") global_count=$((global_count + 1)) ;;
            *) return 1 ;;
        esac
        entries+=("$hash  $relative")
    done
    if [[ "${lines[4]}" != '-' ]]; then [[ "$global_count" == 1 ]] || return 1
    else [[ "$global_count" == 0 ]] || return 1; fi
    [[ "$seen" == *"|host-skills/work/SKILL.md|"* && "$seen" == *"|host-skills/tooling/SKILL.md|"* ]] || return 1
    installer_state_host="${lines[0]}"
    installer_state_version="${lines[1]}"
    installer_state_rust_version="${lines[2]}"
    installer_state_install_dir="${lines[3]}"
    if [[ "${lines[4]}" != '-' ]]; then installer_state_global_dir="${lines[4]}"; fi
    installer_state_files=("${entries[@]}")
}

# installer_state_write FILE HOST VERSION RUST_VERSION INSTALL_DIR GLOBAL_DIR_OR_EMPTY ENTRY...
installer_state_write() {
    local file="$1" host="$2" version="$3" rust="$4" install="$5" global="$6" entry sorted index=0 total
    shift 6
    total=$#
    (( total > 0 )) || return 1
    sorted="$(printf '%s\n' "$@" | LC_ALL=C sort -k2,2)" || return 1
    {
        printf '{\n  "host": "%s",\n  "version": "%s",\n  "rust_workspace_version": "%s",\n' "$host" "$version" "$rust"
        printf '  "install_dir": "%s",\n' "$install"
        if [[ -n "$global" ]]; then printf '  "global_instructions_dir": "%s",\n' "$global"
        else printf '  "global_instructions_dir": null,\n'; fi
        printf '  "status": "installed",\n  "files": [\n'
        while IFS= read -r entry; do
            index=$((index + 1))
            printf '    {\n      "path": "%s",\n      "sha256": "%s"\n    }' "${entry#*  }" "${entry%%  *}"
            if (( index < total )); then printf ',\n'; else printf '\n'; fi
        done <<< "$sorted"
        printf '  ]\n}\n'
    } > "$file"
}

# Exact SHA-256 manifest of managed Skills and CLI files; never scan the Host Skills parent.
installer_actual_manifest() {
    local install="$1" staged="${2:-0}" name dir file relative source
    if [[ "$staged" == 1 ]]; then source="$install"; else
        source="$install/skills"
        installer_directory_path_safe "$source" || return 1
        [[ -d "$source" && -z "$(find "$source" -mindepth 1 -maxdepth 1 ! -name work ! -name tooling ! -name shared -print -quit)" ]] || return 1
    fi
    for file in "$source/work/instructions/instructions.md" "$source/tooling/instructions/instructions.md" "$source/shared/bootstrap.md" "$install/bin/work" "$install/bin/skill"; do
        [[ -f "$file" && ! -L "$file" ]] || return 1
    done
    for name in work tooling shared; do
        if [[ "$staged" == 1 ]]; then dir="$install/$name"; else dir="$install/skills/$name"; fi
        [[ -e "$dir" || -L "$dir" ]] || continue
        [[ -d "$dir" && ! -L "$dir" ]] || return 1
        while IFS= read -r -d '' file; do
            [[ -f "$file" && ! -L "$file" ]] || return 1
            if [[ "$staged" == 1 ]]; then relative="skills/$name/${file#"$dir"/}"
            else relative="${file#"$install"/}"; fi
            installer_relative_valid "$relative" || return 1
            [[ "$relative" != skills/*/SKILL.md ]] || return 1
            local digest
            digest="$(installer_sha256 "$file")" || return 1
            printf '%s  %s\n' "$digest" "$relative"
        done < <(find "$dir" -type f -print0)
        [[ -z "$(find "$dir" ! -type f ! -type d -print -quit)" ]] || return 1
    done
    for name in work tooling; do
        if [[ "$staged" == 1 ]]; then file="$install/host-$name"
        else
            dir="$installer_host_skills_dir/$name"
            installer_directory_path_safe "$dir" && [[ -d "$dir" ]] || return 1
            file="$dir/SKILL.md"
            [[ -z "$(find "$dir" -mindepth 1 ! -path "$file" -print -quit)" ]] || return 1
        fi
        [[ -f "$file" && ! -L "$file" ]] || return 1
        local digest
        digest="$(installer_sha256 "$file")" || return 1
        printf '%s  host-skills/%s/SKILL.md\n' "$digest" "$name"
    done
    dir="$install/bin"
    if [[ -e "$dir" || -L "$dir" ]]; then
        [[ -d "$dir" && ! -L "$dir" ]] || return 1
        while IFS= read -r -d '' file; do
            [[ -f "$file" && ! -L "$file" ]] || return 1
            relative="${file#"$install"/}"
            installer_relative_valid "$relative" || return 1
            local digest
            digest="$(installer_sha256 "$file")" || return 1
            printf '%s  %s\n' "$digest" "$relative"
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
