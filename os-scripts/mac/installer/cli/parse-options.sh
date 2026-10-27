#!/usr/bin/env bash

# Parsing never reads stdin or writes files.
installer_cli_usage() {
    cat <<'USAGE'
Usage: install-work-from-{source,package}.command [options]
  --global-agent | --no-global-agent
  --skills-install-dir <parent-directory>
  --global-agent-install-dir <directory>
  --non-interactive
  --dry-run
  --verbose
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

installer_cli_parse() {
    installer_cli_action=install
    installer_cli_global_agents=''
    installer_cli_skills_install_dir=''
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
            --help|--non-interactive|--dry-run|--verbose|--global-agent|--no-global-agent)
                [[ "$seen" != *",$flag,"* ]] || { installer_cli_error "duplicate option $flag"; return 2; }
                seen+="$flag,"
                case "$flag" in
                    --help) installer_cli_action=help ;;
                    --non-interactive) installer_cli_non_interactive=1 ;;
                    --dry-run) installer_cli_dry_run=1 ;;
                    --verbose) installer_cli_verbose=1 ;;
                    --global-agent) installer_cli_global_agents=1 ;;
                    --no-global-agent) installer_cli_global_agents=0 ;;
                esac
                ;;
            --skills-install-dir|--global-agent-install-dir)
                [[ "$seen" != *",$flag,"* ]] || { installer_cli_error "duplicate option $flag"; return 2; }
                seen+="$flag,"
                (( $# )) || { installer_cli_error "missing value for $flag"; return 2; }
                value="$1"
                shift
                [[ -n "$value" && "$value" != --* ]] || { installer_cli_error "missing value for $flag"; return 2; }
                if [[ "$flag" == --skills-install-dir ]]; then
                    installer_cli_skills_install_dir="$value"
                else
                    installer_cli_global_agents_install_dir="$value"
                fi
                ;;
            *) installer_cli_error "unknown option $flag"; return 2 ;;
        esac
    done
    [[ "$seen" != *',--global-agent,'* || "$seen" != *',--no-global-agent,'* ]] || { installer_cli_error 'conflicting Global AGENTS options'; return 2; }
    [[ "$installer_cli_action" == install || "$seen" != *',--dry-run,'* ]] || { installer_cli_error 'conflicting actions'; return 2; }
    return 0
}
