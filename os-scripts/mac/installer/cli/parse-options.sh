#!/usr/bin/env bash

# Parsing never reads stdin or writes files.
installer_cli_usage() {
    cat <<'USAGE'
Usage: install-agent-work-kit-from-{source,package}.command [options]
  --host codex|claude
  --skills-dir <parent-directory>
  --instructions-dir <directory>
  --global-instructions yes|no
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
    installer_cli_host=''
    installer_cli_global_instructions=''
    installer_cli_skills_dir=''
    installer_cli_instructions_dir=''
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
            --help|--non-interactive|--dry-run|--verbose)
                [[ "$seen" != *",$flag,"* ]] || { installer_cli_error "duplicate option $flag"; return 2; }
                seen+="$flag,"
                case "$flag" in
                    --help) installer_cli_action=help ;;
                    --non-interactive) installer_cli_non_interactive=1 ;;
                    --dry-run) installer_cli_dry_run=1 ;;
                    --verbose) installer_cli_verbose=1 ;;
                esac
                ;;
            --host|--skills-dir|--instructions-dir|--global-instructions)
                [[ "$seen" != *",$flag,"* ]] || { installer_cli_error "duplicate option $flag"; return 2; }
                seen+="$flag,"
                (( $# )) || { installer_cli_error "missing value for $flag"; return 2; }
                value="$1"
                shift
                [[ -n "$value" && "$value" != --* ]] || { installer_cli_error "missing value for $flag"; return 2; }
                case "$flag" in
                    --host)
                        [[ "$value" == codex || "$value" == claude ]] || { installer_cli_error 'invalid Host'; return 2; }
                        installer_cli_host="$value" ;;
                    --skills-dir) installer_cli_skills_dir="$value" ;;
                    --instructions-dir) installer_cli_instructions_dir="$value" ;;
                    --global-instructions)
                        [[ "$value" == yes || "$value" == no ]] || { installer_cli_error 'invalid Global Instructions choice'; return 2; }
                        installer_cli_global_instructions="$value" ;;
                esac
                ;;
            *) installer_cli_error "unknown option $flag"; return 2 ;;
        esac
    done
    [[ "$installer_cli_global_instructions" != no || -z "$installer_cli_instructions_dir" ]] || { installer_cli_error 'conflicting Global Instructions options'; return 2; }
    [[ "$installer_cli_action" != install || "$installer_cli_non_interactive" != 1 || -n "$installer_cli_host" ]] || { installer_cli_error 'missing --host'; return 2; }
    [[ "$installer_cli_action" == install || "$seen" != *',--dry-run,'* ]] || { installer_cli_error 'conflicting actions'; return 2; }
    return 0
}
