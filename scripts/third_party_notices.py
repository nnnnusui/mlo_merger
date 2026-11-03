#!/usr/bin/env python3
"""Converts cargo-about JSON output into the release's third-party notices.

Usage:
    for target in ...; do
        cargo about generate --locked --fail --format json --target "$target" -o "about-$target.json"
    done
    python3 scripts/third_party_notices.py about-*.json -o third_party_notices.json --markdown THIRD_PARTY_NOTICES.md

Pass one cargo-about output per release target: cargo-about resolves every target it is
given at once and then lists crates that no single target compiles, so the outputs are
generated separately and merged here.

The written `third_party_notices.json` is read by other projects; keep its format:
    {"parts": ["mlo_merger"],
     "notices": [{"license": SPDX id, "text": license text,
                  "packages": [{"name", "version", "repository"}]}]}
There is one notice per distinct license text, packages sorted by name and notices by
their first package's name.
"""

import argparse
import json
import re
from pathlib import Path
import sys

PART = "mlo_merger"


def normalize(text):
    return text.replace("\r\n", "\n").strip()


def repository(package):
    for key in ("repository", "homepage"):
        if package.get(key):
            return package[key]
    return f"https://crates.io/crates/{package['name']}"


def package_key(package):
    return package["name"], package["version"]


def collect(documents):
    """Returns {crate key: package} and {normalized text: (license id, {crate key})}."""
    packages = {}
    texts = {}
    for document in documents:
        for crate in document["crates"]:
            package = crate["package"]
            # Workspace crates are path dependencies without a registry source.
            if package.get("source") is None:
                continue
            packages[package_key(package)] = package
        for license in document["licenses"]:
            text = normalize(license["text"])
            if not text:
                continue
            license_id, keys = texts.setdefault(text, (license["id"], set()))
            if license_id != license["id"]:
                sys.exit(f"The same license text is identified as both {license_id} and {license['id']}")
            for used in license["used_by"]:
                keys.add(package_key(used["crate"]))
    return packages, texts


def notices(packages, texts):
    licensed = set()
    result = []
    for text, (license_id, keys) in texts.items():
        keys = keys & packages.keys()
        if not keys:
            continue
        licensed |= keys
        result.append({
            "license": license_id,
            "text": text,
            "packages": [
                {"name": name, "version": version, "repository": repository(packages[name, version])}
                for name, version in sorted(keys)
            ],
        })
    unlicensed = sorted(packages.keys() - licensed)
    if unlicensed:
        sys.exit("No license text found for " + ", ".join(f"{name} {version}" for name, version in unlicensed))
    result.sort(key=lambda notice: (notice["packages"][0]["name"], notice["packages"][0]["version"],
                                    notice["license"], notice["text"]))
    return result


def markdown(notices):
    lines = [
        "# Third-Party Notices",
        "",
        f"{PART} includes the following crates, distributed under the license texts below.",
    ]
    for notice in notices:
        lines += ["", f"## {notice['license']}", ""]
        lines += [f"- [{package['name']} {package['version']}]({package['repository']})"
                  for package in notice["packages"]]
        longest = max((len(run) for run in re.findall("`+", notice["text"])), default=0)
        fence = "`" * max(3, longest + 1)
        lines += ["", fence, notice["text"], fence]
    return "\n".join(lines) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("inputs", nargs="+", type=Path, help="cargo-about JSON outputs (--format json)")
    parser.add_argument("-o", "--output", type=Path, required=True, help="third_party_notices.json to write")
    parser.add_argument("--markdown", type=Path, help="THIRD_PARTY_NOTICES.md to write")
    arguments = parser.parse_args()

    documents = [json.loads(path.read_text(encoding="utf-8")) for path in arguments.inputs]
    result = notices(*collect(documents))
    output = {"parts": [PART], "notices": result}
    arguments.output.write_text(json.dumps(output, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    if arguments.markdown:
        arguments.markdown.write_text(markdown(result), encoding="utf-8")


if __name__ == "__main__":
    main()
