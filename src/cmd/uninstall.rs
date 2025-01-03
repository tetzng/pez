pub(crate) fn run(args: &crate::cli::UninstallArgs) {
    println!("🔍 Starting uninstallation process...");
    if args.plugins.is_empty() {
        eprintln!("❌ Error: No plugins specified");
        std::process::exit(1);
    }

    for plugin in &args.plugins {
        println!("\n✨ Uninstalling plugin: {}", plugin);
        uninstall(plugin, args.force);
    }
    println!("\n🎉 All specified plugins have been uninstalled successfully!");
}

pub(crate) fn uninstall(plugin_repo: &str, force: bool) {
    let parts = plugin_repo.split("/").collect::<Vec<&str>>();
    if parts.len() != 2 {
        eprintln!("❌ Error: Invalid plugin format: {}", plugin_repo);
        std::process::exit(1);
    }
    let source = &crate::git::format_git_url(plugin_repo);
    let config_dir = crate::utils::resolve_fish_config_dir();

    let (mut config, config_path) = crate::utils::ensure_config();
    let repo_path = crate::utils::resolve_pez_data_dir().join(plugin_repo);
    let (mut lock_file, lock_file_path) = crate::utils::ensure_lock_file();
    match lock_file.get_plugin(source) {
        Some(locked_plugin) => {
            if repo_path.exists() {
                std::fs::remove_dir_all(&repo_path).unwrap();
            } else {
                println!(
                    "🚧 Warning: Repository directory at {} does not exist.",
                    &repo_path.display()
                );
                if !force {
                    println!("📄 Detected plugin files based on pez-lock.toml:");
                    locked_plugin.files.iter().for_each(|file| {
                        let dest_path = config_dir.join(file.dir.as_str()).join(&file.name);
                        println!("   - {}", dest_path.display());
                    });
                    println!("If you want to remove these files, use the --force flag.");
                    return;
                }
            }

            println!("🗑️ Removing plugin files based on pez-lock.toml:");
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
            eprintln!("❌ Error: Plugin {} is not installed.", plugin_repo);
            std::process::exit(1);
        }
    }
    println!("✅ Successfully uninstalled: {}", plugin_repo);
}
