#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
scratch="$(mktemp -d "$repo_root/.issue95-core.XXXXXX")"
[[ "$(cd "$scratch" && pwd -P)" == "$scratch" && "$scratch" == "$repo_root"/.issue95-core.* ]] || exit 1
trap 'chmod 755 "$scratch/home/.agents/skills/work" "$scratch/home/.agents/skills" 2>/dev/null || true; rm -rf -- "$scratch"' EXIT
root="$scratch/package"
mkdir -p -- "$root/skills" "$root/bin" "$scratch/home" "$scratch/shims"
cp -R -- "$repo_root/skills/work" "$root/skills/work"
cp -- "$repo_root/AGENTS.md" "$root/AGENTS.md"
printf '{"version":"1.0.5"}\n' > "$root/release-info.json"
mkdir -p -- "$root/installer/state"
cp -- "$repo_root/os-scripts/common/installer/install-state.json" "$root/installer/state/install-state.json"
cp -- "$repo_root/rust/target/release/work" "$root/bin/work"
chmod 755 "$root/bin/work"
cat > "$scratch/shims/osascript" <<'JXA'
#!/usr/bin/env bash
[[ "$1" == -l && "$2" == JavaScript ]] || exit 2
shift 2
script="$1"
if [[ "$script" == -e ]]; then
    shift
    MSYS_NO_PATHCONV=1 node -e "$1; process.stdout.write(run(process.argv.slice(1)))" "${@:2}"
    exit $?
fi
if command -v cygpath >/dev/null 2>&1; then script="$(cygpath -w "$script")"; fi
shift
MSYS_NO_PATHCONV=1 node -e 'process.stdout.write(require(process.argv[1]).run(process.argv.slice(2)))' "$script" "$@"
JXA
chmod 755 "$scratch/shims/osascript"
export PATH="$scratch/shims:$PATH"
export HOME="$scratch/home"
mkdir -p -- "$scratch/linked-codex"
ln -s -- "$scratch/linked-codex" "$HOME/.codex"
target="$scratch/installed/work"
if ! bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
    --non-interactive --no-global-agents --skills general --work-install-dir "$target" --dry-run >/dev/null; then
    printf 'FAIL: macOS core dry run failed.\n' >&2; exit 1
fi
[[ ! -e "$target" ]] || { printf 'FAIL: dry run modified target.\n' >&2; exit 1; }
bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
    --non-interactive --no-global-agents --skills general --work-install-dir "$target" >/dev/null
[[ -f "$target/scripts/work" && -f "$target/install-state.json" ]] || exit 1
printf keep > "$target/unknown.txt"
for mode in interactive non-interactive; do
    if [[ "$mode" == non-interactive ]]; then
        output="$(bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
            --non-interactive --no-global-agents --skills general --work-install-dir "$target" 2>&1)" && {
            printf 'FAIL: %s accepted unknown Work content.\n' "$mode" >&2; exit 1;
        }
    else
        output="$(printf 'y\n' | bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
            --no-global-agents --skills general --work-install-dir "$target" 2>&1)" && {
            printf 'FAIL: %s accepted unknown Work content.\n' "$mode" >&2; exit 1;
        }
    fi
    [[ "$output" == *'unknown entry: unknown.txt'* && "$output" == *'--work-install-dir'* &&
       "$(cat -- "$target/unknown.txt")" == keep ]] || {
        printf 'FAIL: %s did not explain or preserve unknown Work content.\n' "$mode" >&2; exit 1;
    }
done
rm -- "$target/unknown.txt"
ln -s -- "$root/AGENTS.md" "$target/unsafe-link"
output="$(printf 'y\n' | bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
    --no-global-agents --skills general --work-install-dir "$target" 2>&1)" && {
    printf 'FAIL: interactive mode accepted unsafe Work symlink.\n' >&2; exit 1;
}
[[ "$output" == *'unsafe entry: unsafe-link'* && "$output" == *'--work-install-dir'* && -L "$target/unsafe-link" ]] || {
    printf 'FAIL: unsafe Work symlink diagnostic or preservation failed.\n' >&2; exit 1;
}
rm -- "$target/unsafe-link"
[[ ! -e "$scratch/linked-codex/AGENTS.md" ]] || {
    printf 'FAIL: --no-global-agents modified linked Global AGENTS directory.\n' >&2; exit 1;
}
if bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
    --non-interactive --global-agents --skills general --work-install-dir "$scratch/unsafe-global/work" >/dev/null 2>&1; then
    printf 'FAIL: enabled Global AGENTS accepted linked directory.\n' >&2; exit 1
fi
interactive_target="$scratch/custom Work/work"
if ! printf '2\n%s\ny\n' "$interactive_target" | \
    bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
        --skills general --no-global-agents >/dev/null; then
    printf 'FAIL: interactive custom Work directory install failed.\n' >&2; exit 1
fi
[[ -f "$interactive_target/scripts/work" ]] || { printf 'FAIL: custom Work target missing.\n' >&2; exit 1; }
preview_global="$scratch/preview Global"
mkdir -- "$preview_global"
printf 'private existing instructions\n' > "$preview_global/AGENTS.md"
preview_output="$(printf 'y\n2\n%s\n\nn\n' "$preview_global" | \
    bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
        --work-install-dir "$scratch/preview Work/work" --dry-run)" || {
    printf 'FAIL: interactive Global AGENTS preview failed.\n' >&2; exit 1;
}
[[ "$preview_output" == *'Available Skills: general,web,backend'* &&
   "$preview_output" == *'Global AGENTS.md differs'* &&
   "$preview_output" != *'private existing instructions'* &&
   "$preview_output" != *'Global AGENTS directory: 1) current/default'* ]] || {
    printf 'FAIL: interactive Skills list or Global AGENTS comparison output is wrong.\n' >&2; exit 1;
}
global_work_target="$scratch/global Work/work"
global_dir="$scratch/custom Global"
if ! printf '2\n%s\ny\n' "$global_dir" | \
    bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
        --work-install-dir "$global_work_target" --skills general --global-agents >/dev/null; then
    printf 'FAIL: interactive custom Global AGENTS directory install failed.\n' >&2; exit 1
fi
[[ -f "$global_work_target/scripts/work" && -f "$global_dir/AGENTS.md" ]] || {
    printf 'FAIL: custom Global AGENTS target missing.\n' >&2; exit 1;
}
default_global_home="$scratch/default Global home"
mkdir -- "$default_global_home"
if ! printf '\n1\n\ny\n' | HOME="$default_global_home" \
    bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
        --work-install-dir "$global_work_target" >/dev/null; then
    printf 'FAIL: default Global AGENTS directory reinstall failed.\n' >&2; exit 1
fi
[[ -f "$default_global_home/.codex/AGENTS.md" && -f "$global_dir/AGENTS.md" ]] || {
    printf 'FAIL: default Global AGENTS choice did not select the displayed directory.\n' >&2; exit 1;
}
default_target="$HOME/.agents/skills/work"
bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
    --non-interactive --no-global-agents --skills general >/dev/null
migration_target="$scratch/migrated/work"
chmod 555 "${default_target%/*}"
chmod 555 "$default_target"
bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
    --non-interactive --no-global-agents --skills general --work-install-dir "$migration_target" >/dev/null
record="$scratch/migrated/.work-migration-cleanup"
[[ -f "$record" && -f "$default_target/scripts/work" && -f "$migration_target/scripts/work" ]] || {
    printf 'FAIL: migration cleanup was not recorded.\n' >&2; exit 1;
}
chmod 755 "${default_target%/*}"
chmod 755 "$default_target"
printf changed > "$default_target/SKILL.md"
bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
    --non-interactive --no-global-agents --skills general --work-install-dir "$migration_target" >/dev/null
[[ -f "$record" && -f "$default_target/SKILL.md" ]] || {
    printf 'FAIL: changed previous Work was deleted.\n' >&2; exit 1;
}
cp -- "$root/skills/work/SKILL.md" "$default_target/SKILL.md"
bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
    --non-interactive --no-global-agents --skills general --work-install-dir "$migration_target" >/dev/null
[[ ! -e "$record" && ! -e "$default_target" && -f "$migration_target/scripts/work" ]] || {
    printf 'FAIL: pending migration cleanup did not finish.\n' >&2; exit 1;
}
module_path="$repo_root/os-scripts/mac/installer/state/state-model.js"
state_path="$target/install-state.json"
if command -v cygpath >/dev/null 2>&1; then
    module_path="$(cygpath -w "$module_path")"
    state_path="$(cygpath -w "$state_path")"
fi
MSYS_NO_PATHCONV=1 node -e 'const s=require(process.argv[1]); const fs=require("fs"); const v=s.parseState(fs.readFileSync(process.argv[2],"utf8"),process.argv[3]); if(!v||v.skills.join(",")!=="general"||v.global_agents_dir!==null) process.exit(1)' \
    "$module_path" "$state_path" "$target"
rm -- "$target/install-state.json"
if bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
    --non-interactive --no-global-agents --skills general --work-install-dir "$target" >/dev/null 2>&1; then
    printf 'FAIL: noninteractive first update was accepted.\n' >&2; exit 1
fi
[[ -f "$target/scripts/work" && ! -e "$target/install-state.json" ]] || exit 1
cp -R -- "$repo_root/os-scripts/mac/installer" "$scratch/crash-installer"
cp -- "$repo_root/os-scripts/common/installer/options.tsv" "$scratch/crash-installer/cli/options.tsv"
for crash_point in staging first-rename second-rename before-verified after-verified; do
    crash_target="$scratch/crash-$crash_point/work"
    crash_global="$scratch/crash-$crash_point/global"
    printf '{"version":"1.0.5"}\n' > "$root/release-info.json"
    bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
        --non-interactive --global-agents --global-agents-install-dir "$crash_global" \
        --skills general --work-install-dir "$crash_target" >/dev/null
    global_before="$(shasum -a 256 "$crash_global/AGENTS.md")"
    printf '{"version":"1.0.6"}\n' > "$root/release-info.json"
    cp -- "$repo_root/os-scripts/mac/installer/transaction/work.sh" "$scratch/crash-installer/transaction/work.sh"
    python3 -B - "$scratch/crash-installer/transaction/work.sh" "$crash_point" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
point = sys.argv[2]
replacements = {
    "staging": ('    printf \'%s\\n\' "${binary_hash%% *}" > "$txn/binary-sha256.txt" || return 1\n', '    kill -9 $$\n'),
    "first-rename": ('        mv -- "$final" "$previous" || return 1\n', '        kill -9 $$\n'),
    "second-rename": ('! mv -- "$prepared" "$final" || ! "$validate_fn" "$final"',
                      '! mv -- "$prepared" "$final" || { kill -9 $$; } || ! "$validate_fn" "$final"'),
    "before-verified": ("    printf 'verified\\n' > \"$txn/verified.txt\" || return 1\n", '    kill -9 $$\n'),
    "after-verified": ("    printf 'verified\\n' > \"$txn/verified.txt\" || return 1\n",
                       "    printf 'verified\\n' > \"$txn/verified.txt\" || return 1\n    kill -9 $$\n"),
}
before, after = replacements[point]
contents = path.read_text()
if contents.count(before) != 1:
    raise SystemExit(f"crash injection point {point} is ambiguous")
path.write_text(contents.replace(before, before + after if point not in ("second-rename", "after-verified") else after))
PY
    set +e
    bash "$scratch/crash-installer/runtime/execute-locked.sh" package "$root" \
        --non-interactive --global-agents --global-agents-install-dir "$crash_global" \
        --skills general --work-install-dir "$crash_target" >/dev/null 2>&1
    crash_exit=$?
    set -e
    [[ "$crash_exit" == 137 ]] || {
        printf 'FAIL: %s crash returned %s instead of 137.\n' "$crash_point" "$crash_exit" >&2; exit 1;
    }
    crash_txn="$(find "$scratch/crash-$crash_point" -maxdepth 1 -name '.work-install.*' -type d -print -quit)"
    [[ -n "$crash_txn" ]] || { printf 'FAIL: %s left no transaction record.\n' "$crash_point" >&2; exit 1; }
    old_path="$crash_target"
    [[ -d "$crash_txn/previous" ]] && old_path="$crash_txn/previous"
    [[ -f "$old_path/install-state.json" &&
       "$(cat -- "$old_path/install-state.json")" == *'"1.0.5"'* &&
       "$(shasum -a 256 "$crash_global/AGENTS.md")" == "$global_before" ]] || {
        printf 'FAIL: %s lost previous Work.\n' "$crash_point" >&2; exit 1;
    }
    bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
        --non-interactive --global-agents --global-agents-install-dir "$crash_global" \
        --skills general --work-install-dir "$crash_target" >/dev/null
    crash_state="$(cat -- "$crash_target/install-state.json")"
    [[ "$crash_state" == *'"1.0.6"'* && -x "$crash_target/scripts/work" &&
       "$(shasum -a 256 "$crash_global/AGENTS.md")" == "$global_before" &&
       -z "$(find "$scratch/crash-$crash_point" -name '.work-install.*' -print -quit)" ]] || {
        printf 'FAIL: %s did not reinstall new Work.\n' "$crash_point" >&2; exit 1;
    }
done
cp -- "$root/AGENTS.md" "$scratch/original-AGENTS.md"
for global_crash in after-global during-work after-work-verified; do
    global_case="$scratch/global-crash-$global_crash"
    global_work="$global_case/work"
    global_agents="$global_case/global"
    cp -- "$scratch/original-AGENTS.md" "$root/AGENTS.md"
    printf '{"version":"1.0.5"}\n' > "$root/release-info.json"
    bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
        --non-interactive --global-agents --global-agents-install-dir "$global_agents" \
        --skills general --work-install-dir "$global_work" >/dev/null
    old_global_hash="$(shasum -a 256 "$global_agents/AGENTS.md" | cut -d ' ' -f1)"
    printf 'new Global AGENTS for %s\n' "$global_crash" > "$root/AGENTS.md"
    new_global_hash="$(shasum -a 256 "$root/AGENTS.md" | cut -d ' ' -f1)"
    printf '{"version":"1.0.6"}\n' > "$root/release-info.json"
    cp -- "$repo_root/os-scripts/mac/installer/transaction/work.sh" "$scratch/crash-installer/transaction/work.sh"
    python3 -B - "$scratch/crash-installer/transaction/work.sh" "$global_crash" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
point = sys.argv[2]
markers = {
    "after-global": '    if ! installer_publish_work "$final" "$source" "$state_valid" "$confirmed" "$non_interactive" \\\n',
    "during-work": '    printf \'%s\\n\' "${binary_hash%% *}" > "$txn/binary-sha256.txt" || return 1\n',
    "after-work-verified": "    printf 'verified\\n' > \"$txn/verified.txt\" || return 1\n",
}
contents = path.read_text()
marker = markers[point]
if contents.count(marker) != 1:
    raise SystemExit(f"ambiguous combined crash point: {point}")
injection = '    kill -9 $$\n'
contents = contents.replace(marker, injection + marker if point == "after-global" else marker + injection)
path.write_text(contents)
PY
    set +e
    printf 'y\ny\n' | bash "$scratch/crash-installer/runtime/execute-locked.sh" package "$root" \
        --global-agents --global-agents-install-dir "$global_agents" \
        --skills general --work-install-dir "$global_work" >/dev/null 2>&1
    crash_exit=$?
    set -e
    [[ "$crash_exit" == 137 && "$(shasum -a 256 "$global_agents/AGENTS.md" | cut -d ' ' -f1)" == "$new_global_hash" ]] || {
        printf 'FAIL: %s did not interrupt after Global AGENTS replacement.\n' "$global_crash" >&2; exit 1;
    }
    global_txn="$(find "$global_case" -maxdepth 1 -name '.agents-install.*' -type d -print -quit)"
    [[ -n "$global_txn" && -f "$global_txn/global-previous" &&
       "$(shasum -a 256 "$global_txn/global-previous" | cut -d ' ' -f1)" == "$old_global_hash" ]] || {
        printf 'FAIL: %s lost previous Global AGENTS.\n' "$global_crash" >&2; exit 1;
    }
    if [[ "$global_crash" == after-global ]]; then
        recorded_hash="$(cat -- "$global_txn/source-sha256.txt")"
        printf 'invalid\n' > "$global_txn/source-sha256.txt"
        if printf 'y\ny\n' | bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
            --global-agents --global-agents-install-dir "$global_agents" \
            --skills general --work-install-dir "$global_work" >/dev/null 2>&1; then
            printf 'FAIL: damaged Global transaction record was accepted.\n' >&2; exit 1
        fi
        [[ "$(cat -- "$global_txn/global-previous")" == "$(cat -- "$scratch/original-AGENTS.md")" &&
           "$(shasum -a 256 "$global_agents/AGENTS.md" | cut -d ' ' -f1)" == "$new_global_hash" ]] || {
            printf 'FAIL: damaged record changed Global AGENTS content.\n' >&2; exit 1;
        }
        printf '%s\n' "$recorded_hash" > "$global_txn/source-sha256.txt"
        printf 'external edit\n' > "$global_agents/AGENTS.md"
        if printf 'y\ny\n' | bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
            --global-agents --global-agents-install-dir "$global_agents" \
            --skills general --work-install-dir "$global_work" >/dev/null 2>&1; then
            printf 'FAIL: externally changed Global AGENTS was overwritten.\n' >&2; exit 1
        fi
        [[ "$(cat -- "$global_agents/AGENTS.md")" == 'external edit' &&
           "$(shasum -a 256 "$global_txn/global-previous" | cut -d ' ' -f1)" == "$old_global_hash" ]] || {
            printf 'FAIL: external Global AGENTS edit or previous bytes were lost.\n' >&2; exit 1;
        }
        cp -- "$root/AGENTS.md" "$global_agents/AGENTS.md"
        printf keep > "$global_txn/unknown.txt"
        if printf 'y\ny\n' | bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
            --global-agents --global-agents-install-dir "$global_agents" \
            --skills general --work-install-dir "$global_work" >/dev/null 2>&1; then
            printf 'FAIL: unknown Global transaction content was accepted.\n' >&2; exit 1
        fi
        [[ "$(cat -- "$global_txn/unknown.txt")" == keep &&
           "$(shasum -a 256 "$global_txn/global-previous" | cut -d ' ' -f1)" == "$old_global_hash" ]] || {
            printf 'FAIL: unknown Global transaction content was changed.\n' >&2; exit 1;
        }
        rm -- "$global_txn/unknown.txt"
    fi
    restart_output="$(printf 'y\ny\n' | bash "$repo_root/os-scripts/mac/installer/runtime/execute-locked.sh" package "$root" \
        --global-agents --global-agents-install-dir "$global_agents" \
        --skills general --work-install-dir "$global_work")" || {
        printf 'FAIL: %s combined recovery failed.\n' "$global_crash" >&2; exit 1;
    }
    if [[ "$global_crash" != after-work-verified ]]; then
        [[ "$restart_output" == *'Global AGENTS.md differs'* ]] || {
            printf 'FAIL: %s did not restore previous Global AGENTS before reinstall.\n' "$global_crash" >&2; exit 1;
        }
    fi
    [[ "$(cat -- "$global_work/install-state.json")" == *'"1.0.6"'* &&
       "$(shasum -a 256 "$global_agents/AGENTS.md" | cut -d ' ' -f1)" == "$new_global_hash" &&
       -z "$(find "$global_case" -maxdepth 1 \( -name '.agents-install.*' -o -name '.work-install.*' \) -print -quit)" ]] || {
        printf 'FAIL: %s left an incomplete combined transaction.\n' "$global_crash" >&2; exit 1;
    }
done
source_project="$scratch/source-project"
mkdir -p -- "$source_project/os-scripts" "$source_project/skills" "$source_project/rust"
cp -R -- "$repo_root/os-scripts/mac" "$source_project/os-scripts/mac"
cp -R -- "$repo_root/skills/work" "$source_project/skills/work"
cp -- "$repo_root/os-scripts/common/installer/options.tsv" "$source_project/os-scripts/mac/installer/cli/options.tsv"
mkdir -p -- "$source_project/os-scripts/common/installer"
cp -- "$repo_root/os-scripts/common/installer/install-state.json" "$source_project/os-scripts/common/installer/install-state.json"
cp -- "$root/AGENTS.md" "$source_project/AGENTS.md"
printf '1.0.5\n' > "$source_project/VERSION"
cat > "$scratch/shims/cargo" <<'CARGO'
#!/usr/bin/env bash
mkdir -p -- target/release
cp -- "$TEST_WORK_BINARY" target/release/work
CARGO
chmod 755 "$scratch/shims/cargo"
export TEST_WORK_BINARY="$root/bin/work"
source_entry="$source_project/os-scripts/mac/install-work-from-source.command"
source_work_target="$scratch/public-source/work"
source_global="$scratch/public-source/global"
bash "$source_entry" --non-interactive --global-agents --global-agents-install-dir "$source_global" \
    --skills general --work-install-dir "$source_work_target" >/dev/null
source_old_hash="$(shasum -a 256 "$source_global/AGENTS.md" | cut -d ' ' -f1)"
printf '1.0.6\n' > "$source_project/VERSION"
printf 'new public Source Global AGENTS\n' > "$source_project/AGENTS.md"
source_new_hash="$(shasum -a 256 "$source_project/AGENTS.md" | cut -d ' ' -f1)"
python3 -B - "$source_project/os-scripts/mac/installer/transaction/work.sh" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
marker = '    if ! installer_publish_work "$final" "$source" "$state_valid" "$confirmed" "$non_interactive" \\\n'
contents = path.read_text()
if contents.count(marker) != 1:
    raise SystemExit('public Source crash injection point is ambiguous')
path.write_text(contents.replace(marker, '    kill -9 $$\n' + marker))
PY
set +e
printf 'y\ny\n' | bash "$source_entry" --global-agents --global-agents-install-dir "$source_global" \
    --skills general --work-install-dir "$source_work_target" >/dev/null 2>&1
source_crash_exit=$?
set -e
source_txn="$(find "$scratch/public-source" -maxdepth 1 -name '.agents-install.*' -type d -print -quit)"
[[ "$source_crash_exit" == 137 && -n "$source_txn" &&
   "$(shasum -a 256 "$source_txn/global-previous" | cut -d ' ' -f1)" == "$source_old_hash" &&
   "$(shasum -a 256 "$source_global/AGENTS.md" | cut -d ' ' -f1)" == "$source_new_hash" &&
   "$(cat -- "$source_work_target/install-state.json")" == *'"1.0.5"'* ]] || {
    printf 'FAIL: public Source interruption lost the previous install.\n' >&2; exit 1;
}
cp -- "$repo_root/os-scripts/mac/installer/transaction/work.sh" \
    "$source_project/os-scripts/mac/installer/transaction/work.sh"
printf 'y\ny\n' | bash "$source_entry" --global-agents --global-agents-install-dir "$source_global" \
    --skills general --work-install-dir "$source_work_target" >/dev/null
[[ "$(cat -- "$source_work_target/install-state.json")" == *'"1.0.6"'* &&
   "$(shasum -a 256 "$source_global/AGENTS.md" | cut -d ' ' -f1)" == "$source_new_hash" &&
   -z "$(find "$scratch/public-source" -maxdepth 1 \( -name '.agents-install.*' -o -name '.work-install.*' \) -print -quit)" ]] || {
    printf 'FAIL: public Source restart did not finish the transaction.\n' >&2; exit 1;
}
printf 'macOS Shell installer core fixture passed.\n'
