use crate::cli::{FilesArgs, FilesDir, FilesFormat};
use crate::models::{InstallTarget, PluginRepo, TargetDir};
use crate::utils;
use anyhow::{Context, anyhow};
use std::path::PathBuf;

pub(crate) fn run(args: &FilesArgs) -> anyhow::Result<()> {
    let paths = collect_paths(args)?;
    match args.format {
        FilesFormat::Paths => {
            for p in paths {
                println!("{}", p.display());
            }
        }
        FilesFormat::Json => {
            let rendered: Vec<String> = paths
                .into_iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            println!("{}", serde_json::to_string_pretty(&rendered)?);
        }
    }
    Ok(())
}

fn collect_paths(args: &FilesArgs) -> anyhow::Result<Vec<PathBuf>> {
    let (lock_file, _) = utils::load_lock_file()?;
    if lock_file.plugins.is_empty() {
        anyhow::bail!("No plugins are installed.");
    }

    let config_dir = utils::load_fish_config_dir()?;
    let dir_filter = match args.dir {
        FilesDir::All => None,
        FilesDir::ConfD => Some(TargetDir::ConfD),
    };

    let repos: Vec<PluginRepo> = if args.all {
        lock_file.plugins.iter().map(|p| p.repo.clone()).collect()
    } else {
        let list = args
            .plugins
            .as_ref()
            .ok_or_else(|| anyhow!("No plugins specified; pass --all or plugin names"))?;
        list.iter()
            .map(|s| parse_plugin_arg(s))
            .collect::<Result<_, _>>()?
    };

    let mut paths = lock_file.paths_for_repos(&repos, &config_dir, dir_filter.as_ref())?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn parse_plugin_arg(raw: &str) -> anyhow::Result<PluginRepo> {
    // Try full InstallTarget parsing to allow URLs and @ref, but ignore ref in the lookup.
    let target = InstallTarget::from_raw(raw.to_string());
    match target.resolve() {
        Ok(resolved) => Ok(resolved.plugin_repo),
        Err(_) => raw
            .parse::<PluginRepo>()
            .map_err(|e| anyhow!(e))
            .context("Failed to parse plugin identifier"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{FilesArgs, FilesDir, FilesFormat};
    use crate::lock_file::{LockFile, Plugin, PluginFile};
    use crate::models::{PluginRepo, TargetDir};
    use crate::tests_support::env::TestEnvironmentSetup;

    fn with_env<F: FnOnce() -> anyhow::Result<()>>(env: &TestEnvironmentSetup, f: F) {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let prev_fc = std::env::var_os("__fish_config_dir");
        let prev_pc = std::env::var_os("PEZ_CONFIG_DIR");
        unsafe {
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
        }
        let res = f();
        // restore
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
        }
        res.expect("test case failed");
    }

    #[test]
    fn lists_conf_d_paths_sorted_and_deduped() {
        let mut env = TestEnvironmentSetup::new();
        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "pkg".into(),
        };
        let lock = LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: "pkg".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc".into(),
                files: vec![
                    PluginFile {
                        dir: TargetDir::ConfD,
                        name: "b.fish".into(),
                    },
                    PluginFile {
                        dir: TargetDir::ConfD,
                        name: "a.fish".into(),
                    },
                    PluginFile {
                        dir: TargetDir::Functions,
                        name: "noop.fish".into(),
                    },
                ],
            }],
        };
        env.setup_lock_file(lock);
        // create dummy paths to mirror real layout
        let confd = env.fish_config_dir.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&confd).unwrap();
        std::fs::write(confd.join("a.fish"), "").unwrap();
        std::fs::write(confd.join("b.fish"), "").unwrap();

        let args = FilesArgs {
            plugins: Some(vec!["owner/pkg@v1".into()]),
            all: false,
            dir: FilesDir::ConfD,
            format: FilesFormat::Paths,
        };

        with_env(&env, || {
            let paths = collect_paths(&args)?;
            assert_eq!(
                paths,
                vec![
                    env.fish_config_dir.join("conf.d/a.fish"),
                    env.fish_config_dir.join("conf.d/b.fish")
                ]
            );
            Ok(())
        });
    }

    #[test]
    fn errors_without_plugins_and_not_all() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![],
        });
        let args = FilesArgs {
            plugins: None,
            all: false,
            dir: FilesDir::All,
            format: FilesFormat::Paths,
        };
        with_env(&env, || {
            let err = collect_paths(&args).expect_err("should fail");
            assert!(err.to_string().contains("No plugins")); // from lock empty
            Ok(())
        });
    }
}
