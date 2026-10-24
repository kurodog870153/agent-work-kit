#!/usr/bin/env bash

installer_run() {
    local mode="$1" root="$2" helper lock_build='' result
    shift 2
    local module_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
    local installer_dir="$(cd -- "$module_dir/.." && pwd -P)"
    local source_work="$root/skills/work"
    source "$installer_dir/cli/parse-options.sh"
    installer_cli_parse "$@" || return 2
    if [[ "$installer_cli_action" == help ]]; then installer_cli_usage; return 0; fi
    if [[ "$installer_cli_action" == list-skills ]]; then installer_cli_list_skills; return 0; fi
    if [[ "$mode" == source ]]; then
        command -v xcrun >/dev/null 2>&1 || { printf 'Error: Xcode Command Line Tools are required.\n' >&2; return 1; }
        lock_build="$(mktemp -d "${TMPDIR:-/tmp}/work-lock.XXXXXXXX")" || return 1
        helper="$lock_build/work-install-lock"
        if ! xcrun clang -std=c11 -Wall -Wextra -Werror -o "$helper" "$installer_dir/lock/native-lock.c"; then
            printf 'Error: could not build macOS installation lock.\n' >&2
            return 1
        fi
    else
        helper="$root/bin/work-install-lock"
        [[ -x "$helper" ]] || { printf 'Error: packaged macOS lock helper is missing.\n' >&2; return 1; }
    fi
    "$helper" /private/tmp/codex-workflow-kit-work-install.lock /bin/bash \
        "$installer_dir/runtime/execute-locked.sh" "$mode" "$root" "$@"
    result=$?
    if [[ -n "$lock_build" && -d "$lock_build" ]]; then rm -rf -- "$lock_build"; fi
    return "$result"
}
