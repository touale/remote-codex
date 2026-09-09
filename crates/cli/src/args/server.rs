use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Subcommand)]
pub(crate) enum ServerCommand {
    Add(AddArgs),
    /// List saved servers and refresh their connection status.
    List,
    /// Save or replace the server's SSH password in the local OS credential store.
    Auth {
        /// Remove the saved password without connecting to the server.
        #[arg(long, conflicts_with = "password_stdin")]
        forget: bool,
        /// Read the password from standard input instead of prompting.
        #[arg(long)]
        password_stdin: bool,
    },
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
    /// Prompt for an SSH password and save it in the local OS credential store.
    #[arg(long, conflicts_with_all = ["identity", "install_key", "password_stdin", "non_interactive"])]
    pub(crate) save_password: bool,
    /// Read and save an SSH password from standard input; never pass passwords as arguments.
    #[arg(long, conflicts_with_all = ["identity", "install_key"])]
    pub(crate) password_stdin: bool,
    #[arg(long)]
    pub(crate) non_interactive: bool,
}
