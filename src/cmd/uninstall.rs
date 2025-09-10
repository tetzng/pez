use crate::{
    cli::{PluginRepo, UninstallArgs},
    models::TargetDir,
    utils,
};

use console::Emoji;
use futures::{StreamExt, stream};
use std::fs;
use tracing::{error, info, warn};

pub(crate) async fn run(args: &UninstallArgs) -> anyhow::Result<()> {
    info!("{}Starting uninstallation process...", Emoji("🔍 ", ""));
    let jobs = utils::load_jobs();
    let tasks = stream::iter(args.plugins.iter())
        .map(|plugin| {
            let plugin = plugin.clone();
            let force = args.force;
            tokio::task::spawn_blocking(move || {
                info!("\n{}Uninstalling plugin: {}", Emoji("✨ ", ""), &plugin);
                uninstall(&plugin, force)
            })
        })
        .buffer_unordered(jobs);

    let results: Vec<_> = tasks.collect().await;
    for r in results {
        r??;
    }
    info!(
        "\n{}All specified plugins have been uninstalled successfully!",
        Emoji("🎉 ", "")
    );

    Ok(())
}

pub(crate) fn uninstall(plugin_repo: &PluginRepo, force: bool) -> anyhow::Result<()> {
    let plugin_repo_str = plugin_repo.as_str();
    let config_dir = utils::load_fish_config_dir()?;

    let (mut config, config_path) = utils::load_or_create_config()?;
    let repo_path = utils::load_pez_data_dir()?.join(&plugin_repo_str);
    let (mut lock_file, lock_file_path) = utils::load_or_create_lock_file()?;
    match lock_file.get_plugin_by_repo(plugin_repo) {
        Some(locked_plugin) => {
            let locked = locked_plugin.clone();
            locked
                .files
                .iter()
                .filter(|f| f.dir == TargetDir::ConfD)
                .for_each(|f| {
                    let _ = utils::emit_event(&f.name, &utils::Event::Uninstall);
                });

            if repo_path.exists() {
                fs::remove_dir_all(&repo_path)?;
            } else {
                let path_display = repo_path.display();
                warn!(
                    "{}{} Repository directory at {} does not exist.",
                    Emoji("🚧 ", ""),
                    console::style("Warning:").yellow(),
                    path_display
                );
                if !force {
                    info!(
                        "{}Detected plugin files based on pez-lock.toml:",
                        Emoji("📄 ", ""),
                    );
                    locked.files.iter().for_each(|file| {
                        let dest_path = config_dir.join(file.dir.as_str()).join(&file.name);
                        info!("   - {}", dest_path.display());
                    });
                    error!("If you want to remove these files, use the --force flag.");
                    anyhow::bail!(
                        "Repository directory does not exist. Use --force to remove files listed in lock file"
                    );
                }
            }

            info!(
                "{}Removing plugin files based on pez-lock.toml:",
                Emoji("🗑️  ", ""),
            );
            locked.files.iter().for_each(|file| {
                let dest_path = config_dir.join(file.dir.as_str()).join(&file.name);
                if dest_path.exists() {
                    let path_display = dest_path.display();
                    info!("   - {}", path_display);
                    fs::remove_file(&dest_path).unwrap();
                }
            });
            lock_file.remove_plugin(&locked.source);
            lock_file.save(&lock_file_path)?;

            if let Some(ref mut plugin_specs) = config.plugins {
                plugin_specs.retain(|p| p.get_plugin_repo().map_or(true, |r| r != *plugin_repo));
                config.save(&config_path)?;
            }
        }
        None => {
            error!(
                "{}{} Plugin {} is not installed.",
                Emoji("❌ ", ""),
                console::style("Error:").red().bold(),
                plugin_repo_str
            );
            anyhow::bail!("Plugin is not installed: {}", plugin_repo_str);
        }
    }
    info!(
        "{}Successfully uninstalled: {}",
        Emoji("✅ ", ""),
        plugin_repo_str
    );

    Ok(())
}
