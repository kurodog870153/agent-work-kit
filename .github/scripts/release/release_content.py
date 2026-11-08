"""Resolve release prose from committed metadata and merged pull requests."""

import re
import argparse
import json
import subprocess
import sys
from pathlib import Path


VERSION = re.compile(r"v?\d+\.\d+\.\d+(?:[-+][\w.-]+)?", re.I)
HEADING = re.compile(r"^## Release (Title|Description)\s*$", re.I | re.M)


def valid_title(value):
    if not isinstance(value, str) or not value.strip() or "\n" in value or "\r" in value:
        raise ValueError("Release Title must be one nonempty line")
    title = value.strip()
    if (VERSION.search(title)
            or re.match(r"^(merge pull request|merge branch|release|changes|update|chore|version)\b", title, re.I)):
        raise ValueError(f"Release Title is not a meaningful feature summary: {title}")
    return title


def valid_description(value):
    if not isinstance(value, str) or not value.strip():
        raise ValueError("Release Description is empty")
    description = value.strip()
    if not re.search(r"[A-Za-z\u4e00-\u9fff]", description) or VERSION.fullmatch(description):
        raise ValueError("Release Description has no meaningful summary")
    if re.search(r"^## (What's Changed|Commit History|Full Changelog)\b", description, re.I | re.M):
        raise ValueError("Release Description must contain summary prose, not generated notes or commit history")
    return description


def metadata(root, tag):
    path = root / ".github/release-metadata" / f"{tag}.md"
    if not path.exists():
        return {}, None
    raw = path.read_text(encoding="utf-8")
    match = re.fullmatch(r"---\n(?:title: ([^\n]*)\n)?---\n(?:\n(.*))?", raw, re.S)
    if not match:
        raise ValueError(f"invalid release metadata format: {path}")
    fields = {}
    if match.group(1) is not None:
        fields["title"] = valid_title(match.group(1))
    if match.group(2) is not None:
        fields["description"] = valid_description(match.group(2))
    if set(fields) != {"title", "description"}:
        raise ValueError(f"release metadata requires both Title and Description: {path}")
    return fields, str(path.relative_to(root))


def declarations(body):
    body = body or ""
    found = list(HEADING.finditer(body))
    result = {}
    for index, match in enumerate(found):
        key = match.group(1).lower()
        if key in result:
            raise ValueError(f"duplicate PR Release {key} declaration")
        value = body[match.end():found[index + 1].start() if index + 1 < len(found) else len(body)].strip()
        result[key] = valid_title(value) if key == "title" else valid_description(value)
    return result


def resolve(root, tag, pulls):
    chosen, path = metadata(root, tag)
    sources = {key: path for key in chosen}
    if path:
        return chosen, sources
    explicit = {"title": [], "description": []}
    for pull in pulls:
        for key, value in declarations(pull.get("body")).items():
            explicit[key].append((value, f"PR #{pull['number']}"))
    for key in ("title", "description"):
        if key in chosen:
            continue
        values = explicit[key]
        if values:
            if len({value for value, _ in values}) != 1:
                raise ValueError(f"conflicting PR Release {key} declarations: {', '.join(source for _, source in values)}")
            chosen[key], sources[key] = values[0]
            continue
        if len(pulls) != 1:
            raise ValueError(f"Release {key} needs metadata or an unambiguous merged PR; found {len(pulls)} PRs")
        pull = pulls[0]
        if key == "title":
            chosen[key] = valid_title(re.sub(r"^(feat|fix|refactor)(\([^)]+\))?:\s*", "", pull["title"], flags=re.I))
        else:
            body = HEADING.split(pull.get("body") or "", 1)[0].strip()
            chosen[key] = valid_description(body if body else chosen.get("title", pull["title"]))
        sources[key] = f"PR #{pull['number']} fallback"
    return chosen, sources


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[3])
    parser.add_argument("--repository", required=True)
    parser.add_argument("--pr-number", type=int, required=True)
    args = parser.parse_args()
    try:
        tag = "v" + (args.root / "VERSION").read_text(encoding="utf-8").strip()
        metadata(args.root, tag)
        result = subprocess.run(["gh", "api", f"repos/{args.repository}/pulls/{args.pr_number}"],
                                check=True, capture_output=True, text=True)
        declarations(json.loads(result.stdout).get("body"))
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"PR release content check failed: {error}", file=sys.stderr)
        return 1
    print(f"PR release content format is valid for {tag}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
