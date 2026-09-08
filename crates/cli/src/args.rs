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
    #[arg(
        short = 'n',
        long = "name",
        alias = "connection",
        global = true,
        value_name = "SERVER"
    )]
    pub(crate) connection: Option<String>,
    #[arg(
        long,
        alias = "cd",
        short = 'C',
        global = true,
        value_name = "REMOTE_DIRECTORY"
    )]
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
}

#[derive(Subcommand)]
pub(crate) enum Command {
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
    Shell {
        #[arg(hide = true)]
        legacy: Option<String>,
        #[arg(hide = true)]
        legacy_shell: Option<String>,
    },
    #[command(external_subcommand)]
    Codex(Vec<OsString>),
}

impl Cli {
    pub(crate) fn parse_argv(arguments: Vec<OsString>) -> Result<Self, clap::Error> {
        let mut index = 1;
        while index < arguments.len() {
            let arg = arguments[index].to_string_lossy();
            if arg == "--" {
                let mut cli = Self::try_parse_from(&arguments[..index])?;
                cli.command = Some(Command::Codex(arguments[index + 1..].to_vec()));
                return Ok(cli);
            }
            if matches!(
                arg.as_ref(),
                "-n" | "--name"
                    | "--connection"
                    | "--path"
                    | "--cd"
                    | "-C"
                    | "--data-dir"
                    | "--ssh-config"
                    | "--mcp-source"
            ) {
                index += 2;
                continue;
            }
            if matches!(
                arg.as_ref(),
                "--json" | "--takeover" | "--trust-project-mcp"
            ) || [
                "--name=",
                "--connection=",
                "--path=",
                "--cd=",
                "--data-dir=",
                "--ssh-config=",
                "--mcp-source=",
            ]
            .iter()
            .any(|p| arg.starts_with(p))
            {
                index += 1;
                continue;
            }
            break;
        }
        Self::try_parse_from(arguments)
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
        assert!(matches!(cli.command,Some(Command::Codex(args)) if args[0]=="server"));
        Ok(())
    }
}
