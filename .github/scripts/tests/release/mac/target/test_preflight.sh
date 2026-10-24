#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
source "$repo_root/os-scripts/mac/installer/target/preflight.sh"
source_work="$repo_root/skills/work"
binary="$repo_root/rust/target/release/work"
[[ -x "$binary" ]] || { printf 'native Work binary is required for preflight fixture\n' >&2; exit 1; }
scratch="$(mktemp -d "$repo_root/.issue95-preflight.XXXXXX")"
[[ "$(cd "$scratch" && pwd -P)" == "$scratch" && "$scratch" == "$repo_root"/.issue95-preflight.* ]] || exit 1
trap 'rm -rf -- "$scratch"' EXIT
target="$scratch/work"
fail() { printf 'FAIL: %s\n' "$1" >&2; exit 1; }
installer_inspect_target "$target" "$source_work" 0
[[ "$installer_target_kind" == new ]] || fail 'missing target'
mkdir "$target"
installer_inspect_target "$target" "$source_work" 0
[[ "$installer_target_kind" == new ]] || fail 'empty target'
for relative in SKILL.md agents/openai.yaml references/instruction-loading.md \
    references/instruction-loading/invocation.md \
    references/instructions/task/general/instructions.md \
    references/instructions/execute/general/instructions.md; do
    mkdir -p -- "$target/$(dirname "$relative")"
    cp -- "$source_work/$relative" "$target/$relative"
done
mkdir -p "$target/scripts"
printf 'fixture' > "$target/scripts/work"
installer_inspect_target "$target" "$source_work" 0
[[ "$installer_target_kind" == unsafe ]] || fail 'text file accepted as Work binary'
cat > "$target/scripts/work" <<'WORK'
#!/usr/bin/env bash
printf '%s\n' '{"schema":"work-cli-result","status":"success","data":{"help":"usage: work"}}'
WORK
chmod 755 "$target/scripts/work"
installer_inspect_target "$target" "$source_work" 0
[[ "$installer_target_kind" == unsafe ]] || fail 'shell script spoofed Work binary'
cp -- "$binary" "$target/scripts/work"
chmod 755 "$target/scripts/work"
installer_inspect_target "$target" "$source_work" 0
[[ "$installer_target_kind" == first-update ]] || fail 'safe first update'
if installer_target_allows_install 1 1; then fail 'non-interactive takeover'; fi
if installer_target_allows_install 0 0; then fail 'unconfirmed takeover'; fi
installer_target_allows_install 0 1 || fail 'confirmed takeover'
installer_inspect_target "$target" "$source_work" 1
[[ "$installer_target_kind" == managed ]] || fail 'valid state'
printf 'unknown skill\n' > "$target/SKILL.md"
installer_inspect_target "$target" "$source_work" 0
[[ "$installer_target_kind" == unsafe ]] || fail 'unrecognized managed content'
cp -- "$source_work/SKILL.md" "$target/SKILL.md"
find() { printf '%s\0' "$target/SKILL.md"; return 1; }
installer_inspect_target "$target" "$source_work" 0
[[ "$installer_target_kind" == unsafe ]] || fail 'incomplete directory scan'
unset -f find
printf 'user data' > "$target/unknown.txt"
installer_inspect_target "$target" "$source_work" 0
[[ "$installer_target_kind" == unsafe ]] || fail 'unknown file'
rm -- "$target/unknown.txt"
ln -s -- "$source_work/SKILL.md" "$target/linked"
installer_inspect_target "$target" "$source_work" 0
[[ "$installer_target_kind" == unsafe ]] || fail 'unsafe symlink'
rm -- "$target/linked"
rm -- "$target/references/instructions/execute/general/instructions.md"
installer_inspect_target "$target" "$source_work" 0
[[ "$installer_target_kind" == unsafe ]] || fail 'missing required file'
printf 'macOS target preflight cases passed.\n'
