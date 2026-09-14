"""Apply the git-cliff release version to all package manifests in the CI checkout."""
import json
from pathlib import Path
import re
import sys


def main():
    if len(sys.argv) != 2:
        raise SystemExit("Usage: release-version.py VERSION")
    version = sys.argv[1].removeprefix("v")
    number = r"(?:0|[1-9][0-9]*)"
    if re.fullmatch(rf"{number}\.{number}\.{number}", version) is None:
        raise SystemExit("Release version must be X.Y.Z, for example 1.2.3.")

    root = Path(__file__).resolve().parents[2]
    cargo = root / "Cargo.toml"
    content, count = re.subn(
        r'(?ms)(^\[workspace\.package\]\n(?:(?!^\[).)*?^version = ")[^"]+("$)',
        lambda match: f"{match[1]}{version}{match[2]}",
        cargo.read_text(),
    )
    if count != 1:
        raise SystemExit("Cannot locate the Cargo workspace version.")
    cargo.write_text(content)

    lock = root / "Cargo.lock"
    blocks = lock.read_text().split("[[package]]")
    for index, block in enumerate(blocks):
        if re.search(r'^name = "remote-codex(?:-[^"]+)?"$', block, re.M) and not re.search(r"^source =", block, re.M):
            blocks[index] = re.sub(r'^version = "[^"]+"$', f'version = "{version}"', block, count=1, flags=re.M)
    lock.write_text("[[package]]".join(blocks))

    for name in ["package.json", "package-lock.json", "src-tauri/tauri.conf.json"]:
        path = root / "apps/desktop" / name
        data = json.loads(path.read_text())
        data["version"] = version
        if name == "package-lock.json":
            data["packages"][""]["version"] = version
        path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n")
    print(f"Release version: {version}")


if __name__ == "__main__":
    main()
