#!/usr/bin/env bash
set -uo pipefail
export LC_ALL=C

release_tag=__WORK_RELEASE_TAG__
release_base="https://github.com/kurodog870153/agent-work-kit/releases/download/$release_tag"
checksum=SHA256SUMS.txt

if (( $# == 1 )) && [[ "$1" == --help || "$1" == -h ]]; then
    cat <<'USAGE'
Usage: install-agent-work-kit-from-github-release.command [installer options]
Downloads the fixed macOS ARM64 Release, verifies SHA-256, then runs its Package installer.
Use --help on the Package installer for the complete option list.
USAGE
    exit 0
fi

if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
    printf 'Error: this downloader requires macOS ARM64.\n' >&2
    exit 1
fi
selected_host=''
non_interactive=0
forward_args=("$@")
for ((index=0; index<$#; index++)); do
    argument="${forward_args[index]}"
    if [[ "$argument" == --non-interactive ]]; then non_interactive=1; fi
    if [[ "$argument" == --host ]]; then
        [[ -z "$selected_host" && $((index + 1)) -lt $# ]] || { printf 'Error: invalid --host.\n' >&2; exit 2; }
        selected_host="${forward_args[index+1]}"
        [[ "$selected_host" == codex || "$selected_host" == claude ]] || { printf 'Error: invalid --host.\n' >&2; exit 2; }
    fi
done
if [[ -z "$selected_host" ]]; then
    [[ "$non_interactive" != 1 ]] || { printf 'Error: --host is required in non-interactive mode.\n' >&2; exit 2; }
    read -r -p 'Select Host: 1) Codex, 2) Claude Code [] ' answer || exit 1
    case "$answer" in
        1) selected_host=codex ;;
        2) selected_host=claude ;;
        *) printf 'Error: Host selection is required.\n' >&2; exit 2 ;;
    esac
    forward_args+=(--host "$selected_host")
fi
archive="agent-work-kit-$release_tag-$selected_host-macos-arm64.zip"
verbose=0
for argument in "$@"; do [[ "$argument" == --verbose ]] && verbose=1; done
release_log() { (( verbose )) && printf 'Downloader: %s\n' "$1" || true; }
curl_protocol='=https'

scratch_root="${TMPDIR:-/tmp}"
release_cleanup_stale() {
    local entry marker kind pid name lsof_output status
    [[ -x /usr/sbin/lsof ]] || return 0
    for entry in "$scratch_root"/work-release.*; do
        [[ -d "$entry" && ! -L "$entry" && "${entry##*/}" =~ ^work-release\.[A-Za-z0-9]{8}$ ]] || continue
        marker="$entry/.work-release-owner"
        [[ -f "$marker" && ! -L "$marker" ]] || continue
        read -r kind pid < "$marker" || continue
        [[ "$kind" == work-release-v1 && "$pid" =~ ^[0-9]+$ ]] || continue
        kill -0 "$pid" 2>/dev/null && continue
        [[ -z "$(find "$entry" -type l -print -quit)" ]] || continue
        for name in "$entry"/* "$entry"/.[!.]*; do
            [[ -e "$name" || -L "$name" ]] || continue
            case "${name##*/}" in
                .work-release-owner|SHA256SUMS.txt|package|agent-work-kit-v*-codex-macos-arm64.zip|agent-work-kit-v*-claude-macos-arm64.zip) ;;
                *) printf 'Warning: unknown Downloader temporary entry retained: %s\n' "$entry" >&2; continue 2 ;;
            esac
        done
        lsof_output="$(/usr/sbin/lsof -t +D "$entry" 2>&1)"
        status=$?
        [[ "$status" == 1 && -z "$lsof_output" ]] || continue
        if rm -rf -- "$entry"; then release_log 'removed unused Downloader temporary directory'
        else printf 'Warning: Downloader temporary directory awaits cleanup: %s\n' "$entry" >&2; fi
    done
}
release_cleanup_stale
scratch="$(mktemp -d "$scratch_root/work-release.XXXXXXXX")" || exit 1
[[ -d "$scratch" && ! -L "$scratch" && "${scratch##*/}" == work-release.* ]] || exit 1
printf 'work-release-v1 %s\n' "$$" > "$scratch/.work-release-owner" || exit 1
trap 'rm -rf -- "$scratch"' EXIT

download() {
    local name="$1" url="$release_base/$1"
    release_log "downloading $name from fixed Release $release_tag"
    if /usr/bin/curl --fail --location --silent --show-error \
        --proto "$curl_protocol" --proto-redir "$curl_protocol" \
        --connect-timeout 10 --max-time 120 --output "$scratch/$name" "$url"; then
        release_log "downloaded $name"
        return 0
    fi
    rm -f -- "$scratch/$name"
    printf 'Error: download failed: %s\n' "$name" >&2
    return 1
}

download "$checksum" || exit 1
download "$archive" || exit 1
expected=''
while IFS= read -r line || [[ -n "$line" ]]; do
    if [[ "$line" =~ ^([0-9a-f]{64})\ \ ([A-Za-z0-9._-]+)$ ]]; then
        if [[ "${BASH_REMATCH[2]}" == "$archive" ]]; then
            [[ -z "$expected" ]] || { printf 'Error: duplicate ZIP checksum.\n' >&2; exit 1; }
            expected="${BASH_REMATCH[1]}"
        fi
    else
        printf 'Error: invalid outer checksum file.\n' >&2
        exit 1
    fi
done < "$scratch/$checksum"
[[ -n "$expected" ]] || { printf 'Error: ZIP checksum is missing.\n' >&2; exit 1; }
actual="$(/usr/bin/openssl dgst -sha256 "$scratch/$archive" | /usr/bin/awk '{print $NF}')" || exit 1
[[ "$actual" == "$expected" ]] || { printf 'Error: ZIP SHA-256 mismatch.\n' >&2; exit 1; }
release_log 'outer ZIP SHA-256 verified'

listing="$(/usr/bin/unzip -Z -1 "$scratch/$archive")" || { printf 'Error: invalid ZIP.\n' >&2; exit 1; }
[[ -n "$listing" ]] || { printf 'Error: empty ZIP.\n' >&2; exit 1; }
members=0
while IFS= read -r name; do
    [[ "$name" =~ ^[A-Za-z0-9._/-]+$ && "$name" != /* && "$name" != */ &&
       "$name" != . && "$name" != .. && "$name" != *//* &&
       "$name" != ../* && "$name" != */../* && "$name" != */.. &&
       "$name" != ./* && "$name" != */./* && "$name" != */. ]] || {
        printf 'Error: unsafe ZIP member: %s\n' "$name" >&2; exit 1;
    }
    members=$((members + 1))
done <<< "$listing"
if [[ "$(printf '%s\n' "$listing" | /usr/bin/tr '[:upper:]' '[:lower:]' | /usr/bin/sort | /usr/bin/uniq -d)" != '' ]]; then
    printf 'Error: duplicate ZIP member.\n' >&2
    exit 1
fi
attributes="$(/usr/bin/unzip -Z -v "$scratch/$archive")" || exit 1
regular="$(printf '%s\n' "$attributes" | /usr/bin/grep -Ec 'Unix file attributes \(10[0-7]{4} octal\)' || true)"
[[ "$regular" == "$members" ]] || { printf 'Error: ZIP contains a non-regular member.\n' >&2; exit 1; }
release_log 'ZIP members and types verified'

mkdir -- "$scratch/package" || exit 1
/usr/bin/unzip -qq -n "$scratch/$archive" -d "$scratch/package" || exit 1
entry="$scratch/package/install-agent-work-kit-from-package.command"
[[ -f "$entry" && ! -L "$entry" ]] || { printf 'Error: Package entry is missing.\n' >&2; exit 1; }
info="$scratch/package/release-info.json"
[[ -f "$info" && ! -L "$info" ]] || { printf 'Error: Package release metadata is missing.\n' >&2; exit 1; }
package_identity="$(/usr/bin/osascript -l JavaScript -e 'ObjC.import("Foundation"); function run(argv) { try { var info = JSON.parse(ObjC.unwrap($.NSString.stringWithContentsOfFileEncodingError(argv[0], $.NSUTF8StringEncoding, null))); return [info.tag || "", info.host || "", info.target || ""].join("\t"); } catch (_) { return ""; } }' "$info" 2>/dev/null)" || exit 1
IFS=$'\t' read -r package_tag package_host package_target <<< "$package_identity"
[[ "$package_tag" == "$release_tag" && "$package_host" == "$selected_host" && "$package_target" == aarch64-apple-darwin ]] || {
    printf 'Error: Package Release identity does not match Downloader.\n' >&2; exit 1;
}
release_log 'starting verified Package installer'
/bin/bash "$entry" "${forward_args[@]}"
exit $?
