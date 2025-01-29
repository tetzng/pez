use crate::{
    cli::{PluginRepo, UninstallArgs},
    git, utils,
};
use anyhow::Ok;
use console::Emoji;
use std::{fs, process};

pub(crate) fn run(args: &UninstallArgs) -> anyhow::Result<()> {
    println!("{}Starting uninstallation process...", Emoji("🔍 ", ""));
    for plugin in &args.plugins {
        println!("\n{}Uninstalling plugin: {}", Emoji("✨ ", ""), plugin);
        uninstall(plugin, args.force)?;
    }
    println!(
        "\n{}All specified plugins have been uninstalled successfully!",
        Emoji("🎉 ", "")
    );

    Ok(())
}

pub(crate) fn uninstall(plugin_repo: &PluginRepo, force: bool) -> anyhow::Result<()> {
    let plugin_repo_str = plugin_repo.as_str();
    let source = &git::format_git_url(&plugin_repo_str);
    let config_dir = utils::load_fish_config_dir()?;

    let (mut config, config_path) = utils::load_or_create_config()?;
    let repo_path = utils::load_pez_data_dir()?.join(&plugin_repo_str);
    let (mut lock_file, lock_file_path) = utils::load_or_create_lock_file()?;
    match lock_file.get_plugin(source) {
        Some(locked_plugin) => {
            if repo_path.exists() {
                fs::remove_dir_all(&repo_path)?;
            } else {
                println!(
                    "{}{} Repository directory at {} does not exist.",
                    Emoji("🚧 ", ""),
                    console::style("Warning:").yellow(),
                    &repo_path.display()
                );
                if !force {
                    println!(
                        "{}Detected plugin files based on pez-lock.toml:",
                        Emoji("📄 ", ""),
                    );
                    locked_plugin.files.iter().for_each(|file| {
                        let dest_path = config_dir.join(file.dir.as_str()).join(&file.name);
                        println!("   - {}", dest_path.display());
                    });
                    eprintln!("If you want to remove these files, use the --force flag.");
                    process::exit(1);
                }
            }

            println!(
                "{}Removing plugin files based on pez-lock.toml:",
                Emoji("🗑️  ", ""),
            );
            locked_plugin.files.iter().for_each(|file| {
                let dest_path = config_dir.join(file.dir.as_str()).join(&file.name);
                if dest_path.exists() {
                    println!("   - {}", &dest_path.display());
                    fs::remove_file(&dest_path).unwrap();
                }
            });
            lock_file.remove_plugin(source);
            lock_file.save(&lock_file_path)?;

            if let Some(ref mut plugin_specs) = config.plugins {
                plugin_specs.retain(|p| p.repo != plugin_repo.clone());
                config.save(&config_path)?;
            }
        }
        None => {
            eprintln!(
                "{}{} Plugin {} is not installed.",
                Emoji("❌ ", ""),
                console::style("Error:").red().bold(),
                plugin_repo_str
            );
            process::exit(1);
        }
    }
    println!(
        "{}Successfully uninstalled: {}",
        Emoji("✅ ", ""),
        plugin_repo_str
    );

    Ok(())
}
