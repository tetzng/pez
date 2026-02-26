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

#[cfg(feature = "schema-gen")]
pub mod schema;

#[cfg(test)]
mod tests_support;

pub async fn run() -> anyhow::Result<()> {
    let cli = cli::Cli::parse();
    let jobs_override = cli.jobs;
    utils::set_cli_jobs_override(jobs_override);
    // Configure console color policy up front (affects console::style rendering)
    let colors_enabled = utils::colors_enabled_for_stderr();
    console::set_colors_enabled(colors_enabled);
    console::set_colors_enabled_stderr(colors_enabled);

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
        .with_ansi(colors_enabled)
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
            let _ = cmd::list::run(args)?;
        }
        cli::Commands::Prune(args) => {
            cmd::prune::run(args).await?;
        }
        cli::Commands::Doctor(args) => {
            let _ = cmd::doctor::run(args)?;
        }
        cli::Commands::Migrate(args) => {
            cmd::migrate::run(args).await?;
        }
        cli::Commands::Files(args) => {
            let _ = cmd::files::run(args)?;
        }
        cli::Commands::Activate(args) => match args.shell {
            cli::ShellType::Fish => {
                let _ = cmd::activate::run_fish();
            }
        },
        cli::Commands::Completions { shell } => match shell {
            cli::ShellType::Fish => {
                let _ = cmd::completion::generate_fish_completion()?;
            }
        },
    }

    Ok(())
}
