use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum ServerCommand {
    Add(AddArgs),
    /// List saved servers and refresh their connection status.
    List,
    Remove {
        name: String,
        #[arg(long)]
        revoke_key: bool,
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Args, Default)]
pub(crate) struct AddArgs {
    #[arg(long)]
    pub(crate) addr: Option<String>,
    #[arg(short='p',long,value_parser=clap::value_parser!(u16).range(1..))]
    pub(crate) port: Option<u16>,
    #[arg(long, conflicts_with = "no_background")]
    pub(crate) background: bool,
    #[arg(long)]
    pub(crate) no_background: bool,
    #[arg(long,value_parser=["inherit","custom","direct"])]
    pub(crate) proxy_mode: Option<String>,
    #[arg(long)]
    pub(crate) proxy: Option<String>,
    #[arg(long = "env", value_name = "NAME=VALUE")]
    pub(crate) environment: Vec<String>,
    #[arg(long, value_parser=["sandboxed", "unrestricted"])]
    pub(crate) execution_mode: Option<String>,
    #[arg(long)]
    pub(crate) identity: Option<PathBuf>,
    #[arg(long, conflicts_with = "identity")]
    pub(crate) install_key: bool,
    #[arg(long)]
    pub(crate) non_interactive: bool,
}
