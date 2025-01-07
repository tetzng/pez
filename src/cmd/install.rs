use crate::lock_file::{Plugin, PluginFile};
use console::Emoji;
use std::{collections::HashSet, sync::Arc};
use tokio::sync::Mutex;

pub(crate) async fn run(args: &crate::cli::InstallArgs) {
    println!("{}Starting installation process...", Emoji("🔍 ", ""));
    if let Some(plugins) = &args.plugins {
        install(plugins, &args.force).await;
    } else {
        install_from_lock_file(&args.force, &args.prune);
    }
    println!(
        "\n{}All specified plugins have been installed successfully!",
        Emoji("🎉 ", "")
    );
}

async fn install(plugin_repo_list: &Vec<crate::cli::PluginRepo>, force: &bool) {
    let (mut config, config_path) = crate::utils::ensure_config();
    update_config_file(&mut config, &config_path, plugin_repo_list);

    let (mut lock_file, lock_file_path) = crate::utils::ensure_lock_file();

    let pez_data_dir = crate::utils::resolve_pez_data_dir();
    let mut new_plugins = clone_plugins(
        plugin_repo_list.iter().collect(),
        *force,
        lock_file.clone(),
        &pez_data_dir,
    )
    .await;

    let new_plugins = sync_plugin_files(&mut new_plugins, &pez_data_dir).await;
    lock_file.merge_plugins(new_plugins);
    lock_file.save(&lock_file_path);
    println!(
        "{}All plugins have been installed successfully!",
        Emoji("🎉 ", "")
    );
}

fn update_config_file(
    config: &mut crate::config::Config,
    config_path: &std::path::Path,
    plugin_repo_list: &Vec<crate::cli::PluginRepo>,
) {
    match config.plugins {
        Some(ref mut plugin_specs) => {
            for plugin_repo in plugin_repo_list {
                let repo = plugin_repo.as_str();
                if !plugin_specs.iter().any(|p| p.repo == repo) {
                    plugin_specs.push(crate::config::PluginSpec {
                        repo: repo.to_string(),
                        name: None,
                        source: None,
                    });
                }
            }
        }
        None => {
            let plugin_specs = plugin_repo_list
                .iter()
                .map(|plugin_repo| crate::config::PluginSpec {
                    repo: plugin_repo.as_str(),
                    name: None,
                    source: None,
                })
                .collect();
            config.plugins = Some(plugin_specs);
        }
    }
    config.save(&config_path.to_path_buf());
}

async fn clone_plugins(
    plugin_repo_list: Vec<&crate::cli::PluginRepo>,
    force: bool,
    lock_file: crate::lock_file::LockFile,
    pez_data_dir: &std::path::Path,
) -> Vec<crate::lock_file::Plugin> {
    let lock_file = Arc::new(Mutex::new(lock_file));
    let new_lock_plugins: Arc<Mutex<Vec<Plugin>>> = Arc::new(Mutex::new(vec![]));

    let clone_tasks: Vec<_> = plugin_repo_list
        .into_iter()
        .map(|plugin_repo| {
            let plugin_repo = plugin_repo.clone();
            let new_lock_plugins = Arc::clone(&new_lock_plugins);
            let lock_file = Arc::clone(&lock_file);
            let pez_data_dir = pez_data_dir.to_path_buf();

            tokio::spawn(async move {
                let plugin_repo_str = plugin_repo.as_str();
                let repo_path = pez_data_dir.join(&plugin_repo_str);

                if repo_path.exists() {
                    handle_existing_repository(&force, &plugin_repo_str, &repo_path);
                }

                let source = &crate::git::format_git_url(&plugin_repo_str);

                println!(
                    "{}Cloning repository from {} to {}",
                    Emoji("🔗 ", ""),
                    &source,
                    &repo_path.display()
                );
                let repo = crate::git::clone_repository(source, &repo_path).unwrap();
                let name = &plugin_repo.repo;

                let new_plugin = match lock_file.lock().await.get_plugin(source) {
                    Some(lock_file_plugin) => {
                        println!(
                            "{}Checking out commit sha: {}",
                            Emoji("🔄 ", ""),
                            &lock_file_plugin.commit_sha
                        );
                        repo.set_head_detached(
                            git2::Oid::from_str(&lock_file_plugin.commit_sha).unwrap(),
                        )
                        .unwrap();

                        crate::lock_file::Plugin {
                            name: name.to_string(),
                            repo: plugin_repo_str.clone(),
                            source: source.to_string(),
                            commit_sha: lock_file_plugin.commit_sha.clone(),
                            files: vec![],
                        }
                    }
                    None => {
                        let commit_sha = crate::git::get_latest_commit_sha(repo).unwrap();
                        crate::lock_file::Plugin {
                            name: name.to_string(),
                            repo: plugin_repo_str.to_string(),
                            source: source.to_string(),
                            commit_sha,
                            files: vec![],
                        }
                    }
                };
                new_lock_plugins.lock().await.push(new_plugin);
            })
        })
        .collect();

    futures::future::join_all(clone_tasks).await;

    Arc::try_unwrap(new_lock_plugins).unwrap().into_inner()
}

fn handle_existing_repository(force: &bool, repo: &str, repo_path: &std::path::Path) {
    if *force {
        std::fs::remove_dir_all(repo_path).unwrap();
    } else {
        eprintln!(
            "{}{} Plugin already exists: {}, Use --force to reinstall",
            Emoji("❌ ", ""),
            console::style("Error:").red().bold(),
            repo
        );
        std::process::exit(1);
    }
}

async fn sync_plugin_files(
    new_plugins: &mut [Plugin],
    pez_data_dir: &std::path::Path,
) -> Vec<Plugin> {
    println!(
        "{}Copying plugin files to fish config directory...",
        Emoji("📂 ", "")
    );
    let config_dir = crate::utils::resolve_fish_config_dir();
    let target_dirs = crate::models::TargetDir::all();

    let mut copy_tasks = Vec::new();

    let mut dest_paths = HashSet::new();

    for plugin in new_plugins.iter_mut() {
        let repo_path = pez_data_dir.join(&plugin.repo);
        let mut target_files = Vec::new();
        let mut skip_plugin = false;

        println!("{}Copying files:", Emoji("📂 ", ""));
        for target_dir in &target_dirs {
            let target_dir_str = target_dir.as_str();
            let target_path = repo_path.join(target_dir_str);
            if !target_path.exists() {
                continue;
            }

            let file_type = match target_dir {
                crate::models::TargetDir::Themes => ".theme",
                _ => ".fish",
            };
            let files = std::fs::read_dir(target_path).unwrap().filter(|f| {
                f.as_ref().unwrap().file_type().unwrap().is_file()
                    && f.as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .ends_with(file_type)
            });

            for file in files {
                let file = file.unwrap();
                let file_name = file.file_name();
                let dest_path = config_dir.join(target_dir_str).join(&file_name);

                if dest_paths.contains(&dest_path) {
                    println!(
                        "{} Skipping plugin due to duplicate: {}",
                        Emoji("🚨 ", ""),
                        plugin.repo
                    );
                    skip_plugin = true;
                    break;
                }

                println!("   - {}", dest_path.display());

                target_files.push(PluginFile {
                    dir: target_dir.clone(),
                    name: file_name.to_string_lossy().to_string(),
                });

                dest_paths.insert(dest_path.clone());
            }
            if skip_plugin {
                break;
            }
        }

        if !skip_plugin {
            target_files.iter().for_each(|f| {
                let target_dir_str = f.dir.as_str();
                let file_path = repo_path.join(target_dir_str).join(&f.name);
                let dest_path = config_dir.join(target_dir_str).join(&f.name);
                copy_tasks.push(tokio::spawn(async move {
                    tokio::task::spawn_blocking(move || {
                        std::fs::copy(&file_path, &dest_path).unwrap();
                    })
                    .await
                    .unwrap();
                }));
            });

            plugin.files = target_files.clone();
        }
    }

    futures::future::join_all(copy_tasks).await;
    new_plugins.to_vec()
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
