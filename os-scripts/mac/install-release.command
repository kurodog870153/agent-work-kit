#!/usr/bin/env bash

set -u

if (( $# != 0 )); then
    printf 'Usage: install-release.command\n' >&2
    exit 2
fi

script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
project_directory="$script_directory"
source_work="$project_directory/skills/work"
global_agents_source="$project_directory/AGENTS.md"
built_work="$project_directory/bin/work"
missing_source=""
work_install_mode=release

for module in verify-package.sh global-agents.sh work-core.sh; do
    if [[ ! -f "$script_directory/installer/$module" ]]; then
        printf 'Error: release installer module is missing: %s\n' "$module" >&2
        exit 1
    fi
done
source "$script_directory/installer/verify-package.sh"
source "$script_directory/installer/global-agents.sh"
source "$script_directory/installer/work-core.sh"

if ! verify_release_package "$project_directory"; then
    exit 1
fi
if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
    printf 'Error: this release package requires macOS ARM64.\n' >&2
    exit 1
fi
if [[ ! -x "$built_work" ]] || ! "$built_work" --help >/dev/null 2>&1; then
    printf 'Error: packaged Work binary is not executable or failed its startup check.\n' >&2
    exit 1
fi

build_work() {
    [[ -x "$built_work" ]] && "$built_work" --help >/dev/null 2>&1
}

if ! install_global_agents; then
    exit 1
fi
run_work_install
