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


# State reader contract: reordered keys/records, escaped keys, and malformed or ambiguous JSON.
source "$repo_root/os-scripts/mac/installer/state/state.sh"
state="$cli/install-state.json"
cp -- "$state" "$scratch/state.orig"
hash="$(printf '%064d' 0)"
fixture='{"files":[{"sha256":"'"$hash"'","path":"host-skills/tooling/SKILL.md"},{"path":"host-skills/work/SKILL.md","sha256":"'"$hash"'"}],"status":"installed","global_instructions_dir":null,"install_dir":"/tmp/state","rust_workspace_version":"1.2.3","version":"1.2.3","host":"codex"}'
printf '%s\n' "$fixture" > "$scratch/state.fixture"
installer_state_read "$scratch/state.fixture" || fail 'reordered compact State rejected'
printf '%s\n' "$fixture" | sed 's/"host":/"\\u0068ost":/' > "$scratch/state.fixture"
installer_state_read "$scratch/state.fixture" || fail 'valid escaped JSON key rejected'
reject_state() {
    printf '%s\n' "$2" > "$scratch/state.fixture"
    if installer_state_read "$scratch/state.fixture"; then fail "accepted $1 State"; fi
}
reject_state duplicate-root "$(printf '%s' "$fixture" | sed 's/"host":"codex"/"host":"codex","\\u0068ost":"codex"/')"
reject_state duplicate-field "$(printf '%s' "$fixture" | sed 's/"path":"host-skills\/work\/SKILL.md"/"path":"host-skills\/work\/SKILL.md","\\u0070ath":"host-skills\/work\/SKILL.md"/')"
reject_state unknown-field "$(printf '%s' "$fixture" | sed 's/"status":"installed"/"extra":"x","status":"installed"/')"
reject_state unknown-file-field "$(printf '%s' "$fixture" | sed 's/"path":"host-skills\/work\/SKILL.md"/"extra":"x","path":"host-skills\/work\/SKILL.md"/')"
reject_state wrong-type "$(printf '%s' "$fixture" | sed 's/"version":"1.2.3"/"version":null/')"
reject_state empty-files "$(printf '%s' "$fixture" | sed 's/\[{.*}\]/[]/')"
reject_state wrong-files-type "$(printf '%s' "$fixture" | sed 's/\[{.*}\]/null/')"
reject_state duplicate-path "$(printf '%s' "$fixture" | sed 's@host-skills/tooling/SKILL.md@host-skills/work/SKILL.md@')"
reject_state invalid-path "$(printf '%s' "$fixture" | sed 's@host-skills/tooling/SKILL.md@skills/../tooling/SKILL.md@')"
reject_state missing-entry "$(printf '%s' "$fixture" | sed 's@host-skills/tooling/SKILL.md@bin/work@')"
reject_state wrong-hash "$(printf '%s' "$fixture" | sed "s/$hash/abc/")"
reject_state wrong-host "$(printf '%s' "$fixture" | sed 's/"host":"codex"/"host":"other"/')"
reject_state wrong-global "$(printf '%s' "$fixture" | sed 's@"global_instructions_dir":null@"global_instructions_dir":"/tmp/global"@')"
reject_state invalid-json "$fixture,"
reject_state missing-field "$(printf '%s' "$fixture" | sed 's/,"host":"codex"//')"

# Reinstall accepts equivalent non-pretty State, then restores deterministic pretty output.
tr -d '\n' < "$scratch/state.orig" > "$state"
run --instructions-dir "$globaldir" || { cat "$scratch/out"; fail 'compact State reinstall'; }
cmp -s "$state" "$scratch/state.orig" || fail 'State output is not stable'
# A legacy manifest with all the original data must be rejected, not migrated.
installer_state_read "$state" || fail 'read original State'
{
    printf '{\n  "host": "%s",\n  "version": "%s",\n  "rust_workspace_version": "%s",\n' "$installer_state_host" "$installer_state_version" "$installer_state_rust_version"
    printf '  "install_dir": "%s",\n  "global_instructions_dir": "%s",\n  "status": "installed",\n  "files": [\n' "$installer_state_install_dir" "$installer_state_global_dir"
    for ((i=0; i<${#installer_state_files[@]}; i++)); do
        printf '    "%s"' "${installer_state_files[i]}"
        if (( i+1 < ${#installer_state_files[@]} )); then printf ',\n'; else printf '\n'; fi
    done
    printf '  ]\n}\n'
} > "$state"
before="$(snapshot)"
run --instructions-dir "$globaldir" && fail 'legacy string-array State accepted'
[[ "$(snapshot)" == "$before" ]] || fail 'legacy State was modified'
cp -- "$scratch/state.orig" "$state"

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
