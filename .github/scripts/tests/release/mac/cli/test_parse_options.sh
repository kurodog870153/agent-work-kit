#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
source "$repo_root/os-scripts/mac/installer/cli/parse-options.sh"

fail() { printf 'FAIL: %s\n' "$1" >&2; exit 1; }
[[ "$(installer_cli_catalog_path)" == "$repo_root/os-scripts/mac/installer/cli/../../../common/installer/options.tsv" ]] || fail 'shared option catalog path'
[[ "$(awk -F '\t' '$1 == "skill" { print; exit }' "$(installer_cli_catalog_path)")" == '' ]] || fail 'option-only catalog'
expect_invalid() {
    local status
    if installer_cli_parse "$@" </dev/null 2>/dev/null; then
        fail "accepted invalid arguments: $*"
    else
        status=$?
    fi
    [[ "$status" == 2 ]] || fail "wrong error code $status: $*"
}

installer_cli_parse --help </dev/null
[[ "$installer_cli_action" == help ]] || fail 'help action'
installer_cli_parse -h </dev/null
[[ "$installer_cli_action" == help ]] || fail 'short help action'
installer_cli_parse --global-agent --skills-install-dir '目錄 有空白 !&' --global-agent-install-dir '全域 有空白 !&' --non-interactive --dry-run --verbose </dev/null
[[ "$installer_cli_skills_install_dir" == '目錄 有空白 !&' && "$installer_cli_global_agents_install_dir" == '全域 有空白 !&' ]] || fail 'path boundary'
[[ "$installer_cli_non_interactive" == 1 && "$installer_cli_dry_run" == 1 && "$installer_cli_verbose" == 1 ]] || fail 'boolean options'
installer_cli_parse --no-global-agent </dev/null
[[ "$installer_cli_global_agents" == 0 ]] || fail 'global override'

expect_invalid --help -h
expect_invalid --skills
expect_invalid --skills general
expect_invalid --list-skills
expect_invalid --work-install-dir
expect_invalid --global-agents
expect_invalid --no-global-agents
expect_invalid --skill-install-dir old/work
expect_invalid --global-agents-install-dir old/global
expect_invalid --skills-install-dir
expect_invalid --skills-install-dir ''
expect_invalid --global-agent-install-dir --dry-run
expect_invalid --install-dir old/work
expect_invalid --global-agent --no-global-agent
expect_invalid --unknown
expect_invalid --yes

printf 'macOS CLI parser cases passed.\n'
