#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
scratch="$(mktemp -d "$repo_root/.issue95-lock.XXXXXX")"
[[ "$(cd "$scratch" && pwd -P)" == "$scratch" && "$scratch" == "$repo_root"/.issue95-lock.* ]] || exit 1
trap 'rm -rf -- "$scratch"' EXIT
cc -std=c11 -Wall -Wextra -Werror -o "$scratch/work-install-lock" "$repo_root/os-scripts/mac/installer/lock/native-lock.c"
helper="$scratch/work-install-lock"
lock="$scratch/work.lock"
marker="$scratch/ready"
"$helper" "$lock" /bin/bash -c 'printf ready > "$1"; exec sleep 2' bash "$marker" &
holder=$!
for attempt in 1 2 3 4 5 6 7 8 9 10; do
    [[ -f "$marker" ]] && break
    sleep 0.1
done
[[ -f "$marker" ]] || { wait "$holder"; exit 1; }
if "$helper" "$lock" /usr/bin/true 2>/dev/null; then
    printf 'FAIL: concurrent installation acquired lock.\n' >&2
    exit 1
fi
wait "$holder"
"$helper" "$lock" /usr/bin/true
[[ "$(stat -f %Lp "$lock")" == 666 ]] || { printf 'FAIL: lock is not shared across users.\n' >&2; exit 1; }
if "$helper" "$lock" /bin/bash -c 'kill -9 $$' 2>/dev/null; then
    printf 'FAIL: abnormal exit reported success.\n' >&2
    exit 1
fi
"$helper" "$lock" /usr/bin/true
printf 'preserve' > "$scratch/other"
ln -s "$scratch/other" "$scratch/linked.lock"
if "$helper" "$scratch/linked.lock" /usr/bin/true 2>/dev/null; then
    printf 'FAIL: symlink lock accepted.\n' >&2
    exit 1
fi
[[ "$(cat "$scratch/other")" == preserve ]] || exit 1
printf 'macOS native lock cases passed.\n'
