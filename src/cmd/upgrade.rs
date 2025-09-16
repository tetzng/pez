use crate::{
    cli::UpgradeArgs,
    git,
    lock_file::Plugin,
    models::{PluginRepo, TargetDir},
    utils,
};

use console::Emoji;
use futures::{StreamExt, stream};
use std::fs;
use tracing::{error, info, warn};

pub(crate) async fn run(args: &UpgradeArgs) -> anyhow::Result<()> {
    info!("{}Starting upgrade process...", Emoji("🔍 ", ""));
    if let Some(plugins) = &args.plugins {
        let jobs = utils::load_jobs();
        let tasks = stream::iter(plugins.iter())
            .map(|plugin| {
                let plugin = plugin.clone();
                tokio::task::spawn_blocking(move || {
                    info!("{}Upgrading plugin: {}", Emoji("✨ ", ""), &plugin);
                    let res = upgrade(&plugin);
                    if res.is_ok() {
                        info!(
                            "{}Successfully upgraded plugin: {}",
                            Emoji("✅ ", ""),
                            &plugin
                        );
                    }
                    res
                })
            })
            .buffer_unordered(jobs);
        let results: Vec<_> = tasks.collect().await;
        for r in results {
            r??;
        }
    } else {
        upgrade_all().await?;
    }
    info!(
        "{}All specified plugins have been upgraded successfully!",
        Emoji("🎉 ", "")
    );

    Ok(())
}

fn upgrade(plugin: &PluginRepo) -> anyhow::Result<()> {
    let (mut config, config_path) = utils::load_or_create_config()?;

    if config.ensure_plugin_for_repo(plugin) {
        config.save(&config_path)?;
    }

    upgrade_plugin(plugin)?;

    Ok(())
}

async fn upgrade_all() -> anyhow::Result<()> {
    let (config, _) = utils::load_or_create_config()?;
    if let Some(plugins) = &config.plugins {
        let repos: Vec<PluginRepo> = plugins
            .iter()
            .filter_map(|p| p.get_plugin_repo().ok())
            .collect();
        let jobs = utils::load_jobs();
        let tasks = stream::iter(repos.into_iter())
            .map(|repo| {
                tokio::task::spawn_blocking(move || {
                    info!("{}Upgrading plugin: {}", Emoji("✨ ", ""), &repo);
                    upgrade_plugin(&repo)
                })
            })
            .buffer_unordered(jobs);
        let results: Vec<_> = tasks.collect().await;
        for r in results {
            r??;
        }
    }

    Ok(())
}

fn upgrade_plugin(plugin_repo: &PluginRepo) -> anyhow::Result<()> {
    let (mut lock_file, lock_file_path) = utils::load_or_create_lock_file()?;
    let (config, _) = utils::load_or_create_config()?;
    let config_dir = utils::load_fish_config_dir()?;

    match lock_file.get_plugin_by_repo(plugin_repo) {
        Some(lock_file_plugin) => {
            let repo_path = utils::load_pez_data_dir()?.join(lock_file_plugin.repo.as_str());
            if git::is_local_source(&lock_file_plugin.source) {
                info!(
                    "{} {} Plugin {} is a local source; skipping upgrade.",
                    Emoji("🚧 ", ""),
                    crate::utils::label_info(),
                    plugin_repo
                );
                return Ok(());
            }
            if repo_path.exists() {
                let repo = git2::Repository::open(&repo_path)?;
                // Determine desired selection from config (if present); fall back to default head
                let sel = config
                    .plugins
                    .as_ref()
                    .and_then(|ps| {
                        ps.iter()
                            .find(|p| p.get_plugin_repo().ok().as_ref() == Some(plugin_repo))
                            .and_then(|p| p.to_resolved().ok())
                    })
                    .map(|r| crate::resolver::selection_from_ref_kind(&r.ref_kind))
                    .unwrap_or(crate::resolver::Selection::DefaultHead);

                let latest_remote_commit = match git::resolve_selection(&repo, &sel) {
                    Ok(c) => c,
                    Err(e) => {
                        warn!(
                            "Failed to resolve selection for {}: {:?}. Falling back to remote HEAD.",
                            plugin_repo, e
                        );
                        git::get_latest_remote_commit(&repo)?
                    }
                };
                if latest_remote_commit == lock_file_plugin.commit_sha {
                    info!(
                        "{} {} Plugin {} is already up to date.",
                        Emoji("🚀 ", ""),
                        crate::utils::label_info(),
                        plugin_repo
                    );
                    return Ok(());
                }

                repo.set_head_detached(git2::Oid::from_str(&latest_remote_commit)?)?;

                lock_file_plugin.files.iter().for_each(|file| {
                    let dest_path = config_dir.join(file.dir.as_str()).join(&file.name);
                    if dest_path.exists()
                        && let Err(e) = fs::remove_file(&dest_path)
                    {
                        warn!("Failed to remove {}: {:?}", dest_path.display(), e);
                    }
                });
                let mut updated_plugin = Plugin {
                    name: lock_file_plugin.name.to_string(),
                    repo: plugin_repo.clone(),
                    source: lock_file_plugin.source.clone(),
                    commit_sha: latest_remote_commit,
                    files: vec![],
                };
                info!("{:?}", updated_plugin);

                utils::copy_plugin_files_from_repo(&repo_path, &mut updated_plugin)?;

                updated_plugin
                    .files
                    .iter()
                    .filter(|f| f.dir == TargetDir::ConfD)
                    .for_each(|f| {
                        if let Err(e) = utils::emit_event(&f.name, &utils::Event::Update) {
                            error!("Failed to emit event for {}: {:?}", &f.name, e);
                        }
                    });

                if let Err(e) = lock_file.upsert_plugin_by_repo(updated_plugin) {
                    warn!("Failed to update lock file: {:?}", e);
                }
                lock_file.save(&lock_file_path)?;
            } else {
                let path_display = repo_path.display();
                warn!(
                    "{} {} Repository directory at {} does not exist.",
                    Emoji("🚧 ", ""),
                    crate::utils::label_warning(),
                    path_display
                );
                warn!("{}You need to install the plugin first.", Emoji("🚧 ", ""),);
            }
        }
        None => {
            anyhow::bail!("Plugin is not installed: {}", plugin_repo);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::lock_file::{LockFile, PluginFile};
    use crate::tests_support::env::TestEnvironmentSetup;
    use crate::tests_support::log::capture_logs;

    #[test]
    fn test_upgrade_logs_already_up_to_date() {
        // Setup isolated test environment
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let prev_fc = std::env::var_os("__fish_config_dir");
        let prev_pc = std::env::var_os("PEZ_CONFIG_DIR");
        let prev_pd = std::env::var_os("PEZ_DATA_DIR");
        let prev_nc = std::env::var_os("NO_COLOR");
        unsafe {
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
            std::env::set_var("NO_COLOR", "1");
        }

        // Local origin + work repo with one commit on main
        let tmp = tempfile::tempdir().unwrap();
        let origin_path = tmp.path().join("origin.git");
        let workdir_path = tmp.path().join("work");
        let origin = git2::Repository::init_bare(&origin_path).unwrap();
        let work = git2::Repository::init(&workdir_path).unwrap();
        {
            let mut cfg = work.config().unwrap();
            cfg.set_str("user.name", "tester").unwrap();
            cfg.set_str("user.email", "tester@example.com").unwrap();
        }
        std::fs::create_dir_all(&workdir_path).unwrap();
        std::fs::write(workdir_path.join("README.md"), "hello").unwrap();
        let mut index = work.index().unwrap();
        index.add_path(std::path::Path::new("README.md")).unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = work.find_tree(tree_oid).unwrap();
        let sig = work.signature().unwrap();
        let commit_oid = work
            .commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        work.remote("origin", origin_path.to_str().unwrap())
            .unwrap();
        {
            let mut remote = work.find_remote("origin").unwrap();
            remote
                .connect(git2::Direction::Push)
                .and_then(|_| remote.push(&["refs/heads/main:refs/heads/main"], None))
                .unwrap();
        }
        origin.set_head("refs/heads/main").unwrap();

        let repo = PluginRepo {
            owner: "owner".into(),
            repo: "pkg".into(),
        };
        let repo_path = env.data_dir.join(repo.as_str());
        crate::git::clone_repository(origin_path.to_str().unwrap(), &repo_path).unwrap();

        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![crate::lock_file::Plugin {
                name: "pkg".into(),
                repo: repo.clone(),
                source: "https://example.com/owner/pkg".into(),
                commit_sha: commit_oid.to_string(),
                files: vec![PluginFile {
                    dir: TargetDir::Functions,
                    name: "hello.fish".into(),
                }],
            }],
        });
        env.setup_config(config::Config {
            plugins: Some(vec![config::PluginSpec {
                name: None,
                source: config::PluginSource::Repo {
                    repo: repo.clone(),
                    version: None,
                    branch: None,
                    tag: None,
                    commit: None,
                },
            }]),
        });

        let (logs, res) = capture_logs(|| upgrade_plugin(&repo));
        assert!(res.is_ok());
        let joined = logs.join("\n");
        assert!(joined.contains("Plugin owner/pkg is already up to date."));
        assert!(joined.contains("[Info]"));
        assert!(!joined.contains("\u{1b}["));

        // restore env
        unsafe {
            if let Some(v) = prev_fc {
                std::env::set_var("__fish_config_dir", v)
            } else {
                std::env::remove_var("__fish_config_dir")
            }
            if let Some(v) = prev_pc {
                std::env::set_var("PEZ_CONFIG_DIR", v)
            } else {
                std::env::remove_var("PEZ_CONFIG_DIR")
            }
            if let Some(v) = prev_pd {
                std::env::set_var("PEZ_DATA_DIR", v)
            } else {
                std::env::remove_var("PEZ_DATA_DIR")
            }
            if let Some(v) = prev_nc {
                std::env::set_var("NO_COLOR", v)
            } else {
                std::env::remove_var("NO_COLOR")
            }
        }
    }
}
