use clap::Parser;

mod cli;
mod cmd;
mod config;
mod git;
mod lock_file;
mod models;
mod utils;

#[cfg(test)]
mod tests_support;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .compact()
        .with_level(false)
        .with_target(false)
        .without_time()
        .init();

    let cli = cli::Cli::parse();

    match &cli.command {
        cli::Commands::Init => {
            cmd::init::run()?;
        }
        cli::Commands::Install(args) => {
            cmd::install::run(args).await?;
        }
        cli::Commands::Uninstall(args) => {
            cmd::uninstall::run(args).await?;
        }
        cli::Commands::Upgrade(args) => {
            cmd::upgrade::run(args).await?;
        }
        cli::Commands::List(args) => {
            cmd::list::run(args)?;
        }
        cli::Commands::Prune(args) => {
            cmd::prune::run(args)?;
        }
        cli::Commands::Doctor(args) => {
            cmd::doctor::run(args)?;
        }
        cli::Commands::Migrate(args) => {
            cmd::migrate::run(args).await?;
        }
        cli::Commands::Completions { shell } => match shell {
            cli::ShellType::Fish => cmd::completion::generate_completion(clap_complete::aot::Fish),
        },
    }

    Ok(())
}
