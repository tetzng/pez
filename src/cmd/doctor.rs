use crate::{cli, lock_file::LockFile, utils};
use serde_derive::Serialize;
use serde_json::json;
use std::{collections::HashSet, path};
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
    }

    Ok(checks)
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

    fn with_env<F: FnOnce() -> R, R>(env: &TestEnvironmentSetup, f: F) -> R {
        let _lock = crate::tests_support::log::env_lock().lock().unwrap();
        let prev_fc = std::env::var_os("__fish_config_dir");
        let prev_pc = std::env::var_os("PEZ_CONFIG_DIR");
        let prev_pd = std::env::var_os("PEZ_DATA_DIR");
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
        }
        result
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
            let mut statuses = HashMap::new();
            for check in checks {
                statuses.insert(check.name, check.status);
            }
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
