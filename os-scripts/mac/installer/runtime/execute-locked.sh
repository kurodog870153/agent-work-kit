#!/usr/bin/env bash
set -uo pipefail
mode="$1"
root="$2"
shift 2
module_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
installer_dir="$(cd -- "$module_dir/.." && pwd -P)"
source_work="$root/skills/work"
source "$installer_dir/cli/parse-options.sh"
source "$installer_dir/target/preflight.sh"
source "$installer_dir/transaction/work.sh"
source "$installer_dir/runtime/migration-cleanup.sh"
source "$installer_dir/global-agents/policy.sh"
source "$installer_dir/global-agents/transaction.sh"
state_module="$installer_dir/state/state-model.js"
if [[ "$mode" == source ]]; then
    state_template="$root/os-scripts/common/installer/install-state.json"
else
    state_template="$root/installer/state/install-state.json"
fi

installer_fail() { printf 'Error: %s\n' "$1" >&2; return 1; }
installer_prompt() {
    local prompt="$1" default="$2" answer
    read -r -p "$prompt [$default] " answer || return 1
    printf '%s\n' "${answer:-$default}"
}
installer_absolute_path() {
    local input="$1" component result='' index
    local -a components stack=()
    [[ "$input" == /* ]] || input="$PWD/$input"
    IFS=/ read -r -a components <<< "$input"
    for component in "${components[@]}"; do
        case "$component" in
            ''|.) ;;
            ..) if (( ${#stack[@]} )); then index=$((${#stack[@]} - 1)); unset 'stack[index]'; fi ;;
            *) stack+=("$component") ;;
        esac
    done
    for component in "${stack[@]}"; do result+="/$component"; done
    printf '%s\n' "${result:-/}"
}
installer_directory_path_safe() {
    local probe="$1"
    while [[ "$probe" != / ]]; do
        [[ ! -L "$probe" && ( ! -e "$probe" || -d "$probe" ) ]] || return 1
        probe="${probe%/*}"
        [[ -n "$probe" ]] || probe=/
    done
}
installer_version_not_older() {
    local new_major new_minor new_patch old_major old_minor old_patch
    IFS=. read -r new_major new_minor new_patch <<< "$1"
    IFS=. read -r old_major old_minor old_patch <<< "$2"
    (( 10#$new_major > 10#$old_major ||
       (10#$new_major == 10#$old_major && 10#$new_minor > 10#$old_minor) ||
       (10#$new_major == 10#$old_major && 10#$new_minor == 10#$old_minor && 10#$new_patch >= 10#$old_patch) ))
}
installer_binary_in_use() {
    local path="$1" pids
    [[ -f "$path" ]] || return 1
    [[ -x /usr/sbin/lsof ]] || { installer_fail 'lsof is required to check the installed Work binary'; return 0; }
    pids="$(/usr/sbin/lsof -t -- "$path" 2>/dev/null)" || return 1
    [[ -n "$pids" ]]
}
installer_prepare_payload() {
    local prepared="$1" relative id hierarchy mode_item
    for relative in SKILL.md agents/openai.yaml references/instruction-loading.md; do
        mkdir -p -- "$prepared/$(dirname -- "$relative")" || return 1
        cp -- "$source_work/$relative" "$prepared/$relative" || return 1
    done
    for relative in references/instruction-loading references/workflows references/subagents; do
        mkdir -p -- "$prepared/$relative" || return 1
        cp -R -- "$source_work/$relative/." "$prepared/$relative/" || return 1
    done
    local -a chosen
    IFS=, read -r -a chosen <<< "$skills_csv"
    for id in "${chosen[@]}"; do
        hierarchy="$(installer_cli_skill_path "$id")" || return 1
        for mode_item in task execute; do
            relative="references/instructions/$mode_item/$hierarchy"
            mkdir -p -- "$prepared/$relative" || return 1
            cp -R -- "$source_work/$relative/." "$prepared/$relative/" || return 1
        done
    done
    mkdir -p -- "$prepared/scripts" || return 1
    cp -- "$binary" "$prepared/scripts/work" || return 1
    chmod 755 "$prepared/scripts/work" || return 1
    printf '%s\n' "$state_json" > "$prepared/install-state.json" || return 1
}
installer_validate_payload() {
    local path="$1" validation
    [[ -x "$path/scripts/work" ]] || return 1
    "$path/scripts/work" --help >/dev/null 2>&1 || return 1
    validation="$(osascript -l JavaScript "$state_module" validate \
        "$(cat -- "$path/install-state.json")" "$target")" || return 1
    [[ "$validation" == *'"valid":true'* ]]
}
installer_created_work_parents=()
installer_create_work_parents() {
    local probe="$1" index
    local -a missing=()
    while [[ ! -d "$probe" ]]; do
        [[ ! -e "$probe" && ! -L "$probe" ]] || return 1
        missing+=("$probe")
        [[ "$probe" != / ]] || return 1
        probe="${probe%/*}"
        [[ -n "$probe" ]] || probe=/
    done
    for ((index=${#missing[@]}-1; index>=0; index--)); do
        mkdir -- "${missing[index]}" || return 1
        installer_created_work_parents+=("${missing[index]}")
    done
}
installer_remove_empty_work_parents() {
    local index
    for ((index=${#installer_created_work_parents[@]}-1; index>=0; index--)); do
        rmdir -- "${installer_created_work_parents[index]}" 2>/dev/null || break
    done
}

installer_cli_parse "$@" || exit 2
if [[ "$installer_cli_action" == help ]]; then installer_cli_usage; exit 0; fi
if [[ "$installer_cli_action" == list-skills ]]; then installer_cli_list_skills; exit 0; fi
installer_log() { [[ "$installer_cli_verbose" == 1 ]] && printf 'Installer: %s\n' "$1" || true; }
installer_log 'options parsed'
default_target="$(installer_absolute_path "$HOME/.agents/skills/work")"
target="${installer_cli_work_install_dir:-$default_target}"
if [[ -z "$installer_cli_work_install_dir" && "$installer_cli_non_interactive" != 1 ]]; then
    choice="$(installer_prompt 'Work directory: 1) default, 2) custom' 1)" || exit 1
    case "$choice" in
        1) target="$default_target" ;;
        2) target="$(installer_prompt 'Custom Work directory' '')" || exit 1
           [[ -n "$target" ]] || { installer_fail 'Work directory cannot be empty'; exit 1; } ;;
        *) installer_fail 'invalid Work directory choice'; exit 1 ;;
    esac
fi
target="$(installer_absolute_path "$target")"
installer_log 'Work target selected'
[[ "${target##*/}" == work ]] || { installer_fail '--work-install-dir must name a work directory'; exit 1; }
if [[ "$installer_cli_dry_run" != 1 && -d "${target%/*}" ]]; then
    installer_migration_retry_cleanup "$target" "$source_work"
fi
if [[ -d "${target%/*}" ]]; then
    if [[ "$installer_cli_dry_run" == 1 ]]; then
        if installer_work_transaction_pending "$target" || installer_global_transaction_pending "$target"; then
            installer_fail 'dry run cannot resolve a pending installation transaction'; exit 1
        fi
    else
        installer_global_recover_interrupted "$target" "$source_work" || {
            installer_fail 'Global AGENTS recovery requires manual review'; exit 1;
        }
        installer_recover_interrupted_work "$target" "$source_work" || exit 1
    fi
fi
if [[ "$mode" == source ]]; then
    version="$(cat -- "$root/VERSION")" || exit 1
else
    version="$(osascript -l JavaScript "$state_module" release-version "$(cat -- "$root/release-info.json")")" || exit 1
fi
installer_log "candidate version $version"
raw_state=''
if [[ -f "$target/install-state.json" && ! -L "$target/install-state.json" ]]; then
    raw_state="$(cat -- "$target/install-state.json")" || exit 1
fi
skills_override="${installer_cli_skills:-}"
global_override=''
if [[ -n "$installer_cli_global_agents" ]]; then
    if [[ "$installer_cli_global_agents" == 1 ]]; then global_override=true; else global_override=false; fi
fi
settings="$(osascript -l JavaScript "$state_module" settings "$raw_state" "$target" "$skills_override" "$global_override")" || exit 1
IFS=$'\t' read -r state_valid skills_csv global_enabled prior_version saved_global_dir <<< "$settings"
installer_log "installed version ${prior_version:-none}; selected Skills $skills_csv"
target_state_valid="$state_valid"
settings_raw="$raw_state"
settings_dir="$target"
migration=''
if [[ "$state_valid" != 1 && "$target" != "$default_target" &&
      -f "$default_target/install-state.json" && ! -L "$default_target/install-state.json" ]]; then
    old_raw="$(cat -- "$default_target/install-state.json")" || exit 1
    old_validation="$(osascript -l JavaScript "$state_module" validate "$old_raw" "$default_target")" || exit 1
    if [[ "$old_validation" == *'"valid":true'* ]]; then
        installer_inspect_target "$default_target" "$source_work" 1
        if [[ "$installer_target_kind" == managed ]]; then
            settings="$(osascript -l JavaScript "$state_module" settings "$old_raw" "$default_target" "$skills_override" "$global_override")" || exit 1
            IFS=$'\t' read -r old_state_valid skills_csv global_enabled prior_version saved_global_dir <<< "$settings"
            migration="$default_target"
            settings_raw="$old_raw"
            settings_dir="$default_target"
        fi
    fi
fi
if [[ "$installer_cli_non_interactive" != 1 ]]; then
    if [[ -z "$installer_cli_skills" ]]; then
        selection="$(installer_prompt 'Skills (general, all, or comma-separated IDs)' "$skills_csv")" || exit 1
        installer_cli_validate_skills "$selection" || { installer_fail 'invalid Skill selection'; exit 1; }
        settings="$(osascript -l JavaScript "$state_module" settings "$settings_raw" "$settings_dir" "$selection" "$global_override")" || exit 1
        IFS=$'\t' read -r state_valid skills_csv global_enabled prior_version saved_global_dir <<< "$settings"
    fi
    if [[ -z "$installer_cli_global_agents" ]]; then
        default_choice=n
        [[ "$global_enabled" == 1 ]] && default_choice=y
        choice="$(installer_prompt 'Install Global AGENTS.md? (y/n)' "$default_choice")" || exit 1
        case "$choice" in y) global_enabled=1 ;; n) global_enabled=0 ;; *) installer_fail 'invalid Global AGENTS choice'; exit 1 ;; esac
    fi
fi
global_dir="$HOME/.codex"
if [[ "$global_enabled" == 1 && "$saved_global_dir" != - ]]; then
    global_dir="$saved_global_dir"
fi
if [[ -n "$installer_cli_global_agents_install_dir" ]]; then
    global_dir="$installer_cli_global_agents_install_dir"
fi
if [[ "$global_enabled" == 1 && -z "$installer_cli_global_agents_install_dir" &&
      "$installer_cli_non_interactive" != 1 ]]; then
    choice="$(installer_prompt 'Global AGENTS directory: 1) current/default, 2) custom' 1)" || exit 1
    case "$choice" in
        1) ;;
        2) global_dir="$(installer_prompt 'Custom Global AGENTS directory' '')" || exit 1
           [[ -n "$global_dir" ]] || { installer_fail 'Global AGENTS directory cannot be empty'; exit 1; } ;;
        *) installer_fail 'invalid Global AGENTS directory choice'; exit 1 ;;
    esac
fi
if [[ "$global_enabled" == 1 ]]; then
    global_dir="$(installer_absolute_path "$global_dir")"
    installer_directory_path_safe "$global_dir" || { installer_fail 'unsafe Global AGENTS directory'; exit 1; }
    installer_log 'Global AGENTS directory checked'
fi
installer_inspect_target "$target" "$source_work" "$target_state_valid"
confirmed=0
if [[ "$installer_target_kind" == first-update && "$installer_cli_non_interactive" != 1 ]]; then
    printf 'Existing Work without valid install-state.json: %s\n' "$target"
    printf 'Required structure and managed content passed safety inspection.\n'
    choice="$(installer_prompt 'Type YES to take over this Work directory' no)" || exit 1
    [[ "$choice" == YES ]] && confirmed=1
fi
installer_target_allows_install "$installer_cli_non_interactive" "$confirmed" || {
    installer_fail "Work target cannot be installed: $installer_target_reason"
    if [[ "$installer_target_kind" == unsafe ]]; then
        printf 'Keep the existing files. Inspect and organize this directory yourself, or choose another --work-install-dir.\n' >&2
    fi
    exit 1
}
if [[ "$prior_version" != - ]] && ! installer_version_not_older "$version" "$prior_version"; then
    installer_fail "downgrade from $prior_version to $version is not allowed"; exit 1
fi
installer_log 'version comparison passed'
while installer_binary_in_use "$target/scripts/work"; do
    if [[ "$installer_cli_non_interactive" == 1 ]]; then
        installer_fail 'installed Work binary is in use; close that Work process and retry'; exit 1
    fi
    choice="$(installer_prompt 'Installed Work is in use. Close it, then choose r) recheck or c) cancel' c)" || exit 1
    case "$choice" in
        r) ;;
        c) installer_fail 'installation cancelled'; exit 1 ;;
        *) installer_fail 'invalid in-use Work choice'; exit 1 ;;
    esac
done
global_source="$root/AGENTS.md"
global_target="$global_dir/AGENTS.md"
installer_global_agents_plan "$global_source" "$global_target" "$global_enabled" "$installer_cli_non_interactive" 0 || {
    installer_fail 'Global AGENTS target conflicts or is unsafe'; exit 1;
}
if [[ "$installer_global_agents_action" == confirm ]]; then
    printf 'Global AGENTS.md differs (current SHA-256 %s, new %s):\n%s\n' \
        "$installer_global_agents_target_sha256" "$installer_global_agents_source_sha256" "$installer_global_agents_diff"
    choice="$(installer_prompt 'Replace Global AGENTS.md? (y/n)' n)" || exit 1
    if [[ "$choice" == y ]]; then installer_global_agents_action=replace
    else installer_global_agents_action=skip; fi
fi
printf 'Work target: %s\nVersion: %s; Skills: %s; Global AGENTS: %s; Action: %s\n' \
    "$target" "$version" "$skills_csv" "$global_enabled" "$installer_target_kind"
if [[ "$installer_cli_dry_run" == 1 ]]; then printf 'Dry run: no installation target changed.\n'; exit 0; fi
if [[ "$installer_cli_non_interactive" != 1 ]]; then
    choice="$(installer_prompt 'Proceed with installation? (y/n)' n)" || exit 1
    [[ "$choice" == y ]] || { installer_fail 'installation cancelled'; exit 1; }
fi
if [[ "$mode" == source ]]; then
    (cd -- "$root/rust" && cargo build --release --locked -p work-cli) || exit 1
    binary="$root/rust/target/release/work"
else
    binary="$root/bin/work"
fi
[[ -x "$binary" ]] && "$binary" --help >/dev/null 2>&1 || { installer_fail 'Work binary smoke test failed'; exit 1; }
installer_log 'candidate Work binary smoke test passed'
template_json="$(cat -- "$state_template")" || exit 1
state_json="$(osascript -l JavaScript "$state_module" create "$template_json" "$version" "$skills_csv" \
    "$(if [[ "$global_enabled" == 1 ]]; then printf true; else printf false; fi)" "$target" "$global_dir")" || exit 1
parent="${target%/*}"
installer_create_work_parents "$parent" || { installer_remove_empty_work_parents; exit 1; }
if [[ -n "$migration" ]]; then
    installer_migration_write_record "$target" "$migration" "$source_work" "$state_json" || {
        installer_fail 'cannot record previous Work for safe migration cleanup'; exit 1;
    }
fi
global_parent="${global_target%/*}"
created_global_parent=0
if [[ "$installer_global_agents_action" == install && ! -d "$global_parent" ]]; then
    mkdir -p -- "$global_parent" || { installer_remove_empty_work_parents; exit 1; }
    created_global_parent=1
fi
if ! installer_publish_combined_work "$target" "$source_work" "$target_state_valid" "$confirmed" \
    "$installer_cli_non_interactive" installer_prepare_payload installer_validate_payload \
    "$global_source" "$global_target" "$installer_global_agents_action" \
    "$installer_global_agents_source_sha256" "$installer_global_agents_target_sha256"; then
    if (( created_global_parent )) && [[ -z "$(find "$global_parent" -mindepth 1 -print -quit)" ]]; then
        rmdir -- "$global_parent" || true
    fi
    installer_remove_empty_work_parents
    exit 1
fi
installer_log 'Work transaction published and verified'
printf 'Work installed at %s\n' "$target"
if [[ -n "$migration" ]]; then installer_migration_retry_cleanup "$target" "$source_work"; fi
