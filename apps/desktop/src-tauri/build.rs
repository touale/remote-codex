use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};
#[path = "command_names.rs"]
mod command_names;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let path = env::var_os("REMOTE_CODEX_SERVER_ARTIFACT")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            root.join("../../../target/x86_64-unknown-linux-musl/release/remote-codex-server")
        });
    println!("cargo:rerun-if-env-changed=REMOTE_CODEX_SERVER_ARTIFACT");
    println!("cargo:rerun-if-changed={}", path.display());
    let bytes = fs::read(&path)?;
    let generated = format!(
        "const SERVICE: &[u8] = include_bytes!({:?});\nconst SERVICE_SHA: &str = \"{:x}\";\n",
        path.canonicalize()?,
        Sha256::digest(&bytes)
    );
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("bundled.rs"),
        generated,
    )?;
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(command_names::COMMANDS)),
    )?;
    Ok(())
}
