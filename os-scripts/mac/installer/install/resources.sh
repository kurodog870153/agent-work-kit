#!/usr/bin/env bash

installer_release_field() {
    local lines
    lines="$(sed -n "s/^  \"$2\": \"\([^\"]*\)\",\{0,1\}\$/\1/p" "$1")" || return 1
    [[ -n "$lines" && "$lines" != *$'\n'* ]] || return 1
    printf '%s\n' "$lines"
}

installer_binary_in_use() {
    local path="$1" pids lsof_bin
    [[ -f "$path" ]] || return 1
    # macOS ships lsof in /usr/sbin, which a minimal PATH may omit.
    lsof_bin="$(command -v lsof 2>/dev/null)" || lsof_bin=/usr/sbin/lsof
    [[ -x "$lsof_bin" ]] || { installer_fail 'lsof is required to check installed binaries'; return 0; }
    pids="$("$lsof_bin" -t -- "$path" 2>/dev/null)" || return 1
    [[ -n "$pids" ]]
}

# Render the existing entry template. Paths are JSON strings; macOS path validation
# excludes characters requiring JSON escaping.
installer_render_entry() {
    local name="$1" output="$2" line key value seen=' ' count=0
    while IFS= read -r line || [[ -n "$line" ]]; do
        case "$line" in
            '- Shared Bootstrap: {{SHARED_BOOTSTRAP}}') key=shared; value="$installer_skills_dir/shared/bootstrap.md" ;;
            '- Skill Root: {{SKILL_ROOT}}') key=root; value="$installer_skills_dir/$name/instructions/instructions.md" ;;
            '- Work CLI: {{WORK_CLI}}') [[ "$name" == work ]] || return 1; key=work; value="$installer_cli_dir/bin/work" ;;
            '- Skill CLI: {{SKILL_CLI}}') key=skill; value="$installer_cli_dir/bin/skill" ;;
            '- Skill CLI --install: {{SKILL_INSTALL}}') key=install; value="$installer_skills_dir" ;;
            '- Installation State: {{INSTALL_STATE}}') key=state; value="$installer_install_dir/install-state.json" ;;
            *'{{'*|*'}}'*) return 1 ;;
            *) printf '%s\n' "$line" || return 1; continue ;;
        esac
        [[ "$seen" != *" $key "* ]] && installer_path_valid "$value" || return 1
        seen+="$key "; count=$((count+1))
        printf '%s "%s"\n' "${line%% \{\{*}" "$value" || return 1
    done < "$installer_root/skills/$name/SKILL.md" > "$output"
    if [[ "$name" == work ]]; then [[ "$count" == 6 ]]; else [[ "$count" == 5 ]]; fi
}

# Copies the verified sources into the transaction. Shared is copied once and used by both Skills.
installer_stage_payload() {
    local name
    for name in work tooling shared; do
        cp -R -- "$installer_root/skills/$name" "$installer_tx/staged/$name" || return 1
    done
    for name in work tooling; do
        installer_render_entry "$name" "$installer_tx/staged/host-$name" || return 1
        rm -- "$installer_tx/staged/$name/SKILL.md" || return 1
    done
    mkdir -- "$installer_tx/staged/bin" || return 1
    cp -- "$installer_work_binary" "$installer_tx/staged/bin/work" || return 1
    cp -- "$installer_skill_binary" "$installer_tx/staged/bin/skill" || return 1
    chmod 755 "$installer_tx/staged/bin/work" "$installer_tx/staged/bin/skill" || return 1
    if [[ "$installer_global_action" == install || "$installer_global_action" == replace ]]; then
        cp -- "$installer_global_source" "$installer_tx/staged/global" || return 1
    fi
}

# Expected entries of the new installation: managed files plus the Global Instructions copy it owns.
installer_build_manifest() {
    local line manifest
    installer_new_files=()
    manifest="$(installer_actual_manifest "$installer_tx/staged" 1 | LC_ALL=C sort)" || return 1
    while IFS= read -r line; do installer_new_files+=("$line"); done <<< "$manifest"
    (( ${#installer_new_files[@]} > 0 )) || return 1
    if [[ -n "$installer_global_dir" ]]; then
        installer_new_files+=("$installer_global_source_sha  global/${installer_global_target##*/}")
    fi
}

# Checks the installed result: exact managed content, runnable binaries and the Global Instructions copy.
installer_verify_installation() {
    local name
    installer_manifest_matches "$installer_install_dir" "${installer_new_files[@]}" || return 1
    for name in work skill; do
        "$installer_cli_dir/bin/$name" --help >/dev/null 2>&1 || return 1
    done
    if [[ -n "$installer_global_dir" ]]; then
        [[ -f "$installer_global_target" && ! -L "$installer_global_target" &&
           "$(installer_sha256 "$installer_global_target")" == "$installer_global_source_sha" ]] || return 1
    fi
}
