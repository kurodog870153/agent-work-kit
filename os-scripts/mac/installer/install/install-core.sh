#!/usr/bin/env bash

installer_run() {
    local mode="$1" root="$2" helper lock_build='' result
    shift 2
    local module_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
    local installer_dir="$(cd -- "$module_dir/.." && pwd -P)"
    source "$installer_dir/cli/parse-options.sh"
    installer_cli_parse "$@" || return 2
    if [[ "$installer_cli_action" == help ]]; then installer_cli_usage; return 0; fi
    if [[ "$mode" == source ]]; then
        command -v xcrun >/dev/null 2>&1 || { printf 'Error: Xcode Command Line Tools are required.\n' >&2; return 1; }
        lock_build="$(mktemp -d "${TMPDIR:-/tmp}/work-lock.XXXXXXXX")" || return 1
        helper="$lock_build/work-install-lock"
        if ! xcrun clang -std=c11 -Wall -Wextra -Werror -o "$helper" "$installer_dir/lock/native-lock.c"; then
            printf 'Error: could not build macOS installation lock.\n' >&2
            return 1
        fi
    else
        helper="$root/bin/work-install-lock"
        [[ -x "$helper" ]] || { printf 'Error: packaged macOS lock helper is missing.\n' >&2; return 1; }
    fi
    "$helper" /private/tmp/codex-workflow-kit-work-install.lock /bin/bash \
        "$installer_dir/install/install-core.sh" "$mode" "$root" "$@"
    result=$?
    if [[ -n "$lock_build" && -d "$lock_build" ]]; then rm -rf -- "$lock_build"; fi
    return "$result"
}

if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then return 0; fi

set -uo pipefail
mode="$1"
root="$2"
shift 2
module_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
installer_dir="$(cd -- "$module_dir/.." && pwd -P)"
source_work="$root/skills/work"
source_tooling="$root/skills/tooling"
source "$installer_dir/cli/parse-options.sh"
source "$installer_dir/target/preflight.sh"
source "$installer_dir/transaction/work.sh"
source "$installer_dir/install/work-install.sh"
source "$installer_dir/install/tooling-install.sh"
source "$installer_dir/transaction/tooling.sh"
source "$installer_dir/install/migration-cleanup.sh"
source "$installer_dir/global-agents/policy.sh"
source "$installer_dir/global-agents/transaction.sh"
state_module="$installer_dir/state/state-core.js"
work_state_module="$installer_dir/state/work-state.js"
installer_tooling_state_module="$installer_dir/state/tooling-state.js"
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
installer_combined_action() {
    osascript -l JavaScript "$state_module" combined "$1" "$target" "$tooling_target" "$2"
}
installer_work_state_from_combined() {
    local raw="$1" work="$2" tooling="$3" version global_dir
    version="$(osascript -l JavaScript "$state_module" combined "$raw" "$work" "$tooling" work-version)" || return 1
    [[ "$version" != - ]] || { printf ''; return 0; }
    global_dir="$(osascript -l JavaScript "$state_module" combined "$raw" "$work" "$tooling" global-agent-dir)" || return 1
    osascript -l JavaScript "$work_state_module" from-combined "$version" "$work" "$global_dir"
}
installer_combined_write() {
    local status="$1" json temporary="$skills_parent/.install-state.json.tmp"
    [[ ! -e "$temporary" && ! -L "$temporary" ]] || { installer_fail "pending state write needs review: $temporary"; return 1; }
    json="$(osascript -l JavaScript "$state_module" combined-create "$version" "$work_version" \
        "$tooling_version" "$skills_parent" "${global_dir_for_state:--}" "$status")" || return 1
    printf '%s\n' "$json" > "$temporary" || return 1
    mv -f -- "$temporary" "$skills_parent/install-state.json"
}
installer_combined_remove_child_states() {
    local work_raw tooling_raw work_version_record tooling_version_record
    if [[ -e "$target/install-state.json" || -L "$target/install-state.json" ]]; then
        [[ -f "$target/install-state.json" && ! -L "$target/install-state.json" ]] || return 1
        work_raw="$(cat -- "$target/install-state.json")" || return 1
        work_version_record="$(osascript -l JavaScript "$work_state_module" settings "$work_raw" "$target" '' '')" || return 1
        [[ "$work_version_record" == *$'\t'"$work_version"$'\t'* ]] || return 1
        rm -- "$target/install-state.json" || return 1
    fi
    if [[ -e "$tooling_target/install-state.json" || -L "$tooling_target/install-state.json" ]]; then
        [[ -f "$tooling_target/install-state.json" && ! -L "$tooling_target/install-state.json" ]] || return 1
        tooling_raw="$(cat -- "$tooling_target/install-state.json")" || return 1
        tooling_version_record="$(osascript -l JavaScript "$installer_tooling_state_module" version "$tooling_raw" "$tooling_target")" || return 1
        [[ "$tooling_version_record" == "$tooling_version" ]] || return 1
        rm -- "$tooling_target/install-state.json" || return 1
    fi
}

installer_cli_parse "$@" || exit 2
if [[ "$installer_cli_action" == help ]]; then installer_cli_usage; exit 0; fi
installer_log() { [[ "$installer_cli_verbose" == 1 ]] && printf 'Installer: %s\n' "$1" || true; }
installer_log 'options parsed'
default_parent="$(installer_absolute_path "$HOME/.agents/skills")"
skills_parent="${installer_cli_skills_install_dir:-$default_parent}"
default_target="$default_parent/work"
target="$skills_parent/work"
interactive_global_choice=''
interactive_global_dir=''
if [[ "$installer_cli_non_interactive" != 1 ]]; then
    if [[ -z "$installer_cli_global_agents" ]]; then
        initial_state=''
        initial_parent="$(installer_absolute_path "$skills_parent")"
        if installer_directory_path_safe "$initial_parent" &&
           [[ -f "$initial_parent/install-state.json" && ! -L "$initial_parent/install-state.json" ]]; then
            initial_state="$(cat -- "$initial_parent/install-state.json")" || exit 1
        fi
        initial_work="$(installer_work_state_from_combined "$initial_state" "$initial_parent/work" "$initial_parent/tooling")" || exit 1
        initial_settings="$(osascript -l JavaScript "$work_state_module" settings "$initial_work" \
            "$initial_parent/work" '' '')" || exit 1
        IFS=$'\t' read -r initial_valid initial_skills initial_global initial_version initial_dir <<< "$initial_settings"
        default_choice=y
        [[ "$initial_global" == 1 ]] || default_choice=n
        interactive_global_choice="$(installer_prompt 'Install Global AGENTS.md? (y/n)' "$default_choice")" || exit 1
        [[ "$interactive_global_choice" == y || "$interactive_global_choice" == n ]] || {
            installer_fail 'invalid Global AGENTS choice'; exit 1;
        }
    fi
    if [[ "$installer_cli_global_agents" == 1 ||
          ( -z "$installer_cli_global_agents" && "$interactive_global_choice" == y ) ]] &&
          [[ -z "$installer_cli_global_agents_install_dir" ]]; then
        choice="$(installer_prompt "Global AGENTS directory: 1) default ($HOME/.codex), 2) custom" 1)" || exit 1
        case "$choice" in
            1) interactive_global_dir="$HOME/.codex" ;;
            2) interactive_global_dir="$(installer_prompt 'Custom Global AGENTS directory' '')" || exit 1
               [[ -n "$interactive_global_dir" ]] || { installer_fail 'Global AGENTS directory cannot be empty'; exit 1; } ;;
            *) installer_fail 'invalid Global AGENTS directory choice'; exit 1 ;;
        esac
    fi
fi
if [[ -z "$installer_cli_skills_install_dir" && "$installer_cli_non_interactive" != 1 ]]; then
    choice="$(installer_prompt "Skills directory: 1) default ($default_parent), 2) custom" 1)" || exit 1
    case "$choice" in
        1) skills_parent="$default_parent" ;;
        2) skills_parent="$(installer_prompt 'Custom Skills directory' '')" || exit 1
           [[ -n "$skills_parent" ]] || { installer_fail 'Skills directory cannot be empty'; exit 1; } ;;
        *) installer_fail 'invalid Skills directory choice'; exit 1 ;;
    esac
fi
skills_parent="$(installer_absolute_path "$skills_parent")"
target="$skills_parent/work"
tooling_target="$skills_parent/tooling"
installer_directory_path_safe "$skills_parent" || { installer_fail 'unsafe Skills directory'; exit 1; }
installer_log 'Work target selected'
installer_tooling_source_safe "$source_tooling" || { installer_fail 'Tooling source is missing or unsafe'; exit 1; }
combined_path="$skills_parent/install-state.json"
combined_temporary="$skills_parent/.install-state.json.tmp"
if [[ -e "$combined_temporary" || -L "$combined_temporary" ]]; then
    [[ -f "$combined_temporary" && ! -L "$combined_temporary" ]] || { installer_fail 'unsafe pending parent state'; exit 1; }
    temporary_raw="$(cat -- "$combined_temporary")" || exit 1
    [[ "$(installer_combined_action "$temporary_raw" status)" != - ]] || { installer_fail 'invalid pending parent state'; exit 1; }
    if [[ -e "$combined_path" || -L "$combined_path" ]]; then
        [[ -f "$combined_path" && ! -L "$combined_path" ]] || { installer_fail 'unsafe parent state'; exit 1; }
        existing_raw="$(cat -- "$combined_path")" || exit 1
        [[ "$(installer_combined_action "$existing_raw" status)" != - ]] || { installer_fail 'invalid parent state'; exit 1; }
    fi
    mv -f -- "$combined_temporary" "$combined_path" || exit 1
fi
combined_raw=''
if [[ -e "$combined_path" || -L "$combined_path" ]]; then
    [[ -f "$combined_path" && ! -L "$combined_path" ]] || { installer_fail 'unsafe parent state'; exit 1; }
    combined_raw="$(cat -- "$combined_path")" || exit 1
    combined_status="$(installer_combined_action "$combined_raw" status)" || exit 1
    [[ "$combined_status" != - ]] || { installer_fail 'invalid parent install-state.json'; exit 1; }
else
    combined_status=''
    for old_target in "$target" "$tooling_target"; do
        if [[ -d "$old_target" && -n "$(find "$old_target" -mindepth 1 -print -quit)" ]]; then
            installer_fail "existing Skill has no parent install-state.json; preserve and inspect $old_target"; exit 1
        fi
    done
fi
if [[ "$installer_cli_dry_run" != 1 && -d "${target%/*}" ]]; then
    installer_migration_retry_cleanup "$target" "$source_work"
fi
if [[ -d "${target%/*}" ]]; then
    if [[ "$installer_cli_dry_run" == 1 ]]; then
        if installer_work_transaction_pending "$target" || installer_global_transaction_pending "$target" ||
           installer_tooling_transaction_pending "$tooling_target"; then
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
    work_version="$(sed -n 's/^version = "\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)"$/\1/p' "$root/rust/Cargo.toml")" || exit 1
    [[ -n "$work_version" ]] || { installer_fail 'Rust workspace version is missing or invalid'; exit 1; }
    tooling_version="$work_version"
    binary="$root/rust/target/release/work"
    tooling_binary="$root/rust/target/release/tooling"
    if [[ "$installer_cli_dry_run" != 1 ]]; then
        (cd -- "$root/rust" && cargo build --release --locked -p work-cli) || exit 1
        (cd -- "$root/rust" && cargo build --release --locked -p tooling-cli) || exit 1
    fi
else
    release_info="$(cat -- "$root/release-info.json")" || exit 1
    version="$(osascript -l JavaScript "$state_module" release-version "$release_info")" || exit 1
    work_version="$(osascript -l JavaScript "$state_module" work-release-version "$release_info")" || exit 1
    tooling_version="$(osascript -l JavaScript "$state_module" tooling-release-version "$release_info")" || exit 1
    binary="$root/bin/work"
    tooling_binary="$root/bin/tooling"
fi
if [[ "$combined_status" == installing ]]; then
    [[ "$(installer_combined_action "$combined_raw" version)" == "$version" &&
       "$(installer_combined_action "$combined_raw" work-version)" == "$work_version" &&
       "$(installer_combined_action "$combined_raw" tooling-version)" == "$tooling_version" ]] || {
        installer_fail 'pending installation belongs to a different release'; exit 1;
    }
fi
if [[ "$installer_cli_dry_run" != 1 ]]; then
    [[ -x "$binary" ]] && "$binary" --help >/dev/null 2>&1 || { installer_fail 'Work binary smoke test failed'; exit 1; }
    [[ -x "$tooling_binary" ]] && "$tooling_binary" --help >/dev/null 2>&1 || { installer_fail 'Tooling binary smoke test failed'; exit 1; }
fi
if [[ "$installer_cli_dry_run" != 1 && -d "$skills_parent" ]]; then
    installer_tooling_recover "$tooling_target" "$source_tooling" "$tooling_version" "$tooling_binary" || exit 1
    if [[ "$combined_status" == installed ]] && ! installer_work_transaction_pending "$target" &&
       ! installer_tooling_transaction_pending "$tooling_target" &&
       { [[ -e "$target/install-state.json" || -L "$target/install-state.json" ]] ||
         [[ -e "$tooling_target/install-state.json" || -L "$tooling_target/install-state.json" ]]; }; then
        installer_fail 'existing child install-state.json is unsupported; preserve and inspect the installation'; exit 1
    fi
fi
installer_log "candidate version $version"
raw_state=''
if [[ -n "$combined_raw" ]]; then
    raw_state="$(installer_work_state_from_combined "$combined_raw" "$target" "$tooling_target")" || exit 1
fi
skills_override=all
global_override=''
if [[ -n "$installer_cli_global_agents" ]]; then
    if [[ "$installer_cli_global_agents" == 1 ]]; then global_override=true; else global_override=false; fi
elif [[ -n "$interactive_global_choice" ]]; then
    if [[ "$interactive_global_choice" == y ]]; then global_override=true; else global_override=false; fi
fi
settings="$(osascript -l JavaScript "$work_state_module" settings "$raw_state" "$target" "$skills_override" "$global_override")" || exit 1
IFS=$'\t' read -r state_valid skills_csv global_enabled prior_version saved_global_dir <<< "$settings"
installer_log "installed version ${prior_version:-none}; selected Skills $skills_csv"
target_state_valid="$state_valid"
settings_raw="$raw_state"
settings_dir="$target"
migration=''
global_dir="$HOME/.codex"
if [[ "$global_enabled" == 1 && "$saved_global_dir" != - ]]; then
    global_dir="$saved_global_dir"
fi
if [[ -n "$installer_cli_global_agents_install_dir" ]]; then
    global_dir="$installer_cli_global_agents_install_dir"
elif [[ -n "$interactive_global_dir" ]]; then
    global_dir="$interactive_global_dir"
fi
if [[ "$global_enabled" == 1 ]]; then
    global_dir="$(installer_absolute_path "$global_dir")"
    installer_directory_path_safe "$global_dir" || { installer_fail 'unsafe Global AGENTS directory'; exit 1; }
    installer_log 'Global AGENTS directory checked'
fi
installer_inspect_target "$target" "$source_work" "$target_state_valid"
installer_tooling_inspect "$tooling_target" "$source_tooling"
tooling_kind="$installer_tooling_kind"
tooling_prior_version="${installer_tooling_version:--}"
tooling_confirmed=0
if [[ "$tooling_kind" == first-update && "$installer_cli_non_interactive" != 1 ]]; then
    printf 'Existing Tooling without valid install-state.json: %s\n' "$tooling_target"
    choice="$(installer_prompt 'Type YES to take over this Tooling directory' no)" || exit 1
    [[ "$choice" == YES ]] && tooling_confirmed=1
fi
[[ "$tooling_kind" == new || "$tooling_kind" == managed ||
   "$tooling_kind" == first-update && "$installer_cli_non_interactive" != 1 && "$tooling_confirmed" == 1 ]] || {
    installer_fail "Tooling target cannot be installed: $installer_tooling_reason"; exit 1;
}
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
        printf 'Keep the existing files. Inspect and organize this directory yourself, or choose another --skills-install-dir.\n' >&2
    fi
    exit 1
}
if [[ "$combined_status" == installed ]] &&
   ! installer_version_not_older "$version" "$(installer_combined_action "$combined_raw" version)"; then
    installer_fail "release downgrade is not allowed"; exit 1
fi
if [[ "$prior_version" != - ]] && ! installer_version_not_older "$work_version" "$prior_version"; then
    installer_fail "downgrade from $prior_version to $work_version is not allowed"; exit 1
fi
if [[ "$tooling_prior_version" != - && -f "$tooling_target/scripts/tooling" ]] &&
   ! installer_version_not_older "$tooling_version" "$tooling_prior_version"; then
    installer_fail "Tooling downgrade from $tooling_prior_version to $tooling_version is not allowed"; exit 1
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
if installer_binary_in_use "$tooling_target/scripts/tooling"; then
    installer_fail 'installed Tooling binary is in use; close that Tooling process and retry'; exit 1
fi
global_source="$root/AGENTS.md"
global_target="$global_dir/AGENTS.md"
installer_global_agents_plan "$global_source" "$global_target" "$global_enabled" "$installer_cli_non_interactive" 0 || {
    installer_fail 'Global AGENTS target conflicts or is unsafe'; exit 1;
}
if [[ "$installer_global_agents_action" == confirm ]]; then
    printf 'Global AGENTS.md differs (current SHA-256 %s, new %s).\n' \
        "$installer_global_agents_target_sha256" "$installer_global_agents_source_sha256"
    choice="$(installer_prompt 'Replace Global AGENTS.md? (y/n)' n)" || exit 1
    if [[ "$choice" == y ]]; then installer_global_agents_action=replace
    else installer_global_agents_action=skip; fi
fi
printf 'Work target: %s\nTooling target: %s\nVersion: %s; Global AGENTS: %s; Work action: %s; Tooling action: %s\n' \
    "$target" "$tooling_target" "$version" "$global_enabled" "$installer_target_kind" "$tooling_kind"
if [[ "$installer_cli_dry_run" == 1 ]]; then printf 'Dry run: no installation target changed.\n'; exit 0; fi
if [[ "$installer_cli_non_interactive" != 1 ]]; then
    choice="$(installer_prompt 'Proceed with installation? (y/n)' n)" || exit 1
    [[ "$choice" == y ]] || { installer_fail 'installation cancelled'; exit 1; }
fi
installer_log 'candidate Work binary smoke test passed'
template_json="$(cat -- "$state_template")" || exit 1
state_json="$(osascript -l JavaScript "$work_state_module" create "$template_json" "$work_version" "$skills_csv" \
    "$(if [[ "$global_enabled" == 1 ]]; then printf true; else printf false; fi)" "$target" "$global_dir")" || exit 1
tooling_state_json="$(osascript -l JavaScript "$installer_tooling_state_module" create "$tooling_version" "$tooling_target")" || exit 1
parent="${target%/*}"
installer_create_work_parents "$parent" || { installer_remove_empty_work_parents; exit 1; }
global_dir_for_state='-'
[[ "$global_enabled" != 1 ]] || global_dir_for_state="$global_dir"
installer_combined_write installing || exit 1
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
if ! installer_tooling_publish "$tooling_target" "$source_tooling" "$tooling_version" "$tooling_state_json" \
    "$installer_cli_non_interactive" "$tooling_confirmed" "$tooling_binary"; then
    installer_fail "Tooling installation failed after Work was published; inspect $tooling_target and retry for recovery"
    exit 1
fi
printf 'Tooling installed at %s\n' "$tooling_target"
if installer_work_transaction_pending "$target" || installer_tooling_transaction_pending "$tooling_target"; then
    installer_fail 'installation transaction cleanup is pending; preserve its recovery data'; exit 1
fi
installer_combined_write installed || exit 1
installer_combined_remove_child_states || { installer_fail 'child state differs from parent'; exit 1; }
if [[ -n "$migration" ]]; then installer_migration_retry_cleanup "$target" "$source_work"; fi
