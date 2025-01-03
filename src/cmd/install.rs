use console::Emoji;

use crate::lock_file::Plugin;

pub(crate) fn run(args: &crate::cli::InstallArgs) {
    println!("{}Starting installation process...", Emoji("🔍 ", ""));
    if let Some(plugins) = &args.plugins {
        for plugin in plugins {
            println!("\n{}Installing plugin: {plugin}", Emoji("✨ ", ""));
            install(plugin, &args.force);
            println!("{}Successfully installed: {plugin}", Emoji("✅ ", ""));
        }
    } else {
        install_from_lock_file(&args.force, &args.prune);
    }
    println!(
        "\n{}All specified plugins have been installed successfully!",
        Emoji("🎉 ", "")
    );
}

fn install(plugin_repo: &str, force: &bool) -> crate::lock_file::Plugin {
    let parts = plugin_repo.split("/").collect::<Vec<&str>>();
    if parts.len() != 2 {
        eprintln!(
            "{}{} Invalid plugin format: {}",
            Emoji("❌ ", ""),
            console::style("Error:").red().bold(),
            plugin_repo
        );
        std::process::exit(1);
    }
    let name = parts[1].to_string();
    let source = &crate::git::format_git_url(plugin_repo);

    let (mut config, config_path) = crate::utils::ensure_config();

    match config.plugins {
        Some(ref mut plugin_specs) => {
            if !plugin_specs.iter().any(|p| p.repo == plugin_repo) {
                plugin_specs.push(crate::config::PluginSpec {
                    repo: plugin_repo.to_string(),
                    name: None,
                    source: None,
                });
                config.save(&config_path);
            }
        }
        None => {
            config.plugins = Some(vec![crate::config::PluginSpec {
                repo: plugin_repo.to_string(),
                name: None,
                source: None,
            }]);
            config.save(&config_path);
        }
    }

    let repo_path = crate::utils::resolve_pez_data_dir().join(plugin_repo);

    let (mut lock_file, lock_file_path) = crate::utils::ensure_lock_file();

    match lock_file.get_plugin(source) {
        Some(locked_plugin) => {
            if repo_path.exists() {
                if *force {
                    std::fs::remove_dir_all(&repo_path).unwrap();
                } else {
                    eprintln!(
                        "{}{} Plugin already exists: {}, Use --force to reinstall",
                        Emoji("❌ ", ""),
                        console::style("Error:").red().bold(),
                        name
                    );
                    std::process::exit(1);
                }
            }

            println!(
                "{}Cloning repository from {} to {}",
                Emoji("🔗 ", ""),
                &source,
                &repo_path.display()
            );
            let repo = crate::git::clone_repository(source, &repo_path).unwrap();
            println!(
                "{}Checking out commit sha: {}",
                Emoji("🔄 ", ""),
                &locked_plugin.commit_sha
            );
            repo.set_head_detached(git2::Oid::from_str(&locked_plugin.commit_sha).unwrap())
                .unwrap();
            let mut plugin = crate::lock_file::Plugin {
                name: name.to_string(),
                repo: plugin_repo.to_string(),
                source: source.to_string(),
                commit_sha: locked_plugin.commit_sha.clone(),
                files: vec![],
            };
            crate::utils::copy_files_to_config(&repo_path, &mut plugin);
            lock_file.update_plugin(plugin.clone());
            lock_file.save(&lock_file_path);
            plugin
        }
        None => {
            if repo_path.exists() {
                if *force {
                    std::fs::remove_dir_all(&repo_path).unwrap();
                } else {
                    eprintln!(
                        "{}{} Plugin already exists: {}, Use --force to reinstall",
                        Emoji("❌ ", ""),
                        console::style("Error:").red().bold(),
                        name
                    );
                    std::process::exit(1);
                }
            }

            let repo = git2::Repository::clone(source, &repo_path).unwrap();
            let commit_sha = crate::git::get_latest_commit_sha(repo).unwrap();
            let mut plugin = crate::lock_file::Plugin {
                name: name.to_string(),
                repo: plugin_repo.to_string(),
                source: source.to_string(),
                commit_sha,
                files: vec![],
            };
            crate::utils::copy_files_to_config(&repo_path, &mut plugin);

            lock_file.add_plugin(plugin.clone());
            lock_file.save(&lock_file_path);
            plugin
        }
    }
}

fn install_from_lock_file(force: &bool, prune: &bool) {
    let (mut lock_file, lock_file_path) = crate::utils::ensure_lock_file();
    let (config, _) = crate::utils::ensure_config();

    let plugin_specs = match config.plugins {
        Some(plugins) => plugins,
        None => {
            println!("No plugins found in pez.toml");
            vec![]
        }
    };

    for plugin_spec in plugin_specs.iter() {
        let source = crate::git::format_git_url(&plugin_spec.repo);
        let repo_path = crate::utils::resolve_pez_data_dir().join(&plugin_spec.repo);

        println!(
            "\n{}Installing plugin: {}",
            Emoji("✨ ", ""),
            &plugin_spec.repo
        );
        match lock_file.get_plugin(&source) {
            Some(locked_plugin) => {
                if repo_path.exists() {
                    println!(
                        "{}Skipped: {} is already installed.",
                        Emoji("⏭️  ", ""),
                        plugin_spec.repo
                    );

                    continue;
                }

                println!(
                    "{}Cloning repository from {} to {}",
                    Emoji("🔗 ", ""),
                    &source,
                    &repo_path.display()
                );
                let repo = crate::git::clone_repository(&source, &repo_path).unwrap();
                println!(
                    "{}Checking out commit sha: {}",
                    Emoji("🔄 ", ""),
                    &locked_plugin.commit_sha
                );
                repo.set_head_detached(git2::Oid::from_str(&locked_plugin.commit_sha).unwrap())
                    .unwrap();
                let mut plugin = crate::lock_file::Plugin {
                    name: plugin_spec.get_name(),
                    repo: plugin_spec.repo.clone(),
                    source: source.to_string(),
                    commit_sha: locked_plugin.commit_sha.clone(),
                    files: vec![],
                };
                crate::utils::copy_files_to_config(&repo_path, &mut plugin);
                lock_file.update_plugin(plugin);
                lock_file.save(&lock_file_path);
            }
            None => {
                if repo_path.exists() {
                    if *force {
                        std::fs::remove_dir_all(&repo_path).unwrap();
                    } else {
                        eprintln!(
                            "{}{} Plugin already exists: {}, Use --force to reinstall",
                            Emoji("❌ ", ""),
                            console::style("Error:").red().bold(),
                            plugin_spec.repo
                        );
                        std::process::exit(1);
                    }
                }

                println!("Installing {}", plugin_spec.repo);

                let repo = git2::Repository::clone(&source, &repo_path).unwrap();
                let commit_sha = crate::git::get_latest_commit_sha(repo).unwrap();
                let mut plugin = crate::lock_file::Plugin {
                    name: plugin_spec.get_name(),
                    repo: plugin_spec.repo.clone(),
                    source: source.to_string(),
                    commit_sha,
                    files: vec![],
                };
                crate::utils::copy_files_to_config(&repo_path, &mut plugin);

                lock_file.add_plugin(plugin);
                lock_file.save(&lock_file_path);
            }
        }
    }

    let ignored_lock_file_plugins = lock_file
        .plugins
        .iter()
        .filter(|p| {
            !plugin_specs
                .iter()
                .any(|spec| crate::git::format_git_url(&spec.repo) == p.source)
        })
        .cloned()
        .collect::<Vec<Plugin>>();

    if !ignored_lock_file_plugins.is_empty() {
        if *prune {
            for plugin in ignored_lock_file_plugins {
                println!("\n{}Removing plugin: {}", Emoji("🐟 ", ""), &plugin.name);
                let repo_path = crate::utils::resolve_pez_data_dir().join(&plugin.repo);
                if repo_path.exists() {
                    std::fs::remove_dir_all(&repo_path).unwrap();
                } else {
                    println!(
                        "{}Repository directory at {} does not exist.",
                        Emoji("🚧 ", ""),
                        &repo_path.display()
                    );

                    if !force {
                        println!(
                            "{}Detected plugin files based on pez-lock.toml:",
                            Emoji("📄 ", ""),
                        );
                        plugin.files.iter().for_each(|file| {
                            let dest_path = crate::utils::resolve_fish_config_dir()
                                .join(file.dir.as_str())
                                .join(&file.name);
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
                    let dest_path = crate::utils::resolve_fish_config_dir()
                        .join(file.dir.as_str())
                        .join(&file.name);
                    if dest_path.exists() {
                        println!("   - {}", &dest_path.display());
                        std::fs::remove_file(&dest_path).unwrap();
                    }
                    lock_file.remove_plugin(&plugin.source);
                    lock_file.save(&lock_file_path);
                });
            }
        } else {
            println!("\nNotice: The following plugins are in pez-lock.toml but not in pez.toml:");
            for plugin in ignored_lock_file_plugins {
                println!("  - {}", plugin.name);
            }
            println!("If you want to remove them completely, please run:");
            println!("  pez install --prune");
            println!("or:");
            println!("  pez prune");
        }
    }
}
