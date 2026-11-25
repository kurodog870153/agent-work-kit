#!/usr/bin/env bash
# Installer core scenarios on a temporary target with stub binaries (fixtures are generated here).
set -uo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
core="$repo_root/os-scripts/mac/installer/install/install-core.sh"
scratch="$(mktemp -d "$repo_root/.installer-test-core.XXXXXX")"
trap 'rm -rf -- "$scratch"' EXIT
fail() { printf 'FAIL: %s\n' "$1" >&2; exit 1; }
root="$scratch/package"; mkdir -p "$root/bin" "$root/instructions" "$scratch/home" "$scratch/shims"
cp -R -- "$repo_root/skills" "$root/skills"
cp -- "$repo_root/instructions/global-instructions.md" "$root/instructions/"
printf '#!/bin/sh\nexit 0\n' > "$root/bin/work"; cp "$root/bin/work" "$root/bin/skill"; chmod 755 "$root/bin/"*
printf '{\n  "host": "codex",\n  "version": "2.5.0",\n  "rust_workspace_version": "3.0.0",\n  "target": "aarch64-apple-darwin"\n}\n' > "$root/release-info.json"
printf '#!/bin/sh\nexit 1\n' > "$scratch/shims/lsof"; chmod 755 "$scratch/shims/lsof"
cc -std=c11 -Wall -Wextra -Werror -o "$root/bin/work-install-lock" "$repo_root/os-scripts/mac/installer/lock/native-lock.c" || exit 1
source "$core"
export PATH="$scratch/shims:$PATH" HOME="$scratch/home"
cli="$scratch/t/agent-work-kit"; skills="$cli/skills"; globaldir="$scratch/g"
hostentries="$HOME/.agents/skills"
run() { installer_run package "$root" --host codex --non-interactive --install-dir "$cli" "$@" >"$scratch/out" 2>&1; }
snapshot() { (cd "$scratch/t" 2>/dev/null && find . -type f -exec shasum -a 256 {} + | LC_ALL=C sort; cd "$scratch" && find g home/.agents/skills -type f -exec shasum -a 256 {} + 2>/dev/null | LC_ALL=C sort) ; }

run --global-instructions no --dry-run || fail 'dry run'
[[ ! -e "$scratch/t" ]] || fail 'dry run changed target'
if INSTALLER_TEST_FAILURE=apply-host-work run --global-instructions no; then fail 'first-entry failure was ignored'; fi
[[ ! -e "$cli" && ! -e "$hostentries/work" && ! -L "$hostentries/work" ]] || {
    printf 'First-entry failure output:\n' >&2
    cat "$scratch/out" >&2
    printf 'Remaining entries:\n' >&2
    find "$scratch/t" "$HOME/.agents" -maxdepth 6 -print 2>/dev/null >&2 || true
    fail 'first-entry rollback left resources'
}
run --instructions-dir "$globaldir" || { cat "$scratch/out"; fail 'first install'; }
for path in "$hostentries/work/SKILL.md" "$hostentries/tooling/SKILL.md" "$skills/work/instructions/instructions.md" "$skills/tooling/instructions/instructions.md" "$skills/shared/bootstrap.md" "$cli/install-state.json" "$cli/bin/work" "$cli/bin/skill" "$globaldir/AGENTS.md"; do
    [[ -f "$path" ]] || fail "missing $path"
done
for name in work tooling; do
    [[ -d "$hostentries/$name" && ! -L "$hostentries/$name" && -f "$hostentries/$name/SKILL.md" && ! -L "$hostentries/$name/SKILL.md" ]] || fail 'physical Host entry missing'
    [[ ! -e "$skills/$name/SKILL.md" ]] || fail 'Runtime entry template installed'
done
[[ ! -e "$skills/contract" && ! -e "$skills/shared/SKILL.md" && ! -e "$skills/work/scripts" ]] || fail 'obsolete resources installed'
[[ ! -e "$cli/.agent-work-kit-txn" ]] || fail 'transaction left behind'
run --instructions-dir "$globaldir" || fail 'reinstall'
grep -q 'unchanged' "$scratch/out" || fail 'reinstall should keep Global Instructions'

# Failure injection: each stage fails, the previous installation stays intact.
before="$(snapshot)"
for stage in prepare apply-work-moved apply-work apply-shared apply-bin apply-host-work-moved apply-host-work apply-host-tooling verify; do
    if INSTALLER_TEST_FAILURE="$stage" run --instructions-dir "$globaldir"; then fail "failure at $stage was ignored"; fi
    [[ "$(snapshot)" == "$before" && ! -e "$cli/.agent-work-kit-txn" ]] || fail "failure at $stage did not restore"
done
# apply-global is only reached when Global Instructions change: our own previous copy is replaceable.
cp -- "$globaldir/AGENTS.md" "$scratch/global.orig"
cp -- "$root/instructions/global-instructions.md" "$scratch/global.src"
printf '
updated
' >> "$root/instructions/global-instructions.md"
before_global="$(snapshot)"
if INSTALLER_TEST_FAILURE=apply-global run --instructions-dir "$globaldir"; then fail 'failure at apply-global was ignored'; fi
[[ "$(snapshot)" == "$before_global" && ! -e "$cli/.agent-work-kit-txn" ]] || fail 'failure at apply-global did not restore'
run --instructions-dir "$globaldir" || fail 'replace own Global Instructions'
cmp -s "$root/instructions/global-instructions.md" "$globaldir/AGENTS.md" || fail 'Global Instructions not replaced'
cp -- "$scratch/global.src" "$root/instructions/global-instructions.md"
run --instructions-dir "$globaldir" || fail 'restore Global Instructions'
before="$(snapshot)"
# Crash then recovery on the next run (crash before the commit marker restores; after it completes).
for stage in apply-work apply-bin apply-host-work apply-host-tooling verify commit; do
    INSTALLER_TEST_CRASH="$stage" installer_run package "$root" --host codex --non-interactive --install-dir "$cli" --instructions-dir "$globaldir" >/dev/null 2>&1
    [[ $? -eq 97 ]] || fail "crash injection $stage"
    run --instructions-dir "$globaldir" || { cat "$scratch/out"; fail "recovery after $stage"; }
    [[ ! -e "$cli/.agent-work-kit-txn" ]] || fail "transaction left after $stage"
done
INSTALLER_TEST_CRASH=after-commit installer_run package "$root" --host codex --non-interactive --install-dir "$cli" --instructions-dir "$globaldir" >/dev/null 2>&1
[[ $? -eq 97 ]] || fail 'crash after commit'
run --instructions-dir "$globaldir" || fail 'recovery after commit'
[[ -f "$cli/install-state.json" && ! -e "$cli/.agent-work-kit-txn" ]] || fail 'commit recovery'

# Reject a legacy entry and missing or modified physical entries.
cp "$hostentries/work/SKILL.md" "$scratch/entry.orig"
rm -rf -- "$hostentries/work"
mkdir -p "$scratch/foreign"
printf 'keep' > "$scratch/foreign/keep.txt"
ln -s "$scratch/foreign" "$hostentries/work"
run --instructions-dir "$globaldir" && fail 'accepted legacy discovery entry'
[[ -L "$hostentries/work" && "$(cat "$scratch/foreign/keep.txt")" == keep ]] || fail 'legacy entry or foreign data changed'
rm -- "$hostentries/work"
run --instructions-dir "$globaldir" && fail 'repaired missing entry'
mkdir "$hostentries/work"
cp "$scratch/entry.orig" "$hostentries/work/SKILL.md"
printf 'tampered' >> "$hostentries/work/SKILL.md"
before="$(snapshot)"
run --instructions-dir "$globaldir" && fail 'accepted modified entry'
[[ "$(snapshot)" == "$before" ]] || fail 'modified entry changed'
cp "$scratch/entry.orig" "$hostentries/work/SKILL.md"

# Unknown content is never taken over.
rm -rf -- "$hostentries/work" "$hostentries/tooling"; rm -rf -- "$scratch/t" "$globaldir"; mkdir -p "$skills/work"; printf 'mine' > "$skills/work/notes.txt"
before="$(snapshot)"
run --global-instructions no && fail 'accepted unknown Work content'
[[ "$(snapshot)" == "$before" ]] || fail 'unknown content changed'
rm -rf -- "$scratch/t"; mkdir -p "$scratch/g"; printf 'user\n' > "$scratch/g/AGENTS.md"
run --instructions-dir "$scratch/g" && fail 'replaced differing Global Instructions non-interactively'
[[ "$(cat "$scratch/g/AGENTS.md")" == user && ! -e "$skills/work" ]] || fail 'Global Instructions changed'
run --global-instructions no || fail 'install without Global Instructions'
echo x >> "$hostentries/work/SKILL.md"
before="$(snapshot)"; run --global-instructions no && fail 'accepted modified installation'
[[ "$(snapshot)" == "$before" ]] || fail 'modified installation changed'
# Wrong Host package.
rm -rf -- "$hostentries/work" "$hostentries/tooling"; rm -rf -- "$scratch/t"; installer_run package "$root" --host claude --non-interactive --install-dir "$cli" >/dev/null 2>&1 && fail 'Host mismatch accepted'
[[ ! -e "$scratch/t" ]] || fail 'Host mismatch changed target'
printf 'macOS installer cases passed.\n'
