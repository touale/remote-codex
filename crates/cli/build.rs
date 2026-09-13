use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let default = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?)
        .join("../../target/x86_64-unknown-linux-musl/release/remote-codex-server");
    println!("cargo:rerun-if-env-changed=REMOTE_CODEX_SERVER_ARTIFACT");
    let path = env::var_os("REMOTE_CODEX_SERVER_ARTIFACT")
        .map(PathBuf::from)
        .unwrap_or(default);
    println!("cargo:rerun-if-changed={}", path.display());
    let generated = if path.is_file() {
        let bytes = fs::read(&path)?;
        format!(
            "pub(crate) const SERVICE: &[u8] = include_bytes!({:?});\npub(crate) const SERVICE_SHA: &str = \"{:x}\";\n",
            path.canonicalize()?,
            Sha256::digest(&bytes)
        )
    } else {
        "pub(crate) const SERVICE: &[u8] = &[];\npub(crate) const SERVICE_SHA: &str = \"\";\n"
            .into()
    };
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("bundled.rs"),
        generated,
    )?;
    Ok(())
}
