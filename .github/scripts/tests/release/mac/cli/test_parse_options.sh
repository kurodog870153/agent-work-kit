#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
source_work="$repo_root/skills/work"
source "$repo_root/os-scripts/mac/installer/cli/parse-options.sh"

fail() { printf 'FAIL: %s\n' "$1" >&2; exit 1; }
[[ "$(installer_cli_catalog_path)" == "$repo_root/os-scripts/mac/installer/cli/../../../common/installer/options.tsv" ]] || fail 'shared option catalog path'
[[ "$(installer_cli_skill_path spring-boot)" == 'programming-language/java/spring-boot' ]] || fail 'shared Skill catalog'
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
installer_cli_parse --list-skills </dev/null
[[ "$installer_cli_action" == list-skills ]] || fail 'list action'
skill_list="$(installer_cli_list_skills)"
grep -qx 'spring-boot' <<< "$skill_list" || fail 'missing actual Skill ID'
installer_cli_parse --skills java,spring-boot --global-agents --work-install-dir '目錄 有空白 !&/work' --global-agents-install-dir '全域 有空白 !&' --non-interactive --dry-run --verbose </dev/null
[[ "$installer_cli_skills" == 'java,spring-boot' ]] || fail 'skill value'
[[ "$installer_cli_work_install_dir" == '目錄 有空白 !&/work' && "$installer_cli_global_agents_install_dir" == '全域 有空白 !&' ]] || fail 'path boundary'
[[ "$installer_cli_non_interactive" == 1 && "$installer_cli_dry_run" == 1 && "$installer_cli_verbose" == 1 ]] || fail 'boolean options'
installer_cli_parse --skills general --no-global-agents </dev/null
[[ "$installer_cli_global_agents" == 0 ]] || fail 'global override'
installer_cli_parse --skills all </dev/null
[[ "$installer_cli_skills" == all ]] || fail 'all selection'

expect_invalid --help -h
expect_invalid --skills
expect_invalid --skills ''
expect_invalid --skills general,general
expect_invalid --skills web,
expect_invalid --skills web,,general
expect_invalid --skills nonexistent
expect_invalid --work-install-dir
expect_invalid --global-agents-install-dir --dry-run
expect_invalid --install-dir old/work
expect_invalid --global-agents --no-global-agents
expect_invalid --dry-run --list-skills
expect_invalid --unknown
expect_invalid --yes

printf 'macOS CLI parser cases passed.\n'
