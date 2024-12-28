use clap::Parser;

mod cli;
mod commands;
mod lockfile;
mod models;
mod utils;

fn main() {
    let cli = cli::Cli::parse();

    match &cli.command {
        cli::Commands::Install { plugin } => {
            commands::install(plugin);
        }
        cli::Commands::Uninstall { plugin } => {
            println!("Uninstalling {}", plugin);
            unimplemented!();
        }
        cli::Commands::Upgrade { plugin } => {
            if let Some(plugin) = plugin {
                println!("Upgrading {}", plugin);
                unimplemented!();
            } else {
                println!("Upgrading all plugins");
                unimplemented!();
            }
        }
        cli::Commands::List { all_files } => {
            if *all_files {
                println!("Listing all plugin files");
            } else {
                println!("Listing all plugins");
            }
            unimplemented!();
        }
    }
}
