#!/usr/bin/env python3
"""Stage the game executable and its third-party license for distribution."""

import argparse
import shutil
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("executable", type=Path, help="built game executable to stage")
    parser.add_argument("destination", type=Path, help="distribution directory")
    args = parser.parse_args()

    destination = args.destination
    destination.mkdir(parents=True, exist_ok=True)

    repository = Path(__file__).resolve().parent.parent
    license_file = repository / "crates/factory_app/third_party/fira_mono/LICENSE.txt"
    shutil.copy2(args.executable, destination / args.executable.name)
    shutil.copy2(license_file, destination / "licenses.txt")


if __name__ == "__main__":
    main()
