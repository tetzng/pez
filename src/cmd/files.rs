use crate::cli::{Cli, Commands, FilesArgs, FilesDir, FilesFormat, FilesFrom};
use crate::cmd::uninstall;
use crate::lock_file::LockFile;
use crate::models::{InstallTarget, PluginRepo, TargetDir};
use crate::utils;
use anyhow::{Context, anyhow};
use clap::Parser;
use clap::error::ErrorKind;
use std::io::Read;
use std::path::PathBuf;

pub(crate) fn run(args: &FilesArgs) -> anyhow::Result<Vec<PathBuf>> {
    let paths = collect_paths(args)?;
    match args.format {
        FilesFormat::Paths => {
            for line in render_paths(&paths) {
                println!("{line}");
            }
        }
        FilesFormat::Json => {
            println!("{}", render_paths_json(&paths)?);
        }
    }
    Ok(paths)
}

fn render_paths(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.display().to_string()).collect()
}

fn render_paths_json(paths: &[PathBuf]) -> anyhow::Result<String> {
    let rendered: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect();
    Ok(serde_json::to_string_pretty(&rendered)?)
}

fn collect_paths(args: &FilesArgs) -> anyhow::Result<Vec<PathBuf>> {
    if let Some(from) = &args.from
        && should_skip_from_parse(from, &args.passthrough)?
    {
        return Ok(vec![]);
    }

    let (lock_file, _) = utils::load_lock_file()?;

    let config_dir = utils::load_fish_config_dir()?;
    let dir_filter = match args.dir {
        FilesDir::All => None,
        FilesDir::ConfD => Some(TargetDir::ConfD),
    };

    let repos_opt: Option<Vec<PluginRepo>> = if let Some(from) = &args.from {
        repos_from_from_arg(from, &args.passthrough, &lock_file)?
    } else if args.all {
        Some(lock_file.plugins.iter().map(|p| p.repo.clone()).collect())
    } else {
        let list = args
            .plugins
            .as_ref()
            .ok_or_else(|| anyhow!("No plugins specified; pass --all or plugin names"))?;
        Some(
            list.iter()
                .map(|s| parse_plugin_arg(s))
                .collect::<Result<_, _>>()?,
        )
    };

    let repos = match repos_opt {
        Some(repos) => repos,
        None => return Ok(vec![]),
    };
    if repos.is_empty() {
        anyhow::bail!("No plugins are installed.");
    }

    let mut paths = lock_file.paths_for_repos(&repos, &config_dir, dir_filter.as_ref())?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn repos_from_from_arg(
    from: &FilesFrom,
    passthrough: &[String],
    lock_file: &LockFile,
) -> anyhow::Result<Option<Vec<PluginRepo>>> {
    repos_from_from_arg_with_reader(from, passthrough, lock_file, None)
}

fn repos_from_from_arg_with_reader(
    from: &FilesFrom,
    passthrough: &[String],
    lock_file: &LockFile,
    stdin_reader: Option<&mut dyn Read>,
) -> anyhow::Result<Option<Vec<PluginRepo>>> {
    let argv = build_from_argv(from, passthrough);
    let parsed = match Cli::try_parse_from(argv) {
        Ok(parsed) => parsed,
        Err(err) => {
            if is_display_help_or_version(&err) {
                return Ok(None);
            }
            return Err(anyhow!(err.to_string()));
        }
    };
    match parsed.command {
        Commands::Install(install_args) => install_args
            .plugins
            .as_ref()
            .map(|plugins| {
                plugins
                    .iter()
                    .map(|t| t.resolve().map(|r| r.plugin_repo))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap_or_else(|| Ok(lock_file.plugins.iter().map(|p| p.repo.clone()).collect()))
            .map(Some),
        Commands::Upgrade(upgrade_args) => {
            if let Some(list) = &upgrade_args.plugins {
                Ok(Some(list.clone()))
            } else {
                Ok(Some(
                    lock_file.plugins.iter().map(|p| p.repo.clone()).collect(),
                ))
            }
        }
        Commands::Uninstall(uninstall_args) => {
            if let Some(list) = uninstall_args.plugins.as_ref() {
                return Ok(Some(list.clone()));
            }
            if uninstall_args.stdin {
                let repos = if let Some(reader) = stdin_reader {
                    uninstall::read_plugins_from_reader(reader)?
                } else {
                    let stdin = std::io::stdin();
                    uninstall::read_plugins_from_reader(stdin.lock())?
                };
                return Ok(Some(repos));
            }
            anyhow::bail!("No plugins specified for uninstall/remove")
        }
        other => anyhow::bail!("Unsupported --from target: {:?}", other),
    }
}

fn build_from_argv(from: &FilesFrom, passthrough: &[String]) -> Vec<String> {
    let subcmd = match from {
        FilesFrom::Install => "install",
        FilesFrom::Update => "upgrade",
        FilesFrom::Upgrade => "upgrade",
        FilesFrom::Uninstall => "uninstall",
        FilesFrom::Remove => "uninstall",
    };

    let mut argv = Vec::with_capacity(passthrough.len() + 2);
    argv.push("pez".to_string());
    argv.push(subcmd.to_string());
    argv.extend_from_slice(passthrough);
    argv
}

fn should_skip_from_parse(from: &FilesFrom, passthrough: &[String]) -> anyhow::Result<bool> {
    let argv = build_from_argv(from, passthrough);
    match Cli::try_parse_from(argv) {
        Ok(_) => Ok(false),
        Err(err) => {
            if is_display_help_or_version(&err) {
                Ok(true)
            } else {
                Err(anyhow!(err.to_string()))
            }
        }
    }
}

fn is_display_help_or_version(err: &clap::Error) -> bool {
    matches!(
        err.kind(),
        ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
    )
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
    use crate::cli::{FilesArgs, FilesDir, FilesFormat, FilesFrom};
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
            from: None,
            passthrough: vec![],
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
            from: None,
            passthrough: vec![],
        };
        with_env(&env, || {
            let err = collect_paths(&args).expect_err("should fail");
            assert!(err.to_string().contains("No plugins")); // from lock empty
            Ok(())
        });
    }

    #[test]
    fn from_install_parses_targets_with_options() {
        let mut env = TestEnvironmentSetup::new();
        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "pkg".into(),
        };
        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: "pkg".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc".into(),
                files: vec![PluginFile {
                    dir: TargetDir::ConfD,
                    name: "a.fish".into(),
                }],
            }],
        });
        let confd = env.fish_config_dir.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&confd).unwrap();
        std::fs::write(confd.join("a.fish"), "").unwrap();

        let args = FilesArgs {
            plugins: None,
            all: false,
            dir: FilesDir::ConfD,
            format: FilesFormat::Paths,
            from: Some(FilesFrom::Install),
            passthrough: vec!["--force".into(), "owner/pkg@v1".into()],
        };

        with_env(&env, || {
            let paths = collect_paths(&args)?;
            assert_eq!(paths.len(), 1);
            assert!(paths[0].ends_with("conf.d/a.fish"));
            Ok(())
        });
    }

    #[test]
    fn from_install_without_plugins_uses_lock() {
        let mut env = TestEnvironmentSetup::new();
        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "pkg".into(),
        };
        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: "pkg".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc".into(),
                files: vec![PluginFile {
                    dir: TargetDir::ConfD,
                    name: "a.fish".into(),
                }],
            }],
        });
        let confd = env.fish_config_dir.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&confd).unwrap();
        std::fs::write(confd.join("a.fish"), "").unwrap();

        let args = FilesArgs {
            plugins: None,
            all: false,
            dir: FilesDir::ConfD,
            format: FilesFormat::Paths,
            from: Some(FilesFrom::Install),
            passthrough: vec![],
        };

        with_env(&env, || {
            let paths = collect_paths(&args)?;
            assert_eq!(paths.len(), 1);
            assert!(paths[0].ends_with("conf.d/a.fish"));
            Ok(())
        });
    }

    #[test]
    fn from_uninstall_with_stdin_reads_reader() {
        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "pkg".into(),
        };
        let other = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "other".into(),
        };
        let lock_file = LockFile {
            version: 1,
            plugins: vec![
                Plugin {
                    name: "pkg".into(),
                    repo: repo.clone(),
                    source: repo.default_remote_source(),
                    commit_sha: "abc".into(),
                    files: vec![PluginFile {
                        dir: TargetDir::ConfD,
                        name: "a.fish".into(),
                    }],
                },
                Plugin {
                    name: "other".into(),
                    repo: other.clone(),
                    source: other.default_remote_source(),
                    commit_sha: "def".into(),
                    files: vec![],
                },
            ],
        };

        let mut input = std::io::Cursor::new("owner/pkg\n");
        let repos = repos_from_from_arg_with_reader(
            &FilesFrom::Uninstall,
            &["--stdin".into()],
            &lock_file,
            Some(&mut input),
        )
        .expect("stdin repos should parse");
        let repos = repos.expect("repos should be available");
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].as_str(), "owner/pkg");
    }

    #[test]
    fn from_help_returns_none() {
        let lock_file = LockFile {
            version: 1,
            plugins: vec![],
        };
        let repos = repos_from_from_arg(&FilesFrom::Install, &["--help".into()], &lock_file)
            .expect("help should not error");
        assert!(repos.is_none());
    }

    #[test]
    fn should_skip_from_parse_returns_true_for_help() {
        let res = should_skip_from_parse(&FilesFrom::Install, &["--help".into()]).unwrap();
        assert!(res);
    }

    #[test]
    fn should_skip_from_parse_errors_on_invalid_args() {
        let err = should_skip_from_parse(&FilesFrom::Install, &["--nope".into()]);
        assert!(err.is_err());
    }

    #[test]
    fn run_outputs_json_paths() {
        let mut env = TestEnvironmentSetup::new();
        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "pkg".into(),
        };
        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: "pkg".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc".into(),
                files: vec![PluginFile {
                    dir: TargetDir::ConfD,
                    name: "a.fish".into(),
                }],
            }],
        });
        let confd = env.fish_config_dir.join(TargetDir::ConfD.as_str());
        std::fs::create_dir_all(&confd).unwrap();
        std::fs::write(confd.join("a.fish"), "").unwrap();

        let args = FilesArgs {
            plugins: Some(vec!["owner/pkg".into()]),
            all: false,
            dir: FilesDir::ConfD,
            format: FilesFormat::Json,
            from: None,
            passthrough: vec![],
        };

        with_env(&env, || {
            let paths = run(&args).unwrap();
            let json = render_paths_json(&paths).unwrap();
            let paths: Vec<String> = serde_json::from_str(&json).unwrap();
            assert_eq!(paths.len(), 1);
            assert!(paths[0].ends_with("conf.d/a.fish"));
            Ok(())
        });
    }

    #[test]
    fn render_paths_returns_display_strings() {
        let paths = vec![PathBuf::from("alpha/beta"), PathBuf::from("gamma")];
        let expected: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
        assert_eq!(render_paths(&paths), expected);
    }
}
