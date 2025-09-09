use crate::{cli, lock_file::LockFile, utils};
use serde_derive::Serialize;
use serde_json::json;
use std::{collections::HashSet, path};
use tracing::{info, warn};

#[derive(Serialize)]
struct DoctorCheck<'a> {
    name: &'a str,
    status: &'a str, // ok | warn | error
    details: String,
}

pub(crate) fn run(args: &cli::DoctorArgs) -> anyhow::Result<()> {
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

    match args.format {
        Some(cli::DoctorFormat::Json) => {
            println!("{}", serde_json::to_string_pretty(&json!(checks))?);
        }
        None => {
            info!("pez doctor checks:");
            for c in &checks {
                let prefix = match c.status {
                    "ok" => "✔",
                    "warn" => "⚠",
                    _ => "✖",
                };
                println!("{} {:<12} - {}", prefix, c.name, c.details);
            }
            if checks.iter().any(|c| c.status == "error") {
                warn!("Errors detected. Please resolve the above items.");
            }
        }
    }

    Ok(())
}
