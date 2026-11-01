#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../../../.." && pwd -P)"
source "$repo_root/os-scripts/mac/installer/target/preflight.sh"
installer_recognize_legacy_work() { return 0; } # Transaction fixtures use text in place of the CLI binary.
source "$repo_root/os-scripts/mac/installer/transaction/work.sh"
source "$repo_root/os-scripts/mac/installer/global-agents/policy.sh"
source "$repo_root/os-scripts/mac/installer/global-agents/transaction.sh"
source_work="$repo_root/skills/work"
scratch="$(mktemp -d "$repo_root/.issue95-transaction.XXXXXX")"
[[ "$(cd "$scratch" && pwd -P)" == "$scratch" && "$scratch" == "$repo_root"/.issue95-transaction.* ]] || exit 1
trap 'rm -rf -- "$scratch"' EXIT
target="$scratch/work"
fail() { printf 'FAIL: %s\n' "$1" >&2; exit 1; }
prepare_work() {
    local prepared="$1" relative
    for relative in SKILL.md agents/openai.yaml references/instruction-loading.md \
        references/instructions/task/general/instructions.md \
        references/instructions/execute/general/instructions.md; do
        mkdir -p -- "$prepared/$(dirname -- "$relative")"
        cp -- "$source_work/$relative" "$prepared/$relative"
    done
    mkdir -p -- "$prepared/scripts"
    printf new > "$prepared/scripts/work"
    printf '{"version":"1.1.0"}\n' > "$prepared/install-state.json"
}
validate_work() { [[ "$(cat -- "$1/scripts/work")" == new ]]; }
empty_target="$scratch/empty/work"
mkdir -p -- "$empty_target"
installer_publish_work "$empty_target" "$source_work" 0 0 1 prepare_work validate_work >/dev/null || fail 'empty Work install'
[[ -f "$empty_target/scripts/work" ]] || fail 'empty Work target'
empty_recovery="$scratch/empty-recovery/work"
mkdir -p -- "$empty_recovery"
if installer_publish_work "$empty_recovery" "$source_work" 0 0 1 prepare_work validate_work restore >/dev/null 2>&1; then
    fail 'empty Work restore failure accepted'
fi
installer_recover_interrupted_work "$empty_recovery" "$source_work" || fail 'empty Work retry restore'
[[ -d "$empty_recovery" && -z "$(find "$empty_recovery" -mindepth 1 -print -quit)" ]] || fail 'empty Work lost after restore'
installer_publish_work "$target" "$source_work" 0 0 0 prepare_work validate_work >/dev/null || fail 'first install'
[[ -f "$target/scripts/work" ]] || fail 'first install target'
printf old > "$target/scripts/work"
for failure in replace verify restore; do
    if installer_publish_work "$target" "$source_work" 0 1 0 prepare_work validate_work "$failure" >/dev/null 2>&1; then
        fail "injected $failure failure accepted"
    fi
    if [[ "$failure" == restore ]]; then
        installer_recover_interrupted_work "$target" "$source_work" || fail 'retry restore'
    fi
    [[ "$(cat -- "$target/scripts/work")" == old ]] || fail "old Work lost after $failure"
done
printf keep > "$target/unknown.txt"
if installer_publish_work "$target" "$source_work" 0 1 0 prepare_work validate_work >/dev/null 2>&1; then
    fail 'unknown content accepted'
fi
[[ "$(cat -- "$target/unknown.txt")" == keep ]] || fail 'unknown content changed'
rm -- "$target/unknown.txt"
installer_publish_work "$target" "$source_work" 0 1 0 prepare_work validate_work >/dev/null || fail 'upgrade'
[[ "$(cat -- "$target/scripts/work")" == new ]] || fail 'upgrade target'
for interrupted_point in staging first-rename second-rename verified; do
    printf old > "$target/scripts/work"
    txn="$(mktemp -d "$scratch/.work-install.XXXXXXXX")"
    printf '%s\n' "$target" > "$txn/target-path.txt"
    mkdir -- "$txn/prepared"
    prepare_work "$txn/prepared"
    state_hash="$(shasum -a 256 -- "$txn/prepared/install-state.json")"
    binary_hash="$(shasum -a 256 -- "$txn/prepared/scripts/work")"
    printf '%s\n' "${state_hash%% *}" > "$txn/state-sha256.txt"
    printf '%s\n' "${binary_hash%% *}" > "$txn/binary-sha256.txt"
    if [[ "$interrupted_point" != staging ]]; then mv -- "$target" "$txn/previous"; fi
    if [[ "$interrupted_point" == second-rename || "$interrupted_point" == verified ]]; then
        mv -- "$txn/prepared" "$target"
    fi
    if [[ "$interrupted_point" == verified ]]; then
        printf 'verified\n' > "$txn/verified.txt"
        installer_recover_interrupted_work "$target" "$source_work" || fail 'verified cleanup retry'
        [[ "$(cat -- "$target/scripts/work")" == new ]] || fail 'verified new Work replaced'
    else
        installer_publish_work "$target" "$source_work" 0 1 0 prepare_work validate_work >/dev/null || fail "retry after $interrupted_point"
        [[ "$(cat -- "$target/scripts/work")" == new ]] || fail "new Work missing after $interrupted_point"
    fi
    [[ ! -e "$txn" ]] || fail "transaction retained after $interrupted_point"
done
printf old > "$target/scripts/work"
installer_publish_work "$target" "$source_work" 0 1 0 prepare_work validate_work cleanup >/dev/null || fail 'cleanup failure marked install failed'
[[ "$(cat -- "$target/scripts/work")" == new ]] || fail 'verified new Work lost after cleanup failure'
installer_recover_interrupted_work "$target" "$source_work" || fail 'pending cleanup retry'
[[ "$(cat -- "$target/scripts/work")" == new ]] || fail 'pending cleanup replaced new Work'
global_source="$scratch/global-source.md"
global_target="$scratch/AGENTS.md"
printf 'new global\n' > "$global_source"
printf 'old global\r\n' > "$global_target"
installer_global_agents_plan "$global_source" "$global_target" 1 0 1
source_hash="$installer_global_agents_source_sha256"
target_hash="$installer_global_agents_target_sha256"
if installer_publish_combined_work "$target" "$source_work" 0 1 0 prepare_work validate_work \
    "$global_source" "$global_target" replace "$source_hash" "$target_hash" replace >/dev/null 2>&1; then
    fail 'combined Work failure accepted'
fi
[[ "$(installer_global_agents_hash "$global_target")" == "$target_hash" ]] || fail 'Global AGENTS changed after Work failure'
installer_publish_combined_work "$target" "$source_work" 0 1 0 prepare_work validate_work \
    "$global_source" "$global_target" replace "$source_hash" "$target_hash" >/dev/null || fail 'combined publish'
[[ "$(installer_global_agents_hash "$global_target")" == "$source_hash" ]] || fail 'combined Global AGENTS publish'
printf 'macOS Work transaction cases passed.\n'
