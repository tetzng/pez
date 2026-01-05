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
    use crate::lock_file::{self, LockFile, PluginFile};
    use crate::tests_support::env::TestEnvironmentSetup;
    use crate::tests_support::log::capture_logs;
    use std::ffi::OsString;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

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

    fn commit_paths(repo: &git2::Repository, paths: &[&str], message: &str) -> String {
        let mut index = repo.index().unwrap();
        for path in paths {
            index.add_path(Path::new(path)).unwrap();
        }
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("tester", "tester@example.com").unwrap();
        let parent = repo
            .head()
            .ok()
            .and_then(|head| head.target())
            .and_then(|oid| repo.find_commit(oid).ok());
        let commit_id = match parent {
            Some(ref parent) => repo
                .commit(
                    Some("refs/heads/main"),
                    &sig,
                    &sig,
                    message,
                    &tree,
                    &[parent],
                )
                .unwrap(),
            None => repo
                .commit(Some("refs/heads/main"), &sig, &sig, message, &tree, &[])
                .unwrap(),
        };
        commit_id.to_string()
    }

    fn push_origin_refs(repo: &git2::Repository, refs: &[&str]) {
        let mut remote = repo.find_remote("origin").unwrap();
        remote
            .connect(git2::Direction::Push)
            .and_then(|_| remote.push(refs, None))
            .unwrap();
    }

    fn push_origin(repo: &git2::Repository) {
        push_origin_refs(repo, &["refs/heads/main:refs/heads/main"]);
    }

    fn init_origin_with_two_commits() -> (tempfile::TempDir, PathBuf, String, String) {
        let tmp = tempfile::tempdir().unwrap();
        let origin_path = tmp.path().join("origin.git");
        let workdir_path = tmp.path().join("work");
        let origin = git2::Repository::init_bare(&origin_path).unwrap();
        let work = git2::Repository::init(&workdir_path).unwrap();

        std::fs::create_dir_all(workdir_path.join(TargetDir::ConfD.as_str())).unwrap();
        std::fs::create_dir_all(workdir_path.join(TargetDir::Functions.as_str())).unwrap();
        std::fs::write(
            workdir_path
                .join(TargetDir::ConfD.as_str())
                .join("alpha.fish"),
            "echo one\n",
        )
        .unwrap();
        std::fs::write(
            workdir_path
                .join(TargetDir::Functions.as_str())
                .join("beta.fish"),
            "echo beta\n",
        )
        .unwrap();

        let first = commit_paths(
            &work,
            &["conf.d/alpha.fish", "functions/beta.fish"],
            "first commit",
        );
        work.remote("origin", origin_path.to_str().unwrap())
            .unwrap();
        let first_commit = work
            .find_commit(git2::Oid::from_str(&first).unwrap())
            .unwrap();
        work.branch("pinned", &first_commit, false).unwrap();
        push_origin_refs(
            &work,
            &[
                "refs/heads/main:refs/heads/main",
                "refs/heads/pinned:refs/heads/pinned",
            ],
        );
        origin.set_head("refs/heads/main").unwrap();

        std::fs::write(
            workdir_path
                .join(TargetDir::ConfD.as_str())
                .join("alpha.fish"),
            "echo two\n",
        )
        .unwrap();
        let second = commit_paths(&work, &["conf.d/alpha.fish"], "second commit");
        push_origin(&work);
        origin.set_head("refs/heads/main").unwrap();

        (tmp, origin_path, first, second)
    }

    struct UpgradeFixture {
        _origin_tmp: tempfile::TempDir,
        env: TestEnvironmentSetup,
        repo: PluginRepo,
        first_commit: String,
        second_commit: String,
    }

    impl UpgradeFixture {
        fn new(include_in_config: bool) -> Self {
            let (origin_tmp, origin_path, first, second) = init_origin_with_two_commits();
            let mut env = TestEnvironmentSetup::new();
            let repo = PluginRepo {
                host: None,
                owner: "owner".into(),
                repo: "upgrade".into(),
            };
            let repo_path = env.data_dir.join(repo.as_str());
            crate::git::clone_repository(origin_path.to_str().unwrap(), &repo_path).unwrap();

            let config = if include_in_config {
                config::Config {
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
                }
            } else {
                config::Config { plugins: None }
            };
            env.setup_config(config);

            env.setup_lock_file(LockFile {
                version: 1,
                plugins: vec![crate::lock_file::Plugin {
                    name: "upgrade".into(),
                    repo: repo.clone(),
                    source: "https://example.com/owner/upgrade".into(),
                    commit_sha: first.clone(),
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

            Self {
                _origin_tmp: origin_tmp,
                env,
                repo,
                first_commit: first,
                second_commit: second,
            }
        }
    }

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
            host: None,
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

    #[test]
    fn upgrade_plugin_uses_pinned_selection_for_repo() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        crate::utils::clear_cli_jobs_override_for_tests();
        let mut fixture = UpgradeFixture::new(false);
        let _override = EnvOverride::new(&[
            "PEZ_SUPPRESS_EMIT",
            "__fish_config_dir",
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
        ]);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
            std::env::set_var("__fish_config_dir", &fixture.env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &fixture.env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &fixture.env.data_dir);
        }

        fixture.env.setup_config(config::Config {
            plugins: Some(vec![config::PluginSpec {
                name: None,
                source: config::PluginSource::Repo {
                    repo: fixture.repo.clone(),
                    version: None,
                    branch: Some("pinned".into()),
                    tag: None,
                    commit: None,
                },
            }]),
        });

        upgrade_plugin(&fixture.repo).expect("upgrade should succeed");

        let lock = lock_file::load(&fixture.env.lock_file_path).unwrap();
        let updated = lock.get_plugin_by_repo(&fixture.repo).unwrap();
        assert_eq!(updated.commit_sha, fixture.first_commit);
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test(flavor = "multi_thread")]
    async fn run_upgrades_selected_plugins_and_emits_events() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        crate::utils::clear_cli_jobs_override_for_tests();
        let fixture = UpgradeFixture::new(false);
        let _override = EnvOverride::new(&[
            "PATH",
            "PEZ_SUPPRESS_EMIT",
            "__fish_config_dir",
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_JOBS",
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
            std::env::set_var("__fish_config_dir", &fixture.env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &fixture.env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &fixture.env.data_dir);
            std::env::set_var("PEZ_JOBS", "1");
        }

        let args = UpgradeArgs {
            plugins: Some(vec![fixture.repo.clone()]),
        };
        run(&args).await.expect("run should succeed");

        let cfg = config::load(&fixture.env.config_path).unwrap();
        assert!(
            cfg.plugins
                .unwrap_or_default()
                .iter()
                .any(|p| p.get_plugin_repo().ok().as_ref() == Some(&fixture.repo))
        );

        let lock = lock_file::load(&fixture.env.lock_file_path).unwrap();
        let updated = lock.get_plugin_by_repo(&fixture.repo).unwrap();
        assert_eq!(updated.commit_sha, fixture.second_commit);

        let log_contents = std::fs::read_to_string(&log_path).unwrap_or_default();
        assert!(log_contents.contains("emit alpha_update"));
        assert!(!log_contents.contains("emit beta_update"));
    }

    #[allow(clippy::await_holding_lock)]
    #[tokio::test(flavor = "multi_thread")]
    async fn run_upgrades_all_plugins() {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        crate::utils::clear_cli_jobs_override_for_tests();
        let fixture = UpgradeFixture::new(true);
        let _override = EnvOverride::new(&[
            "PEZ_SUPPRESS_EMIT",
            "__fish_config_dir",
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "PEZ_JOBS",
        ]);
        unsafe {
            std::env::set_var("PEZ_SUPPRESS_EMIT", "1");
            std::env::set_var("__fish_config_dir", &fixture.env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &fixture.env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &fixture.env.data_dir);
            std::env::set_var("PEZ_JOBS", "1");
        }

        let args = UpgradeArgs { plugins: None };
        run(&args).await.expect("run should succeed");

        let lock = lock_file::load(&fixture.env.lock_file_path).unwrap();
        let updated = lock.get_plugin_by_repo(&fixture.repo).unwrap();
        assert_eq!(updated.commit_sha, fixture.second_commit);
    }
}
