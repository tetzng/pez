use crate::resolver;
use crate::resolver::{ref_kind_to_repo_source, ref_kind_to_url_source};
use crate::{
    cli::{InstallArgs, InstallTarget, PluginRepo, ResolvedInstallTarget},
    config, git,
    lock_file::{LockFile, Plugin, PluginFile},
    models::TargetDir,
    utils,
};

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

async fn install(targets: &[InstallTarget], force: &bool) -> anyhow::Result<()> {
    let (mut config, config_path) = utils::load_or_create_config()?;
    add_plugins_to_config(&mut config, &config_path, targets)?;

    let (mut lock_file, lock_file_path) = utils::load_or_create_lock_file()?;

    let pez_data_dir = utils::load_pez_data_dir()?;
    let resolved: Vec<ResolvedInstallTarget> = targets
        .iter()
        .map(|t| t.resolve())
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut new_plugins =
        clone_plugins(&resolved, *force, lock_file.clone(), &pez_data_dir).await?;

    let new_plugins = sync_plugin_files(&mut new_plugins, &pez_data_dir).await?;

    for plugin in &new_plugins {
        emit_event(plugin, &utils::Event::Install)?;
    }

    lock_file.merge_plugins(new_plugins);
    lock_file.save(&lock_file_path)?;
    info!(
        "{}All plugins have been installed successfully!",
        Emoji("✅ ", "")
    );
    Ok(())
}

fn emit_event(plugin: &Plugin, event: &utils::Event) -> anyhow::Result<()> {
    plugin
        .files
        .iter()
        .filter(|f| f.dir == TargetDir::ConfD)
        .for_each(|f| {
            let _ = utils::emit_event(&f.name, event);
        });

    Ok(())
}

fn add_plugins_to_config(
    config: &mut config::Config,
    config_path: &path::Path,
    targets: &[InstallTarget],
) -> anyhow::Result<()> {
    let resolved: Vec<ResolvedInstallTarget> = targets
        .iter()
        .map(|t| t.resolve())
        .collect::<anyhow::Result<Vec<_>>>()?;
    match config.plugins {
        Some(ref mut plugin_specs) => {
            for r in &resolved {
                if !plugin_specs
                    .iter()
                    .any(|p| p.get_plugin_repo().is_ok_and(|pr| pr == r.plugin_repo))
                {
                    let default_source = format!("https://github.com/{}", r.plugin_repo.as_str());
                    let spec = if r.is_local {
                        config::PluginSpec {
                            name: None,
                            source: config::PluginSource::Path {
                                path: r.source.clone(),
                            },
                        }
                    } else if r.source == default_source {
                        config::PluginSpec {
                            name: None,
                            source: ref_kind_to_repo_source(&r.plugin_repo, &r.ref_kind),
                        }
                    } else {
                        config::PluginSpec {
                            name: None,
                            source: ref_kind_to_url_source(&r.source, &r.ref_kind),
                        }
                    };
                    plugin_specs.push(spec);
                }
            }
        }
        None => {
            let plugin_specs = resolved
                .into_iter()
                .map(|r| {
                    let default_source = format!("https://github.com/{}", r.plugin_repo.as_str());
                    if r.is_local {
                        config::PluginSpec {
                            name: None,
                            source: config::PluginSource::Path { path: r.source },
                        }
                    } else if r.source == default_source {
                        config::PluginSpec {
                            name: None,
                            source: ref_kind_to_repo_source(&r.plugin_repo, &r.ref_kind),
                        }
                    } else {
                        config::PluginSpec {
                            name: None,
                            source: ref_kind_to_url_source(&r.source, &r.ref_kind),
                        }
                    }
                })
                .collect();
            config.plugins = Some(plugin_specs);
        }
    }
    config.save(&config_path.to_path_buf())?;

    Ok(())
}

async fn clone_plugins(
    resolved_targets: &[ResolvedInstallTarget],
    force: bool,
    lock_file: LockFile,
    pez_data_dir: &path::Path,
) -> anyhow::Result<Vec<Plugin>> {
    let lock_file = Arc::new(Mutex::new(lock_file));
    let new_lock_plugins: Arc<Mutex<Vec<Plugin>>> = Arc::new(Mutex::new(vec![]));

    let clone_tasks: Vec<_> = resolved_targets
        .iter()
        .cloned()
        .map(|resolved| {
            let new_lock_plugins = Arc::clone(&new_lock_plugins);
            let lock_file = Arc::clone(&lock_file);
            let pez_data_dir = pez_data_dir.to_path_buf();

            tokio::spawn(async move {
                let plugin_repo = resolved.plugin_repo.clone();
                let plugin_repo_str = plugin_repo.as_str();
                let repo_path = pez_data_dir.join(&plugin_repo_str);

                if repo_path.exists()
                    && let Err(e) = handle_existing_repository(&force, &plugin_repo, &repo_path)
                {
                    warn!(
                        "Failed to prepare existing repository {}: {:?}",
                        repo_path.display(),
                        e
                    );
                    return; // skip this plugin task on error
                }

                let base_source = resolved.source.clone();

                let repo_path_display = repo_path.display();
                info!(
                    "{}Cloning repository from {} to {}",
                    Emoji("🔗 ", ""),
                    &base_source,
                    repo_path_display
                );
                if resolved.is_local {
                    // Local source; skip clone. We'll copy files from `base_source` later in sync.
                    let name = &plugin_repo.repo;
                    let new_plugin = Plugin {
                        name: name.to_string(),
                        repo: plugin_repo.clone(),
                        source: base_source.clone(),
                        commit_sha: "local".to_string(),
                        files: vec![],
                    };
                    new_lock_plugins.lock().await.push(new_plugin);
                    return;
                }

                let repo = match git::clone_repository(&base_source, &repo_path) {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(
                            "Failed to clone {} to {}: {:?}",
                            base_source,
                            repo_path.display(),
                            e
                        );
                        return;
                    }
                };
                let name = &plugin_repo.repo;
                let new_plugin = {
                    let locked_opt = lock_file
                        .lock()
                        .await
                        .get_plugin_by_repo(&plugin_repo)
                        .cloned();
                    if let Some(lock_file_plugin) = locked_opt {
                        if !force {
                            info!(
                                "{}Checking out commit sha: {}",
                                Emoji("🔄 ", ""),
                                &lock_file_plugin.commit_sha
                            );
                            if let Ok(oid) = git2::Oid::from_str(&lock_file_plugin.commit_sha) {
                                if let Err(e) = repo.set_head_detached(oid) {
                                    warn!(
                                        "Failed to detach HEAD to {}: {:?}",
                                        lock_file_plugin.commit_sha, e
                                    );
                                }
                            } else {
                                warn!(
                                    "Invalid commit SHA in lock file: {}",
                                    lock_file_plugin.commit_sha
                                );
                            }
                            Plugin {
                                name: name.to_string(),
                                repo: plugin_repo.clone(),
                                source: base_source.clone(),
                                commit_sha: lock_file_plugin.commit_sha.clone(),
                                files: vec![],
                            }
                        } else {
                            // force: resolve newest according to ref_kind
                            let sel = resolver::selection_from_ref_kind(&resolved.ref_kind);
                            let commit_sha = match git::resolve_selection(&repo, &sel) {
                                std::result::Result::Ok(sha) => sha,
                                Err(e) => {
                                    warn!(
                                        "Failed to resolve selection: {:?}. Falling back to HEAD.",
                                        e
                                    );
                                    match git::get_latest_commit_sha(repo) {
                                        Ok(s) => s,
                                        Err(e) => {
                                            warn!("Failed to read HEAD commit: {:?}", e);
                                            return;
                                        }
                                    }
                                }
                            };
                            Plugin {
                                name: name.to_string(),
                                repo: plugin_repo.clone(),
                                source: base_source.clone(),
                                commit_sha,
                                files: vec![],
                            }
                        }
                    } else {
                        // fresh install: resolve selection
                        let sel = resolver::selection_from_ref_kind(&resolved.ref_kind);
                        let commit_sha = match git::resolve_selection(&repo, &sel) {
                            std::result::Result::Ok(sha) => sha,
                            Err(e) => {
                                warn!(
                                    "Failed to resolve selection: {:?}. Falling back to HEAD.",
                                    e
                                );
                                git::get_latest_commit_sha(repo).unwrap()
                            }
                        };
                        Plugin {
                            name: name.to_string(),
                            repo: plugin_repo.clone(),
                            source: base_source.clone(),
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

    let new_lock_plugins_result = Arc::try_unwrap(new_lock_plugins);

    match new_lock_plugins_result {
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
        let repo_path = if git::is_local_source(&plugin.source) {
            path::PathBuf::from(&plugin.source)
        } else {
            pez_data_dir.join(plugin.repo.as_str())
        };
        let mut target_files = Vec::new();
        let mut skip_plugin = false;

        info!("{}Copying files:", Emoji("📂 ", ""));
        for target_dir in &target_dirs {
            let target_dir_str = target_dir.as_str();
            let target_path = repo_path.join(target_dir_str);
            if !target_path.exists() {
                continue;
            }

            // Recursively walk and filter by extension
            let expected_ext = match target_dir {
                TargetDir::Themes => Some("theme"),
                _ => Some("fish"),
            };

            for entry in walkdir::WalkDir::new(&target_path)
                .into_iter()
                .filter_map(Result::ok)
            {
                if entry.file_type().is_dir() {
                    continue;
                }
                if let Some(ext) = expected_ext
                    && entry.path().extension().and_then(|s| s.to_str()) != Some(ext)
                {
                    continue;
                }

                let rel = match entry.path().strip_prefix(&target_path) {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                let dest_path = config_dir.join(target_dir_str).join(rel);

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
                    name: rel.to_string_lossy().to_string(),
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
                        if let Some(parent) = dest_path.parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        let _ = fs::copy(&file_path, &dest_path);
                    })
                    .await
                    .ok();
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
        let resolved = plugin_spec.to_resolved()?;
        let repo_for_id = resolved.plugin_repo.clone();
        let source_base = resolved.source.clone();
        let ref_kind = resolved.ref_kind.clone();
        let repo_path = utils::load_pez_data_dir()?.join(repo_for_id.as_str());

        info!("\n{}Installing plugin: {}", Emoji("🐟 ", ""), &repo_for_id);
        match lock_file.get_plugin_by_repo(&repo_for_id) {
            Some(locked_plugin) => {
                if repo_path.exists() && !*force {
                    info!(
                        "{}Skipped: {} is already installed.",
                        Emoji("⏭️  ", ""),
                        repo_for_id
                    );

                    continue;
                }

                let repo_path_display = repo_path.display();
                info!(
                    "{}Cloning repository from {} to {}",
                    Emoji("🔗 ", ""),
                    &source_base,
                    repo_path_display
                );
                // For local path sources, cloning is not applicable
                let repo = if git::is_local_source(&source_base) {
                    None
                } else {
                    Some(git::clone_repository(&source_base, &repo_path)?)
                };
                let commit_sha = if *force {
                    if let Some(repo) = &repo {
                        let sel = resolver::selection_from_ref_kind(&ref_kind);
                        match git::resolve_selection(repo, &sel) {
                            std::result::Result::Ok(sha) => sha,
                            Err(e) => {
                                warn!(
                                    "Failed to resolve selection: {:?}. Falling back to pinned.",
                                    e
                                );
                                locked_plugin.commit_sha.clone()
                            }
                        }
                    } else {
                        "local".to_string()
                    }
                } else {
                    if let Some(repo) = &repo {
                        info!(
                            "{}Checking out commit sha: {}",
                            Emoji("🔄 ", ""),
                            &locked_plugin.commit_sha
                        );
                        if let Ok(oid) = git2::Oid::from_str(&locked_plugin.commit_sha) {
                            let _ = repo.set_head_detached(oid);
                        }
                    }
                    locked_plugin.commit_sha.clone()
                };
                let mut plugin = Plugin {
                    name: plugin_spec.get_name()?,
                    repo: repo_for_id.clone(),
                    source: source_base.to_string(),
                    commit_sha,
                    files: vec![],
                };
                if git::is_local_source(&source_base) {
                    utils::copy_plugin_files_from_repo(path::Path::new(&source_base), &mut plugin)?;
                } else {
                    utils::copy_plugin_files_from_repo(&repo_path, &mut plugin)?;
                }
                emit_event(&plugin, &utils::Event::Install)?;

                if let Err(e) = lock_file.update_plugin(plugin) {
                    warn!("Failed to update lock file entry: {:?}", e);
                }
                lock_file.save(&lock_file_path)?;
            }
            None => {
                if repo_path.exists() && !git::is_local_source(&source_base) {
                    if *force {
                        fs::remove_dir_all(&repo_path)?;
                    } else {
                        error!(
                            "{}{} Plugin already exists: {}, Use --force to reinstall",
                            Emoji("❌ ", ""),
                            console::style("Error:").red().bold(),
                            repo_for_id
                        );
                        process::exit(1);
                    }
                }

                let commit_sha = if git::is_local_source(&source_base) {
                    info!(
                        "{}Installing from local path: {}",
                        Emoji("📁 ", ""),
                        &source_base
                    );
                    "local".to_string()
                } else {
                    let repo = git::clone_repository(&source_base, &repo_path)?;
                    let sel = resolver::selection_from_ref_kind(&ref_kind);
                    match git::resolve_selection(&repo, &sel) {
                        std::result::Result::Ok(sha) => sha,
                        Err(e) => {
                            warn!(
                                "Failed to resolve selection: {:?}. Falling back to HEAD.",
                                e
                            );
                            git::get_latest_commit_sha(repo)?
                        }
                    }
                };
                let mut plugin = Plugin {
                    name: plugin_spec.get_name()?,
                    repo: repo_for_id.clone(),
                    source: source_base.to_string(),
                    commit_sha,
                    files: vec![],
                };
                if git::is_local_source(&source_base) {
                    utils::copy_plugin_files_from_repo(path::Path::new(&source_base), &mut plugin)?;
                } else {
                    utils::copy_plugin_files_from_repo(&repo_path, &mut plugin)?;
                }
                emit_event(&plugin, &utils::Event::Install)?;

                if let Err(e) = lock_file.add_plugin(plugin) {
                    warn!("Failed to add lock file entry: {:?}", e);
                }
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
                .any(|spec| spec.get_plugin_repo().is_ok_and(|r| r == p.repo))
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
                    let path_display = repo_path.display();
                    warn!(
                        "{}Repository directory at {} does not exist.",
                        Emoji("🚧 ", ""),
                        path_display
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

                emit_event(&plugin, &utils::Event::Uninstall)?;

                let fish_config_dir = utils::load_fish_config_dir()?;
                plugin.files.iter().for_each(|file| {
                    let dest_path = fish_config_dir.join(file.dir.as_str()).join(&file.name);
                    if dest_path.exists() {
                        let path_display = dest_path.display();
                        info!("   - {}", path_display);
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
    use config::{PluginSource, PluginSpec};

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
                    name: None,
                    source: PluginSource::Repo {
                        repo: PluginRepo {
                            owner: "owner".to_string(),
                            repo: "new-repo".to_string(),
                        },
                        version: None,
                        branch: None,
                        tag: None,
                        commit: None,
                    },
                },
                added_plugin_spec: PluginSpec {
                    name: None,
                    source: PluginSource::Repo {
                        repo: PluginRepo {
                            owner: "owner".to_string(),
                            repo: "added-repo".to_string(),
                        },
                        version: None,
                        branch: None,
                        tag: None,
                        commit: None,
                    },
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
        #[allow(dead_code)]
        new_plugin_spec: PluginSpec,
        added_plugin_spec: PluginSpec,
    }

    #[test]
    fn test_add_plugin_in_empty_config() {
        let mut test_env = TestEnvironmentSetup::new();
        let _test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config { plugins: None });

        let config = test_env.config.as_mut().expect("Config is not initialized");
        let targets = vec![crate::cli::InstallTarget::from_raw("owner/new-repo")];

        let result = add_plugins_to_config(config, &test_env.config_path, &targets);
        assert!(result.is_ok());

        let updated_config = config::load(&test_env.config_path).unwrap();
        let updated_plugin_specs = updated_config.plugins.unwrap();
        assert_eq!(updated_plugin_specs.len(), 1);
        assert_eq!(
            updated_plugin_specs[0].get_plugin_repo().unwrap().as_str(),
            "owner/new-repo"
        );
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

        let targets = vec![crate::cli::InstallTarget::from_raw("owner/added-repo")];

        let result = add_plugins_to_config(config, &test_env.config_path, &targets);
        assert!(result.is_ok());

        let updated_config = config::load(&test_env.config_path).unwrap();
        let updated_plugin_specs = updated_config.plugins.unwrap();
        assert_eq!(updated_plugin_specs.len(), 1);
        assert_eq!(
            updated_plugin_specs[0].get_plugin_repo().unwrap().as_str(),
            "owner/added-repo"
        );
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

        let targets = vec![crate::cli::InstallTarget::from_raw("owner/new-repo")];

        let result = add_plugins_to_config(config, &test_env.config_path, &targets);
        assert!(result.is_ok());

        let updated_config = config::load(&test_env.config_path).unwrap();
        let updated_plugin_specs = updated_config.plugins.unwrap();
        assert_eq!(updated_plugin_specs.len(), 2);
        assert!(updated_plugin_specs.iter().any(|p| {
            p.get_plugin_repo()
                .map(|r| r.as_str() == "owner/added-repo")
                .unwrap_or(false)
        }));
        assert!(updated_plugin_specs.iter().any(|p| {
            p.get_plugin_repo()
                .map(|r| r.as_str() == "owner/new-repo")
                .unwrap_or(false)
        }));
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
