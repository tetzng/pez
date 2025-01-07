use std::fmt;

use clap::{Args, Parser, Subcommand};
use regex::Regex;

#[derive(Parser, Debug)]
#[command(name = "pez", version, about, long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Initialize pez
    Init,

    /// Install fish plugin(s)
    Install(InstallArgs),

    /// Uninstall fish plugin(s)
    Uninstall(UninstallArgs),

    /// Upgrade installed fish plugin(s)
    Upgrade(UpgradeArgs),

    /// List installed fish plugins
    List(ListArgs),

    /// Prune uninstalled plugins
    Prune(PruneArgs),

    /// Generate shell completion scripts
    Completions {
        #[arg(value_enum)]
        shell: ShellType,
    },
}

#[derive(Args, Debug)]
pub(crate) struct InstallArgs {
    /// GitHub repo in the format <owner>/<repo>
    pub(crate) plugins: Option<Vec<PluginRepo>>,

    /// Force install even if the plugin is already installed
    #[arg(short, long)]
    pub(crate) force: bool,

    /// Prune uninstalled plugins
    #[arg(short, long, conflicts_with = "plugins")]
    pub(crate) prune: bool,
}

#[derive(Args, Debug)]
pub(crate) struct UninstallArgs {
    /// GitHub repo in the format <owner>/<repo>
    #[arg(required = true)]
    pub(crate) plugins: Vec<PluginRepo>,

    /// Force uninstall even if the plugin data directory does not exist
    #[arg(short, long)]
    pub(crate) force: bool,
}

#[derive(Args, Debug)]
pub(crate) struct UpgradeArgs {
    /// GitHub repo in the format <owner>/<repo>
    pub(crate) plugins: Option<Vec<PluginRepo>>,
}

#[derive(Args, Debug)]
pub(crate) struct ListArgs {
    /// List format
    #[arg(long, value_enum)]
    pub(crate) format: Option<ListFormat>,

    /// Show only outdated plugins
    #[arg(long)]
    pub(crate) outdated: bool,
}

#[derive(Args, Debug)]
pub(crate) struct PruneArgs {
    /// Force prune even if the plugin data directory does not exist
    #[arg(short, long)]
    pub(crate) force: bool,

    /// Dry run without actually removing any files
    #[arg(long)]
    pub(crate) dry_run: bool,

    /// Confirm all prompts
    #[arg(short, long)]
    pub(crate) yes: bool,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub(crate) enum ListFormat {
    Table,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub(crate) enum ShellType {
    Fish,
}

#[derive(Debug, Clone)]
pub struct PluginRepo {
    pub owner: String,
    pub repo: String,
}

impl PluginRepo {
    pub fn as_str(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }
}

impl fmt::Display for PluginRepo {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for PluginRepo {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let re = Regex::new(r"^[a-zA-Z0-9-]+/[a-zA-Z0-9_.-]+$").unwrap();
        if re.is_match(s) && !s.ends_with('.') {
            let parts: Vec<&str> = s.split('/').collect();
            Ok(PluginRepo {
                owner: parts[0].to_string(),
                repo: parts[1].to_string(),
            })
        } else {
            Err(format!(
                "Invalid format: {}. Expected format: <owner>/<repo>",
                s
            ))
        }
    }
}
