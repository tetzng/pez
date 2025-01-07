use console::Emoji;

use crate::cli::PluginRepo;

pub(crate) fn run(args: &crate::cli::UninstallArgs) {
    println!("{}Starting uninstallation process...", Emoji("🔍 ", ""));
    for plugin in &args.plugins {
        println!("\n{}Uninstalling plugin: {}", Emoji("✨ ", ""), plugin);
        uninstall(plugin, args.force);
    }
    println!(
        "\n{}All specified plugins have been uninstalled successfully!",
        Emoji("🎉 ", "")
    );
}

pub(crate) fn uninstall(plugin_repo: &PluginRepo, force: bool) {
    let plugin_repo = plugin_repo.as_str();
    let source = &crate::git::format_git_url(&plugin_repo);
    let config_dir = crate::utils::resolve_fish_config_dir();

    let (mut config, config_path) = crate::utils::ensure_config();
    let repo_path = crate::utils::resolve_pez_data_dir().join(&plugin_repo);
    let (mut lock_file, lock_file_path) = crate::utils::ensure_lock_file();
    match lock_file.get_plugin(source) {
        Some(locked_plugin) => {
            if repo_path.exists() {
                std::fs::remove_dir_all(&repo_path).unwrap();
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
                    std::process::exit(1);
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
                    std::fs::remove_file(&dest_path).unwrap();
                }
            });
            lock_file.remove_plugin(source);
            lock_file.save(&lock_file_path);

            if let Some(ref mut plugin_specs) = config.plugins {
                plugin_specs.retain(|p| p.repo != plugin_repo);
                config.save(&config_path);
            }
        }
        None => {
            eprintln!(
                "{}{} Plugin {} is not installed.",
                Emoji("❌ ", ""),
                console::style("Error:").red().bold(),
                plugin_repo
            );
            std::process::exit(1);
        }
    }
    println!(
        "{}Successfully uninstalled: {}",
        Emoji("✅ ", ""),
        plugin_repo
    );
}
