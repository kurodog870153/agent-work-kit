#!/usr/bin/env bash

# Read-only inspection. The caller supplies a validated state flag and handles confirmation.
installer_target_reject() {
    installer_target_kind=unsafe
    installer_target_reason="$1"
}

installer_managed_relative() {
    case "$1" in
        SKILL.md|agents|agents/openai.yaml|references|references/instruction-loading.md|\
        references/instruction-loading|references/instruction-loading/*|\
        references/workflows|references/workflows/*|references/subagents|references/subagents/*|\
        references/instructions|references/instructions/task|references/instructions/task/*|\
        references/instructions/execute|references/instructions/execute/*|\
        scripts|scripts/work|install-state.json) return 0 ;;
        *) return 1 ;;
    esac
}

installer_retired_relative() {
    case "$1" in
        references/instruction-loading/handoff.md|references/instruction-loading/internal-envelope.md|\
        references/instruction-loading/invocation.md|references/instruction-loading/private-roles.md|\
        references/subagents|references/subagents/artifact-editor.md|references/subagents/execute.md|\
        references/subagents/task-coordinator.md|references/subagents/task-skill.md|\
        references/workflows/execute/use-deterministic-handoffs.md|\
        references/workflows/task/use-deterministic-handoffs.md) return 0 ;;
        *) return 1 ;;
    esac
}

installer_recognize_legacy_work() {
    local target="$1" response
    [[ -x "$target/scripts/work" ]] || return 1
    [[ "$(file -b -- "$target/scripts/work")" == *Mach-O*arm64* ]] || return 1
    response="$("$target/scripts/work" --help 2>/dev/null)" || return 1
    osascript -l JavaScript -e 'function run(argv) { try { var result = JSON.parse(argv[0]); return result.schema === "work-cli-result" && result.status === "success" && typeof result.data.help === "string" && /^usage: work\b/i.test(result.data.help) ? "valid" : "invalid"; } catch (_) { return "invalid"; } }' "$response" 2>/dev/null | grep -qx valid || return 1
    grep -q '^name: work$' "$target/SKILL.md" || return 1
    grep -q '^  display_name: "Work"$' "$target/agents/openai.yaml" || return 1
    grep -q '^# Work Instruction Loading Bootstrap$' "$target/references/instruction-loading.md" || return 1
    if [[ -f "$target/references/instruction-loading/invocation.md" ]]; then
        grep -q '^# Invocation contract$' "$target/references/instruction-loading/invocation.md" || return 1
    else
        grep -q '^# Work Instruction Loading Bootstrap$' "$target/references/instruction-loading.md" || return 1
    fi
    grep -Eq '^# (任務規劃指令|Task planning)$' "$target/references/instructions/task/general/instructions.md" || return 1
    grep -Eq '^# (任務執行指令|Task execution)$' "$target/references/instructions/execute/general/instructions.md" || return 1
}

installer_inspect_target() {
    local target="$1" source="$2" state_valid="$3" probe entry relative listing
    local -a entries=()
    installer_target_kind=unsafe
    installer_target_reason=''
    [[ "$target" == /* && "$target" != / ]] || { installer_target_reject 'target is not an absolute Work path'; return 0; }
    probe="$target"
    while [[ "$probe" != / ]]; do
        if [[ -L "$probe" ]]; then installer_target_reject "symbolic link in target path: $probe"; return 0; fi
        probe="$(dirname -- "$probe")"
    done
    if [[ ! -e "$target" ]]; then installer_target_kind=new; return 0; fi
    if [[ ! -d "$target" ]]; then installer_target_reject 'target is not a directory'; return 0; fi
    listing="$(mktemp "${TMPDIR:-/tmp}/work-preflight.XXXXXXXX")" || { installer_target_reject 'cannot inspect Work directory'; return 0; }
    if ! find "$target" -mindepth 1 -print0 > "$listing"; then
        rm -f -- "$listing"
        installer_target_reject 'cannot fully enumerate Work directory'
        return 0
    fi
    while IFS= read -r -d '' entry; do entries+=("$entry"); done < "$listing"
    rm -f -- "$listing"
    if (( ${#entries[@]} == 0 )); then installer_target_kind=new; return 0; fi
    for entry in "${entries[@]}"; do
        relative="${entry#"$target"/}"
        if [[ -L "$entry" || ! -f "$entry" && ! -d "$entry" ]]; then
            installer_target_reject "unsafe entry: $relative"; return 0
        fi
        if ! installer_managed_relative "$relative"; then
            installer_target_reject "unknown entry: $relative"; return 0
        fi
        if [[ "$relative" != scripts && "$relative" != scripts/work && "$relative" != install-state.json &&
              ! -e "$source/$relative" ]] && ! installer_retired_relative "$relative"; then
            installer_target_reject "unrecognized Work entry: $relative"; return 0
        fi
    done
    for relative in SKILL.md agents/openai.yaml references/instruction-loading.md \
        references/instructions/task/general/instructions.md \
        references/instructions/execute/general/instructions.md scripts/work; do
        if [[ ! -f "$target/$relative" ]]; then
            installer_target_reject "required Work file is missing: $relative"; return 0
        fi
    done
    if [[ "$state_valid" != 1 ]] && ! installer_recognize_legacy_work "$target"; then
        installer_target_reject 'existing Work binary or managed content is not recognizable'
        return 0
    fi
    if [[ "$state_valid" == 1 ]]; then
        installer_target_kind=managed
    else
        installer_target_kind=first-update
    fi
}

installer_target_allows_install() {
    local non_interactive="$1" confirmed="$2"
    case "$installer_target_kind" in
        new|managed) return 0 ;;
        first-update) [[ "$non_interactive" != 1 && "$confirmed" == 1 ]] ;;
        *) return 1 ;;
    esac
}
