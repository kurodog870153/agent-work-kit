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
source "$script_directory/installer/package/verify-integrity.sh"
if ! verify_release_package "$script_directory"; then exit 1; fi
if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
    printf 'Error: this release package requires macOS ARM64.\n' >&2
    exit 1
fi
for binary in work skill; do
    if [[ ! -x "$script_directory/bin/$binary" ]] || ! "$script_directory/bin/$binary" --help >/dev/null 2>&1; then
        printf 'Error: packaged %s binary failed its startup check.\n' "$binary" >&2
        exit 1
    fi
done
source "$script_directory/installer/install/install-core.sh"
installer_run package "$script_directory" "$@"
