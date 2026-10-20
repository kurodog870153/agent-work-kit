"""Create and validate a release package's internal SHA256SUMS manifest."""

import argparse
import hashlib
import os
import re
import stat
import sys
import zipfile
from pathlib import Path


MANIFEST = "SHA256SUMS"
SAFE_PATH = re.compile(r"[A-Za-z0-9._/-]+\Z")
HASH_LINE = re.compile(r"([0-9a-f]{64})  ([A-Za-z0-9._/-]+)\Z")


def safe_path(name):
    if (
        not SAFE_PATH.fullmatch(name)
        or name.startswith("/")
        or name.endswith("/")
        or any(part in ("", ".", "..") for part in name.split("/"))
    ):
        raise ValueError(f"unsafe release path: {name!r}")
    return name


def file_hash(stream):
    digest = hashlib.sha256()
    for chunk in iter(lambda: stream.read(1024 * 1024), b""):
        digest.update(chunk)
    return digest.hexdigest()


def read_manifest(data):
    if data.startswith(b"\xef\xbb\xbf") or b"\r" in data or not data.endswith(b"\n"):
        raise ValueError("SHA256SUMS must be UTF-8 without BOM and use LF lines")
    try:
        lines = data.decode("utf-8", errors="strict").splitlines()
    except UnicodeDecodeError as error:
        raise ValueError("SHA256SUMS is not UTF-8") from error
    if not lines:
        raise ValueError("SHA256SUMS is empty")
    entries = {}
    folded = set()
    for line in lines:
        match = HASH_LINE.fullmatch(line)
        if not match:
            raise ValueError(f"invalid SHA256SUMS entry: {line!r}")
        name = safe_path(match.group(2))
        if name == MANIFEST or name.casefold() in folded:
            raise ValueError(f"duplicate or self-referential SHA256SUMS path: {name}")
        folded.add(name.casefold())
        entries[name] = match.group(1)
    if list(entries) != sorted(entries):
        raise ValueError("SHA256SUMS entries must be sorted")
    return entries


def tree_files(root):
    if root.is_symlink() or not root.is_dir():
        raise ValueError("package root must be a real directory")
    files = {}
    folded = set()
    for parent, directories, filenames in os.walk(root, followlinks=False):
        for name in directories + filenames:
            path = Path(parent) / name
            relative = safe_path(path.relative_to(root).as_posix())
            mode = path.lstat().st_mode
            attributes = getattr(path.lstat(), "st_file_attributes", 0)
            if stat.S_ISLNK(mode) or attributes & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0):
                raise ValueError(f"symlink or reparse point in package: {relative}")
            if relative.casefold() in folded:
                raise ValueError(f"case-colliding package path: {relative}")
            folded.add(relative.casefold())
            if path.is_file():
                files[relative] = path
            elif not path.is_dir():
                raise ValueError(f"special file in package: {relative}")
    return files


def create_manifest(root):
    files = tree_files(root)
    if MANIFEST in files:
        raise ValueError("remove an existing SHA256SUMS before creating a new one")
    lines = []
    for name, path in sorted(files.items()):
        with path.open("rb") as stream:
            lines.append(f"{file_hash(stream)}  {name}\n")
    if not lines:
        raise ValueError("release package has no files")
    data = "".join(lines).encode("utf-8")
    (root / MANIFEST).write_bytes(data)
    return data


def verify_tree(root):
    files = tree_files(root)
    if MANIFEST not in files:
        raise ValueError("SHA256SUMS is missing")
    entries = read_manifest(files.pop(MANIFEST).read_bytes())
    if set(files) != set(entries):
        raise ValueError(f"package file list differs from SHA256SUMS: missing={sorted(set(entries)-set(files))}, extra={sorted(set(files)-set(entries))}")
    for name, path in files.items():
        with path.open("rb") as stream:
            if file_hash(stream) != entries[name]:
                raise ValueError(f"SHA-256 mismatch: {name}")
    return entries


def verify_zip(path):
    with zipfile.ZipFile(path) as archive:
        members = {}
        folded = set()
        for info in archive.infolist():
            name = info.filename.rstrip("/") if info.is_dir() else info.filename
            safe_path(name)
            if name.casefold() in folded:
                raise ValueError(f"duplicate or case-colliding ZIP member: {name}")
            folded.add(name.casefold())
            mode = info.external_attr >> 16
            if stat.S_ISLNK(mode) or (mode and not (stat.S_ISREG(mode) or stat.S_ISDIR(mode))):
                raise ValueError(f"unsafe ZIP member type: {name}")
            if not info.is_dir():
                members[name] = info
        if MANIFEST not in members:
            raise ValueError("SHA256SUMS is missing from ZIP")
        entries = read_manifest(archive.read(MANIFEST))
        del members[MANIFEST]
        if set(members) != set(entries):
            raise ValueError("ZIP file list differs from SHA256SUMS")
        for name in members:
            with archive.open(members[name]) as stream:
                if file_hash(stream) != entries[name]:
                    raise ValueError(f"ZIP SHA-256 mismatch: {name}")
        return entries


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("create", "verify-tree", "verify-zip"))
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    try:
        if args.action == "create":
            create_manifest(args.path)
        elif args.action == "verify-tree":
            verify_tree(args.path)
        else:
            verify_zip(args.path)
    except (OSError, ValueError, zipfile.BadZipFile) as error:
        print(f"Release integrity check failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
