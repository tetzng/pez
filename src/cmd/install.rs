use crate::{
    cli::{InstallArgs, PluginRepo},
    config, git,
    lock_file::{LockFile, Plugin, PluginFile},
    models::TargetDir,
    utils,
};
use anyhow::Ok;
use console::Emoji;
use futures::future;
use std::{collections::HashSet, fs, path, process, result, sync::Arc};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

pub(crate) async fn run(args: &InstallArgs) -> anyhow::Result<()> {
    info!("{}Starting installation process...", Emoji("🔍 ", ""));

    handle_installation(args).await?;

    Ok(())
}

async fn handle_installation(args: &InstallArgs) -> anyhow::Result<()> {
    if let Some(plugins) = &args.plugins {
        install(plugins, &args.force).await?;
        info!(
            "\n{}All specified plugins have been installed successfully!",
            Emoji("🎉 ", "")
        );
    } else {
        install_all(&args.force, &args.prune)?;
    }

    Ok(())
}

async fn install(plugin_repo_list: &Vec<PluginRepo>, force: &bool) -> anyhow::Result<()> {
    let (mut config, config_path) = utils::load_or_create_config()?;
    add_plugins_to_config(&mut config, &config_path, plugin_repo_list)?;

    let (mut lock_file, lock_file_path) = utils::load_or_create_lock_file()?;

    let pez_data_dir = utils::load_pez_data_dir()?;
    let mut new_plugins = clone_plugins(
        plugin_repo_list.iter().collect(),
        *force,
        lock_file.clone(),
        &pez_data_dir,
    )
    .await?;

    let new_plugins = sync_plugin_files(&mut new_plugins, &pez_data_dir).await?;
    lock_file.merge_plugins(new_plugins);
    lock_file.save(&lock_file_path)?;
    info!(
        "{}All plugins have been installed successfully!",
        Emoji("✅ ", "")
    );
    Ok(())
}

fn add_plugins_to_config(
    config: &mut config::Config,
    config_path: &path::Path,
    plugin_repo_list: &Vec<PluginRepo>,
) -> anyhow::Result<()> {
    match config.plugins {
        Some(ref mut plugin_specs) => {
            for plugin_repo in plugin_repo_list {
                if !plugin_specs.iter().any(|p| p.repo == *plugin_repo) {
                    plugin_specs.push(config::PluginSpec {
                        repo: plugin_repo.clone(),
                        name: None,
                        source: None,
                    });
                }
            }
        }
        None => {
            let plugin_specs = plugin_repo_list
                .iter()
                .map(|plugin_repo| config::PluginSpec {
                    repo: plugin_repo.clone(),
                    name: None,
                    source: None,
                })
                .collect();
            config.plugins = Some(plugin_specs);
        }
    }
    config.save(&config_path.to_path_buf())?;

    Ok(())
}

async fn clone_plugins(
    plugin_repo_list: Vec<&PluginRepo>,
    force: bool,
    lock_file: LockFile,
    pez_data_dir: &path::Path,
) -> anyhow::Result<Vec<Plugin>> {
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
                    handle_existing_repository(&force, &plugin_repo, &repo_path).unwrap();
                }

                let source = &git::format_git_url(&plugin_repo_str);

                info!(
                    "{}Cloning repository from {} to {}",
                    Emoji("🔗 ", ""),
                    &source,
                    &repo_path.display()
                );
                let repo = git::clone_repository(source, &repo_path).unwrap();
                let name = &plugin_repo.repo;

                let new_plugin = match lock_file.lock().await.get_plugin(source) {
                    Some(lock_file_plugin) => {
                        info!(
                            "{}Checking out commit sha: {}",
                            Emoji("🔄 ", ""),
                            &lock_file_plugin.commit_sha
                        );
                        repo.set_head_detached(
                            git2::Oid::from_str(&lock_file_plugin.commit_sha).unwrap(),
                        )
                        .unwrap();

                        Plugin {
                            name: name.to_string(),
                            repo: plugin_repo.clone(),
                            source: source.to_string(),
                            commit_sha: lock_file_plugin.commit_sha.clone(),
                            files: vec![],
                        }
                    }
                    None => {
                        let commit_sha = git::get_latest_commit_sha(repo).unwrap();
                        Plugin {
                            name: name.to_string(),
                            repo: plugin_repo.clone(),
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

    future::join_all(clone_tasks).await;

    match Arc::try_unwrap(new_lock_plugins) {
        result::Result::Ok(new_lock_plugins) => Ok(new_lock_plugins.into_inner()),
        Err(_) => panic!("Failed to unwrap new_lock_plugins"),
    }
}

fn handle_existing_repository(
    force: &bool,
    repo: &PluginRepo,
    repo_path: &path::Path,
) -> anyhow::Result<()> {
    if *force {
        fs::remove_dir_all(repo_path)?;
    } else {
        anyhow::bail!(
            "{}{} Plugin already exists: {}, Use --force to reinstall",
            Emoji("❌ ", ""),
            console::style("Error:").red().bold(),
            repo.as_str()
        );
    }
    Ok(())
}

async fn sync_plugin_files(
    new_plugins: &mut [Plugin],
    pez_data_dir: &path::Path,
) -> anyhow::Result<Vec<Plugin>> {
    info!(
        "\n{}Copying plugin files to fish config directory...",
        Emoji("🐟 ", "")
    );
    let config_dir = utils::load_fish_config_dir()?;
    let target_dirs = TargetDir::all();

    let mut copy_tasks = Vec::new();

    let mut dest_paths = HashSet::new();

    for plugin in new_plugins.iter_mut() {
        let repo_path = pez_data_dir.join(plugin.repo.as_str());
        let mut target_files = Vec::new();
        let mut skip_plugin = false;

        info!("{}Copying files:", Emoji("📂 ", ""));
        for target_dir in &target_dirs {
            let target_dir_str = target_dir.as_str();
            let target_path = repo_path.join(target_dir_str);
            if !target_path.exists() {
                continue;
            }

            let file_type = match target_dir {
                TargetDir::Themes => ".theme",
                _ => ".fish",
            };
            let files = fs::read_dir(target_path)?.filter(|f| {
                f.as_ref().unwrap().file_type().unwrap().is_file()
                    && f.as_ref()
                        .unwrap()
                        .file_name()
                        .to_string_lossy()
                        .ends_with(file_type)
            });

            for file in files {
                let file_name = file?.file_name();
                let dest_path = config_dir.join(target_dir_str).join(&file_name);

                if dest_paths.contains(&dest_path) {
                    warn!(
                        "{} Skipping plugin due to duplicate: {}",
                        Emoji("🚨 ", ""),
                        plugin.repo
                    );
                    skip_plugin = true;
                    break;
                }

                info!("   - {}", dest_path.display());

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
                        fs::copy(&file_path, &dest_path).unwrap();
                    })
                    .await
                    .unwrap();
                }));
            });

            plugin.files = target_files.clone();
        }
    }

    futures::future::join_all(copy_tasks).await;
    Ok(new_plugins.to_vec())
}

fn install_all(force: &bool, prune: &bool) -> anyhow::Result<()> {
    let (mut lock_file, lock_file_path) = utils::load_or_create_lock_file()?;
    let (config, _) = utils::load_config()?;

    let plugin_specs = match config.plugins {
        Some(plugins) => plugins,
        None => {
            info!("No plugins found in pez.toml");
            vec![]
        }
    };

    for plugin_spec in plugin_specs.iter() {
        let source = git::format_git_url(&plugin_spec.repo.as_str());
        let repo_path = utils::load_pez_data_dir()?.join(plugin_spec.repo.as_str());

        info!(
            "\n{}Installing plugin: {}",
            Emoji("🐟 ", ""),
            &plugin_spec.repo
        );
        match lock_file.get_plugin(&source) {
            Some(locked_plugin) => {
                if repo_path.exists() {
                    info!(
                        "{}Skipped: {} is already installed.",
                        Emoji("⏭️  ", ""),
                        plugin_spec.repo
                    );

                    continue;
                }

                info!(
                    "{}Cloning repository from {} to {}",
                    Emoji("🔗 ", ""),
                    &source,
                    &repo_path.display()
                );
                let repo = git::clone_repository(&source, &repo_path)?;
                info!(
                    "{}Checking out commit sha: {}",
                    Emoji("🔄 ", ""),
                    &locked_plugin.commit_sha
                );
                repo.set_head_detached(git2::Oid::from_str(&locked_plugin.commit_sha)?)?;
                let mut plugin = Plugin {
                    name: plugin_spec.get_name()?,
                    repo: plugin_spec.repo.clone(),
                    source: source.to_string(),
                    commit_sha: locked_plugin.commit_sha.clone(),
                    files: vec![],
                };
                utils::copy_plugin_files_from_repo(&repo_path, &mut plugin)?;
                lock_file.update_plugin(plugin);
                lock_file.save(&lock_file_path)?;
            }
            None => {
                if repo_path.exists() {
                    if *force {
                        fs::remove_dir_all(&repo_path)?;
                    } else {
                        error!(
                            "{}{} Plugin already exists: {}, Use --force to reinstall",
                            Emoji("❌ ", ""),
                            console::style("Error:").red().bold(),
                            plugin_spec.repo
                        );
                        process::exit(1);
                    }
                }

                let repo = git2::Repository::clone(&source, &repo_path)?;
                let commit_sha = git::get_latest_commit_sha(repo)?;
                let mut plugin = Plugin {
                    name: plugin_spec.get_name()?,
                    repo: plugin_spec.repo.clone(),
                    source: source.to_string(),
                    commit_sha,
                    files: vec![],
                };
                utils::copy_plugin_files_from_repo(&repo_path, &mut plugin)?;

                lock_file.add_plugin(plugin);
                lock_file.save(&lock_file_path)?;
            }
        }
    }

    let ignored_lock_file_plugins = lock_file
        .plugins
        .iter()
        .filter(|p| {
            !plugin_specs
                .iter()
                .any(|spec| git::format_git_url(&spec.repo.as_str()) == p.source)
        })
        .cloned()
        .collect::<Vec<Plugin>>();

    if !ignored_lock_file_plugins.is_empty() {
        if *prune {
            for plugin in ignored_lock_file_plugins {
                info!("\n{}Removing plugin: {}", Emoji("🐟 ", ""), &plugin.name);
                let repo_path = utils::load_pez_data_dir()?.join(plugin.repo.as_str());
                if repo_path.exists() {
                    fs::remove_dir_all(&repo_path)?;
                } else {
                    warn!(
                        "{}Repository directory at {} does not exist.",
                        Emoji("🚧 ", ""),
                        &repo_path.display()
                    );

                    if !force {
                        info!(
                            "{}Detected plugin files based on pez-lock.toml:",
                            Emoji("📄 ", ""),
                        );
                        let fish_config_dir = utils::load_fish_config_dir()?;

                        plugin.files.iter().for_each(|file| {
                            let dest_path =
                                fish_config_dir.join(file.dir.as_str()).join(&file.name);
                            info!("   - {}", dest_path.display());
                        });
                        info!("If you want to remove these files, use the --force flag.");
                        continue;
                    }
                }

                info!(
                    "{}Removing plugin files based on pez-lock.toml:",
                    Emoji("🗑️  ", ""),
                );
                let fish_config_dir = utils::load_fish_config_dir()?;
                plugin.files.iter().for_each(|file| {
                    let dest_path = fish_config_dir.join(file.dir.as_str()).join(&file.name);
                    if dest_path.exists() {
                        info!("   - {}", &dest_path.display());
                        fs::remove_file(&dest_path).unwrap();
                    }
                    lock_file.remove_plugin(&plugin.source);
                    lock_file.save(&lock_file_path).unwrap();
                });
            }
        } else {
            info!("\nNotice: The following plugins are in pez-lock.toml but not in pez.toml:");
            for plugin in ignored_lock_file_plugins {
                info!("  - {}", plugin.name);
            }
            info!("If you want to remove them completely, please run:");
            info!("  pez install --prune");
            info!("or:");
            info!("  pez prune");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use config::PluginSpec;

    use super::*;
    use crate::tests_support::env::TestEnvironmentSetup;

    struct TestDataBuilder {
        new_plugin_spec: PluginSpec,
        added_plugin_spec: PluginSpec,
    }

    impl TestDataBuilder {
        fn new() -> Self {
            Self {
                new_plugin_spec: PluginSpec {
                    repo: PluginRepo {
                        owner: "owner".to_string(),
                        repo: "new-repo".to_string(),
                    },
                    name: None,
                    source: None,
                },
                added_plugin_spec: PluginSpec {
                    repo: PluginRepo {
                        owner: "owner".to_string(),
                        repo: "added-repo".to_string(),
                    },
                    name: None,
                    source: None,
                },
            }
        }
        fn build(self) -> TestData {
            TestData {
                new_plugin_spec: self.new_plugin_spec,
                added_plugin_spec: self.added_plugin_spec,
            }
        }
    }

    struct TestData {
        new_plugin_spec: PluginSpec,
        added_plugin_spec: PluginSpec,
    }

    #[test]
    fn test_add_plugin_in_empty_config() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config { plugins: None });

        let config = test_env.config.as_mut().expect("Config is not initialized");
        let plugin_repo_list = vec![test_data.new_plugin_spec.repo];

        let result = add_plugins_to_config(config, &test_env.config_path, &plugin_repo_list);
        assert!(result.is_ok());

        let updated_config = config::load(&test_env.config_path).unwrap();
        let updated_plugin_specs = updated_config.plugins.unwrap();
        assert_eq!(updated_plugin_specs.len(), 1);
        assert_eq!(updated_plugin_specs[0].repo.as_str(), "owner/new-repo");
    }

    #[test]
    fn test_add_existing_plugin_to_config() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config {
            plugins: Some(vec![test_data.added_plugin_spec.clone()]),
        });

        let config = test_env.config.as_mut().expect("Config is not initialized");
        assert_eq!(config.plugins.as_ref().unwrap().len(), 1);

        let plugin_repo_list = vec![test_data.added_plugin_spec.repo];

        let result = add_plugins_to_config(config, &test_env.config_path, &plugin_repo_list);
        assert!(result.is_ok());

        let updated_config = config::load(&test_env.config_path).unwrap();
        let updated_plugin_specs = updated_config.plugins.unwrap();
        assert_eq!(updated_plugin_specs.len(), 1);
        assert_eq!(updated_plugin_specs[0].repo.as_str(), "owner/added-repo");
    }

    #[test]
    fn test_add_new_plugin_to_existing_config() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config {
            plugins: Some(vec![test_data.added_plugin_spec.clone()]),
        });

        let config = test_env.config.as_mut().expect("Config is not initialized");
        assert_eq!(config.plugins.as_ref().unwrap().len(), 1);

        let plugin_repo_list = vec![test_data.new_plugin_spec.repo];

        let result = add_plugins_to_config(config, &test_env.config_path, &plugin_repo_list);
        assert!(result.is_ok());

        let updated_config = config::load(&test_env.config_path).unwrap();
        let updated_plugin_specs = updated_config.plugins.unwrap();
        assert_eq!(updated_plugin_specs.len(), 2);
        assert!(updated_plugin_specs
            .iter()
            .any(|p| p.repo.as_str() == "owner/added-repo"));
        assert!(updated_plugin_specs
            .iter()
            .any(|p| p.repo.as_str() == "owner/new-repo"));
    }

    #[test]
    fn test_handle_existing_repository_with_force() {
        let test_env = TestEnvironmentSetup::new();
        let repo = PluginRepo {
            owner: "owner".to_string(),
            repo: "repo".to_string(),
        };
        test_env.setup_data_repo(vec![repo.clone()]);
        let repo_path = test_env.data_dir.join(repo.as_str());

        let result = handle_existing_repository(&true, &repo, &repo_path);
        assert!(result.is_ok());
        assert!(!repo_path.exists());
    }

    #[test]
    fn test_repository_handling_without_force() {
        let test_env = TestEnvironmentSetup::new();
        let repo = PluginRepo {
            owner: "owner".to_string(),
            repo: "repo".to_string(),
        };
        test_env.setup_data_repo(vec![repo.clone()]);
        let repo_path = test_env.data_dir.join(repo.as_str());

        let result = handle_existing_repository(&false, &repo, &repo_path);
        assert!(result.is_err());
        assert!(repo_path.exists());
    }
}
