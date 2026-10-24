#!/usr/bin/env bash

# Source this module after setting source_work. Parsing never reads stdin or writes files.
installer_cli_usage() {
    cat <<'USAGE'
Usage: install-work-from-{source,package}.command [options]
  --skills <general|all|id[,id...]>
  --global-agents | --no-global-agents
  --work-install-dir <work-directory>
  --global-agents-install-dir <directory>
  --non-interactive
  --dry-run
  --verbose
  --list-skills
  --help, -h
USAGE
}

installer_cli_error() {
    printf 'Error: %s. Use --help for usage.\n' "$1" >&2
    return 2
}

installer_cli_catalog_path() {
    local cli_dir="${BASH_SOURCE[0]%/*}"
    if [[ -f "$cli_dir/options.tsv" ]]; then
        printf '%s\n' "$cli_dir/options.tsv"
    else
        printf '%s\n' "$cli_dir/../../../common/installer/options.tsv"
    fi
}

installer_cli_option_exists() {
    local kind name path
    while IFS=$'\t' read -r kind name path; do
        [[ "$kind" == option && "$name" == "$1" ]] && return 0
    done < "$(installer_cli_catalog_path)"
    return 1
}

installer_cli_skill_path() {
    local kind name path
    while IFS=$'\t' read -r kind name path; do
        if [[ "$kind" == skill && "$name" == "$1" ]]; then
            printf '%s' "$path"
            return 0
        fi
    done < "$(installer_cli_catalog_path)"
    return 1
}

installer_cli_skill_exists() {
    local path
    path="$(installer_cli_skill_path "$1")" || return 1
    [[ -f "$source_work/references/instructions/task/$path/instructions.md" &&
       -f "$source_work/references/instructions/execute/$path/instructions.md" ]]
}

installer_cli_list_skills() {
    local kind name path
    while IFS=$'\t' read -r kind name path; do
        [[ "$kind" == skill ]] || continue
        installer_cli_skill_exists "$name" && printf '%s\n' "$name"
    done < "$(installer_cli_catalog_path)"
}

installer_cli_validate_skills() {
    local value="$1" id
    local -a ids
    [[ -n "$value" && "$value" != ,* && "$value" != *, && "$value" != *,,* ]] || return 1
    [[ "$value" == general || "$value" == all ]] && return 0
    IFS=, read -r -a ids <<< "$value"
    local seen=,
    for id in "${ids[@]}"; do
        installer_cli_skill_exists "$id" || return 1
        [[ "$seen" != *",$id,"* ]] || return 1
        seen+="$id,"
    done
}

installer_cli_parse() {
    installer_cli_action=install
    installer_cli_skills=''
    installer_cli_global_agents=''
    installer_cli_work_install_dir=''
    installer_cli_global_agents_install_dir=''
    installer_cli_non_interactive=0
    installer_cli_dry_run=0
    installer_cli_verbose=0
    local seen=, flag value
    while (( $# )); do
        flag="$1"
        shift
        installer_cli_option_exists "$flag" || { installer_cli_error "unknown option $flag"; return 2; }
        [[ "$flag" != -h ]] || flag=--help
        case "$flag" in
            --help|--list-skills|--non-interactive|--dry-run|--verbose|--global-agents|--no-global-agents)
                [[ "$seen" != *",$flag,"* ]] || { installer_cli_error "duplicate option $flag"; return 2; }
                seen+="$flag,"
                case "$flag" in
                    --help) installer_cli_action=help ;;
                    --list-skills) installer_cli_action=list-skills ;;
                    --non-interactive) installer_cli_non_interactive=1 ;;
                    --dry-run) installer_cli_dry_run=1 ;;
                    --verbose) installer_cli_verbose=1 ;;
                    --global-agents) installer_cli_global_agents=1 ;;
                    --no-global-agents) installer_cli_global_agents=0 ;;
                esac
                ;;
            --skills|--work-install-dir|--global-agents-install-dir)
                [[ "$seen" != *",$flag,"* ]] || { installer_cli_error "duplicate option $flag"; return 2; }
                seen+="$flag,"
                (( $# )) || { installer_cli_error "missing value for $flag"; return 2; }
                value="$1"
                shift
                [[ -n "$value" && "$value" != --* ]] || { installer_cli_error "missing value for $flag"; return 2; }
                if [[ "$flag" == --skills ]]; then
                    installer_cli_validate_skills "$value" || { installer_cli_error "invalid Skill selection $value"; return 2; }
                    installer_cli_skills="$value"
                elif [[ "$flag" == --work-install-dir ]]; then
                    installer_cli_work_install_dir="$value"
                else
                    installer_cli_global_agents_install_dir="$value"
                fi
                ;;
            *) installer_cli_error "unknown option $flag"; return 2 ;;
        esac
    done
    [[ "$seen" != *',--global-agents,'* || "$seen" != *',--no-global-agents,'* ]] || { installer_cli_error 'conflicting Global AGENTS options'; return 2; }
    [[ "$seen" != *',--help,'* || "$seen" != *',--list-skills,'* ]] || { installer_cli_error 'conflicting actions'; return 2; }
    [[ "$installer_cli_action" == install || "$seen" != *',--dry-run,'* ]] || { installer_cli_error 'conflicting actions'; return 2; }
    return 0
}
