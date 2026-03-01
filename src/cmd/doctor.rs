use crate::{cli, lock_file::LockFile, models::TargetDir, utils};
use serde_derive::Serialize;
use serde_json::json;
use std::{collections::HashSet, fs, path};
use tracing::{info, warn};

#[derive(Serialize)]
pub(crate) struct DoctorCheck {
    name: &'static str,
    status: &'static str, // ok | warn | error
    details: String,
}

pub(crate) fn run(args: &cli::DoctorArgs) -> anyhow::Result<Vec<DoctorCheck>> {
    let checks = collect_checks()?;

    match args.format {
        Some(cli::DoctorFormat::Json) => {
            println!("{}", serde_json::to_string_pretty(&json!(checks))?);
        }
        None => {
            info!("pez doctor checks:");
            for line in render_plain_lines(&checks) {
                println!("{line}");
            }
            if has_error(&checks) {
                warn!("Errors detected. Please resolve the above items.");
            }
        }
    }

    Ok(checks)
}

fn collect_checks() -> anyhow::Result<Vec<DoctorCheck>> {
    let mut checks: Vec<DoctorCheck> = Vec::new();

    match utils::load_config() {
        Ok((_cfg, path)) => checks.push(DoctorCheck {
            name: "config",
            status: "ok",
            details: format!("found: {}", path.display()),
        }),
        Err(_) => checks.push(DoctorCheck {
            name: "config",
            status: "warn",
            details: "pez.toml not found".to_string(),
        }),
    }

    let mut lock: Option<LockFile> = None;
    match utils::load_lock_file() {
        Ok((l, path)) => {
            lock = Some(l);
            checks.push(DoctorCheck {
                name: "lock_file",
                status: "ok",
                details: format!("found: {}", path.display()),
            })
        }
        Err(_) => checks.push(DoctorCheck {
            name: "lock_file",
            status: "warn",
            details: "pez-lock.toml not found".to_string(),
        }),
    }

    let fish_config_dir = utils::load_fish_config_dir()?;
    checks.push(DoctorCheck {
        name: "fish_config_dir",
        status: if fish_config_dir.exists() {
            "ok"
        } else {
            "warn"
        },
        details: fish_config_dir.display().to_string(),
    });

    let pez_data_dir = utils::load_pez_data_dir()?;
    checks.push(DoctorCheck {
        name: "pez_data_dir",
        status: if pez_data_dir.exists() { "ok" } else { "warn" },
        details: pez_data_dir.display().to_string(),
    });

    // Activation is configured in the user's fish config directory, not the install target.
    let fish_runtime_config_dir = utils::load_default_fish_config_dir()?;
    let activate_check = check_activate_configured(&fish_runtime_config_dir);
    let activation_enabled = activate_check.status == "ok";
    checks.push(activate_check);
    checks.push(check_event_hook_readiness(activation_enabled));
    checks.push(check_install_layout(&fish_config_dir));

    if let Some(lock_file) = lock {
        let mut missing_repos = vec![];
        for p in &lock_file.plugins {
            let repo_path = pez_data_dir.join(p.repo.as_str());
            if !repo_path.exists() {
                missing_repos.push(p.repo.as_str());
            }
        }
        checks.push(DoctorCheck {
            name: "repos",
            status: if missing_repos.is_empty() {
                "ok"
            } else {
                "warn"
            },
            details: if missing_repos.is_empty() {
                "all cloned".to_string()
            } else {
                format!("missing: {}", missing_repos.join(", "))
            },
        });

        let mut missing_files = vec![];
        let mut dest_set: HashSet<path::PathBuf> = HashSet::new();
        let mut duplicates = vec![];
        for p in &lock_file.plugins {
            for f in &p.files {
                let dest = fish_config_dir.join(f.dir.as_str()).join(&f.name);
                if !dest.exists() {
                    missing_files.push(dest.display().to_string());
                }
                if !dest_set.insert(dest.clone()) {
                    duplicates.push(dest.display().to_string());
                }
            }
        }
        checks.push(DoctorCheck {
            name: "target_files",
            status: if missing_files.is_empty() {
                "ok"
            } else {
                "warn"
            },
            details: if missing_files.is_empty() {
                "all present".to_string()
            } else {
                format!("missing: {}", missing_files.join(", "))
            },
        });
        checks.push(DoctorCheck {
            name: "duplicates",
            status: if duplicates.is_empty() { "ok" } else { "error" },
            details: if duplicates.is_empty() {
                "no conflicts".to_string()
            } else {
                format!("conflicting destinations: {}", duplicates.join(", "))
            },
        });
        checks.push(check_theme_assets(&lock_file, &fish_config_dir));
    }

    Ok(checks)
}

fn check_activate_configured(fish_config_dir: &path::Path) -> DoctorCheck {
    let config_fish_path = fish_config_dir.join("config.fish");
    if !config_fish_path.exists() {
        return DoctorCheck {
            name: "activate_configured",
            status: "warn",
            details: format!(
                "missing: {} (add `pez activate fish | source` for shell hooks)",
                config_fish_path.display()
            ),
        };
    }

    match fs::read_to_string(&config_fish_path) {
        Ok(contents) => {
            if has_activate_fish_line(&contents) {
                DoctorCheck {
                    name: "activate_configured",
                    status: "ok",
                    details: format!("found in {}", config_fish_path.display()),
                }
            } else {
                DoctorCheck {
                    name: "activate_configured",
                    status: "warn",
                    details: format!(
                        "not found in {} (add `pez activate fish | source`)",
                        config_fish_path.display()
                    ),
                }
            }
        }
        Err(err) => DoctorCheck {
            name: "activate_configured",
            status: "warn",
            details: format!("failed to read {}: {err}", config_fish_path.display()),
        },
    }
}

fn has_activate_fish_line(contents: &str) -> bool {
    contents.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.starts_with('#') && trimmed.contains("pez activate fish")
    })
}

fn check_event_hook_readiness(activation_enabled: bool) -> DoctorCheck {
    if activation_enabled {
        return DoctorCheck {
            name: "event_hook_readiness",
            status: "ok",
            details: "activate wrapper detected; conf.d events should run in the current shell"
                .to_string(),
        };
    }

    DoctorCheck {
        name: "event_hook_readiness",
        status: "warn",
        details: "activate wrapper not detected; run `pez activate fish | source`".to_string(),
    }
}

fn check_install_layout(fish_config_dir: &path::Path) -> DoctorCheck {
    let mut invalid_paths = Vec::new();
    let mut missing_dirs = Vec::new();

    for dir in ["functions", "completions", "conf.d", "themes"] {
        let path = fish_config_dir.join(dir);
        if path.exists() {
            if !path.is_dir() {
                invalid_paths.push(path.display().to_string());
            }
        } else {
            missing_dirs.push(dir);
        }
    }

    if !invalid_paths.is_empty() {
        return DoctorCheck {
            name: "install_layout",
            status: "warn",
            details: format!(
                "expected directories but found non-directories: {}",
                invalid_paths.join(", ")
            ),
        };
    }

    if missing_dirs.is_empty() {
        DoctorCheck {
            name: "install_layout",
            status: "ok",
            details: "target directories are present".to_string(),
        }
    } else {
        DoctorCheck {
            name: "install_layout",
            status: "ok",
            details: format!(
                "ready (missing dirs will be created on install: {})",
                missing_dirs.join(", ")
            ),
        }
    }
}

fn check_theme_assets(lock_file: &LockFile, fish_config_dir: &path::Path) -> DoctorCheck {
    let mut missing = Vec::new();
    let mut tracked_theme_count = 0usize;

    for plugin in &lock_file.plugins {
        for file in &plugin.files {
            if file.dir != TargetDir::Themes {
                continue;
            }
            tracked_theme_count += 1;
            let dest = fish_config_dir.join(file.dir.as_str()).join(&file.name);
            if !dest.exists() {
                missing.push(dest.display().to_string());
            }
        }
    }

    if tracked_theme_count == 0 {
        return DoctorCheck {
            name: "theme_assets",
            status: "ok",
            details: "no theme assets recorded in lock file".to_string(),
        };
    }

    if missing.is_empty() {
        DoctorCheck {
            name: "theme_assets",
            status: "ok",
            details: "all theme assets are present".to_string(),
        }
    } else {
        DoctorCheck {
            name: "theme_assets",
            status: "warn",
            details: format!("missing: {}", missing.join(", ")),
        }
    }
}

fn status_prefix(status: &str) -> &'static str {
    match status {
        "ok" => "✔",
        "warn" => "⚠",
        _ => "✖",
    }
}

fn render_plain_lines(checks: &[DoctorCheck]) -> Vec<String> {
    checks
        .iter()
        .map(|c| format!("{} {:<12} - {}", status_prefix(c.status), c.name, c.details))
        .collect()
}

fn has_error(checks: &[DoctorCheck]) -> bool {
    checks.iter().any(|c| c.status == "error")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::lock_file::{LockFile, Plugin, PluginFile};
    use crate::models::{PluginRepo, TargetDir};
    use crate::tests_support::env::TestEnvironmentSetup;
    use crate::tests_support::log::capture_logs;
    use std::collections::HashMap;
    use std::path::Path;

    fn with_env<F: FnOnce() -> R, R>(env: &TestEnvironmentSetup, f: F) -> R {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let prev_fc = std::env::var_os("__fish_config_dir");
        let prev_pc = std::env::var_os("PEZ_CONFIG_DIR");
        let prev_pd = std::env::var_os("PEZ_DATA_DIR");
        let prev_pt = std::env::var_os("PEZ_TARGET_DIR");
        unsafe {
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
        }
        let result = f();
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
        result
    }

    fn with_env_and_target_dir<F: FnOnce() -> R, R>(
        env: &TestEnvironmentSetup,
        target_dir: &Path,
        f: F,
    ) -> R {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let prev_fc = std::env::var_os("__fish_config_dir");
        let prev_pc = std::env::var_os("PEZ_CONFIG_DIR");
        let prev_pd = std::env::var_os("PEZ_DATA_DIR");
        let prev_pt = std::env::var_os("PEZ_TARGET_DIR");
        unsafe {
            std::env::set_var("__fish_config_dir", &env.fish_config_dir);
            std::env::set_var("PEZ_CONFIG_DIR", &env.config_dir);
            std::env::set_var("PEZ_DATA_DIR", &env.data_dir);
            std::env::set_var("PEZ_TARGET_DIR", target_dir);
        }
        let result = f();
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
        result
    }

    fn status_map(checks: Vec<DoctorCheck>) -> HashMap<&'static str, &'static str> {
        let mut statuses = HashMap::new();
        for check in checks {
            statuses.insert(check.name, check.status);
        }
        statuses
    }

    #[test]
    fn doctor_reports_missing_repos_and_files() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::init());
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
                    name: "pkg.fish".into(),
                }],
            }],
        });

        with_env(&env, || {
            let checks = collect_checks().unwrap();
            let statuses = status_map(checks);
            assert_eq!(statuses.get("config"), Some(&"ok"));
            assert_eq!(statuses.get("lock_file"), Some(&"ok"));
            assert_eq!(statuses.get("fish_config_dir"), Some(&"ok"));
            assert_eq!(statuses.get("pez_data_dir"), Some(&"ok"));
            assert_eq!(statuses.get("repos"), Some(&"warn"));
            assert_eq!(statuses.get("target_files"), Some(&"warn"));
            assert_eq!(statuses.get("duplicates"), Some(&"ok"));
        });
    }

    #[test]
    fn doctor_warns_when_activate_is_not_configured() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::init());

        with_env(&env, || {
            let statuses = status_map(collect_checks().unwrap());
            assert_eq!(statuses.get("activate_configured"), Some(&"warn"));
            assert_eq!(statuses.get("event_hook_readiness"), Some(&"warn"));
            assert_eq!(statuses.get("install_layout"), Some(&"ok"));
        });
    }

    #[test]
    fn doctor_reports_activate_configured_when_config_fish_contains_command() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::init());
        std::fs::write(
            env.fish_config_dir.join("config.fish"),
            "if status is-interactive\n    pez activate fish | source\nend\n",
        )
        .unwrap();

        with_env(&env, || {
            let statuses = status_map(collect_checks().unwrap());
            assert_eq!(statuses.get("activate_configured"), Some(&"ok"));
            assert_eq!(statuses.get("event_hook_readiness"), Some(&"ok"));
        });
    }

    #[test]
    fn doctor_uses_runtime_config_for_activate_when_target_dir_is_overridden() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::init());
        std::fs::write(
            env.fish_config_dir.join("config.fish"),
            "if status is-interactive\n    pez activate fish | source\nend\n",
        )
        .unwrap();
        let target_dir = env._temp_dir.path().join("target-only");
        std::fs::create_dir_all(&target_dir).unwrap();

        with_env_and_target_dir(&env, &target_dir, || {
            let statuses = status_map(collect_checks().unwrap());
            assert_eq!(statuses.get("activate_configured"), Some(&"ok"));
            assert_eq!(statuses.get("event_hook_readiness"), Some(&"ok"));
        });
    }

    #[test]
    fn doctor_warns_when_install_layout_contains_file_conflicts() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::init());
        std::fs::write(env.fish_config_dir.join("functions"), "not a directory").unwrap();

        with_env(&env, || {
            let statuses = status_map(collect_checks().unwrap());
            assert_eq!(statuses.get("install_layout"), Some(&"warn"));
        });
    }

    #[test]
    fn doctor_warns_when_tracked_theme_assets_are_missing() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::init());
        let repo = PluginRepo {
            host: None,
            owner: "owner".into(),
            repo: "theme".into(),
        };
        env.setup_lock_file(LockFile {
            version: 1,
            plugins: vec![Plugin {
                name: "theme".into(),
                repo: repo.clone(),
                source: repo.default_remote_source(),
                commit_sha: "abc".into(),
                files: vec![PluginFile {
                    dir: TargetDir::Themes,
                    name: "theme.theme".into(),
                }],
            }],
        });

        with_env(&env, || {
            let statuses = status_map(collect_checks().unwrap());
            assert_eq!(statuses.get("theme_assets"), Some(&"warn"));
        });
    }

    #[test]
    fn render_plain_lines_prefixes_statuses() {
        let checks = vec![
            DoctorCheck {
                name: "ok",
                status: "ok",
                details: "one".into(),
            },
            DoctorCheck {
                name: "warn",
                status: "warn",
                details: "two".into(),
            },
            DoctorCheck {
                name: "error",
                status: "error",
                details: "three".into(),
            },
        ];
        let lines = render_plain_lines(&checks);
        assert!(lines[0].starts_with("✔ "));
        assert!(lines[1].starts_with("⚠ "));
        assert!(lines[2].starts_with("✖ "));
    }

    #[test]
    fn has_error_detects_errors() {
        let ok_checks = vec![DoctorCheck {
            name: "config",
            status: "ok",
            details: "ok".into(),
        }];
        assert!(!has_error(&ok_checks));

        let err_checks = vec![DoctorCheck {
            name: "duplicates",
            status: "error",
            details: "oops".into(),
        }];
        assert!(has_error(&err_checks));
    }

    #[test]
    fn run_does_not_warn_without_errors() {
        let mut env = TestEnvironmentSetup::new();
        env.setup_config(config::init());
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
                    name: "pkg.fish".into(),
                }],
            }],
        });

        with_env(&env, || {
            let args = cli::DoctorArgs { format: None };
            let (logs, result) = capture_logs(|| run(&args));
            let checks = result.unwrap();
            assert!(!checks.is_empty());
            assert!(
                logs.iter().any(|msg| msg.contains("pez doctor checks:")),
                "missing header logs: {logs:?}"
            );
            assert!(
                !logs.iter().any(|msg| msg.contains("Errors detected")),
                "unexpected warning logs: {logs:?}"
            );
        });
    }
}
