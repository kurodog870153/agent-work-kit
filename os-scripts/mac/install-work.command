#!/usr/bin/env bash

set -u

if (( $# != 0 )); then
    printf 'Error: this installer no longer accepts plan, task, execute, or all arguments.\n' >&2
    exit 2
fi

script_directory="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
project_directory="$(cd -- "$script_directory/../.." && pwd -P)"
source_work="$project_directory/skills/work"
missing_source=""
global_agents_source="$project_directory/AGENTS.md"

if [[ ! -f "$script_directory/installer/global-agents.sh" ]]; then
    printf 'Error: Global AGENTS.md installer module is missing.\n' >&2
    exit 1
fi
source "$script_directory/installer/global-agents.sh"

validate_rust_toolchain() {
    local tool version major minor toolchain_bin
    if ! command -v rustc >/dev/null 2>&1 || ! command -v cargo >/dev/null 2>&1; then
        for toolchain_bin in "$HOME/.cargo/bin" /opt/homebrew/opt/rustup/bin /usr/local/opt/rustup/bin; do
            if [[ -x "$toolchain_bin/rustc" && -x "$toolchain_bin/cargo" ]]; then
                PATH="$toolchain_bin:$PATH"
                export PATH
                break
            fi
        done
    fi
    for tool in rustc cargo; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            printf 'Error: %s is required. Install Rust 1.85 or newer with Cargo and try again.\n' "$tool" >&2
            return 1
        fi
        version="$($tool --version)" || return 1
        if [[ ! "$version" =~ ^(rustc|cargo)[[:space:]]+([0-9]+)\.([0-9]+)\. ]]; then
            printf 'Error: cannot read %s version: %s\n' "$tool" "$version" >&2
            return 1
        fi
        major="${BASH_REMATCH[2]}"
        minor="${BASH_REMATCH[3]}"
        if (( major < 1 || (major == 1 && minor < 85) )); then
            printf 'Error: %s 1.85 or newer is required; found %s.\n' "$tool" "$version" >&2
            return 1
        fi
    done
    if ! command -v xcrun >/dev/null 2>&1 || ! xcrun --find clang >/dev/null 2>&1; then
        printf 'Error: Xcode Command Line Tools with clang are required. Install them and try again.\n' >&2
        return 1
    fi
    local sdk
    sdk="$(xcrun --show-sdk-path)" || return 1
    if [[ ! -d "$sdk" ]]; then
        printf 'Error: the macOS SDK is unavailable: %s\n' "$sdk" >&2
        return 1
    fi
}

build_work() {
    if ! (cd -- "$project_directory/rust" && cargo build --release --locked -p work-cli); then
        printf 'Error: Rust build failed. Check linker/SDK setup and crate downloads; the installed Work binary was not changed.\n' >&2
        return 1
    fi
    built_work="$project_directory/rust/target/release/work"
    if [[ ! -x "$built_work" ]] || ! "$built_work" --help >/dev/null 2>&1; then
        printf 'Error: the built Work binary failed its startup check; the installed binary was not changed.\n' >&2
        return 1
    fi
}

if ! install_global_agents; then
    exit 1
fi

if ! validate_rust_toolchain; then
    exit 1
fi

if [[ ! -f "$script_directory/installer/work-core.sh" ]]; then
    printf 'Error: Work installer core is missing.\n' >&2
    exit 1
fi
source "$script_directory/installer/work-core.sh"
run_work_install
