use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Install a fish plugin
    Install {
        /// GitHub repo in the format <author>/<repo>
        plugin: String,
    },
    /// Uninstall a fish plugin
    Uninstall {
        /// GitHub repo in the format <author>/<repo>
        plugin: String,
    },
    /// Upgrade all installed fish plugins
    Upgrade {
        /// GitHub repo in the format <author>/<repo>
        plugin: Option<String>,
    },
    /// List installed fish plugins
    List {
        /// show all plugin files
        #[arg(long)]
        all_files: bool,
    },
}
