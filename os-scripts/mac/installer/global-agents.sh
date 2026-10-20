#!/usr/bin/env bash

# The caller supplies global_agents_source. This module has no toolchain dependency.
install_global_agents() {
    local answer target_directory target_file transaction_directory staged

    if ! read -r -p 'Install Global AGENTS.md? [y/N] ' answer; then
        printf 'Error: installation input ended before a selection was completed.\n' >&2
        return 1
    fi
    if [[ ! "$answer" =~ ^[Yy]$ ]]; then
        printf 'Global AGENTS.md: Skipped.\n'
        return 0
    fi
    if [[ ! -f "$global_agents_source" ]]; then
        printf 'Error: Global AGENTS.md source not found: "%s".\n' "$global_agents_source" >&2
        return 1
    fi

    while true; do
        printf 'Global AGENTS.md installation location:\n'
        printf '  1. Default Codex directory: "%s/.codex"\n' "$HOME"
        printf '  2. Custom directory\n'
        if ! read -r -p 'Select an installation location [1]: ' answer; then
            printf 'Error: installation input ended before a selection was completed.\n' >&2
            return 1
        fi
        case "${answer:-1}" in
            1) target_directory="$HOME/.codex"; break ;;
            2)
                if ! read -r -p 'Enter the target directory: ' target_directory; then
                    printf 'Error: installation input ended before a selection was completed.\n' >&2
                    return 1
                fi
                if [[ -z "$target_directory" ]]; then
                    printf 'The directory cannot be empty. Please try again.\n'
                    continue
                fi
                if [[ "$target_directory" == '~' ]]; then
                    target_directory="$HOME"
                elif [[ "$target_directory" == '~/'* ]]; then
                    target_directory="$HOME/${target_directory#~/}"
                fi
                break ;;
            *) printf 'Invalid selection. Please try again.\n' ;;
        esac
    done

    if [[ ! -d "$target_directory" ]]; then
        if ! read -r -p 'The directory does not exist. Create it? [y/N] ' answer; then
            printf 'Error: installation input ended before a selection was completed.\n' >&2
            return 1
        fi
        if [[ ! "$answer" =~ ^[Yy]$ ]]; then
            printf 'Global AGENTS.md: Skipped.\n'
            return 0
        fi
        if ! mkdir -p -- "$target_directory"; then
            printf 'Error: failed to create Global AGENTS.md directory: "%s".\n' "$target_directory" >&2
            return 1
        fi
    fi
    target_directory="$(cd -- "$target_directory" && pwd -P)" || return 1
    target_file="$target_directory/AGENTS.md"

    if [[ -e "$target_file" || -L "$target_file" ]]; then
        if [[ ! -f "$target_file" || -L "$target_file" ]]; then
            printf 'Error: Global AGENTS.md target is not a regular file: "%s".\n' "$target_file" >&2
            return 1
        fi
        if ! read -r -p 'AGENTS.md already exists. Overwrite it? [y/N] ' answer; then
            printf 'Error: installation input ended before a selection was completed.\n' >&2
            return 1
        fi
        if [[ ! "$answer" =~ ^[Yy]$ ]]; then
            printf 'Global AGENTS.md: Skipped.\n'
            return 0
        fi
    fi

    transaction_directory="$(mktemp -d "$target_directory/.agents-install.XXXXXX")" || return 1
    staged="$transaction_directory/prepared"
    if ! cp -- "$global_agents_source" "$staged" || ! chmod 644 "$staged" || ! cmp -s -- "$global_agents_source" "$staged"; then
        printf 'Error: failed to prepare Global AGENTS.md; target was not changed. Recovery directory: "%s".\n' "$transaction_directory" >&2
        return 1
    fi
    if [[ -e "$target_file" ]]; then
        if ! mv -- "$target_file" "$transaction_directory/previous"; then
            printf 'Error: failed to back up Global AGENTS.md; target was not changed.\n' >&2
            return 1
        fi
    fi
    if ! mv -- "$staged" "$target_file"; then
        if [[ -e "$transaction_directory/previous" ]] && ! mv -- "$transaction_directory/previous" "$target_file"; then
            printf 'Error: Global AGENTS.md restore failed; previous file remains in "%s/previous".\n' "$transaction_directory" >&2
            return 1
        fi
        printf 'Error: Global AGENTS.md replacement failed; previous file was restored. Recovery directory: "%s".\n' "$transaction_directory" >&2
        return 1
    fi
    printf 'Global AGENTS.md installed in "%s". Recovery directory: "%s".\n' "$target_file" "$transaction_directory"
}
