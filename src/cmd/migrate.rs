use crate::{
    cli::{InstallArgs, MigrateArgs},
    config, utils,
};
use console::Emoji;
use std::{
    fs,
    io::{BufRead, BufReader},
};
use tracing::{error, info, warn};

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
    let mut repos: Vec<crate::models::PluginRepo> = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        match trimmed.parse::<crate::models::PluginRepo>() {
            Ok(repo) => {
                if repo.owner == "jorgebucaran" && repo.repo == "fisher" {
                    continue;
                }
                repos.push(repo)
            }
            Err(_) => warn!(
                "{}Skipping unrecognized entry: {}",
                Emoji("⚠ ", ""),
                trimmed
            ),
        }
    }

    if repos.is_empty() {
        warn!("{}No valid entries to migrate.", Emoji("⚠ ", ""));
        return Ok(());
    }

    let (mut cfg, cfg_path) = utils::load_or_create_config()?;
    let mut planned = Vec::new();
    match cfg.plugins.as_mut() {
        Some(list) => {
            if args.force {
                planned = repos
                    .iter()
                    .map(|r| config::PluginSpec {
                        name: None,
                        source: config::PluginSource::Repo {
                            repo: r.clone(),
                            version: None,
                            branch: None,
                            tag: None,
                            commit: None,
                        },
                    })
                    .collect();
                if !args.dry_run {
                    cfg.plugins = Some(planned.clone());
                }
            } else {
                for r in repos {
                    if !list
                        .iter()
                        .any(|p| p.get_plugin_repo().is_ok_and(|pr| pr == r))
                    {
                        planned.push(config::PluginSpec {
                            name: None,
                            source: config::PluginSource::Repo {
                                repo: r.clone(),
                                version: None,
                                branch: None,
                                tag: None,
                                commit: None,
                            },
                        });
                        if !args.dry_run {
                            list.push(config::PluginSpec {
                                name: None,
                                source: config::PluginSource::Repo {
                                    repo: r,
                                    version: None,
                                    branch: None,
                                    tag: None,
                                    commit: None,
                                },
                            });
                        }
                    }
                }
            }
        }
        None => {
            planned = repos
                .iter()
                .map(|r| config::PluginSpec {
                    name: None,
                    source: config::PluginSource::Repo {
                        repo: r.clone(),
                        version: None,
                        branch: None,
                        tag: None,
                        commit: None,
                    },
                })
                .collect();
            if !args.dry_run {
                cfg.plugins = Some(planned.clone());
            }
        }
    }

    if args.dry_run {
        info!("{}Dry run: planned updates to pez.toml", Emoji("🧪 ", ""));
    } else {
        cfg.save(&cfg_path)?;
        info!("{}Updated {}", Emoji("✅ ", ""), cfg_path.display());
    }
    for p in &planned {
        println!(
            "  - {}",
            p.get_plugin_repo().map(|r| r.as_str()).unwrap_or_default()
        );
    }
    if planned.is_empty() {
        info!("{}Nothing to update.", Emoji("ℹ ", ""));
    }

    if !args.dry_run && args.install && !planned.is_empty() {
        let targets: Vec<_> = planned
            .iter()
            .filter_map(|p| p.get_plugin_repo().ok())
            .map(|r| crate::models::InstallTarget::from_raw(r.as_str()))
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
