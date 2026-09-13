"""Check source size and Rust workspace dependency direction without extra packages."""

import json
import os
import re
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
IGNORED = {".git", "tools", ".dev-notes", ".tools", ".artifacts", "target", "node_modules", "dist", "gen", "__pycache__"}
SOURCE_SUFFIXES = {".rs", ".py", ".sh", ".sql", ".ts", ".tsx", ".js", ".jsx", ".css"}
MAX_LINES = 500
TARGET_LINES = 300

# Extend deliberately when introducing the documented workspace boundaries.
ALLOWED = {
    "remote-codex-core": set(),
    "remote-codex-protocol": set(),
    "remote-codex-transfer": {"remote-codex-protocol"},
    "remote-codex-client": {"remote-codex-core", "remote-codex-protocol", "remote-codex-adapter", "remote-codex-transfer"},
    "remote-codex-adapter": {"remote-codex-core", "remote-codex-protocol"},
    "remote-codex-server": {
        "remote-codex-core", "remote-codex-protocol", "remote-codex-adapter", "remote-codex-transfer"
    },
    "remote-codex": {"remote-codex-client"},
    "remote-codex-desktop": {"remote-codex-client"},
    "remote-codex-test-support": {"remote-codex-client", "remote-codex-adapter", "remote-codex-protocol"},
}
DEV_ALLOWED = {
    "remote-codex": {"remote-codex-test-support"},
    "remote-codex-server": {"remote-codex-test-support"},
}
CORE_FORBIDDEN = {"clap", "sqlx", "tauri", "tokio", "tokio-tungstenite", "nix"}


def check_sources():
    errors = []
    largest = (0, "")
    count = 0
    for directory, children, files in os.walk(ROOT):
        children[:] = sorted(name for name in children if name not in IGNORED)
        for name in sorted(files):
            path = Path(directory) / name
            if path.suffix not in SOURCE_SUFFIXES:
                continue
            relative = str(path.relative_to(ROOT))
            # No generated-code exemption is registered in this workspace yet.
            lines = len(path.read_text().splitlines())
            count += 1
            largest = max(largest, (lines, relative))
            if lines > MAX_LINES:
                errors.append(f"{relative}: {lines} lines exceeds {MAX_LINES}")
            elif lines > TARGET_LINES:
                print(f"NOTE: {relative}: {lines} lines exceeds the {TARGET_LINES}-line target")
    print(f"Source size: {count} files; largest {largest[1]} ({largest[0]} lines)")
    return errors


def check_dependencies():
    command = ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline", "--locked"]
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, check=True)
    packages = json.loads(result.stdout)["packages"]
    names = {package["name"] for package in packages}
    errors = []
    for package in packages:
        name = package["name"]
        if name not in ALLOWED:
            errors.append(f"{name}: missing explicit architecture rule")
            continue
        for dependency in package["dependencies"]:
            target = dependency["name"]
            allowed = ALLOWED[name]
            if dependency["kind"] == "dev":
                allowed = allowed | DEV_ALLOWED.get(name, set())
            if target in names and target not in allowed:
                kind = dependency["kind"] or "normal"
                errors.append(f"Forbidden {kind} workspace dependency: {name} -> {target}")
            if name == "remote-codex-core" and target in CORE_FORBIDDEN:
                errors.append(f"Core cannot depend on infrastructure: {target}")
    print(f"Dependency direction: checked {len(packages)} packages")
    return errors


def check_application_boundary():
    errors = []
    source = (ROOT / "crates/client/src/lib.rs").read_text()
    for module in ["store", "ssh", "local", "remote", "runtime", "credentials", "extensions", "servers", "sessions"]:
        if re.search(r"pub\s+mod\s+" + module + r"\s*;", source):
            errors.append(f"Client infrastructure must be private: {module}")
    for path in (ROOT / "crates/client/src/application").glob("*.rs"):
        text = path.read_text()
        for declaration in re.findall(r"\bpub\s+(?:async\s+)?fn\s+[^{{;]+", text):
            if re.search(r"\b(Value|LocalStore|SshTransport|Remote|Engine)\b", declaration):
                errors.append(f"Native or infrastructure type in application API: {path.name}")
    for path in (ROOT / "crates/cli/src").rglob("*.rs"):
        text = path.read_text()
        if re.search(r"\b(sqlx|remote_codex_adapter|remote_codex_protocol)\b", text):
            errors.append(f"CLI must use the application boundary: {path.relative_to(ROOT)}")
    for path in (ROOT / "crates/client/src").rglob("*.rs"):
        relative = path.relative_to(ROOT / "crates/client/src")
        if relative.parts[0] in {"store", "tests"} or path.name.endswith("_tests.rs"):
            continue
        if re.search(r"sqlx::(?:query|raw_sql)|\bstore\.pool\b", path.read_text()):
            errors.append(f"Client persistence belongs in store: {path.relative_to(ROOT)}")
    return errors


def check_desktop_boundary():
    errors = []
    root = ROOT / "apps/desktop/src"
    imports = re.compile(r"(?:from\s*|import\s*\()\s*['\"]([^'\"]+)['\"]")
    for path in root.rglob("*"):
        if path.suffix not in {".ts", ".tsx"} or path.name.endswith(".test.ts"):
            continue
        relative = path.relative_to(root)
        source = path.read_text()
        for target in imports.findall(source):
            if not target.startswith("."):
                if target.startswith("@tauri-apps/api") and relative.parts[0] != "bridge":
                    errors.append(f"Native IPC belongs in bridge: {relative}")
                continue
            resolved = (path.parent / target).resolve()
            if not resolved.is_relative_to(root):
                if relative != Path("main.tsx"):
                    errors.append(f"Production import outside src: {relative} -> {target}")
                continue
            owner = resolved.relative_to(root).parts[0]
            domain = relative.parts[0]
            if (domain in {"bridge", "ui"} and owner != domain) or (domain != "app" and owner == "app" and relative != Path("main.tsx")):
                errors.append(f"Forbidden frontend dependency: {relative} -> {target}")
        if re.search(r":\s*ReturnType<typeof use\w+>", source):
            errors.append(f"Pass only consumed hook members: {relative}")
    handlers = (ROOT / "apps/desktop/src-tauri/src/lib.rs").read_text().split("tauri::generate_handler![", 1)[1].split("])", 1)[0]
    registered = set(re.findall(r"commands::\w+::(\w+)", handlers))
    declared = set(re.findall(r"^  (\w+): Command<", (root / "bridge/commands.ts").read_text(), re.MULTILINE))
    if registered != declared:
        errors.append(f"Desktop command contract differs: missing={sorted(registered-declared)}, extra={sorted(declared-registered)}")
    manifest = set(re.findall(r'"(\w+)"', (ROOT / "apps/desktop/src-tauri/command_names.rs").read_text()))
    permissions = json.loads((ROOT / "apps/desktop/src-tauri/capabilities/main.json").read_text())["permissions"]
    allowed = {p.removeprefix("allow-").replace("-", "_") for p in permissions if p.startswith("allow-")}
    if declared != manifest or declared != allowed:
        errors.append("Desktop commands, manifest and capability permissions must match")
    print(f"Desktop boundaries: checked {len(declared)} typed IPC commands")
    return errors


def main():
    try:
        errors = check_sources() + check_dependencies() + check_application_boundary() + check_desktop_boundary()
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Architecture check failed to run: {error}", file=sys.stderr)
        return 1
    if errors:
        for error in errors:
            print(error, file=sys.stderr)
        return 1
    print("Architecture checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
