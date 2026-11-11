#!/usr/bin/env python3
"""Checks and packages releases, as .github/workflows/release.yml runs it.

  package_release.py check TAG
      fails unless TAG is v followed by Cargo.toml's version
  package_release.py platform TARGET TAG
      packages target/TARGET/release's executable, the documents, notices/ and
      asset/gen_mlo_merger.ts as dist/mlo_merger-TARGET.{zip,tar.gz}
  package_release.py all
      combines dist/'s platform archives and the browser build in web-build/ into
      dist/mlo_merger-all-TAG.zip, whose top directory is mlo_merger-all

The release targets, and how each is built and named, are .github/release-targets.json.
`platform` reads the commit and repository from GITHUB_SHA, GITHUB_SERVER_URL and
GITHUB_REPOSITORY, as GitHub Actions sets them; elsewhere, set them by hand.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import re
import tempfile

ROOT = Path(__file__).resolve().parent.parent
TARGETS = ROOT / ".github" / "release-targets.json"

# Files every platform archive ships next to the executable.
DOCUMENTS = (
    "Readme.md", "LICENSE.md", "asset/config.toml",
    "docs/ARCHITECTURE.md", "docs/TODO.md", "docs/usage.md",
    "docs/reference.md", "docs/development.md", "docs/web.md",
    ".cargo/config.toml", "asset/gen_mlo_merger.ts",
)
NOTICES = ("third_party_notices.json", "THIRD_PARTY_NOTICES.md")


def targets():
    return json.loads(TARGETS.read_text(encoding="utf-8"))


def crate():
    """Cargo.toml's package name and version, read without tomllib, which Python 3.10 lacks."""
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    section = re.search(r"^\[package\]\s*$(.*?)(?=^\[)", text, re.M | re.S).group(1)
    field = lambda key: re.search(rf'^{key}\s*=\s*"([^"]*)"', section, re.M).group(1)
    return field("name"), field("version")


def check(tag):
    name, version = crate()
    if tag != f"v{version}":
        raise SystemExit(f"Tag {tag} does not match Cargo.toml version {version}")
    return name, version


def platform(target, tag):
    (entry,) = (entry for entry in targets() if entry["target"] == target)
    name, version = check(tag)
    archive = f"{name}-{target}"
    package = ROOT / "package" / archive
    package.mkdir(parents=True)
    shutil.copy2(ROOT / "target" / target / "release" / entry["executable"], package)
    repository = f"{os.environ['GITHUB_SERVER_URL']}/{os.environ['GITHUB_REPOSITORY']}"
    release = {
        "name": name,
        "version": version,
        "tag": tag,
        "commit": os.environ["GITHUB_SHA"],
        "repository": repository,
        "release_url": f"{repository}/releases/tag/{tag}",
        "targets": [target],
    }
    (package / "release.json").write_text(json.dumps(release, indent=2) + "\n", encoding="utf-8")
    for filename in DOCUMENTS:
        destination = package / filename
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(ROOT / filename, destination)
    for filename in NOTICES:
        shutil.copy2(ROOT / "notices" / filename, package)
    dist = ROOT / "dist"
    dist.mkdir(exist_ok=True)
    shutil.make_archive(str(dist / archive), entry["archive"], root_dir=package.parent, base_dir=archive)


def all_platforms():
    platforms = {entry["target"]: entry["platform"] for entry in targets()}
    dist = ROOT / "dist"
    archives = sorted(dist.glob("mlo_merger-*.zip")) + sorted(dist.glob("mlo_merger-*.tar.gz"))
    archives = [archive for archive in archives if not archive.name.startswith("mlo_merger-all")]
    if not archives:
        raise SystemExit("No platform archives to combine")
    name = "mlo_merger-all"
    with tempfile.TemporaryDirectory() as temporary:
        package = Path(temporary) / name
        package.mkdir()
        release = None
        built = []
        for archive in archives:
            extracted = Path(temporary) / "extracted" / archive.name
            shutil.unpack_archive(archive, extracted)
            (root,) = extracted.iterdir()
            platform_name = platforms[root.name.removeprefix("mlo_merger-")]
            # Every platform's release.json describes the same build but its own target.
            own = json.loads((root / "release.json").read_text(encoding="utf-8"))
            built += own.pop("targets")
            if release is None:
                release = own
            elif own != release:
                raise SystemExit(f"{archive.name} was built from another release")
            for path in sorted(root.rglob("*")):
                if not path.is_file():
                    continue
                relative = path.relative_to(root)
                if relative == Path("release.json"):
                    continue
                if relative.name in ("mlo_merger", "mlo_merger.exe") and len(relative.parts) == 1:
                    destination = package / f"mlo_merger-{platform_name}{relative.suffix}"
                else:
                    destination = package / relative
                    if destination.exists():
                        continue
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, destination)
        missing = sorted(set(platforms) - set(built))
        if missing:
            raise SystemExit(f"Platform archives are missing for {', '.join(missing)}")
        release["targets"] = sorted(built)
        # The browser build; see docs/web.md.
        web = ROOT / "web-build" / "web"
        if not (web / "pkg" / "mlo_merger_web_bg.wasm").is_file():
            raise SystemExit("The browser build is missing")
        shutil.copytree(web, package / "web")
        release["web"] = {
            "wasm_bindgen": json.loads((ROOT / "web-build" / "wasm_bindgen.json").read_text(encoding="utf-8")),
            "files": {
                path.relative_to(package / "web").as_posix(): hashlib.sha256(path.read_bytes()).hexdigest()
                for path in sorted((package / "web").rglob("*")) if path.is_file()
            },
        }
        (package / "release.json").write_text(json.dumps(release, indent=2) + "\n", encoding="utf-8")
        shutil.make_archive(str(dist / f"{name}-{release['tag']}"), "zip", root_dir=temporary, base_dir=name)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("check").add_argument("tag")
    command = commands.add_parser("platform")
    command.add_argument("target")
    command.add_argument("tag")
    commands.add_parser("all")
    arguments = parser.parse_args()
    if arguments.command == "check":
        name, version = check(arguments.tag)
        print(f"{name} {version}")
    elif arguments.command == "platform":
        platform(arguments.target, arguments.tag)
    else:
        all_platforms()


if __name__ == "__main__":
    sys.exit(main())
