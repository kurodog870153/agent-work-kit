"""Build loopback-only Downloader scripts outside the Release asset directory."""

from release_package import build_downloaders


def build_test_downloaders(repository, output):
    paths = build_downloaders(repository, output)
    for path in paths:
        data = path.read_bytes()
        if path.suffix == ".command":
            anchor = b"curl_protocol='=https'\n"
            injection = b"""if [[ "${WORK_RELEASE_TEST_MODE:-}" == 1 && "${WORK_RELEASE_TEST_BASE_URL:-}" == http://127.0.0.1:* ]]; then
    release_base="${WORK_RELEASE_TEST_BASE_URL%/}/$release_tag"
    curl_protocol='=http,https'
fi
"""
        else:
            anchor = b'$base = "https://github.com/kurodog870153/agent-work-kit/releases/download/$releaseTag"\r\n'
            injection = rb"""if ($env:WORK_RELEASE_TEST_MODE -eq '1' -and $env:WORK_RELEASE_TEST_BASE_URL -match '^http://127\.0\.0\.1:[0-9]+$') {
    $base = $env:WORK_RELEASE_TEST_BASE_URL.TrimEnd('/') + '/' + $releaseTag
}
""".replace(b"\n", b"\r\n")
        if data.count(anchor) != 1:
            raise ValueError(f"Downloader fixture anchor missing: {path}")
        path.write_bytes(data.replace(anchor, anchor + injection))
    return paths
