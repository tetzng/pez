use crate::{cli::UninstallArgs, models::PluginRepo, models::TargetDir, utils};

use console::Emoji;
use futures::{StreamExt, stream};
use std::fs;
use tracing::{error, info, warn};

pub(crate) async fn run(args: &UninstallArgs) -> anyhow::Result<()> {
    info!("{}Starting uninstallation process...", Emoji("🔍 ", ""));
    let jobs = utils::load_jobs();
    let mut plugins: Vec<PluginRepo> = args.plugins.clone().unwrap_or_default();
    if plugins.is_empty() && args.stdin {
        let stdin_plugins = read_plugins_from_stdin()?;
        plugins.extend(stdin_plugins);
    }
    if plugins.is_empty() {
        anyhow::bail!("No plugins specified for uninstall");
    }
    let tasks = stream::iter(plugins.iter())
        .map(|plugin| {
            let plugin = plugin.clone();
            let force = args.force;
            tokio::task::spawn_blocking(move || {
                info!("\n{}Uninstalling plugin: {}", Emoji("✨ ", ""), plugin);
                uninstall(&plugin, force)
            })
        })
        .buffer_unordered(jobs);

    let results: Vec<_> = tasks.collect().await;
    for r in results {
        r??;
    }
    info!(
        "\n{}All specified plugins have been uninstalled successfully!",
        Emoji("🎉 ", "")
    );

    Ok(())
}

#[allow(dead_code)]
fn read_plugins_from_stdin() -> anyhow::Result<Vec<PluginRepo>> {
    use std::io::{self, Read};
    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    let mut out = Vec::new();
    for line in buf.lines() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        match s.parse::<PluginRepo>() {
            Ok(p) => out.push(p),
            Err(_) => warn!("{}Skipping unrecognized entry: {}", Emoji("⚠ ", ""), s),
        }
    }
    out.sort_by_key(|a| a.as_str());
    out.dedup_by(|a, b| a.as_str() == b.as_str());
    Ok(out)
}

pub(crate) fn uninstall(plugin_repo: &PluginRepo, force: bool) -> anyhow::Result<()> {
    let plugin_repo_str = plugin_repo.as_str();
    let config_dir = utils::load_fish_config_dir()?;

    let (mut config, config_path) = utils::load_or_create_config()?;
    let repo_path = utils::load_pez_data_dir()?.join(&plugin_repo_str);
    let (mut lock_file, lock_file_path) = utils::load_or_create_lock_file()?;
    match lock_file.get_plugin_by_repo(plugin_repo) {
        Some(locked_plugin) => {
            let locked = locked_plugin.clone();
            locked
                .files
                .iter()
                .filter(|f| f.dir == TargetDir::ConfD)
                .for_each(|f| {
                    let _ = utils::emit_event(&f.name, &utils::Event::Uninstall);
                });

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
                    locked.files.iter().for_each(|file| {
                        let dest_path = config_dir.join(file.dir.as_str()).join(&file.name);
                        info!("   - {}", dest_path.display());
                    });
                    error!("If you want to remove these files, use the --force flag.");
                    anyhow::bail!(
                        "Repository directory does not exist. Use --force to remove files listed in lock file"
                    );
                }
            }

            info!(
                "{}Removing plugin files based on pez-lock.toml:",
                Emoji("🗑️  ", ""),
            );
            locked.files.iter().for_each(|file| {
                let dest_path = config_dir.join(file.dir.as_str()).join(&file.name);
                if dest_path.exists() {
                    let path_display = dest_path.display();
                    info!("   - {}", path_display);
                    if let Err(e) = fs::remove_file(&dest_path) {
                        warn!("Failed to remove {}: {:?}", path_display, e);
                    }
                }
            });
            lock_file.remove_plugin(&locked.source);
            lock_file.save(&lock_file_path)?;

            if let Some(ref mut plugin_specs) = config.plugins {
                plugin_specs.retain(|p| p.get_plugin_repo().map_or(true, |r| r != *plugin_repo));
                config.save(&config_path)?;
            }
        }
        None => {
            error!(
                "{}{} Plugin {} is not installed.",
                Emoji("❌ ", ""),
                console::style("Error:").red().bold(),
                plugin_repo_str
            );
            anyhow::bail!("Plugin is not installed: {}", plugin_repo_str);
        }
    }
    info!(
        "{}Successfully uninstalled: {}",
        Emoji("✅ ", ""),
        plugin_repo_str
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::lock_file::{self, LockFile, PluginFile};
    use crate::tests_support::env::TestEnvironmentSetup;

    #[test]
    fn test_uninstall_removes_repo_and_files_and_updates_lock_and_config() {
        // Setup isolated test environment
        let mut env = TestEnvironmentSetup::new();

        // Ensure pez uses our isolated dirs
        unsafe {
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
        }

        // Create config with one plugin spec
        let repo = PluginRepo {
            owner: "owner".into(),
            repo: "pkg".into(),
        };
        let spec = config::PluginSpec {
            name: None,
            source: config::PluginSource::Repo {
                repo: repo.clone(),
                version: None,
                branch: None,
                tag: None,
                commit: None,
            },
        };
        env.setup_config(config::Config {
            plugins: Some(vec![spec]),
        });

        // Create repo dir and a file record in lockfile that points to a functions file
        env.setup_data_repo(vec![repo.clone()]);
        let functions_dir = env.fish_config_dir.join(TargetDir::Functions.as_str());
        std::fs::create_dir_all(&functions_dir).unwrap();
        let dest_file = functions_dir.join("hello.fish");
        std::fs::File::create(&dest_file).unwrap();

        let plugin = crate::lock_file::Plugin {
            name: "pkg".into(),
            repo: repo.clone(),
            source: format!("https://github.com/{}", repo.as_str()),
            commit_sha: "abc1234".into(),
            files: vec![PluginFile {
                dir: TargetDir::Functions,
                name: "hello.fish".into(),
            }],
        };
        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![plugin],
        });

        // Act: uninstall with --force (true)
        let res = uninstall(&repo, true);
        assert!(res.is_ok());

        // Assert: repo directory removed
        assert!(std::fs::metadata(env.data_dir.join(repo.as_str())).is_err());
        // Assert: destination file removed
        assert!(std::fs::metadata(&dest_file).is_err());
        // Assert: lock file updated (plugin removed)
        let lock = lock_file::load(&env.lock_file_path).unwrap();
        assert!(lock.plugins.is_empty());
        // Assert: config updated (plugin spec removed)
        let cfg = config::load(&env.config_path).unwrap();
        assert!(
            cfg.plugins
                .unwrap()
                .into_iter()
                .all(|p| p.get_plugin_repo().unwrap() != repo)
        );
    }
}
