use crate::resolver;
use crate::{
    cli::InstallArgs,
    config, git,
    lock_file::{LockFile, Plugin},
    models::TargetDir,
    models::{InstallTarget, PluginRepo, ResolvedInstallTarget},
    utils,
};

use anyhow::Context;
use console::Emoji;
use futures::{StreamExt, stream};
use std::{collections::HashSet, fs, path, result, sync::Arc};
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

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

fn ensure_repo_parent(repo_path: &path::Path) -> anyhow::Result<()> {
    if let Some(parent) = repo_path.parent()
        && !parent.exists()
    {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create parent directory for {}",
                repo_path.display()
            )
        })?;
    }
    Ok(())
}

fn add_plugins_to_config(
    config: &mut config::Config,
    config_path: &path::Path,
    targets: &[InstallTarget],
) -> anyhow::Result<()> {
    let mut changed = false;
    for target in targets {
        let resolved = target.resolve()?;
        if config.ensure_plugin_from_resolved(&resolved) {
            changed = true;
        }
    }

    if changed {
        config.save(&config_path.to_path_buf())?;
    }

    Ok(())
}

enum ExistingRepoPolicy {
    CliInstall,
    InstallAll,
}

enum PreparedInstall {
    Prepared {
        plugin: Plugin,
        repo_base: path::PathBuf,
    },
    Skipped,
}

async fn clone_plugins(
    resolved_targets: &[ResolvedInstallTarget],
    force: bool,
    lock_file: LockFile,
    pez_data_dir: &path::Path,
) -> anyhow::Result<Vec<Plugin>> {
    let lock_file = Arc::new(Mutex::new(lock_file));
    let new_lock_plugins: Arc<Mutex<Vec<Plugin>>> = Arc::new(Mutex::new(vec![]));

    let jobs = utils::load_jobs().max(1);
    stream::iter(resolved_targets.iter().cloned())
        .for_each_concurrent(jobs, |resolved| {
            let new_lock_plugins = Arc::clone(&new_lock_plugins);
            let lock_file = Arc::clone(&lock_file);
            let pez_data_dir = pez_data_dir.to_path_buf();
            async move {
                let plugin_repo = resolved.plugin_repo.clone();
                let locked_opt = lock_file
                    .lock()
                    .await
                    .get_plugin_by_repo(&plugin_repo)
                    .cloned();
                let plugin_name = plugin_repo.repo.clone();

                match prepare_plugin_from_resolved(
                    &plugin_name,
                    &resolved,
                    locked_opt.as_ref(),
                    force,
                    &pez_data_dir,
                    ExistingRepoPolicy::CliInstall,
                ) {
                    Ok(PreparedInstall::Prepared { plugin, .. }) => {
                        new_lock_plugins.lock().await.push(plugin);
                    }
                    Ok(PreparedInstall::Skipped) => {}
                    Err(e) => {
                        warn!("Failed to prepare plugin {}: {:?}", plugin_repo, e);
                    }
                }
            }
        })
        .await;

    let new_lock_plugins_result = Arc::try_unwrap(new_lock_plugins);

    match new_lock_plugins_result {
        result::Result::Ok(new_lock_plugins) => Ok(new_lock_plugins.into_inner()),
        Err(_) => anyhow::bail!("Internal error: pending references to new_lock_plugins remain"),
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
            "{} {} Plugin already exists: {}. Use --force to reinstall",
            Emoji("❌ ", ""),
            crate::utils::label_error(),
            repo.as_str()
        );
    }
    Ok(())
}

fn prepare_plugin_from_resolved(
    plugin_name: &str,
    resolved: &ResolvedInstallTarget,
    locked_plugin: Option<&Plugin>,
    force: bool,
    pez_data_dir: &path::Path,
    existing_repo_policy: ExistingRepoPolicy,
) -> anyhow::Result<PreparedInstall> {
    let repo_for_id = resolved.plugin_repo.clone();
    let source_base = resolved.source.clone();
    let ref_kind = resolved.ref_kind.clone();
    let repo_path = pez_data_dir.join(repo_for_id.as_str());
    let is_local_source = git::is_local_source(&source_base);

    match existing_repo_policy {
        ExistingRepoPolicy::CliInstall => {
            if repo_path.exists() {
                handle_existing_repository(&force, &repo_for_id, &repo_path)?;
            }
        }
        ExistingRepoPolicy::InstallAll => {
            if let Some(_locked) = locked_plugin
                && repo_path.exists()
                && !force
            {
                info!(
                    "{}Skipped: {} is already installed.",
                    Emoji("⏭️  ", ""),
                    repo_for_id
                );
                return Ok(PreparedInstall::Skipped);
            }

            if repo_path.exists() && !is_local_source {
                if force {
                    fs::remove_dir_all(&repo_path).with_context(|| {
                        format!("failed to remove existing repo at {}", repo_path.display())
                    })?;
                } else if locked_plugin.is_none() {
                    anyhow::bail!(
                        "Plugin already exists: {} (path: {}). Use --force to reinstall",
                        repo_for_id,
                        repo_path.display()
                    );
                }
            }
        }
    }

    let repo = if is_local_source {
        None
    } else {
        info!(
            "{}Cloning repository from {} to {}",
            Emoji("🔗 ", ""),
            &source_base,
            repo_path.display()
        );
        ensure_repo_parent(&repo_path)?;
        Some(
            git::clone_repository(&source_base, &repo_path).with_context(|| {
                format!(
                    "failed to clone {} into {}",
                    &source_base,
                    repo_path.display()
                )
            })?,
        )
    };

    let commit_sha = if let Some(locked) = locked_plugin {
        if force {
            if let Some(repo) = &repo {
                let sel = resolver::selection_from_ref_kind(&ref_kind);
                match git::resolve_selection(repo, &sel) {
                    std::result::Result::Ok(sha) => sha,
                    Err(e) => {
                        warn!(
                            "Failed to resolve selection: {:?}. Falling back to HEAD.",
                            e
                        );
                        git::get_latest_commit_sha(repo)?
                    }
                }
            } else {
                "local".to_string()
            }
        } else {
            if let Some(repo) = &repo {
                info!(
                    "{}Using pinned commit: {}",
                    Emoji("🔄 ", ""),
                    &locked.commit_sha
                );
                git::checkout_commit(repo, &locked.commit_sha).with_context(|| {
                    format!(
                        "failed to checkout pinned commit {} for repository {}",
                        &locked.commit_sha, &source_base
                    )
                })?;
            }
            locked.commit_sha.clone()
        }
    } else if is_local_source {
        info!(
            "{}Installing from local path: {}",
            Emoji("📁 ", ""),
            &source_base
        );
        "local".to_string()
    } else {
        let repo = repo
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("expected cloned repository for remote source"))?;
        let sel = resolver::selection_from_ref_kind(&ref_kind);
        let commit_sha = match git::resolve_selection(repo, &sel) {
            std::result::Result::Ok(sha) => sha,
            Err(e) => {
                warn!(
                    "Failed to resolve selection: {:?}. Falling back to HEAD.",
                    e
                );
                git::get_latest_commit_sha(repo)?
            }
        };
        if let Err(e) = git::checkout_commit(repo, &commit_sha) {
            warn!("Failed to detach HEAD to {}: {:?}", &commit_sha, e);
        }
        commit_sha
    };

    if locked_plugin.is_some()
        && force
        && let Some(repo) = &repo
        && let Err(e) = git::checkout_commit(repo, &commit_sha)
    {
        warn!("Failed to detach HEAD to {}: {:?}", &commit_sha, e);
    }

    debug!(
        repo = %repo_for_id,
        source = %source_base,
        commit = %commit_sha,
        "Install resolved commit"
    );

    let plugin = Plugin {
        name: plugin_name.to_string(),
        repo: repo_for_id,
        source: source_base.clone(),
        commit_sha,
        files: vec![],
    };

    let repo_base = if is_local_source {
        path::PathBuf::from(&source_base)
    } else {
        repo_path
    };

    Ok(PreparedInstall::Prepared { plugin, repo_base })
}

enum CopyStrategy {
    Dedupe,
    Direct,
}

fn copy_prepared_plugin_files(
    plugin: &mut Plugin,
    repo_base: &path::Path,
    fish_config_dir: &path::Path,
    dest_paths: Option<&mut HashSet<path::PathBuf>>,
    copy_strategy: CopyStrategy,
) -> anyhow::Result<()> {
    match copy_strategy {
        CopyStrategy::Dedupe => {
            info!("{}Copying files:", Emoji("📂 ", ""));
            let outcome =
                utils::copy_plugin_files(repo_base, fish_config_dir, plugin, dest_paths, true)?;
            if outcome.skipped_due_to_duplicate {
                warn!(
                    "{} Skipping plugin due to duplicate: {}",
                    Emoji("🚨 ", ""),
                    plugin.repo
                );
                plugin.files.clear();
            }
            Ok(())
        }
        CopyStrategy::Direct => {
            utils::copy_plugin_files_from_repo(repo_base, plugin)?;
            Ok(())
        }
    }
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
    let mut dest_paths: HashSet<path::PathBuf> = HashSet::new();

    for plugin in new_plugins.iter_mut() {
        let repo_path = if git::is_local_source(&plugin.source) {
            path::PathBuf::from(&plugin.source)
        } else {
            pez_data_dir.join(plugin.repo.as_str())
        };

        copy_prepared_plugin_files(
            plugin,
            &repo_path,
            &config_dir,
            Some(&mut dest_paths),
            CopyStrategy::Dedupe,
        )?;
    }

    Ok(new_plugins.to_vec())
}

enum InstallOutcome {
    Installed(Plugin),
    Skipped,
}

fn install_resolved_target(
    plugin_spec: &config::PluginSpec,
    resolved: &ResolvedInstallTarget,
    locked_plugin: Option<&Plugin>,
    force: bool,
    pez_data_dir: &path::Path,
    fish_config_dir: &path::Path,
    dest_paths: &mut HashSet<path::PathBuf>,
) -> anyhow::Result<InstallOutcome> {
    let repo_for_id = resolved.plugin_repo.clone();
    let plugin_name = plugin_spec.get_name()?;

    info!("\n{}Installing plugin: {}", Emoji("🐟 ", ""), &repo_for_id);

    let prepared = prepare_plugin_from_resolved(
        &plugin_name,
        resolved,
        locked_plugin,
        force,
        pez_data_dir,
        ExistingRepoPolicy::InstallAll,
    )?;

    let (mut plugin, repo_base) = match prepared {
        PreparedInstall::Prepared { plugin, repo_base } => (plugin, repo_base),
        PreparedInstall::Skipped => return Ok(InstallOutcome::Skipped),
    };

    if locked_plugin.is_some() {
        copy_prepared_plugin_files(
            &mut plugin,
            &repo_base,
            fish_config_dir,
            Some(dest_paths),
            CopyStrategy::Dedupe,
        )?;
    } else {
        copy_prepared_plugin_files(
            &mut plugin,
            &repo_base,
            fish_config_dir,
            None,
            CopyStrategy::Direct,
        )?;
    }

    emit_event(&plugin, &utils::Event::Install)?;
    Ok(InstallOutcome::Installed(plugin))
}

fn install_all(force: &bool, prune: &bool) -> anyhow::Result<()> {
    let (mut lock_file, lock_file_path) = utils::load_or_create_lock_file()?;
    let (config, _) = utils::load_config()?;
    let pez_data_dir = utils::load_pez_data_dir()?;
    let fish_config_dir = utils::load_fish_config_dir()?;

    let plugin_specs = match config.plugins {
        Some(plugins) => plugins,
        None => {
            info!("No plugins found in pez.toml");
            vec![]
        }
    };

    // Track destination paths we've populated to detect duplicates across plugins
    let mut dest_paths: HashSet<path::PathBuf> = HashSet::new();

    for plugin_spec in plugin_specs.iter() {
        let resolved = plugin_spec.to_resolved()?;
        let repo_for_id = resolved.plugin_repo.clone();
        let outcome = install_resolved_target(
            plugin_spec,
            &resolved,
            lock_file.get_plugin_by_repo(&repo_for_id),
            *force,
            &pez_data_dir,
            &fish_config_dir,
            &mut dest_paths,
        )?;
        if let InstallOutcome::Installed(plugin) = outcome {
            if let Err(e) = lock_file.upsert_plugin_by_repo(plugin) {
                warn!("Failed to update lock file entry: {:?}", e);
            }
            lock_file.save(&lock_file_path)?;
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
                info!("{}Removing plugin: {}", Emoji("🐟 ", ""), &plugin.name);
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
                for file in &plugin.files {
                    let dest_path = fish_config_dir.join(file.dir.as_str()).join(&file.name);
                    if dest_path.exists()
                        && let Err(e) = fs::remove_file(&dest_path)
                    {
                        warn!("Failed to remove {}: {:?}", dest_path.display(), e);
                    }
                }
                lock_file.remove_plugin(&plugin.source);
                if let Err(e) = lock_file.save(&lock_file_path) {
                    warn!("Failed to save lock file: {:?}", e);
                }
            }
        } else {
            info!(
                "{} The following plugins are in pez-lock.toml but not in pez.toml:",
                crate::utils::label_notice()
            );
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
    use crate::lock_file::PluginFile;
    use crate::tests_support::env::TestEnvironmentSetup;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    struct EnvOverride {
        entries: Vec<(&'static str, Option<std::ffi::OsString>)>,
    }

    impl EnvOverride {
        fn new(keys: &[&'static str]) -> Self {
            let entries = keys
                .iter()
                .map(|&key| (key, std::env::var_os(key)))
                .collect();
            Self { entries }
        }
    }

    impl Drop for EnvOverride {
        fn drop(&mut self) {
            for (key, value) in &self.entries {
                if let Some(v) = value {
                    unsafe {
                        std::env::set_var(key, v);
                    }
                } else {
                    unsafe {
                        std::env::remove_var(key);
                    }
                }
            }
        }
    }

    fn set_test_env_vars(test_env: &TestEnvironmentSetup) {
        unsafe {
            std::env::set_var("PEZ_CONFIG_DIR", &test_env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &test_env.data_dir);
            std::env::set_var("PEZ_TARGET_DIR", &test_env.fish_config_dir);
            std::env::set_var("HOME", test_env._temp_dir.path());
            std::env::remove_var("__fish_config_dir");
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::remove_var("__fish_user_data_dir");
            std::env::remove_var("XDG_DATA_HOME");
        }
    }

    fn init_remote_repo(path: &Path) -> String {
        std::fs::create_dir_all(path).unwrap();
        let repo = git2::Repository::init(path).unwrap();
        let conf_dir = path.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&conf_dir).unwrap();
        std::fs::write(conf_dir.join("force-test.fish"), "echo force test\n").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new("conf.d/force-test.fish")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("pez", "pez@example.com").unwrap();
        let commit_id = repo
            .commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();
        commit_id.to_string()
    }

    fn init_remote_repo_with_conf_file(path: &Path, conf_file_name: &str) -> String {
        std::fs::create_dir_all(path).unwrap();
        let repo = git2::Repository::init(path).unwrap();
        let conf_dir = path.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&conf_dir).unwrap();
        std::fs::write(conf_dir.join(conf_file_name), "echo host test\n").unwrap();

        let rel_path = Path::new("conf.d").join(conf_file_name);
        let mut index = repo.index().unwrap();
        index.add_path(&rel_path).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("pez", "pez@example.com").unwrap();
        let commit_id = repo
            .commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();
        commit_id.to_string()
    }

    fn commit_file(repo: &git2::Repository, rel_path: &Path, message: &str) -> String {
        let mut index = repo.index().unwrap();
        index.add_path(rel_path).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("pez", "pez@example.com").unwrap();
        let parent = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| repo.find_commit(oid).ok());
        let commit_id = match parent {
            Some(ref parent) => repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[parent])
                .unwrap(),
            None => repo
                .commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
                .unwrap(),
        };
        commit_id.to_string()
    }

    fn init_remote_repo_with_two_commits(path: &Path) -> (String, String) {
        std::fs::create_dir_all(path).unwrap();
        let repo = git2::Repository::init(path).unwrap();
        let conf_dir = path.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&conf_dir).unwrap();
        let rel_path = Path::new("conf.d/sequence-test.fish");
        std::fs::write(path.join(rel_path), "echo one\n").unwrap();
        let first = commit_file(&repo, rel_path, "first commit");
        std::fs::write(path.join(rel_path), "echo two\n").unwrap();
        let second = commit_file(&repo, rel_path, "second commit");
        (first, second)
    }

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
                            host: None,
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
                            host: None,
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
        let targets = vec![crate::models::InstallTarget::from_raw("owner/new-repo")];

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

        let targets = vec![crate::models::InstallTarget::from_raw("owner/added-repo")];

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

        let targets = vec![crate::models::InstallTarget::from_raw("owner/new-repo")];

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
            host: None,
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
            host: None,
            owner: "owner".to_string(),
            repo: "repo".to_string(),
        };
        test_env.setup_data_repo(vec![repo.clone()]);
        let repo_path = test_env.data_dir.join(repo.as_str());

        let result = handle_existing_repository(&false, &repo, &repo_path);
        assert!(result.is_err());
        assert!(repo_path.exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn run_installs_local_plugin_and_updates_lock() {
        let _env_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let test_env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "__fish_user_data_dir",
            "XDG_DATA_HOME",
            "HOME",
            "PEZ_SUPPRESS_EMIT",
        ]);

        let source_dir = test_env._temp_dir.path().join("local-plugin");
        let conf_dir = source_dir.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&conf_dir).unwrap();
        std::fs::write(conf_dir.join("local-plugin.fish"), "echo local\n").unwrap();

        set_test_env_vars(&test_env);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
        }

        let args = InstallArgs {
            plugins: Some(vec![InstallTarget::from_raw(
                source_dir.to_string_lossy().to_string(),
            )]),
            force: false,
            prune: false,
        };

        tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(run(&args)))
            .unwrap();

        let updated_config = config::load(&test_env.config_path).unwrap();
        let plugin_specs = updated_config.plugins.unwrap();
        assert_eq!(plugin_specs.len(), 1);
        let repo = plugin_specs[0].get_plugin_repo().unwrap();
        assert_eq!(repo.owner, "local");
        assert_eq!(repo.repo, "local-plugin");

        let saved_lock = crate::lock_file::load(&test_env.lock_file_path).unwrap();
        let locked_plugin = saved_lock.get_plugin_by_repo(&repo).unwrap();
        assert_eq!(locked_plugin.commit_sha, "local");

        let fish_file = test_env
            .fish_config_dir
            .join(TargetDir::ConfD.as_str())
            .join("local-plugin.fish");
        assert!(fish_file.exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn run_install_fails_when_target_dir_is_file() {
        let _env_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let test_env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "__fish_user_data_dir",
            "XDG_DATA_HOME",
            "HOME",
            "PEZ_SUPPRESS_EMIT",
        ]);

        let source_dir = test_env._temp_dir.path().join("local-plugin-fail");
        let conf_dir = source_dir.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&conf_dir).unwrap();
        std::fs::write(conf_dir.join("local-plugin-fail.fish"), "echo local\n").unwrap();

        std::fs::remove_dir_all(&test_env.fish_config_dir).unwrap();
        std::fs::write(&test_env.fish_config_dir, "not-a-directory").unwrap();

        set_test_env_vars(&test_env);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
        }

        let args = InstallArgs {
            plugins: Some(vec![InstallTarget::from_raw(
                source_dir.to_string_lossy().to_string(),
            )]),
            force: false,
            prune: false,
        };

        let result =
            tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(run(&args)));
        assert!(
            result.is_err(),
            "install should fail when target dir is not a directory"
        );
    }

    #[test]
    fn run_installs_multi_host_same_owner_repo_with_distinct_paths_and_lock_rows() {
        let _env_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let test_env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "__fish_user_data_dir",
            "XDG_DATA_HOME",
            "HOME",
            "PEZ_SUPPRESS_EMIT",
        ]);

        let remote_root = test_env._temp_dir.path().join("remotes");
        let github_repo_path = remote_root.join("github.com").join("owner").join("repo");
        let gitlab_repo_path = remote_root.join("gitlab.com").join("owner").join("repo");
        init_remote_repo_with_conf_file(&github_repo_path, "github-repo.fish");
        init_remote_repo_with_conf_file(&gitlab_repo_path, "gitlab-repo.fish");

        set_test_env_vars(&test_env);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
        }

        let mut github_target = InstallTarget::from_raw("github.com/owner/repo")
            .resolve()
            .unwrap();
        let mut gitlab_target = InstallTarget::from_raw("gitlab.com/owner/repo")
            .resolve()
            .unwrap();
        github_target.source = format!("file://{}", github_repo_path.display());
        gitlab_target.source = format!("file://{}", gitlab_repo_path.display());

        let rt = tokio::runtime::Runtime::new().unwrap();
        let mut cloned_plugins = rt
            .block_on(clone_plugins(
                &[github_target, gitlab_target],
                false,
                LockFile {
                    version: 1,
                    plugins: vec![],
                },
                &test_env.data_dir,
            ))
            .unwrap();
        let installed_plugins = rt
            .block_on(sync_plugin_files(&mut cloned_plugins, &test_env.data_dir))
            .unwrap();
        let mut lock_file = LockFile {
            version: 1,
            plugins: vec![],
        };
        lock_file.merge_plugins(installed_plugins);
        lock_file.save(&test_env.lock_file_path).unwrap();

        let github_clone = test_env
            .data_dir
            .join("github.com")
            .join("owner")
            .join("repo");
        let gitlab_clone = test_env
            .data_dir
            .join("gitlab.com")
            .join("owner")
            .join("repo");
        assert!(github_clone.join(".git").exists());
        assert!(gitlab_clone.join(".git").exists());

        let saved_lock = crate::lock_file::load(&test_env.lock_file_path).unwrap();
        let lock_repos = saved_lock
            .plugins
            .iter()
            .map(|p| p.repo.as_str())
            .collect::<Vec<_>>();
        assert!(
            lock_repos
                .iter()
                .any(|repo| repo == "github.com/owner/repo")
        );
        assert!(
            lock_repos
                .iter()
                .any(|repo| repo == "gitlab.com/owner/repo")
        );

        let fish_conf_d = test_env.fish_config_dir.join(TargetDir::ConfD.as_str());
        assert!(fish_conf_d.join("github-repo.fish").exists());
        assert!(fish_conf_d.join("gitlab-repo.fish").exists());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn clone_plugins_prefers_locked_commit_when_not_forced() {
        let temp_dir = tempfile::tempdir().unwrap();
        let remote_repo_path = temp_dir.path().join("owner").join("sequence-repo");
        let (first, second) = init_remote_repo_with_two_commits(&remote_repo_path);
        let remote_url = format!("file://{}", remote_repo_path.display());

        let resolved = InstallTarget::from_raw(remote_url.clone())
            .resolve()
            .unwrap();
        let lock_plugin = Plugin {
            name: resolved.plugin_repo.repo.clone(),
            repo: resolved.plugin_repo.clone(),
            source: remote_url.clone(),
            commit_sha: first.clone(),
            files: vec![],
        };
        let lock_file = LockFile {
            version: 1,
            plugins: vec![lock_plugin],
        };
        let data_dir = temp_dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();

        let plugins = clone_plugins(&[resolved], false, lock_file, &data_dir)
            .await
            .unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].commit_sha, first);
        assert_ne!(plugins[0].commit_sha, second);
    }

    #[test]
    fn ensure_repo_parent_creates_missing_parent() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path().join("missing").join("repo");
        let parent = repo_path.parent().unwrap();
        assert!(!parent.exists());

        ensure_repo_parent(&repo_path).unwrap();
        assert!(parent.exists());
    }

    #[test]
    fn ensure_repo_parent_skips_existing_parent_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let parent_file = temp_dir.path().join("not-a-dir");
        std::fs::write(&parent_file, "file").unwrap();
        let repo_path = parent_file.join("repo");

        let result = ensure_repo_parent(&repo_path);
        assert!(result.is_ok());
        assert!(parent_file.exists());
    }

    #[test]
    fn emit_event_only_for_conf_d() {
        let _env_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _override = EnvOverride::new(&["PATH", "PEZ_SUPPRESS_EMIT", "PEZ_TEST_FISH_LOG"]);
        let temp_dir = tempfile::tempdir().unwrap();
        let bin_dir = temp_dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let log_path = temp_dir.path().join("fish.log");
        let fish_path = bin_dir.join("fish");
        let script = format!("#!/bin/sh\n\necho \"$@\" >> \"{}\"\n", log_path.display());
        std::fs::write(&fish_path, script).unwrap();
        let mut perms = std::fs::metadata(&fish_path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&fish_path, perms).unwrap();

        let existing_path = std::env::var("PATH").unwrap_or_default();
        unsafe {
            std::env::set_var("PATH", format!("{}:{}", bin_dir.display(), existing_path));
            std::env::remove_var("PEZ_SUPPRESS_EMIT");
            std::env::set_var("PEZ_TEST_FISH_LOG", &log_path);
        }

        let repo = PluginRepo::new(None, "owner".to_string(), "repo".to_string()).unwrap();
        let plugin = Plugin {
            name: "repo".to_string(),
            repo,
            source: "source".to_string(),
            commit_sha: "sha".to_string(),
            files: vec![
                PluginFile {
                    dir: TargetDir::ConfD,
                    name: "alpha.fish".to_string(),
                },
                PluginFile {
                    dir: TargetDir::Functions,
                    name: "beta.fish".to_string(),
                },
            ],
        };

        emit_event(&plugin, &utils::Event::Install).unwrap();

        let log_contents = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert!(log_contents.contains("emit alpha_install"));
        assert!(!log_contents.contains("emit beta_install"));
    }

    #[test]
    fn install_all_clones_when_repo_missing_for_locked_plugin() {
        let _env_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut test_env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "__fish_user_data_dir",
            "XDG_DATA_HOME",
            "HOME",
            "PEZ_SUPPRESS_EMIT",
        ]);

        let remote_root = tempfile::tempdir().unwrap();
        let remote_repo_path = remote_root.path().join("owner").join("locked-repo");
        let expected_commit = init_remote_repo(&remote_repo_path);
        let remote_url = format!("file://{}", remote_repo_path.display());

        let plugin_spec = PluginSpec {
            name: None,
            source: PluginSource::Url {
                url: remote_url.clone(),
                version: None,
                branch: None,
                tag: None,
                commit: None,
            },
        };
        let repo_for_id = plugin_spec.get_plugin_repo().unwrap();
        test_env.setup_config(config::Config {
            plugins: Some(vec![plugin_spec]),
        });
        test_env.setup_lock_file(crate::lock_file::LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: repo_for_id.repo.clone(),
                repo: repo_for_id.clone(),
                source: remote_url.clone(),
                commit_sha: expected_commit.clone(),
                files: vec![],
            }],
        });

        set_test_env_vars(&test_env);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
        }

        let force = false;
        let prune = false;
        let result = install_all(&force, &prune);
        assert!(result.is_ok());

        let repo_path = test_env.data_dir.join(repo_for_id.as_str());
        assert!(repo_path.join(".git").exists());
    }

    #[test]
    fn install_all_fails_when_pinned_commit_checkout_fails() {
        let _env_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut test_env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "__fish_user_data_dir",
            "XDG_DATA_HOME",
            "HOME",
            "PEZ_SUPPRESS_EMIT",
        ]);

        let remote_root = tempfile::tempdir().unwrap();
        let remote_repo_path = remote_root.path().join("owner").join("broken-pinned");
        init_remote_repo(&remote_repo_path);
        let remote_url = format!("file://{}", remote_repo_path.display());

        let plugin_spec = PluginSpec {
            name: None,
            source: PluginSource::Url {
                url: remote_url.clone(),
                version: None,
                branch: None,
                tag: None,
                commit: None,
            },
        };
        let repo_for_id = plugin_spec.get_plugin_repo().unwrap();
        test_env.setup_config(config::Config {
            plugins: Some(vec![plugin_spec]),
        });
        test_env.setup_lock_file(crate::lock_file::LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: repo_for_id.repo.clone(),
                repo: repo_for_id,
                source: remote_url,
                commit_sha: "deadbeef".to_string(),
                files: vec![],
            }],
        });

        set_test_env_vars(&test_env);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
        }

        let force = false;
        let prune = false;
        let result = install_all(&force, &prune);
        assert!(
            result.is_err(),
            "install_all should fail on invalid pinned commit"
        );
        let err_text = format!("{:#}", result.unwrap_err());
        assert!(err_text.contains("failed to checkout pinned commit"));
    }

    #[test]
    fn install_all_force_keeps_local_data_dir() {
        let _env_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut test_env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "__fish_user_data_dir",
            "XDG_DATA_HOME",
            "HOME",
            "PEZ_SUPPRESS_EMIT",
        ]);

        let source_dir = test_env._temp_dir.path().join("local-keep");
        let conf_dir = source_dir.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&conf_dir).unwrap();
        std::fs::write(conf_dir.join("local-keep.fish"), "echo keep\n").unwrap();

        let plugin_spec = PluginSpec {
            name: None,
            source: PluginSource::Path {
                path: source_dir.to_string_lossy().to_string(),
            },
        };
        let repo_for_id = plugin_spec.get_plugin_repo().unwrap();
        test_env.setup_config(config::Config {
            plugins: Some(vec![plugin_spec]),
        });
        test_env.setup_lock_file(crate::lock_file::LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: repo_for_id.repo.clone(),
                repo: repo_for_id.clone(),
                source: source_dir.to_string_lossy().to_string(),
                commit_sha: "local".to_string(),
                files: vec![],
            }],
        });

        let repo_path = test_env.data_dir.join(repo_for_id.as_str());
        std::fs::create_dir_all(&repo_path).unwrap();
        std::fs::write(repo_path.join("sentinel.txt"), "keep").unwrap();

        set_test_env_vars(&test_env);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
        }

        let force = true;
        let prune = false;
        let result = install_all(&force, &prune);
        assert!(result.is_ok());
        assert!(repo_path.join("sentinel.txt").exists());
    }

    #[test]
    fn install_all_new_remote_repo_no_force_does_not_bail() {
        let _env_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut test_env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "__fish_user_data_dir",
            "XDG_DATA_HOME",
            "HOME",
            "PEZ_SUPPRESS_EMIT",
        ]);

        let remote_root = tempfile::tempdir().unwrap();
        let remote_repo_path = remote_root.path().join("owner").join("new-remote");
        init_remote_repo(&remote_repo_path);
        let remote_url = format!("file://{}", remote_repo_path.display());

        let plugin_spec = PluginSpec {
            name: None,
            source: PluginSource::Url {
                url: remote_url.clone(),
                version: None,
                branch: None,
                tag: None,
                commit: None,
            },
        };
        let repo_for_id = plugin_spec.get_plugin_repo().unwrap();
        test_env.setup_config(config::Config {
            plugins: Some(vec![plugin_spec]),
        });
        test_env.setup_lock_file(crate::lock_file::LockFile {
            version: 1,
            plugins: vec![],
        });

        set_test_env_vars(&test_env);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
        }

        let force = false;
        let prune = false;
        let result = install_all(&force, &prune);
        assert!(result.is_ok());

        let repo_path = test_env.data_dir.join(repo_for_id.as_str());
        assert!(repo_path.join(".git").exists());
    }

    #[test]
    fn install_all_local_repo_existing_path_no_force() {
        let _env_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut test_env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "__fish_user_data_dir",
            "XDG_DATA_HOME",
            "HOME",
            "PEZ_SUPPRESS_EMIT",
        ]);

        let source_dir = test_env._temp_dir.path().join("local-new");
        let conf_dir = source_dir.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&conf_dir).unwrap();
        std::fs::write(conf_dir.join("local-new.fish"), "echo new\n").unwrap();

        let plugin_spec = PluginSpec {
            name: None,
            source: PluginSource::Path {
                path: source_dir.to_string_lossy().to_string(),
            },
        };
        let repo_for_id = plugin_spec.get_plugin_repo().unwrap();
        test_env.setup_config(config::Config {
            plugins: Some(vec![plugin_spec]),
        });
        test_env.setup_lock_file(crate::lock_file::LockFile {
            version: 1,
            plugins: vec![],
        });

        let repo_path = test_env.data_dir.join(repo_for_id.as_str());
        std::fs::create_dir_all(&repo_path).unwrap();
        std::fs::write(repo_path.join("sentinel.txt"), "exists").unwrap();

        set_test_env_vars(&test_env);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
        }

        let force = false;
        let prune = false;
        let result = install_all(&force, &prune);
        assert!(result.is_ok());
        assert!(repo_path.join("sentinel.txt").exists());
    }

    #[test]
    fn install_all_reports_ignored_lock_plugins_when_prune_false() {
        let _env_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut test_env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "__fish_user_data_dir",
            "XDG_DATA_HOME",
            "HOME",
            "PEZ_SUPPRESS_EMIT",
        ]);

        let repo_keep = PluginRepo::new(None, "owner".to_string(), "keep".to_string()).unwrap();
        let repo_extra = PluginRepo::new(None, "owner".to_string(), "extra".to_string()).unwrap();
        test_env.setup_config(config::Config {
            plugins: Some(vec![PluginSpec {
                name: None,
                source: PluginSource::Repo {
                    repo: repo_keep.clone(),
                    version: None,
                    branch: None,
                    tag: None,
                    commit: None,
                },
            }]),
        });
        test_env.setup_lock_file(crate::lock_file::LockFile {
            version: 1,
            plugins: vec![
                Plugin {
                    name: repo_keep.repo.clone(),
                    repo: repo_keep.clone(),
                    source: repo_keep.default_remote_source(),
                    commit_sha: "keep-sha".to_string(),
                    files: vec![],
                },
                Plugin {
                    name: repo_extra.repo.clone(),
                    repo: repo_extra.clone(),
                    source: repo_extra.default_remote_source(),
                    commit_sha: "extra-sha".to_string(),
                    files: vec![],
                },
            ],
        });

        let repo_path = test_env.data_dir.join(repo_keep.as_str());
        std::fs::create_dir_all(&repo_path).unwrap();

        set_test_env_vars(&test_env);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
        }

        let force = false;
        let prune = false;
        let (logs, result) =
            crate::tests_support::log::capture_logs(|| install_all(&force, &prune));
        assert!(result.is_ok());
        assert!(
            logs.iter()
                .any(|line| line.contains("pez-lock.toml but not in pez.toml"))
        );
        let ignored_lines: Vec<_> = logs
            .iter()
            .filter(|line| line.starts_with("  - "))
            .collect();
        assert!(ignored_lines.iter().any(|line| line.contains("extra")));
        assert!(!ignored_lines.iter().any(|line| line.contains("keep")));
    }

    #[test]
    fn install_all_force_reclones_remote_repo() {
        let _log_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut test_env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "HOME",
            "PEZ_SUPPRESS_EMIT",
        ]);

        let remote_root = tempfile::tempdir().unwrap();
        let remote_repo_path = remote_root.path().join("owner").join("force-repo");
        let expected_commit = init_remote_repo(&remote_repo_path);
        let remote_url = format!("file://{}", remote_repo_path.display());

        let plugin_repo = PluginRepo {
            host: None,
            owner: "owner".to_string(),
            repo: "force-repo".to_string(),
        };

        let plugin_spec = PluginSpec {
            name: None,
            source: PluginSource::Url {
                url: remote_url.clone(),
                version: None,
                branch: None,
                tag: None,
                commit: None,
            },
        };
        test_env.setup_config(config::Config {
            plugins: Some(vec![plugin_spec]),
        });

        let lock_plugin = Plugin {
            name: "force-repo".to_string(),
            repo: plugin_repo.clone(),
            source: remote_url.clone(),
            commit_sha: "old-lock-sha".to_string(),
            files: vec![],
        };
        test_env.setup_lock_file(crate::lock_file::LockFile {
            version: 1,
            plugins: vec![lock_plugin],
        });

        let repo_path = test_env.data_dir.join(plugin_repo.as_str());
        std::fs::create_dir_all(&repo_path).unwrap();
        std::fs::write(repo_path.join("stale.txt"), b"stale").unwrap();

        unsafe {
            std::env::set_var("PEZ_CONFIG_DIR", &test_env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &test_env.data_dir);
            std::env::set_var("PEZ_TARGET_DIR", &test_env.fish_config_dir);
            std::env::remove_var("__fish_config_dir");
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::set_var("HOME", test_env._temp_dir.path());
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
        }

        let force = true;
        let prune = false;
        let result = install_all(&force, &prune);
        assert!(
            result.is_ok(),
            "install_all should succeed with --force when repo exists"
        );

        assert!(repo_path.join(".git").exists());
        assert!(!repo_path.join("stale.txt").exists());

        let fish_file = test_env
            .fish_config_dir
            .join(TargetDir::ConfD.as_str())
            .join("force-test.fish");
        assert!(fish_file.exists());

        let saved_lock = crate::lock_file::load(&test_env.lock_file_path).unwrap();
        let updated_plugin = saved_lock.get_plugin_by_repo(&plugin_repo).unwrap();
        assert_eq!(updated_plugin.commit_sha, expected_commit);
        assert_eq!(updated_plugin.source, remote_url);
    }

    #[test]
    fn install_all_force_unresolvable_selector_falls_back_to_head() {
        let _log_lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut test_env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_TARGET_DIR",
            "__fish_config_dir",
            "XDG_CONFIG_HOME",
            "HOME",
            "PEZ_SUPPRESS_EMIT",
        ]);

        let remote_root = tempfile::tempdir().unwrap();
        let remote_repo_path = remote_root
            .path()
            .join("owner")
            .join("force-missing-selector");
        let (first_commit, head_commit) = init_remote_repo_with_two_commits(&remote_repo_path);
        let remote_url = format!("file://{}", remote_repo_path.display());

        let plugin_repo = PluginRepo {
            host: None,
            owner: "owner".to_string(),
            repo: "force-missing-selector".to_string(),
        };

        let plugin_spec = PluginSpec {
            name: None,
            source: PluginSource::Url {
                url: remote_url.clone(),
                version: None,
                branch: Some("missing-branch".to_string()),
                tag: None,
                commit: None,
            },
        };
        test_env.setup_config(config::Config {
            plugins: Some(vec![plugin_spec]),
        });

        let lock_plugin = Plugin {
            name: plugin_repo.repo.clone(),
            repo: plugin_repo.clone(),
            source: remote_url.clone(),
            commit_sha: first_commit.clone(),
            files: vec![],
        };
        test_env.setup_lock_file(crate::lock_file::LockFile {
            version: 1,
            plugins: vec![lock_plugin],
        });

        unsafe {
            std::env::set_var("PEZ_CONFIG_DIR", &test_env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &test_env.data_dir);
            std::env::set_var("PEZ_TARGET_DIR", &test_env.fish_config_dir);
            std::env::remove_var("__fish_config_dir");
            std::env::remove_var("XDG_CONFIG_HOME");
            std::env::set_var("HOME", test_env._temp_dir.path());
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
        }

        let force = true;
        let prune = false;
        let result = install_all(&force, &prune);
        assert!(
            result.is_ok(),
            "install_all should succeed and fall back to HEAD when selector cannot be resolved"
        );

        let saved_lock = crate::lock_file::load(&test_env.lock_file_path).unwrap();
        let updated_plugin = saved_lock.get_plugin_by_repo(&plugin_repo).unwrap();
        assert_eq!(updated_plugin.commit_sha, head_commit);
        assert_ne!(updated_plugin.commit_sha, first_commit);
    }
}
