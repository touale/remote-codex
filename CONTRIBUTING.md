# Contributing to Remote Codex

For release downloads and usage, see the [README](README.md#installation).
This guide covers building and testing the source checkout.

## Build from source

Local CLI and desktop builds target macOS on Apple Silicon. Both embed a Linux
x86_64 execution service, which must be built first.

### Dependencies

- Git and Xcode Command Line Tools.
- Rust through rustup; `rust-toolchain.toml` selects the project toolchain.
- Zig 0.14.1 and cargo-zigbuild 0.23.4 for cross-compiling the Linux service.
- Node.js 24 and npm for the desktop application only.

Install Zig using the [official downloads](https://ziglang.org/download/), and make
`zig` available on PATH. Clone the repository:

```sh
git clone https://github.com/touale/remote-codex.git
cd remote-codex
```

Run subsequent commands from the repository root so rustup uses the selected
project toolchain. Prepare cross-compilation:

```sh
rustup target add x86_64-unknown-linux-musl
cargo install cargo-zigbuild --version 0.23.4 --locked
```

### Remote execution service and CLI

```sh
cargo zigbuild --release --locked -p remote-codex-server --target x86_64-unknown-linux-musl
cargo build --release --locked -p remote-codex
./target/release/remote-codex --help
```

The CLI embeds the service from
`target/x86_64-unknown-linux-musl/release/remote-codex-server`. For a custom target
directory or a separately built service, set `REMOTE_CODEX_SERVER_ARTIFACT` to its
absolute path before building the CLI or desktop application. Rebuild the local
application after changing the service.

### Desktop application

After building the execution service:

```sh
npm ci --prefix apps/desktop
npm --prefix apps/desktop run tauri -- build --bundles app,dmg
```

The application is written to `target/release/bundle/macos/Remote Codex.app` and the
DMG to `target/release/bundle/dmg/`. Configure a signing identity in the build
environment for release distribution; local builds can use ad-hoc signing.

## Development

Start the desktop development application after preparing the service:

```sh
npm --prefix apps/desktop run tauri -- dev
```

Keep shared connection, configuration and session behavior in the Rust client.
Presentation belongs in the CLI or desktop frontend.

| Location | Responsibility |
| --- | --- |
| `crates/client` | Shared application services, SSH, storage and recovery |
| `crates/cli` | Commands and terminal interface |
| `crates/codex-adapter` | Native Codex protocol integration |
| `crates/server` | Remote execution service |
| `crates/core`, `crates/protocol`, `crates/transfer` | Shared types, protocol and transfers |
| `crates/test-support` | Shared fixtures and native/SSH acceptance programs |
| `apps/desktop` | Tauri backend and React frontend |
| `tests` | Repository architecture checks |

## Validation

Run checks appropriate to the change. Add focused tests for observable behavior;
avoid tests that merely repeat implementation details.

### Rust

```sh
cargo fmt --all --check
cargo clippy --workspace --exclude remote-codex-desktop --all-targets --locked -- -D warnings
cargo test --workspace --exclude remote-codex-desktop --all-targets --locked
python3 tests/architecture.py
```

The architecture check requires Python 3. Product builds do not require Python.
For desktop backend changes, prepare the service and run the Cargo checks without
`--exclude remote-codex-desktop`.

### Desktop frontend

```sh
npm run check:frontend --prefix apps/desktop
```

This covers formatting, types, tests and the production frontend build. CI also
builds the isolated desktop E2E application and runs its UI checks.

### Native Codex and SSH integration

The native contract tests use `REMOTE_CODEX_TEST_BINARY` and temporary test state.
SSH acceptance requires an explicitly authorized disposable server:

```sh
cargo run --locked -p remote-codex-test-support --bin acceptance -- --help
```

Provide the native Codex, packaged CLI, Linux service, SSH target, credentials and
report directory through the documented flags. Never use private conversations or
production workspaces as fixtures.

## Submitting changes

- Discuss substantial changes in an issue first.
- Keep changes focused and reuse existing module boundaries.
- Describe the problem, resulting behavior and validation in the pull request.
- Include application and Codex versions, platform details and reproduction steps
  in bug reports. Remove credentials and private session content from logs.

Use [Issues](https://github.com/touale/remote-codex/issues) and
[Pull requests](https://github.com/touale/remote-codex/pulls) to contribute.
