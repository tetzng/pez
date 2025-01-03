use console::Emoji;

pub(crate) fn run(args: &crate::cli::PruneArgs) {
    println!("{}Starting prune process...", Emoji("🔍 ", ""));
    prune(args.force);
}

fn prune(force: bool) {
    let config_dir = crate::utils::resolve_fish_config_dir();
    let data_dir = crate::utils::resolve_pez_data_dir();
    let (config, _) = crate::utils::ensure_config();
    let (mut lock_file, lock_file_path) = crate::utils::ensure_lock_file();

    if config.plugins.is_none() {
        println!(
            "{}{} No plugins are defined in pez.toml.",
            Emoji("🚧 ", ""),
            console::style("Warning:").yellow()
        );
        println!(
            "{}All plugins defined in pez-lock.toml will be removed.",
            Emoji("🚧 ", "")
        );

        println!(
            "{}Are you sure you want to continue? [y/N]",
            Emoji("🚧 ", "")
        );
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        if input.trim().to_lowercase() != "y" {
            println!("{}Aborted.", Emoji("🚧 ", ""));
            return;
        }
    }

    let remove_plugins: Vec<_> = if config.plugins.is_none() {
        lock_file.plugins.clone()
    } else {
        lock_file
            .plugins
            .iter()
            .filter(|plugin| {
                !config
                    .plugins
                    .as_ref()
                    .unwrap()
                    .iter()
                    .any(|p| p.repo == plugin.repo)
            })
            .cloned()
            .collect()
    };

    for plugin in remove_plugins {
        let repo_path = data_dir.join(&plugin.repo);
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

                plugin.files.iter().for_each(|file| {
                    let dest_path = file.get_path(&config_dir);
                    println!("   - {}", dest_path.display());
                });
                println!("If you want to remove these files, use the --force flag.");
                continue;
            }
        }

        println!(
            "{}Removing plugin files based on pez-lock.toml:",
            Emoji("🗑️  ", ""),
        );
        plugin.files.iter().for_each(|file| {
            let dest_path = file.get_path(&config_dir);
            if dest_path.exists() {
                println!("   - {}", &dest_path.display());
                std::fs::remove_file(&dest_path).unwrap();
            }
        });
        lock_file.remove_plugin(&plugin.source);
        lock_file.save(&lock_file_path);
    }
}
