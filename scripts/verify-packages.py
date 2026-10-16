#!/usr/bin/env python3
"""Verify workspace archives with an isolated temporary Cargo registry each run."""
import argparse
import hashlib
import shutil
import subprocess
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-dirty", action="store_true")
    args = parser.parse_args()
    target = ROOT / "target"
    target.mkdir(exist_ok=True)
    output = target / "package"
    output.mkdir(exist_ok=True)
    # Cargo caches local registry sources by URL and version. Reusing the same
    # tmp-registry path after editing an unpublished version can resolve stale
    # sibling sources, so every verification gets its own registry identity.
    with tempfile.TemporaryDirectory(prefix="verify-packages-", dir=target) as temporary:
        command = ["cargo", "package", "--workspace", "--locked", "--target-dir", temporary]
        if args.allow_dirty:
            command.append("--allow-dirty")
        subprocess.run(command, cwd=ROOT, check=True)
        archives = sorted((Path(temporary) / "package").glob("*.crate"))
        if len(archives) != 4:
            raise SystemExit("Expected exactly four verified crate archives")
        checksums = []
        for archive in archives:
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            shutil.copy2(archive, output / archive.name)
            checksums.append(f"{digest}  {archive.name}\n")
        (output / "SHA256SUMS").write_text("".join(checksums), encoding="utf-8")
    print("Verified four crate archives; saved archives and SHA256SUMS in target/package")


if __name__ == "__main__":
    main()
