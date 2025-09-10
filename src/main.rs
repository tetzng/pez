use clap::Parser;
use tracing::Level;
use tracing_subscriber::EnvFilter;

mod cli;
mod cmd;
mod config;
mod git;
mod lock_file;
mod models;
mod resolver;
mod utils;

#[cfg(test)]
mod tests_support;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();

    // Configure logging level from -v count, or RUST_LOG if provided
    let level = match cli.verbose {
        0 => Level::INFO,
        1 => Level::INFO,
        _ => Level::DEBUG,
    };
    let filter = std::env::var("RUST_LOG")
        .ok()
        .unwrap_or_else(|| level.as_str().to_lowercase());

    tracing_subscriber::fmt()
        .compact()
        .with_level(false)
        .with_target(false)
        .without_time()
        .with_env_filter(EnvFilter::new(filter))
        .init();

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
            cmd::prune::run(args).await?;
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
