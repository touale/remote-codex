use std::{ffi::OsString, path::PathBuf};
pub(super) async fn get() -> Option<OsString> {
    std::env::var_os("PATH")
}
pub(super) fn candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(home) = std::env::var_os("USERPROFILE") {
        paths.push(PathBuf::from(home).join(".local/bin/codex.exe"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        let package = PathBuf::from(appdata).join("npm/node_modules/@openai/codex");
        for root in [
            package.join("node_modules/@openai/codex-win32-x64"),
            package,
        ] {
            paths.push(root.join("vendor/x86_64-pc-windows-msvc/codex/codex.exe"));
        }
    }
    if let Some(search) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&search) {
            for package in [
                "node_modules/@openai/codex/node_modules/@openai/codex-win32-x64",
                "node_modules/@openai/codex",
            ] {
                paths.push(
                    directory
                        .join(package)
                        .join("vendor/x86_64-pc-windows-msvc/codex/codex.exe"),
                );
            }
        }
    }
    paths
}
