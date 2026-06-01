use crate::{cli::UninstallArgs, models::PluginRepo, models::TargetDir, utils};

use console::Emoji;
use futures::{StreamExt, stream};
use std::{collections::HashSet, io};
use tracing::{error, info, warn};

pub(crate) async fn run(args: &UninstallArgs) -> anyhow::Result<()> {
    info!("{}Starting uninstallation process...", Emoji("🔍 ", ""));
    let jobs = utils::load_jobs().max(1);
    let mut plugins: Vec<PluginRepo> = args.plugins.clone().unwrap_or_default();
    if plugins.is_empty() && args.stdin {
        let stdin_plugins = read_plugins_from_stdin()?;
        plugins.extend(stdin_plugins);
    }
    normalize_plugins(&mut plugins);
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
        "{}All specified plugins have been uninstalled successfully!",
        Emoji("🎉 ", "")
    );

    Ok(())
}

fn normalize_plugins(plugins: &mut Vec<PluginRepo>) {
    let mut seen = HashSet::new();
    plugins.retain(|repo| seen.insert(repo.as_str()));
}

pub(crate) fn read_plugins_from_reader<R: io::Read>(
    mut reader: R,
) -> anyhow::Result<Vec<PluginRepo>> {
    let mut buf = String::new();
    reader.read_to_string(&mut buf)?;
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

#[allow(dead_code)]
fn read_plugins_from_stdin() -> anyhow::Result<Vec<PluginRepo>> {
    #[cfg(test)]
    if let Some(input) = take_stdin_for_tests() {
        return read_plugins_from_reader(std::io::Cursor::new(input));
    }
    let stdin = io::stdin();
    let handle = stdin.lock();
    read_plugins_from_reader(handle)
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
            let file_paths: Vec<_> = locked
                .files
                .iter()
                .map(|file| file.get_path(&config_dir))
                .collect();

            if !repo_path.exists() {
                let path_display = repo_path.display();
                warn!(
                    "{} {} Repository directory at {} does not exist.",
                    Emoji("🚧 ", ""),
                    crate::utils::label_warning(),
                    path_display
                );
                if !force {
                    info!(
                        "{}Detected plugin files based on pez-lock.toml:",
                        Emoji("📄 ", ""),
                    );
                    file_paths
                        .iter()
                        .for_each(|dest_path| info!("   - {}", dest_path.display()));
                    error!("If you want to remove these files, use the --force flag.");
                    anyhow::bail!(
                        "Repository directory does not exist. Use --force to remove files listed in lock file"
                    );
                }
            }

            let staged_removals = utils::stage_files_for_removal(&file_paths)?;

            info!(
                "{}Removing plugin files based on pez-lock.toml:",
                Emoji("🗑️  ", ""),
            );
            for dest_path in staged_removals.removed_paths() {
                info!("   - {}", dest_path.display());
            }

            let original_config = config.clone();
            let config_saved = if let Some(ref mut plugin_specs) = config.plugins {
                plugin_specs.retain(|p| p.get_plugin_repo().map_or(true, |r| r != *plugin_repo));
                config.save(&config_path)?;
                true
            } else {
                false
            };

            lock_file.remove_plugin(&locked.source);
            if let Err(err) = lock_file.save(&lock_file_path) {
                if config_saved && let Err(restore_err) = original_config.save(&config_path) {
                    warn!(
                        "Failed to restore config {} after failed uninstall: {:?}",
                        config_path.display(),
                        restore_err
                    );
                }
                return Err(err);
            }

            staged_removals.commit();
            utils::remove_dir_all_best_effort(&repo_path);
            locked
                .files
                .iter()
                .filter(|f| f.dir == TargetDir::ConfD)
                .for_each(|f| {
                    let _ = utils::emit_event(&f.name, &utils::Event::Uninstall);
                });
        }
        None => {
            error!(
                "{} {} Plugin {} is not installed.",
                Emoji("❌ ", ""),
                crate::utils::label_error(),
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
fn stdin_store() -> &'static std::sync::Mutex<Option<String>> {
    static STDIN_INPUT: std::sync::OnceLock<std::sync::Mutex<Option<String>>> =
        std::sync::OnceLock::new();
    STDIN_INPUT.get_or_init(|| std::sync::Mutex::new(None))
}

#[cfg(test)]
fn take_stdin_for_tests() -> Option<String> {
    stdin_store().lock().unwrap().take()
}

#[cfg(test)]
struct StdinGuard {
    prev: Option<String>,
}

#[cfg(test)]
impl StdinGuard {
    fn new(value: Option<String>) -> Self {
        let store = stdin_store();
        let mut guard = store.lock().unwrap();
        let prev = guard.take();
        *guard = value;
        Self { prev }
    }
}

#[cfg(test)]
impl Drop for StdinGuard {
    fn drop(&mut self) {
        let store = stdin_store();
        let mut guard = store.lock().unwrap();
        *guard = self.prev.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::lock_file::{self, LockFile, PluginFile};
    use crate::tests_support::env::TestEnvironmentSetup;
    use crate::tests_support::log::capture_logs;
    use std::ffi::OsString;
    use std::io::Cursor;
    use std::os::unix::fs::PermissionsExt;

    struct EnvOverride {
        keys: Vec<&'static str>,
        previous: Vec<Option<OsString>>,
    }

    impl EnvOverride {
        fn new(keys: &[&'static str]) -> Self {
            let previous = keys.iter().map(std::env::var_os).collect();
            Self {
                keys: keys.to_vec(),
                previous,
            }
        }
    }

    impl Drop for EnvOverride {
        fn drop(&mut self) {
            for (key, prev) in self.keys.iter().zip(self.previous.drain(..)) {
                if let Some(value) = prev {
                    unsafe {
                        std::env::set_var(key, value);
                    }
                } else {
                    unsafe {
                        std::env::remove_var(key);
                    }
                }
            }
        }
    }

    #[test]
    fn read_plugins_from_reader_filters_and_sorts_entries() {
        let input = r"

# comment line
owner/plugin-a
invalid entry
owner/plugin-b
owner/plugin-a
";

        let (logs, result) = capture_logs(|| read_plugins_from_reader(Cursor::new(input)));
        let plugins = result.expect("parsing should succeed");
        let names: Vec<String> = plugins.iter().map(|p| p.as_str()).collect();
        assert_eq!(names, vec!["owner/plugin-a", "owner/plugin-b"]);
        let warnings = logs
            .iter()
            .filter(|msg| msg.contains("Skipping unrecognized entry"))
            .count();
        assert_eq!(warnings, 1, "logs: {:?}", logs);
    }

    #[test]
    fn normalize_plugins_removes_duplicates_preserving_first_occurrence() {
        let mut plugins = vec![
            "owner/one".parse::<PluginRepo>().unwrap(),
            "owner/two".parse::<PluginRepo>().unwrap(),
            "owner/one".parse::<PluginRepo>().unwrap(),
            "owner/three".parse::<PluginRepo>().unwrap(),
            "owner/two".parse::<PluginRepo>().unwrap(),
        ];

        normalize_plugins(&mut plugins);

        let names: Vec<String> = plugins.iter().map(|p| p.as_str()).collect();
        assert_eq!(names, vec!["owner/one", "owner/two", "owner/three"]);
    }

    #[test]
    fn test_uninstall_removes_repo_and_files_and_updates_lock_and_config() {
        // Setup isolated test environment
        let mut env = TestEnvironmentSetup::new();

        // Ensure pez uses our isolated dirs
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let prev_fc = std::env::var_os("__fish_config_dir");
        let prev_pc = std::env::var_os("PEZ_CONFIG_DIR");
        let prev_pd = std::env::var_os("PEZ_DATA_DIR");
        unsafe {
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
        }

        // Create config with one plugin spec
        let repo = PluginRepo {
            host: None,
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
            source: repo.default_remote_source(),
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
        }
    }

    #[test]
    fn test_uninstall_honors_target_dir_override() {
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();

        let prev_fc = std::env::var_os("__fish_config_dir");
        let prev_pc = std::env::var_os("PEZ_CONFIG_DIR");
        let prev_pd = std::env::var_os("PEZ_DATA_DIR");
        let prev_pt = std::env::var_os("PEZ_TARGET_DIR");

        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "alt".into(),
        };

        let override_dir = env.fish_config_dir.join("alt_target");
        std::fs::create_dir_all(&override_dir).unwrap();

        unsafe {
            std::env::remove_var("__fish_config_dir");
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
            std::env::set_var("PEZ_TARGET_DIR", &override_dir);
        }

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
        env.setup_data_repo(vec![repo.clone()]);

        let target_dir = override_dir.join(TargetDir::Functions.as_str());
        std::fs::create_dir_all(&target_dir).unwrap();
        let target_file = target_dir.join("alt.fish");
        std::fs::write(&target_file, "echo hi").unwrap();

        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![crate::lock_file::Plugin {
                name: "alt".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc1234".into(),
                files: vec![PluginFile {
                    dir: TargetDir::Functions,
                    name: "alt.fish".into(),
                }],
            }],
        });

        uninstall(&repo, true).expect("uninstall should succeed");

        assert!(std::fs::metadata(&target_file).is_err());

        unsafe {
            if let Some(v) = prev_fc {
                std::env::set_var("__fish_config_dir", v);
            } else {
                std::env::remove_var("__fish_config_dir");
            }
            if let Some(v) = prev_pc {
                std::env::set_var("PEZ_CONFIG_DIR", v);
            } else {
                std::env::remove_var("PEZ_CONFIG_DIR");
            }
            if let Some(v) = prev_pd {
                std::env::set_var("PEZ_DATA_DIR", v);
            } else {
                std::env::remove_var("PEZ_DATA_DIR");
            }
            if let Some(v) = prev_pt {
                std::env::set_var("PEZ_TARGET_DIR", v);
            } else {
                std::env::remove_var("PEZ_TARGET_DIR");
            }
        }
    }

    #[test]
    fn test_uninstall_logs_repo_missing_without_force() {
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

        // Create config with one plugin and lockfile with one file entry
        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "missing".into(),
        };
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
        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![crate::lock_file::Plugin {
                name: "missing".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc1234".into(),
                files: vec![PluginFile {
                    dir: TargetDir::Functions,
                    name: "hello.fish".into(),
                }],
            }],
        });

        // Act: repo dir does not exist and force = false
        let (logs, res) = capture_logs(|| uninstall(&repo, false));
        assert!(res.is_err());
        let joined = logs.join("\n");
        assert!(joined.contains("[Warning]"));
        assert!(joined.contains("Repository directory at"));
        assert!(joined.contains("Detected plugin files based on pez-lock.toml"));
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

    #[test]
    fn uninstall_preserves_state_when_plugin_file_removal_fails() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PATH",
            "PEZ_SUPPRESS_EMIT",
            "__fish_config_dir",
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
        ]);
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
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
        }

        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "blocked".into(),
        };
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
        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![crate::lock_file::Plugin {
                name: "blocked".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc1234".into(),
                files: vec![PluginFile {
                    dir: TargetDir::ConfD,
                    name: "blocked.fish".into(),
                }],
            }],
        });
        env.setup_data_repo(vec![repo.clone()]);

        let blocked_path = env
            .fish_config_dir
            .join(TargetDir::ConfD.as_str())
            .join("blocked.fish");
        std::fs::create_dir_all(blocked_path.parent().unwrap()).unwrap();
        std::fs::write(&blocked_path, "echo blocked\n").unwrap();
        let conf_d_dir = blocked_path.parent().unwrap();
        let original_perms = std::fs::metadata(conf_d_dir).unwrap().permissions();
        let mut read_only_perms = original_perms.clone();
        read_only_perms.set_mode(0o500);
        std::fs::set_permissions(conf_d_dir, read_only_perms).unwrap();

        let err = uninstall(&repo, true).expect_err("file removal failure should abort uninstall");
        std::fs::set_permissions(conf_d_dir, original_perms).unwrap();
        let err_text = format!("{:#}", err);
        assert!(err_text.contains("Failed to remove"), "{err_text}");

        let saved_lock = lock_file::load(&env.lock_file_path).unwrap();
        assert!(saved_lock.get_plugin_by_repo(&repo).is_some());

        let saved_config = config::load(&env.config_path).unwrap();
        assert!(
            saved_config.plugins.unwrap_or_default().iter().any(|p| p
                .get_plugin_repo()
                .ok()
                .as_ref()
                == Some(&repo))
        );
        assert!(blocked_path.is_file());
        assert!(
            env.data_dir.join(repo.as_str()).exists(),
            "repo directory should remain when file deletion fails"
        );
        assert!(
            !log_path.exists(),
            "uninstall event should not be emitted when deletion is rejected"
        );
    }

    #[test]
    fn uninstall_preserves_repo_when_lock_save_fails() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_SUPPRESS_EMIT",
            "__fish_config_dir",
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
        ]);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
        }

        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "blocked".into(),
        };
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
        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![crate::lock_file::Plugin {
                name: "blocked".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc1234".into(),
                files: vec![PluginFile {
                    dir: TargetDir::ConfD,
                    name: "blocked.fish".into(),
                }],
            }],
        });
        env.setup_data_repo(vec![repo.clone()]);
        env.setup_fish_config();
        let repo_path = env.data_dir.join(repo.as_str());
        let plugin_file = env
            .fish_config_dir
            .join(TargetDir::ConfD.as_str())
            .join("blocked.fish");
        let original_perms = std::fs::metadata(&env.lock_file_path)
            .unwrap()
            .permissions();
        let mut read_only_perms = original_perms.clone();
        read_only_perms.set_mode(0o400);
        std::fs::set_permissions(&env.lock_file_path, read_only_perms).unwrap();

        let err = uninstall(&repo, true).expect_err("lock save failure should abort uninstall");
        std::fs::set_permissions(&env.lock_file_path, original_perms).unwrap();
        let err_text = format!("{:#}", err);
        assert!(err_text.contains("Permission denied"), "{err_text}");
        assert!(
            repo_path.exists(),
            "repo directory should remain when lock save fails"
        );
        assert!(
            plugin_file.exists(),
            "staged plugin files should be restored when lock save fails"
        );
        let saved_lock = lock_file::load(&env.lock_file_path).unwrap();
        assert!(saved_lock.get_plugin_by_repo(&repo).is_some());
        let saved_config = config::load(&env.config_path).unwrap();
        assert!(
            saved_config.plugins.unwrap_or_default().iter().any(|p| p
                .get_plugin_repo()
                .ok()
                .as_ref()
                == Some(&repo))
        );
    }

    #[test]
    fn uninstall_preserves_state_when_config_save_fails() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PEZ_SUPPRESS_EMIT",
            "__fish_config_dir",
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
        ]);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
        }

        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "blocked".into(),
        };
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
        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![crate::lock_file::Plugin {
                name: "blocked".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc1234".into(),
                files: vec![PluginFile {
                    dir: TargetDir::ConfD,
                    name: "blocked.fish".into(),
                }],
            }],
        });
        env.setup_data_repo(vec![repo.clone()]);
        env.setup_fish_config();
        let repo_path = env.data_dir.join(repo.as_str());
        let plugin_file = env
            .fish_config_dir
            .join(TargetDir::ConfD.as_str())
            .join("blocked.fish");
        let original_perms = std::fs::metadata(&env.config_path).unwrap().permissions();
        let mut read_only_perms = original_perms.clone();
        read_only_perms.set_mode(0o400);
        std::fs::set_permissions(&env.config_path, read_only_perms).unwrap();

        let err = uninstall(&repo, true).expect_err("config save failure should abort uninstall");
        std::fs::set_permissions(&env.config_path, original_perms).unwrap();
        let err_text = format!("{:#}", err);
        assert!(err_text.contains("Permission denied"), "{err_text}");
        assert!(
            repo_path.exists(),
            "repo directory should remain when config save fails"
        );
        assert!(
            plugin_file.exists(),
            "staged plugin files should be restored when config save fails"
        );
        let saved_lock = lock_file::load(&env.lock_file_path).unwrap();
        assert!(saved_lock.get_plugin_by_repo(&repo).is_some());
        let saved_config = config::load(&env.config_path).unwrap();
        assert!(
            saved_config.plugins.unwrap_or_default().iter().any(|p| p
                .get_plugin_repo()
                .ok()
                .as_ref()
                == Some(&repo))
        );
    }

    #[test]
    fn uninstall_emits_events_only_for_conf_d_files() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let mut env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "PATH",
            "PEZ_SUPPRESS_EMIT",
            "__fish_config_dir",
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
        ]);

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
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
        }

        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "emit".into(),
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
        env.setup_data_repo(vec![repo.clone()]);

        let conf_dir = env.fish_config_dir.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&conf_dir).unwrap();
        std::fs::File::create(conf_dir.join("alpha.fish")).unwrap();
        let func_dir = env.fish_config_dir.join(TargetDir::Functions.as_str());
        std::fs::create_dir_all(&func_dir).unwrap();
        std::fs::File::create(func_dir.join("beta.fish")).unwrap();

        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![crate::lock_file::Plugin {
                name: "emit".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc1234".into(),
                files: vec![
                    PluginFile {
                        dir: TargetDir::ConfD,
                        name: "alpha.fish".into(),
                    },
                    PluginFile {
                        dir: TargetDir::Functions,
                        name: "beta.fish".into(),
                    },
                ],
            }],
        });

        uninstall(&repo, true).expect("uninstall should succeed");

        let log_contents = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert!(log_contents.contains("alpha_uninstall"));
        assert!(!log_contents.contains("beta_uninstall"));
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test(flavor = "multi_thread")]
    async fn run_bails_without_plugins_or_stdin() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        crate::utils::clear_cli_jobs_override_for_tests();
        let _guard = StdinGuard::new(Some("owner/from-stdin\n".to_string()));
        let args = UninstallArgs {
            plugins: None,
            force: false,
            stdin: false,
        };
        let err = run(&args).await.expect_err("expected failure");
        assert!(
            err.to_string()
                .contains("No plugins specified for uninstall")
        );
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test(flavor = "multi_thread")]
    async fn run_reads_plugins_from_stdin() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        crate::utils::clear_cli_jobs_override_for_tests();
        let mut env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "__fish_config_dir",
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_JOBS",
        ]);
        unsafe {
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
            std::env::set_var("PEZ_JOBS", "1");
        }

        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "stdin".into(),
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
        env.setup_data_repo(vec![repo.clone()]);

        let target_dir = env.fish_config_dir.join(TargetDir::Functions.as_str());
        std::fs::create_dir_all(&target_dir).unwrap();
        let target_file = target_dir.join("stdin.fish");
        std::fs::File::create(&target_file).unwrap();

        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![crate::lock_file::Plugin {
                name: "stdin".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc1234".into(),
                files: vec![PluginFile {
                    dir: TargetDir::Functions,
                    name: "stdin.fish".into(),
                }],
            }],
        });

        let _guard = StdinGuard::new(Some(format!("{}\n", repo.as_str())));
        let args = UninstallArgs {
            plugins: None,
            force: true,
            stdin: true,
        };
        run(&args).await.expect("run should succeed");

        assert!(std::fs::metadata(&target_file).is_err());
        let lock = lock_file::load(&env.lock_file_path).unwrap();
        assert!(lock.plugins.is_empty());
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test(flavor = "multi_thread")]
    async fn run_uninstalls_plugins_from_args() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        crate::utils::clear_cli_jobs_override_for_tests();
        let mut env = TestEnvironmentSetup::new();
        let _override = EnvOverride::new(&[
            "__fish_config_dir",
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_JOBS",
        ]);
        unsafe {
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
            std::env::set_var("PEZ_JOBS", "1");
        }

        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "args".into(),
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
        env.setup_data_repo(vec![repo.clone()]);

        let target_dir = env.fish_config_dir.join(TargetDir::Functions.as_str());
        std::fs::create_dir_all(&target_dir).unwrap();
        let target_file = target_dir.join("args.fish");
        std::fs::File::create(&target_file).unwrap();

        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![crate::lock_file::Plugin {
                name: "args".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc1234".into(),
                files: vec![PluginFile {
                    dir: TargetDir::Functions,
                    name: "args.fish".into(),
                }],
            }],
        });

        let args = UninstallArgs {
            plugins: Some(vec![repo.clone()]),
            force: true,
            stdin: false,
        };
        run(&args).await.expect("run should succeed");

        assert!(std::fs::metadata(&target_file).is_err());
        let lock = lock_file::load(&env.lock_file_path).unwrap();
        assert!(lock.plugins.is_empty());
    }
}
