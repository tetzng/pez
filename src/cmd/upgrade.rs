use crate::{
    cli::{PluginRepo, UpgradeArgs},
    config::PluginSpec,
    git,
    lock_file::Plugin,
    models::TargetDir,
    utils,
};
use anyhow::Ok;
use console::Emoji;
use futures::{StreamExt, stream};
use std::{fs, process};
use tracing::{error, info, warn};

pub(crate) async fn run(args: &UpgradeArgs) -> anyhow::Result<()> {
    info!("{}Starting upgrade process...", Emoji("🔍 ", ""));
    if let Some(plugins) = &args.plugins {
        let jobs = utils::load_jobs();
        let tasks = stream::iter(plugins.iter())
            .map(|plugin| {
                let plugin = plugin.clone();
                tokio::task::spawn_blocking(move || {
                    info!("\n{}Upgrading plugin: {}", Emoji("✨ ", ""), &plugin);
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
        "\n{}All specified plugins have been upgraded successfully!",
        Emoji("🎉 ", "")
    );

    Ok(())
}

fn upgrade(plugin: &PluginRepo) -> anyhow::Result<()> {
    let (mut config, config_path) = utils::load_or_create_config()?;

    match config.plugins {
        Some(ref mut plugin_specs) => {
            if !plugin_specs
                .iter()
                .any(|p| p.get_plugin_repo().is_ok_and(|r| r == *plugin))
            {
                plugin_specs.push(PluginSpec {
                    name: None,
                    source: crate::config::PluginSource::Repo {
                        repo: plugin.clone(),
                        version: None,
                        branch: None,
                        tag: None,
                        commit: None,
                    },
                });
                config.save(&config_path)?;
            }
        }
        None => {
            config.plugins = Some(vec![PluginSpec {
                name: None,
                source: crate::config::PluginSource::Repo {
                    repo: plugin.clone(),
                    version: None,
                    branch: None,
                    tag: None,
                    commit: None,
                },
            }]);
            config.save(&config_path)?;
        }
    }

    upgrade_plugin(plugin)?;

    Ok(())
}

async fn upgrade_all() -> anyhow::Result<()> {
    let (config, _) = utils::load_or_create_config()?;
    if let Some(plugins) = &config.plugins {
        let jobs = utils::load_jobs();
        let tasks = stream::iter(plugins.iter())
            .map(|plugin_spec| {
                let repo = plugin_spec.get_plugin_repo().unwrap();
                tokio::task::spawn_blocking(move || {
                    info!("\n{}Upgrading plugin: {}", Emoji("✨ ", ""), &repo);
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
    let config_dir = utils::load_fish_config_dir()?;

    match lock_file.get_plugin_by_repo(plugin_repo) {
        Some(lock_file_plugin) => {
            let repo_path = utils::load_pez_data_dir()?.join(lock_file_plugin.repo.as_str());
            if git::is_local_source(&lock_file_plugin.source) {
                info!(
                    "{}{} Plugin {} is a local source; skipping upgrade.",
                    Emoji("🚧 ", ""),
                    console::style("Info:").cyan(),
                    plugin_repo
                );
                return Ok(());
            }
            if repo_path.exists() {
                let repo = git2::Repository::open(&repo_path)?;
                let latest_remote_commit = git::get_latest_remote_commit(&repo)?;
                if latest_remote_commit == lock_file_plugin.commit_sha {
                    info!(
                        "{}{} Plugin {} is already up to date.",
                        Emoji("🚀 ", ""),
                        console::style("Info:").cyan(),
                        plugin_repo
                    );
                    return Ok(());
                }

                repo.set_head_detached(git2::Oid::from_str(&latest_remote_commit)?)?;

                lock_file_plugin.files.iter().for_each(|file| {
                    let dest_path = config_dir.join(file.dir.as_str()).join(&file.name);
                    if dest_path.exists() {
                        fs::remove_file(&dest_path).unwrap();
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

                lock_file.update_plugin(updated_plugin);
                lock_file.save(&lock_file_path)?;
            } else {
                let path_display = repo_path.display();
                warn!(
                    "{}{} Repository directory at {} does not exist.",
                    Emoji("🚧 ", ""),
                    console::style("Warning:").yellow(),
                    path_display
                );
                warn!("{}You need to install the plugin first.", Emoji("🚧 ", ""),);
            }
        }
        None => {
            error!(
                "{}{} Plugin {} is not installed.",
                Emoji("❌ ", ""),
                console::style("Error:").red().bold(),
                plugin_repo
            );
            process::exit(1);
        }
    }

    Ok(())
}
