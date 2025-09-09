use crate::{
    cli::PruneArgs,
    config,
    lock_file::{LockFile, Plugin},
    utils,
};
use console::Emoji;
use futures::{StreamExt, stream};
use std::{fs, io, path};
use tracing::{info, warn};

struct PruneContext<'a> {
    fish_config_dir: &'a path::Path,
    data_dir: &'a path::Path,
    config: &'a config::Config,
    lock_file: &'a mut LockFile,
    lock_file_path: &'a path::Path,
}

pub(crate) async fn run(args: &PruneArgs) -> anyhow::Result<()> {
    let fish_config_dir = utils::load_fish_config_dir()?;
    let data_dir = utils::load_pez_data_dir()?;
    let (config, _) = utils::load_config()?;
    let (mut lock_file, lock_file_path) = utils::load_lock_file()?;
    let mut ctx = PruneContext {
        fish_config_dir: &fish_config_dir,
        data_dir: &data_dir,
        config: &config,
        lock_file: &mut lock_file,
        lock_file_path: &lock_file_path,
    };

    if args.dry_run {
        info!("{}Starting dry run prune process...", Emoji("🔍 ", ""));
        dry_run(args.force, &mut ctx)?;
        info!(
            "\n{}Dry run completed. No files have been removed.",
            Emoji("🎉 ", "")
        );
    } else {
        info!("{}Starting prune process...", Emoji("🔍 ", ""));
        prune_parallel(args.force, args.yes, &mut ctx).await?;
    }

    Ok(())
}

fn confirm_removal() -> anyhow::Result<bool> {
    warn!(
        "{}Are you sure you want to continue? [y/N]",
        Emoji("🚧 ", "")
    );
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_lowercase() == "y")
}

fn find_unused_plugins(
    config: &config::Config,
    lock_file: &LockFile,
) -> anyhow::Result<Vec<Plugin>> {
    if config.plugins.is_none() {
        return Ok(lock_file.plugins.clone());
    }

    Ok(lock_file
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
        .collect())
}

#[allow(dead_code)]
fn prune<F>(
    force: bool,
    yes: bool,
    confirm_removal: F,
    ctx: &mut PruneContext,
) -> anyhow::Result<()>
where
    F: Fn() -> anyhow::Result<bool>,
{
    info!("{}Checking for unused plugins...", Emoji("🔍 ", ""));

    let remove_plugins: Vec<_> = find_unused_plugins(ctx.config, ctx.lock_file)?;
    if remove_plugins.is_empty() {
        info!(
            "{}No unused plugins found. Your environment is clean!",
            Emoji("🎉 ", "")
        );
        return Ok(());
    }

    if ctx.config.plugins.is_none() {
        warn!(
            "{}{} No plugins are defined in pez.toml.",
            Emoji("🚧 ", ""),
            console::style("Warning:").yellow()
        );
        warn!(
            "{}All plugins defined in pez-lock.toml will be removed.",
            Emoji("🚧 ", "")
        );

        if !yes && !confirm_removal()? {
            anyhow::bail!("{}Prune process aborted.", Emoji("🚧 ", ""));
        }
    }

    for plugin in remove_plugins {
        let repo_path = ctx.data_dir.join(plugin.repo.as_str());
        if repo_path.exists() {
            fs::remove_dir_all(&repo_path)?;
        } else {
            let path_display = repo_path.display();
            warn!(
                "{}{} Repository directory at {} does not exist.",
                Emoji("🚧 ", ""),
                console::style("Warning:").yellow(),
                path_display
            );

            if !force {
                info!(
                    "{}Detected plugin files based on pez-lock.toml:",
                    Emoji("📄 ", ""),
                );

                plugin.files.iter().for_each(|file| {
                    let dest_path = file.get_path(ctx.fish_config_dir);
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
        plugin.files.iter().for_each(|file| {
            let dest_path = file.get_path(ctx.fish_config_dir);
            if dest_path.exists() {
                let path_display = dest_path.display();
                info!("   - {}", path_display);
                fs::remove_file(&dest_path).unwrap();
            }
        });
        ctx.lock_file.remove_plugin(&plugin.source);
        ctx.lock_file.save(ctx.lock_file_path)?;
    }
    info!(
        "\n{}All uninstalled plugins have been pruned successfully!",
        Emoji("🎉 ", "")
    );

    Ok(())
}

async fn prune_parallel(force: bool, yes: bool, ctx: &mut PruneContext<'_>) -> anyhow::Result<()> {
    info!("{}Checking for unused plugins...", Emoji("🔍 ", ""));

    let remove_plugins: Vec<_> = find_unused_plugins(ctx.config, ctx.lock_file)?;
    if remove_plugins.is_empty() {
        info!(
            "{}No unused plugins found. Your environment is clean!",
            Emoji("🎉 ", "")
        );
        return Ok(());
    }

    if ctx.config.plugins.is_none() {
        warn!(
            "{}{} No plugins are defined in pez.toml.",
            Emoji("🚧 ", ""),
            console::style("Warning:").yellow()
        );
        warn!(
            "{}All plugins defined in pez-lock.toml will be removed.",
            Emoji("🚧 ", "")
        );

        if !yes && !confirm_removal()? {
            anyhow::bail!("{}Prune process aborted.", Emoji("🚧 ", ""));
        }
    }

    let jobs = utils::load_jobs();
    let fish_config_dir = ctx.fish_config_dir.to_path_buf();
    let data_dir = ctx.data_dir.to_path_buf();

    let tasks = stream::iter(remove_plugins.iter())
        .map(|plugin| {
            let plugin = plugin.clone();
            let fish_config_dir = fish_config_dir.clone();
            let data_dir = data_dir.clone();
            async move {
                let repo_path = data_dir.join(plugin.repo.as_str());
                if repo_path.exists() {
                    tokio::task::spawn_blocking(move || fs::remove_dir_all(&repo_path))
                        .await
                        .unwrap()?;
                } else {
                    let path_display = repo_path.display();
                    warn!(
                        "{}{} Repository directory at {} does not exist.",
                        Emoji("🚧 ", ""),
                        console::style("Warning:").yellow(),
                        path_display
                    );
                    if !force {
                        info!(
                            "{}Detected plugin files based on pez-lock.toml:",
                            Emoji("📄 ", ""),
                        );
                        for file in &plugin.files {
                            let dest_path =
                                fish_config_dir.join(file.dir.as_str()).join(&file.name);
                            info!("   - {}", dest_path.display());
                        }
                        return Ok::<Option<String>, anyhow::Error>(None);
                    }
                }

                info!(
                    "{}Removing plugin files based on pez-lock.toml:",
                    Emoji("🗑️  ", ""),
                );
                for file in &plugin.files {
                    let dest_path = fish_config_dir.join(file.dir.as_str()).join(&file.name);
                    if dest_path.exists() {
                        let to_delete = dest_path.clone();
                        tokio::task::spawn_blocking(move || fs::remove_file(&to_delete))
                            .await
                            .unwrap()
                            .ok();
                    }
                }

                Ok(Some(plugin.source.clone()))
            }
        })
        .buffer_unordered(jobs);

    let mut sources_to_remove: Vec<String> = Vec::new();
    futures::pin_mut!(tasks);
    while let Some(res) = tasks.next().await {
        if let Some(source) = res? {
            sources_to_remove.push(source);
        }
    }

    if !sources_to_remove.is_empty() {
        ctx.lock_file
            .plugins
            .retain(|p| !sources_to_remove.contains(&p.source));
        ctx.lock_file.save(ctx.lock_file_path)?;
    }

    info!(
        "\n{}All uninstalled plugins have been pruned successfully!",
        Emoji("🎉 ", "")
    );
    Ok(())
}

fn dry_run(force: bool, ctx: &mut PruneContext) -> anyhow::Result<()> {
    if ctx.config.plugins.is_none() {
        warn!(
            "{}{} No plugins are defined in pez.toml.",
            Emoji("🚧 ", ""),
            console::style("Warning:").yellow()
        );
        warn!(
            "{}All plugins defined in pez-lock.toml will be removed.",
            Emoji("🚧 ", "")
        );
    }

    let remove_plugins: Vec<_> = if ctx.config.plugins.is_none() {
        ctx.lock_file.plugins.clone()
    } else {
        ctx.lock_file
            .plugins
            .iter()
            .filter(|plugin| {
                !ctx.config
                    .plugins
                    .as_ref()
                    .unwrap()
                    .iter()
                    .any(|p| p.repo == plugin.repo)
            })
            .cloned()
            .collect()
    };

    info!("{}Plugins that would be removed:", Emoji("🐟 ", ""));
    remove_plugins.iter().for_each(|plugin| {
        info!("  - {}", &plugin.repo);
    });

    for plugin in remove_plugins {
        let repo_path = ctx.data_dir.join(plugin.repo.as_str());
        if !repo_path.exists() {
            let path_display = repo_path.display();
            warn!(
                "{}{} Repository directory at {} does not exist.",
                Emoji("🚧 ", ""),
                console::style("Warning:").yellow(),
                path_display
            );

            if !force {
                info!(
                    "{}Detected plugin files based on pez-lock.toml:",
                    Emoji("📄 ", ""),
                );

                plugin.files.iter().for_each(|file| {
                    let dest_path = file.get_path(ctx.fish_config_dir);
                    info!("   - {}", dest_path.display());
                });
                info!("If you want to remove these files, use the --force flag.");
                continue;
            }
        }

        info!(
            "{}Plugin files that would be removed based on pez-lock.toml:",
            Emoji("🗑️  ", ""),
        );
        plugin.files.iter().for_each(|file| {
            let dest_path = file.get_path(ctx.fish_config_dir);
            if dest_path.exists() {
                let path_display = dest_path.display();
                info!("   - {}", path_display);
            }
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::vec;

    use super::*;
    use crate::{
        cli::PluginRepo,
        lock_file::{self, PluginFile},
        models::TargetDir,
        tests_support::env::TestEnvironmentSetup,
    };
    use config::PluginSpec;

    struct TestDataBuilder {
        used_plugin: Plugin,
        unused_plugin: Plugin,
        used_plugin_spec: PluginSpec,
    }

    impl TestDataBuilder {
        fn new() -> Self {
            Self {
                used_plugin: Plugin {
                    name: "used-repo".to_string(),
                    repo: PluginRepo {
                        owner: "owner".to_string(),
                        repo: "used-repo".to_string(),
                    },
                    source: "https://example.com/owner/used-repo".to_string(),
                    commit_sha: "sha".to_string(),
                    files: vec![PluginFile {
                        dir: TargetDir::Functions,
                        name: "used.fish".to_string(),
                    }],
                },
                unused_plugin: Plugin {
                    name: "unused-repo".to_string(),
                    repo: PluginRepo {
                        owner: "owner".to_string(),
                        repo: "unused-repo".to_string(),
                    },
                    source: "https://example.com/owner/unused-repo".to_string(),
                    commit_sha: "sha".to_string(),
                    files: vec![PluginFile {
                        dir: TargetDir::Functions,
                        name: "unused.fish".to_string(),
                    }],
                },
                used_plugin_spec: PluginSpec {
                    repo: PluginRepo {
                        owner: "owner".to_string(),
                        repo: "used-repo".to_string(),
                    },
                    name: None,
                    source: None,
                },
            }
        }
        fn build(self) -> TestData {
            TestData {
                used_plugin: self.used_plugin,
                unused_plugin: self.unused_plugin,
                used_plugin_spec: self.used_plugin_spec,
            }
        }
    }

    struct TestData {
        used_plugin: Plugin,
        unused_plugin: Plugin,
        used_plugin_spec: PluginSpec,
    }

    impl TestEnvironmentSetup {
        fn create_context<'a>(&'a mut self) -> PruneContext<'a> {
            PruneContext {
                fish_config_dir: &self.fish_config_dir,
                data_dir: &self.data_dir,
                config: self.config.as_ref().expect("Config is not initialized"),
                lock_file: self
                    .lock_file
                    .as_mut()
                    .expect("Lock file is not initialized"),
                lock_file_path: &self.lock_file_path,
            }
        }
    }

    #[test]
    fn test_find_unused_plugins() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config {
            plugins: Some(vec![test_data.used_plugin_spec]),
        });
        test_env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![test_data.used_plugin, test_data.unused_plugin],
        });
        let ctx = test_env.create_context();

        let result = find_unused_plugins(ctx.config, ctx.lock_file);
        assert!(result.is_ok());

        let unused_plugins = result.unwrap();
        assert_eq!(unused_plugins.len(), 1, "Only one plugin should be unused");
        assert_eq!(
            unused_plugins[0].repo.as_str(),
            "owner/unused-repo",
            "owner/unused-repo should be unused"
        );
    }

    #[test]
    fn test_prune() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config {
            plugins: Some(vec![test_data.used_plugin_spec]),
        });
        test_env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![test_data.used_plugin, test_data.unused_plugin],
        });
        test_env.setup_data_repo(test_env.lock_file.as_ref().unwrap().get_plugin_repos());

        let mut ctx = test_env.create_context();

        let result = prune(false, false, || Ok(false), &mut ctx);
        assert!(result.is_ok());

        let saved_lock_file = lock_file::load(ctx.lock_file_path).unwrap();
        assert_eq!(
            saved_lock_file.plugins.len(),
            1,
            "Only one plugin should remain"
        );
        assert_eq!(
            saved_lock_file.plugins[0].repo.as_str(),
            "owner/used-repo",
            "owner/used-repo should remain"
        );
        assert!(
            fs::metadata(ctx.data_dir.join("owner/unused-repo")).is_err(),
            "Unused repo directory should be deleted"
        );
        assert!(
            fs::metadata(ctx.data_dir.join("owner/used-repo")).is_ok(),
            "Used repo directory should still exist"
        );
    }

    #[test]
    fn test_prune_empty_remove_plugins() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config {
            plugins: Some(vec![test_data.used_plugin_spec]),
        });
        test_env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![test_data.used_plugin],
        });
        test_env.setup_data_repo(test_env.lock_file.as_ref().unwrap().get_plugin_repos());

        let mut ctx = test_env.create_context();
        let prev_plugins_len = ctx.lock_file.plugins.len();

        let result = prune(false, false, || Ok(false), &mut ctx);
        assert!(result.is_ok());

        let saved_lock_file = lock_file::load(ctx.lock_file_path).unwrap();
        assert_eq!(
            saved_lock_file.plugins.len(),
            prev_plugins_len,
            "No plugins should be removed"
        );
        assert!(
            fs::metadata(ctx.data_dir.join("owner/used-repo")).is_ok(),
            "Used repo directory should still exist"
        );
    }

    #[test]
    fn test_prune_empty_config_without_yes_and_confirm_removal_true() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config { plugins: None });
        test_env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![test_data.unused_plugin],
        });
        test_env.setup_data_repo(test_env.lock_file.as_ref().unwrap().get_plugin_repos());

        let mut ctx = test_env.create_context();

        let result = prune(false, false, || Ok(true), &mut ctx);
        assert!(result.is_ok());

        let lock_file = lock_file::load(ctx.lock_file_path).unwrap();
        assert_eq!(lock_file.plugins.len(), 0, "All plugins should be removed");
        assert!(
            fs::metadata(ctx.data_dir.join("owner/unused-repo")).is_err(),
            "Unused repo directory should be deleted"
        );
    }

    #[test]
    fn test_prune_empty_config_without_yes_and_confirm_removal_false() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config { plugins: None });
        test_env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![test_data.unused_plugin],
        });
        test_env.setup_data_repo(test_env.lock_file.as_ref().unwrap().get_plugin_repos());

        let mut ctx = test_env.create_context();
        let prev_plugins_len = ctx.lock_file.plugins.len();

        let result = prune(false, false, || Ok(false), &mut ctx);
        assert!(result.is_err_and(|e| e.to_string().contains("Prune process aborted.")));

        let lock_file = lock_file::load(ctx.lock_file_path).unwrap();
        assert_eq!(
            lock_file.plugins.len(),
            prev_plugins_len,
            "No plugins should be removed"
        );
        assert!(
            fs::metadata(ctx.data_dir.join("owner/unused-repo")).is_ok(),
            "Unused repo directory should still exist"
        );
    }

    #[test]
    fn test_prune_empty_config_with_yes() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config { plugins: None });
        test_env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![test_data.unused_plugin],
        });
        test_env.setup_data_repo(test_env.lock_file.as_ref().unwrap().get_plugin_repos());

        let mut ctx = test_env.create_context();

        let result = prune(false, true, || Ok(false), &mut ctx);
        assert!(result.is_ok());

        let lock_file = lock_file::load(ctx.lock_file_path).unwrap();
        assert_eq!(lock_file.plugins.len(), 0, "All plugins should be removed");
        assert!(
            fs::metadata(ctx.data_dir.join("owner/unused-repo")).is_err(),
            "Unused repo directory should be deleted"
        );
    }

    #[test]
    fn test_prune_empty_config_missing_data_dir_with_force() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config {
            plugins: Some(vec![test_data.used_plugin_spec]),
        });
        test_env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![test_data.used_plugin, test_data.unused_plugin],
        });
        test_env.setup_fish_config();
        assert!(
            fs::metadata(test_env.fish_config_dir.join("functions/unused.fish")).is_ok(),
            "Unused plugin file should exist"
        );

        let mut ctx = test_env.create_context();

        let result = prune(true, false, || Ok(false), &mut ctx);
        assert!(result.is_ok());

        let lock_file = lock_file::load(ctx.lock_file_path).unwrap();
        assert_eq!(
            lock_file.plugins.len(),
            1,
            "Unused plugin should be removed"
        );
        assert!(
            fs::metadata(test_env.fish_config_dir.join("functions/unused.fish")).is_err(),
            "Unused plugin file should be deleted"
        );
    }

    #[test]
    fn test_prune_empty_config_missing_data_dir_without_force() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config {
            plugins: Some(vec![test_data.used_plugin_spec]),
        });
        test_env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![test_data.used_plugin, test_data.unused_plugin],
        });
        test_env.setup_fish_config();

        let mut ctx = test_env.create_context();

        let result = prune(false, false, || Ok(false), &mut ctx);
        assert!(result.is_ok());

        let lock_file = lock_file::load(ctx.lock_file_path).unwrap();
        assert_eq!(lock_file.plugins.len(), 2, "No plugins should be removed");
        assert!(
            fs::metadata(test_env.fish_config_dir.join("functions/unused.fish")).is_ok(),
            "Unused plugin file should still exist"
        );
    }

    #[test]
    fn test_prune_dry_run() {
        let mut test_env = TestEnvironmentSetup::new();
        let test_data = TestDataBuilder::new().build();
        test_env.setup_config(config::Config {
            plugins: Some(vec![test_data.used_plugin_spec]),
        });
        test_env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![test_data.used_plugin, test_data.unused_plugin],
        });
        test_env.setup_data_repo(test_env.lock_file.as_ref().unwrap().get_plugin_repos());

        let mut ctx = test_env.create_context();

        let result = dry_run(false, &mut ctx);
        assert!(result.is_ok());

        let saved_lock_file = lock_file::load(ctx.lock_file_path).unwrap();
        assert_eq!(
            saved_lock_file.plugins.len(),
            2,
            "No plugins should be removed"
        );
        assert!(
            fs::metadata(ctx.data_dir.join("owner/unused-repo")).is_ok(),
            "Unused repo directory should still exist"
        );
        assert!(
            fs::metadata(ctx.data_dir.join("owner/used-repo")).is_ok(),
            "Used repo directory should still exist"
        );
    }
}
