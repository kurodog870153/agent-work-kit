#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
scratch="$(mktemp -d "/private/tmp/agent-work-kit-test-lock.XXXXXXXX")"
holder=''
trap 'if [[ -n "$holder" ]]; then kill -9 "$holder" 2>/dev/null || true; wait "$holder" 2>/dev/null || true; fi; rm -rf -- "$scratch"' EXIT
chmod 755 "$scratch"
cc -std=c11 -Wall -Wextra -Werror -o "$scratch/work-install-lock" "$repo_root/os-scripts/mac/installer/lock/native-lock.c"
helper="$scratch/work-install-lock"
marker="$scratch/ready"

# Normal exit releases the lock, allowing the next installation.
"$helper" /usr/bin/true
"$helper" /usr/bin/true
"$helper" /bin/bash -c 'printf ready > "$1"; exec sleep 120' bash "$marker" &
holder=$!
for attempt in $(seq 1 100); do
    [[ -f "$marker" ]] && break
    kill -0 "$holder" 2>/dev/null || { wait "$holder"; exit 1; }
    sleep 0.1
done
[[ -f "$marker" ]] || exit 1
if "$helper" /usr/bin/true >"$scratch/contend.log" 2>&1; then
    printf 'FAIL: concurrent same-user installation acquired lock.\n' >&2; exit 1
fi
grep -F 'another Work installation is running' "$scratch/contend.log"
[[ "$(stat -f %Lp "/private/tmp/agent-work-kit-user-$(id -u).lock")" == 600 ]] || exit 1

# Lightweight check of a different real UID, not a second Installer E2E.
if [[ "${GITHUB_ACTIONS:-}" == true ]]; then
    [[ "$(id -u)" != "$(id -u nobody)" ]] || exit 1
    sudo -n -u nobody "$helper" /usr/bin/true
fi
kill -0 "$holder"
kill -9 "$holder"; wait "$holder" 2>/dev/null || true; holder=''
"$helper" /usr/bin/true
printf 'macOS user installation lock cases passed.\n'
