#!/usr/bin/env bash

installer_prepare_payload() {
    local prepared="$1" relative mode_item hierarchy
    for relative in SKILL.md agents/openai.yaml references/instruction-loading.md; do
        mkdir -p -- "$prepared/$(dirname -- "$relative")" || return 1
        cp -- "$source_work/$relative" "$prepared/$relative" || return 1
    done
    for relative in references/instruction-loading references/workflows references/subagents; do
        mkdir -p -- "$prepared/$relative" || return 1
        cp -R -- "$source_work/$relative/." "$prepared/$relative/" || return 1
    done
    for mode_item in task execute; do
        relative="references/instructions/$mode_item"
        mkdir -p -- "$prepared/$relative" || return 1
        for hierarchy in "$source_work/$relative/"*; do
            [[ -d "$hierarchy" ]] || continue
            cp -R -- "$hierarchy" "$prepared/$relative/" || return 1
        done
    done
    mkdir -p -- "$prepared/scripts" || return 1
    cp -- "$binary" "$prepared/scripts/work" || return 1
    chmod 755 "$prepared/scripts/work" || return 1
    printf '%s\n' "$state_json" > "$prepared/install-state.json" || return 1
}

installer_validate_payload() {
    local path="$1" validation
    [[ -x "$path/scripts/work" ]] || return 1
    "$path/scripts/work" --help >/dev/null 2>&1 || return 1
    validation="$(osascript -l JavaScript "$work_state_module" validate \
        "$(cat -- "$path/install-state.json")" "$target")" || return 1
    [[ "$validation" == *'"valid":true'* ]]
}
