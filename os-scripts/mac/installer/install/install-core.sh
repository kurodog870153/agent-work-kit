#!/usr/bin/env bash

# Runs the installer core under the machine-wide installation lock (one transaction at a time, any Host or target).
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
        lock_build="$(mktemp -d "${TMPDIR:-/tmp}/agent-work-kit-lock.XXXXXXXX")" || return 1
        helper="$lock_build/install-lock"
        if ! xcrun clang -std=c11 -Wall -Wextra -Werror -o "$helper" "$installer_dir/lock/native-lock.c"; then
            printf 'Error: could not build macOS installation lock.\n' >&2
            rm -rf -- "$lock_build"
            return 1
        fi
    else
        helper="$root/bin/work-install-lock"
        [[ -x "$helper" ]] || { printf 'Error: packaged macOS lock helper is missing.\n' >&2; return 1; }
    fi
    "$helper" /private/tmp/agent-work-kit-install.lock /bin/bash "$installer_dir/install/install-core.sh" "$mode" "$root" "$@"
    result=$?
    if [[ -n "$lock_build" && -d "$lock_build" ]]; then rm -rf -- "$lock_build"; fi
    return "$result"
}

if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then return 0; fi

set -uo pipefail
installer_mode="$1"
installer_root="$2"
shift 2
module_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
installer_dir="$(cd -- "$module_dir/.." && pwd -P)"
source "$installer_dir/cli/parse-options.sh"
source "$installer_dir/state/state.sh"
source "$installer_dir/target/preflight.sh"
source "$installer_dir/install/resources.sh"
source "$installer_dir/global-instructions/policy.sh"
source "$installer_dir/transaction/transaction.sh"

installer_prompt() {
    local prompt="$1" default="$2" answer
    read -r -p "$prompt [$default] " answer || return 1
    printf '%s\n' "${answer:-$default}"
}

installer_cli_parse "$@" || exit 2
if [[ "$installer_cli_action" == help ]]; then installer_cli_usage; exit 0; fi
installer_log() { [[ "$installer_cli_verbose" == 1 ]] && printf 'Installer: %s\n' "$1" || true; }
installer_created_dirs=()
installer_state_clear

# --- Host and release identity -------------------------------------------------------------------------------
selected_host="$installer_cli_host"
if [[ -z "$selected_host" ]]; then
    choice="$(installer_prompt 'Select Host: 1) Codex, 2) Claude Code' '')" || exit 1
    case "$choice" in
        1) selected_host=codex ;;
        2) selected_host=claude ;;
        *) installer_fail 'Host selection is required'; exit 1 ;;
    esac
fi
if [[ "$installer_mode" == package ]]; then
    info="$installer_root/release-info.json"
    package_host="$(installer_release_field "$info" host)" && package_target="$(installer_release_field "$info" target)" &&
        version="$(installer_release_field "$info" version)" && rust_version="$(installer_release_field "$info" rust_workspace_version)" ||
        { installer_fail 'release-info.json is missing or invalid'; exit 1; }
    [[ "$package_host" == "$selected_host" && "$package_target" == aarch64-apple-darwin ]] ||
        { installer_fail 'release package Host or platform does not match the selected installation'; exit 1; }
    installer_work_binary="$installer_root/bin/work"
    installer_skill_binary="$installer_root/bin/skill"
else
    version="$(cat -- "$installer_root/VERSION")" || exit 1
    version="${version%$'\n'}"
    rust_version="$(sed -n 's/^version = "\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)"$/\1/p' "$installer_root/rust/Cargo.toml")" || exit 1
    installer_work_binary="$installer_root/rust/target/release/work"
    installer_skill_binary="$installer_root/rust/target/release/skill"
fi
installer_version_valid "$version" && installer_version_valid "$rust_version" ||
    { installer_fail 'release or Rust workspace version is missing or invalid'; exit 1; }
installer_sources_safe "$installer_root" || exit 1
installer_global_source="$installer_root/instructions/global-instructions.md"

# --- Locations -----------------------------------------------------------------------------------------------
if [[ "$selected_host" == codex ]]; then
    default_skills="$HOME/.agents/skills"; default_global_dir="$HOME/.codex"; global_filename=AGENTS.md
else
    default_skills="$HOME/.claude/skills"; default_global_dir="$HOME/.claude"; global_filename=CLAUDE.md
fi
skills_input="${installer_cli_skills_dir:-$default_skills}"
if [[ -z "$installer_cli_skills_dir" && "$installer_cli_non_interactive" != 1 ]]; then
    choice="$(installer_prompt "Skills directory: 1) default ($default_skills), 2) custom" 1)" || exit 1
    case "$choice" in
        1) ;;
        2) skills_input="$(installer_prompt 'Custom Skills directory' '')" || exit 1
           [[ -n "$skills_input" ]] || { installer_fail 'Skills directory cannot be empty'; exit 1; } ;;
        *) installer_fail 'invalid Skills directory choice'; exit 1 ;;
    esac
fi
installer_skills_dir="$(installer_absolute_path "$skills_input")"
installer_path_valid "$installer_skills_dir" && installer_directory_path_safe "$installer_skills_dir" ||
    { installer_fail 'unsafe Skills directory'; exit 1; }
installer_tx="$installer_skills_dir/.agent-work-kit-txn"
installer_cli_dir="$(installer_absolute_path "$installer_skills_dir/../agent-work-kit")"
installer_path_valid "$installer_cli_dir" && installer_directory_path_safe "$installer_cli_dir" ||
    { installer_fail 'unsafe CLI directory'; exit 1; }
if installer_path_overlaps "$installer_skills_dir" "$installer_cli_dir"; then
    installer_fail 'the CLI directory must be separate from the Skills directory'; exit 1
fi
installer_log 'locations selected'

# --- Recovery and target inspection --------------------------------------------------------------------------
if [[ -e "$installer_tx" || -L "$installer_tx" ]]; then
    if [[ "$installer_cli_dry_run" == 1 ]]; then
        installer_fail 'dry run cannot resolve a pending installation transaction'; exit 1
    fi
    installer_tx_recover || exit 1
    installer_log 'interrupted installation recovered'
fi
installer_inspect_targets "$installer_skills_dir" "$installer_cli_dir"
if [[ "$installer_target_kind" == unsafe ]]; then
    installer_fail "targets cannot be installed: $installer_target_reason"
    printf 'Existing content is preserved and not taken over. Inspect it yourself or choose other directories.\n'
 >&2
    exit 1
fi
previous_global_dir=''
previous_global_sha=''
if [[ "$installer_target_kind" == managed ]]; then
    previous_global_dir="$installer_state_global_dir"
    for entry in "${installer_state_files[@]}"; do
        [[ "$entry" == *"  global/"* ]] && previous_global_sha="${entry%%  *}"
    done
fi

# --- Global Instructions -------------------------------------------------------------------------------------
global_enabled=1
[[ "$installer_target_kind" == new || -n "$previous_global_dir" ]] || global_enabled=0
if [[ -n "$installer_cli_global_instructions" ]]; then
    [[ "$installer_cli_global_instructions" == yes ]] && global_enabled=1 || global_enabled=0
elif [[ "$installer_cli_non_interactive" != 1 ]]; then
    default_choice=y; [[ "$global_enabled" == 1 ]] || default_choice=n
    choice="$(installer_prompt 'Install Global Instructions? (y/n)' "$default_choice")" || exit 1
    [[ "$choice" == y || "$choice" == n ]] || { installer_fail 'invalid Global Instructions choice'; exit 1; }
    [[ "$choice" == y ]] && global_enabled=1 || global_enabled=0
fi
installer_global_dir=''
installer_global_target=''
if [[ "$global_enabled" == 1 ]]; then
    global_input="${previous_global_dir:-$default_global_dir}"
    if [[ -n "$installer_cli_instructions_dir" ]]; then global_input="$installer_cli_instructions_dir"
    elif [[ "$installer_cli_non_interactive" != 1 ]]; then
        choice="$(installer_prompt "Global Instructions directory: 1) default ($global_input), 2) custom" 1)" || exit 1
        case "$choice" in
            1) ;;
            2) global_input="$(installer_prompt 'Custom Global Instructions directory' '')" || exit 1
               [[ -n "$global_input" ]] || { installer_fail 'Global Instructions directory cannot be empty'; exit 1; } ;;
            *) installer_fail 'invalid Global Instructions directory choice'; exit 1 ;;
        esac
    fi
    installer_global_dir="$(installer_absolute_path "$global_input")"
    installer_path_valid "$installer_global_dir" && installer_directory_path_safe "$installer_global_dir" ||
        { installer_fail 'unsafe Global Instructions directory'; exit 1; }
    installer_global_target="$installer_global_dir/$global_filename"
    if [[ "$installer_global_target" == "$installer_skills_dir"/* || "$installer_global_target" == "$installer_cli_dir"/* ]]; then
        installer_fail 'Global Instructions must be outside the Skills and CLI directories'; exit 1
    fi
    [[ "$installer_global_dir" == "$previous_global_dir" ]] || previous_global_sha=''
    installer_log 'Global Instructions directory checked'
fi
installer_global_plan "$installer_global_source" "$installer_global_target" "$global_enabled" "$installer_cli_non_interactive" "$previous_global_sha" ||
    { installer_fail 'Global Instructions target differs, is unsafe, or needs interactive confirmation (use --global-instructions no to skip it)'; exit 1; }
if [[ "$installer_global_action" == confirm ]]; then
    printf 'Global %s differs (current SHA-256 %s, new %s).\n' "$global_filename" "$installer_global_target_sha" "$installer_global_source_sha"
    choice="$(installer_prompt "Replace Global $global_filename? (y/n)" n)" || exit 1
    if [[ "$choice" == y ]]; then installer_global_action=replace
    else installer_global_action=skip; installer_global_dir=''; installer_global_target=''; fi
fi

# --- Binaries ------------------------------------------------------------------------------------------------
if [[ "$installer_cli_dry_run" != 1 ]]; then
    if [[ "$installer_mode" == source ]]; then
        (cd -- "$installer_root/rust" && cargo build --release --locked -p work-cli -p skill-cli) || { installer_fail 'Rust build failed'; exit 1; }
    fi
    for binary in "$installer_work_binary" "$installer_skill_binary"; do
        [[ -f "$binary" && ! -L "$binary" && -x "$binary" ]] && "$binary" --help >/dev/null 2>&1 ||
            { installer_fail "release binary is missing or failed its startup check: $binary"; exit 1; }
    done
    installer_log 'binary smoke tests passed'
fi
for binary in work skill; do
    while installer_binary_in_use "$installer_cli_dir/bin/$binary"; do
        [[ "$installer_cli_non_interactive" != 1 ]] || { installer_fail "installed $binary binary is in use; close it and retry"; exit 1; }
        choice="$(installer_prompt "Installed $binary is in use. Close it, then choose r) recheck or c) cancel" c)" || exit 1
        [[ "$choice" == r ]] || { installer_fail 'installation cancelled'; exit 1; }
    done
done

# --- Plan and confirmation -----------------------------------------------------------------------------------
printf 'Host: %s; version: %s; Rust workspace: %s; action: %s\n' "$selected_host" "$version" "$rust_version" "$installer_target_kind"
printf 'Skills: %s/{work,tooling,shared}\nCLI: %s/bin/{work,skill}\n' "$installer_skills_dir" "$installer_cli_dir"
printf 'Global Instructions: %s (%s)\n' "${installer_global_target:-disabled}" "$installer_global_action"
if [[ "$installer_cli_dry_run" == 1 ]]; then printf 'Dry run: no installation target changed.\n'; exit 0; fi
if [[ "$installer_cli_non_interactive" != 1 ]]; then
    choice="$(installer_prompt 'Proceed with installation? (y/n)' n)" || exit 1
    [[ "$choice" == y ]] || { installer_fail 'installation cancelled'; exit 1; }
fi

# --- Transaction: Prepare, Apply, Verify, Commit -------------------------------------------------------------
installer_transaction_failed() {
    installer_fail "$1"
    if installer_tx_rollback; then printf 'The previous installation state was restored.\n' >&2
    else printf 'Recovery is incomplete; the next installer run retries it.\n' >&2; fi
    exit 1
}
installer_create_directories "$installer_skills_dir" || { installer_remove_created_directories; installer_fail 'could not create the Skills directory'; exit 1; }
if ! installer_tx_begin; then installer_remove_created_directories; installer_fail 'could not start the installation transaction'; exit 1; fi
installer_create_directories "$installer_cli_dir" && { [[ -z "$installer_global_dir" ]] || installer_create_directories "$installer_global_dir"; } &&
    installer_tx_note_created "${installer_created_dirs[@]+"${installer_created_dirs[@]}"}" ||
    installer_transaction_failed 'could not create installation directories'
installer_inject prepare || installer_transaction_failed 'prepare failed'
installer_stage_payload && installer_build_manifest || installer_transaction_failed 'could not prepare installation resources'
installer_state_write "$installer_tx/staged/state" "$selected_host" "$version" "$rust_version" "$installer_skills_dir" "$installer_cli_dir" \
    "$installer_global_dir" "${installer_new_files[@]}" || installer_transaction_failed 'could not prepare install state'
installer_state_read "$installer_tx/staged/state" || installer_transaction_failed 'prepared install state is invalid'
plan=(work tooling shared bin)
[[ "$installer_global_action" != install && "$installer_global_action" != replace ]] || plan+=(global)
plan+=(state)
installer_tx_write_plan "${plan[@]}" || installer_transaction_failed 'could not record the installation plan'
installer_log 'prepared'
for name in "${plan[@]}"; do
    [[ "$name" != state ]] || continue
    installer_tx_apply_component "$name" || installer_transaction_failed "could not apply $name"
done
installer_inject verify || installer_transaction_failed 'verification failed'
installer_verify_installation || installer_transaction_failed 'installed result failed verification'
installer_log 'applied and verified'
installer_inject commit || installer_transaction_failed 'commit failed'
: > "$installer_tx/committed" || installer_transaction_failed 'could not commit the installation'
installer_inject after-commit || exit 1
installer_tx_finish || { installer_fail 'installation is committed; rerun the installer to finish cleanup'; exit 1; }
printf 'Installed Work, Tooling, Shared and the Work/Skill CLI for %s.\n' "$selected_host"
