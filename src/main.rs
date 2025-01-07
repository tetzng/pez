use clap::Parser;

mod cli;
mod config;
mod git;
mod lock_file;
mod models;
mod utils;
pub mod cmd {
    pub mod completion;
    pub mod init;
    pub mod install;
    pub mod list;
    pub mod prune;
    pub mod uninstall;
    pub mod upgrade;
}

#[tokio::main]
async fn main() {
    let cli = cli::Cli::parse();

    match &cli.command {
        cli::Commands::Init => {
            crate::cmd::init::run();
        }
        cli::Commands::Install(args) => {
            crate::cmd::install::run(args).await;
        }
        cli::Commands::Uninstall(args) => {
            crate::cmd::uninstall::run(args);
        }
        cli::Commands::Upgrade(args) => {
            crate::cmd::upgrade::run(args);
        }
        cli::Commands::List(args) => {
            crate::cmd::list::run(args);
        }
        cli::Commands::Prune(args) => {
            crate::cmd::prune::run(args);
        }
        cli::Commands::Completions { shell } => match shell {
            cli::ShellType::Fish => {
                crate::cmd::completion::generate_completion(clap_complete::aot::Fish)
            }
        },
    }
}
