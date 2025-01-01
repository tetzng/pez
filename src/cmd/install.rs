pub(crate) fn run(args: &crate::cli::InstallArgs) {
    if let Some(plugins) = &args.plugins {
        for plugin in plugins {
            println!("Installing {}", &plugin);
            install(plugin, &args.force);
        }
    } else {
        install_from_lock_file(&args.force);
    }
}

fn install(plugin_repo: &str, force: &bool) -> crate::models::Plugin {
    let parts = plugin_repo.split("/").collect::<Vec<&str>>();
    if parts.len() != 2 {
        eprintln!("Invalid plugin format: {}", plugin_repo)
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
                    eprintln!("Plugin already exists: {}, Use --force to reinstall", name)
                }
            }

            let repo = crate::git::clone_repository(source, &repo_path).unwrap();
            repo.set_head_detached(git2::Oid::from_str(&locked_plugin.commit_sha).unwrap())
                .unwrap();
            let mut plugin = crate::models::Plugin {
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
                    eprintln!("Plugin already exists: {}, Use --force to reinstall", name)
                }
            }

            let repo = git2::Repository::clone(source, &repo_path).unwrap();
            let commit_sha = crate::git::get_latest_commit_sha(repo).unwrap();
            let mut plugin = crate::models::Plugin {
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

fn install_from_lock_file(force: &bool) {
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

        match lock_file.get_plugin(&source) {
            Some(locked_plugin) => {
                if repo_path.exists() {
                    continue;
                }
                println!("Installing {}", plugin_spec.repo);

                let repo = crate::git::clone_repository(&source, &repo_path).unwrap();
                repo.set_head_detached(git2::Oid::from_str(&locked_plugin.commit_sha).unwrap())
                    .unwrap();
                let mut plugin = crate::models::Plugin {
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
                            "Plugin already exists: {}, Use --force to reinstall",
                            plugin_spec.repo
                        )
                    }
                }

                println!("Installing {}", plugin_spec.repo);

                let repo = git2::Repository::clone(&source, &repo_path).unwrap();
                let commit_sha = crate::git::get_latest_commit_sha(repo).unwrap();
                let mut plugin = crate::models::Plugin {
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
        .collect::<Vec<&crate::models::Plugin>>();

    if !ignored_lock_file_plugins.is_empty() {
        println!("Notice: The following plugins are in pez-lock.toml but not in pez.toml:");
        for plugin in ignored_lock_file_plugins {
            println!("  - {}", plugin.name);
        }
        println!("If you want to remove them completely, please run:");
        println!("  pez install --prune");
        println!("or:");
        println!("  pez prune");
    }
}
