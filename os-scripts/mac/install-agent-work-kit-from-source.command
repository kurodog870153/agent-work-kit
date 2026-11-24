#!/usr/bin/env bash
set -u
# Only the outermost public entry owns the final prompt.
installer_entry_non_interactive=0
for installer_entry_arg in "$@"; do
    [[ "$installer_entry_arg" != --non-interactive ]] || installer_entry_non_interactive=1
done
installer_entry_finish() {
    local result=$?
    trap - EXIT
    if [[ -n "${installer_entry_scratch:-}" ]]; then
        rm -rf -- "$installer_entry_scratch" ||
            printf 'Warning: Downloader temporary directory awaits cleanup: %s\n' "$installer_entry_scratch" >&2
    fi
    if [[ "$installer_entry_non_interactive" != 1 && "${AGENT_WORK_KIT_INSTALLER_NESTED:-}" != 1 &&
          -z "${CI:-}" && -z "${GITHUB_ACTIONS:-}" && -t 0 ]]; then
        printf '\nInstaller exited (code %s).\nPress Enter to exit...' "$result"
        IFS= read -r installer_entry_answer || true
    fi
    exit "$result"
}
trap installer_entry_finish EXIT
script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
project_directory="$(cd -- "$script_directory/../.." && pwd -P)"
source "$script_directory/installer/install/install-core.sh"
installer_run source "$project_directory" "$@"
