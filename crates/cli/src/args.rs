mod config;
mod server;
use clap::{Parser, Subcommand};
pub(crate) use config::ConfigCommand;
pub(crate) use server::{AddArgs, ServerCommand};
use std::{ffi::OsString, path::PathBuf};

#[derive(Parser)]
#[command(
    name = "remote-codex",
    version,
    about = "Run local Codex with remote execution environments"
)]
pub(crate) struct Cli {
    #[arg(short = 'n', long = "name", global = true, value_name = "SERVER")]
    pub(crate) connection: Option<String>,
    #[arg(long, global = true, value_name = "REMOTE_DIRECTORY")]
    pub(crate) path: Option<String>,
    #[arg(long, global = true)]
    pub(crate) json: bool,
    #[arg(long, global = true)]
    pub(crate) data_dir: Option<PathBuf>,
    #[arg(long, global = true)]
    pub(crate) ssh_config: Option<PathBuf>,
    #[arg(long, global = true)]
    pub(crate) takeover: bool,
    /// Trust the current remote project MCP configuration.
    #[arg(long, global = true)]
    pub(crate) trust_project_mcp: bool,
    /// Resolve conflicting local/project MCP names explicitly.
    #[arg(long, global=true, value_parser=["local","remote"])]
    pub(crate) mcp_source: Option<String>,
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
    #[arg(skip)]
    pub(crate) native_arguments: Vec<OsString>,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Check for and install Remote Codex CLI updates.
    Update {
        #[command(subcommand)]
        command: Option<UpdateCommand>,
    },
    /// Initialize, inspect or remove saved servers.
    Server {
        #[command(subcommand)]
        command: ServerCommand,
    },
    /// Resume a local Codex session; its execution environment connects automatically.
    Resume {
        id: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(long)]
        archived: bool,
        /// Read local Codex history without connecting to the environment.
        #[arg(long, requires = "id")]
        read: bool,
        /// Continue a page returned by --read.
        #[arg(long, requires = "read")]
        cursor: Option<String>,
    },
    /// Inspect or change persistent settings for the server selected with -n.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Open an interactive SSH shell on the selected server.
    Shell,
}

#[derive(Subcommand)]
pub(crate) enum UpdateCommand {
    /// Check the latest stable Remote Codex release.
    Check,
    /// Show the CLI version and update status.
    Status,
    /// View or change the shared product update policy.
    Config {
        #[arg(long, value_parser = ["notify", "auto", "manual"])]
        mode: Option<String>,
    },
}

impl Cli {
    pub(crate) fn parse_argv(arguments: Vec<OsString>) -> Result<Self, clap::Error> {
        let separator = arguments.iter().position(|arg| arg == "--");
        let Some(index) = separator else {
            return Self::try_parse_from(arguments);
        };
        let mut cli = Self::try_parse_from(&arguments[..index])?;
        if !matches!(
            cli.command,
            None | Some(Command::Resume { read: false, .. })
        ) {
            return Err(clap::Error::raw(
                clap::error::ErrorKind::ArgumentConflict,
                "native arguments are only supported when opening a session",
            ));
        }
        cli.native_arguments = arguments[index + 1..].to_vec();
        Ok(cli)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn separator_keeps_native_arguments_out_of_management_parser() -> Result<(), clap::Error> {
        let cli = Cli::parse_argv(
            ["remote-codex", "-n", "dev", "--", "server", "remove", "dev"]
                .into_iter()
                .map(Into::into)
                .collect(),
        )?;
        assert!(cli.command.is_none());
        assert_eq!(cli.native_arguments[0], "server");
        assert!(Cli::parse_argv(vec!["remote-codex".into(), "conect".into()]).is_err());
        Ok(())
    }
}
