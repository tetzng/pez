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
    Prune,
}

#[derive(Args, Debug)]
pub(crate) struct InstallArgs {
    /// GitHub repo in the format <owner>/<repo>
    pub(crate) plugins: Option<Vec<String>>,

    /// Force install even if the plugin is already installed
    #[arg(short, long)]
    pub(crate) force: bool,
}

#[derive(Args, Debug)]
pub(crate) struct UninstallArgs {
    /// GitHub repo in the format <owner>/<repo>
    pub(crate) plugins: Vec<String>,
}

#[derive(Args, Debug)]
pub(crate) struct UpgradeArgs {
    /// GitHub repo in the format <owner>/<repo>
    pub(crate) plugins: Option<Vec<String>>,
}

#[derive(Args, Debug)]
pub(crate) struct ListArgs {
    /// Show only outdated plugins
    #[arg(short, long)]
    pub(crate) outdated: bool,
}
