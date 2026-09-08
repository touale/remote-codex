use clap::Subcommand;

#[derive(Subcommand)]
pub(crate) enum ConfigCommand {
    List {
        /// Only show settings explicitly saved for this server.
        #[arg(long)]
        overrides: bool,
    },
    Get {
        key: String,
    },
    Set {
        key: String,
        #[arg(required_unless_present = "stdin", conflicts_with = "stdin")]
        value: Option<String>,
        #[arg(long)]
        stdin: bool,
        #[arg(long, requires = "stdin")]
        secret: bool,
        #[arg(long,value_parser=clap::value_parser!(i64).range(0..))]
        if_revision: Option<i64>,
    },
    Unset {
        key: String,
        #[arg(long,value_parser=clap::value_parser!(i64).range(0..))]
        if_revision: Option<i64>,
    },
}
