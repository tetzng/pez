use crate::{cli, config, git, lock_file::Plugin, resolver, utils};

use console::Emoji;
use serde_json::json;
use std::io;
use tabled::{Table, Tabled};
use tracing::{info, warn};

#[derive(Debug, Tabled)]
struct PluginRow {
    name: String,
    repo: String,
    source: String,
    selector: String,
    commit: String,
}

#[derive(Debug, Tabled)]
struct PluginOutdatedRow {
    name: String,
    repo: String,
    source: String,
    current: String,
    latest: String,
}

struct OutdatedPlugin {
    plugin: Plugin,
    latest: String,
}

pub(crate) fn run(args: &cli::ListArgs) -> anyhow::Result<()> {
    let result = utils::load_lock_file();
    if result.is_err() {
        info!("No plugins installed!");
        return Ok(());
    }

    let config_opt = utils::load_config().ok().map(|(c, _)| c);
    let (lock_file, _) = match result {
        Ok(v) => v,
        Err(_) => {
            info!("No plugins installed!");
            return Ok(());
        }
    };
    let mut plugins = lock_file.plugins.clone();
    if let Some(filter) = &args.filter {
        match filter {
            cli::ListFilter::All => {}
            cli::ListFilter::Local => plugins.retain(|p| git::is_local_source(&p.source)),
            cli::ListFilter::Remote => plugins.retain(|p| !git::is_local_source(&p.source)),
        }
    }
    let plugins = &plugins;
    if plugins.is_empty() {
        info!("No plugins installed!");
        return Ok(());
    }

    if args.outdated {
        match args.format.clone().unwrap_or(cli::ListFormat::Plain) {
            cli::ListFormat::Table => list_outdated_table(plugins, config_opt.as_ref())?,
            cli::ListFormat::Json => list_outdated_json(plugins, config_opt.as_ref())?,
            cli::ListFormat::Plain => list_outdated(plugins, config_opt.as_ref())?,
        }
    } else {
        match args.format.clone().unwrap_or(cli::ListFormat::Plain) {
            cli::ListFormat::Table => display_plugins_in_table(plugins, config_opt.as_ref()),
            cli::ListFormat::Json => list_json(plugins, config_opt.as_ref())?,
            cli::ListFormat::Plain => list(plugins)?,
        }
    }

    Ok(())
}

fn list(plugins: &[Plugin]) -> anyhow::Result<()> {
    display_plugins(plugins, io::stdout())?;
    Ok(())
}

fn display_plugins<W: io::Write>(plugins: &[Plugin], mut writer: W) -> anyhow::Result<()> {
    for plugin in plugins {
        writeln!(writer, "{}", plugin.repo)?;
    }

    Ok(())
}

fn display_plugins_in_table(plugins: &[Plugin], config: Option<&crate::config::Config>) {
    fn short7(s: &str) -> String {
        s.chars().take(7).collect()
    }
    fn selector_of(
        cfg: Option<&crate::config::Config>,
        repo: &crate::models::PluginRepo,
    ) -> String {
        let cfg = match cfg {
            Some(c) => c,
            None => return "-".into(),
        };
        let spec = match cfg.plugins.as_ref().and_then(|ps| {
            ps.iter()
                .find(|p| p.get_plugin_repo().ok().as_ref() == Some(repo))
        }) {
            Some(s) => s,
            None => return "-".into(),
        };
        match &spec.source {
            crate::config::PluginSource::Repo {
                version,
                branch,
                tag,
                commit,
                ..
            }
            | crate::config::PluginSource::Url {
                version,
                branch,
                tag,
                commit,
                ..
            } => {
                if let Some(c) = commit {
                    return format!("commit:{}", c);
                }
                if let Some(b) = branch {
                    return format!("branch:{}", b);
                }
                if let Some(t) = tag {
                    return format!("tag:{}", t);
                }
                if let Some(v) = version {
                    return format!("version:{}", v);
                }
                "-".into()
            }
            crate::config::PluginSource::Path { .. } => "local".into(),
        }
    }
    let plugin_rows = plugins
        .iter()
        .map(|p| PluginRow {
            name: p.get_name(),
            repo: p.repo.as_str().clone(),
            source: p.source.clone(),
            selector: selector_of(config, &p.repo),
            commit: short7(&p.commit_sha),
        })
        .collect::<Vec<PluginRow>>();
    let table = Table::new(&plugin_rows);
    println!("{table}");
}

fn list_outdated(plugins: &[Plugin], config: Option<&config::Config>) -> anyhow::Result<()> {
    let outdated_plugins = get_outdated_plugins(plugins, config)?;
    if outdated_plugins.is_empty() {
        info!("{}All plugins are up to date!", Emoji("🎉 ", ""));
        return Ok(());
    }
    let plugins_only: Vec<Plugin> = outdated_plugins
        .into_iter()
        .map(|entry| entry.plugin)
        .collect();
    display_plugins(&plugins_only, io::stdout())?;

    Ok(())
}

fn get_outdated_plugins(
    plugins: &[Plugin],
    config: Option<&config::Config>,
) -> anyhow::Result<Vec<OutdatedPlugin>> {
    let data_dir = utils::load_pez_data_dir()?;
    let mut outdated_plugins: Vec<OutdatedPlugin> = Vec::new();

    for plugin in plugins {
        if git::is_local_source(&plugin.source) {
            continue;
        }

        let repo_path = data_dir.join(plugin.repo.as_str());
        let repo = match git2::Repository::open(&repo_path) {
            Ok(repo) => repo,
            Err(err) => {
                warn!(
                    "Failed to open repository for {} at {}: {err:?}",
                    plugin.repo,
                    repo_path.display()
                );
                continue;
            }
        };

        let mut selection = resolver::Selection::DefaultHead;
        let mut selection_desc = describe_selection(&selection);
        let mut selection_from_config = false;

        if let Some(cfg) = config
            && let Some(specs) = &cfg.plugins
            && let Some(spec) = specs
                .iter()
                .find(|candidate| candidate.get_plugin_repo().ok().as_ref() == Some(&plugin.repo))
        {
            match spec.to_resolved() {
                Ok(resolved) => {
                    if resolved.is_local {
                        // Nothing to compare against for local sources; skip.
                        continue;
                    }
                    selection = resolver::selection_from_ref_kind(&resolved.ref_kind);
                    selection_desc = describe_selection(&selection);
                    selection_from_config = true;
                }
                Err(err) => {
                    warn!(
                        "Failed to interpret selector for {}: {err:?}. Falling back to origin/HEAD.",
                        plugin.repo
                    );
                }
            }
        }

        let latest = match git::resolve_selection(&repo, &selection) {
            Ok(commit) => commit,
            Err(err) => {
                if selection_from_config {
                    warn!(
                        "Unable to resolve {selection_desc} for {}: {err:?}. Falling back to origin/HEAD.",
                        plugin.repo
                    );
                } else {
                    warn!(
                        "Unable to resolve remote state for {} using {selection_desc}: {err:?}. Falling back to origin/HEAD.",
                        plugin.repo
                    );
                }
                match git::get_remote_head_commit(&repo) {
                    Ok(commit) => commit,
                    Err(head_err) => {
                        warn!(
                            "Failed to determine origin/HEAD for {}: {head_err:?}. Skipping outdated check.",
                            plugin.repo
                        );
                        continue;
                    }
                }
            }
        };

        if plugin.commit_sha != latest {
            outdated_plugins.push(OutdatedPlugin {
                plugin: plugin.clone(),
                latest,
            });
        }
    }

    Ok(outdated_plugins)
}

fn list_outdated_table(plugins: &[Plugin], config: Option<&config::Config>) -> anyhow::Result<()> {
    fn short7(s: &str) -> String {
        s.chars().take(7).collect()
    }
    let outdated_plugins = get_outdated_plugins(plugins, config)?;
    if outdated_plugins.is_empty() {
        info!("{}All plugins are up to date!", Emoji("🎉 ", ""));
        return Ok(());
    }

    let plugin_rows = outdated_plugins
        .iter()
        .map(|entry| PluginOutdatedRow {
            name: entry.plugin.get_name(),
            repo: entry.plugin.repo.as_str().clone(),
            source: entry.plugin.source.clone(),
            current: short7(&entry.plugin.commit_sha),
            latest: short7(&entry.latest),
        })
        .collect::<Vec<PluginOutdatedRow>>();
    let table = Table::new(&plugin_rows);
    println!("{table}");

    Ok(())
}

fn list_json(plugins: &[Plugin], config: Option<&crate::config::Config>) -> anyhow::Result<()> {
    fn selector_of(
        cfg: Option<&crate::config::Config>,
        repo: &crate::models::PluginRepo,
    ) -> Option<String> {
        let cfg = cfg?;
        let spec = cfg.plugins.as_ref().and_then(|ps| {
            ps.iter()
                .find(|p| p.get_plugin_repo().ok().as_ref() == Some(repo))
        })?;
        match &spec.source {
            crate::config::PluginSource::Repo {
                version,
                branch,
                tag,
                commit,
                ..
            }
            | crate::config::PluginSource::Url {
                version,
                branch,
                tag,
                commit,
                ..
            } => {
                if let Some(c) = commit {
                    return Some(format!("commit:{}", c));
                }
                if let Some(b) = branch {
                    return Some(format!("branch:{}", b));
                }
                if let Some(t) = tag {
                    return Some(format!("tag:{}", t));
                }
                if let Some(v) = version {
                    return Some(format!("version:{}", v));
                }
                None
            }
            crate::config::PluginSource::Path { .. } => Some("local".into()),
        }
    }
    let value = json!(
        plugins
            .iter()
            .map(|p| json!({
                "name": p.get_name(),
                "repo": p.repo.as_str(),
                "source": p.source,
                "selector": selector_of(config, &p.repo),
                "commit": p.commit_sha,
            }))
            .collect::<Vec<_>>()
    );
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn list_outdated_json(plugins: &[Plugin], config: Option<&config::Config>) -> anyhow::Result<()> {
    let outdated_plugins = get_outdated_plugins(plugins, config)?;
    if outdated_plugins.is_empty() {
        info!("{}All plugins are up to date!", Emoji("🎉 ", ""));
        return Ok(());
    }
    let value = json!(
        outdated_plugins
            .iter()
            .map(|entry| {
                json!({
                    "name": entry.plugin.get_name(),
                    "repo": entry.plugin.repo.as_str(),
                    "source": entry.plugin.source,
                    "current": entry.plugin.commit_sha,
                    "latest": entry.latest,
                })
            })
            .collect::<Vec<_>>()
    );
    println!("{}", serde_json::to_string_pretty(&value)?);
    Ok(())
}

fn describe_selection(selection: &resolver::Selection) -> String {
    match selection {
        resolver::Selection::DefaultHead => "origin/HEAD".to_string(),
        resolver::Selection::Latest => "latest".to_string(),
        resolver::Selection::Branch(name) => format!("branch:{name}"),
        resolver::Selection::Tag(name) => format!("tag:{name}"),
        resolver::Selection::Commit(sha) => format!("commit:{sha}"),
        resolver::Selection::Version(version) => format!("version:{version}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{self, PluginSpec};
    use crate::lock_file::{LockFile, Plugin};
    use crate::models::PluginRepo;
    use crate::tests_support::env::TestEnvironmentSetup;
    use crate::tests_support::log::{capture_logs, env_lock};
    use git2::Direction;
    use std::path::Path;

    #[test]
    fn test_display_plugins() {
        let plugins = vec![
            Plugin {
                name: "name".to_string(),
                repo: PluginRepo {
                    owner: "owner".to_string(),
                    repo: "repo".to_string(),
                },
                source: "source".to_string(),
                commit_sha: "commit_sha".to_string(),
                files: vec![],
            },
            Plugin {
                name: "name2".to_string(),
                repo: PluginRepo {
                    owner: "owner".to_string(),
                    repo: "repo2".to_string(),
                },
                source: "source2".to_string(),
                commit_sha: "commit_sha2".to_string(),
                files: vec![],
            },
        ];

        let mut output = io::Cursor::new(Vec::new());
        display_plugins(&plugins, &mut output).unwrap();

        let actual_output = String::from_utf8(output.into_inner()).unwrap();
        let expected_output = "owner/repo\nowner/repo2\n";

        assert_eq!(actual_output, expected_output);
    }

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

    fn clone_into_data_dir(origin: &Path, env: &TestEnvironmentSetup, repo: &PluginRepo) -> String {
        let repo_path = env.data_dir.join(repo.as_str());
        if let Some(parent) = repo_path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let remote = format!("file://{}", origin.display());
        crate::git::clone_repository(&remote, &repo_path).unwrap();
        remote
    }

    fn init_remote_with_branch(
        branch: &str,
    ) -> (tempfile::TempDir, std::path::PathBuf, String, String) {
        let tmp = tempfile::tempdir().unwrap();
        let origin_path = tmp.path().join("origin.git");
        let work_path = tmp.path().join("work");
        let origin = git2::Repository::init_bare(&origin_path).unwrap();
        let work = git2::Repository::init(&work_path).unwrap();
        {
            let mut cfg = work.config().unwrap();
            cfg.set_str("user.name", "tester").unwrap();
            cfg.set_str("user.email", "tester@example.com").unwrap();
        }
        std::fs::write(work_path.join("README.md"), "initial").unwrap();
        let mut index = work.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = work.find_tree(tree_id).unwrap();
        let sig = work.signature().unwrap();
        let base_commit = work
            .commit(Some("refs/heads/main"), &sig, &sig, "initial", &tree, &[])
            .unwrap();
        work.remote("origin", origin_path.to_str().unwrap())
            .unwrap();
        {
            let mut remote = work.find_remote("origin").unwrap();
            remote.connect(Direction::Push).unwrap();
            remote
                .push(&["refs/heads/main:refs/heads/main"], None)
                .unwrap();
            remote.disconnect().ok();
        }
        origin.set_head("refs/heads/main").unwrap();

        work.branch(branch, &work.find_commit(base_commit).unwrap(), false)
            .unwrap();
        work.set_head(&format!("refs/heads/{branch}")).unwrap();
        work.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .unwrap();
        std::fs::write(work_path.join("FEATURE"), "feature branch").unwrap();
        let mut index = work.index().unwrap();
        index.add_path(Path::new("FEATURE")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = work.find_tree(tree_id).unwrap();
        let branch_commit = work
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "feature commit",
                &tree,
                &[&work.find_commit(base_commit).unwrap()],
            )
            .unwrap();
        {
            let mut remote = work.find_remote("origin").unwrap();
            remote.connect(Direction::Push).unwrap();
            remote
                .push(&[&format!("refs/heads/{branch}:refs/heads/{branch}")], None)
                .unwrap();
            remote.disconnect().ok();
        }

        (
            tmp,
            origin_path,
            base_commit.to_string(),
            branch_commit.to_string(),
        )
    }

    fn init_remote_with_tags() -> (tempfile::TempDir, std::path::PathBuf, String, String) {
        let tmp = tempfile::tempdir().unwrap();
        let origin_path = tmp.path().join("origin.git");
        let work_path = tmp.path().join("work");
        let origin = git2::Repository::init_bare(&origin_path).unwrap();
        let work = git2::Repository::init(&work_path).unwrap();
        {
            let mut cfg = work.config().unwrap();
            cfg.set_str("user.name", "tester").unwrap();
            cfg.set_str("user.email", "tester@example.com").unwrap();
        }
        std::fs::write(work_path.join("README.md"), "initial").unwrap();
        let mut index = work.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = work.find_tree(tree_id).unwrap();
        let sig = work.signature().unwrap();
        let v1_commit = work
            .commit(Some("refs/heads/main"), &sig, &sig, "v1.0.0", &tree, &[])
            .unwrap();
        let base_obj = work.find_object(v1_commit, None).unwrap();
        work.tag("v1.0.0", &base_obj, &sig, "", false).unwrap();

        std::fs::write(work_path.join("CHANGELOG.md"), "updates").unwrap();
        let mut index = work.index().unwrap();
        index.add_path(Path::new("CHANGELOG.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = work.find_tree(tree_id).unwrap();
        let v1_1_commit = work
            .commit(
                Some("refs/heads/main"),
                &sig,
                &sig,
                "v1.1.0",
                &tree,
                &[&work.find_commit(v1_commit).unwrap()],
            )
            .unwrap();
        let latest_obj = work.find_object(v1_1_commit, None).unwrap();
        work.tag("v1.1.0", &latest_obj, &sig, "", false).unwrap();

        work.remote("origin", origin_path.to_str().unwrap())
            .unwrap();
        {
            let mut remote = work.find_remote("origin").unwrap();
            remote.connect(Direction::Push).unwrap();
            remote
                .push(&["refs/heads/main:refs/heads/main"], None)
                .unwrap();
            remote
                .push(
                    &[
                        "refs/tags/v1.0.0:refs/tags/v1.0.0",
                        "refs/tags/v1.1.0:refs/tags/v1.1.0",
                    ],
                    None,
                )
                .unwrap();
            remote.disconnect().ok();
        }
        origin.set_head("refs/heads/main").unwrap();

        (
            tmp,
            origin_path,
            v1_commit.to_string(),
            v1_1_commit.to_string(),
        )
    }

    fn configure_env(env: &TestEnvironmentSetup) -> EnvOverride {
        let guard = EnvOverride::new(&[
            "PEZ_CONFIG_DIR",
            "PEZ_DATA_DIR",
            "__fish_config_dir",
            "PEZ_TARGET_DIR",
            "NO_COLOR",
        ]);
        unsafe {
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::remove_var("PEZ_TARGET_DIR");
            std::env::set_var("NO_COLOR", "1");
        }
        guard
    }

    #[test]
    fn list_outdated_respects_branch_selector() {
        let _lock = env_lock().lock().unwrap();
        let (tmp, origin_path, base_commit, branch_commit) = init_remote_with_branch("feature");
        let mut env = TestEnvironmentSetup::new();
        let _env_guard = configure_env(&env);

        let repo = PluginRepo {
            owner: "owner".into(),
            repo: "pkg".into(),
        };
        let remote = clone_into_data_dir(&origin_path, &env, &repo);

        let config = config::Config {
            plugins: Some(vec![PluginSpec {
                name: None,
                source: config::PluginSource::Repo {
                    repo: repo.clone(),
                    version: None,
                    branch: Some("feature".into()),
                    tag: None,
                    commit: None,
                },
            }]),
        };
        env.setup_config(config.clone());

        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: "pkg".into(),
                repo: repo.clone(),
                source: remote.clone(),
                commit_sha: base_commit.clone(),
                files: vec![],
            }],
        });

        let plugins = env.lock_file.as_ref().unwrap().plugins.clone();
        let outdated = get_outdated_plugins(&plugins, Some(&config)).unwrap();
        assert_eq!(outdated.len(), 1);
        assert_eq!(outdated[0].latest, branch_commit);

        // keep tmp alive
        drop(tmp);
    }

    #[test]
    fn list_outdated_skips_tag_pinned_plugin() {
        let _lock = env_lock().lock().unwrap();
        let (tmp, origin_path, tag_commit, head_commit) = init_remote_with_tags();
        let mut env = TestEnvironmentSetup::new();
        let _env_guard = configure_env(&env);

        let repo = PluginRepo {
            owner: "owner".into(),
            repo: "pkg".into(),
        };
        let remote = clone_into_data_dir(&origin_path, &env, &repo);

        let config = config::Config {
            plugins: Some(vec![PluginSpec {
                name: None,
                source: config::PluginSource::Repo {
                    repo: repo.clone(),
                    version: None,
                    branch: None,
                    tag: Some("v1.0.0".into()),
                    commit: None,
                },
            }]),
        };
        env.setup_config(config.clone());

        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: "pkg".into(),
                repo: repo.clone(),
                source: remote.clone(),
                commit_sha: tag_commit.clone(),
                files: vec![],
            }],
        });

        let plugins = env.lock_file.as_ref().unwrap().plugins.clone();
        let outdated = get_outdated_plugins(&plugins, Some(&config)).unwrap();
        assert!(outdated.is_empty());

        // ensure fixture not dropped early
        assert_ne!(tag_commit, head_commit);
        drop(tmp);
    }

    #[test]
    fn list_outdated_respects_version_selector() {
        let _lock = env_lock().lock().unwrap();
        let (tmp, origin_path, v1_commit, latest_commit) = init_remote_with_tags();
        let mut env = TestEnvironmentSetup::new();
        let _env_guard = configure_env(&env);

        let repo = PluginRepo {
            owner: "owner".into(),
            repo: "pkg".into(),
        };
        let remote = clone_into_data_dir(&origin_path, &env, &repo);

        let config = config::Config {
            plugins: Some(vec![PluginSpec {
                name: None,
                source: config::PluginSource::Repo {
                    repo: repo.clone(),
                    version: Some("v1".into()),
                    branch: None,
                    tag: None,
                    commit: None,
                },
            }]),
        };
        env.setup_config(config.clone());

        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: "pkg".into(),
                repo: repo.clone(),
                source: remote.clone(),
                commit_sha: v1_commit.clone(),
                files: vec![],
            }],
        });

        let plugins = env.lock_file.as_ref().unwrap().plugins.clone();
        let outdated = get_outdated_plugins(&plugins, Some(&config)).unwrap();
        assert_eq!(outdated.len(), 1);
        assert_eq!(outdated[0].latest, latest_commit);
        drop(tmp);
    }

    #[test]
    fn list_outdated_falls_back_when_selector_missing() {
        let _lock = env_lock().lock().unwrap();
        let (tmp, origin_path, base_commit, _) = init_remote_with_branch("feature");
        let mut env = TestEnvironmentSetup::new();
        let _env_guard = configure_env(&env);

        let repo = PluginRepo {
            owner: "owner".into(),
            repo: "pkg".into(),
        };
        let remote = clone_into_data_dir(&origin_path, &env, &repo);

        let config = config::Config {
            plugins: Some(vec![PluginSpec {
                name: None,
                source: config::PluginSource::Repo {
                    repo: repo.clone(),
                    version: None,
                    branch: Some("missing".into()),
                    tag: None,
                    commit: None,
                },
            }]),
        };
        env.setup_config(config.clone());

        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: "pkg".into(),
                repo: repo.clone(),
                source: remote,
                commit_sha: base_commit.clone(),
                files: vec![],
            }],
        });

        let plugins = env.lock_file.as_ref().unwrap().plugins.clone();
        let (logs, result) = capture_logs(|| get_outdated_plugins(&plugins, Some(&config)));
        let outdated = result.unwrap();
        assert!(outdated.is_empty());
        assert!(
            logs.iter()
                .any(|msg| msg.contains("Falling back to origin/HEAD")),
            "logs: {:?}",
            logs
        );

        drop(tmp);
    }
}
