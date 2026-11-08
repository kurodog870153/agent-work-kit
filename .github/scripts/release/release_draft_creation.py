"""Create or refresh an unpublished release draft with validated assets."""

import argparse
import subprocess
import sys
from pathlib import Path

from release_publication_checks import api, check_assets, expected_assets


def prepare_draft(repository, tag, expected_sha, directory, title_file, notes_file):
    ref = api(f"repos/{repository}/git/ref/tags/{tag}")
    if ref["object"]["sha"] != expected_sha:
        raise ValueError("release tag points to a different commit")
    pages = api(f"repos/{repository}/releases?per_page=100", "--paginate", "--slurp")
    matches = [release for page in pages for release in page if release["tag_name"] == tag]
    if len(matches) > 1 or (matches and not matches[0]["draft"]):
        raise ValueError("release is published or has ambiguous state")
    assets = sorted(directory.glob("*.zip")) + sorted(directory.glob("*.command")) + sorted(directory.glob("*.bat")) + [directory / "SHA256SUMS.txt"]
    if {path.name for path in assets} != expected_assets(tag.removeprefix("v")) or not all(path.is_file() for path in assets):
        raise ValueError("release assets are missing or unexpected")
    title = title_file.read_text(encoding="utf-8").strip()
    notes = notes_file.read_text(encoding="utf-8")
    if not title:
        raise ValueError("release title is empty")
    if matches:
        draft = matches[0]
        if (draft.get("name") != title or (draft.get("body") or "").replace("\r\n", "\n") != notes.replace("\r\n", "\n")
                or not {asset["name"] for asset in draft["assets"]}.issubset(expected_assets(tag.removeprefix("v")))):
            raise ValueError("existing draft content or assets differ from validated release")
    base = ["gh", "release"]
    if matches:
        subprocess.run(base + ["upload", tag, *(str(path) for path in assets), "--repo", repository, "--clobber"], check=True)
    else:
        subprocess.run(base + ["create", tag, *(str(path) for path in assets), "--repo", repository, "--draft", "--verify-tag",
                               "--title", title, "--notes-file", str(notes_file)], check=True)
    check_assets(repository, tag, directory, published=False,
                 expected_title=title, expected_notes=notes)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--expected-sha", required=True)
    parser.add_argument("--directory", type=Path, required=True)
    parser.add_argument("--title-file", type=Path, required=True)
    parser.add_argument("--notes-file", type=Path, required=True)
    args = parser.parse_args()
    try:
        prepare_draft(args.repository, args.tag, args.expected_sha, args.directory, args.title_file, args.notes_file)
    except (KeyError, OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Release draft creation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
