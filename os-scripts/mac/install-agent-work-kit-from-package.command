#!/usr/bin/env bash
set -u
script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
source "$script_directory/installer/package/verify-integrity.sh"
if ! verify_release_package "$script_directory"; then exit 1; fi
if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
    printf 'Error: this release package requires macOS ARM64.\n' >&2
    exit 1
fi
if [[ ! -x "$script_directory/bin/work" ]] || ! "$script_directory/bin/work" --help >/dev/null 2>&1; then
    printf 'Error: packaged Work binary failed its startup check.\n' >&2
    exit 1
fi
source "$script_directory/installer/install/install-core.sh"
installer_run package "$script_directory" "$@"
