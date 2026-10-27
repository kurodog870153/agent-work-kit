#!/usr/bin/env bash
set -u
script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
project_directory="$(cd -- "$script_directory/../.." && pwd -P)"
source "$script_directory/installer/install/install-core.sh"
installer_run source "$project_directory" "$@"
