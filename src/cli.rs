use clap::{Args, Parser, Subcommand};

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
    pub(crate) plugins: Option<Vec<String>>,

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
    pub(crate) plugins: Vec<String>,

    /// Force uninstall even if the plugin data directory does not exist
    #[arg(short, long)]
    pub(crate) force: bool,
}

#[derive(Args, Debug)]
pub(crate) struct UpgradeArgs {
    /// GitHub repo in the format <owner>/<repo>
    pub(crate) plugins: Option<Vec<String>>,
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
    // Json,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub(crate) enum ShellType {
    Fish,
}
