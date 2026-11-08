#!/usr/bin/env python3
"""Check and optionally record the latest RustDesk client release."""

from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path
from urllib.request import Request, urlopen


ROOT = Path(__file__).resolve().parents[1]
VERSION_FILE = ROOT / ".github/rustdesk-upstream-version"
UPSTREAM_API = "https://api.github.com/repos/rustdesk/rustdesk/releases/latest"
VERSION_RE = re.compile(r"^\d+\.\d+\.\d+$")


def latest_version() -> str:
    override = os.environ.get("UPSTREAM_VERSION")
    if override:
        return override.removeprefix("v")
    headers = {"Accept": "application/vnd.github+json", "User-Agent": "onekvm-rustdesk-version-check"}
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    with urlopen(Request(UPSTREAM_API, headers=headers), timeout=30) as response:
        tag = json.load(response).get("tag_name", "")
    version = tag.removeprefix("v")
    if not version:
        raise RuntimeError("GitHub did not return the latest RustDesk tag")
    return version


def current_version() -> str:
    version = VERSION_FILE.read_text().strip()
    if not VERSION_RE.fullmatch(version):
        raise RuntimeError(f"unsupported pinned RustDesk version {version!r}")
    return version


def set_github_output(name: str, value: str) -> None:
    output_path = os.environ.get("GITHUB_OUTPUT")
    if output_path:
        with open(output_path, "a", encoding="utf-8") as output:
            output.write(f"{name}={value}\n")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--update", action="store_true", help="record the latest release before checking")
    args = parser.parse_args()

    latest = latest_version()
    if not VERSION_RE.fullmatch(latest):
        raise RuntimeError(f"unsupported RustDesk version {latest!r}")
    current = current_version()
    if current != latest and args.update:
        VERSION_FILE.write_text(f"{latest}\n")
        current = current_version()

    print(f"RustDesk release: {latest}")
    print(f"Recorded upstream release: {current}")
    set_github_output("version", latest)
    if current != latest:
        print(f"ERROR: recorded release is {current}, expected {latest}", file=sys.stderr)
        return 1
    print("RustDesk release marker is current.")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, ValueError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        raise SystemExit(1)
