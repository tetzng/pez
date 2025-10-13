use crate::{
    cli::{InstallArgs, MigrateArgs},
    config::{self, PluginSource, PluginSpec},
    models::{InstallTarget, ResolvedInstallTarget},
    utils,
};
use console::Emoji;
use std::{
    fs,
    io::{BufRead, BufReader},
};
use tracing::{error, info, warn};

#[derive(Clone)]
struct MigratedEntry {
    raw: String,
    resolved: ResolvedInstallTarget,
    spec: PluginSpec,
}

impl MigratedEntry {
    fn new(raw: String, resolved: ResolvedInstallTarget) -> Self {
        let spec = config::PluginSpec::from_resolved(&resolved);
        Self {
            raw,
            resolved,
            spec,
        }
    }
}

fn dedup_entries(entries: Vec<MigratedEntry>) -> Vec<MigratedEntry> {
    let mut unique: Vec<MigratedEntry> = Vec::new();
    for entry in entries {
        if let Some(pos) = unique
            .iter()
            .position(|existing| existing.resolved.plugin_repo == entry.resolved.plugin_repo)
        {
            unique[pos] = entry;
        } else {
            unique.push(entry);
        }
    }
    unique
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MigratedRef {
    Version(String),
    Tag(String),
    Branch(String),
    Commit(String),
}

impl MigratedRef {
    fn into_suffix(self) -> String {
        match self {
            MigratedRef::Version(v) => v,
            MigratedRef::Tag(t) => format!("tag:{t}"),
            MigratedRef::Branch(b) => format!("branch:{b}"),
            MigratedRef::Commit(c) => format!("commit:{c}"),
        }
    }
}

fn spec_ref_descriptor(spec: &PluginSpec) -> Option<MigratedRef> {
    match &spec.source {
        PluginSource::Repo {
            version,
            branch,
            tag,
            commit,
            ..
        }
        | PluginSource::Url {
            version,
            branch,
            tag,
            commit,
            ..
        } => {
            if let Some(v) = version {
                Some(MigratedRef::Version(v.clone()))
            } else if let Some(t) = tag {
                Some(MigratedRef::Tag(t.clone()))
            } else if let Some(b) = branch {
                Some(MigratedRef::Branch(b.clone()))
            } else {
                commit.as_ref().map(|c| MigratedRef::Commit(c.clone()))
            }
        }
        PluginSource::Path { .. } => None,
    }
}

fn should_update_existing(existing: &PluginSpec, incoming: &PluginSpec) -> bool {
    if existing.source == incoming.source {
        return false;
    }
    let existing_ref = spec_ref_descriptor(existing);
    let incoming_ref = spec_ref_descriptor(incoming);

    match (&existing_ref, &incoming_ref) {
        (Some(_), None) => false,
        (Some(existing_ref), Some(incoming_ref)) => existing_ref != incoming_ref,
        (_, Some(_)) => true,
        (None, None) => {
            let existing_is_url = matches!(existing.source, PluginSource::Url { .. });
            let incoming_is_url = matches!(incoming.source, PluginSource::Url { .. });
            if existing_is_url && !incoming_is_url {
                return false;
            }
            let existing_is_path = matches!(existing.source, PluginSource::Path { .. });
            if existing_is_path && !matches!(incoming.source, PluginSource::Path { .. }) {
                return false;
            }
            true
        }
    }
}

fn describe_spec(spec: &PluginSpec) -> String {
    let mut base = match &spec.source {
        PluginSource::Repo { repo, .. } => repo.as_str(),
        PluginSource::Url { url, .. } => url.clone(),
        PluginSource::Path { path } => path.clone(),
    };
    if base.is_empty() {
        base = spec
            .get_plugin_repo()
            .map(|repo| repo.as_str())
            .unwrap_or_else(|_| String::new());
    }

    let suffix = spec_ref_descriptor(spec).map(|r| r.into_suffix());

    match suffix {
        Some(s) if !s.is_empty() => format!("{base}@{s}"),
        _ => base,
    }
}

pub(crate) async fn run(args: &MigrateArgs) -> anyhow::Result<()> {
    let fish_config_dir = utils::load_fish_config_dir()?;
    let fisher_plugins_path = fish_config_dir.join("fish_plugins");
    if !fisher_plugins_path.exists() {
        error!(
            "{}fish_plugins not found at {}",
            Emoji("❌ ", ""),
            fisher_plugins_path.display()
        );
        anyhow::bail!("fish_plugins not found");
    }

    info!(
        "{}Reading {}",
        Emoji("📄 ", ""),
        fisher_plugins_path.display()
    );

    let file = fs::File::open(&fisher_plugins_path)?;
    let reader = BufReader::new(file);
    let mut entries: Vec<MigratedEntry> = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((_, suffix)) = trimmed.split_once('@')
            && suffix.trim().is_empty()
        {
            warn!(
                "{}Skipping entry with empty ref suffix: {}",
                Emoji("⚠ ", ""),
                trimmed
            );
            continue;
        }
        let target = InstallTarget::from_raw(trimmed);
        match target.resolve() {
            Ok(resolved) => {
                let looks_like_url = resolved.source.contains("://")
                    || resolved.source.starts_with("git@")
                    || resolved.source.starts_with("ssh://");
                let repo_has_ref_suffix = resolved.plugin_repo.repo.contains('@');
                if looks_like_url
                    && repo_has_ref_suffix
                    && matches!(resolved.ref_kind, crate::resolver::RefKind::None)
                {
                    warn!(
                        "{}Skipping URL entry with ref suffix: {}",
                        Emoji("⚠ ", ""),
                        trimmed
                    );
                    continue;
                }
                if resolved.plugin_repo.owner == "jorgebucaran"
                    && resolved.plugin_repo.repo == "fisher"
                {
                    continue;
                }
                entries.push(MigratedEntry::new(trimmed.to_string(), resolved));
            }
            Err(err) => warn!(
                "{}Skipping unrecognized entry: {} ({err})",
                Emoji("⚠ ", ""),
                trimmed
            ),
        }
    }

    if entries.is_empty() {
        warn!("{}No valid entries to migrate.", Emoji("⚠ ", ""));
        return Ok(());
    }

    let entries = dedup_entries(entries);
    if entries.is_empty() {
        warn!("{}No valid entries to migrate.", Emoji("⚠ ", ""));
        return Ok(());
    }

    let (mut cfg, cfg_path) = utils::load_or_create_config()?;
    let mut planned: Vec<MigratedEntry> = Vec::new();

    if args.force {
        planned = entries.clone();
        if !args.dry_run {
            let specs: Vec<PluginSpec> = planned.iter().map(|entry| entry.spec.clone()).collect();
            cfg.plugins = Some(specs);
        }
    } else if let Some(list) = cfg.plugins.as_mut() {
        for entry in &entries {
            let repo = &entry.resolved.plugin_repo;
            let existing_index = list.iter().position(|spec| {
                spec.get_plugin_repo()
                    .is_ok_and(|existing_repo| existing_repo == *repo)
            });

            if let Some(idx) = existing_index {
                if should_update_existing(&list[idx], &entry.spec) {
                    if !args.dry_run {
                        let existing = &mut list[idx];
                        existing.source = entry.spec.source.clone();
                    }
                    planned.push(entry.clone());
                }
            } else {
                if !args.dry_run {
                    list.push(entry.spec.clone());
                }
                planned.push(entry.clone());
            }
        }
    } else {
        planned = entries.clone();
        if !args.dry_run {
            let specs: Vec<PluginSpec> = planned.iter().map(|entry| entry.spec.clone()).collect();
            cfg.plugins = Some(specs);
        }
    }

    if args.dry_run {
        info!("{}Dry run: planned updates to pez.toml", Emoji("🧪 ", ""));
    } else {
        cfg.save(&cfg_path)?;
        info!("{}Updated {}", Emoji("✅ ", ""), cfg_path.display());
    }
    for p in &planned {
        println!("  - {}", describe_spec(&p.spec));
    }
    if planned.is_empty() {
        info!("{}Nothing to update.", Emoji("ℹ ", ""));
    }

    if !args.dry_run && args.install && !planned.is_empty() {
        let targets: Vec<_> = planned
            .iter()
            .map(|entry| InstallTarget::from_raw(entry.raw.clone()))
            .collect();
        let install_args = InstallArgs {
            plugins: Some(targets),
            force: false,
            prune: false,
        };
        info!("{}Installing migrated plugins...", Emoji("🚀 ", ""));
        crate::cmd::install::run(&install_args).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config, models::PluginRepo, tests_support::env::TestEnvironmentSetup};
    use std::fs;

    struct EnvGuard(Vec<(&'static str, Option<std::ffi::OsString>)>);

    impl EnvGuard {
        fn set(vars: &[(&'static str, std::ffi::OsString)]) -> Self {
            let saved = vars
                .iter()
                .map(|(key, _)| (*key, std::env::var_os(key)))
                .collect::<Vec<_>>();
            for (key, value) in vars {
                unsafe {
                    std::env::set_var(key, value);
                }
            }
            Self(saved)
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, original) in &self.0 {
                if let Some(value) = original {
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

    fn run_migrate(args: &MigrateArgs) -> anyhow::Result<()> {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(super::run(args))
    }

    #[test]
    fn migrates_versioned_entries() {
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::set(&[
            (
                "PEZ_TARGET_DIR",
                env.fish_config_dir.clone().into_os_string(),
            ),
            ("PEZ_CONFIG_DIR", env.config_dir.clone().into_os_string()),
        ]);

        env.setup_config(config::init());

        let fish_plugins_path = env.fish_config_dir.join("fish_plugins");
        fs::write(
            &fish_plugins_path,
            "IlanCosman/tide@v5\njoseluisq/gitnow@2.13.0\n",
        )
        .unwrap();

        let args = MigrateArgs {
            dry_run: false,
            force: false,
            install: false,
        };

        run_migrate(&args).unwrap();

        let cfg = config::load(&env.config_path).unwrap();
        let plugins = cfg.plugins.expect("plugins written");
        assert_eq!(plugins.len(), 2);

        let tide = plugins
            .iter()
            .find(|spec| {
                spec.get_plugin_repo()
                    .map(|repo| repo.as_str() == "IlanCosman/tide")
                    .unwrap_or(false)
            })
            .expect("tide entry");
        match &tide.source {
            PluginSource::Repo { version, .. } => {
                assert_eq!(version.as_deref(), Some("v5"));
            }
            other => panic!("expected repo source, got {other:?}"),
        }

        let gitnow = plugins
            .iter()
            .find(|spec| {
                spec.get_plugin_repo()
                    .map(|repo| repo.as_str() == "joseluisq/gitnow")
                    .unwrap_or(false)
            })
            .expect("gitnow entry");
        match &gitnow.source {
            PluginSource::Repo { version, .. } => {
                assert_eq!(version.as_deref(), Some("2.13.0"));
            }
            other => panic!("expected repo source, got {other:?}"),
        }
    }

    #[test]
    fn updates_existing_entry_with_version() {
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::set(&[
            (
                "PEZ_TARGET_DIR",
                env.fish_config_dir.clone().into_os_string(),
            ),
            ("PEZ_CONFIG_DIR", env.config_dir.clone().into_os_string()),
        ]);

        let existing_spec = PluginSpec {
            name: Some("gitnow".to_string()),
            source: PluginSource::Repo {
                repo: PluginRepo {
                    host: None,
                    owner: "joseluisq".to_string(),
                    repo: "gitnow".to_string(),
                },
                version: None,
                branch: None,
                tag: None,
                commit: None,
            },
        };
        env.setup_config(config::Config {
            plugins: Some(vec![existing_spec]),
        });

        let target = InstallTarget::from_raw("joseluisq/gitnow@2.13.0");
        let resolved = target.resolve().unwrap();
        let spec = PluginSpec::from_resolved(&resolved);
        match spec.source {
            PluginSource::Repo { version, .. } => {
                assert_eq!(version.as_deref(), Some("2.13.0"));
            }
            _ => panic!("expected repo spec"),
        }

        let fish_plugins_path = env.fish_config_dir.join("fish_plugins");
        fs::write(&fish_plugins_path, "joseluisq/gitnow@2.13.0\n").unwrap();

        let args = MigrateArgs {
            dry_run: false,
            force: false,
            install: false,
        };
        run_migrate(&args).unwrap();

        let cfg = config::load(&env.config_path).unwrap();
        let plugins = cfg.plugins.expect("plugins written");
        assert_eq!(plugins.len(), 1);

        match &plugins[0].source {
            PluginSource::Repo { version, .. } => {
                assert_eq!(version.as_deref(), Some("2.13.0"));
            }
            other => panic!("expected repo source, got {other:?}"),
        }
    }

    #[test]
    fn updates_existing_pinned_entry_when_ref_changes() {
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::set(&[
            (
                "PEZ_TARGET_DIR",
                env.fish_config_dir.clone().into_os_string(),
            ),
            ("PEZ_CONFIG_DIR", env.config_dir.clone().into_os_string()),
        ]);

        let existing_spec = PluginSpec {
            name: Some("gitnow".to_string()),
            source: PluginSource::Repo {
                repo: PluginRepo {
                    host: None,
                    owner: "joseluisq".to_string(),
                    repo: "gitnow".to_string(),
                },
                version: Some("1.0.0".to_string()),
                branch: None,
                tag: None,
                commit: None,
            },
        };
        env.setup_config(config::Config {
            plugins: Some(vec![existing_spec]),
        });

        let fish_plugins_path = env.fish_config_dir.join("fish_plugins");
        fs::write(&fish_plugins_path, "joseluisq/gitnow@2.13.0\n").unwrap();

        let args = MigrateArgs {
            dry_run: false,
            force: false,
            install: false,
        };
        run_migrate(&args).unwrap();

        let cfg = config::load(&env.config_path).unwrap();
        let plugins = cfg.plugins.expect("plugins written");
        assert_eq!(plugins.len(), 1);

        match &plugins[0].source {
            PluginSource::Repo { version, .. } => {
                assert_eq!(version.as_deref(), Some("2.13.0"));
            }
            other => panic!("expected repo source, got {other:?}"),
        }
        assert_eq!(plugins[0].name.as_deref(), Some("gitnow"));
    }

    #[test]
    fn keeps_existing_version_when_migrated_entry_is_unpinned() {
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::set(&[
            (
                "PEZ_TARGET_DIR",
                env.fish_config_dir.clone().into_os_string(),
            ),
            ("PEZ_CONFIG_DIR", env.config_dir.clone().into_os_string()),
        ]);

        let existing_spec = PluginSpec {
            name: None,
            source: PluginSource::Repo {
                repo: PluginRepo {
                    host: None,
                    owner: "IlanCosman".to_string(),
                    repo: "tide".to_string(),
                },
                version: Some("v5".to_string()),
                branch: None,
                tag: None,
                commit: None,
            },
        };
        env.setup_config(config::Config {
            plugins: Some(vec![existing_spec]),
        });

        let fish_plugins_path = env.fish_config_dir.join("fish_plugins");
        fs::write(&fish_plugins_path, "IlanCosman/tide\n").unwrap();

        let args = MigrateArgs {
            dry_run: false,
            force: false,
            install: false,
        };
        run_migrate(&args).unwrap();

        let cfg = config::load(&env.config_path).unwrap();
        let plugins = cfg.plugins.expect("plugins written");
        assert_eq!(plugins.len(), 1);

        match &plugins[0].source {
            PluginSource::Repo { version, .. } => {
                assert_eq!(version.as_deref(), Some("v5"));
            }
            other => panic!("expected repo source, got {other:?}"),
        }
    }

    #[test]
    fn skips_url_entries_with_ref_suffix() {
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::set(&[
            (
                "PEZ_TARGET_DIR",
                env.fish_config_dir.clone().into_os_string(),
            ),
            ("PEZ_CONFIG_DIR", env.config_dir.clone().into_os_string()),
        ]);

        env.setup_config(config::Config { plugins: None });

        let fish_plugins_path = env.fish_config_dir.join("fish_plugins");
        fs::write(
            &fish_plugins_path,
            "https://gitlab.com/foo/bar@branch:main\n",
        )
        .unwrap();

        let args = MigrateArgs {
            dry_run: false,
            force: false,
            install: false,
        };
        run_migrate(&args).unwrap();

        let cfg = config::load(&env.config_path).unwrap();
        assert!(cfg.plugins.is_none());
    }

    #[test]
    fn migrates_ssh_url_entry() {
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::set(&[
            (
                "PEZ_TARGET_DIR",
                env.fish_config_dir.clone().into_os_string(),
            ),
            ("PEZ_CONFIG_DIR", env.config_dir.clone().into_os_string()),
        ]);

        env.setup_config(config::Config { plugins: None });

        let fish_plugins_path = env.fish_config_dir.join("fish_plugins");
        fs::write(&fish_plugins_path, "git@bitbucket.org:team/pkg.git\n").unwrap();

        let args = MigrateArgs {
            dry_run: false,
            force: false,
            install: false,
        };
        run_migrate(&args).unwrap();

        let cfg = config::load(&env.config_path).unwrap();
        let plugins = cfg.plugins.expect("plugins written");
        assert_eq!(plugins.len(), 1);
        match &plugins[0].source {
            PluginSource::Url { url, .. } => {
                assert_eq!(url, "git@bitbucket.org:team/pkg.git");
            }
            other => panic!("expected Url source, got {other:?}"),
        }
        let repo = plugins[0].get_plugin_repo().unwrap();
        assert_eq!(repo.host.as_deref(), Some("bitbucket.org"));
        assert_eq!(repo.owner, "team");
        assert_eq!(repo.repo, "pkg");
    }

    #[test]
    fn skips_empty_ref_entries() {
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::set(&[
            (
                "PEZ_TARGET_DIR",
                env.fish_config_dir.clone().into_os_string(),
            ),
            ("PEZ_CONFIG_DIR", env.config_dir.clone().into_os_string()),
        ]);

        env.setup_config(config::Config { plugins: None });

        let fish_plugins_path = env.fish_config_dir.join("fish_plugins");
        fs::write(&fish_plugins_path, "owner/repo@\n").unwrap();

        let args = MigrateArgs {
            dry_run: false,
            force: false,
            install: false,
        };
        run_migrate(&args).unwrap();

        let cfg = config::load(&env.config_path).unwrap();
        assert!(cfg.plugins.is_none());
    }

    #[test]
    fn skips_git_at_url_with_ref_suffix() {
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::set(&[
            (
                "PEZ_TARGET_DIR",
                env.fish_config_dir.clone().into_os_string(),
            ),
            ("PEZ_CONFIG_DIR", env.config_dir.clone().into_os_string()),
        ]);

        env.setup_config(config::Config { plugins: None });

        let fish_plugins_path = env.fish_config_dir.join("fish_plugins");
        fs::write(
            &fish_plugins_path,
            "git@bitbucket.org:team/pkg.git@branch:main\n",
        )
        .unwrap();

        let args = MigrateArgs {
            dry_run: false,
            force: false,
            install: false,
        };
        run_migrate(&args).unwrap();

        let cfg = config::load(&env.config_path).unwrap();
        assert!(cfg.plugins.is_none());
    }

    #[test]
    fn preserves_custom_url_when_pinned_ref_matches() {
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::set(&[
            (
                "PEZ_TARGET_DIR",
                env.fish_config_dir.clone().into_os_string(),
            ),
            ("PEZ_CONFIG_DIR", env.config_dir.clone().into_os_string()),
        ]);

        let existing_spec = PluginSpec {
            name: None,
            source: PluginSource::Url {
                url: "git@bitbucket.org:team/pkg.git".to_string(),
                version: Some("2.0.0".to_string()),
                branch: None,
                tag: None,
                commit: None,
            },
        };
        env.setup_config(config::Config {
            plugins: Some(vec![existing_spec.clone()]),
        });

        let fish_plugins_path = env.fish_config_dir.join("fish_plugins");
        fs::write(&fish_plugins_path, "bitbucket.org/team/pkg@2.0.0\n").unwrap();

        let args = MigrateArgs {
            dry_run: false,
            force: false,
            install: false,
        };
        run_migrate(&args).unwrap();

        let cfg = config::load(&env.config_path).unwrap();
        let plugins = cfg.plugins.expect("plugins written");
        assert_eq!(plugins.len(), 1);
        match &plugins[0].source {
            PluginSource::Url { url, version, .. } => {
                assert_eq!(url, "git@bitbucket.org:team/pkg.git");
                assert_eq!(version.as_deref(), Some("2.0.0"));
            }
            other => panic!("expected Url source, got {other:?}"),
        }
    }

    #[test]
    fn preserves_custom_url_when_incoming_is_unpinned_repo() {
        let mut env = TestEnvironmentSetup::new();
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let _guard = EnvGuard::set(&[
            (
                "PEZ_TARGET_DIR",
                env.fish_config_dir.clone().into_os_string(),
            ),
            ("PEZ_CONFIG_DIR", env.config_dir.clone().into_os_string()),
        ]);

        let existing_spec = PluginSpec {
            name: None,
            source: PluginSource::Url {
                url: "git@bitbucket.org:team/pkg.git".to_string(),
                version: None,
                branch: None,
                tag: None,
                commit: None,
            },
        };
        env.setup_config(config::Config {
            plugins: Some(vec![existing_spec.clone()]),
        });

        let fish_plugins_path = env.fish_config_dir.join("fish_plugins");
        fs::write(&fish_plugins_path, "bitbucket.org/team/pkg\n").unwrap();

        let args = MigrateArgs {
            dry_run: false,
            force: false,
            install: false,
        };
        run_migrate(&args).unwrap();

        let cfg = config::load(&env.config_path).unwrap();
        let plugins = cfg.plugins.expect("plugins written");
        assert_eq!(plugins.len(), 1);
        match &plugins[0].source {
            PluginSource::Url { url, .. } => {
                assert_eq!(url, "git@bitbucket.org:team/pkg.git");
            }
            other => panic!("expected Url source, got {other:?}"),
        }
    }
}
